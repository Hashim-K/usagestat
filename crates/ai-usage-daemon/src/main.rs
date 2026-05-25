use usagestat_core::{
    AppConfig, LoadedProvider, MetricLine, NormalizedMetrics, ProgressFormat, ProviderSummary,
    UsageCache, UsageSnapshot, paths,
};

const DASHBOARD_HTML: &str = include_str!("dashboard.html");
use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Duration as ChronoDuration, TimeZone, Utc};
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, json};
use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use usagestat_plugins::{discover_providers, probe_provider};

#[derive(Debug, Parser)]
#[command(name = "usagestatd")]
#[command(about = "Local agent usage polling daemon")]
struct Cli {
    #[arg(long, default_value = "127.0.0.1:6736")]
    bind: String,

    #[arg(long)]
    refresh_sec: Option<u64>,

    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,

    #[arg(long = "plugin-dir", value_name = "DIR")]
    plugin_dirs: Vec<PathBuf>,
}

#[derive(Debug, Default)]
struct AppState {
    cache: UsageCache,
    providers: Vec<ProviderSummary>,
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();
    let config_path = cli.config.clone().unwrap_or_else(paths::config_file);
    let config = AppConfig::load_optional(&config_path)
        .with_context(|| format!("load config {}", config_path.display()))?;
    let refresh_sec = cli.refresh_sec.unwrap_or(config.refresh_sec);
    let plugin_dirs = paths::plugin_dirs(&config, &cli.plugin_dirs);
    let cache_path = paths::cache_file();
    let history_path = paths::data_dir().join("history.jsonl");
    let cache = UsageCache::load_optional(&cache_path)
        .with_context(|| format!("load cache {}", cache_path.display()))?;

    let state = Arc::new(Mutex::new(AppState {
        cache,
        providers: Vec::new(),
    }));
    let refresh_flag = Arc::new(AtomicBool::new(false));

    start_poller(
        Arc::clone(&state),
        Arc::clone(&refresh_flag),
        config,
        plugin_dirs,
        cache_path,
        history_path,
        refresh_sec,
    );
    serve(&cli.bind, state, refresh_flag)
}

fn start_poller(
    state: Arc<Mutex<AppState>>,
    refresh_flag: Arc<AtomicBool>,
    config: AppConfig,
    plugin_dirs: Vec<PathBuf>,
    cache_path: PathBuf,
    history_path: PathBuf,
    refresh_sec: u64,
) {
    thread::spawn(move || {
        loop {
            let mut providers = discover_providers(&plugin_dirs);
            sort_providers(&mut providers, &config);
            let summaries = provider_summaries(&providers, &config);
            state.lock().expect("app state poisoned").providers = summaries;

            for provider in &providers {
                if config.is_enabled(&provider.manifest.id, provider.manifest.enabled_by_default) {
                    let source = config.source_mode(&provider.manifest.id);
                    let snapshot = probe_provider(
                        provider,
                        source,
                        config.provider_config(&provider.manifest.id),
                    );
                    let record = history_record_from_snapshot(&snapshot);
                    let mut guard = state.lock().expect("app state poisoned");
                    guard.cache.upsert(snapshot);
                    if let Err(e) = guard.cache.save(&cache_path) {
                        log::warn!("failed to save usage cache: {e}");
                    }
                    if let Err(e) = append_history_record(&history_path, &record) {
                        log::warn!("failed to append usage history: {e}");
                    }
                }
            }

            // Sleep until next cycle, but wake early if refresh is requested.
            refresh_flag.store(false, Ordering::Relaxed);
            let deadline = Instant::now() + Duration::from_secs(refresh_sec.max(1));
            loop {
                thread::sleep(Duration::from_millis(500));
                if refresh_flag.load(Ordering::Relaxed) {
                    break;
                }
                if Instant::now() >= deadline {
                    break;
                }
            }
        }
    });
}

fn serve(bind: &str, state: Arc<Mutex<AppState>>, refresh_flag: Arc<AtomicBool>) -> Result<()> {
    let listener = TcpListener::bind(bind)?;
    log::info!("listening on http://{bind}");

    for stream in listener.incoming() {
        let state = Arc::clone(&state);
        let flag = Arc::clone(&refresh_flag);
        match stream {
            Ok(stream) => {
                thread::spawn(move || handle_connection(stream, state, flag));
            }
            Err(e) => log::warn!("accept failed: {e}"),
        }
    }

    Ok(())
}

