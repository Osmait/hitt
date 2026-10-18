//! Collection runner against a local HTTP server (no internet).

use hitt_core::core::auth::AuthConfig;
use hitt_core::core::client::HttpClient;
use hitt_core::core::collection::{Collection, CollectionItem};
use hitt_core::core::environment::Environment;
use hitt_core::core::request::{HttpMethod, KeyValuePair, Protocol, Request};
use hitt_core::testing::assertion_engine::Assertion;
use hitt_core::testing::collection_runner::{
    plan_requests, run_collection, to_json, to_junit_xml, to_text, RequestRunStatus, RunEvent,
    RunnerOptions, NOT_RUN_BAIL,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Serve forever on a local port. `/status/NNN...` answers with status NNN,
/// anything else with 200; the body is always the raw request head, so tests
/// can assert on the headers / path that were sent.
async fn spawn_server() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut head = Vec::new();
                let mut buf = [0u8; 2048];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => head.extend_from_slice(&buf[..n]),
                    }
                }
                let head = String::from_utf8_lossy(&head).into_owned();
                let path = head.split_whitespace().nth(1).unwrap_or("/").to_string();
                let status = path
                    .strip_prefix("/status/")
                    .and_then(|s| s.get(..3))
                    .and_then(|s| s.parse::<u16>().ok())
                    .unwrap_or(200);
                let resp = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{head}",
                    head.len()
                );
                let _ = stream.write_all(resp.as_bytes()).await;
            });
        }
    });
    base
}

fn get(name: &str, url: impl Into<String>) -> Request {
    Request::new(name, HttpMethod::GET, url)
}

fn with_assertions(mut req: Request, assertions: Vec<Assertion>) -> Request {
    req.assertions = assertions;
    req
}

fn folder(name: &str, items: Vec<CollectionItem>, auth: Option<AuthConfig>) -> CollectionItem {
    CollectionItem::Folder {
        id: uuid::Uuid::new_v4(),
        name: name.into(),
        items,
        auth,
        description: None,
    }
}

fn req_item(req: Request) -> CollectionItem {
    CollectionItem::Request(Box::new(req))
}

#[tokio::test]
async fn runs_requests_in_tree_order_with_assertions() {
    let base = spawn_server().await;
    let mut coll = Collection::new("API");
    coll.add_request(with_assertions(
        get("root ok", format!("{base}/a")),
        vec![
            Assertion::status_equals(200),
            Assertion::body_contains("GET /a"),
        ],
    ));
    coll.items.push(folder(
        "Users",
        vec![
            req_item(with_assertions(
                get("broken", format!("{base}/status/500")),
                vec![
                    Assertion::status_equals(200),
                    Assertion::body_contains("GET"),
                ],
            )),
            req_item(get("no assertions", format!("{base}/b"))),
        ],
        None,
    ));

    let client = HttpClient::new().unwrap();
    let mut events = Vec::new();
    let summary = run_collection(&client, &coll, None, &RunnerOptions::default(), |e| {
        events.push(e.clone());
    })
    .await
    .unwrap();

    let names: Vec<&str> = summary.results.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["root ok", "broken", "no assertions"]);
    assert_eq!(summary.total, 3);
    assert_eq!(summary.passed, 2);
    assert_eq!(summary.failed, 1);
    assert_eq!(summary.errors, 0);
    assert_eq!(summary.assertions_passed, 3);
    assert_eq!(summary.assertions_failed, 1);
    assert!(!summary.is_success());
    assert!(!summary.bailed);

    let broken = &summary.results[1];
    assert_eq!(broken.status, RequestRunStatus::Failed);
    assert_eq!(broken.http_status, Some(500));
    assert_eq!(broken.folder.as_deref(), Some("Users"));
    let resp = broken.response.as_ref().unwrap();
    assert_eq!(resp.assertion_results.len(), 2);

    // Started + (RequestStarted, RequestFinished) per request.
    assert!(matches!(events[0], RunEvent::Started { total: 3 }));
    assert_eq!(events.len(), 1 + 3 * 2);
    assert!(matches!(&events[6], RunEvent::RequestFinished(r) if r.index == 2));
}

