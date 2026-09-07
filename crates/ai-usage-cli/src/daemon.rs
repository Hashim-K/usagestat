//! Opt-in, per-user systemd service management. No root privileges or shell needed.
use anyhow::{Context, Result, bail};
use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};
use usagestat_core::{AppConfig, paths};

const UNIT: &str = "usagestat.service";
const MARKER: &str = "# Managed by usagestat daemon enable\n";

pub(super) fn dev_profile() -> bool {
    paths::is_dev_profile()
}

fn service_name() -> &'static str {
    if dev_profile() {
        "usagestat-dev.service"
    } else {
        UNIT
    }
}

fn service_unit_file() -> Result<PathBuf> {
    Ok(dirs::config_dir()
        .context("locate user config directory")?
        .join("systemd/user")
        .join(service_name()))
}

pub(super) fn dashboard_base_url(bind: Option<SocketAddr>) -> Result<String> {
    let unit = if bind.is_none() {
        read_optional_unit(&service_unit_file()?)?
    } else {
        String::new()
    };
    let bind = match bind {
        Some(bind) => bind,
        None if unit.starts_with(MARKER) => unit_bind(&unit)?,
        None => SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 6736),
    };
    if bind.port() == 0 {
        bail!("the dashboard requires a fixed, nonzero daemon port");
    }
    Ok(local_url(bind))
}

pub(super) fn check_health(base_url: &str) -> Result<()> {
    let body: serde_json::Value = local_client(Duration::from_secs(1))?
        .get(format!("{base_url}/health"))
        .send()?
        .error_for_status()?
        .json()?;
    if body.get("status").and_then(|status| status.as_str()) != Some("ok") {
        bail!("unexpected daemon health response");
    }
    Ok(())
}

