use std::path::PathBuf;

use crate::AppConfig;

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ai-usage")
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ai-usage")
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn default_plugin_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(value) = std::env::var("AI_USAGE_PLUGIN_DIR") {
        dirs.push(PathBuf::from(value));
    }

    dirs.push(config_dir().join("plugins"));
    dirs.push(PathBuf::from("plugins"));
    dirs
}

pub fn plugin_dirs(config: &AppConfig, extra_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    dirs.extend(extra_dirs.iter().cloned());
    dirs.extend(config.plugin_dirs.iter().cloned());
    dirs.extend(default_plugin_dirs());
    dedupe_dirs(dirs)
}

fn dedupe_dirs(dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in dirs {
        if !out.iter().any(|existing| existing == &dir) {
            out.push(dir);
        }
    }
    out
}
