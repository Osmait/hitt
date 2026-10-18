use anyhow::Result;
use chrono::Utc;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::cell::RefCell;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

use super::auth::AuthConfig;
use super::constants::DEFAULT_TIMEOUT_MS;
use super::helpers::append_query_params;
use super::request::{HttpMethod, KeyValuePair, Request, RequestBody};
use super::response::{Cookie, RequestTiming, Response, ResponseBody, ResponseSize};
use super::variables::VariableResolver;
use crate::storage::config::AppConfig;

/// Maximum number of redirects followed when `follow_redirects` is enabled.
const MAX_REDIRECTS: usize = 10;

#[derive(Clone)]
pub struct HttpClient {
    client: reqwest::Client,
}

tokio::task_local! {
    /// `Set-Cookie` headers received while the current `HttpClient::send` is
    /// in flight, one entry per hop of the redirect chain (final hop last).
    static RECEIVED_SET_COOKIES: RefCell<Vec<String>>;
}

/// The client's cookie store: reqwest's default [`reqwest::cookie::Jar`]
/// (same persistence behaviour as `cookie_store(true)`), additionally
/// recording every `Set-Cookie` header for the request that is being sent.
/// reqwest only exposes the final response's headers, so without this the
/// cookies set by intermediate redirect responses would be invisible.
#[derive(Default)]
struct RecordingCookieStore {
    jar: reqwest::cookie::Jar,
}

impl reqwest::cookie::CookieStore for RecordingCookieStore {
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &HeaderValue>, url: &url::Url) {
        let headers: Vec<HeaderValue> = cookie_headers.cloned().collect();
        // Outside of `send` (no task-local in scope) there is nothing to record.
        let _ = RECEIVED_SET_COOKIES.try_with(|received| {
            received.borrow_mut().extend(
                headers
                    .iter()
                    .filter_map(|h| h.to_str().ok())
                    .map(str::to_string),
            );
        });
        self.jar.set_cookies(&mut headers.iter(), url);
    }

    fn cookies(&self, url: &url::Url) -> Option<HeaderValue> {
        self.jar.cookies(url)
    }
}

impl HttpClient {
    /// Client with default settings (30s timeout, follow redirects, verify SSL).
    pub fn new() -> Result<Self> {
        Self::with_options(DEFAULT_TIMEOUT_MS, true, true)
    }

    /// Client built from the user's `timeout_ms`, `follow_redirects` and
    /// `verify_ssl` settings.
    pub fn from_config(config: &AppConfig) -> Result<Self> {
        Self::with_options(
            config.timeout_ms,
            config.follow_redirects,
            config.verify_ssl,
        )
    }

    /// Client with explicit options. A `timeout_ms` of 0 disables the timeout.
    pub fn with_options(timeout_ms: u64, follow_redirects: bool, verify_ssl: bool) -> Result<Self> {
        let redirect_policy = if follow_redirects {
            reqwest::redirect::Policy::limited(MAX_REDIRECTS)
        } else {
            reqwest::redirect::Policy::none()
        };
        let mut builder = reqwest::Client::builder()
            .cookie_provider(Arc::new(RecordingCookieStore::default()))
            .user_agent(user_agent())
            .redirect(redirect_policy)
            .danger_accept_invalid_certs(!verify_ssl);
        if timeout_ms > 0 {
            builder = builder.timeout(Duration::from_millis(timeout_ms));
        }
        Ok(Self {
            client: builder.build()?,
        })
    }

    pub async fn send(&self, request: &Request, resolver: &VariableResolver) -> Result<Response> {
        RECEIVED_SET_COOKIES
            .scope(RefCell::new(Vec::new()), self.send_inner(request, resolver))
            .await
    }

