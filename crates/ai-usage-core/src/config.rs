use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default = "default_refresh_sec")]
    pub refresh_sec: u64,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            refresh_sec: default_refresh_sec(),
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
    pub fn is_enabled(&self, provider_id: &str, enabled_by_default: bool) -> bool {
        self.providers
            .iter()
            .find(|provider| provider.id == provider_id)
            .map(|provider| provider.enabled)
            .unwrap_or(enabled_by_default)
    }
}

fn default_refresh_sec() -> u64 {
    60
}

fn default_true() -> bool {
    true
}
