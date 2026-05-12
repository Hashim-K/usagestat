use rquickjs::{Ctx, Exception, Function, Object};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

const ENV_ALLOWLIST: &[&str] = &[
    "AI_USAGE_PLUGIN_DIR",
    "CODEX_HOME",
    "CLAUDE_CONFIG_DIR",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CURSOR_HOME",
    "DEEPSEEK_API_KEY",
    "DEEPSEEK_KEY",
    "GEMINI_API_KEY",
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GLM_API_KEY",
    "COPILOT_API_TOKEN",
    "COPILOT_USAGE_URL",
    "OPENAI_API_KEY",
    "OPENROUTER_API_KEY",
    "OPENROUTER_API_BASE",
    "ZAI_API_KEY",
    "ZAI_API_TOKEN",
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HttpRequest {
    url: String,
    #[serde(default = "default_method")]
    method: String,
    #[serde(default)]
    headers: HashMap<String, String>,
    #[serde(default)]
    body_text: Option<String>,
    #[serde(default = "default_timeout_ms")]
    timeout_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HttpResponse {
    status: u16,
    headers: HashMap<String, String>,
    body_text: String,
}

pub fn inject<'js>(
    ctx: &Ctx<'js>,
    probe_ctx: &Object<'js>,
    plugin_id: &str,
) -> rquickjs::Result<()> {
    let host = Object::new(ctx.clone())?;
    inject_log(ctx, &host, plugin_id)?;
    inject_env(ctx, &host)?;
    inject_fs(ctx, &host)?;
    inject_http(ctx, &host)?;
    probe_ctx.set("host", host)?;
    patch_http_wrapper(ctx)?;
    inject_utils(ctx)?;
    Ok(())
}

fn inject_log<'js>(ctx: &Ctx<'js>, host: &Object<'js>, plugin_id: &str) -> rquickjs::Result<()> {
    let log_obj = Object::new(ctx.clone())?;

    let pid = plugin_id.to_string();
    log_obj.set(
        "info",
        Function::new(ctx.clone(), move |message: String| {
            log::info!("[plugin:{pid}] {}", redact_log_message(&message));
        })?,
    )?;

    let pid = plugin_id.to_string();
    log_obj.set(
        "warn",
        Function::new(ctx.clone(), move |message: String| {
            log::warn!("[plugin:{pid}] {}", redact_log_message(&message));
        })?,
    )?;

    let pid = plugin_id.to_string();
    log_obj.set(
        "error",
        Function::new(ctx.clone(), move |message: String| {
            log::error!("[plugin:{pid}] {}", redact_log_message(&message));
        })?,
    )?;

    host.set("log", log_obj)?;
    Ok(())
}

fn inject_env<'js>(ctx: &Ctx<'js>, host: &Object<'js>) -> rquickjs::Result<()> {
    let env_obj = Object::new(ctx.clone())?;
    env_obj.set(
        "get",
        Function::new(ctx.clone(), move |name: String| -> Option<String> {
            if !ENV_ALLOWLIST.contains(&name.as_str()) {
                return None;
            }
            std::env::var(name).ok().filter(|value| !value.is_empty())
        })?,
    )?;
    host.set("env", env_obj)?;
    Ok(())
}

fn inject_fs<'js>(ctx: &Ctx<'js>, host: &Object<'js>) -> rquickjs::Result<()> {
    let fs_obj = Object::new(ctx.clone())?;

    if let Some(home) = home_dir() {
        fs_obj.set("homeDir", home.to_string_lossy().to_string())?;
    }

    fs_obj.set(
        "exists",
        Function::new(ctx.clone(), move |path: String| -> bool {
            expand_path(&path).exists()
        })?,
    )?;

    fs_obj.set(
        "readText",
        Function::new(
            ctx.clone(),
            move |ctx_inner: Ctx<'_>, path: String| -> rquickjs::Result<String> {
                let path = expand_path(&path);
                std::fs::read_to_string(&path)
                    .map_err(|error| Exception::throw_message(&ctx_inner, &error.to_string()))
            },
        )?,
    )?;

    fs_obj.set(
        "writeText",
        Function::new(
            ctx.clone(),
            move |ctx_inner: Ctx<'_>, path: String, content: String| -> rquickjs::Result<()> {
                let path = expand_path(&path);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|error| {
                        Exception::throw_message(&ctx_inner, &error.to_string())
                    })?;
                }
                std::fs::write(&path, content)
                    .map_err(|error| Exception::throw_message(&ctx_inner, &error.to_string()))
            },
        )?,
    )?;

    fs_obj.set(
        "listDir",
        Function::new(
            ctx.clone(),
            move |ctx_inner: Ctx<'_>, path: String| -> rquickjs::Result<Vec<String>> {
                let path = expand_path(&path);
                let entries = std::fs::read_dir(&path)
                    .map_err(|error| Exception::throw_message(&ctx_inner, &error.to_string()))?;
                let mut names = Vec::new();
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if !name.is_empty() {
                        names.push(name);
                    }
                }
                names.sort();
                Ok(names)
            },
        )?,
    )?;

    host.set("fs", fs_obj)?;
    Ok(())
}

