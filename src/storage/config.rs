use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::core::constants::{DEFAULT_HISTORY_LIMIT, DEFAULT_THEME, DEFAULT_TIMEOUT_MS};

/// A validated theme name wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThemeName(String);

impl ThemeName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ThemeName {
    fn default() -> Self {
        Self(DEFAULT_THEME.to_string())
    }
}

impl From<&str> for ThemeName {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl std::fmt::Display for ThemeName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for ThemeName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<&str> for ThemeName {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub theme: ThemeName,
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
            theme: ThemeName::default(),
            default_environment: None,
            history_limit: DEFAULT_HISTORY_LIMIT,
            follow_redirects: true,
            verify_ssl: true,
            timeout_ms: DEFAULT_TIMEOUT_MS,
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
            config.validate()?;
            Ok(config)
        } else {
            let config = Self::default();
            config.save()?;
            Ok(config)
        }
    }

    /// Validate configuration values are sensible.
    pub fn validate(&self) -> Result<()> {
        if self.timeout_ms == 0 {
            anyhow::bail!("timeout_ms must be greater than 0");
        }
        if self.history_limit == 0 {
            anyhow::bail!("history_limit must be greater than 0");
        }
        Ok(())
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

