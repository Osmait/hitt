use anyhow::Result;
use serde::Deserialize;

use crate::core::collection::Collection;
use crate::core::request::{HttpMethod, KeyValuePair, Request, RequestBody};

#[derive(Debug, Deserialize)]
struct HarFile {
    log: HarLog,
}

#[derive(Debug, Deserialize)]
struct HarLog {
    entries: Vec<HarEntry>,
}

#[derive(Debug, Deserialize)]
struct HarEntry {
    request: HarRequest,
    #[serde(default)]
    response: Option<HarResponse>,
}

#[derive(Debug, Deserialize)]
struct HarRequest {
    method: String,
    url: String,
    headers: Vec<HarNameValue>,
    #[serde(default)]
    #[serde(rename = "queryString")]
    query_string: Vec<HarNameValue>,
    #[serde(rename = "postData")]
    post_data: Option<HarPostData>,
}

#[derive(Debug, Deserialize)]
struct HarResponse {
    status: u16,
}

#[derive(Debug, Deserialize)]
struct HarNameValue {
    name: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct HarPostData {
    #[serde(rename = "mimeType")]
    mime_type: String,
    text: Option<String>,
    #[serde(default)]
    params: Vec<HarNameValue>,
}

pub fn import_har(content: &str) -> Result<Collection> {
    let har: HarFile = serde_json::from_str(content)?;
    let mut collection = Collection::new("HAR Import");

    for entry in har.log.entries {
        let method = HttpMethod::from_str(&entry.request.method).unwrap_or(HttpMethod::GET);
        let url_parsed = url::Url::parse(&entry.request.url)?;
        let name = format!(
            "{} {}",
            entry.request.method,
            url_parsed.path()
        );

        let base_url = format!(
            "{}://{}{}",
            url_parsed.scheme(),
            url_parsed.host_str().unwrap_or(""),
            url_parsed.path()
        );

        let mut request = Request::new(name, method, base_url);

        // Headers (skip pseudo-headers and cookies)
        request.headers = entry
            .request
            .headers
            .into_iter()
            .filter(|h| {
                !h.name.starts_with(':')
                    && !h.name.eq_ignore_ascii_case("cookie")
                    && !h.name.eq_ignore_ascii_case("host")
            })
            .map(|h| KeyValuePair::new(h.name, h.value))
            .collect();

        // Query params
        request.params = entry
            .request
            .query_string
            .into_iter()
            .map(|q| KeyValuePair::new(q.name, q.value))
            .collect();

        // Body
        if let Some(post_data) = entry.request.post_data {
            if !post_data.params.is_empty() {
                request.body = Some(RequestBody::FormUrlEncoded(
                    post_data
                        .params
                        .into_iter()
                        .map(|p| KeyValuePair::new(p.name, p.value))
                        .collect(),
                ));
            } else if let Some(text) = post_data.text {
                if post_data.mime_type.contains("json") {
                    request.body = Some(RequestBody::Json(text));
                } else {
                    request.body = Some(RequestBody::Raw {
                        content: text,
                        content_type: post_data.mime_type,
                    });
                }
            }
        }

        collection.add_request(request);
    }

    Ok(collection)
}