fn inject_http<'js>(ctx: &Ctx<'js>, host: &Object<'js>) -> rquickjs::Result<()> {
    let http_obj = Object::new(ctx.clone())?;

    http_obj.set(
        "_requestRaw",
        Function::new(
            ctx.clone(),
            move |ctx_inner: Ctx<'_>, req_json: String| -> rquickjs::Result<String> {
                let request: HttpRequest = serde_json::from_str(&req_json).map_err(|error| {
                    Exception::throw_message(&ctx_inner, &format!("invalid request: {error}"))
                })?;
                let response = execute_http_request(request).map_err(|error| {
                    Exception::throw_message(&ctx_inner, &format!("http request failed: {error}"))
                })?;
                serde_json::to_string(&response)
                    .map_err(|error| Exception::throw_message(&ctx_inner, &error.to_string()))
            },
        )?,
    )?;

    host.set("http", http_obj)?;
    Ok(())
}

fn patch_http_wrapper(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    ctx.eval::<(), _>(
        r#"
        (function() {
            var raw = __ai_usage_ctx.host.http._requestRaw;
            __ai_usage_ctx.host.http.request = function(req) {
                var response = raw(JSON.stringify({
                    url: req.url,
                    method: req.method || "GET",
                    headers: req.headers || {},
                    bodyText: req.bodyText || null,
                    timeoutMs: req.timeoutMs || 10000
                }));
                return JSON.parse(response);
            };
        })();
        "#
        .as_bytes(),
    )
}

fn inject_utils(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    ctx.eval::<(), _>(
        r#"
        (function() {
            var ctx = __ai_usage_ctx;

            ctx.line = {
                text: function(opts) {
                    var line = { type: "text", label: opts.label, value: opts.value };
                    if (opts.color) line.color = opts.color;
                    if (opts.subtitle) line.subtitle = opts.subtitle;
                    return line;
                },
                badge: function(opts) {
                    var line = { type: "badge", label: opts.label, text: opts.text };
                    if (opts.color) line.color = opts.color;
                    if (opts.subtitle) line.subtitle = opts.subtitle;
                    return line;
                },
                progress: function(opts) {
                    var line = {
                        type: "progress",
                        label: opts.label,
                        used: opts.used,
                        limit: opts.limit,
                        format: opts.format || { kind: "percent" }
                    };
                    if (opts.resetsAt) line.resetsAt = opts.resetsAt;
                    if (opts.periodDurationMs) line.periodDurationMs = opts.periodDurationMs;
                    if (opts.color) line.color = opts.color;
                    return line;
                }
            };

            ctx.util = {
                tryParseJson: function(text) {
                    try { return JSON.parse(text); } catch (_) { return null; }
                },
                request: function(req) {
                    return ctx.host.http.request(req);
                },
                requestJson: function(req) {
                    var resp = ctx.host.http.request(req);
                    var json = null;
                    try { json = resp.bodyText ? JSON.parse(resp.bodyText) : null; } catch (_) {}
                    return { resp: resp, json: json };
                },
                isAuthStatus: function(status) {
                    return status === 401 || status === 403;
                },
                parseDateMs: function(value) {
                    var ms = Date.parse(value);
                    return Number.isFinite(ms) ? ms : null;
                },
                toIso: function(value) {
                    if (value === null || value === undefined) return null;
                    var n = Number(value);
                    if (!Number.isFinite(n)) return null;
                    return new Date(n).toISOString();
                },
                retryOnceOnAuth: function(opts) {
                    var first = opts.request(null);
                    if (!ctx.util.isAuthStatus(first.status)) return first;
                    var refreshed = opts.refresh();
                    if (!refreshed) return first;
                    return opts.request(refreshed);
                }
            };

            ctx.fmt = {
                planLabel: function(value) {
                    var text = String(value || "").trim();
                    if (!text) return "";
                    text = text.replace(/[_-]+/g, " ").replace(/\s+/g, " ").trim();
                    return text.replace(/(^|\s)([a-z])/g, function(match, space, letter) {
                        return space + letter.toUpperCase();
                    });
                }
            };

            ctx.jwt = {
                decodePayload: function(token) {
                    if (typeof token !== "string") return null;
                    var parts = token.split(".");
                    if (parts.length < 2) return null;
                    var b64 = parts[1].replace(/-/g, "+").replace(/_/g, "/");
                    while (b64.length % 4) b64 += "=";
                    try {
                        return JSON.parse(atob(b64));
                    } catch (_) {
                        return null;
                    }
                }
            };
        })();
        "#
        .as_bytes(),
    )
}

fn execute_http_request(request: HttpRequest) -> Result<HttpResponse, reqwest::Error> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(request.timeout_ms))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let method = request
        .method
        .parse::<reqwest::Method>()
        .unwrap_or(reqwest::Method::GET);
    let mut builder = client.request(method, request.url);

    for (name, value) in request.headers {
        builder = builder.header(name, value);
    }

    if let Some(body) = request.body_text {
        builder = builder.body(body);
    }

    let response = builder.send()?;
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .filter_map(|(name, value)| Some((name.to_string(), value.to_str().ok()?.to_string())))
        .collect();
    let body_text = response.text()?;

    Ok(HttpResponse {
        status,
        headers,
        body_text,
    })
}

fn expand_path(path: &str) -> PathBuf {
    if path == "~" {
        return home_dir().unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

fn default_method() -> String {
    "GET".to_string()
}

fn default_timeout_ms() -> u64 {
    10_000
}

fn redact_log_message(message: &str) -> String {
    let mut out = message.to_string();
    for marker in ["sk-", "pk-", "api_", "key_", "secret_"] {
        if let Some(index) = out.find(marker) {
            let end = out[index..]
                .find(|ch: char| ch.is_whitespace() || ch == '"' || ch == '\'')
                .map(|offset| index + offset)
                .unwrap_or(out.len());
            out.replace_range(index..end, "[REDACTED]");
        }
    }
    out
}
