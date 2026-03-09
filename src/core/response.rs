use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

use super::request::KeyValuePair;
use crate::testing::assertion_engine::AssertionResult;

#[derive(Debug, Clone)]
pub struct Response {
    pub id: Uuid,
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<KeyValuePair>,
    pub body: ResponseBody,
    pub cookies: Vec<Cookie>,
    pub timing: RequestTiming,
    pub size: ResponseSize,
    pub assertion_results: Vec<AssertionResult>,
    pub timestamp: DateTime<Utc>,
}

impl Response {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn is_redirect(&self) -> bool {
        (300..400).contains(&self.status)
    }

    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.status)
    }

    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.status)
    }

    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|h| h.key.eq_ignore_ascii_case(name))
            .map(|h| h.value.as_str())
    }

    pub fn content_type(&self) -> Option<&str> {
        self.header_value("content-type")
    }

    pub fn is_json(&self) -> bool {
        self.content_type()
            .map(|ct| ct.contains("json"))
            .unwrap_or(false)
    }

    pub fn body_text(&self) -> Option<&str> {
        match &self.body {
            ResponseBody::Text(s) => Some(s.as_str()),
            ResponseBody::Json(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn body_json(&self) -> Option<serde_json::Value> {
        self.body_text()
            .and_then(|s| serde_json::from_str(s).ok())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseBody {
    Text(String),
    Json(String),
    Xml(String),
    Html(String),
    Binary(Vec<u8>),
    Empty,
}

impl ResponseBody {
    pub fn from_content_type(body: String, content_type: Option<&str>) -> Self {
        match content_type {
            Some(ct) if ct.contains("json") => Self::Json(body),
            Some(ct) if ct.contains("xml") => Self::Xml(body),
            Some(ct) if ct.contains("html") => Self::Html(body),
            Some(ct) if ct.contains("octet-stream") || ct.contains("image") => {
                Self::Binary(body.into_bytes())
            }
            _ => Self::Text(body),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Text(s) | Self::Json(s) | Self::Xml(s) | Self::Html(s) => s.len(),
            Self::Binary(b) => b.len(),
            Self::Empty => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: Option<String>,
    pub path: Option<String>,
    pub expires: Option<String>,
    pub http_only: bool,
    pub secure: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RequestTiming {
    pub dns_lookup: Duration,
    pub tcp_connect: Duration,
    pub tls_handshake: Option<Duration>,
    pub first_byte: Duration,
    pub content_download: Duration,
    pub total: Duration,
}

impl RequestTiming {
    pub fn simple(total: Duration) -> Self {
        Self {
            total,
            ..Default::default()
        }
    }

    pub fn format_total(&self) -> String {
        let ms = self.total.as_millis();
        if ms < 1000 {
            format!("{}ms", ms)
        } else {
            format!("{:.1}s", self.total.as_secs_f64())
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResponseSize {
    pub headers: usize,
    pub body: usize,
}

impl ResponseSize {
    pub fn total(&self) -> usize {
        self.headers + self.body
    }

    pub fn format(&self) -> String {
        let total = self.total();
        if total < 1024 {
            format!("{} B", total)
        } else if total < 1024 * 1024 {
            format!("{:.1} KB", total as f64 / 1024.0)
        } else {
            format!("{:.1} MB", total as f64 / (1024.0 * 1024.0))
        }
    }
}
