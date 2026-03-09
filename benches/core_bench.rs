use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::collections::HashMap;
use std::time::Duration;

use hitt::core::request::{HttpMethod, KeyValuePair, Request};
use hitt::core::response::{RequestTiming, Response, ResponseBody, ResponseSize};
use hitt::core::variables::VariableResolver;
use hitt::exporters::curl::to_curl;
use hitt::importers::curl::parse_curl;
use hitt::testing::assertion_engine::{Assertion, AssertionEngine, AssertionKind};

fn bench_variable_resolution(c: &mut Criterion) {
    let mut group = c.benchmark_group("variable_resolution");

    // Simple — no variables (fast path)
    let resolver = VariableResolver::new();
    group.bench_function("no_variables", |b| {
        b.iter(|| resolver.resolve(black_box("https://api.example.com/users?page=1")));
    });

    // Single variable
    let mut resolver = VariableResolver::new();
    let mut vars = HashMap::new();
    vars.insert("base_url".into(), "https://api.example.com".into());
    resolver.add_scope("env", vars);
    group.bench_function("single_variable", |b| {
        b.iter(|| resolver.resolve(black_box("{{base_url}}/users?page=1")));
    });

    // Many variables in scope, few used
    let mut resolver = VariableResolver::new();
    let mut vars = HashMap::new();
    for i in 0..50 {
        vars.insert(format!("var_{i}"), format!("value_{i}"));
    }
    resolver.add_scope("env", vars);
    group.bench_function("50_vars_3_used", |b| {
        b.iter(|| resolver.resolve(black_box("{{var_0}}/{{var_25}}/{{var_49}}")));
    });

    // Multi-scope chain (6 scopes like production)
    let mut resolver = VariableResolver::new();
    for scope in &["chain", "collection", "environment", "dotenv", "global"] {
        let mut vars = HashMap::new();
        for i in 0..20 {
            vars.insert(format!("{scope}_{i}"), format!("val_{scope}_{i}"));
        }
        resolver.add_scope(*scope, vars);
    }
    group.bench_function("6_scopes_deep_lookup", |b| {
        b.iter(|| resolver.resolve(black_box("{{global_19}}")));
    });

    // Resolve headers batch
    let headers: Vec<KeyValuePair> = (0..10)
        .map(|i| KeyValuePair::new(format!("X-Header-{i}"), format!("{{var_{i}}}")))
        .collect();
    let mut resolver = VariableResolver::new();
    let mut vars = HashMap::new();
    for i in 0..10 {
        vars.insert(format!("var_{i}"), format!("resolved_{i}"));
    }
    resolver.add_scope("env", vars);
    group.bench_function("resolve_10_headers", |b| {
        b.iter(|| resolver.resolve_headers(black_box(&headers)));
    });

    group.finish();
}

fn bench_curl_parse_export(c: &mut Criterion) {
    let mut group = c.benchmark_group("curl");

    let simple = "curl https://api.example.com/users";
    group.bench_function("parse_simple_get", |b| {
        b.iter(|| parse_curl(black_box(simple)));
    });

    let complex = r#"curl -X POST https://api.example.com/users \
        -H 'Content-Type: application/json' \
        -H 'Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.token' \
        -H 'X-Request-ID: abc-123' \
        -H 'Accept: application/json' \
        -d '{"name":"John","email":"john@example.com","role":"admin"}'"#;
    group.bench_function("parse_complex_post", |b| {
        b.iter(|| parse_curl(black_box(complex)));
    });

    let request = parse_curl(complex).unwrap();
    let resolver = VariableResolver::new();
    group.bench_function("export_to_curl", |b| {
        b.iter(|| to_curl(black_box(&request), black_box(&resolver)));
    });

    group.finish();
}

