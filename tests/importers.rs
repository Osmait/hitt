use hitt::core::auth::AuthConfig;
use hitt::core::request::{HttpMethod, RequestBody};
use hitt::importers::curl::parse_curl;
use hitt::importers::dotenv::parse_dotenv;
use hitt::importers::har::import_har;
use hitt::importers::openapi::import_openapi;
use hitt::postman::env_import::import_postman_environment;
use hitt::postman::import::import_postman_collection;

// ── cURL importer ───────────────────────────────────────────────────────────

#[test]
fn curl_simple_get() {
    let req = parse_curl("curl https://api.example.com/users").unwrap();
    assert_eq!(req.method, HttpMethod::GET);
    assert_eq!(req.url, "https://api.example.com/users");
}

#[test]
fn curl_post_with_json() {
    let req = parse_curl(
        r#"curl -X POST https://api.example.com/users -H "Content-Type: application/json" -d '{"name":"John"}'"#,
    )
    .unwrap();
    assert_eq!(req.method, HttpMethod::POST);
    assert!(matches!(req.body, Some(RequestBody::Json(_))));
}

#[test]
fn curl_put_method() {
    let req = parse_curl(
        r#"curl -X PUT https://api.example.com/users/1 -d '{"name":"Jane"}'"#,
    )
    .unwrap();
    assert_eq!(req.method, HttpMethod::PUT);
}

#[test]
fn curl_multiple_headers() {
    let req = parse_curl(
        r#"curl -H "Accept: application/json" -H "X-Api-Key: abc123" -H "Cache-Control: no-cache" https://api.example.com"#,
    )
    .unwrap();
    assert_eq!(req.headers.len(), 3);
    assert_eq!(req.headers[0].key, "Accept");
    assert_eq!(req.headers[1].key, "X-Api-Key");
    assert_eq!(req.headers[2].key, "Cache-Control");
}

#[test]
fn curl_bearer_auth_via_header() {
    let req = parse_curl(
        r#"curl -H "Authorization: Bearer mytoken" https://api.example.com"#,
    )
    .unwrap();
    assert!(req
        .headers
        .iter()
        .any(|h| h.key == "Authorization" && h.value == "Bearer mytoken"));
}

#[test]
fn curl_basic_auth() {
    let req = parse_curl("curl -u admin:password https://api.example.com").unwrap();
    assert!(matches!(req.auth, Some(AuthConfig::Basic { .. })));
    if let Some(AuthConfig::Basic { username, password }) = &req.auth {
        assert_eq!(username, "admin");
        assert_eq!(password, "password");
    }
}

#[test]
fn curl_form_data() {
    let req = parse_curl(
        r#"curl -F "name=John" -F "file=@photo.jpg" https://api.example.com/upload"#,
    )
    .unwrap();
    assert!(matches!(req.body, Some(RequestBody::FormData(_))));
    if let Some(RequestBody::FormData(fields)) = &req.body {
        assert_eq!(fields.len(), 2);
    }
}

#[test]
fn curl_url_with_query_params() {
    let req = parse_curl("curl 'https://api.example.com/search?q=test&page=2'").unwrap();
    assert_eq!(req.url, "https://api.example.com/search");
    assert_eq!(req.params.len(), 2);
    assert_eq!(req.params[0].key, "q");
    assert_eq!(req.params[1].key, "page");
}