    async fn send_inner(&self, request: &Request, resolver: &VariableResolver) -> Result<Response> {
        let url = resolver.resolve(&request.url);

        // Build query params
        let params = resolver.resolve_params(&request.params);
        let url_with_params = append_query_params(&url, &params, urlencoding);

        // Build request
        let method = to_reqwest_method(request.method);
        let mut req_builder = self.client.request(method, &url_with_params);

        // Apply headers
        let headers = resolver.resolve_headers(&request.headers);
        let mut header_map = HeaderMap::new();
        for h in &headers {
            let name = HeaderName::from_bytes(h.key.as_bytes())
                .map_err(|_| anyhow::anyhow!("invalid header name '{}'", h.key))?;
            let value = HeaderValue::from_str(&h.value)
                .map_err(|_| anyhow::anyhow!("invalid header value for '{}'", h.key))?;
            // `append`, not `insert`: repeated header names are all sent.
            header_map.append(name, value);
        }
        let explicit_content_type = header_map.contains_key(reqwest::header::CONTENT_TYPE);
        req_builder = req_builder.headers(header_map);

        // Apply auth
        req_builder = apply_auth(req_builder, request.auth.as_ref(), resolver);

        // Apply body
        req_builder = apply_body(
            req_builder,
            request.body.as_ref(),
            resolver,
            explicit_content_type,
        )?;

        // Send and time the request
        let start = Instant::now();
        let resp = req_builder.send().await?;
        // `send()` resolves once the response headers have been received.
        let first_byte = start.elapsed();

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
                KeyValuePair::new(k.as_str().to_string(), v.to_str().unwrap_or("").to_string())
            })
            .collect();

        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(std::string::ToString::to_string);

        let cookies = RECEIVED_SET_COOKIES
            .try_with(|received| collect_cookies(&received.borrow()))
            .unwrap_or_default();

        let download_start = Instant::now();
        let body_bytes = resp.bytes().await?;
        let content_download = download_start.elapsed();
        let total_time = start.elapsed();
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
            timing: RequestTiming::measured(first_byte, content_download, total_time),
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

/// The `User-Agent` sent by hitt's HTTP clients: `hitt/<version>`.
pub fn user_agent() -> String {
    format!("hitt/{}", env!("CARGO_PKG_VERSION"))
}

fn to_reqwest_method(method: HttpMethod) -> reqwest::Method {
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

#[allow(clippy::similar_names)]
fn apply_auth(
    mut builder: reqwest::RequestBuilder,
    auth: Option<&AuthConfig>,
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
        Some(AuthConfig::OAuth2 { token: Some(t), .. }) => {
            let resolved = resolver.resolve(t);
            builder = builder.bearer_auth(resolved);
        }
        Some(AuthConfig::OAuth2 { token: None, .. } | AuthConfig::Inherit | AuthConfig::None)
        | None => {}
    }
    builder
}

/// Cookies set along the whole redirect chain, parsed from the raw
/// `Set-Cookie` headers in the order received. A cookie set again later in
/// the chain (same name, domain and path) replaces the earlier one.
fn collect_cookies(set_cookie_headers: &[String]) -> Vec<Cookie> {
    let mut cookies: Vec<Cookie> = Vec::new();
    for cookie in set_cookie_headers
        .iter()
        .filter_map(|h| parse_set_cookie(h))
    {
        cookies.retain(|c| {
            !(c.name == cookie.name && c.domain == cookie.domain && c.path == cookie.path)
        });
        cookies.push(cookie);
    }
    cookies
}

/// Parse one `Set-Cookie` header value (`name=value; Attr=...; Flag`).
fn parse_set_cookie(header: &str) -> Option<Cookie> {
    let mut parts = header.split(';');
    let (name, value) = parts.next()?.split_once('=')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let mut cookie = Cookie {
        name: name.to_string(),
        value: value.trim().trim_matches('"').to_string(),
        domain: None,
        path: None,
        expires: None,
        http_only: false,
        secure: false,
    };
    for attr in parts {
        let (key, val) = match attr.split_once('=') {
            Some((k, v)) => (k.trim(), Some(v.trim())),
            None => (attr.trim(), None),
        };
        match (key.to_ascii_lowercase().as_str(), val) {
            ("domain", Some(v)) if !v.is_empty() => {
                cookie.domain = Some(v.trim_start_matches('.').to_string());
            }
            ("path", Some(v)) if !v.is_empty() => cookie.path = Some(v.to_string()),
            ("expires", Some(v)) if !v.is_empty() => cookie.expires = Some(v.to_string()),
            ("httponly", _) => cookie.http_only = true,
            ("secure", _) => cookie.secure = true,
            _ => {}
        }
    }
    Some(cookie)
}

