//! Regression tests for `HttpClient::send` against local TCP servers.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

use hitt_core::core::client::HttpClient;
use hitt_core::core::request::{HttpMethod, Request, RequestBody};
use hitt_core::core::response::ResponseBody;
use hitt_core::core::variables::VariableResolver;

/// Read one raw HTTP request (headers + `content-length` body) from `stream`.
fn read_request(stream: &mut std::net::TcpStream) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = stream.read(&mut chunk).unwrap();
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        let text = String::from_utf8_lossy(&buf);
        if let Some(end) = text.find("\r\n\r\n") {
            let content_length = text[..end]
                .lines()
                .find_map(|l| {
                    let (k, v) = l.split_once(':')?;
                    k.eq_ignore_ascii_case("content-length")
                        .then(|| v.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            if buf.len() >= end + 4 + content_length {
                break;
            }
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// Serve `responses.len()` connections in order, answering the n-th with
/// `responses[n]` (which gets `{base}` replaced by the server's base URL).
/// Every raw request received is sent on the returned channel.
fn serve(responses: Vec<String>) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    let base_clone = base.clone();
    std::thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let raw = read_request(&mut stream);
            let _ = tx.send(raw);
            let response = response.replace("{base}", &base_clone);
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    (base, rx)
}

fn ok_response(content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Header lines (lowercased name, value) of a raw request.
fn header_values(raw: &str, name: &str) -> Vec<String> {
    raw.split("\r\n\r\n")
        .next()
        .unwrap()
        .lines()
        .skip(1)
        .filter_map(|l| l.split_once(':'))
        .filter(|(k, _)| k.trim().eq_ignore_ascii_case(name))
        .map(|(_, v)| v.trim().to_string())
        .collect()
}

#[tokio::test]
async fn html_body_is_text() {
    let (base, _rx) = serve(vec![ok_response("text/html", "<h1>hi</h1>")]);
    let client = HttpClient::new().unwrap();
    let resp = client
        .send(
            &Request::new("html", HttpMethod::GET, format!("{base}/")),
            &VariableResolver::new(),
        )
        .await
        .unwrap();
    assert!(matches!(resp.body, ResponseBody::Html(_)));
    assert_eq!(resp.body_text(), Some("<h1>hi</h1>"));
}

#[tokio::test]
async fn cookies_set_during_redirects_are_reported() {
    let (base, _rx) = serve(vec![
        "HTTP/1.1 302 Found\r\nlocation: {base}/cookies\r\nset-cookie: A=1; Path=/; HttpOnly\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".to_string(),
        "HTTP/1.1 200 OK\r\nset-cookie: B=2\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok".to_string(),
    ]);
    let client = HttpClient::new().unwrap();
    let resp = client
        .send(
            &Request::new("redir", HttpMethod::GET, format!("{base}/cookies/set?A=1")),
            &VariableResolver::new(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status, 200);
    let names: Vec<&str> = resp.cookies.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["A", "B"]);
    let a = &resp.cookies[0];
    assert_eq!(a.value, "1");
    assert_eq!(a.path.as_deref(), Some("/"));
    assert!(a.http_only);
}

#[tokio::test]
async fn redirect_cookie_is_persisted_for_next_request() {
    let (base, rx) = serve(vec![
        "HTTP/1.1 302 Found\r\nlocation: {base}/done\r\nset-cookie: S=abc; Path=/\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".to_string(),
        ok_response("text/plain", "ok"),
        ok_response("text/plain", "ok"),
    ]);
    let client = HttpClient::new().unwrap();
    let resolver = VariableResolver::new();
    client
        .send(
            &Request::new("a", HttpMethod::GET, format!("{base}/set")),
            &resolver,
        )
        .await
        .unwrap();
    let second = client
        .send(
            &Request::new("b", HttpMethod::GET, format!("{base}/other")),
            &resolver,
        )
        .await
        .unwrap();
    // The second request sets no cookie itself, so none are reported for it.
    assert!(second.cookies.is_empty());
    let raws: Vec<String> = rx.iter().take(3).collect();
    assert_eq!(header_values(&raws[1], "cookie"), vec!["S=abc"]);
    assert_eq!(header_values(&raws[2], "cookie"), vec!["S=abc"]);
}

#[tokio::test]
async fn duplicate_header_names_are_all_sent() {
    let (base, rx) = serve(vec![ok_response("text/plain", "ok")]);
    let client = HttpClient::new().unwrap();
    let request = Request::new("dup", HttpMethod::GET, format!("{base}/"))
        .with_header("X-Tag", "one")
        .with_header("X-Tag", "two");
    client
        .send(&request, &VariableResolver::new())
        .await
        .unwrap();
    let raw = rx.recv().unwrap();
    assert_eq!(header_values(&raw, "x-tag"), vec!["one", "two"]);
}

#[tokio::test]
async fn invalid_header_name_is_an_error() {
    let client = HttpClient::new().unwrap();
    // No server needed: the request fails before anything is sent.
    let request =
        Request::new("bad", HttpMethod::GET, "http://127.0.0.1:9/").with_header("Bad Header", "x");
    let err = client
        .send(&request, &VariableResolver::new())
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), "invalid header name 'Bad Header'");
}

#[tokio::test]
async fn explicit_content_type_wins_over_body_type() {
    let bodies = [
        RequestBody::Json("{}".into()),
        RequestBody::Raw {
            content: "x".into(),
            content_type: "text/plain".into(),
        },
        RequestBody::GraphQL {
            query: "{ a }".into(),
            variables: None,
        },
    ];
    for body in bodies {
        let (base, rx) = serve(vec![ok_response("text/plain", "ok")]);
        let client = HttpClient::new().unwrap();
        let request = Request::new("ct", HttpMethod::POST, format!("{base}/"))
            .with_header("Content-Type", "application/vnd.custom+json")
            .with_body(body);
        client
            .send(&request, &VariableResolver::new())
            .await
            .unwrap();
        let raw = rx.recv().unwrap();
        assert_eq!(
            header_values(&raw, "content-type"),
            vec!["application/vnd.custom+json"]
        );
    }
}

#[tokio::test]
async fn body_type_sets_content_type_without_explicit_header() {
    let (base, rx) = serve(vec![ok_response("text/plain", "ok")]);
    let client = HttpClient::new().unwrap();
    let request = Request::new("ct", HttpMethod::POST, format!("{base}/"))
        .with_header("Content-Type", "text/ignored")
        .with_body(RequestBody::Json("{}".into()));
    // A disabled Content-Type header does not count as explicit.
    let mut request = request;
    request.headers[0].enabled = false;
    client
        .send(&request, &VariableResolver::new())
        .await
        .unwrap();
    let raw = rx.recv().unwrap();
    assert_eq!(
        header_values(&raw, "content-type"),
        vec!["application/json"]
    );
}

#[tokio::test]
async fn graphql_invalid_variables_is_an_error() {
    let client = HttpClient::new().unwrap();
    let request = Request::new("gql", HttpMethod::POST, "http://127.0.0.1:9/").with_body(
        RequestBody::GraphQL {
            query: "{ a }".into(),
            variables: Some("{not json".into()),
        },
    );
    let err = client
        .send(&request, &VariableResolver::new())
        .await
        .unwrap_err();
    assert!(
        err.to_string()
            .starts_with("GraphQL variables are not valid JSON:"),
        "{err}"
    );
}

#[tokio::test]
async fn empty_error_body_is_empty_text() {
    let (base, _rx) = serve(vec![
        "HTTP/1.1 500 Internal Server Error\r\ncontent-type: text/html\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".to_string(),
    ]);
    let client = HttpClient::new().unwrap();
    let resp = client
        .send(
            &Request::new("e", HttpMethod::GET, format!("{base}/")),
            &VariableResolver::new(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status, 500);
    assert_eq!(resp.body_text(), Some(""));
}