#[derive(Debug, Subcommand)]
pub enum DaemonCommand {
    /// Start the daemon now and automatically at login, preserving the T3 mode
    Enable {
        /// Daemon executable (defaults to usagestatd beside this CLI, then PATH)
        #[arg(long, value_name = "PATH")]
        binary: Option<PathBuf>,
        #[arg(long, default_value = "127.0.0.1:6736")]
        bind: SocketAddr,
        /// Select T3 auto mode and create its management key if missing
        #[arg(long)]
        t3: bool,
    },
    /// Stop the daemon, or set only its T3 mode to off with --t3
    Disable {
        /// Set T3 mode to off; retain the daemon and management key
        #[arg(long)]
        t3: bool,
    },
    /// Toggle T3 between auto and off, preserving whether the daemon is running
    Toggle {
        #[arg(long, required = true)]
        t3: bool,
    },
    /// Set whether the T3 bridge follows the daemon (auto) or stays off
    T3 {
        #[arg(value_enum)]
        mode: T3Mode,
    },
    /// Show whether the daemon is running and startup at login is enabled
    Status,
    /// Print the management key to paste into T3 Code
    Key,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ServiceStatus {
    service: &'static str,
    autostart: bool,
    running: bool,
    unit_file: PathBuf,
    t3_mode: T3Mode,
    t3_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    management_key_file: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dashboard_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hub_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum T3Mode {
    /// Expose the bridge whenever the daemon runs
    Auto,
    /// Keep the bridge disabled
    Off,
}

impl T3Mode {
    fn from_enabled(enabled: bool) -> Self {
        if enabled { Self::Auto } else { Self::Off }
    }

    fn status_text(self, running: bool, available: bool) -> &'static str {
        match (self, running, available) {
            (Self::Off, _, _) => "off",
            (Self::Auto, false, _) => "auto · unavailable (daemon stopped)",
            (Self::Auto, true, true) => "auto · available",
            (Self::Auto, true, false) => "auto · unavailable (bridge not responding)",
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DaemonSettings {
    t3_mode: T3Mode,
}

impl DaemonSettings {
    fn load(path: &Path, unit: &str, key: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct StoredSettings {
                    t3_mode: Option<T3Mode>,
                    t3_enabled: Option<bool>,
                }
                let stored: StoredSettings =
                    serde_json::from_str(&text).context("read daemon settings")?;
                Ok(Self {
                    t3_mode: stored
                        .t3_mode
                        .or_else(|| stored.t3_enabled.map(T3Mode::from_enabled))
                        .context("daemon settings must contain t3Mode (auto or off)")?,
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self {
                // Preserve services created before the settings file existed.
                t3_mode: T3Mode::from_enabled(unit_enables_t3(unit, key)?),
            }),
            Err(e) => Err(e).context("read daemon settings"),
        }
    }

    fn save(&self, path: &Path) -> Result<()> {
        write_atomic(path, &format!("{}\n", serde_json::to_string_pretty(self)?))
    }
}

pub fn run(
    command: &DaemonCommand,
    config_path: &Path,
    extra_dirs: &[PathBuf],
    json: bool,
) -> Result<()> {
    if !cfg!(target_os = "linux") {
        bail!("daemon autostart currently requires Linux with a systemd user session");
    }
    let service = service_name();
    let key_file = paths::config_dir()?.join("t3-management-key");
    if matches!(command, DaemonCommand::Key) {
        let key = read_key(&key_file)
            .context("no usable management key; run `usagestat daemon t3 auto` first")?;
        if json {
            println!(
                "{}",
                serde_json::to_string(&serde_json::json!({"managementKey": key}))?
            );
        } else {
            println!("{key}");
        }
        return Ok(());
    }
    let unit_file = service_unit_file()?;
    let settings_file = paths::config_dir()?.join("daemon.json");
    let old_unit = read_optional_unit(&unit_file)?;
    let mut settings = DaemonSettings::load(&settings_file, &old_unit, &key_file)?;
    let mut base_url = None;
    match command {
        DaemonCommand::Enable { binary, bind, t3 } => {
            // Validate all inputs before changing the service or generating a key.
            if bind.port() == 0 {
                bail!("daemon autostart requires a fixed, nonzero port");
            }
            ensure_managed_unit(&unit_file)?;
            systemctl(&["show-environment"])?;
            let binary = find_binary(binary.as_deref())?;
            let config = AppConfig::load_optional(config_path)?;
            let plugin_dirs = paths::plugin_dirs(&config, extra_dirs)?
                .into_iter()
                .map(|dir| absolute(&dir))
                .collect::<Result<Vec<_>>>()?;
            let config_path = absolute(config_path)?;
            let t3_enabled = *t3 || settings.t3_mode == T3Mode::Auto;
            let unit = render_unit(
                &binary,
                &config_path,
                &plugin_dirs,
                t3_enabled.then_some(key_file.as_path()),
                *bind,
            )?;
            if t3_enabled {
                ensure_key(&key_file)?;
            }
            write_atomic(&unit_file, &unit)?;
            settings.t3_mode = T3Mode::from_enabled(t3_enabled);
            settings.save(&settings_file)?;
            systemctl(&["daemon-reload"])?;
            // An app can supply a different XDG_CONFIG_HOME from the user
            // manager. Enabling the absolute path also links the unit there.
            systemctl(&[
                "enable",
                unit_file
                    .to_str()
                    .context("service unit path must be UTF-8")?,
            ])?;
            // Restart applies changed bind/config/plugin paths on repeated enable.
            systemctl(&["restart", service])?;
            let url = local_url(*bind);
            wait_until_ready(&url).with_context(|| format!(
                "daemon did not become ready; inspect `journalctl --user -u {service}` or disable the daemon"
            ))?;
            base_url = Some(url);
        }
        DaemonCommand::Disable { t3: true }
        | DaemonCommand::Toggle { .. }
        | DaemonCommand::T3 { .. } => {
            ensure_managed_unit(&unit_file)?;
            systemctl(&["show-environment"])?;
            let mode = match command {
                DaemonCommand::T3 { mode } => *mode,
                DaemonCommand::Toggle { .. } => {
                    T3Mode::from_enabled(settings.t3_mode == T3Mode::Off)
                }
                _ => T3Mode::Off,
            };
            let enabled = mode == T3Mode::Auto;
            // Edit only the bridge argument, preserving the installed service's
            // binary, bind address, config, plugin paths, and environment.
            let updated = if old_unit.is_empty() {
                None
            } else {
                Some(set_unit_t3(&old_unit, &key_file, enabled)?)
            };
            if enabled {
                ensure_key(&key_file)?;
            }
            let updated = updated.filter(|(unit, _)| unit != &old_unit);
            if let Some((unit, _)) = &updated {
                write_atomic(&unit_file, unit)?;
            }
            settings.t3_mode = mode;
            settings.save(&settings_file)?;
            if let Some((_, bind)) = updated {
                systemctl(&["daemon-reload"])?;
                // Unlike restart, try-restart leaves a stopped daemon stopped.
                systemctl(&["try-restart", service])?;
                let state = systemctl(&["show", service, "--property=ActiveState", "--value"])?;
                if String::from_utf8_lossy(&state.stdout).trim() == "active" {
                    let url = local_url(bind);
                    wait_until_ready(&url).with_context(|| {
                        format!(
                            "daemon did not become ready; inspect `journalctl --user -u {service}`"
                        )
                    })?;
                    base_url = Some(url);
                }
            }
        }
        DaemonCommand::Disable { t3: false } => {
            ensure_managed_unit(&unit_file)?;
            // An absent service is already disabled. Still surface user-bus errors.
            let state = systemctl(&["show", service, "--property=LoadState", "--value"])?;
            settings.save(&settings_file)?;
            if String::from_utf8_lossy(&state.stdout).trim() != "not-found" {
                systemctl(&["disable", "--now", service])?;
            }
        }
        DaemonCommand::Status => {}
        DaemonCommand::Key => unreachable!(),
    }
    let status = service_status(service, unit_file, key_file, settings.t3_mode, base_url)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&status)?);
    } else {
        println!(
            "Startup at login: {}",
            if status.autostart {
                "enabled"
            } else {
                "disabled"
            }
        );
        println!(
            "Daemon: {}",
            if status.running { "running" } else { "stopped" }
        );
        if let Some(url) = status.dashboard_url {
            println!("Dashboard URL: {url}");
        }
        println!(
            "T3: {}",
            status
                .t3_mode
                .status_text(status.running, status.t3_available)
        );
        if let Some(url) = status.hub_url {
            println!("Hub URL: {url}");
        }
        if let Some(path) = status.management_key_file {
            println!("Management key file: {}", path.display());
            println!(
                "Show the key with: {} daemon key",
                if dev_profile() {
                    "usagestat-dev"
                } else {
                    "usagestat"
                }
            );
        }
    }
    Ok(())
}

fn systemctl(args: &[&str]) -> Result<Output> {
    let output = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .context("run systemctl --user; a systemd user session is required")?;
    if !output.status.success() {
        bail!(
            "systemctl --user {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output)
}

fn service_status(
    service: &'static str,
    unit_file: PathBuf,
    management_key_file: PathBuf,
    t3_mode: T3Mode,
    base_url: Option<String>,
) -> Result<ServiceStatus> {
    let output = systemctl(&["show", service, "--property=ActiveState,UnitFileState"])?;
    let properties = String::from_utf8_lossy(&output.stdout);
    let running = properties.lines().any(|line| line == "ActiveState=active");
    let unit = read_optional_unit(&unit_file)?;
    let base_url = base_url.or_else(|| {
        unit.starts_with(MARKER)
            .then(|| unit_bind(&unit).ok().map(local_url))
            .flatten()
    });
    // Auto is a saved preference, not evidence of a working connection. Check
    // the authenticated endpoint before advertising the bridge as available.
    let t3_available = t3_mode == T3Mode::Auto
        && running
        && base_url
            .as_ref()
            .is_some_and(|url| quota_endpoint_available(url, &management_key_file));
    Ok(ServiceStatus {
        service,
        autostart: properties
            .lines()
            .any(|line| line == "UnitFileState=enabled"),
        running,
        unit_file,
        t3_mode,
        t3_available,
        management_key_file: (t3_mode == T3Mode::Auto).then_some(management_key_file),
        dashboard_url: base_url
            .as_ref()
            .filter(|_| running)
            .map(|url| format!("{url}/dashboard")),
        hub_url: base_url.filter(|_| t3_available),
    })
}

fn read_optional_unit(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(unit) => Ok(unit),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e).context("read daemon service configuration"),
    }
}

fn unit_enables_t3(unit: &str, key: &Path) -> Result<bool> {
    let argument = format!(" --management-key-file {}", path_arg(key)?);
    Ok(unit.starts_with(MARKER)
        && unit
            .lines()
            .any(|line| line.starts_with("ExecStart=") && line.contains(&argument)))
}

// Split the generated ExecStart syntax while retaining quoting and escaping.
// Tokens stay encoded for systemd; they are never evaluated by a shell.
fn unit_words(command: &str) -> Result<Vec<&str>> {
    let mut words = Vec::new();
    let mut start = None;
    let mut quote = None;
    let mut escaped = false;
    for (i, c) in command.char_indices() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if let Some(delimiter) = quote {
            if c == delimiter {
                quote = None;
            }
        } else if c == '"' || c == '\'' {
            quote = Some(c);
        } else if c.is_whitespace() {
            if let Some(begin) = start.take() {
                words.push(&command[begin..i]);
            }
            continue;
        }
        start.get_or_insert(i);
    }
    if quote.is_some() || escaped {
        bail!("invalid quoting in managed daemon service");
    }
    if let Some(begin) = start {
        words.push(&command[begin..]);
    }
    Ok(words)
}