#[tokio::test]
async fn bail_stops_after_first_failure() {
    let base = spawn_server().await;
    let mut coll = Collection::new("API");
    coll.add_request(get("first", format!("{base}/a")));
    coll.add_request(with_assertions(
        get("fails", format!("{base}/status/404")),
        vec![Assertion::status_equals(200)],
    ));
    coll.add_request(get("never", format!("{base}/c")));

    let client = HttpClient::new().unwrap();
    let options = RunnerOptions {
        stop_on_failure: true,
        ..RunnerOptions::default()
    };
    let mut finished = Vec::new();
    let summary = run_collection(&client, &coll, None, &options, |e| {
        if let RunEvent::RequestFinished(r) = e {
            finished.push((r.index, r.status));
        }
    })
    .await
    .unwrap();
    assert!(summary.bailed);
    assert_eq!(summary.results[1].status, RequestRunStatus::Failed);
    // The request that never ran is reported as skipped everywhere.
    assert_eq!(summary.total, 3);
    assert_eq!(summary.failed, 1);
    assert_eq!(summary.skipped, 1);
    let never = &summary.results[2];
    assert_eq!(never.name, "never");
    assert_eq!(never.status, RequestRunStatus::Skipped);
    assert_eq!(never.error.as_deref(), Some(NOT_RUN_BAIL));
    assert_eq!(finished.last(), Some(&(2, RequestRunStatus::Skipped)));

    let json: serde_json::Value = serde_json::from_str(&to_json(&summary).unwrap()).unwrap();
    assert_eq!(json["skipped"], 1);
    assert_eq!(json["results"][2]["status"], "skipped");
    let xml = to_junit_xml(&summary);
    assert!(xml.contains("tests=\"3\" failures=\"1\" errors=\"0\" skipped=\"1\""));
    assert!(xml.contains("<skipped message=\"not run (bail)\"/>"));
    let text = to_text(&summary);
    assert!(
        text.contains("3 total, 1 passed, 1 failed, 0 errors, 1 skipped"),
        "{text}"
    );
    assert!(text.contains("skipped: not run (bail)"));

    // Unrun requests of later iterations are skipped too.
    let options = RunnerOptions {
        stop_on_failure: true,
        iterations: 2,
        ..RunnerOptions::default()
    };
    let summary = run_collection(&client, &coll, None, &options, |_| {})
        .await
        .unwrap();
    assert_eq!(summary.total, 6);
    assert_eq!(summary.skipped, 4);
    assert_eq!(summary.results[5].iteration, 2);
    assert_eq!(summary.results[5].index, 5);

    // Without bail all three run.
    let summary = run_collection(&client, &coll, None, &RunnerOptions::default(), |_| {})
        .await
        .unwrap();
    assert_eq!(summary.total, 3);
    assert!(!summary.bailed);
}

#[tokio::test]
async fn realtime_protocols_are_skipped() {
    let base = spawn_server().await;
    let mut coll = Collection::new("API");
    let mut ws = get("socket", "ws://127.0.0.1:1/ws");
    ws.protocol = Protocol::WebSocket;
    let mut sse = get("stream", "http://127.0.0.1:1/sse");
    sse.protocol = Protocol::Sse;
    coll.add_request(ws);
    coll.add_request(sse);
    coll.add_request(get("http", format!("{base}/a")));

    let client = HttpClient::new().unwrap();
    let summary = run_collection(&client, &coll, None, &RunnerOptions::default(), |_| {})
        .await
        .unwrap();
    assert_eq!(summary.skipped, 2);
    assert_eq!(summary.passed, 1);
    assert!(summary.is_success());
    assert_eq!(summary.results[0].status, RequestRunStatus::Skipped);
    assert!(summary.results[0]
        .error
        .as_deref()
        .unwrap()
        .contains("WebSocket"));
}

#[tokio::test]
async fn environment_overrides_collection_variables() {
    let base = spawn_server().await;
    let mut coll = Collection::new("API");
    coll.variables
        .push(KeyValuePair::new("base", "http://127.0.0.1:1"));
    coll.variables.push(KeyValuePair::new("who", "collection"));
    coll.variables
        .push(KeyValuePair::new("only_coll", "c-value"));
    coll.add_request(with_assertions(
        get("vars", "{{base}}/vars")
            .with_header("x-who", "{{who}}")
            .with_header("x-coll", "{{only_coll}}"),
        vec![
            Assertion::body_contains("x-who: environment"),
            Assertion::body_contains("x-coll: c-value"),
        ],
    ));
    let mut env = Environment::new("dev");
    env.add_variable("base", base.clone());
    env.add_variable("who", "environment");

    let client = HttpClient::new().unwrap();
    let summary = run_collection(
        &client,
        &coll,
        Some(&env),
        &RunnerOptions::default(),
        |_| {},
    )
    .await
    .unwrap();
    assert!(summary.is_success(), "{}", to_text(&summary));
    assert_eq!(summary.environment.as_deref(), Some("dev"));
    assert_eq!(summary.results[0].url, format!("{base}/vars"));
}

