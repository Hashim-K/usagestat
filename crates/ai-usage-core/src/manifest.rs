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
}
