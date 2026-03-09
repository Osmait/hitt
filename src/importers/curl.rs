use anyhow::{bail, Result};

use crate::core::auth::AuthConfig;
use crate::core::request::{HttpMethod, KeyValuePair, Request, RequestBody};

pub fn parse_curl(input: &str) -> Result<Request> {
    let input = input.trim();
    let input = if input.starts_with("curl") {
        input.to_string()
    } else {
        // Maybe it's without the curl prefix
        format!("curl {}", input)
    };

    // Handle line continuations
    let input = input.replace("\\\n", " ").replace("\\\r\n", " ");

    let words = shell_words::split(&input)?;
    if words.is_empty() || words[0] != "curl" {
        bail!("Not a curl command");
    }

    let mut method = None;
    let mut url = None;
    let mut headers = Vec::new();
    let mut data = None;
    let mut auth = None;
    let mut form_data = Vec::new();
    let mut is_form = false;

    let mut i = 1;
    while i < words.len() {
        match words[i].as_str() {
            "-X" | "--request" => {
                i += 1;
                if i < words.len() {
                    method = HttpMethod::from_str(&words[i]);
                }
            }
            "-H" | "--header" => {
                i += 1;
                if i < words.len() {
                    if let Some((key, value)) = parse_header(&words[i]) {
                        headers.push(KeyValuePair::new(key, value));
                    }
                }
            }
            "-d" | "--data" | "--data-raw" | "--data-binary" => {
                i += 1;
                if i < words.len() {
                    data = Some(words[i].clone());
                }
            }
            "--data-urlencode" => {
                i += 1;
                if i < words.len() {
                    if data.is_some() {
                        let existing = data.take().unwrap();
                        data = Some(format!("{}&{}", existing, words[i]));
                    } else {
                        data = Some(words[i].clone());
                    }
                }
            }
            "-u" | "--user" => {
                i += 1;
                if i < words.len() {
                    let parts: Vec<&str> = words[i].splitn(2, ':').collect();
                    if parts.len() == 2 {
                        auth = Some(AuthConfig::basic(parts[0], parts[1]));
                    }
                }
            }
            "-F" | "--form" => {
                i += 1;
                if i < words.len() {
                    is_form = true;
                    if let Some((key, value)) = parse_form_field(&words[i]) {
                        form_data.push(KeyValuePair::new(key, value));
                    }
                }
            }
            "--url" => {
                i += 1;
                if i < words.len() {
                    url = Some(words[i].clone());
                }
            }
            "-b" | "--cookie" => {
                i += 1;
                if i < words.len() {
                    headers.push(KeyValuePair::new("Cookie", &words[i]));
                }
            }
            "-A" | "--user-agent" => {
                i += 1;
                if i < words.len() {
                    headers.push(KeyValuePair::new("User-Agent", &words[i]));
                }
            }
            "-e" | "--referer" => {
                i += 1;
                if i < words.len() {
                    headers.push(KeyValuePair::new("Referer", &words[i]));
                }
            }
            // Flags we recognize but skip
            "-k" | "--insecure" | "-L" | "--location" | "--compressed" | "-s" | "--silent"
            | "-S" | "--show-error" | "-v" | "--verbose" | "-i" | "--include" => {}
            // Skip flags with values we don't need
            "-o" | "--output" | "--connect-timeout" | "--max-time" | "-w" | "--write-out" => {
                i += 1; // skip the value
            }
            arg => {
                // If it looks like a URL and we don't have one yet
                if url.is_none()
                    && !arg.starts_with('-')
                    && (arg.starts_with("http://")
                        || arg.starts_with("https://")
                        || arg.contains('.'))
                {
                    url = Some(arg.to_string());
                }
            }
        }
        i += 1;
    }

    let url = url.unwrap_or_default();

    // Determine body
    let body = if is_form && !form_data.is_empty() {
        Some(RequestBody::FormData(form_data))
    } else if let Some(data) = data {
        // Try to detect if it's JSON
        let content_type = headers
            .iter()
            .find(|h| h.key.eq_ignore_ascii_case("content-type"))
            .map(|h| h.value.clone());

        match content_type.as_deref() {
            Some(ct) if ct.contains("json") => Some(RequestBody::Json(data)),
            Some(ct) if ct.contains("x-www-form-urlencoded") => {
                let pairs = parse_urlencoded(&data);
                Some(RequestBody::FormUrlEncoded(pairs))
            }
            _ => {
                // Auto-detect JSON
                if data.trim_start().starts_with('{') || data.trim_start().starts_with('[') {
                    Some(RequestBody::Json(data))
                } else {
                    Some(RequestBody::Raw {
                        content: data,
                        content_type: "text/plain".to_string(),
                    })
                }
            }
        }
    } else {
        None
    };

    // Default method based on whether we have data
    let method = method.unwrap_or(if body.is_some() {
        HttpMethod::POST
    } else {
        HttpMethod::GET
    });

    // Extract query params from URL
    let (clean_url, params) = extract_query_params(&url);

    let mut request = Request::new("Imported from cURL", method, clean_url);
    request.headers = headers;
    request.params = params;
    request.body = body;
    request.auth = auth;

    Ok(request)
}

