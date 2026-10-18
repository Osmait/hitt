use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::request::{HttpMethod, KeyValuePair, Request, RequestBody};
use super::response::{RequestTiming, Response, ResponseBody, ResponseSize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: Uuid,
    pub method: HttpMethod,
    pub url: String,
    pub status: Option<u16>,
    pub duration_ms: Option<u64>,
    pub size_bytes: Option<usize>,
    pub timestamp: DateTime<Utc>,
    pub collection_id: Option<Uuid>,
    pub request_id: Option<Uuid>,
    pub response_body: Option<String>,
    pub request_body: Option<String>,
    /// Snapshot of the request as it was sent (unresolved: `{{vars}}` are
    /// kept), so it can be reopened or replayed faithfully. `None` for
    /// entries recorded before snapshots existed.
    #[serde(default)]
    pub request: Option<Request>,
    /// Response headers, used to rebuild the response when reopening.
    #[serde(default)]
    pub response_headers: Vec<KeyValuePair>,
}

impl HistoryEntry {
    pub fn new(method: HttpMethod, url: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            method,
            url: url.into(),
            status: None,
            duration_ms: None,
            size_bytes: None,
            timestamp: Utc::now(),
            collection_id: None,
            request_id: None,
            response_body: None,
            request_body: None,
            request: None,
            response_headers: Vec::new(),
        }
    }

    /// Build an entry from a finished exchange, keeping a snapshot of the
    /// (unresolved) request and the response's text body and headers.
    pub fn from_exchange(request: &Request, response: &Response) -> Self {
        let mut entry = Self::new(request.method, request.url.clone());
        entry.status = Some(response.status);
        entry.duration_ms =
            Some(u64::try_from(response.timing.total.as_millis()).unwrap_or(u64::MAX));
        entry.size_bytes = Some(response.size.total());
        entry.timestamp = response.timestamp;
        entry.request_id = Some(request.id);
        entry.response_body = match &response.body {
            ResponseBody::Text(s)
            | ResponseBody::Json(s)
            | ResponseBody::Xml(s)
            | ResponseBody::Html(s) => Some(s.clone()),
            ResponseBody::Binary(_) | ResponseBody::Empty => None,
        };
        entry.request_body = request.body.as_ref().and_then(|b| match b {
            RequestBody::Json(content) | RequestBody::Raw { content, .. } => Some(content.clone()),
            _ => None,
        });
        entry.response_headers.clone_from(&response.headers);
        entry.request = Some(request.clone());
        entry
    }

    /// The request to reopen / replay: the stored snapshot (with a fresh id so
    /// it never collides with the collection request it came from), or a bare
    /// method + URL request for entries without a snapshot.
    pub fn to_request(&self) -> Request {
        if let Some(snapshot) = &self.request {
            let mut request = snapshot.clone();
            request.id = Uuid::new_v4();
            return request;
        }
        Request::new(
            format!("{} {}", self.method, self.short_url(40)),
            self.method,
            &self.url,
        )
    }

    /// Rebuild the stored response (status, headers, text body, timing) so it
    /// can be shown without re-sending. `None` if no status was recorded.
    pub fn to_response(&self) -> Option<Response> {
        let status = self.status?;
        let content_type = self
            .response_headers
            .iter()
            .find(|h| h.key.eq_ignore_ascii_case("content-type"))
            .map(|h| h.value.as_str());
        let body = match &self.response_body {
            Some(text) if !text.is_empty() => {
                ResponseBody::from_content_type(text.clone(), content_type)
            }
            _ => ResponseBody::Empty,
        };
        let body_len = body.len();
        let total = self.size_bytes.unwrap_or(body_len);
        Some(Response {
            id: self.id,
            status,
            status_text: reqwest::StatusCode::from_u16(status)
                .ok()
                .and_then(|s| s.canonical_reason())
                .unwrap_or("")
                .to_string(),
            headers: self.response_headers.clone(),
            body,
            cookies: Vec::new(),
            timing: RequestTiming::simple(std::time::Duration::from_millis(
                self.duration_ms.unwrap_or(0),
            )),
            size: ResponseSize {
                headers: total.saturating_sub(body_len),
                body: body_len,
            },
            assertion_results: Vec::new(),
            timestamp: self.timestamp,
        })
    }

    /// Drop the response body if it is larger than `max_bytes` (used before
    /// persisting so the history file stays small).
    pub fn drop_large_body(&mut self, max_bytes: usize) {
        if self
            .response_body
            .as_ref()
            .is_some_and(|b| b.len() > max_bytes)
        {
            self.response_body = None;
        }
    }

    pub fn display_url(&self) -> &str {
        // Strip protocol and show path
        self.url
            .strip_prefix("https://")
            .or_else(|| self.url.strip_prefix("http://"))
            .unwrap_or(&self.url)
    }

    pub fn short_url(&self, max_len: usize) -> String {
        let url = self.display_url();
        if url.chars().count() <= max_len {
            url.to_string()
        } else {
            // Char-based so multi-byte URLs never split inside a character.
            let cut: String = url.chars().take(max_len.saturating_sub(3)).collect();
            format!("{cut}...")
        }
    }
}

#[derive(Debug, Default)]
pub struct HistoryStore {
    entries: Vec<HistoryEntry>,
    max_entries: usize,
}

impl HistoryStore {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_entries,
        }
    }

    /// A store pre-filled with `entries` (newest first), trimmed to `max_entries`.
    pub fn with_entries(max_entries: usize, mut entries: Vec<HistoryEntry>) -> Self {
        entries.truncate(max_entries);
        Self {
            entries,
            max_entries,
        }
    }

    pub fn max_entries(&self) -> usize {
        self.max_entries
    }

    /// Change the limit, dropping the oldest entries if over it.
    pub fn set_max_entries(&mut self, max_entries: usize) {
        self.max_entries = max_entries;
        self.entries.truncate(max_entries);
    }

    pub fn add(&mut self, entry: HistoryEntry) {
        self.entries.insert(0, entry);
        if self.entries.len() > self.max_entries {
            self.entries.truncate(self.max_entries);
        }
    }

    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    pub fn search(&self, query: &str) -> Vec<&HistoryEntry> {
        let query = query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| {
                e.url.to_lowercase().contains(&query)
                    || e.method.as_str().to_lowercase().contains(&query)
            })
            .collect()
    }

    pub fn get(&self, id: &Uuid) -> Option<&HistoryEntry> {
        self.entries.iter().find(|e| e.id == *id)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
