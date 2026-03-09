use std::time::Duration;
use uuid::Uuid;

use hitt::core::auth::AuthConfig;
use hitt::core::collection::{Collection, CollectionItem};
use hitt::core::environment::Environment;
use hitt::core::history::{HistoryEntry, HistoryStore};
use hitt::core::request::{HttpMethod, KeyValuePair, Request, RequestBody};
use hitt::core::response::{RequestTiming, Response, ResponseBody, ResponseSize};

// ── Request ─────────────────────────────────────────────────────────────────

#[test]
fn request_new_defaults() {
    let req = Request::new("Test", HttpMethod::GET, "https://example.com");
    assert_eq!(req.name, "Test");
    assert_eq!(req.method, HttpMethod::GET);
    assert_eq!(req.url, "https://example.com");
    assert!(req.headers.is_empty());
    assert!(req.params.is_empty());
    assert!(req.auth.is_none());
    assert!(req.body.is_none());
    assert!(req.assertions.is_empty());
}

#[test]
fn request_builder_chain() {
    let req = Request::new("Test", HttpMethod::POST, "https://example.com")
        .with_header("Content-Type", "application/json")
        .with_param("page", "1")
        .with_body(RequestBody::Json(r#"{"key":"value"}"#.to_string()))
        .with_auth(AuthConfig::bearer("tok123"));

    assert_eq!(req.headers.len(), 1);
    assert_eq!(req.headers[0].key, "Content-Type");
    assert_eq!(req.params.len(), 1);
    assert!(matches!(req.body, Some(RequestBody::Json(_))));
    assert!(matches!(req.auth, Some(AuthConfig::Bearer { .. })));
}

// ── HttpMethod ──────────────────────────────────────────────────────────────

#[test]
fn http_method_from_str_all_valid() {
    let cases = vec![
        ("GET", HttpMethod::GET),
        ("POST", HttpMethod::POST),
        ("PUT", HttpMethod::PUT),
        ("PATCH", HttpMethod::PATCH),
        ("DELETE", HttpMethod::DELETE),
        ("HEAD", HttpMethod::HEAD),
        ("OPTIONS", HttpMethod::OPTIONS),
        ("TRACE", HttpMethod::TRACE),
    ];
    for (s, expected) in cases {
        assert_eq!(HttpMethod::from_str(s), Some(expected), "failed for {}", s);
    }
}

#[test]
fn http_method_from_str_case_insensitive() {
    assert_eq!(HttpMethod::from_str("get"), Some(HttpMethod::GET));
    assert_eq!(HttpMethod::from_str("Post"), Some(HttpMethod::POST));
}

#[test]
fn http_method_from_str_invalid() {
    assert_eq!(HttpMethod::from_str("INVALID"), None);
    assert_eq!(HttpMethod::from_str(""), None);
}

#[test]
fn http_method_as_str_roundtrip() {
    for m in HttpMethod::all() {
        let s = m.as_str();
        assert_eq!(HttpMethod::from_str(s), Some(*m));
    }
}

#[test]
fn http_method_display() {
    assert_eq!(format!("{}", HttpMethod::GET), "GET");
    assert_eq!(format!("{}", HttpMethod::DELETE), "DELETE");
}

// ── RequestBody ─────────────────────────────────────────────────────────────

#[test]
fn request_body_content_type() {
    assert_eq!(
        RequestBody::Json("{}".into()).content_type(),
        Some("application/json")
    );
    assert_eq!(
        RequestBody::FormUrlEncoded(vec![]).content_type(),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(RequestBody::FormData(vec![]).content_type(), None);
    assert_eq!(
        RequestBody::Raw {
            content: String::new(),
            content_type: "text/xml".into()
        }
        .content_type(),
        Some("text/xml")
    );
    assert_eq!(
        RequestBody::Binary(std::path::PathBuf::new()).content_type(),
        Some("application/octet-stream")
    );
    assert_eq!(
        RequestBody::GraphQL {
            query: String::new(),
            variables: None
        }
        .content_type(),
        Some("application/json")
    );
    assert_eq!(
        RequestBody::Protobuf {
            message: String::new()
        }
        .content_type(),
        Some("application/grpc")
    );
    assert_eq!(RequestBody::None.content_type(), None);
}

// ── Response helpers ────────────────────────────────────────────────────────

fn make_response(status: u16, body: ResponseBody, headers: Vec<KeyValuePair>) -> Response {
    Response {
        id: Uuid::new_v4(),
        status,
        status_text: String::new(),
        headers,
        body,
        cookies: vec![],
        timing: RequestTiming::default(),
        size: ResponseSize::default(),
        assertion_results: vec![],
        timestamp: chrono::Utc::now(),
    }
}

#[test]
fn response_status_helpers() {
    assert!(make_response(200, ResponseBody::Empty, vec![]).is_success());
    assert!(make_response(299, ResponseBody::Empty, vec![]).is_success());
    assert!(!make_response(300, ResponseBody::Empty, vec![]).is_success());

    assert!(make_response(301, ResponseBody::Empty, vec![]).is_redirect());
    assert!(!make_response(200, ResponseBody::Empty, vec![]).is_redirect());

    assert!(make_response(404, ResponseBody::Empty, vec![]).is_client_error());
    assert!(!make_response(500, ResponseBody::Empty, vec![]).is_client_error());

    assert!(make_response(503, ResponseBody::Empty, vec![]).is_server_error());
    assert!(!make_response(200, ResponseBody::Empty, vec![]).is_server_error());
}

#[test]
fn response_header_value_case_insensitive() {
    let resp = make_response(
        200,
        ResponseBody::Empty,
        vec![KeyValuePair::new("Content-Type", "application/json")],
    );
    assert_eq!(resp.header_value("content-type"), Some("application/json"));
    assert_eq!(resp.header_value("CONTENT-TYPE"), Some("application/json"));
    assert_eq!(resp.header_value("X-Missing"), None);
}

#[test]
fn response_body_text_and_json() {
    let resp = make_response(
        200,
        ResponseBody::Json(r#"{"name":"test"}"#.into()),
        vec![],
    );
    assert_eq!(resp.body_text(), Some(r#"{"name":"test"}"#));
    let json = resp.body_json().unwrap();
    assert_eq!(json["name"], "test");

    let resp2 = make_response(200, ResponseBody::Empty, vec![]);
    assert_eq!(resp2.body_text(), None);
    assert!(resp2.body_json().is_none());
}

// ── ResponseBody ────────────────────────────────────────────────────────────

#[test]
fn response_body_from_content_type() {
    assert!(matches!(
        ResponseBody::from_content_type("{}".into(), Some("application/json")),
        ResponseBody::Json(_)
    ));
    assert!(matches!(
        ResponseBody::from_content_type("<x/>".into(), Some("application/xml")),
        ResponseBody::Xml(_)
    ));
    assert!(matches!(
        ResponseBody::from_content_type("<html>".into(), Some("text/html")),
        ResponseBody::Html(_)
    ));
    assert!(matches!(
        ResponseBody::from_content_type("data".into(), Some("application/octet-stream")),
        ResponseBody::Binary(_)
    ));
    assert!(matches!(
        ResponseBody::from_content_type("plain".into(), Some("text/plain")),
        ResponseBody::Text(_)
    ));
    assert!(matches!(
        ResponseBody::from_content_type("no type".into(), None),
        ResponseBody::Text(_)
    ));
}

#[test]
fn response_body_len() {
    assert_eq!(ResponseBody::Text("hello".into()).len(), 5);
    assert_eq!(ResponseBody::Json("{}".into()).len(), 2);
    assert_eq!(ResponseBody::Empty.len(), 0);
    assert!(ResponseBody::Empty.is_empty());
    assert!(!ResponseBody::Text("x".into()).is_empty());
}

// ── ResponseSize ────────────────────────────────────────────────────────────

#[test]
fn response_size_format() {
    assert_eq!(
        (ResponseSize { headers: 100, body: 400 }).format(),
        "500 B"
    );
    assert_eq!(
        (ResponseSize { headers: 0, body: 2048 }).format(),
        "2.0 KB"
    );
    assert_eq!(
        (ResponseSize {
            headers: 0,
            body: 1_500_000
        })
        .format(),
        "1.4 MB"
    );
}

// ── RequestTiming ───────────────────────────────────────────────────────────

#[test]
fn request_timing_format_total() {
    let t = RequestTiming::simple(Duration::from_millis(250));
    assert_eq!(t.format_total(), "250ms");

    let t = RequestTiming::simple(Duration::from_millis(1500));
    assert_eq!(t.format_total(), "1.5s");
}

// ── Collection ──────────────────────────────────────────────────────────────

#[test]
fn collection_new() {
    let c = Collection::new("My API");
    assert_eq!(c.name, "My API");
    assert!(c.items.is_empty());
    assert!(c.variables.is_empty());
}

#[test]
fn collection_add_request_and_count() {
    let mut c = Collection::new("Test");
    c.add_request(Request::new("R1", HttpMethod::GET, "/a"));
    c.add_request(Request::new("R2", HttpMethod::POST, "/b"));
    assert_eq!(c.request_count(), 2);
}

#[test]
fn collection_add_folder() {
    let mut c = Collection::new("Test");
    let folder = c.add_folder("Auth Endpoints");
    folder.push(CollectionItem::Request(Request::new(
        "Login",
        HttpMethod::POST,
        "/login",
    )));
    assert_eq!(c.items.len(), 1);
    assert!(c.items[0].is_folder());
    assert_eq!(c.request_count(), 1);
}

#[test]
fn collection_find_request() {
    let mut c = Collection::new("Test");
    let req = Request::new("Find Me", HttpMethod::GET, "/find");
    let id = req.id;
    c.add_request(req);

    assert!(c.find_request(&id).is_some());
    assert_eq!(c.find_request(&id).unwrap().name, "Find Me");
    assert!(c.find_request(&Uuid::new_v4()).is_none());
}

#[test]
fn collection_find_request_mut() {
    let mut c = Collection::new("Test");
    let req = Request::new("Mutable", HttpMethod::GET, "/mut");
    let id = req.id;
    c.add_request(req);

    c.find_request_mut(&id).unwrap().name = "Updated".to_string();
    assert_eq!(c.find_request(&id).unwrap().name, "Updated");
}

#[test]
fn collection_find_request_in_folder() {
    let mut c = Collection::new("Test");
    let req = Request::new("Nested", HttpMethod::GET, "/nested");
    let id = req.id;
    let folder = c.add_folder("Folder");
    folder.push(CollectionItem::Request(Request::new(
        "Nested",
        HttpMethod::GET,
        "/nested",
    )));
    // Get the actual ID of the request we just pushed
    let nested_id = match &c.items[0] {
        CollectionItem::Folder { items, .. } => match &items[0] {
            CollectionItem::Request(r) => r.id,
            _ => panic!(),
        },
        _ => panic!(),
    };
    assert!(c.find_request(&nested_id).is_some());
}

#[test]
fn collection_all_requests() {
    let mut c = Collection::new("Test");
    c.add_request(Request::new("R1", HttpMethod::GET, "/a"));
    let folder = c.add_folder("Folder");
    folder.push(CollectionItem::Request(Request::new(
        "R2",
        HttpMethod::POST,
        "/b",
    )));
    let reqs = c.all_requests();
    assert_eq!(reqs.len(), 2);
}

// ── CollectionItem ──────────────────────────────────────────────────────────

#[test]
fn collection_item_helpers() {
    let req = CollectionItem::Request(Request::new("Test", HttpMethod::GET, "/"));
    assert_eq!(req.name(), "Test");
    assert!(!req.is_folder());

    let folder = CollectionItem::Folder {
        id: Uuid::new_v4(),
        name: "Folder".into(),
        items: vec![],
        auth: None,
        description: None,
    };
    assert_eq!(folder.name(), "Folder");
    assert!(folder.is_folder());
}

// ── KeyValuePair ────────────────────────────────────────────────────────────

#[test]
fn key_value_pair_builder() {
    let kv = KeyValuePair::new("key", "val")
        .with_description("A description")
        .disabled();
    assert_eq!(kv.key, "key");
    assert_eq!(kv.value, "val");
    assert_eq!(kv.description, Some("A description".into()));
    assert!(!kv.enabled);
}

// ── HistoryStore ────────────────────────────────────────────────────────────

#[test]
fn history_store_add_and_search() {
    let mut store = HistoryStore::new(100);
    assert!(store.is_empty());

    store.add(HistoryEntry::new(HttpMethod::GET, "https://api.example.com/users"));
    store.add(HistoryEntry::new(HttpMethod::POST, "https://api.example.com/posts"));
    assert_eq!(store.len(), 2);

    let results = store.search("users");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].url, "https://api.example.com/users");
}

#[test]
fn history_store_get_by_id() {
    let mut store = HistoryStore::new(100);
    let entry = HistoryEntry::new(HttpMethod::GET, "https://example.com");
    let id = entry.id;
    store.add(entry);
    assert!(store.get(&id).is_some());
    assert!(store.get(&Uuid::new_v4()).is_none());
}

#[test]
fn history_store_truncation() {
    let mut store = HistoryStore::new(3);
    for i in 0..5 {
        store.add(HistoryEntry::new(HttpMethod::GET, format!("https://example.com/{}", i)));
    }
    assert_eq!(store.len(), 3);
}

#[test]
fn history_store_clear() {
    let mut store = HistoryStore::new(100);
    store.add(HistoryEntry::new(HttpMethod::GET, "https://example.com"));
    store.clear();
    assert!(store.is_empty());
}

#[test]
fn history_store_search_by_method() {
    let mut store = HistoryStore::new(100);
    store.add(HistoryEntry::new(HttpMethod::GET, "https://example.com/a"));
    store.add(HistoryEntry::new(HttpMethod::POST, "https://example.com/b"));
    let results = store.search("post");
    assert_eq!(results.len(), 1);
}

// ── Environment ─────────────────────────────────────────────────────────────

#[test]
fn environment_new_and_add() {
    let mut env = Environment::new("Test");
    env.add_variable("key1", "val1");
    env.add_secret("api_key", "secret123");

    assert_eq!(env.get("key1"), Some("val1"));
    assert_eq!(env.get("api_key"), Some("secret123"));
    assert_eq!(env.get("missing"), None);
}

#[test]
fn environment_set_update_and_insert() {
    let mut env = Environment::new("Test");
    env.add_variable("key", "original");
    env.set("key", "updated");
    assert_eq!(env.get("key"), Some("updated"));

    // Set non-existent key creates it
    env.set("new_key", "new_val");
    assert_eq!(env.get("new_key"), Some("new_val"));
}

#[test]
fn environment_active_variables() {
    let mut env = Environment::new("Test");
    env.add_variable("a", "1");
    env.add_variable("b", "2");
    // Disable one
    env.values[1].enabled = false;

    let active: Vec<_> = env.active_variables().collect();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0], ("a", "1"));
}