#[test]
fn curl_implicit_post_with_data() {
    let req = parse_curl(r#"curl -d '{"test":true}' https://api.example.com"#).unwrap();
    assert_eq!(req.method, HttpMethod::POST);
}

#[test]
fn curl_line_continuation() {
    let req = parse_curl(
        "curl \\\n  -X DELETE \\\n  https://api.example.com/users/1",
    )
    .unwrap();
    assert_eq!(req.method, HttpMethod::DELETE);
}

// ── Postman importer ────────────────────────────────────────────────────────

#[test]
fn postman_import_collection_name() {
    let content = include_str!("fixtures/sample_postman_collection.json");
    let collection = import_postman_collection(content).unwrap();
    assert_eq!(collection.name, "Sample API");
}

#[test]
fn postman_import_collection_description() {
    let content = include_str!("fixtures/sample_postman_collection.json");
    let collection = import_postman_collection(content).unwrap();
    assert_eq!(
        collection.description.as_deref(),
        Some("A sample Postman collection for testing")
    );
}

#[test]
fn postman_import_folder_structure() {
    let content = include_str!("fixtures/sample_postman_collection.json");
    let collection = import_postman_collection(content).unwrap();
    // Should have one folder "Users"
    assert_eq!(collection.items.len(), 1);
    assert!(collection.items[0].is_folder());
    assert_eq!(collection.items[0].name(), "Users");
}

#[test]
fn postman_import_request_count() {
    let content = include_str!("fixtures/sample_postman_collection.json");
    let collection = import_postman_collection(content).unwrap();
    assert_eq!(collection.request_count(), 2);
}

#[test]
fn postman_import_variables() {
    let content = include_str!("fixtures/sample_postman_collection.json");
    let collection = import_postman_collection(content).unwrap();
    assert_eq!(collection.variables.len(), 2);
    assert!(collection.variables.iter().any(|v| v.key == "base_url"));
    assert!(collection.variables.iter().any(|v| v.key == "auth_token"));
}

#[test]
fn postman_import_collection_auth() {
    let content = include_str!("fixtures/sample_postman_collection.json");
    let collection = import_postman_collection(content).unwrap();
    assert!(collection.auth.is_some());
    assert!(matches!(collection.auth, Some(AuthConfig::Bearer { .. })));
}

#[test]
fn postman_import_request_auth() {
    let content = include_str!("fixtures/sample_postman_collection.json");
    let collection = import_postman_collection(content).unwrap();
    let reqs = collection.all_requests();
    let create_user = reqs.iter().find(|r| r.name == "Create User").unwrap();
    assert!(create_user.auth.is_some());
    assert!(matches!(create_user.auth, Some(AuthConfig::Bearer { .. })));
}

#[test]
fn postman_import_request_body() {
    let content = include_str!("fixtures/sample_postman_collection.json");
    let collection = import_postman_collection(content).unwrap();
    let reqs = collection.all_requests();
    let create_user = reqs.iter().find(|r| r.name == "Create User").unwrap();
    assert!(matches!(create_user.body, Some(RequestBody::Json(_))));
}

#[test]
fn postman_import_environment() {
    let content = include_str!("fixtures/sample_postman_environment.json");
    let env = import_postman_environment(content).unwrap();
    assert_eq!(env.name, "Test Environment");
    assert_eq!(env.values.len(), 3);
    assert_eq!(env.get("host"), Some("api.example.com"));
    assert_eq!(env.get("page_size"), Some("25"));

    // Check secret flag
    let token_var = env.values.iter().find(|v| v.key == "token").unwrap();
    assert!(token_var.secret);
}

// ── OpenAPI importer ────────────────────────────────────────────────────────

#[test]
fn openapi_import_collection_name() {
    let spec = include_str!("fixtures/petstore_openapi.yaml");
    let collection = import_openapi(spec).unwrap();
    assert_eq!(collection.name, "Petstore API");
}

#[test]
fn openapi_import_description() {
    let spec = include_str!("fixtures/petstore_openapi.yaml");
    let collection = import_openapi(spec).unwrap();
    assert_eq!(
        collection.description.as_deref(),
        Some("A sample pet store API")
    );
}

#[test]
fn openapi_import_base_url_variable() {
    let spec = include_str!("fixtures/petstore_openapi.yaml");
    let collection = import_openapi(spec).unwrap();
    let base = collection
        .variables
        .iter()
        .find(|v| v.key == "base_url")
        .unwrap();
    assert_eq!(base.value, "https://petstore.example.com/v1");
}

#[test]
fn openapi_import_request_count() {
    let spec = include_str!("fixtures/petstore_openapi.yaml");
    let collection = import_openapi(spec).unwrap();
    // 3 operations: GET /pets, POST /pets, GET /pets/{petId}
    assert_eq!(collection.request_count(), 3);
}

#[test]
fn openapi_import_tags_become_folders() {
    let spec = include_str!("fixtures/petstore_openapi.yaml");
    let collection = import_openapi(spec).unwrap();
    // All 3 operations have tag "pets"
    assert!(collection.items.iter().any(|i| i.is_folder() && i.name() == "pets"));
}

#[test]
fn openapi_import_methods() {
    let spec = include_str!("fixtures/petstore_openapi.yaml");
    let collection = import_openapi(spec).unwrap();
    let reqs = collection.all_requests();
    let methods: Vec<_> = reqs.iter().map(|r| r.method).collect();
    assert!(methods.contains(&HttpMethod::GET));
    assert!(methods.contains(&HttpMethod::POST));
}

// ── HAR importer ────────────────────────────────────────────────────────────

#[test]
fn har_import_request_count() {
    let content = include_str!("fixtures/sample.har");
    let collection = import_har(content).unwrap();
    assert_eq!(collection.request_count(), 2);
}

#[test]
fn har_import_methods() {
    let content = include_str!("fixtures/sample.har");
    let collection = import_har(content).unwrap();
    let reqs = collection.all_requests();
    assert_eq!(reqs[0].method, HttpMethod::GET);
    assert_eq!(reqs[1].method, HttpMethod::POST);
}

#[test]
fn har_import_urls() {
    let content = include_str!("fixtures/sample.har");
    let collection = import_har(content).unwrap();
    let reqs = collection.all_requests();
    assert!(reqs[0].url.contains("api.example.com/users"));
    assert!(reqs[1].url.contains("api.example.com/users"));
}

#[test]
fn har_import_headers_filtered() {
    let content = include_str!("fixtures/sample.har");
    let collection = import_har(content).unwrap();
    let reqs = collection.all_requests();
    // Should filter out pseudo-headers, cookies, host
    let first = &reqs[0];
    assert!(first.headers.iter().all(|h| !h.key.starts_with(':')));
    assert!(first
        .headers
        .iter()
        .all(|h| !h.key.eq_ignore_ascii_case("host")));
}

#[test]
fn har_import_post_body() {
    let content = include_str!("fixtures/sample.har");
    let collection = import_har(content).unwrap();
    let reqs = collection.all_requests();
    let post_req = &reqs[1];
    assert!(matches!(post_req.body, Some(RequestBody::Json(_))));
}

// ── dotenv importer ─────────────────────────────────────────────────────────

#[test]
fn dotenv_basic_parsing() {
    let content = "KEY=value\nNAME=John";
    let vars = parse_dotenv(content).unwrap();
    assert_eq!(vars.get("KEY").unwrap(), "value");
    assert_eq!(vars.get("NAME").unwrap(), "John");
}

#[test]
fn dotenv_quoted_values() {
    let content = r#"
KEY1="hello world"
KEY2='single quoted'
KEY3=unquoted
"#;
    let vars = parse_dotenv(content).unwrap();
    assert_eq!(vars.get("KEY1").unwrap(), "hello world");
    assert_eq!(vars.get("KEY2").unwrap(), "single quoted");
    assert_eq!(vars.get("KEY3").unwrap(), "unquoted");
}

#[test]
fn dotenv_comments_and_empty_lines() {
    let content = "# Comment\n\nKEY=value\n# Another comment\n";
    let vars = parse_dotenv(content).unwrap();
    assert_eq!(vars.len(), 1);
    assert_eq!(vars.get("KEY").unwrap(), "value");
}

#[test]
fn dotenv_export_prefix() {
    let content = "export API_KEY=abc123";
    let vars = parse_dotenv(content).unwrap();
    assert_eq!(vars.get("API_KEY").unwrap(), "abc123");
}

#[test]
fn dotenv_escape_sequences() {
    let content = r#"MSG="hello\nworld""#;
    let vars = parse_dotenv(content).unwrap();
    assert_eq!(vars.get("MSG").unwrap(), "hello\nworld");
}

#[test]
fn dotenv_empty_value() {
    let content = "EMPTY=";
    let vars = parse_dotenv(content).unwrap();
    assert_eq!(vars.get("EMPTY").unwrap(), "");
}

#[test]
fn dotenv_inline_comment() {
    let content = "KEY=value # this is a comment";
    let vars = parse_dotenv(content).unwrap();
    assert_eq!(vars.get("KEY").unwrap(), "value");
}

#[test]
fn dotenv_fixture_file() {
    let content = include_str!("fixtures/sample.env");
    let vars = parse_dotenv(content).unwrap();
    assert_eq!(vars.get("DB_HOST").unwrap(), "localhost");
    assert_eq!(vars.get("DB_PORT").unwrap(), "5432");
    assert_eq!(vars.get("DB_NAME").unwrap(), "my_database");
    assert_eq!(vars.get("API_KEY").unwrap(), "abc123def456");
    assert_eq!(vars.get("API_SECRET").unwrap(), "single quoted secret");
    assert_eq!(vars.get("DEBUG").unwrap(), "true");
}

#[test]
fn dotenv_special_chars_in_value() {
    let content = "URL=https://example.com/path?key=value";
    let vars = parse_dotenv(content).unwrap();
    // Note: unquoted values with # are split, but URL has no #
    assert!(vars.get("URL").unwrap().starts_with("https://"));
}