fn unit_command(unit: &str) -> Result<&str> {
    let commands: Vec<_> = unit
        .lines()
        .filter_map(|line| line.strip_prefix("ExecStart="))
        .collect();
    let [command] = commands.as_slice() else {
        bail!("expected one ExecStart in managed daemon service");
    };
    Ok(command)
}

fn unit_bind(unit: &str) -> Result<SocketAddr> {
    let words = unit_words(unit_command(unit)?)?;
    words
        .windows(2)
        .find(|pair| pair[0] == "--bind")
        .context("managed daemon service is missing its bind address")?[1]
        .parse()
        .context("invalid bind address in managed daemon service")
}

fn set_unit_t3(unit: &str, key: &Path, enabled: bool) -> Result<(String, SocketAddr)> {
    let mut words = unit_words(unit_command(unit)?)?;
    let bind = unit_bind(unit)?;
    let key_arg = path_arg(key)?;
    if let Some(index) = words
        .iter()
        .position(|word| *word == "--management-key-file")
    {
        if words.get(index + 1) != Some(&key_arg.as_str()) {
            bail!("managed daemon service uses an unexpected management key path");
        }
        if !enabled {
            words.drain(index..index + 2);
        }
    } else if enabled {
        words.extend(["--management-key-file", &key_arg]);
    }
    let command = format!("ExecStart={}", words.join(" "));
    let mut updated = String::new();
    for line in unit.lines() {
        updated.push_str(if line.starts_with("ExecStart=") {
            &command
        } else {
            line
        });
        updated.push('\n');
        if line == "[Service]" && !unit.contains("\nUnsetEnvironment=USAGESTAT_MANAGEMENT_KEY\n") {
            updated.push_str("UnsetEnvironment=USAGESTAT_MANAGEMENT_KEY\n");
        }
    }
    Ok((updated, bind))
}

