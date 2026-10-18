use std::time::Duration;

use tempfile::TempDir;

use hitt_core::core::constants::MAX_PERSISTED_HISTORY_BODY;
use hitt_core::core::history::{HistoryEntry, HistoryStore};
use hitt_core::core::request::{HttpMethod, KeyValuePair, Request, RequestBody};
use hitt_core::core::response::{RequestTiming, Response, ResponseBody, ResponseSize};
use hitt_core::storage::history_store::{clear_history, history_file, load_history, save_history};

fn sample_request() -> Request {
    let mut req = Request::new("create user", HttpMethod::POST, "{{base}}/users");
    req.headers.push(KeyValuePair::new("X-Trace", "abc"));
    req.params.push(KeyValuePair::new("verbose", "1"));
    req.body = Some(RequestBody::Json(r#"{"name":"ada"}"#.into()));
    req
}

fn sample_response(body: &str) -> Response {
    Response {
        id: uuid::Uuid::new_v4(),
        status: 201,
        status_text: "Created".into(),
        headers: vec![KeyValuePair::new("Content-Type", "application/json")],
        body: ResponseBody::Json(body.into()),
        cookies: Vec::new(),
        timing: RequestTiming::simple(Duration::from_millis(42)),
        size: ResponseSize {
            headers: 30,
            body: body.len(),
        },
        assertion_results: Vec::new(),
        timestamp: chrono::Utc::now(),
    }
}

#[test]
fn from_exchange_keeps_request_snapshot_and_response() {
    let req = sample_request();
    let entry = HistoryEntry::from_exchange(&req, &sample_response(r#"{"id":1}"#));
    assert_eq!(entry.url, "{{base}}/users", "URL is stored unresolved");
    assert_eq!(entry.status, Some(201));
    assert_eq!(entry.duration_ms, Some(42));
    assert_eq!(entry.request_id, Some(req.id));
    assert_eq!(entry.request_body.as_deref(), Some(r#"{"name":"ada"}"#));
    assert_eq!(entry.response_body.as_deref(), Some(r#"{"id":1}"#));
    let snapshot = entry.request.as_ref().unwrap();
    assert_eq!(snapshot.headers[0].key, "X-Trace");
    assert_eq!(snapshot.params[0].key, "verbose");
}

#[test]
fn to_request_uses_snapshot_with_fresh_id() {
    let req = sample_request();
    let entry = HistoryEntry::from_exchange(&req, &sample_response("{}"));
    let reopened = entry.to_request();
    assert_ne!(reopened.id, req.id);
    assert_eq!(reopened.name, req.name);
    assert_eq!(reopened.url, req.url);
    assert_eq!(reopened.headers.len(), 1);
    assert!(matches!(reopened.body, Some(RequestBody::Json(_))));
}

#[test]
fn to_request_falls_back_to_method_and_url() {
    let entry = HistoryEntry::new(HttpMethod::DELETE, "https://api.test/items/7");
    let req = entry.to_request();
    assert_eq!(req.method, HttpMethod::DELETE);
    assert_eq!(req.url, "https://api.test/items/7");
    assert!(req.headers.is_empty());
}

#[test]
fn to_response_rebuilds_stored_response() {
    let entry = HistoryEntry::from_exchange(&sample_request(), &sample_response(r#"{"id":1}"#));
    let resp = entry.to_response().unwrap();
    assert_eq!(resp.status, 201);
    assert_eq!(resp.status_text, "Created");
    assert!(matches!(resp.body, ResponseBody::Json(_)));
    assert_eq!(resp.body_text(), Some(r#"{"id":1}"#));
    assert_eq!(resp.timing.total, Duration::from_millis(42));
    assert_eq!(resp.content_type(), Some("application/json"));

    // No status recorded -> nothing to show.
    assert!(HistoryEntry::new(HttpMethod::GET, "http://x")
        .to_response()
        .is_none());
}

#[test]
fn old_entries_without_snapshot_still_deserialize() {
    let json = r#"{
        "id": "6f1c1b8e-6a43-4f0e-9d39-8f1f0c5e2a11",
        "method": "GET",
        "url": "http://old.test",
        "status": 200,
        "duration_ms": 5,
        "size_bytes": 10,
        "timestamp": "2026-01-01T00:00:00Z",
        "collection_id": null,
        "request_id": null,
        "response_body": "ok",
        "request_body": null
    }"#;
    let entry: HistoryEntry = serde_json::from_str(json).unwrap();
    assert!(entry.request.is_none());
    assert!(entry.response_headers.is_empty());
    assert_eq!(entry.to_request().url, "http://old.test");
}

#[test]
fn short_url_is_char_boundary_safe() {
    let entry = HistoryEntry::new(HttpMethod::GET, "https://example.test/ñandú/ü");
    // Would panic with byte slicing inside a multi-byte char.
    for max in 0..30 {
        let _ = entry.short_url(max);
    }
}

#[test]
fn store_enforces_limit() {
    let mut store = HistoryStore::new(3);
    for i in 0..5 {
        store.add(HistoryEntry::new(HttpMethod::GET, format!("http://x/{i}")));
    }
    assert_eq!(store.len(), 3);
    assert_eq!(store.entries()[0].url, "http://x/4", "newest first");

    store.set_max_entries(1);
    assert_eq!(store.len(), 1);
    assert_eq!(store.max_entries(), 1);
    assert_eq!(store.entries()[0].url, "http://x/4");

    let entries = (0..4)
        .map(|i| HistoryEntry::new(HttpMethod::GET, format!("http://y/{i}")))
        .collect();
    assert_eq!(HistoryStore::with_entries(2, entries).len(), 2);
}

#[test]
fn history_file_roundtrip_and_clear() {
    let tmp = TempDir::new().unwrap();
    let path = history_file(&tmp.path().join("collections"));
    assert_eq!(path, tmp.path().join("history.json"));
    assert!(load_history(&path, 10).unwrap().is_empty(), "missing file");

    let small = HistoryEntry::from_exchange(&sample_request(), &sample_response("{}"));
    let big_body = "x".repeat(MAX_PERSISTED_HISTORY_BODY + 1);
    let big = HistoryEntry::from_exchange(&sample_request(), &sample_response(&big_body));
    save_history(&path, &[big.clone(), small.clone()]).unwrap();

    let loaded = load_history(&path, 10).unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].id, big.id);
    assert!(
        loaded[0].response_body.is_none(),
        "large body not persisted"
    );
    assert!(loaded[0].request.is_some(), "snapshot still persisted");
    assert_eq!(loaded[1].response_body.as_deref(), Some("{}"));
    assert_eq!(loaded[1].request.as_ref().unwrap().headers[0].value, "abc");

    assert_eq!(load_history(&path, 1).unwrap().len(), 1, "limit applied");

    clear_history(&path).unwrap();
    assert!(!path.exists());
    clear_history(&path).unwrap();
}
