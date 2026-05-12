use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default = "default_refresh_sec")]
    pub refresh_sec: u64,
    #[serde(default)]
    pub plugin_dirs: Vec<PathBuf>,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            refresh_sec: default_refresh_sec(),
            plugin_dirs: Vec::new(),
            providers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl AppConfig {
    pub fn load_optional(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let text = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    pub fn is_enabled(&self, provider_id: &str, enabled_by_default: bool) -> bool {
        self.providers
            .iter()
            .find(|provider| provider.id == provider_id)
            .map(|provider| provider.enabled)
            .unwrap_or(enabled_by_default)
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse config {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
}

fn default_refresh_sec() -> u64 {
    60
}

fn default_true() -> bool {
    true
}
