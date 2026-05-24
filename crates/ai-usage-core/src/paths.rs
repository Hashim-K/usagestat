use std::path::PathBuf;

use crate::AppConfig;

fn app_dir_name() -> &'static str {
    if std::env::current_exe()
        .ok()
        .and_then(|path| path.file_name().map(|name| name.to_owned()))
        .is_some_and(|name| name == "usagestat-dev")
    {
        "usagestat-dev"
    } else {
        "usagestat"
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(app_dir_name())
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(app_dir_name())
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn cache_file() -> PathBuf {
    data_dir().join("snapshots.json")
}

pub fn default_plugin_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(value) =
        std::env::var("USAGESTAT_PLUGIN_DIR").or_else(|_| std::env::var("AI_USAGE_PLUGIN_DIR"))
    {
        dirs.push(PathBuf::from(value));
    }

    dirs.push(config_dir().join("plugins"));
    if app_dir_name() == "usagestat-dev" {
        dirs.push(data_dir().join("plugins"));
    }
    if let Some(prefix) = install_prefix() {
        let app_dir = app_dir_name();
        dirs.push(prefix.join(format!("share/{app_dir}/plugins")));
        dirs.push(prefix.join(format!("lib/{app_dir}/plugins")));
    }
    dirs.push(PathBuf::from("plugins"));
    dirs
}

fn install_prefix() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let bin_dir = exe.parent()?;
    if bin_dir.file_name().is_some_and(|name| name == "bin") {
        return bin_dir.parent().map(PathBuf::from);
    }
    None
}

pub fn plugin_dirs(config: &AppConfig, extra_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    dirs.extend(extra_dirs.iter().cloned());
    dirs.extend(config.plugin_dirs.iter().cloned());
    dirs.extend(default_plugin_dirs());
    dedupe_dirs(dirs)
}

fn dedupe_dirs(dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for dir in dirs {
        let canonical = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
        let already_seen = out.iter().any(|existing| {
            let existing_canonical =
                std::fs::canonicalize(existing).unwrap_or_else(|_| existing.clone());
            existing_canonical == canonical
        });
        if !already_seen {
            out.push(dir);
        }
    }
    out
}
