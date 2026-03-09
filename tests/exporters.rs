use hitt::core::auth::AuthConfig;
use hitt::core::collection::Collection;
use hitt::core::request::{HttpMethod, KeyValuePair, Request, RequestBody};
use hitt::core::variables::VariableResolver;
use hitt::exporters::curl::to_curl;
use hitt::exporters::markdown_docs::generate_docs;
use hitt::postman::export::export_postman_collection;

// ── cURL exporter ───────────────────────────────────────────────────────────

#[test]
fn curl_export_simple_get() {
    let req = Request::new("Test", HttpMethod::GET, "https://api.example.com/users");
    let resolver = VariableResolver::new();
    let curl = to_curl(&req, &resolver);
    assert!(curl.starts_with("curl"));
    assert!(curl.contains("https://api.example.com/users"));
    // GET should not have -X flag
    assert!(!curl.contains("-X"));
}

#[test]
fn curl_export_post_with_json() {
    let req = Request::new("Test", HttpMethod::POST, "https://api.example.com/users")
        .with_body(RequestBody::Json(r#"{"name":"John"}"#.to_string()));
    let resolver = VariableResolver::new();
    let curl = to_curl(&req, &resolver);
    assert!(curl.contains("-X POST"));
    assert!(curl.contains("-d"));
    assert!(curl.contains("Content-Type: application/json"));
}

#[test]
fn curl_export_headers() {
    let req = Request::new("Test", HttpMethod::GET, "https://api.example.com")
        .with_header("Accept", "application/json")
        .with_header("X-Custom", "value");
    let resolver = VariableResolver::new();
    let curl = to_curl(&req, &resolver);
    assert!(curl.contains("-H 'Accept: application/json'"));
    assert!(curl.contains("-H 'X-Custom: value'"));
}

#[test]
fn curl_export_bearer_auth() {
    let req = Request::new("Test", HttpMethod::GET, "https://api.example.com")
        .with_auth(AuthConfig::bearer("my-token"));
    let resolver = VariableResolver::new();
    let curl = to_curl(&req, &resolver);
    assert!(curl.contains("Authorization: Bearer my-token"));
}

#[test]
fn curl_export_basic_auth() {
    let req = Request::new("Test", HttpMethod::GET, "https://api.example.com")
        .with_auth(AuthConfig::basic("user", "pass"));
    let resolver = VariableResolver::new();
    let curl = to_curl(&req, &resolver);
    assert!(curl.contains("-u 'user:pass'"));
}

#[test]
fn curl_export_query_params() {
    let req = Request::new("Test", HttpMethod::GET, "https://api.example.com/search")
        .with_param("q", "test")
        .with_param("page", "1");
    let resolver = VariableResolver::new();
    let curl = to_curl(&req, &resolver);
    assert!(curl.contains("q=test"));
    assert!(curl.contains("page=1"));
}

#[test]
fn curl_export_resolves_variables() {
    let req = Request::new("Test", HttpMethod::GET, "{{base_url}}/users");
    let mut resolver = VariableResolver::new();
    let mut vars = std::collections::HashMap::new();
    vars.insert("base_url".to_string(), "https://api.example.com".to_string());
    resolver.add_scope("env", vars);
    let curl = to_curl(&req, &resolver);
    assert!(curl.contains("https://api.example.com/users"));
}

// ── Markdown exporter ───────────────────────────────────────────────────────

#[test]
fn markdown_collection_heading() {
    let c = Collection::new("My API");
    let md = generate_docs(&c);
    assert!(md.starts_with("# My API"));
}

#[test]
fn markdown_contains_requests() {
    let mut c = Collection::new("Test API");
    c.add_request(
        Request::new("Get Users", HttpMethod::GET, "https://api.example.com/users")
            .with_header("Accept", "application/json"),
    );
    let md = generate_docs(&c);
    assert!(md.contains("`GET`"));
    assert!(md.contains("Get Users"));
    assert!(md.contains("https://api.example.com/users"));
    assert!(md.contains("Table of Contents"));
}

#[test]
fn markdown_folder_headings() {
    let mut c = Collection::new("Test API");
    let folder = c.add_folder("Authentication");
    folder.push(hitt::core::collection::CollectionItem::Request(
        Request::new("Login", HttpMethod::POST, "/login"),
    ));
    let md = generate_docs(&c);
    assert!(md.contains("Authentication"));
    assert!(md.contains("Login"));
}

// ── Postman exporter ────────────────────────────────────────────────────────

#[test]
fn postman_export_valid_json() {
    let mut c = Collection::new("Export Test");
    c.add_request(Request::new("R1", HttpMethod::GET, "https://example.com"));
    let json = export_postman_collection(&c).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed["info"]["name"].as_str() == Some("Export Test"));
    assert!(parsed["info"]["schema"]
        .as_str()
        .unwrap()
        .contains("collection/v2.1.0"));
}

#[test]
fn postman_export_preserves_requests() {
    let mut c = Collection::new("Test");
    c.add_request(
        Request::new("Get Items", HttpMethod::GET, "https://example.com/items")
            .with_header("Accept", "application/json"),
    );
    c.add_request(
        Request::new("Create Item", HttpMethod::POST, "https://example.com/items")
            .with_body(RequestBody::Json(r#"{"name":"test"}"#.into())),
    );
    let json = export_postman_collection(&c).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let items = parsed["item"].as_array().unwrap();
    assert_eq!(items.len(), 2);
}

#[test]
fn postman_export_variables() {
    let mut c = Collection::new("Test");
    c.variables.push(KeyValuePair::new("base_url", "https://api.example.com"));
    let json = export_postman_collection(&c).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let vars = parsed["variable"].as_array().unwrap();
    assert_eq!(vars.len(), 1);
    assert_eq!(vars[0]["key"], "base_url");
}
