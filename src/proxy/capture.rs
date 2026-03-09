use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::request::{HttpMethod, KeyValuePair, Request};

#[derive(Debug, Clone)]
pub struct CapturedRequest {
    pub id: Uuid,
    pub method: HttpMethod,
    pub url: String,
    pub host: String,
    pub path: String,
    pub headers: Vec<KeyValuePair>,
    pub body: Option<String>,
    pub status: Option<u16>,
    pub response_size: Option<usize>,
    pub duration_ms: Option<u64>,
    pub timestamp: DateTime<Utc>,
}

impl CapturedRequest {
    pub fn to_request(&self) -> Request {
        let mut req = Request::new(
            format!("{} {}", self.method, self.path),
            self.method,
            &self.url,
        );
        req.headers = self.headers.clone();
        if let Some(body) = &self.body {
            let content_type = self
                .headers
                .iter()
                .find(|h| h.key.eq_ignore_ascii_case("content-type"))
                .map(|h| h.value.as_str());

            if content_type.map(|ct| ct.contains("json")).unwrap_or(false) {
                req.body = Some(crate::core::request::RequestBody::Json(body.clone()));
            } else {
                req.body = Some(crate::core::request::RequestBody::Raw {
                    content: body.clone(),
                    content_type: content_type.unwrap_or("text/plain").to_string(),
                });
            }
        }
        req
    }
}

#[derive(Debug, Default)]
pub struct CaptureStore {
    pub requests: Vec<CapturedRequest>,
    pub selected: usize,
}

impl CaptureStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, request: CapturedRequest) {
        self.requests.push(request);
    }

    pub fn clear(&mut self) {
        self.requests.clear();
        self.selected = 0;
    }

    pub fn selected_request(&self) -> Option<&CapturedRequest> {
        self.requests.get(self.selected)
    }

    pub fn len(&self) -> usize {
        self.requests.len()
    }

    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }
}
