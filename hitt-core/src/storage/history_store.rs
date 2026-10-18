//! Persistence of the request history (`history.json`, next to the
//! collections directory like `environments/`).

use anyhow::{Context, Result};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

use super::collections_store::atomic_write;
use crate::core::constants::MAX_PERSISTED_HISTORY_BODY;
use crate::core::history::HistoryEntry;

/// Path of the history file for a given collections directory.
pub fn history_file(collections_dir: &Path) -> PathBuf {
    collections_dir
        .parent()
        .unwrap_or(collections_dir)
        .join("history.json")
}

/// Load persisted entries (newest first), keeping at most `limit`.
/// A missing file yields an empty history.
pub fn load_history(path: &Path, limit: usize) -> Result<Vec<HistoryEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut entries: Vec<HistoryEntry> =
        serde_json::from_str(&content).with_context(|| format!("parsing {}", path.display()))?;
    entries.truncate(limit);
    Ok(entries)
}

/// Atomically write `entries`. Response bodies larger than
/// `MAX_PERSISTED_HISTORY_BODY` are dropped from the file (not from memory).
pub fn save_history(path: &Path, entries: &[HistoryEntry]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let slim: Vec<Cow<'_, HistoryEntry>> = entries
        .iter()
        .map(|e| {
            if e.response_body
                .as_ref()
                .is_some_and(|b| b.len() > MAX_PERSISTED_HISTORY_BODY)
            {
                let mut owned = e.clone();
                owned.drop_large_body(MAX_PERSISTED_HISTORY_BODY);
                Cow::Owned(owned)
            } else {
                Cow::Borrowed(e)
            }
        })
        .collect();
    let content = serde_json::to_vec(&slim)?;
    atomic_write(path, &content)
}

/// Delete the history file (no-op if it does not exist).
pub fn clear_history(path: &Path) -> Result<()> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
