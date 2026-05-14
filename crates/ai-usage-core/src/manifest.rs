use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderManifest {
    pub id: String,
    pub name: String,
    pub entry: String,
    #[serde(default)]
    pub enabled_by_default: bool,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub links: Vec<ProviderLink>,
    /// Explicit source modes this plugin supports (e.g. ["oauth", "web"]).
    /// Empty means the plugin hasn't declared modes — no validation is applied.
    #[serde(default)]
    pub supported_modes: Vec<String>,
    /// What "auto" resolves to for this plugin (e.g. "oauth").
    /// Empty string means unspecified / plugin decides internally.
    #[serde(default)]
    pub auto_mode: String,
    /// The primary web URL for this provider (e.g. "https://claude.ai").
    /// Used to locate browser cookies when running in web mode.
    #[serde(default)]
    pub web_url: Option<String>,
}

impl ProviderManifest {
    /// Returns true if the requested mode is supported by this plugin.
    /// "auto" is always accepted. Plugins without declared modes accept everything.
    pub fn is_mode_supported(&self, mode: &str) -> bool {
        mode == "auto"
            || self.supported_modes.is_empty()
            || self.supported_modes.iter().any(|m| m == mode)
    }

    /// Returns a human-readable error if the mode is unsupported, or None if ok.
    pub fn check_mode(&self, mode: &str) -> Option<String> {
        if self.is_mode_supported(mode) {
            return None;
        }
        let available = self.supported_modes.join(", ");
        Some(format!(
            "Mode '{}' is not supported by the '{}' plugin. Supported modes: {}.",
            mode, self.id, available
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderLink {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct LoadedProvider {
    pub manifest: ProviderManifest,
    pub dir: PathBuf,
    pub entry_script: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSummary {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// Supported source modes declared by the plugin manifest.
    pub supported_modes: Vec<String>,
    /// What "auto" resolves to for this plugin.
    pub auto_mode: String,
    /// The primary web URL for this provider, if web mode is supported.
    pub web_url: Option<String>,
}