fn parse_header(header: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = header.splitn(2, ':').collect();
    if parts.len() == 2 {
        Some((parts[0].trim().to_string(), parts[1].trim().to_string()))
    } else {
        None
    }
}

fn parse_form_field(field: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = field.splitn(2, '=').collect();
    if parts.len() == 2 {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    }
}

fn parse_urlencoded(data: &str) -> Vec<KeyValuePair> {
    data.split('&')
        .filter_map(|pair| {
            let parts: Vec<&str> = pair.splitn(2, '=').collect();
            if parts.len() == 2 {
                Some(KeyValuePair::new(
                    url::form_urlencoded::parse(parts[0].as_bytes())
                        .next()
                        .map(|(k, _)| k.to_string())
                        .unwrap_or_else(|| parts[0].to_string()),
                    url::form_urlencoded::parse(parts[1].as_bytes())
                        .next()
                        .map(|(_, v)| v.to_string())
                        .unwrap_or_else(|| parts[1].to_string()),
                ))
            } else {
                None
            }
        })
        .collect()
}

fn extract_query_params(url: &str) -> (String, Vec<KeyValuePair>) {
    if let Some(idx) = url.find('?') {
        let base = url[..idx].to_string();
        let query = &url[idx + 1..];
        let params = parse_urlencoded(query);
        (base, params)
    } else {
        (url.to_string(), Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_get() {
        let req = parse_curl("curl https://api.example.com/users").unwrap();
        assert_eq!(req.method, HttpMethod::GET);
        assert_eq!(req.url, "https://api.example.com/users");
    }

    #[test]
    fn test_post_with_data() {
        let req = parse_curl(
            r#"curl -X POST https://api.example.com/users -H "Content-Type: application/json" -d '{"name":"John"}'"#,
        )
        .unwrap();
        assert_eq!(req.method, HttpMethod::POST);
        assert!(matches!(req.body, Some(RequestBody::Json(_))));
    }

    #[test]
    fn test_basic_auth() {
        let req = parse_curl("curl -u admin:password https://api.example.com").unwrap();
        assert!(matches!(req.auth, Some(AuthConfig::Basic { .. })));
    }

    #[test]
    fn test_headers() {
        let req = parse_curl(
            r#"curl -H "Authorization: Bearer token123" -H "Accept: application/json" https://api.example.com"#,
        )
        .unwrap();
        assert_eq!(req.headers.len(), 2);
    }

    #[test]
    fn test_query_params() {
        let req =
            parse_curl("curl 'https://api.example.com/users?page=1&limit=10'").unwrap();
        assert_eq!(req.url, "https://api.example.com/users");
        assert_eq!(req.params.len(), 2);
    }
}