fn handle_connection(
    mut stream: TcpStream,
    state: Arc<Mutex<AppState>>,
    refresh_flag: Arc<AtomicBool>,
) {
    let mut buf = [0_u8; 4096];
    let Ok(n) = stream.read(&mut buf) else {
        return;
    };
    let request = String::from_utf8_lossy(&buf[..n]);
    let first_line = request.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let raw_path = parts.next().unwrap_or("/");
    let path = raw_path
        .split('?')
        .next()
        .unwrap_or(raw_path)
        .trim_end_matches('/');
    let path = if path.is_empty() { "/" } else { path };

    let response = route(method, path, &state, &refresh_flag);
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn route(
    method: &str,
    path: &str,
    state: &Arc<Mutex<AppState>>,
    refresh_flag: &Arc<AtomicBool>,
) -> String {
    if method == "OPTIONS" {
        return response_no_content();
    }

    // POST /v1/refresh — trigger immediate re-probe
    if method == "POST" && path == "/v1/refresh" {
        refresh_flag.store(true, Ordering::Relaxed);
        return response_json(200, "OK", r#"{"status":"refresh_scheduled"}"#);
    }

    if method != "GET" {
        return response_json(
            405,
            "Method Not Allowed",
            r#"{"error":"method_not_allowed"}"#,
        );
    }

    if path == "/health" {
        return response_json(200, "OK", r#"{"status":"ok"}"#);
    }

    if path == "/dashboard" || path == "/" {
        return response_html(200, "OK", DASHBOARD_HTML);
    }

    if path == "/v1/providers" {
        let providers = state.lock().expect("app state poisoned").providers.clone();
        let body = serde_json::to_string_pretty(&providers).unwrap_or_else(|_| "[]".into());
        return response_json(200, "OK", &body);
    }

    if path == "/v1/usage" {
        let guard = state.lock().expect("app state poisoned");
        let snapshots = ordered_snapshots(&guard);
        let body = serde_json::to_string_pretty(&snapshots).unwrap_or_else(|_| "[]".into());
        return response_json(200, "OK", &body);
    }

    if path == "/v1/history" {
        let body =
            serde_json::to_string_pretty(&read_history(None)).unwrap_or_else(|_| "[]".into());
        return response_json(200, "OK", &body);
    }

    if let Some(provider_id) = path.strip_prefix("/v1/cost/") {
        return serve_cost(provider_id);
    }

    if let Some(rest) = path
        .strip_prefix("/v1/local-usage/")
        .or_else(|| path.strip_prefix("/v1/ccusage/"))
    {
        return serve_local_usage_report(rest);
    }

    if let Some(provider_id) = path.strip_prefix("/v1/history/") {
        let body = serde_json::to_string_pretty(&read_history(Some(provider_id)))
            .unwrap_or_else(|_| "[]".into());
        return response_json(200, "OK", &body);
    }

    if let Some(provider_id) = path.strip_prefix("/v1/usage/") {
        let guard = state.lock().expect("app state poisoned");
        return match guard.cache.get(provider_id) {
            Some(snap) => {
                let body = serde_json::to_string_pretty(snap).unwrap_or_else(|_| "{}".into());
                response_json(200, "OK", &body)
            }
            None => response_json(404, "Not Found", r#"{"error":"provider_not_found"}"#),
        };
    }

    response_json(404, "Not Found", r#"{"error":"not_found"}"#)
}

fn response_json(status: u16, reason: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Connection: close\r\n\
         Content-Type: application/json; charset=utf-8\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type\r\n\
         Content-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

fn serve_cost(provider_id: &str) -> String {
    let canonical = match provider_id {
        "claude" | "anthropic" => "claude",
        "codex" | "openai" | "chatgpt" => "codex",
        _ => {
            return response_json(
                200,
                "OK",
                r#"{"error":{"code":"UNSUPPORTED","message":"Cost data not available for this provider"}}"#,
            );
        }
    };
    match scan_local_usage(canonical) {
        Ok(events) => {
            let daily = aggregate_usage(&events, Bucket::Day);
            let totals = daily
                .iter()
                .fold(ModelAggregate::default(), |mut acc, row| {
                    acc.input_tokens += row.input_tokens;
                    acc.output_tokens += row.output_tokens;
                    acc.cache_read_tokens += row.cache_read_tokens;
                    acc.cache_creation_tokens += row.cache_creation_tokens;
                    acc.reasoning_output_tokens += row.reasoning_output_tokens;
                    acc.total_tokens += row.total_tokens;
                    acc.cost_usd += row.cost_usd;
                    acc
                });
            let body = serde_json::to_string_pretty(&json!({
                "provider": canonical,
                "currency": "USD",
                "daily": daily,
                "totals": totals,
            }))
            .unwrap_or_else(|_| "{}".into());
            response_json(200, "OK", &body)
        }
        Err(_) => response_json(
            200,
            "OK",
            r#"{"error":{"code":"UNSUPPORTED","message":"Cost data not available for this provider"}}"#,
        ),
    }
}

fn serve_local_usage_report(rest: &str) -> String {
    let mut parts = rest.split('/');
    let provider_id = parts.next().unwrap_or_default();
    let report = parts.next().unwrap_or("daily");
    if parts.next().is_some() {
        return response_json(404, "Not Found", r#"{"error":"not_found"}"#);
    }
    let provider = match provider_id {
        "claude" | "anthropic" => "claude",
        "codex" | "openai" | "chatgpt" => "codex",
        _ => {
            return response_json(
                200,
                "OK",
                r#"{"error":{"code":"UNSUPPORTED","message":"Local usage reports are not available for this provider"}}"#,
            );
        }
    };
    if !matches!(
        report,
        "daily" | "weekly" | "monthly" | "session" | "blocks"
    ) {
        return response_json(
            400,
            "Bad Request",
            r#"{"error":{"code":"BAD_REPORT","message":"Unsupported usage report"}}"#,
        );
    }

    match local_usage_report(provider, report) {
        Ok(body) => response_json(200, "OK", &body),
        Err(e) => {
            log::warn!("local usage report failed: {e}");
            response_json(
                200,
                "OK",
                r#"{"error":{"code":"UNAVAILABLE","message":"Local usage report unavailable"}}"#,
            )
        }
    }
}

fn local_usage_report(provider: &str, report: &str) -> Result<String> {
    let events = scan_local_usage(provider)?;
    let value = match report {
        "daily" => json!({ "daily": aggregate_usage(&events, Bucket::Day) }),
        "weekly" => json!({ "weekly": aggregate_usage(&events, Bucket::Week) }),
        "monthly" => json!({ "monthly": aggregate_usage(&events, Bucket::Month) }),
        "session" => json!({ "sessions": aggregate_sessions(&events) }),
        "blocks" => json!({ "blocks": aggregate_blocks(&events) }),
        _ => {
            return Ok(
                r#"{"error":{"code":"BAD_REPORT","message":"Unsupported usage report"}}"#
                    .to_string(),
            );
        }
    };
    Ok(serde_json::to_string_pretty(&value)?)
}

#[derive(Debug, Clone, Default)]
struct LocalUsageEvent {
    ts: DateTime<Utc>,
    session_id: String,
    project: String,
    model: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
    reasoning_output_tokens: u64,
    cost_usd: f64,
}

#[derive(Copy, Clone)]
enum Bucket {
    Day,
    Week,
    Month,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageAggregate {
    date: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
    reasoning_output_tokens: u64,
    total_tokens: u64,
    cost_usd: f64,
    models: HashMap<String, ModelAggregate>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelAggregate {
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
    reasoning_output_tokens: u64,
    total_tokens: u64,
    cost_usd: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionAggregate {
    session_id: String,
    project: String,
    last_activity: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
    reasoning_output_tokens: u64,
    total_tokens: u64,
    cost_usd: f64,
    models: HashMap<String, ModelAggregate>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct BlockAggregate {
    block_start: String,
    block_end: String,
    active: bool,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
    reasoning_output_tokens: u64,
    total_tokens: u64,
    cost_usd: f64,
    models: HashMap<String, ModelAggregate>,
}

fn scan_local_usage(provider: &str) -> Result<Vec<LocalUsageEvent>> {
    match provider {
        "claude" => scan_claude_usage(),
        "codex" => scan_codex_usage(),
        _ => Ok(Vec::new()),
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn scan_claude_usage() -> Result<Vec<LocalUsageEvent>> {
    let Some(home) = home_dir() else {
        return Ok(Vec::new());
    };
    let roots = [
        home.join(".claude/projects"),
        home.join("Library/Developer/Xcode/CodingAssistant/ClaudeAgentConfig/projects"),
    ];
    let mut events = Vec::new();
    for root in roots {
        for file in jsonl_files(&root) {
            scan_claude_file(&file, &mut events)?;
        }
    }
    Ok(events)
}

fn scan_claude_file(path: &Path, events: &mut Vec<LocalUsageEvent>) -> Result<()> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return Ok(()),
    };
    let fallback_session = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let fallback_project = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .map(project_from_slug)
        .unwrap_or_else(|| "unknown".to_string());
    for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(v) = serde_json::from_str::<JsonValue>(&line) else {
            continue;
        };
        let Some(usage) = v.pointer("/message/usage") else {
            continue;
        };
        let Some(ts) = parse_ts(v.get("timestamp")) else {
            continue;
        };
        let model = v
            .pointer("/message/model")
            .and_then(JsonValue::as_str)
            .unwrap_or("unknown")
            .to_string();
        let input = json_u64_value(usage, &["input_tokens", "inputTokens"]);
        let output = json_u64_value(usage, &["output_tokens", "outputTokens"]);
        let cache_read =
            json_u64_value(usage, &["cache_read_input_tokens", "cacheReadInputTokens"]);
        let cache_creation = json_u64_value(
            usage,
            &["cache_creation_input_tokens", "cacheCreationInputTokens"],
        );
        if input + output + cache_read + cache_creation == 0 {
            continue;
        }
        let session_id = v
            .get("sessionId")
            .and_then(JsonValue::as_str)
            .unwrap_or(&fallback_session)
            .to_string();
        let project = v
            .get("cwd")
            .and_then(JsonValue::as_str)
            .map(project_label)
            .unwrap_or_else(|| fallback_project.clone());
        let cost = estimate_cost_usd(&model, input, output, cache_creation, cache_read);
        events.push(LocalUsageEvent {
            ts,
            session_id,
            project,
            model,
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: cache_read,
            cache_creation_tokens: cache_creation,
            reasoning_output_tokens: 0,
            cost_usd: cost,
        });
    }
    Ok(())
}

fn scan_codex_usage() -> Result<Vec<LocalUsageEvent>> {
    let Some(home) = home_dir() else {
        return Ok(Vec::new());
    };
    let roots = [
        home.join(".codex/sessions"),
        home.join(".codex/archived_sessions"),
    ];
    let mut events = Vec::new();
    for root in roots {
        for file in jsonl_files(&root) {
            scan_codex_file(&file, &mut events)?;
        }
    }
    Ok(events)
}

fn scan_codex_file(path: &Path, events: &mut Vec<LocalUsageEvent>) -> Result<()> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return Ok(()),
    };
    let fallback_session = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .trim_start_matches("rollout-")
        .to_string();
    let mut session_id = fallback_session;
    let mut model = "unknown".to_string();
    let mut project = "unknown".to_string();
    for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(v) = serde_json::from_str::<JsonValue>(&line) else {
            continue;
        };
        if let Some(id) = v.pointer("/payload/id").and_then(JsonValue::as_str) {
            session_id = id.to_string();
        }
        if let Some(m) = v
            .pointer("/payload/model")
            .or_else(|| v.pointer("/payload/model_slug"))
            .and_then(JsonValue::as_str)
        {
            model = m.to_string();
        }
        if let Some(cwd) = v.pointer("/payload/cwd").and_then(JsonValue::as_str) {
            project = project_label(cwd);
        }
        let usage = v
            .pointer("/payload/info/last_token_usage")
            .or_else(|| v.pointer("/payload/last_token_usage"));
        let Some(usage) = usage else {
            continue;
        };
        let Some(ts) = parse_ts(v.get("timestamp")) else {
            continue;
        };
        let input = json_u64_value(usage, &["input_tokens", "inputTokens"]);
        let output = json_u64_value(usage, &["output_tokens", "outputTokens"]);
        let cache_read = json_u64_value(
            usage,
            &[
                "cached_input_tokens",
                "cachedInputTokens",
                "cache_read_input_tokens",
            ],
        );
        let reasoning =
            json_u64_value(usage, &["reasoning_output_tokens", "reasoningOutputTokens"]);
        if input + output + cache_read + reasoning == 0 {
            continue;
        }
        let cost = estimate_cost_usd(&model, input, output, 0, cache_read);
        events.push(LocalUsageEvent {
            ts,
            session_id: session_id.clone(),
            project: project.clone(),
            model: model.clone(),
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: cache_read,
            cache_creation_tokens: 0,
            reasoning_output_tokens: reasoning,
            cost_usd: cost,
        });
    }
    Ok(())
}

fn jsonl_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_jsonl_files(root, &mut out);
    out
}

fn collect_jsonl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl_files(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            out.push(path);
        }
    }
}

fn parse_ts(value: Option<&JsonValue>) -> Option<DateTime<Utc>> {
    match value? {
        JsonValue::String(s) => DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc)),
        JsonValue::Number(n) => n
            .as_i64()
            .and_then(|secs| Utc.timestamp_opt(secs, 0).single()),
        _ => None,
    }
}

fn json_u64_value(value: &JsonValue, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(JsonValue::as_u64))
        .unwrap_or(0)
}

fn project_from_slug(slug: &str) -> String {
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.replace('-', "/")
    }
}

