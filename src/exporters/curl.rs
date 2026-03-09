use crate::core::auth::AuthConfig;
use crate::core::request::{HttpMethod, Request, RequestBody};
use crate::core::variables::VariableResolver;

pub fn to_curl(request: &Request, resolver: &VariableResolver) -> String {
    let mut parts = vec!["curl".to_string()];

    // Method (skip for GET as it's default)
    if request.method != HttpMethod::GET {
        parts.push(format!("-X {}", request.method));
    }

    // URL with resolved variables
    let url = resolver.resolve(&request.url);
    let mut full_url = url.clone();

    // Add query params
    let params = resolver.resolve_params(&request.params);
    if !params.is_empty() {
        let query: Vec<String> = params
            .iter()
            .map(|p| format!("{}={}", shell_escape(&p.key), shell_escape(&p.value)))
            .collect();
        if full_url.contains('?') {
            full_url = format!("{}&{}", full_url, query.join("&"));
        } else {
            full_url = format!("{}?{}", full_url, query.join("&"));
        }
    }
    parts.push(format!("'{full_url}'"));

    // Headers
    let headers = resolver.resolve_headers(&request.headers);
    for header in &headers {
        parts.push(format!("-H '{}: {}'", header.key, header.value));
    }

    // Auth
    match &request.auth {
        Some(AuthConfig::Bearer { token }) => {
            let resolved = resolver.resolve(token);
            parts.push(format!("-H 'Authorization: Bearer {resolved}'"));
        }
        Some(AuthConfig::Basic { username, password }) => {
            let user = resolver.resolve(username);
            let pass = resolver.resolve(password);
            parts.push(format!("-u '{user}:{pass}'"));
        }
        Some(AuthConfig::ApiKey {
            key,
            value,
            location,
        }) => {
            let k = resolver.resolve(key);
            let v = resolver.resolve(value);
            match location {
                crate::core::auth::ApiKeyLocation::Header => {
                    parts.push(format!("-H '{k}: {v}'"));
                }
                crate::core::auth::ApiKeyLocation::QueryParam => {
                    // Already added to URL
                }
            }
        }
        _ => {}
    }

    // Body
    match &request.body {
        Some(RequestBody::Json(json)) => {
            let resolved = resolver.resolve(json);
            parts.push(format!("-d '{}'", shell_escape(&resolved)));
            // Add content-type if not already in headers
            if !headers
                .iter()
                .any(|h| h.key.eq_ignore_ascii_case("content-type"))
            {
                parts.push("-H 'Content-Type: application/json'".to_string());
            }
        }
        Some(RequestBody::FormUrlEncoded(pairs)) => {
            for pair in pairs.iter().filter(|p| p.enabled) {
                let k = resolver.resolve(&pair.key);
                let v = resolver.resolve(&pair.value);
                parts.push(format!("--data-urlencode '{k}={v}'"));
            }
        }
        Some(RequestBody::Raw { content, .. }) => {
            let resolved = resolver.resolve(content);
            parts.push(format!("-d '{}'", shell_escape(&resolved)));
        }
        Some(RequestBody::GraphQL { query, variables }) => {
            let mut gql = serde_json::json!({ "query": resolver.resolve(query) });
            if let Some(vars) = variables {
                let resolved = resolver.resolve(vars);
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&resolved) {
                    gql["variables"] = v;
                }
            }
            parts.push(format!("-d '{}'", shell_escape(&gql.to_string())));
            if !headers
                .iter()
                .any(|h| h.key.eq_ignore_ascii_case("content-type"))
            {
                parts.push("-H 'Content-Type: application/json'".to_string());
            }
        }
        _ => {}
    }

    parts.join(" \\\n  ")
}

fn shell_escape(s: &str) -> String {
    s.replace('\'', "'\\''")
}
