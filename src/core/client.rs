use anyhow::Result;
use chrono::Utc;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::time::Instant;
use uuid::Uuid;

use super::auth::AuthConfig;
use super::request::{HttpMethod, KeyValuePair, Request, RequestBody};
use super::response::{Cookie, RequestTiming, Response, ResponseBody, ResponseSize};
use super::variables::VariableResolver;

#[derive(Clone)]
pub struct HttpClient {
    client: reqwest::Client,
}

impl HttpClient {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .user_agent(format!("hitt/{}", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { client })
    }

    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }

    pub async fn send(
        &self,
        request: &Request,
        resolver: &VariableResolver,
    ) -> Result<Response> {
        let url = resolver.resolve(&request.url);

        // Build query params
        let params = resolver.resolve_params(&request.params);
        let url_with_params = if params.is_empty() {
            url.clone()
        } else {
            let query: Vec<String> = params
                .iter()
                .map(|p| format!("{}={}", urlencoding(&p.key), urlencoding(&p.value)))
                .collect();
            if url.contains('?') {
                format!("{}&{}", url, query.join("&"))
            } else {
                format!("{}?{}", url, query.join("&"))
            }
        };

        // Build request
        let method = to_reqwest_method(&request.method);
        let mut req_builder = self.client.request(method, &url_with_params);

        // Apply headers
        let headers = resolver.resolve_headers(&request.headers);
        let mut header_map = HeaderMap::new();
        for h in &headers {
            if let (Ok(name), Ok(value)) = (
                HeaderName::from_bytes(h.key.as_bytes()),
                HeaderValue::from_str(&h.value),
            ) {
                header_map.insert(name, value);
            }
        }
        req_builder = req_builder.headers(header_map);

        // Apply auth
        req_builder = apply_auth(req_builder, &request.auth, resolver);

        // Apply body
        req_builder = apply_body(req_builder, &request.body, resolver);

        // Send and time the request
        let start = Instant::now();
        let resp = req_builder.send().await?;
        let total_time = start.elapsed();

        // Parse response
        let status = resp.status().as_u16();
        let status_text = resp
            .status()
            .canonical_reason()
            .unwrap_or("Unknown")
            .to_string();

        let resp_headers: Vec<KeyValuePair> = resp
            .headers()
            .iter()
            .map(|(k, v)| {
                KeyValuePair::new(
                    k.as_str().to_string(),
                    v.to_str().unwrap_or("").to_string(),
                )
            })
            .collect();

        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let cookies: Vec<Cookie> = resp
            .cookies()
            .map(|c| Cookie {
                name: c.name().to_string(),
                value: c.value().to_string(),
                domain: c.domain().map(|s| s.to_string()),
                path: c.path().map(|s| s.to_string()),
                expires: None,
                http_only: c.http_only(),
                secure: c.secure(),
            })
            .collect();

        let body_bytes = resp.bytes().await?;
        let headers_size: usize = resp_headers
            .iter()
            .map(|h| h.key.len() + h.value.len() + 4)
            .sum();
        let body_size = body_bytes.len();

        let body_text = String::from_utf8_lossy(&body_bytes).to_string();
        let body = ResponseBody::from_content_type(body_text, content_type.as_deref());

        Ok(Response {
            id: Uuid::new_v4(),
            status,
            status_text,
            headers: resp_headers,
            body,
            cookies,
            timing: RequestTiming::simple(total_time),
            size: ResponseSize {
                headers: headers_size,
                body: body_size,
            },
            assertion_results: Vec::new(),
            timestamp: Utc::now(),
        })
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new().expect("Failed to create HTTP client")
    }
}

fn to_reqwest_method(method: &HttpMethod) -> reqwest::Method {
    match method {
        HttpMethod::GET => reqwest::Method::GET,
        HttpMethod::POST => reqwest::Method::POST,
        HttpMethod::PUT => reqwest::Method::PUT,
        HttpMethod::PATCH => reqwest::Method::PATCH,
        HttpMethod::DELETE => reqwest::Method::DELETE,
        HttpMethod::HEAD => reqwest::Method::HEAD,
        HttpMethod::OPTIONS => reqwest::Method::OPTIONS,
        HttpMethod::TRACE => reqwest::Method::TRACE,
    }
}

fn apply_auth(
    mut builder: reqwest::RequestBuilder,
    auth: &Option<AuthConfig>,
    resolver: &VariableResolver,
) -> reqwest::RequestBuilder {
    match auth {
        Some(AuthConfig::Bearer { token }) => {
            let resolved = resolver.resolve(token);
            builder = builder.bearer_auth(resolved);
        }
        Some(AuthConfig::Basic { username, password }) => {
            let user = resolver.resolve(username);
            let pass = resolver.resolve(password);
            builder = builder.basic_auth(user, Some(pass));
        }
        Some(AuthConfig::ApiKey {
            key,
            value,
            location,
        }) => {
            let k = resolver.resolve(key);
            let v = resolver.resolve(value);
            match location {
                super::auth::ApiKeyLocation::Header => {
                    if let (Ok(name), Ok(val)) = (
                        HeaderName::from_bytes(k.as_bytes()),
                        HeaderValue::from_str(&v),
                    ) {
                        builder = builder.header(name, val);
                    }
                }
                super::auth::ApiKeyLocation::QueryParam => {
                    builder = builder.query(&[(k, v)]);
                }
            }
        }
        Some(AuthConfig::OAuth2 { token, .. }) => {
            if let Some(t) = token {
                let resolved = resolver.resolve(t);
                builder = builder.bearer_auth(resolved);
            }
        }
        _ => {}
    }
    builder
}

fn apply_body(
    builder: reqwest::RequestBuilder,
    body: &Option<RequestBody>,
    resolver: &VariableResolver,
) -> reqwest::RequestBuilder {
    match body {
        Some(RequestBody::Json(json)) => {
            let resolved = resolver.resolve(json);
            builder
                .header("content-type", "application/json")
                .body(resolved)
        }
        Some(RequestBody::FormUrlEncoded(pairs)) => {
            let resolved: Vec<(String, String)> = pairs
                .iter()
                .filter(|p| p.enabled)
                .map(|p| (resolver.resolve(&p.key), resolver.resolve(&p.value)))
                .collect();
            builder.form(&resolved)
        }
        Some(RequestBody::Raw {
            content,
            content_type,
        }) => {
            let resolved = resolver.resolve(content);
            builder.header("content-type", content_type.as_str()).body(resolved)
        }
        Some(RequestBody::GraphQL { query, variables }) => {
            let mut gql = serde_json::json!({ "query": resolver.resolve(query) });
            if let Some(vars) = variables {
                let resolved = resolver.resolve(vars);
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&resolved) {
                    gql["variables"] = v;
                }
            }
            builder
                .header("content-type", "application/json")
                .body(gql.to_string())
        }
        Some(RequestBody::None) | None => builder,
        Some(RequestBody::FormData(_)) => {
            // TODO: implement multipart form data
            builder
        }
        Some(RequestBody::Binary(_path)) => {
            // TODO: implement binary body
            builder
        }
        Some(RequestBody::Protobuf { .. }) => {
            // TODO: implement protobuf body
            builder
        }
    }
}

fn urlencoding(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}
