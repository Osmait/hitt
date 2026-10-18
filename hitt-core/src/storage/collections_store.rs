use anyhow::Result;
use serde::Deserialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::core::collection::Collection;
use crate::core::environment::Environment;

/// Write to a temp file in the same directory, then atomically rename.
/// This prevents data corruption if the process crashes mid-write.
pub(crate) fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(content)?;
    tmp.flush()?;
    tmp.persist(path)?;
    Ok(())
}

pub struct CollectionsStore {
    dir: PathBuf,
}

impl CollectionsStore {
    pub fn new(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    pub fn load_all(&self) -> Result<Vec<Collection>> {
        let mut collections = Vec::new();
        if self.dir.exists() {
            for entry in std::fs::read_dir(&self.dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "json") {
                    if let Ok(collection) = self.load_collection(&path) {
                        collections.push(collection);
                    }
                }
            }
        }
        Ok(collections)
    }

    pub fn load_collection(&self, path: &Path) -> Result<Collection> {
        let content = std::fs::read_to_string(path)?;
        let collection: Collection = serde_json::from_str(&content)?;
        Ok(collection)
    }

    /// Save to `<Name>.json`, or `<Name>_2.json`, `<Name>_3.json`... when the
    /// file is owned by a different collection (another id), so two
    /// collections with the same name never overwrite each other.
    pub fn save_collection(&self, collection: &Collection) -> Result<PathBuf> {
        let content = serde_json::to_string_pretty(collection)?;
        save_owned(
            &self.dir,
            &collection.name,
            collection.id,
            content.as_bytes(),
        )
    }

    /// Delete the file(s) owned by this collection's id; files of other
    /// collections with the same name are left untouched.
    pub fn delete_collection(&self, collection: &Collection) -> Result<()> {
        delete_owned(&self.dir, collection.id)
    }

    // Environment files
    pub fn environments_dir(&self) -> PathBuf {
        self.dir.parent().unwrap_or(&self.dir).join("environments")
    }

    pub fn load_environments(&self) -> Result<Vec<Environment>> {
        self.load_all_environments()
    }

    /// Load every environment file, sorted by name so that index-based
    /// selection (e.g. the TUI's active environment) is stable across runs.
    /// Unreadable or malformed files are skipped.
    pub fn load_all_environments(&self) -> Result<Vec<Environment>> {
        let dir = self.environments_dir();
        let mut environments = Vec::new();
        if dir.exists() {
            for entry in std::fs::read_dir(&dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "json") {
                    if let Ok(env) = self.load_environment(&path) {
                        environments.push(env);
                    }
                }
            }
        }
        environments.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(environments)
    }

    pub fn load_environment(&self, path: &Path) -> Result<Environment> {
        let content = std::fs::read_to_string(path)?;
        let env: Environment = serde_json::from_str(&content)?;
        Ok(env)
    }

    /// Collision-safe like [`Self::save_collection`].
    pub fn save_environment(&self, env: &Environment) -> Result<PathBuf> {
        let dir = self.environments_dir();
        std::fs::create_dir_all(&dir)?;
        let content = serde_json::to_string_pretty(env)?;
        save_owned(&dir, &env.name, env.id, content.as_bytes())
    }

    /// Delete only the file(s) owned by this environment's id.
    pub fn delete_environment(&self, env: &Environment) -> Result<()> {
        delete_owned(&self.environments_dir(), env.id)
    }
}

/// The `id` field every stored collection / environment file has.
#[derive(Deserialize)]
struct FileOwner {
    id: Uuid,
}

/// Id stored in a JSON file; `None` if it's missing, unreadable or malformed.
fn file_owner(path: &Path) -> Option<Uuid> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<FileOwner>(&content)
        .ok()
        .map(|o| o.id)
}

/// Every `.json` file in `dir` whose stored id is `id` (none if `dir` is missing).
fn files_owned_by(dir: &Path, id: Uuid) -> Result<Vec<PathBuf>> {
    let mut owned = Vec::new();
    if !dir.exists() {
        return Ok(owned);
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "json") && file_owner(&path) == Some(id) {
            owned.push(path);
        }
    }
    Ok(owned)
}

/// Write `content` for the item `id` named `name` into `dir`.
///
/// The target is the first of `<name>.json`, `<name>_2.json`, ... that is
/// free or already owned by `id`; a file owned by another id (or one whose
/// id can't be read) is never overwritten. Other files still owned by `id`
/// (e.g. an older suffixed slot) are removed afterwards so the item isn't
/// loaded twice.
fn save_owned(dir: &Path, name: &str, id: Uuid, content: &[u8]) -> Result<PathBuf> {
    let base = sanitize_filename(name);
    let preferred = dir.join(format!("{base}.json"));
    // Fast path: the usual case of re-saving into our own unsuffixed file.
    if file_owner(&preferred) == Some(id) {
        atomic_write(&preferred, content)?;
        return Ok(preferred);
    }

    let mut n = 1usize;
    let path = loop {
        let candidate = if n == 1 {
            preferred.clone()
        } else {
            dir.join(format!("{base}_{n}.json"))
        };
        if !candidate.exists() || file_owner(&candidate) == Some(id) {
            break candidate;
        }
        n += 1;
    };
    atomic_write(&path, content)?;
    for stale in files_owned_by(dir, id)? {
        if stale != path {
            std::fs::remove_file(stale)?;
        }
    }
    Ok(path)
}

/// Remove every file in `dir` owned by `id`.
fn delete_owned(dir: &Path, id: Uuid) -> Result<()> {
    for path in files_owned_by(dir, id)? {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
