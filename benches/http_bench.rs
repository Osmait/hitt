use criterion::{criterion_group, criterion_main, Criterion};
use std::collections::HashMap;

use hitt::core::client::HttpClient;
use hitt::core::request::{HttpMethod, KeyValuePair, Request, RequestBody};
use hitt::core::variables::VariableResolver;

fn bench_http_requests(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let client = HttpClient::new().unwrap();

    let mut group = c.benchmark_group("http_requests");
    group.sample_size(20);
    group.measurement_time(std::time::Duration::from_secs(15));

    // GET request
    let request = Request::new("bench-get", HttpMethod::GET, "https://httpbin.org/get");
    let resolver = VariableResolver::new();

    group.bench_function("GET_httpbin", |b| {
        b.to_async(&rt)
            .iter(|| client.send(&request, &resolver));
    });

    // POST with JSON body
    let mut post_request = Request::new("bench-post", HttpMethod::POST, "https://httpbin.org/post");
    post_request.body = Some(RequestBody::Json(
        r#"{"name":"bench","value":42}"#.into(),
    ));

    group.bench_function("POST_json_httpbin", |b| {
        b.to_async(&rt)
            .iter(|| client.send(&post_request, &resolver));
    });

    // GET with variable resolution
    let mut var_request = Request::new("bench-vars", HttpMethod::GET, "{{base_url}}/get");
    var_request
        .headers
        .push(KeyValuePair::new("Authorization", "Bearer {{token}}"));
    let mut resolver = VariableResolver::new();
    let mut vars = HashMap::new();
    vars.insert("base_url".into(), "https://httpbin.org".into());
    vars.insert("token".into(), "bench-token-123".into());
    resolver.add_scope("env", vars);

    group.bench_function("GET_with_variables", |b| {
        b.to_async(&rt)
            .iter(|| client.send(&var_request, &resolver));
    });

    group.finish();
}

criterion_group!(benches, bench_http_requests);
criterion_main!(benches);
