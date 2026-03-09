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

    pub fn set(&mut self, cookie: StoredCookie) {
        if let Some(existing) = self
            .cookies
            .iter_mut()
            .find(|c| c.name == cookie.name && c.domain == cookie.domain)
        {
            *existing = cookie;
        } else {
            self.cookies.push(cookie);
        }
    }

    pub fn get(&self, name: &str, domain: &str) -> Option<&StoredCookie> {
        self.cookies
            .iter()
            .find(|c| c.name == name && c.domain == domain)
    }

    pub fn get_for_domain(&self, domain: &str) -> Vec<&StoredCookie> {
        self.cookies
            .iter()
            .filter(|c| domain.ends_with(&c.domain))
            .collect()
    }

    pub fn remove(&mut self, name: &str, domain: &str) {
        self.cookies
            .retain(|c| !(c.name == name && c.domain == domain));
    }

    pub fn clear(&mut self) {
        self.cookies.clear();
    }
}
