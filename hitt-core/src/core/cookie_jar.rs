use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CookieJar {
    pub cookies: Vec<StoredCookie>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub expires: Option<String>,
    pub http_only: bool,
    pub secure: bool,
}

impl CookieJar {
    pub fn new() -> Self {
        Self::default()
    }
}
