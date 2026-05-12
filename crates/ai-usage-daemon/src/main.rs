use ai_usage_core::{AppConfig, LoadedProvider, ProviderSummary, UsageCache, paths};
use ai_usage_plugins::{discover_providers, probe_provider};
use anyhow::{Context, Result};
use clap::Parser;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Parser)]
#[command(name = "ai-usage-daemon")]
#[command(about = "Local AI usage polling daemon")]
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
    refresh_sec: u64,
) {
    thread::spawn(move || {
        loop {
            let providers = discover_providers(&plugin_dirs);
            let summaries = provider_summaries(&providers, &config);
            state.lock().expect("app state poisoned").providers = summaries;

            for provider in &providers {
                if config.is_enabled(&provider.manifest.id, provider.manifest.enabled_by_default) {
                    let snapshot = probe_provider(provider);
                    let mut guard = state.lock().expect("app state poisoned");
                    guard.cache.upsert(snapshot);
                    if let Err(e) = guard.cache.save(&cache_path) {
                        log::warn!("failed to save usage cache: {e}");
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

fn serve(
    bind: &str,
    state: Arc<Mutex<AppState>>,
    refresh_flag: Arc<AtomicBool>,
) -> Result<()> {
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
        return response_json(405, "Method Not Allowed", r#"{"error":"method_not_allowed"}"#);
    }

    if path == "/health" {
        return response_json(200, "OK", r#"{"status":"ok"}"#);
    }

    if path == "/v1/providers" {
        let providers = state.lock().expect("app state poisoned").providers.clone();
        let body = serde_json::to_string_pretty(&providers).unwrap_or_else(|_| "[]".into());
        return response_json(200, "OK", &body);
    }

    if path == "/v1/usage" {
        let snapshots = state.lock().expect("app state poisoned").cache.list();
        let body = serde_json::to_string_pretty(&snapshots).unwrap_or_else(|_| "[]".into());
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

fn response_no_content() -> String {
    "HTTP/1.1 204 No Content\r\n\
     Connection: close\r\n\
     Access-Control-Allow-Origin: *\r\n\
     Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
     Access-Control-Allow-Headers: Content-Type\r\n\r\n"
        .to_string()
}

fn provider_summaries(providers: &[LoadedProvider], config: &AppConfig) -> Vec<ProviderSummary> {
    providers
        .iter()
        .map(|p| ProviderSummary {
            id: p.manifest.id.clone(),
            name: p.manifest.name.clone(),
            enabled: config.is_enabled(&p.manifest.id, p.manifest.enabled_by_default),
        })
        .collect()
}