/// Apply the request body. `explicit_content_type` is true when the request
/// already carries an enabled `Content-Type` header, which then wins over the
/// type implied by the body.
#[allow(clippy::similar_names)]
fn apply_body(
    builder: reqwest::RequestBuilder,
    body: Option<&RequestBody>,
    resolver: &VariableResolver,
    explicit_content_type: bool,
) -> Result<reqwest::RequestBuilder> {
    let with_content_type = |builder: reqwest::RequestBuilder, content_type: &str| {
        if explicit_content_type {
            builder
        } else {
            builder.header("content-type", content_type)
        }
    };
    Ok(match body {
        Some(RequestBody::Json(json)) => {
            let resolved = resolver.resolve(json);
            with_content_type(builder, "application/json").body(resolved)
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
            with_content_type(builder, content_type).body(resolved)
        }
        Some(RequestBody::GraphQL { query, variables }) => {
            let mut gql = serde_json::json!({ "query": resolver.resolve(query) });
            if let Some(vars) = variables {
                let resolved = resolver.resolve(vars);
                // Blank variables mean "none"; anything else must be valid JSON.
                if !resolved.trim().is_empty() {
                    let v = serde_json::from_str::<serde_json::Value>(&resolved).map_err(|e| {
                        anyhow::anyhow!("GraphQL variables are not valid JSON: {e}")
                    })?;
                    gql["variables"] = v;
                }
            }
            with_content_type(builder, "application/json").body(gql.to_string())
        }
        Some(RequestBody::None) | None => builder,
        // reqwest sets the multipart Content-Type (with its boundary) itself.
        Some(RequestBody::FormData(pairs)) => builder.multipart(build_multipart(pairs, resolver)?),
        Some(RequestBody::Binary(_path)) => {
            // TODO: implement binary body
            builder
        }
        Some(RequestBody::Protobuf { .. }) => {
            // TODO: implement protobuf body
            builder
        }
    })
}

/// Build a multipart form from the enabled rows of a form-data body. Keys and
/// values have `{{variables}}` resolved; a value starting with `@` (curl's
/// `-F key=@path` syntax) is sent as a file part read from disk.
fn build_multipart(
    pairs: &[KeyValuePair],
    resolver: &VariableResolver,
) -> Result<reqwest::multipart::Form> {
    let mut form = reqwest::multipart::Form::new();
    for pair in pairs.iter().filter(|p| p.enabled) {
        let key = resolver.resolve(&pair.key);
        let value = resolver.resolve(&pair.value);
        match form_file_path(&value) {
            Some(path) => {
                let bytes = std::fs::read(path).map_err(|e| {
                    anyhow::anyhow!("form-data field '{key}': cannot read file '{path}': {e}")
                })?;
                let file_name = std::path::Path::new(path)
                    .file_name()
                    .map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned());
                let part = reqwest::multipart::Part::bytes(bytes)
                    .file_name(file_name)
                    .mime_str("application/octet-stream")?;
                form = form.part(key, part);
            }
            None => form = form.text(key, value),
        }
    }
    Ok(form)
}

/// The file path of a form-data value written as `@path`, if any.
pub fn form_file_path(value: &str) -> Option<&str> {
    value
        .strip_prefix('@')
        .map(str::trim)
        .filter(|p| !p.is_empty())
}

fn urlencoding(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}
