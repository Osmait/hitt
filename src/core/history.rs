use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::request::HttpMethod;

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
        if url.len() <= max_len {
            url.to_string()
        } else {
            format!("{}...", &url[..max_len.saturating_sub(3)])
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
