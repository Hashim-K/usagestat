use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CCUSAGE_VERSION: &str = "18.0.10";
const CCUSAGE_TIMEOUT_SECS: u64 = 30;
const CCUSAGE_POLL_INTERVAL_MS: u64 = 100;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CcusageQueryOpts {
    pub provider: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub home_path: Option<String>,
    pub claude_path: Option<String>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CcusageProvider {
    Claude,
    Codex,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum CcusageRunnerKind {
    Bunx,
    PnpmDlx,
    YarnDlx,
    NpmExec,
    Npx,
}

#[derive(Debug, Eq, PartialEq)]
enum CcusageRunnerResult {
    Success(String),
    Failed,
    TimedOut,
}

#[derive(Copy, Clone)]
struct CcusageProviderConfig {
    package_name: &'static str,
    npm_exec_bin: &'static str,
    home_env_var: &'static str,
}

pub fn parse_provider(value: &str) -> Option<CcusageProvider> {
    match value.trim().to_ascii_lowercase().as_str() {
        "claude" | "anthropic" => Some(CcusageProvider::Claude),
        "codex" | "openai" | "chatgpt" => Some(CcusageProvider::Codex),
        _ => None,
    }
}

pub fn provider_id(provider: CcusageProvider) -> &'static str {
    match provider {
        CcusageProvider::Claude => "claude",
        CcusageProvider::Codex => "codex",
    }
}

pub fn resolve_provider(opts: &CcusageQueryOpts, plugin_id: &str) -> CcusageProvider {
    opts.provider
        .as_deref()
        .and_then(parse_provider)
        .or_else(|| parse_provider(plugin_id))
        .unwrap_or(CcusageProvider::Claude)
}

pub fn query_status_json(opts: &CcusageQueryOpts, plugin_id: &str) -> String {
    let provider = resolve_provider(opts, plugin_id);
    let runners = collect_runners();
    if runners.is_empty() {
        return serde_json::json!({ "status": "no_runner" }).to_string();
    }

    for (kind, program) in runners {
        match run_with_runner(kind, &program, opts, provider) {
            CcusageRunnerResult::Success(result) => {
                let Ok(data) = serde_json::from_str::<JsonValue>(&result) else {
                    continue;
                };
                return serde_json::json!({ "status": "ok", "data": data }).to_string();
            }
            CcusageRunnerResult::Failed => {}
            CcusageRunnerResult::TimedOut => {
                return serde_json::json!({ "status": "runner_failed" }).to_string();
            }
        }
    }

    serde_json::json!({ "status": "runner_failed" }).to_string()
}

pub fn query_daily(opts: &CcusageQueryOpts, plugin_id: &str) -> Result<JsonValue, String> {
    let status_json = query_status_json(opts, plugin_id);
    let status: JsonValue = serde_json::from_str(&status_json).map_err(|e| e.to_string())?;
    match status.get("status").and_then(|v| v.as_str()) {
        Some("ok") => status
            .get("data")
            .cloned()
            .ok_or_else(|| "missing ccusage data".to_string()),
        Some(other) => Err(other.to_string()),
        None => Err("invalid ccusage response".to_string()),
    }
}

fn provider_config(provider: CcusageProvider) -> CcusageProviderConfig {
    match provider {
        CcusageProvider::Claude => CcusageProviderConfig {
            package_name: "ccusage",
            npm_exec_bin: "ccusage",
            home_env_var: "CLAUDE_CONFIG_DIR",
        },
        CcusageProvider::Codex => CcusageProviderConfig {
            package_name: "@ccusage/codex",
            npm_exec_bin: "ccusage-codex",
            home_env_var: "CODEX_HOME",
        },
    }
}

fn package_spec(provider: CcusageProvider) -> String {
    let config = provider_config(provider);
    format!("{}@{}", config.package_name, CCUSAGE_VERSION)
}

fn runner_order() -> [CcusageRunnerKind; 5] {
    [
        CcusageRunnerKind::Bunx,
        CcusageRunnerKind::PnpmDlx,
        CcusageRunnerKind::YarnDlx,
        CcusageRunnerKind::NpmExec,
        CcusageRunnerKind::Npx,
    ]
}

fn runner_candidates(kind: CcusageRunnerKind) -> Vec<String> {
    let mut candidates = Vec::new();
    match kind {
        CcusageRunnerKind::Bunx => {
            if let Some(home) = dirs::home_dir() {
                candidates.push(home.join(".bun/bin/bunx").to_string_lossy().to_string());
            }
            candidates.extend(
                ["/opt/homebrew/bin/bunx", "/usr/local/bin/bunx", "bunx"].map(String::from),
            );
        }
        CcusageRunnerKind::PnpmDlx => {
            candidates.extend(
                ["/opt/homebrew/bin/pnpm", "/usr/local/bin/pnpm", "pnpm"].map(String::from),
            );
        }
        CcusageRunnerKind::YarnDlx => {
            candidates.extend(
                ["/opt/homebrew/bin/yarn", "/usr/local/bin/yarn", "yarn"].map(String::from),
            );
        }
        CcusageRunnerKind::NpmExec => {
            candidates
                .extend(["/opt/homebrew/bin/npm", "/usr/local/bin/npm", "npm"].map(String::from));
        }
        CcusageRunnerKind::Npx => {
            candidates
                .extend(["/opt/homebrew/bin/npx", "/usr/local/bin/npx", "npx"].map(String::from));
        }
    }

    let mut unique = Vec::new();
    for candidate in candidates {
        if !candidate.is_empty() && !unique.iter().any(|seen| seen == &candidate) {
            unique.push(candidate);
        }
    }
    unique
}

fn path_entries_with(home: Option<&Path>, existing_path: Option<&OsStr>) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    if let Some(home) = home {
        entries.push(home.join(".bun/bin"));
        entries.push(home.join(".nvm/current/bin"));
        entries.extend(nvm_node_bin_paths(home));
        entries.push(home.join(".local/bin"));
    }
    entries.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    if let Some(existing_path) = existing_path {
        entries.extend(std::env::split_paths(existing_path));
    }

    let mut unique = Vec::new();
    for entry in entries {
        if !entry.as_os_str().is_empty() && !unique.iter().any(|seen| seen == &entry) {
            unique.push(entry);
        }
    }
    unique
}

