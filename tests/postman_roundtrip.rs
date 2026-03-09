use hitt::core::auth::AuthConfig;
use hitt::core::collection::Collection;
use hitt::core::request::{HttpMethod, KeyValuePair, Request, RequestBody};
use hitt::postman::env_export::export_postman_environment;
use hitt::postman::env_import::import_postman_environment;
use hitt::postman::export::export_postman_collection;
use hitt::postman::import::import_postman_collection;

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
    let mut env = hitt::core::environment::Environment::new("Test Env");
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
}