#[tokio::test]
async fn folder_filter_and_auth_inheritance() {
    let base = spawn_server().await;
    let mut coll = Collection::new("API");
    coll.auth = Some(AuthConfig::bearer("coll-token"));
    coll.add_request(get("outside", format!("{base}/outside")));
    let mut inherit = get("inherit", format!("{base}/inherit"));
    inherit.auth = Some(AuthConfig::Inherit);
    let mut explicit_none = get("no auth", format!("{base}/none"));
    explicit_none.auth = Some(AuthConfig::None);
    coll.items.push(folder(
        "Admin",
        vec![folder(
            "Nested",
            vec![
                req_item(with_assertions(
                    inherit,
                    vec![Assertion::body_contains("Bearer folder-token")],
                )),
                req_item(with_assertions(
                    get("unset", format!("{base}/unset")),
                    vec![Assertion::body_contains("Bearer folder-token")],
                )),
                req_item(explicit_none),
            ],
            None,
        )],
        Some(AuthConfig::bearer("folder-token")),
    ));

    // Folder by name, by path, case-insensitive.
    for selector in ["Nested", "admin/nested"] {
        let plan = plan_requests(&coll, Some(selector)).unwrap();
        assert_eq!(plan.len(), 3, "selector {selector}");
        assert_eq!(plan[0].folder.as_deref(), Some("Admin/Nested"));
    }
    assert!(plan_requests(&coll, Some("Missing")).is_err());

    let client = HttpClient::new().unwrap();
    let options = RunnerOptions {
        folder: Some("Admin".into()),
        ..RunnerOptions::default()
    };
    let summary = run_collection(&client, &coll, None, &options, |_| {})
        .await
        .unwrap();
    assert_eq!(summary.total, 3);
    assert!(summary.is_success(), "{}", to_text(&summary));
    let no_auth_body = summary.results[2]
        .response
        .as_ref()
        .unwrap()
        .body_text()
        .unwrap()
        .to_lowercase();
    assert!(!no_auth_body.contains("authorization"));

    // Whole collection: the root request inherits the collection auth.
    let plan = plan_requests(&coll, None).unwrap();
    assert!(matches!(
        &plan[0].request.auth,
        Some(AuthConfig::Bearer { token }) if token == "coll-token"
    ));

    let err = run_collection(
        &client,
        &coll,
        None,
        &RunnerOptions {
            folder: Some("Missing".into()),
            ..RunnerOptions::default()
        },
        |_| {},
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("Missing"));
}

#[tokio::test]
async fn zero_iterations_is_rejected() {
    let mut coll = Collection::new("API");
    coll.add_request(get("a", "http://127.0.0.1:9/"));
    let options = RunnerOptions {
        iterations: 0,
        ..RunnerOptions::default()
    };
    let err = run_collection(&HttpClient::new().unwrap(), &coll, None, &options, |_| {})
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), "iterations must be at least 1");
}

