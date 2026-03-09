use anyhow::Result;
use std::path::PathBuf;

use crate::core::history::HistoryEntry;

pub struct HistoryDb {
    db: sled::Db,
}

impl HistoryDb {
    pub fn open(path: PathBuf) -> Result<Self> {
        let db = sled::open(path)?;
        Ok(Self { db })
    }

    pub fn insert(&self, entry: &HistoryEntry) -> Result<()> {
        let key = entry.id.to_string();
        let value = serde_json::to_vec(entry)?;
        self.db.insert(key.as_bytes(), value)?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<HistoryEntry>> {
        match self.db.get(id.as_bytes())? {
            Some(bytes) => {
                let entry: HistoryEntry = serde_json::from_slice(&bytes)?;
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    pub fn list(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        let mut entries: Vec<HistoryEntry> = self
            .db
            .iter()
            .filter_map(|item| {
                item.ok()
                    .and_then(|(_, v)| serde_json::from_slice(&v).ok())
            })
            .collect();

        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        entries.truncate(limit);
        Ok(entries)
    }

    pub fn clear(&self) -> Result<()> {
        self.db.clear()?;
        Ok(())
    }

    pub fn count(&self) -> usize {
        self.db.len()
    }
}
