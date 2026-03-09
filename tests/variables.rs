use std::collections::HashMap;

use hitt::core::environment::Environment;
use hitt::core::request::KeyValuePair;
use hitt::core::variables::VariableResolver;

#[test]
fn basic_interpolation() {
    let mut resolver = VariableResolver::new();
    let mut vars = HashMap::new();
    vars.insert("host".to_string(), "api.example.com".to_string());
    resolver.add_scope("env", vars);

    assert_eq!(
        resolver.resolve("https://{{host}}/api"),
        "https://api.example.com/api"
    );
}

#[test]
fn multi_variable_interpolation() {
    let mut resolver = VariableResolver::new();
    let mut vars = HashMap::new();
    vars.insert("scheme".to_string(), "https".to_string());
    vars.insert("host".to_string(), "api.example.com".to_string());
    vars.insert("version".to_string(), "v2".to_string());
    resolver.add_scope("env", vars);

    assert_eq!(
        resolver.resolve("{{scheme}}://{{host}}/{{version}}/users"),
        "https://api.example.com/v2/users"
    );
}

#[test]
fn unresolved_variable_preserved() {
    let resolver = VariableResolver::new();
    assert_eq!(resolver.resolve("{{unknown}}"), "{{unknown}}");
}

#[test]
fn no_variables_passthrough() {
    let resolver = VariableResolver::new();
    assert_eq!(
        resolver.resolve("https://example.com/path"),
        "https://example.com/path"
    );
}

#[test]
fn scope_priority() {
    let mut resolver = VariableResolver::new();

    let mut high = HashMap::new();
    high.insert("key".to_string(), "high_value".to_string());
    resolver.add_scope("collection", high);

    let mut low = HashMap::new();
    low.insert("key".to_string(), "low_value".to_string());
    resolver.add_scope("environment", low);

    assert_eq!(resolver.resolve("{{key}}"), "high_value");
}

#[test]
fn dynamic_guid() {
    let resolver = VariableResolver::new();
    let result = resolver.resolve("{{$guid}}");
    assert_ne!(result, "{{$guid}}");
    assert!(uuid::Uuid::parse_str(&result).is_ok());
}

#[test]
fn dynamic_timestamp() {
    let resolver = VariableResolver::new();
    let result = resolver.resolve("{{$timestamp}}");
    assert_ne!(result, "{{$timestamp}}");
    let ts: i64 = result.parse().expect("should be a number");
    assert!(ts > 1_000_000_000); // after ~2001
}

#[test]
fn dynamic_iso_timestamp() {
    let resolver = VariableResolver::new();
    let result = resolver.resolve("{{$isoTimestamp}}");
    assert_ne!(result, "{{$isoTimestamp}}");
    assert!(result.contains('T')); // ISO format contains T separator
}

#[test]
fn dynamic_random_int() {
    let resolver = VariableResolver::new();
    let result = resolver.resolve("{{$randomInt}}");
    let n: i32 = result.parse().expect("should be a number");
    assert!((0..1000).contains(&n));
}

#[test]
fn dynamic_random_email() {
    let resolver = VariableResolver::new();
    let result = resolver.resolve("{{$randomEmail}}");
    assert!(result.contains('@'));
    assert!(result.contains("example.com"));
}

#[test]
fn dynamic_random_full_name() {
    let resolver = VariableResolver::new();
    let result = resolver.resolve("{{$randomFullName}}");
    assert!(result.contains(' ')); // First Last
}

#[test]
fn dynamic_random_boolean() {
    let resolver = VariableResolver::new();
    let result = resolver.resolve("{{$randomBoolean}}");
    assert!(result == "true" || result == "false");
}

#[test]
fn from_context_integration() {
    let collection_vars = vec![
        KeyValuePair::new("base_url", "https://api.example.com"),
        KeyValuePair::new("shared", "from_collection"),
    ];

    let mut env = Environment::new("Test");
    env.add_variable("token", "env-token");
    env.add_variable("shared", "from_env");

    let mut dotenv = HashMap::new();
    dotenv.insert("db_host".to_string(), "localhost".to_string());

    let resolver = VariableResolver::from_context(
        None,
        &collection_vars,
        Some(&env),
        Some(&dotenv),
        None,
    );

    // Collection var
    assert_eq!(
        resolver.resolve("{{base_url}}"),
        "https://api.example.com"
    );
    // Env var
    assert_eq!(resolver.resolve("{{token}}"), "env-token");
    // Dotenv var
    assert_eq!(resolver.resolve("{{db_host}}"), "localhost");
    // Collection has higher priority than environment
    assert_eq!(resolver.resolve("{{shared}}"), "from_collection");
}

#[test]
fn resolve_headers() {
    let mut resolver = VariableResolver::new();
    let mut vars = HashMap::new();
    vars.insert("token".to_string(), "abc123".to_string());
    resolver.add_scope("env", vars);

    let headers = vec![
        KeyValuePair::new("Authorization", "Bearer {{token}}"),
        KeyValuePair::new("Accept", "application/json"),
    ];
    let resolved = resolver.resolve_headers(&headers);
    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved[0].value, "Bearer abc123");
    assert_eq!(resolved[1].value, "application/json");
}

#[test]
fn resolve_params() {
    let mut resolver = VariableResolver::new();
    let mut vars = HashMap::new();
    vars.insert("page_size".to_string(), "25".to_string());
    resolver.add_scope("env", vars);

    let params = vec![
        KeyValuePair::new("limit", "{{page_size}}"),
        KeyValuePair::new("offset", "0"),
    ];
    let resolved = resolver.resolve_params(&params);
    assert_eq!(resolved[0].value, "25");
    assert_eq!(resolved[1].value, "0");
}

#[test]
fn disabled_headers_filtered_in_resolve() {
    let resolver = VariableResolver::new();
    let headers = vec![
        KeyValuePair::new("Keep", "yes"),
        KeyValuePair::new("Remove", "no").disabled(),
    ];
    let resolved = resolver.resolve_headers(&headers);
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].key, "Keep");
}