fn nvm_node_bin_paths(home: &Path) -> Vec<PathBuf> {
    let nvm_dir = home.join(".nvm");
    let Some(version) = resolve_nvm_alias(&nvm_dir, "default", 0) else {
        return Vec::new();
    };
    vec![nvm_dir.join("versions/node").join(version).join("bin")]
}

fn resolve_nvm_alias(nvm_dir: &Path, alias: &str, depth: usize) -> Option<String> {
    if depth > 4 {
        return None;
    }
    let raw = std::fs::read_to_string(nvm_dir.join("alias").join(alias)).ok()?;
    let value = raw
        .lines()
        .next()
        .unwrap_or_default()
        .split('#')
        .next()
        .unwrap_or_default()
        .trim();
    if value.is_empty() {
        return None;
    }
    if value.starts_with('v') {
        return Some(value.to_string());
    }
    resolve_nvm_alias(nvm_dir, value, depth + 1)
}

fn enriched_path() -> Option<OsString> {
    let home = dirs::home_dir();
    let existing_path = std::env::var_os("PATH");
    std::env::join_paths(path_entries_with(home.as_deref(), existing_path.as_deref())).ok()
}

fn runner_available(candidate: &str, enriched_path: Option<&OsStr>) -> bool {
    let mut command = Command::new(candidate);
    command
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(path) = enriched_path {
        command.env("PATH", path);
    }
    command.status().map(|s| s.success()).unwrap_or(false)
}

fn collect_runners() -> Vec<(CcusageRunnerKind, String)> {
    let path = enriched_path();
    let mut runners = Vec::new();
    for kind in runner_order() {
        for candidate in runner_candidates(kind) {
            if runner_available(&candidate, path.as_deref()) {
                runners.push((kind, candidate));
                break;
            }
        }
    }
    runners
}

