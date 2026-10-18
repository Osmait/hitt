use hitt_core::core::auth::AuthConfig;
use hitt_core::core::collection::Collection;
use hitt_core::core::request::{HttpMethod, KeyValuePair, Request, RequestBody};
use hitt_core::postman::env_export::export_postman_environment;
use hitt_core::postman::env_import::import_postman_environment;
use hitt_core::postman::export::export_postman_collection;
use hitt_core::postman::import::import_postman_collection;

#[test]
fn collection_roundtrip() {
    // Create a collection
    let mut collection = Collection::new("Roundtrip Test");
    collection.description = Some("Testing roundtrip".into());
    collection
        .variables
        .push(KeyValuePair::new("base_url", "https://api.example.com"));
    collection.add_request(
        Request::new("Get Users", HttpMethod::GET, "{{base_url}}/users")
            .with_header("Accept", "application/json"),
    );
    collection.add_request(
        Request::new("Create User", HttpMethod::POST, "{{base_url}}/users")
            .with_body(RequestBody::Json(
                r#"{"name":"John","email":"john@example.com"}"#.to_string(),
            ))
            .with_auth(AuthConfig::bearer("{{token}}")),
    );

    // Export to Postman format
    let json = export_postman_collection(&collection).unwrap();

    // Import back
    let imported = import_postman_collection(&json).unwrap();

    // Compare
    assert_eq!(imported.name, "Roundtrip Test");
    assert_eq!(imported.description.as_deref(), Some("Testing roundtrip"));
    assert_eq!(imported.request_count(), 2);
    assert_eq!(imported.variables.len(), 1);
    assert_eq!(imported.variables[0].key, "base_url");

    let reqs = imported.all_requests();
    assert_eq!(reqs[0].method, HttpMethod::GET);
    assert_eq!(reqs[1].method, HttpMethod::POST);
    assert!(matches!(reqs[1].body, Some(RequestBody::Json(_))));
    assert!(matches!(reqs[1].auth, Some(AuthConfig::Bearer { .. })));
}

#[test]
fn environment_roundtrip() {
    let mut env = hitt_core::core::environment::Environment::new("Test Env");
    env.add_variable("host", "api.example.com");
    env.add_variable("port", "8080");
    env.add_secret("api_key", "secret123");

    // Export
    let json = export_postman_environment(&env).unwrap();

    // Import back
    let imported = import_postman_environment(&json).unwrap();

    assert_eq!(imported.name, "Test Env");
    assert_eq!(imported.values.len(), 3);
    assert_eq!(imported.get("host"), Some("api.example.com"));
    assert_eq!(imported.get("port"), Some("8080"));

    // Secrets are exported as empty strings
    let secret_var = imported.values.iter().find(|v| v.key == "api_key").unwrap();
    assert!(secret_var.secret);
    assert_eq!(secret_var.value, ""); // secret values not exported
}

#[test]
fn fixture_import_export_import() {
    let content = include_str!("fixtures/sample_postman_collection.json");
    let first_import = import_postman_collection(content).unwrap();

    // Export
    let exported = export_postman_collection(&first_import).unwrap();

    // Import again
    let second_import = import_postman_collection(&exported).unwrap();

    // Structural equivalence
    assert_eq!(first_import.name, second_import.name);
    assert_eq!(first_import.request_count(), second_import.request_count());
    assert_eq!(first_import.variables.len(), second_import.variables.len());

    // Both have same description
    assert_eq!(first_import.description, second_import.description);

    // Query params survive the roundtrip without being duplicated in the URL
    let first_get = first_import.all_requests()[0];
    let second_get = second_import.all_requests()[0];
    assert_eq!(second_get.url, first_get.url);
    assert_eq!(second_get.url, "{{base_url}}/api/users");
    assert_eq!(second_get.params.len(), 1);
}

#[test]
fn import_strips_query_from_url_when_params_extracted() {
    let content = include_str!("fixtures/sample_postman_collection.json");
    let collection = import_postman_collection(content).unwrap();
    let get_users = collection.all_requests()[0];
    assert_eq!(get_users.url, "{{base_url}}/api/users");
    assert_eq!(get_users.params.len(), 1);
    assert_eq!(get_users.params[0].key, "page");
    assert_eq!(get_users.params[0].value, "1");
}

#[test]
fn import_keeps_query_in_string_url() {
    // A plain string URL has no `query` array, so the query must stay in the URL.
    let content = r#"{
        "info": {"name": "S", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"},
        "item": [{"name": "R", "request": {"method": "GET", "url": "{{host}}/x?a=1"}}]
    }"#;
    let collection = import_postman_collection(content).unwrap();
    let req = collection.all_requests()[0];
    assert_eq!(req.url, "{{host}}/x?a=1");
    assert!(req.params.is_empty());
}

#[test]
fn export_raw_url_includes_enabled_query_params() {
    let mut collection = Collection::new("Q");
    let mut disabled = KeyValuePair::new("debug", "true");
    disabled.enabled = false;
    let mut req =
        Request::new("Search", HttpMethod::GET, "{{base_url}}/search").with_param("q", "{{term}}");
    req.params.push(disabled);
    collection.add_request(req);

    let json = export_postman_collection(&collection).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let url = &value["item"][0]["request"]["url"];
    assert_eq!(url["raw"], "{{base_url}}/search?q={{term}}");
    assert_eq!(url["query"].as_array().unwrap().len(), 2);

    let imported = import_postman_collection(&json).unwrap();
    let req = imported.all_requests()[0];
    assert_eq!(req.url, "{{base_url}}/search");
    assert_eq!(req.params.len(), 2);
    assert!(!req.params[1].enabled);
}
