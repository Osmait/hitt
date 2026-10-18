use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::core::constants::{DEFAULT_HISTORY_LIMIT, DEFAULT_THEME, DEFAULT_TIMEOUT_MS};

// ---------------------------------------------------------------------------
// Theme override structs (pure data, used for TOML deserialization).
// Moved here from ui::theme so the storage layer has no UI dependency.
// The TUI's Theme struct reads these and materializes live ratatui colors.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThemeOverride {
    pub background: Option<String>,
    pub foreground: Option<String>,
    pub accent: Option<String>,
    pub success: Option<String>,
    pub warning: Option<String>,
    pub error: Option<String>,
    pub muted: Option<String>,
    #[serde(default)]
    pub methods: Option<MethodColorsOverride>,
    #[serde(default)]
    pub status: Option<StatusColorsOverride>,
    #[serde(default)]
    pub syntax: Option<SyntaxColorsOverride>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MethodColorsOverride {
    pub get: Option<String>,
    pub post: Option<String>,
    pub put: Option<String>,
    pub patch: Option<String>,
    pub delete: Option<String>,
    pub head: Option<String>,
    pub options: Option<String>,
    pub ws: Option<String>,
    pub sse: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatusColorsOverride {
    pub info: Option<String>,
    pub success: Option<String>,
    pub redirect: Option<String>,
    pub client_error: Option<String>,
    pub server_error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyntaxColorsOverride {
    pub json_key: Option<String>,
    pub json_string: Option<String>,
    pub json_number: Option<String>,
    pub json_boolean: Option<String>,
    pub json_null: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BorderOverride {
    pub style: Option<String>,
    pub sidebar_width: Option<u16>,
}

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
    /// Name of the environment active when hitt last ran (restored at startup,
    /// takes precedence over `default_environment`).
    #[serde(default)]
    pub last_environment: Option<String>,
    pub history_limit: usize,
    /// Save request history to `history.json` so it survives restarts.
    #[serde(default = "default_true")]
    pub persist_history: bool,
    pub follow_redirects: bool,
    pub verify_ssl: bool,
    pub timeout_ms: u64,
    pub proxy: Option<String>,
    pub collections_dir: PathBuf,
    pub editor: Option<String>,
    #[serde(default)]
    pub colors: Option<ThemeOverride>,
    #[serde(default)]
    pub borders: Option<BorderOverride>,
}

fn default_true() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: ThemeName::default(),
            default_environment: None,
            last_environment: None,
            history_limit: DEFAULT_HISTORY_LIMIT,
            persist_history: true,
            follow_redirects: true,
            verify_ssl: true,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            proxy: None,
            collections_dir: config_dir().join("collections"),
            editor: None,
            colors: None,
            borders: None,
        }
    }
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let path = config_file();
        if path.exists() {
            Self::load_from(&path)
        } else {
            let config = Self::default();
            config.save()?;
            Ok(config)
        }
    }

    /// Read and validate the config at `path`.
    pub fn load_from(path: &std::path::Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
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
        self.save_to(&config_file())
    }

    /// Atomically write the config as TOML to `path`.
    pub fn save_to(&self, path: &std::path::Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        let mut tmp =
            tempfile::NamedTempFile::new_in(path.parent().unwrap_or(std::path::Path::new(".")))?;
        std::io::Write::write_all(&mut tmp, content.as_bytes())?;
        std::io::Write::flush(&mut tmp)?;
        tmp.persist(path)?;
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
