use crate::core::auth::{ApiKeyLocation, AuthConfig};
use crate::core::collection::Collection;
use crate::core::helpers::append_query_params;
use crate::core::request::{HttpMethod, KeyValuePair, Request, RequestBody};
use crate::core::variables::VariableResolver;
use crate::testing::collection_runner::effective_auth_in;

/// Like [`to_curl`], but with the auth the request is really sent with inside
/// `collection` (its own, or inherited from its folder / the collection).
pub fn to_curl_in(
    collection: &Collection,
    request: &Request,
    resolver: &VariableResolver,
) -> String {
    let mut effective = request.clone();
    effective.auth = effective_auth_in(collection, request);
    to_curl(&effective, resolver)
}

pub fn to_curl(request: &Request, resolver: &VariableResolver) -> String {
    let mut parts = vec!["curl".to_string()];

    // Method (skip for GET as it's default)
    if request.method != HttpMethod::GET {
        parts.push(format!("-X {}", request.method));
    }

    // URL with resolved variables
    let url = resolver.resolve(&request.url);

    // Add query params (skipping any already present in the URL)
    let mut params = resolver.resolve_params(&request.params);
    if let Some(AuthConfig::ApiKey {
        key,
        value,
        location: ApiKeyLocation::QueryParam,
    }) = &request.auth
    {
        params.push(KeyValuePair::new(
            resolver.resolve(key),
            resolver.resolve(value),
        ));
    }
    let full_url = append_query_params(&url, &params, shell_escape);
    parts.push(format!("'{full_url}'"));

    // Headers
    let headers = resolver.resolve_headers(&request.headers);
    for header in &headers {
        parts.push(format!("-H '{}: {}'", header.key, header.value));
    }

    // Auth
    match &request.auth {
        Some(
            AuthConfig::Bearer { token }
            | AuthConfig::OAuth2 {
                token: Some(token), ..
            },
        ) => {
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
                ApiKeyLocation::Header => {
                    parts.push(format!("-H '{k}: {v}'"));
                }
                ApiKeyLocation::QueryParam => {
                    // Added to the URL's query above.
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
        Some(RequestBody::FormData(pairs)) => {
            // `key=@path` values are file parts, exactly as curl's own syntax.
            for pair in pairs.iter().filter(|p| p.enabled) {
                let k = resolver.resolve(&pair.key);
                let v = resolver.resolve(&pair.value);
                parts.push(format!("-F '{}'", shell_escape(&format!("{k}={v}"))));
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
