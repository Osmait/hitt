use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub theme: String,
    pub default_environment: Option<String>,
    pub history_limit: usize,
    pub follow_redirects: bool,
    pub verify_ssl: bool,
    pub timeout_ms: u64,
    pub proxy: Option<String>,
    pub collections_dir: PathBuf,
    pub editor: Option<String>,
    pub vim_mode: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: "catppuccin".to_string(),
            default_environment: None,
            history_limit: 1000,
            follow_redirects: true,
            verify_ssl: true,
            timeout_ms: 30000,
            proxy: None,
            collections_dir: config_dir().join("collections"),
            editor: None,
            vim_mode: true,
        }
    }
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let path = config_file();
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let config: Self = toml::from_str(&content)?;
            Ok(config)
        } else {
            let config = Self::default();
            config.save()?;
            Ok(config)
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = config_file();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("hitt")
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("hitt")
}
