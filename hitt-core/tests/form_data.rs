//! Multipart form-data bodies: sent by the HTTP client and exported to cURL.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;

use hitt_core::core::client::HttpClient;
use hitt_core::core::request::{HttpMethod, KeyValuePair, Request, RequestBody};
use hitt_core::core::variables::VariableResolver;
use hitt_core::exporters::curl::to_curl;

/// Accept one connection on a local listener, capture the raw request (headers
/// and body) and answer `200 OK`.
fn capture_one_request() -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/upload", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
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
        stream
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok")
            .unwrap();
        String::from_utf8_lossy(&buf).into_owned()
    });
    (url, handle)
}

fn resolver_with(vars: &[(&str, &str)]) -> VariableResolver {
    let mut resolver = VariableResolver::new();
    let map: HashMap<String, String> = vars
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    resolver.add_scope("test", map);
    resolver
}

fn disabled(key: &str, value: &str) -> KeyValuePair {
    let mut kv = KeyValuePair::new(key, value);
    kv.enabled = false;
    kv
}

#[tokio::test]
async fn form_data_sends_multipart_text_and_file_fields() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("note.txt");
    std::fs::write(&file, "file-contents-123").unwrap();

    let (url, server) = capture_one_request();
    let req = Request::new("upload", HttpMethod::POST, url).with_body(RequestBody::FormData(vec![
        KeyValuePair::new("name", "{{user}}"),
        KeyValuePair::new("{{field}}", "plain"),
        disabled("skipped", "nope"),
        KeyValuePair::new("doc", format!("@{}", file.display())),
    ]));
    let resolver = resolver_with(&[("user", "alice"), ("field", "kind")]);

    let resp = HttpClient::new()
        .unwrap()
        .send(&req, &resolver)
        .await
        .unwrap();
    assert_eq!(resp.status, 200);

    let raw = server.join().unwrap();
    let lower = raw.to_ascii_lowercase();
    let ct_line = lower
        .lines()
        .find(|l| l.starts_with("content-type:"))
        .expect("content-type header");
    assert!(
        ct_line.contains("multipart/form-data; boundary="),
        "{ct_line}"
    );
    let boundary = ct_line.split("boundary=").nth(1).unwrap().trim();
    assert!(lower.contains(&format!("--{boundary}")));
    assert!(lower.contains(&format!("--{boundary}--")));
    assert_eq!(
        lower.matches("content-type:").count(),
        2,
        "single CT header + file part"
    );

    assert!(raw.contains("Content-Disposition: form-data; name=\"name\"\r\n\r\nalice\r\n"));
    assert!(raw.contains("Content-Disposition: form-data; name=\"kind\"\r\n\r\nplain\r\n"));
    assert!(raw.contains("name=\"doc\"; filename=\"note.txt\""));
    assert!(raw.contains("file-contents-123"));
    assert!(!raw.contains("skipped"));
    assert!(!raw.contains("nope"));
}

#[tokio::test]
async fn form_data_missing_file_is_a_clear_error() {
    let req = Request::new("upload", HttpMethod::POST, "http://127.0.0.1:9/never").with_body(
        RequestBody::FormData(vec![KeyValuePair::new("doc", "@/definitely/not/here.bin")]),
    );
    let err = HttpClient::new()
        .unwrap()
        .send(&req, &VariableResolver::new())
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("doc"), "{err}");
    assert!(err.contains("/definitely/not/here.bin"), "{err}");
}

#[test]
fn curl_export_form_data_uses_dash_f() {
    let req = Request::new("upload", HttpMethod::POST, "https://api.example.com/upload").with_body(
        RequestBody::FormData(vec![
            KeyValuePair::new("name", "{{user}}"),
            KeyValuePair::new("doc", "@./it's.png"),
            disabled("skipped", "x"),
        ]),
    );
    let curl = to_curl(&req, &resolver_with(&[("user", "alice")]));
    assert!(curl.contains("-F 'name=alice'"), "{curl}");
    assert!(curl.contains("-F 'doc=@./it'\\''s.png'"), "{curl}");
    assert!(!curl.contains("skipped"));
    assert!(!curl.contains("-d "));
}
