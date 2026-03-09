use crate::core::request::HttpMethod;
use super::capture::CapturedRequest;

#[derive(Debug, Clone, Default)]
pub struct ProxyFilter {
    pub host_pattern: Option<String>,
    pub method: Option<HttpMethod>,
    pub status_range: Option<(u16, u16)>,
    pub path_contains: Option<String>,
}

impl ProxyFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_host(mut self, pattern: impl Into<String>) -> Self {
        self.host_pattern = Some(pattern.into());
        self
    }

    pub fn with_method(mut self, method: HttpMethod) -> Self {
        self.method = Some(method);
        self
    }

    pub fn with_status_range(mut self, min: u16, max: u16) -> Self {
        self.status_range = Some((min, max));
        self
    }

    pub fn matches(&self, request: &CapturedRequest) -> bool {
        if let Some(ref host) = self.host_pattern {
            if !request.host.contains(host.as_str()) {
                return false;
            }
        }

        if let Some(method) = &self.method {
            if request.method != *method {
                return false;
            }
        }

        if let Some((min, max)) = self.status_range {
            if let Some(status) = request.status {
                if status < min || status > max {
                    return false;
                }
            } else {
                return false;
            }
        }

        if let Some(ref path) = self.path_contains {
            if !request.path.contains(path.as_str()) {
                return false;
            }
        }

        true
    }

    pub fn apply<'a>(&self, requests: &'a [CapturedRequest]) -> Vec<&'a CapturedRequest> {
        requests.iter().filter(|r| self.matches(r)).collect()
    }
}