fn absolute(path: &Path) -> Result<PathBuf> {
    Ok(if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    })
}

fn find_binary(explicit: Option<&Path>) -> Result<PathBuf> {
    let candidates = if let Some(path) = explicit {
        vec![path.to_path_buf()]
    } else {
        let daemon_name = if dev_profile() {
            "usagestatd-dev"
        } else {
            "usagestatd"
        };
        let daemon_name = format!("{daemon_name}{}", std::env::consts::EXE_SUFFIX);
        let mut candidates = vec![std::env::current_exe()?.with_file_name(&daemon_name)];
        if let Some(path) = std::env::var_os("PATH") {
            candidates.extend(std::env::split_paths(&path).map(|dir| dir.join(&daemon_name)));
        }
        candidates
    };
    for candidate in candidates {
        if candidate.is_file() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if candidate.metadata()?.permissions().mode() & 0o111 == 0 {
                    continue;
                }
            }
            return candidate
                .canonicalize()
                .context("resolve daemon executable");
        }
    }
    bail!(
        "usagestatd was not found; install it beside usagestat, use --binary PATH, or build with `cargo build -p usagestat-cli -p usagestat-daemon`"
    )
}

fn ensure_managed_unit(path: &Path) -> Result<()> {
    match fs::read_to_string(path) {
        Ok(text) if !text.starts_with(MARKER) => bail!(
            "{} is not managed by usagestat; preserve or move that service before using this command",
            path.display()
        ),
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => Ok(()),
    }
}

