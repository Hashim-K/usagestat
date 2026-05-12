use std::path::PathBuf;

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

pub fn plugin_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(value) = std::env::var("AI_USAGE_PLUGIN_DIR") {
        dirs.push(PathBuf::from(value));
    }

    dirs.push(config_dir().join("plugins"));
    dirs.push(PathBuf::from("plugins"));
    dirs
}