fn project_label(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn estimate_cost_usd(
    model: &str,
    input: u64,
    output: u64,
    cache_creation: u64,
    cache_read: u64,
) -> f64 {
    let m = model.to_ascii_lowercase();
    let (input_rate, output_rate, cache_write_rate, cache_read_rate) = if m.contains("opus") {
        (5.0, 25.0, 6.25, 0.50)
    } else if m.contains("haiku") {
        (1.0, 5.0, 1.25, 0.10)
    } else if m.contains("sonnet") || m.contains("claude") {
        (3.0, 15.0, 3.75, 0.30)
    } else if m.contains("gpt-5") || m.contains("codex") {
        (1.25, 10.0, 1.25, 0.125)
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };
    (input as f64 * input_rate
        + output as f64 * output_rate
        + cache_creation as f64 * cache_write_rate
        + cache_read as f64 * cache_read_rate)
        / 1_000_000.0
}

fn aggregate_usage(events: &[LocalUsageEvent], bucket: Bucket) -> Vec<UsageAggregate> {
    let mut map: HashMap<String, UsageAggregate> = HashMap::new();
    for event in events {
        let key = bucket_key_for(event.ts, bucket);
        let row = map.entry(key.clone()).or_insert_with(|| UsageAggregate {
            date: key,
            ..UsageAggregate::default()
        });
        add_event_to_usage(row, event);
    }
    let mut rows: Vec<_> = map.into_values().collect();
    rows.sort_by(|a, b| b.date.cmp(&a.date));
    rows
}

fn aggregate_sessions(events: &[LocalUsageEvent]) -> Vec<SessionAggregate> {
    let mut map: HashMap<String, SessionAggregate> = HashMap::new();
    for event in events {
        let row = map
            .entry(event.session_id.clone())
            .or_insert_with(|| SessionAggregate {
                session_id: event.session_id.clone(),
                project: event.project.clone(),
                ..SessionAggregate::default()
            });
        if row.last_activity.is_empty() || row.last_activity < event.ts.to_rfc3339() {
            row.last_activity = event.ts.to_rfc3339();
        }
        add_event_to_session(row, event);
    }
    let mut rows: Vec<_> = map.into_values().collect();
    rows.sort_by(|a, b| b.cost_usd.total_cmp(&a.cost_usd));
    rows
}

fn aggregate_blocks(events: &[LocalUsageEvent]) -> Vec<BlockAggregate> {
    let mut map: HashMap<i64, BlockAggregate> = HashMap::new();
    let now = Utc::now();
    for event in events {
        let start = event.ts.timestamp() / (5 * 3600) * (5 * 3600);
        let start_dt = Utc.timestamp_opt(start, 0).single().unwrap_or(event.ts);
        let end_dt = start_dt + ChronoDuration::hours(5);
        let row = map.entry(start).or_insert_with(|| BlockAggregate {
            block_start: start_dt.to_rfc3339(),
            block_end: end_dt.to_rfc3339(),
            active: now >= start_dt && now < end_dt,
            ..BlockAggregate::default()
        });
        add_event_to_block(row, event);
    }
    let mut rows: Vec<_> = map.into_values().collect();
    rows.sort_by(|a, b| b.block_start.cmp(&a.block_start));
    rows
}

fn bucket_key_for(ts: DateTime<Utc>, bucket: Bucket) -> String {
    match bucket {
        Bucket::Day => ts.format("%Y-%m-%d").to_string(),
        Bucket::Month => ts.format("%Y-%m").to_string(),
        Bucket::Week => {
            let date = ts.date_naive();
            let monday = date - ChronoDuration::days(date.weekday().num_days_from_monday() as i64);
            monday.format("%Y-%m-%d").to_string()
        }
    }
}

fn add_model(models: &mut HashMap<String, ModelAggregate>, event: &LocalUsageEvent) {
    let model = models.entry(event.model.clone()).or_default();
    model.input_tokens += event.input_tokens;
    model.output_tokens += event.output_tokens;
    model.cache_read_tokens += event.cache_read_tokens;
    model.cache_creation_tokens += event.cache_creation_tokens;
    model.reasoning_output_tokens += event.reasoning_output_tokens;
    model.total_tokens += event.input_tokens
        + event.output_tokens
        + event.cache_read_tokens
        + event.cache_creation_tokens
        + event.reasoning_output_tokens;
    model.cost_usd += event.cost_usd;
}

fn add_event_to_usage(row: &mut UsageAggregate, event: &LocalUsageEvent) {
    row.input_tokens += event.input_tokens;
    row.output_tokens += event.output_tokens;
    row.cache_read_tokens += event.cache_read_tokens;
    row.cache_creation_tokens += event.cache_creation_tokens;
    row.reasoning_output_tokens += event.reasoning_output_tokens;
    row.total_tokens += event.input_tokens
        + event.output_tokens
        + event.cache_read_tokens
        + event.cache_creation_tokens
        + event.reasoning_output_tokens;
    row.cost_usd += event.cost_usd;
    add_model(&mut row.models, event);
}

fn add_event_to_session(row: &mut SessionAggregate, event: &LocalUsageEvent) {
    row.input_tokens += event.input_tokens;
    row.output_tokens += event.output_tokens;
    row.cache_read_tokens += event.cache_read_tokens;
    row.cache_creation_tokens += event.cache_creation_tokens;
    row.reasoning_output_tokens += event.reasoning_output_tokens;
    row.total_tokens += event.input_tokens
        + event.output_tokens
        + event.cache_read_tokens
        + event.cache_creation_tokens
        + event.reasoning_output_tokens;
    row.cost_usd += event.cost_usd;
    add_model(&mut row.models, event);
}

fn add_event_to_block(row: &mut BlockAggregate, event: &LocalUsageEvent) {
    row.input_tokens += event.input_tokens;
    row.output_tokens += event.output_tokens;
    row.cache_read_tokens += event.cache_read_tokens;
    row.cache_creation_tokens += event.cache_creation_tokens;
    row.reasoning_output_tokens += event.reasoning_output_tokens;
    row.total_tokens += event.input_tokens
        + event.output_tokens
        + event.cache_read_tokens
        + event.cache_creation_tokens
        + event.reasoning_output_tokens;
    row.cost_usd += event.cost_usd;
    add_model(&mut row.models, event);
}

fn response_html(status: u16, reason: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Connection: close\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

fn response_no_content() -> String {
    "HTTP/1.1 204 No Content\r\n\
     Connection: close\r\n\
     Access-Control-Allow-Origin: *\r\n\
     Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
     Access-Control-Allow-Headers: Content-Type\r\n\r\n"
        .to_string()
}

fn ordered_snapshots(state: &AppState) -> Vec<UsageSnapshot> {
    let mut snapshots = Vec::new();
    for provider in &state.providers {
        if !provider.enabled {
            continue;
        }
        if let Some(snapshot) = state.cache.get(&provider.id) {
            snapshots.push(snapshot.clone());
        }
    }
    let seen: std::collections::HashSet<String> = snapshots
        .iter()
        .map(|snapshot| snapshot.provider_id.clone())
        .collect();
    for snapshot in state.cache.list() {
        if !seen.contains(&snapshot.provider_id) {
            snapshots.push(snapshot);
        }
    }
    snapshots
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotRecord {
    ts: String,
    #[serde(alias = "provider_id")]
    provider_id: String,
    #[serde(alias = "display_name")]
    display_name: String,
    plan: Option<String>,
    #[serde(alias = "primary_percent")]
    primary_percent: f64,
    #[serde(alias = "input_tokens")]
    input_tokens: Option<u64>,
    #[serde(alias = "output_tokens")]
    output_tokens: Option<u64>,
    #[serde(default, alias = "cache_read_tokens")]
    cache_read_tokens: Option<u64>,
    #[serde(default, alias = "cache_creation_tokens")]
    cache_creation_tokens: Option<u64>,
    #[serde(default, alias = "total_tokens")]
    total_tokens: Option<u64>,
    cost: Option<f64>,
    #[serde(alias = "reset_time")]
    reset_time: Option<String>,
    #[serde(default)]
    progress: Vec<HistoryProgressRecord>,
    #[serde(default)]
    text: Vec<HistoryTextRecord>,
    #[serde(default)]
    badges: Vec<HistoryBadgeRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryProgressRecord {
    label: String,
    used: f64,
    limit: f64,
    percent: Option<f64>,
    format: String,
    suffix: Option<String>,
    resets_at: Option<String>,
    period_duration_ms: Option<u64>,
    color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryTextRecord {
    label: String,
    value: String,
    color: Option<String>,
    subtitle: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryBadgeRecord {
    label: String,
    text: String,
    color: Option<String>,
    subtitle: Option<String>,
}

fn history_record_from_snapshot(snapshot: &UsageSnapshot) -> SnapshotRecord {
    let metrics = NormalizedMetrics::from_snapshot(snapshot);
    let progress = progress_history(snapshot);
    let text = text_history(snapshot);
    let badges = badge_history(snapshot);
    let token_breakdown = token_breakdown(snapshot, &metrics);
    SnapshotRecord {
        ts: snapshot.fetched_at.to_rfc3339(),
        provider_id: snapshot.provider_id.clone(),
        display_name: snapshot.display_name.clone(),
        plan: snapshot.plan.clone(),
        primary_percent: metrics.primary_percent,
        input_tokens: token_breakdown.input.or(metrics.input_tokens),
        output_tokens: token_breakdown.output.or(metrics.output_tokens),
        cache_read_tokens: token_breakdown.cache_read,
        cache_creation_tokens: token_breakdown.cache_creation,
        total_tokens: token_breakdown.total.or_else(|| {
            Some(
                [
                    token_breakdown.input.or(metrics.input_tokens),
                    token_breakdown.output.or(metrics.output_tokens),
                    token_breakdown.cache_read,
                    token_breakdown.cache_creation,
                ]
                .into_iter()
                .flatten()
                .sum(),
            )
            .filter(|total| *total > 0)
        }),
        cost: metrics.cost,
        reset_time: metrics.reset_time,
        progress,
        text,
        badges,
    }
}

fn progress_history(snapshot: &UsageSnapshot) -> Vec<HistoryProgressRecord> {
    snapshot
        .metrics
        .iter()
        .filter_map(|metric| match metric {
            MetricLine::Progress {
                label,
                used,
                limit,
                format,
                resets_at,
                period_duration_ms,
                color,
            } => {
                let (format_name, suffix) = match format {
                    ProgressFormat::Percent => ("percent".to_string(), None),
                    ProgressFormat::Dollars => ("dollars".to_string(), None),
                    ProgressFormat::Count { suffix } => ("count".to_string(), Some(suffix.clone())),
                };
                Some(HistoryProgressRecord {
                    label: label.clone(),
                    used: *used,
                    limit: *limit,
                    percent: (*limit > 0.0).then(|| (*used / *limit * 100.0).clamp(0.0, 100.0)),
                    format: format_name,
                    suffix,
                    resets_at: resets_at.map(|dt| dt.to_rfc3339()),
                    period_duration_ms: *period_duration_ms,
                    color: color.clone(),
                })
            }
            _ => None,
        })
        .collect()
}

fn text_history(snapshot: &UsageSnapshot) -> Vec<HistoryTextRecord> {
    snapshot
        .metrics
        .iter()
        .filter_map(|metric| match metric {
            MetricLine::Text {
                label,
                value,
                color,
                subtitle,
            } => Some(HistoryTextRecord {
                label: label.clone(),
                value: value.clone(),
                color: color.clone(),
                subtitle: subtitle.clone(),
            }),
            _ => None,
        })
        .collect()
}

fn badge_history(snapshot: &UsageSnapshot) -> Vec<HistoryBadgeRecord> {
    snapshot
        .metrics
        .iter()
        .filter_map(|metric| match metric {
            MetricLine::Badge {
                label,
                text,
                color,
                subtitle,
            } => Some(HistoryBadgeRecord {
                label: label.clone(),
                text: text.clone(),
                color: color.clone(),
                subtitle: subtitle.clone(),
            }),
            _ => None,
        })
        .collect()
}

#[derive(Default)]
struct TokenBreakdown {
    input: Option<u64>,
    output: Option<u64>,
    cache_read: Option<u64>,
    cache_creation: Option<u64>,
    total: Option<u64>,
}

fn token_breakdown(snapshot: &UsageSnapshot, metrics: &NormalizedMetrics) -> TokenBreakdown {
    let mut out = TokenBreakdown {
        input: metrics.input_tokens,
        output: metrics.output_tokens,
        ..TokenBreakdown::default()
    };

    for metric in &snapshot.metrics {
        match metric {
            MetricLine::Progress {
                label,
                used,
                format,
                ..
            } => {
                if matches!(format, ProgressFormat::Count { .. }) {
                    assign_token_value(&mut out, label, *used);
                }
            }
            MetricLine::Text { label, value, .. } => {
                if let Some(value) = parse_u64_loose(value) {
                    assign_token_u64(&mut out, label, value);
                }
            }
            MetricLine::Badge { label, text, .. } => {
                if let Some(value) = parse_u64_loose(text) {
                    assign_token_u64(&mut out, label, value);
                }
            }
        }
    }

    let total = [out.input, out.output, out.cache_read, out.cache_creation]
        .into_iter()
        .flatten()
        .sum::<u64>();
    if total > 0 {
        out.total = Some(total);
    }
    out
}

fn assign_token_value(out: &mut TokenBreakdown, label: &str, value: f64) {
    if value.is_finite() && value >= 0.0 {
        assign_token_u64(out, label, value.min(u64::MAX as f64) as u64);
    }
}

fn assign_token_u64(out: &mut TokenBreakdown, label: &str, value: u64) {
    let label = label.to_lowercase();
    if !(label.contains("token") || label.contains("tok")) {
        return;
    }
    if label.contains("cache") && (label.contains("read") || label.contains("hit")) {
        out.cache_read = Some(value);
    } else if label.contains("cache")
        && (label.contains("write") || label.contains("creation") || label.contains("create"))
    {
        out.cache_creation = Some(value);
    } else if label.contains("output") || label.contains("completion") {
        out.output = Some(value);
    } else if label.contains("input") || label.contains("prompt") {
        out.input = Some(value);
    } else if label.contains("total") {
        out.total = Some(value);
    }
}

fn parse_u64_loose(value: &str) -> Option<u64> {
    let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

fn append_history_record(path: &std::path::Path, record: &SnapshotRecord) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create dir {}", parent.display()))?;
    }
    let line = serde_json::to_string(record).context("serialize history record")?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open {}", path.display()))?;
    writeln!(file, "{line}").with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn read_history(provider_id: Option<&str>) -> Vec<SnapshotRecord> {
    let path = paths::data_dir().join("history.jsonl");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };

    text.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<SnapshotRecord>(line).ok())
        .filter(|record| {
            provider_id
                .map(|id| record.provider_id.eq_ignore_ascii_case(id))
                .unwrap_or(true)
        })
        .collect()
}

fn provider_summaries(providers: &[LoadedProvider], config: &AppConfig) -> Vec<ProviderSummary> {
    providers
        .iter()
        .map(|p| ProviderSummary {
            id: p.manifest.id.clone(),
            name: p.manifest.name.clone(),
            enabled: config.is_enabled(&p.manifest.id, p.manifest.enabled_by_default),
            supported_modes: p.manifest.supported_modes.clone(),
            auto_mode: p.manifest.auto_mode.clone(),
            web_url: p.manifest.web_url.clone(),
            status_page_url: p.manifest.resolved_status_page_url(),
            usage_dashboard_url: p.manifest.resolved_usage_dashboard_url(),
            icon: p.manifest.resolved_icon(&p.dir),
        })
        .collect()
}

fn sort_providers(providers: &mut [LoadedProvider], config: &AppConfig) {
    let order: HashMap<&str, usize> = config
        .providers
        .iter()
        .enumerate()
        .map(|(index, provider)| (provider.id.as_str(), index))
        .collect();
    providers.sort_by(|a, b| {
        let ao = order.get(a.manifest.id.as_str()).copied();
        let bo = order.get(b.manifest.id.as_str()).copied();
        match (ao, bo) {
            (Some(a_index), Some(b_index)) => a_index.cmp(&b_index),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.manifest.id.cmp(&b.manifest.id),
        }
    });
}
