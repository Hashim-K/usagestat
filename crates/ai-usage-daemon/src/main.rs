use ai_usage_core::{UsageCache, paths};
use ai_usage_plugins::{discover_providers, probe_provider};
use anyhow::Result;
use clap::Parser;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(name = "ai-usage-daemon")]
#[command(about = "Local AI usage polling daemon")]
struct Cli {
    #[arg(long, default_value = "127.0.0.1:6736")]
    bind: String,

    #[arg(long, default_value_t = 60)]
    refresh_sec: u64,
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();
    let cache = Arc::new(Mutex::new(UsageCache::default()));

    start_poller(Arc::clone(&cache), cli.refresh_sec);
    serve(&cli.bind, cache)
}

fn start_poller(cache: Arc<Mutex<UsageCache>>, refresh_sec: u64) {
    thread::spawn(move || {
        loop {
            let providers = discover_providers(&paths::plugin_dirs());
            for provider in providers {
                let snapshot = probe_provider(&provider);
                cache.lock().expect("usage cache poisoned").upsert(snapshot);
            }
            thread::sleep(Duration::from_secs(refresh_sec));
        }
    });
}

fn serve(bind: &str, cache: Arc<Mutex<UsageCache>>) -> Result<()> {
    let listener = TcpListener::bind(bind)?;
    log::info!("listening on http://{bind}");

    for stream in listener.incoming() {
        let cache = Arc::clone(&cache);
        match stream {
            Ok(stream) => {
                thread::spawn(move || handle_connection(stream, cache));
            }
            Err(error) => log::warn!("accept failed: {error}"),
        }
    }

    Ok(())
}

fn handle_connection(mut stream: TcpStream, cache: Arc<Mutex<UsageCache>>) {
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

    let response = route(method, path, &cache);
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn route(method: &str, path: &str, cache: &Arc<Mutex<UsageCache>>) -> String {
    if method == "OPTIONS" {
        return response_no_content();
    }
    if method != "GET" {
        return response_json(
            405,
            "Method Not Allowed",
            r#"{"error":"method_not_allowed"}"#,
        );
    }

    if path == "/v1/usage" {
        let snapshots = cache.lock().expect("usage cache poisoned").list();
        let body = serde_json::to_string_pretty(&snapshots).unwrap_or_else(|_| "[]".to_string());
        return response_json(200, "OK", &body);
    }

    if let Some(provider_id) = path.strip_prefix("/v1/usage/") {
        let guard = cache.lock().expect("usage cache poisoned");
        return match guard.get(provider_id) {
            Some(snapshot) => {
                let body =
                    serde_json::to_string_pretty(snapshot).unwrap_or_else(|_| "{}".to_string());
                response_json(200, "OK", &body)
            }
            None => response_json(404, "Not Found", r#"{"error":"provider_not_found"}"#),
        };
    }

    response_json(404, "Not Found", r#"{"error":"not_found"}"#)
}

fn response_json(status: u16, reason: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} {reason}\r\nConnection: close\r\nContent-Type: application/json; charset=utf-8\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

fn response_no_content() -> String {
    "HTTP/1.1 204 No Content\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\n\r\n".to_string()
}