#[tokio::test]
async fn report_url_includes_enabled_query_params() {
    let base = spawn_server().await;
    let mut coll = Collection::new("API");
    let mut req = get("search", format!("{base}/search?fixed=1"))
        .with_param("q", "a b")
        .with_param("off", "x");
    req.params[1].enabled = false;
    coll.add_request(req);
    let summary = run_collection(
        &HttpClient::new().unwrap(),
        &coll,
        None,
        &RunnerOptions::default(),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(
        summary.results[0].url,
        format!("{base}/search?fixed=1&q=a+b")
    );
}

#[tokio::test]
async fn network_errors_and_iterations() {
    let base = spawn_server().await;
    let refused = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let mut coll = Collection::new("API");
    coll.add_request(get("ok", format!("{base}/a")));
    coll.add_request(get("down", format!("http://127.0.0.1:{refused}/")));

    let client = HttpClient::new().unwrap();
    let options = RunnerOptions {
        iterations: 2,
        delay_ms: 5,
        ..RunnerOptions::default()
    };
    let summary = run_collection(&client, &coll, None, &options, |_| {})
        .await
        .unwrap();
    assert_eq!(summary.total, 4);
    assert_eq!(summary.errors, 2);
    assert_eq!(summary.results[3].iteration, 2);
    assert_eq!(summary.results[3].index, 3);
    assert_eq!(summary.results[1].status, RequestRunStatus::Error);
    assert!(summary.results[1].error.is_some());
}

#[tokio::test]
async fn json_junit_and_text_reports() {
    let base = spawn_server().await;
    let mut coll = Collection::new("Demo & <API>");
    coll.add_request(with_assertions(
        get("ok", format!("{base}/a")),
        vec![Assertion::status_equals(200)],
    ));
    coll.items.push(folder(
        "Users",
        vec![req_item(with_assertions(
            get("bad \"quote\"", format!("{base}/status/503")),
            vec![Assertion::status_equals(200)],
        ))],
        None,
    ));
    let mut ws = get("socket", "ws://127.0.0.1:1/");
    ws.protocol = Protocol::WebSocket;
    coll.add_request(ws);

    let client = HttpClient::new().unwrap();
    let summary = run_collection(&client, &coll, None, &RunnerOptions::default(), |_| {})
        .await
        .unwrap();

    let json: serde_json::Value = serde_json::from_str(&to_json(&summary).unwrap()).unwrap();
    assert_eq!(json["total"], 3);
    assert_eq!(json["passed"], 1);
    assert_eq!(json["failed"], 1);
    assert_eq!(json["skipped"], 1);
    assert_eq!(json["results"][1]["status"], "failed");
    assert_eq!(json["results"][1]["http_status"], 503);
    assert_eq!(json["results"][1]["folder"], "Users");
    assert!(json["results"][0].get("response").is_none());

    let xml = to_junit_xml(&summary);
    assert!(xml.starts_with("<?xml"));
    assert!(xml.contains(
        "<testsuites name=\"hitt\" tests=\"3\" failures=\"1\" errors=\"0\" skipped=\"1\""
    ));
    assert!(xml.contains("<testsuite name=\"Demo &amp; &lt;API&gt;\""));
    assert!(xml.contains("name=\"GET bad &quot;quote&quot;\""));
    assert!(xml.contains("classname=\"Demo &amp; &lt;API&gt;.Users\""));
    assert!(xml.contains("<failure message=\"1 of 1 assertion(s) failed\""));
    assert!(xml.contains("<skipped message="));
    assert_eq!(xml.matches("<testcase ").count(), 3);
    assert!(xml.trim_end().ends_with("</testsuites>"));

    let text = to_text(&summary);
    assert!(text.contains("3 total, 1 passed, 1 failed, 0 errors, 1 skipped"));
    assert!(text.contains("Expected status 200, got 503"));
}

#[test]
fn effective_auth_in_inherits_from_folder_then_collection() {
    use hitt_core::core::auth::AuthConfig;
    use hitt_core::core::collection::{Collection, CollectionItem};
    use hitt_core::core::request::{HttpMethod, Request};
    use hitt_core::testing::collection_runner::effective_auth_in;

    let bearer = |t: &str| AuthConfig::Bearer { token: t.into() };
    let mut coll = Collection::new("C");
    coll.auth = Some(bearer("root"));

    let top = Request::new("top", HttpMethod::GET, "http://x");
    let nested = Request::new("nested", HttpMethod::GET, "http://x");
    let mut own = Request::new("own", HttpMethod::GET, "http://x");
    own.auth = Some(bearer("own"));
    let mut none = Request::new("none", HttpMethod::GET, "http://x");
    none.auth = Some(AuthConfig::None);

    coll.items
        .push(CollectionItem::Request(Box::new(top.clone())));
    coll.items.push(CollectionItem::Folder {
        id: uuid::Uuid::new_v4(),
        name: "F".into(),
        items: vec![
            CollectionItem::Request(Box::new(nested.clone())),
            CollectionItem::Request(Box::new(own.clone())),
            CollectionItem::Request(Box::new(none.clone())),
        ],
        auth: Some(bearer("folder")),
        description: None,
    });

    let token = |auth: Option<AuthConfig>| match auth {
        Some(AuthConfig::Bearer { token }) => token,
        Some(AuthConfig::None) => "none".into(),
        other => panic!("unexpected auth: {other:?}"),
    };
    assert_eq!(token(effective_auth_in(&coll, &top)), "root");
    assert_eq!(token(effective_auth_in(&coll, &nested)), "folder");
    assert_eq!(token(effective_auth_in(&coll, &own)), "own");
    assert_eq!(token(effective_auth_in(&coll, &none)), "none");
}