fn read_key(path: &Path) -> Result<String> {
    let key = usagestat_core::storage::read_private(path)?
        .trim()
        .to_string();
    if key.is_empty() || !key.bytes().all(|b| b.is_ascii_graphic()) {
        bail!("management key must be nonempty ASCII text without whitespace");
    }
    Ok(key)
}

fn ensure_key(path: &Path) -> Result<String> {
    if path.try_exists()? {
        return read_key(path);
    }
    let mut random = [0_u8; 32];
    getrandom::getrandom(&mut random)
        .map_err(|error| anyhow::anyhow!("generate management key: {error}"))?;
    let key: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    usagestat_core::storage::create_once(path, format!("{key}\n").as_bytes())?;
    read_key(path)
}

// systemd unit quoting is not shell quoting. Percent specifiers and (for
// ExecStart only) dollar expansion must also be escaped, even inside quotes.
fn unit_quote(value: &str, exec: bool) -> Result<String> {
    if value.chars().any(|c| c.is_control()) {
        bail!("service paths and environment values must not contain control characters");
    }
    let value = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%");
    Ok(format!(
        "\"{}\"",
        if exec {
            value.replace('$', "$$")
        } else {
            value
        }
    ))
}

fn path_arg(path: &Path) -> Result<String> {
    unit_quote(
        path.to_str().context("service paths must be valid UTF-8")?,
        true,
    )
}

fn render_unit(
    binary: &Path,
    config: &Path,
    plugin_dirs: &[PathBuf],
    key: Option<&Path>,
    bind: SocketAddr,
) -> Result<String> {
    let mut command = format!(
        "{} --bind {} --config {}",
        path_arg(binary)?,
        bind,
        path_arg(config)?
    );
    if let Some(key) = key {
        command.push_str(&format!(" --management-key-file {}", path_arg(key)?));
    }
    for dir in plugin_dirs {
        command.push_str(&format!(" --plugin-dir {}", path_arg(dir)?));
    }
    let mut environment = String::new();
    // Preserve CLI discovery and XDG locations; never persist the invoking
    // process's full environment (it can contain provider credentials).
    for name in ["PATH", "XDG_CONFIG_HOME", "XDG_DATA_HOME"] {
        if let Ok(value) = std::env::var(name) {
            environment.push_str(&format!(
                "Environment={}\n",
                unit_quote(&format!("{name}={value}"), false)?
            ));
        }
    }
    let env_file = paths::config_dir()?.join("daemon.env");
    let env_file = unit_quote(
        env_file
            .to_str()
            .context("environment file path must be UTF-8")?,
        false,
    )?;
    Ok(format!(
        "{MARKER}[Unit]\nDescription=usagestat usage backend\n\n[Service]\nType=exec\nExecStart={command}\nWorkingDirectory=%h\n{environment}EnvironmentFile=-{env_file}\nUnsetEnvironment=USAGESTAT_MANAGEMENT_KEY\nRestart=on-failure\nRestartSec=5\nUMask=0077\n\n[Install]\nWantedBy=default.target\n"
    ))
}

fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    usagestat_core::storage::write_atomic(path, contents.as_bytes())?;
    Ok(())
}

fn local_url(bind: SocketAddr) -> String {
    let ip = match bind.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    format!("http://{}", SocketAddr::new(ip, bind.port()))
}

fn local_client(timeout: Duration) -> Result<reqwest::blocking::Client> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    Ok(reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(timeout)
        .build()?)
}

fn quota_endpoint_available(url: &str, key_file: &Path) -> bool {
    let check = || -> Result<bool> {
        let key = read_key(key_file)?;
        let response = local_client(Duration::from_secs(1))?
            .get(format!("{url}/v0/management/quota-scheduler/status"))
            .bearer_auth(key)
            .send()?;
        if !response.status().is_success() {
            return Ok(false);
        }
        let body: serde_json::Value = response.json()?;
        Ok(body
            .get("accounts")
            .is_some_and(|accounts| accounts.is_object()))
    };
    check().unwrap_or(false)
}

fn wait_until_ready(url: &str) -> Result<()> {
    let client = local_client(Duration::from_secs(1))?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(response) = client.get(format!("{url}/health")).send() {
            if response.status().is_success() {
                let body: serde_json::Value = response.json()?;
                if body.get("status").and_then(|status| status.as_str()) == Some("ok") {
                    return Ok(());
                }
            }
        }
        if Instant::now() >= deadline {
            bail!("daemon health endpoint is not responding at {url}/health");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use super::*;

    fn temp_dir() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "usagestat-service-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn generates_private_persistent_key_without_rotation() {
        let dir = temp_dir();
        let path = dir.join("key");
        let key = ensure_key(&path).unwrap();
        assert_eq!(key.len(), 64);
        assert!(key.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(ensure_key(&path).unwrap(), key);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::write(&path, "invalid key\n").unwrap();
        assert!(ensure_key(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "invalid key\n");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn service_arguments_escape_systemd_expansion_and_reject_injection() {
        assert_eq!(
            unit_quote("/path with space/$VAR/%h/\"file\"", true).unwrap(),
            "\"/path with space/$$VAR/%%h/\\\"file\\\"\""
        );
        assert_eq!(
            unit_quote("PATH=/a/$literal/%path", false).unwrap(),
            "\"PATH=/a/$literal/%%path\""
        );
        assert!(unit_quote("/path\nExecStart=/evil", true).is_err());
        let unit = render_unit(
            Path::new("/opt/my tools/usagestatd"),
            Path::new("/config.toml"),
            &[PathBuf::from("/my plugins")],
            Some(Path::new("/key")),
            "127.0.0.1:6736".parse().unwrap(),
        )
        .unwrap();
        assert!(unit.contains("ExecStart=\"/opt/my tools/usagestatd\" --bind 127.0.0.1:6736"));
        assert!(unit.contains("--plugin-dir \"/my plugins\""));
        assert!(unit.contains("--management-key-file \"/key\""));
        assert!(unit_enables_t3(&unit, Path::new("/key")).unwrap());
        assert!(unit.contains("WantedBy=default.target"));
    }

    #[test]
    fn ordinary_service_keeps_t3_disabled_even_with_a_retained_key() {
        let dir = temp_dir();
        let key = dir.join("key");
        fs::write(&key, "invalid old key\n").unwrap();
        let unit = render_unit(
            Path::new("/usr/bin/usagestatd"),
            Path::new("/config --management-key-file \"/key\".toml"),
            &[],
            None,
            "127.0.0.1:6736".parse().unwrap(),
        )
        .unwrap();
        assert!(!unit_enables_t3(&unit, &key).unwrap());
        assert!(!unit_enables_t3(&unit, Path::new("/key")).unwrap());
        assert!(unit.contains("\nUnsetEnvironment=USAGESTAT_MANAGEMENT_KEY\n"));
        assert_eq!(fs::read_to_string(&key).unwrap(), "invalid old key\n");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn remembers_t3_preference_and_migrates_existing_services() {
        let dir = temp_dir();
        let path = dir.join("daemon.json");
        let key = dir.join("key");
        let unit = render_unit(
            Path::new("/usr/bin/usagestatd"),
            Path::new("/config.toml"),
            &[],
            Some(&key),
            "127.0.0.1:6736".parse().unwrap(),
        )
        .unwrap();
        assert_eq!(
            DaemonSettings::load(&path, "", &key).unwrap().t3_mode,
            T3Mode::Off
        );
        let mut settings = DaemonSettings::load(&path, &unit, &key).unwrap();
        assert_eq!(settings.t3_mode, T3Mode::Auto);
        settings.save(&path).unwrap();
        // The saved preference survives even if a service has not been installed.
        assert_eq!(
            DaemonSettings::load(&path, "", &key).unwrap().t3_mode,
            T3Mode::Auto
        );
        settings.t3_mode = T3Mode::Off;
        settings.save(&path).unwrap();
        assert_eq!(
            DaemonSettings::load(&path, &unit, &key).unwrap().t3_mode,
            T3Mode::Off
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&fs::read_to_string(&path).unwrap()).unwrap(),
            serde_json::json!({"t3Mode": "off"})
        );
        // Boolean preferences from older builds migrate without losing state.
        for enabled in [false, true] {
            fs::write(&path, serde_json::json!({"t3Enabled": enabled}).to_string()).unwrap();
            assert_eq!(
                DaemonSettings::load(&path, "", &key).unwrap().t3_mode,
                T3Mode::from_enabled(enabled)
            );
        }
        fs::write(&path, r#"{"t3Mode":"invalid","t3Enabled":true}"#).unwrap();
        assert!(DaemonSettings::load(&path, &unit, &key).is_err());
        fs::write(&path, "invalid json").unwrap();
        assert!(DaemonSettings::load(&path, &unit, &key).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn changing_t3_preserves_custom_service_settings_and_quoted_paths() {
        let key = Path::new("/config %h/$VAR/\"key\"");
        let unit = render_unit(
            Path::new("/my --bind invalid/工具/usagestatd"),
            Path::new("/config --management-key-file \"/key\".toml"),
            &[PathBuf::from("/my \\ plugins/$VAR/%h")],
            None,
            "[::1]:7345".parse().unwrap(),
        )
        .unwrap();
        let (enabled, bind) = set_unit_t3(&unit, key, true).unwrap();
        assert_eq!(bind.to_string(), "[::1]:7345");
        assert!(unit_enables_t3(&enabled, key).unwrap());
        assert_eq!(set_unit_t3(&enabled, key, true).unwrap().0, enabled);
        let (disabled, _) = set_unit_t3(&enabled, key, false).unwrap();
        assert_eq!(disabled, unit);
        assert_eq!(set_unit_t3(&disabled, key, false).unwrap().0, disabled);
        assert!(set_unit_t3(&enabled, Path::new("/different-key"), false).is_err());
        assert!(unit_words("\"unterminated").is_err());
        // Older generated services also get protection from inherited env keys.
        let old_unit = enabled.replace("UnsetEnvironment=USAGESTAT_MANAGEMENT_KEY\n", "");
        assert_eq!(
            set_unit_t3(&old_unit, key, false)
                .unwrap()
                .0
                .matches("UnsetEnvironment=USAGESTAT_MANAGEMENT_KEY")
                .count(),
            1
        );
    }

    #[test]
    fn parses_explicit_bridge_controls_and_plain_daemon_commands() {
        use clap::Parser;
        for args in [
            vec!["usagestat", "daemon", "t3", "auto"],
            vec!["usagestat", "daemon", "t3", "off"],
            vec!["usagestat", "daemon", "toggle", "--t3"],
            vec!["usagestat", "daemon", "disable", "--t3"],
            vec!["usagestat", "daemon", "disable"],
            vec!["usagestat", "daemon", "enable"],
        ] {
            assert!(crate::Cli::try_parse_from(args).is_ok());
        }
        assert!(crate::Cli::try_parse_from(["usagestat", "daemon", "toggle"]).is_err());
        assert!(crate::Cli::try_parse_from(["usagestat", "daemon", "t3", "on"]).is_err());
        assert!(crate::Cli::try_parse_from(["usagestat", "daemon", "t3"]).is_err());
    }

    #[test]
    fn status_distinguishes_saved_mode_from_connection_availability() {
        assert_eq!(
            T3Mode::Auto.status_text(false, false),
            "auto · unavailable (daemon stopped)"
        );
        assert_eq!(T3Mode::Auto.status_text(true, true), "auto · available");
        assert_eq!(
            T3Mode::Auto.status_text(true, false),
            "auto · unavailable (bridge not responding)"
        );
        for running in [false, true] {
            assert_eq!(T3Mode::Off.status_text(running, false), "off");
        }
    }

    #[test]
    fn availability_requires_an_authenticated_quota_response() {
        let dir = temp_dir();
        let key = dir.join("key");
        fs::write(&key, "test-key").unwrap();
        for (status, body, expected) in [
            ("200 OK", r#"{"accounts":{}}"#, true),
            ("401 Unauthorized", r#"{"accounts":{}}"#, false),
            ("200 OK", r#"{"status":"ok"}"#, false),
            ("200 OK", "invalid json", false),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = std::io::BufReader::new(&mut stream);
                let mut request = String::new();
                loop {
                    let mut line = String::new();
                    assert!(std::io::BufRead::read_line(&mut reader, &mut line).unwrap() > 0);
                    request.push_str(&line);
                    if line == "\r\n" {
                        break;
                    }
                }
                assert!(request.starts_with("GET /v0/management/quota-scheduler/status "));
                assert!(
                    request
                        .to_ascii_lowercase()
                        .contains("authorization: bearer test-key\r\n")
                );
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            });
            assert_eq!(quota_endpoint_available(&url, &key), expected);
            server.join().unwrap();
        }
        assert!(!quota_endpoint_available(
            "http://127.0.0.1:1",
            &dir.join("missing")
        ));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn preserves_unmanaged_services() {
        let dir = temp_dir();
        let path = dir.join(UNIT);
        assert!(ensure_managed_unit(&path).is_ok());
        fs::write(&path, "[Service]\nExecStart=/unrelated\n").unwrap();
        assert!(ensure_managed_unit(&path).is_err());
        write_atomic(&path, &format!("{MARKER}[Service]\n")).unwrap();
        assert!(ensure_managed_unit(&path).is_ok());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn resolves_local_urls_for_wildcards_and_ipv6() {
        for (bind, expected) in [
            ("0.0.0.0:6736", "http://127.0.0.1:6736"),
            ("[::]:6736", "http://[::1]:6736"),
            ("[::1]:7000", "http://[::1]:7000"),
        ] {
            assert_eq!(local_url(bind.parse().unwrap()), expected);
        }
    }

    #[test]
    fn readiness_uses_native_health_without_a_key_or_provider_initialization() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = std::io::BufReader::new(&mut stream);
            let mut request = String::new();
            loop {
                let mut line = String::new();
                std::io::BufRead::read_line(&mut reader, &mut line).unwrap();
                request.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            assert!(!request.to_ascii_lowercase().contains("authorization:"));
            assert!(request.starts_with("GET /health "));
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{\"status\":\"ok\"}").unwrap();
        });
        wait_until_ready(&url).unwrap();
        server.join().unwrap();
    }
}