fn make_test_response() -> Response {
    Response {
        id: uuid::Uuid::new_v4(),
        status: 200,
        status_text: "OK".into(),
        headers: vec![
            KeyValuePair::new("content-type", "application/json"),
            KeyValuePair::new("x-request-id", "abc-123"),
        ],
        body: ResponseBody::Json(
            serde_json::json!({
                "users": [
                    {"id": 1, "name": "Alice", "email": "alice@example.com"},
                    {"id": 2, "name": "Bob", "email": "bob@example.com"},
                    {"id": 3, "name": "Charlie", "email": "charlie@example.com"}
                ],
                "total": 3,
                "page": 1
            })
            .to_string(),
        ),
        timing: RequestTiming {
            dns_lookup: Duration::from_millis(5),
            tcp_connect: Duration::from_millis(10),
            tls_handshake: Some(Duration::from_millis(15)),
            first_byte: Duration::from_millis(50),
            content_download: Duration::from_millis(20),
            total: Duration::from_millis(100),
        },
        size: ResponseSize {
            headers: 200,
            body: 256,
        },
        cookies: vec![],
        assertion_results: vec![],
        timestamp: chrono::Utc::now(),
    }
}

fn bench_assertions(c: &mut Criterion) {
    let mut group = c.benchmark_group("assertions");
    let response = make_test_response();

    // Single assertion
    let assertions = vec![Assertion::status_equals(200)];
    group.bench_function("single_status_check", |b| {
        b.iter(|| AssertionEngine::run_assertions(black_box(&assertions), black_box(&response)));
    });

    // Full assertion suite (mixed types)
    let assertions = vec![
        Assertion::status_equals(200),
        Assertion::status_range(200, 299),
        Assertion::new(AssertionKind::BodyContains("Alice".into())),
        Assertion::new(AssertionKind::BodyPathExists("$.users[0].name".into())),
        Assertion::new(AssertionKind::BodyPathEquals {
            path: "$.total".into(),
            expected: serde_json::json!(3),
        }),
        Assertion::new(AssertionKind::HeaderExists("content-type".into())),
        Assertion::new(AssertionKind::HeaderEquals {
            name: "x-request-id".into(),
            expected: "abc-123".into(),
        }),
        Assertion::new(AssertionKind::BodyPathType {
            path: "$.users".into(),
            expected: hitt::testing::assertion_engine::JsonType::Array,
        }),
    ];
    group.bench_function("8_mixed_assertions", |b| {
        b.iter(|| AssertionEngine::run_assertions(black_box(&assertions), black_box(&response)));
    });

    group.finish();
}

fn bench_collection_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("serialization");

    // Build a realistic collection
    let mut coll = hitt::core::collection::Collection::new("Benchmark API");
    for i in 0..50 {
        let mut req = Request::new(format!("Request {i}"), HttpMethod::GET, format!("https://api.example.com/endpoint/{i}"));
        for j in 0..5 {
            req.headers
                .push(KeyValuePair::new(format!("X-H-{j}"), format!("val-{j}")));
        }
        coll.add_request(req);
    }

    let json = serde_json::to_string(&coll).unwrap();

    group.bench_function("serialize_50_requests", |b| {
        b.iter(|| serde_json::to_string(black_box(&coll)).unwrap());
    });

    group.bench_function("deserialize_50_requests", |b| {
        b.iter(|| {
            serde_json::from_str::<hitt::core::collection::Collection>(black_box(&json)).unwrap()
        });
    });

    group.finish();
}

fn bench_openapi_import(c: &mut Criterion) {
    let spec = std::fs::read_to_string("tests/fixtures/petstore_openapi.yaml")
        .expect("petstore fixture must exist");

    c.bench_function("openapi_import_petstore", |b| {
        b.iter(|| hitt::importers::openapi::import_openapi(black_box(&spec)));
    });
}

criterion_group!(
    benches,
    bench_variable_resolution,
    bench_curl_parse_export,
    bench_assertions,
    bench_collection_serialization,
    bench_openapi_import,
);
criterion_main!(benches);