fn append_common_args(args: &mut Vec<String>, opts: &CcusageQueryOpts) {
    args.extend([
        "daily".to_string(),
        "--json".to_string(),
        "--order".to_string(),
        "desc".to_string(),
    ]);

    if let Some(since) = opts
        .since
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        args.push("--since".to_string());
        args.push(since.to_string());
    }
    if let Some(until) = opts
        .until
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        args.push("--until".to_string());
        args.push(until.to_string());
    }
}

fn runner_args(
    kind: CcusageRunnerKind,
    opts: &CcusageQueryOpts,
    provider: CcusageProvider,
) -> Vec<String> {
    let config = provider_config(provider);
    let package = package_spec(provider);
    let mut args = match kind {
        CcusageRunnerKind::Bunx => vec!["--silent".to_string(), package],
        CcusageRunnerKind::PnpmDlx => vec!["-s".to_string(), "dlx".to_string(), package],
        CcusageRunnerKind::YarnDlx => vec!["dlx".to_string(), "-q".to_string(), package],
        CcusageRunnerKind::NpmExec => vec![
            "exec".to_string(),
            "--yes".to_string(),
            format!("--package={package}"),
            "--".to_string(),
            config.npm_exec_bin.to_string(),
        ],
        CcusageRunnerKind::Npx => vec!["--yes".to_string(), package],
    };
    append_common_args(&mut args, opts);
    args
}

fn home_override(opts: &CcusageQueryOpts) -> Option<&str> {
    opts.home_path
        .as_deref()
        .or(opts.claude_path.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn configure_command(command: &mut Command, args: &[String], path: Option<&OsStr>) {
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = path {
        command.env("PATH", path);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
}

fn run_with_runner(
    kind: CcusageRunnerKind,
    program: &str,
    opts: &CcusageQueryOpts,
    provider: CcusageProvider,
) -> CcusageRunnerResult {
    let args = runner_args(kind, opts, provider);
    let path = enriched_path();
    let mut command = Command::new(program);
    configure_command(&mut command, &args, path.as_deref());
    if let Some(home_path) = home_override(opts) {
        command.env(
            provider_config(provider).home_env_var,
            expand_home(home_path),
        );
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => return CcusageRunnerResult::Failed,
    };

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = child.wait_with_output();
                let Ok(output) = output else {
                    return CcusageRunnerResult::Failed;
                };
                if status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    return normalize_output(&stdout)
                        .map(CcusageRunnerResult::Success)
                        .unwrap_or(CcusageRunnerResult::Failed);
                }
                return CcusageRunnerResult::Failed;
            }
            Ok(None) if start.elapsed() > Duration::from_secs(CCUSAGE_TIMEOUT_SECS) => {
                terminate_child(&mut child);
                return CcusageRunnerResult::TimedOut;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(CCUSAGE_POLL_INTERVAL_MS)),
            Err(_) => return CcusageRunnerResult::Failed,
        }
    }
}

fn terminate_child(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pgid = format!("-{}", child.id());
        let _ = Command::new("kill")
            .arg("-KILL")
            .arg(pgid)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn expand_home(path: &str) -> String {
    if path == "~" {
        return dirs::home_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    path.to_string()
}

fn extract_last_json_value(stdout: &str) -> Option<String> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return None;
    }
    if serde_json::from_str::<JsonValue>(trimmed).is_ok() {
        return Some(trimmed.to_string());
    }
    let mut starts: Vec<usize> = trimmed
        .char_indices()
        .filter(|(_, c)| *c == '{' || *c == '[')
        .map(|(idx, _)| idx)
        .collect();
    starts.reverse();
    for start in starts {
        let candidate = trimmed[start..].trim();
        if serde_json::from_str::<JsonValue>(candidate).is_ok() {
            return Some(candidate.to_string());
        }
    }
    None
}

fn normalize_output(stdout: &str) -> Option<String> {
    let json_value = extract_last_json_value(stdout)?;
    let parsed: JsonValue = serde_json::from_str(&json_value).ok()?;
    let normalized = match parsed {
        JsonValue::Array(daily) => serde_json::json!({ "daily": daily }),
        JsonValue::Object(map) => {
            let daily = map.get("daily")?;
            if !daily.is_array() {
                return None;
            }
            JsonValue::Object(map)
        }
        _ => return None,
    };
    serde_json::to_string(&normalized).ok()
}
