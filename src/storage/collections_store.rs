use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::core::collection::Collection;
use crate::core::environment::Environment;

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

    pub fn save_collection(&self, collection: &Collection) -> Result<PathBuf> {
        let filename = sanitize_filename(&collection.name);
        let path = self.dir.join(format!("{filename}.json"));
        let content = serde_json::to_string_pretty(collection)?;
        std::fs::write(&path, content)?;
        Ok(path)
    }

    pub fn delete_collection(&self, collection: &Collection) -> Result<()> {
        let filename = sanitize_filename(&collection.name);
        let path = self.dir.join(format!("{filename}.json"));
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    // Environment files
    pub fn environments_dir(&self) -> PathBuf {
        self.dir.parent().unwrap_or(&self.dir).join("environments")
    }

    pub fn load_environments(&self) -> Result<Vec<Environment>> {
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
        Ok(environments)
    }

    pub fn load_environment(&self, path: &Path) -> Result<Environment> {
        let content = std::fs::read_to_string(path)?;
        let env: Environment = serde_json::from_str(&content)?;
        Ok(env)
    }

    pub fn save_environment(&self, env: &Environment) -> Result<PathBuf> {
        let dir = self.environments_dir();
        std::fs::create_dir_all(&dir)?;
        let filename = sanitize_filename(&env.name);
        let path = dir.join(format!("{filename}.json"));
        let content = serde_json::to_string_pretty(env)?;
        std::fs::write(&path, content)?;
        Ok(path)
    }
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
