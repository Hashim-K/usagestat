use usagestat_core::{
    AppConfig, LoadedProvider, MetricLine, NormalizedMetrics, ProgressFormat, ProviderSummary,
    UsageCache, UsageSnapshot, paths,
};

const DASHBOARD_HTML: &str = include_str!("dashboard.html");
use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
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
    let output = std::process::Command::new("codexbar")
        .args(["cost", "--provider", canonical, "--format", "json"])
        .output();
    match output {
        Ok(o) if !o.stdout.is_empty() => {
            let body = String::from_utf8_lossy(&o.stdout).into_owned();
            response_json(200, "OK", &body)
        }
        _ => response_json(
            200,
            "OK",
            r#"{"error":{"code":"UNSUPPORTED","message":"Cost data not available for this provider"}}"#,
        ),
    }
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
