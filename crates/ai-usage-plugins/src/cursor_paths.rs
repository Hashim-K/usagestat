//! Cursor Stable vs Cursor Nightly install paths (separate `state.vscdb` per app).

use std::path::PathBuf;

const STATE_DB_SUFFIX: &str = "User/globalStorage/state.vscdb";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorInstall {
    Stable,
    Nightly,
}

impl CursorInstall {
    pub fn app_dir_name(self) -> &'static str {
        match self {
            Self::Stable => "Cursor",
            Self::Nightly => "Cursor Nightly",
        }
    }

    pub fn from_plugin_id(plugin_id: &str) -> Option<Self> {
        match plugin_id.trim() {
            "cursor" => Some(Self::Stable),
            "cursor-nightly" => Some(Self::Nightly),
            _ => None,
        }
    }
}

fn platform_roots() -> Vec<PathBuf> {
    usagestat_core::provider_paths::app_support_dir()
        .into_iter()
        .collect()
}

fn resolve_from_roots(install: CursorInstall, roots: &[PathBuf]) -> Option<PathBuf> {
    roots
        .iter()
        .map(|root| root.join(install.app_dir_name()).join(STATE_DB_SUFFIX))
        .find(|path| path.is_file())
}

/// `state.vscdb` for one install only (stable **or** nightly - never merged).
pub fn resolve_cursor_state_db_for(install: CursorInstall) -> Option<PathBuf> {
    let key = match install {
        CursorInstall::Stable => "CURSOR_STATE_DB",
        CursorInstall::Nightly => "CURSOR_NIGHTLY_STATE_DB",
    };
    resolve_profile(
        install,
        &platform_roots(),
        std::env::var(key).ok().as_deref(),
    )
}

fn resolve_profile(
    install: CursorInstall,
    roots: &[PathBuf],
    custom: Option<&str>,
) -> Option<PathBuf> {
    if let Some(custom) = custom.filter(|value| !value.is_empty()) {
        let path = usagestat_core::paths::expand_home(custom);
        // An invalid explicit profile must not select a different installation.
        return path.is_file().then_some(path);
    }
    resolve_from_roots(install, roots)
}

pub fn resolve_cursor_state_db_for_plugin_id(plugin_id: &str) -> Option<PathBuf> {
    CursorInstall::from_plugin_id(plugin_id).and_then(resolve_cursor_state_db_for)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_redirected_roots_keep_stable_and_nightly_separate() {
        let root =
            std::env::temp_dir().join(format!("usagestat-cursor-paths-{}", std::process::id()));
        let native = root.join("redirected 使用 config");
        let legacy = root.join("legacy");
        for base in [&native, &legacy] {
            for install in [CursorInstall::Stable, CursorInstall::Nightly] {
                let path = base.join(install.app_dir_name()).join(STATE_DB_SUFFIX);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, []).unwrap();
            }
        }
        let roots = [native.clone(), legacy];
        for install in [CursorInstall::Stable, CursorInstall::Nightly] {
            assert_eq!(
                resolve_from_roots(install, &roots),
                Some(native.join(install.app_dir_name()).join(STATE_DB_SUFFIX))
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_explicit_database_never_falls_back_to_an_installed_account() {
        let fixture = tempfile::tempdir().unwrap();
        let database = fixture.path().join("Cursor").join(STATE_DB_SUFFIX);
        std::fs::create_dir_all(database.parent().unwrap()).unwrap();
        std::fs::write(&database, []).unwrap();
        let roots = [fixture.path().to_owned()];
        assert_eq!(
            resolve_profile(CursorInstall::Stable, &roots, None),
            Some(database)
        );
        assert_eq!(
            resolve_profile(
                CursorInstall::Stable,
                &roots,
                Some(fixture.path().join("missing").to_str().unwrap())
            ),
            None
        );
        assert_eq!(resolve_profile(CursorInstall::Nightly, &roots, None), None);
    }

    #[test]
    fn from_plugin_id_maps_both_providers() {
        assert_eq!(
            CursorInstall::from_plugin_id("cursor"),
            Some(CursorInstall::Stable)
        );
        assert_eq!(
            CursorInstall::from_plugin_id("cursor-nightly"),
            Some(CursorInstall::Nightly)
        );
        assert_eq!(CursorInstall::from_plugin_id("claude"), None);
    }
}
