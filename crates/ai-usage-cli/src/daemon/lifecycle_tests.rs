use super::*;
use std::cell::RefCell;

struct FakeManager {
    state: Registration,
    calls: RefCell<Vec<&'static str>>,
    reject: bool,
}
impl ServiceManager for FakeManager {
    fn kind(&self) -> &'static str {
        "fixture"
    }
    fn name(&self) -> String {
        "fixture".into()
    }
    fn file(&self) -> Option<PathBuf> {
        None
    }
    fn validate(&self) -> Result<()> {
        self.calls.borrow_mut().push("validate");
        if self.reject {
            bail!("unmanaged fixture");
        }
        Ok(())
    }
    fn query(&self) -> Result<Registration> {
        self.calls.borrow_mut().push("query");
        Ok(self.state)
    }
    fn install(&self, _: &Installation, _: &Path) -> Result<()> {
        self.calls.borrow_mut().push("install");
        Ok(())
    }
    fn enable(&self) -> Result<()> {
        self.calls.borrow_mut().push("enable");
        Ok(())
    }
    fn disable(&self) -> Result<()> {
        self.calls.borrow_mut().push("disable");
        Ok(())
    }
    fn restart(&self) -> Result<()> {
        self.calls.borrow_mut().push("restart");
        Ok(())
    }
}

fn installation(root: &Path) -> Installation {
    Installation {
        owner: root.to_owned(),
        binary: root.join("usagestatd"),
        bind: "127.0.0.1:7345".parse().unwrap(),
        config: root.join("config.toml"),
        plugin_dirs: vec![root.join("plugins")],
        environment: BTreeMap::new(),
        management_key_file: root.join("management-key"),
        control_key_file: root.join("control-key"),
    }
}

#[test]
fn t3_persists_without_a_manager_and_keeps_stopped_services_stopped() {
    for (installed, running) in [(false, false), (true, false), (true, true)] {
        let directory = usagestat_core::storage::temporary_directory().unwrap();
        let path = directory.path().join("daemon.json");
        let install = installation(directory.path());
        let key = install.management_key_file.clone();
        let mut settings = DaemonSettings {
            t3_mode: SavedT3Mode::Off,
            installation: installed.then_some(install),
        };
        let manager = FakeManager {
            state: Registration {
                registered: installed,
                running,
                enabled: false,
            },
            calls: RefCell::new(Vec::new()),
            reject: !installed,
        };
        let restarted = apply_t3(&mut settings, SavedT3Mode::Auto, &path, &key, &manager).unwrap();
        assert_eq!(restarted, running);
        let retained_key = read_key(&key).unwrap();
        assert_eq!(
            DaemonSettings::load(&path).unwrap().unwrap().t3_mode,
            SavedT3Mode::Auto
        );
        assert_eq!(
            *manager.calls.borrow(),
            if !installed {
                vec![]
            } else if running {
                vec!["validate", "query", "install", "restart"]
            } else {
                vec!["validate", "query", "install"]
            }
        );
        apply_t3(&mut settings, SavedT3Mode::Off, &path, &key, &manager).unwrap();
        assert!(read_key(&key).unwrap() == retained_key);
        assert_eq!(
            DaemonSettings::load(&path).unwrap().unwrap().t3_mode,
            SavedT3Mode::Off
        );
        assert!(!manager.calls.borrow().contains(&"enable"));
    }
}

#[test]
fn refuses_unmanaged_changes_before_persisting_intent_or_keys() {
    let directory = usagestat_core::storage::temporary_directory().unwrap();
    let path = directory.path().join("daemon.json");
    let install = installation(directory.path());
    let key = install.management_key_file.clone();
    let mut settings = DaemonSettings {
        t3_mode: SavedT3Mode::Off,
        installation: Some(install),
    };
    settings.save(&path).unwrap();
    let manager = FakeManager {
        state: Registration::default(),
        calls: RefCell::new(Vec::new()),
        reject: true,
    };
    assert!(apply_t3(&mut settings, SavedT3Mode::Auto, &path, &key, &manager).is_err());
    assert!(!key.exists());
    assert_eq!(
        DaemonSettings::load(&path).unwrap().unwrap().t3_mode,
        SavedT3Mode::Off
    );
    assert_eq!(*manager.calls.borrow(), ["validate"]);
}

#[test]
fn legacy_boolean_preferences_remain_compatible_and_invalid_settings_fail() {
    let directory = usagestat_core::storage::temporary_directory().unwrap();
    let path = directory.path().join("daemon.json");
    for (text, mode) in [
        (r#"{"t3Enabled":true}"#, SavedT3Mode::Auto),
        (r#"{"t3Enabled":false}"#, SavedT3Mode::Off),
    ] {
        fs::write(&path, text).unwrap();
        assert_eq!(DaemonSettings::load(&path).unwrap().unwrap().t3_mode, mode);
    }
    for text in [
        r#"{"t3Mode":"invalid","t3Enabled":true}"#,
        "{}",
        "invalid json",
    ] {
        fs::write(&path, text).unwrap();
        assert!(DaemonSettings::load(&path).is_err());
    }
}
