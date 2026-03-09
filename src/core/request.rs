use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

use super::auth::AuthConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: Uuid,
    pub name: String,
    pub protocol: Protocol,
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<KeyValuePair>,
    pub params: Vec<KeyValuePair>,
    pub auth: Option<AuthConfig>,
    pub body: Option<RequestBody>,
    pub assertions: Vec<crate::testing::assertion_engine::Assertion>,
    pub pre_request_script: Option<String>,
    pub test_script: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Request {
    pub fn new(name: impl Into<String>, method: HttpMethod, url: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            protocol: Protocol::Http,
            method,
            url: url.into(),
            headers: Vec::new(),
            params: Vec::new(),
            auth: None,
            body: None,
            assertions: Vec::new(),
            pre_request_script: None,
            test_script: None,
            description: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push(KeyValuePair::new(key, value));
        self
    }

    pub fn with_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.push(KeyValuePair::new(key, value));
        self
    }

    pub fn with_body(mut self, body: RequestBody) -> Self {
        self.body = Some(body);
        self
    }

    pub fn with_auth(mut self, auth: AuthConfig) -> Self {
        self.auth = Some(auth);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Protocol {
    Http,
    WebSocket,
    Sse,
    Grpc {
        proto_file: PathBuf,
        service: String,
        method: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    PATCH,
    DELETE,
    HEAD,
    OPTIONS,
    TRACE,
}

impl HttpMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GET => "GET",
            Self::POST => "POST",
            Self::PUT => "PUT",
            Self::PATCH => "PATCH",
            Self::DELETE => "DELETE",
            Self::HEAD => "HEAD",
            Self::OPTIONS => "OPTIONS",
            Self::TRACE => "TRACE",
        }
    }

    pub fn all() -> &'static [HttpMethod] {
        &[
            Self::GET,
            Self::POST,
            Self::PUT,
            Self::PATCH,
            Self::DELETE,
            Self::HEAD,
            Self::OPTIONS,
            Self::TRACE,
        ]
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "GET" => Some(Self::GET),
            "POST" => Some(Self::POST),
            "PUT" => Some(Self::PUT),
            "PATCH" => Some(Self::PATCH),
            "DELETE" => Some(Self::DELETE),
            "HEAD" => Some(Self::HEAD),
            "OPTIONS" => Some(Self::OPTIONS),
            "TRACE" => Some(Self::TRACE),
            _ => None,
        }
    }
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RequestBody {
    Json(String),
    FormData(Vec<KeyValuePair>),
    FormUrlEncoded(Vec<KeyValuePair>),
    Raw {
        content: String,
        content_type: String,
    },
    Binary(PathBuf),
    GraphQL {
        query: String,
        variables: Option<String>,
    },
    Protobuf {
        message: String,
    },
    None,
}

impl RequestBody {
    pub fn content_type(&self) -> Option<&str> {
        match self {
            Self::Json(_) => Some("application/json"),
            Self::FormUrlEncoded(_) => Some("application/x-www-form-urlencoded"),
            Self::FormData(_) => None, // multipart boundary set by client
            Self::Raw { content_type, .. } => Some(content_type.as_str()),
            Self::Binary(_) => Some("application/octet-stream"),
            Self::GraphQL { .. } => Some("application/json"),
            Self::Protobuf { .. } => Some("application/grpc"),
            Self::None => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyValuePair {
    pub key: String,
    pub value: String,
    pub enabled: bool,
    pub description: Option<String>,
}

impl KeyValuePair {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            enabled: true,
            description: None,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}
