use crate::ccusage::{CcusageQueryOpts, query_status_json};
use hmac::{Hmac, Mac};
use rquickjs::{Ctx, Exception, Function, Object, function::Rest};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const ENV_ALLOWLIST: &[&str] = &[
    "USAGESTAT_PLUGIN_DIR",
    "AI_USAGE_PLUGIN_DIR",
    "ABACUS_COOKIE",
    "ALIBABA_CODING_PLAN_API_KEY",
    "ALIBABA_CODING_PLAN_COOKIE",
    "ALIBABA_COOKIE",
    "ALIBABA_TOKEN_PLAN_COOKIE",
    "APPDATA",
    "ARK_API_KEY",
    "AUGMENT_ACCESS_TOKEN",
    "AZURE_OPENAI_API_KEY",
    "AZURE_OPENAI_API_VERSION",
    "AZURE_OPENAI_DEPLOYMENT",
    "AZURE_OPENAI_DEPLOYMENT_NAME",
    "AZURE_OPENAI_ENDPOINT",
    "AWS_ACCESS_KEY_ID",
    "AWS_DEFAULT_REGION",
    "AWS_PROFILE",
    "AWS_REGION",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "CODEBUFF_API_KEY",
    "CODEX_HOME",
    "CODEX_REFRESH_URL",
    "CODEX_USAGE_URL",
    "CODEXBAR_BEDROCK_API_URL",
    "CODEXBAR_BEDROCK_BUDGET",
    "CLAUDE_AI_SESSION_KEY",
    "CLAUDE_CONFIG_DIR",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CLAUDE_WEB_SESSION_KEY",
    "CLOUDSDK_CONFIG",
    "CHUTES_API_KEY",
    "CHUTES_API_URL",
    "COMMAND_CODE_API_KEY",
    "COMMAND_CODE_COOKIE",
    "COMMANDCODE_COOKIE",
    "CROF_API_KEY",
    "CROFAI_API_KEY",
    "CURSOR_AGENT_HOME",
    "CURSOR_HOME",
    "DEEPGRAM_API_KEY",
    "DEEPGRAM_PROJECT_ID",
    "DEEPSEEK_API_KEY",
    "DEEPSEEK_KEY",
    "DOUBAO_API_KEY",
    "DROID_COOKIE",
    "ELEVENLABS_API_KEY",
    "ELEVENLABS_API_URL",
    "FACTORY_COOKIE",
    "FIREWORKS_API_KEY",
    "GEMINI_API_KEY",
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GLM_API_KEY",
    "GOOGLE_APPLICATION_CREDENTIALS",
    "COPILOT_API_TOKEN",
    "COPILOT_USAGE_URL",
    "GROK_COOKIE",
    "GROQ_API_KEY",
    "GROQ_API_URL",
    "GROQCLOUD_API_KEY",
    "KILO_API_KEY",
    "KIMI_API_KEY",
    "LLMPROXY_API_KEY",
    "LLM_PROXY_API_KEY",
    "LLM_PROXY_API_URL",
    "LLM_PROXY_BASE_URL",
    "LITELLM_API_KEY",
    "LITELLM_BASE_URL",
    "LOCALAPPDATA",
    "MANUS_COOKIE",
    "MANUS_SESSION_TOKEN",
    "MIMO_API_URL",
    "MIMO_COOKIE",
    "MISTRAL_COOKIE",
    "MOONSHOT_API_KEY",
    "MOONSHOT_API_URL",
    "MOONSHOT_KEY",
    "MOONSHOT_REGION",
    "NANOGPT_API_KEY",
    "NEURALWATT_API_KEY",
    "OLLAMA_COOKIE",
    "OPENAI_API_KEY",
    "OPENAI_PLATFORM_API_KEY",
    "OPENCODE_COOKIE",
    "OPENROUTER_API_KEY",
    "OPENROUTER_API_BASE",
    "POE_API_KEY",
    "SRC_ACCESS_TOKEN",
    "STEPFUN_COOKIE",
    "STEPFUN_OASIS_TOKEN",
    "STEPFUN_PASSWORD",
    "STEPFUN_TOKEN",
    "STEPFUN_USERNAME",
    "T3_CHAT_COOKIE",
    "T3CHAT_COOKIE",
    "USAGESTAT_BEDROCK_API_URL",
    "USAGESTAT_BEDROCK_BUDGET",
    "VENICE_API_KEY",
    "VOLCENGINE_API_KEY",
    "WARP_API_KEY",
    "ZED_ACCESS_TOKEN",
    "ZED_SERVER_URL",
    "ZED_USER_ID",
    "XI_API_KEY",
    "XIAOMI_MIMO_COOKIE",
    "XDG_CONFIG_HOME",
    "ZAI_API_KEY",
    "ZAI_API_TOKEN",
];

const FIRECTL_TIMEOUT_SECS: u64 = 15;
const FIRECTL_POLL_INTERVAL_MS: u64 = 50;
const COMMAND_OUTPUT_LIMIT_BYTES: usize = 1024 * 1024;
const COMMAND_POLL_INTERVAL_MS: u64 = 50;
const AWS_COST_EXPLORER_REGION: &str = "us-east-1";
const AWS_COST_EXPLORER_SERVICE: &str = "ce";
const AWS_COST_EXPLORER_TARGET: &str = "AWSInsightsIndexService.GetCostAndUsage";
const AWS_COST_EXPLORER_DEFAULT_URL: &str = "https://ce.us-east-1.amazonaws.com";

type HmacSha256 = Hmac<Sha256>;

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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AwsCostExplorerRequest {
    #[serde(default)]
    access_key_id: String,
    #[serde(default)]
    secret_access_key: String,
    #[serde(default)]
    session_token: Option<String>,
    #[serde(default)]
    api_url: Option<String>,
    start_date: String,
    end_date: String,
    #[serde(default = "default_cost_explorer_granularity")]
    granularity: String,
    #[serde(default)]
    next_page_token: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AwsCostExplorerResponse {
    status: u16,
    body_text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpsTestResult {
    pub url: String,
    pub status: u16,
    pub body_bytes: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandRequest {
    program: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default = "default_command_timeout_ms")]
    timeout_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandResponse {
    status: i32,
    stdout: String,
    stderr: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct KeychainPasswordItem {
    account: String,
    password: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LsDiscoverRequest {
    process_name: String,
    #[serde(default)]
    markers: Vec<String>,
    #[serde(default)]
    csrf_flag: Option<String>,
    #[serde(default)]
    port_flag: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LsDiscoverResponse {
    csrf: String,
    ports: Vec<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    extension_port: Option<u16>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FireworksBillingExportRequest {
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    account_id: String,
    #[serde(default)]
    start_time: String,
    #[serde(default)]
    end_time: String,
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
    inject_keychain(ctx, &host, plugin_id)?;
    inject_ls(ctx, &host)?;
    inject_http(ctx, &host)?;
    inject_command(ctx, &host)?;
    inject_aws(ctx, &host)?;
    inject_sqlite(ctx, &host)?;
    inject_ccusage(ctx, &host, plugin_id)?;
    inject_usage_daily(ctx, &host, plugin_id)?;
    inject_cursor_logs(ctx, &host)?;
    inject_cursor_usage_export(ctx, &host)?;
    inject_fireworks(ctx, &host, plugin_id)?;
    probe_ctx.set("host", host)?;
    patch_http_wrapper(ctx)?;
    patch_ls_wrapper(ctx)?;
    patch_ccusage_wrapper(ctx)?;
    patch_usage_daily_wrapper(ctx)?;
    patch_cursor_logs_wrapper(ctx)?;
    patch_cursor_usage_export_wrapper(ctx)?;
    patch_fireworks_wrapper(ctx)?;
    inject_utils(ctx)?;
    Ok(())
}

pub fn test_https_request(url: &str, timeout_ms: u64) -> Result<HttpsTestResult, String> {
    let response = execute_http_request(HttpRequest {
        url: url.to_string(),
        method: "GET".to_string(),
        headers: HashMap::new(),
        body_text: None,
        timeout_ms,
    })
    .map_err(|error| error.to_string())?;

    Ok(HttpsTestResult {
        url: url.to_string(),
        status: response.status,
        body_bytes: response.body_text.len(),
    })
}

fn inject_command<'js>(ctx: &Ctx<'js>, host: &Object<'js>) -> rquickjs::Result<()> {
    let command_obj = Object::new(ctx.clone())?;

    command_obj.set(
        "_runRaw",
        Function::new(
            ctx.clone(),
            move |ctx_inner: Ctx<'_>, req_json: String| -> rquickjs::Result<String> {
                let request: CommandRequest = serde_json::from_str(&req_json).map_err(|error| {
                    Exception::throw_message(
                        &ctx_inner,
                        &format!("invalid command request: {error}"),
                    )
                })?;
                let response = execute_command_request(request)
                    .map_err(|error| Exception::throw_message(&ctx_inner, &error))?;
                serde_json::to_string(&response)
                    .map_err(|error| Exception::throw_message(&ctx_inner, &error.to_string()))
            },
        )?,
    )?;

    host.set("command", command_obj)?;
    Ok(())
}

fn inject_aws<'js>(ctx: &Ctx<'js>, host: &Object<'js>) -> rquickjs::Result<()> {
    let aws_obj = Object::new(ctx.clone())?;

    aws_obj.set(
        "_costExplorerRaw",
        Function::new(
            ctx.clone(),
            move |ctx_inner: Ctx<'_>, req_json: String| -> rquickjs::Result<String> {
                let request: AwsCostExplorerRequest =
                    serde_json::from_str(&req_json).map_err(|error| {
                        Exception::throw_message(
                            &ctx_inner,
                            &format!("invalid AWS Cost Explorer request: {error}"),
                        )
                    })?;
                let response = execute_aws_cost_explorer_request(request)
                    .map_err(|error| Exception::throw_message(&ctx_inner, &error))?;
                serde_json::to_string(&response)
                    .map_err(|error| Exception::throw_message(&ctx_inner, &error.to_string()))
            },
        )?,
    )?;

    host.set("aws", aws_obj)?;
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

    fs_obj.set(
        "firstExisting",
        Function::new(ctx.clone(), move |paths: Vec<String>| -> Option<String> {
            first_existing_path(&paths)
        })?,
    )?;

    fs_obj.set(
        "firstExistingAppSupport",
        Function::new(ctx.clone(), move |relative: String| -> Option<String> {
            first_existing_app_support_path(&relative)
        })?,
    )?;

    host.set("fs", fs_obj)?;
    Ok(())
}

fn inject_keychain<'js>(
    ctx: &Ctx<'js>,
    host: &Object<'js>,
    plugin_id: &str,
) -> rquickjs::Result<()> {
    let keychain_obj = Object::new(ctx.clone())?;

    let pid_read = plugin_id.to_string();
    keychain_obj.set(
        "readGenericPassword",
        Function::new(
            ctx.clone(),
            move |ctx_inner: Ctx<'_>,
                  service: String,
                  account_args: Rest<Option<String>>|
                  -> rquickjs::Result<String> {
                let account = account_args
                    .0
                    .into_iter()
                    .next()
                    .flatten()
                    .and_then(|value| non_empty_trimmed(&value));
                log_keychain_read(&pid_read, &service, account.as_deref());
                platform_keychain_read(&service, account.as_deref()).map_err(|error| {
                    Exception::throw_message(
                        &ctx_inner,
                        &format!("keychain item not found: {error}"),
                    )
                })
            },
        )?,
    )?;

    let pid_read_current = plugin_id.to_string();
    keychain_obj.set(
        "readGenericPasswordForCurrentUser",
        Function::new(
            ctx.clone(),
            move |ctx_inner: Ctx<'_>, service: String| -> rquickjs::Result<String> {
                let account = current_keychain_account();
                log_keychain_read(&pid_read_current, &service, Some(&account));
                platform_keychain_read(&service, Some(&account)).map_err(|error| {
                    Exception::throw_message(
                        &ctx_inner,
                        &format!("keychain item not found: {error}"),
                    )
                })
            },
        )?,
    )?;

    let pid_read_generic_item = plugin_id.to_string();
    keychain_obj.set(
        "readGenericPasswordItem",
        Function::new(
            ctx.clone(),
            move |ctx_inner: Ctx<'_>, service: String| -> rquickjs::Result<String> {
                log::info!(
                    "[plugin:{pid_read_generic_item}] keychain generic item read: service={service}"
                );
                let item = platform_keychain_read_generic_item(&service).map_err(|error| {
                    Exception::throw_message(
                        &ctx_inner,
                        &format!("keychain item not found: {error}"),
                    )
                })?;
                serde_json::to_string(&item).map_err(|error| {
                    Exception::throw_message(
                        &ctx_inner,
                        &format!("keychain item serialization failed: {error}"),
                    )
                })
            },
        )?,
    )?;

    let pid_read_internet = plugin_id.to_string();
    keychain_obj.set(
        "readInternetPassword",
        Function::new(
            ctx.clone(),
            move |ctx_inner: Ctx<'_>, server: String| -> rquickjs::Result<String> {
                log::info!(
                    "[plugin:{pid_read_internet}] keychain internet password read: server={server}"
                );
                let item = platform_keychain_read_internet_password(&server).map_err(|error| {
                    Exception::throw_message(
                        &ctx_inner,
                        &format!("keychain item not found: {error}"),
                    )
                })?;
                serde_json::to_string(&item).map_err(|error| {
                    Exception::throw_message(
                        &ctx_inner,
                        &format!("keychain item serialization failed: {error}"),
                    )
                })
            },
        )?,
    )?;

    let pid_write = plugin_id.to_string();
    keychain_obj.set(
        "writeGenericPassword",
        Function::new(
            ctx.clone(),
            move |ctx_inner: Ctx<'_>, service: String, value: String| -> rquickjs::Result<()> {
                log::info!("[plugin:{pid_write}] keychain write: service={service}");
                platform_keychain_write(&service, None, &value).map_err(|error| {
                    Exception::throw_message(&ctx_inner, &format!("keychain write failed: {error}"))
                })
            },
        )?,
    )?;

    let pid_write_current = plugin_id.to_string();
    keychain_obj.set(
        "writeGenericPasswordForCurrentUser",
        Function::new(
            ctx.clone(),
            move |ctx_inner: Ctx<'_>, service: String, value: String| -> rquickjs::Result<()> {
                let account = current_keychain_account();
                log::info!(
                    "[plugin:{pid_write_current}] keychain write: service={service}, account={}",
                    redact_value(&account)
                );
                platform_keychain_write(&service, Some(&account), &value).map_err(|error| {
                    Exception::throw_message(&ctx_inner, &format!("keychain write failed: {error}"))
                })
            },
        )?,
    )?;

    let pid_write_account = plugin_id.to_string();
    keychain_obj.set(
        "writeGenericPasswordForAccount",
        Function::new(
            ctx.clone(),
            move |ctx_inner: Ctx<'_>,
                  service: String,
                  account: String,
                  value: String|
                  -> rquickjs::Result<()> {
                let Some(account) = non_empty_trimmed(&account) else {
                    return Err(Exception::throw_message(
                        &ctx_inner,
                        "keychain account must not be empty",
                    ));
                };
                log::info!(
                    "[plugin:{pid_write_account}] keychain write: service={service}, account={}",
                    redact_value(&account)
                );
                platform_keychain_write(&service, Some(&account), &value).map_err(|error| {
                    Exception::throw_message(&ctx_inner, &format!("keychain write failed: {error}"))
                })
            },
        )?,
    )?;

    let pid_delete = plugin_id.to_string();
    keychain_obj.set(
        "deleteGenericPassword",
        Function::new(
            ctx.clone(),
            move |ctx_inner: Ctx<'_>,
                  service: String,
                  account_args: Rest<Option<String>>|
                  -> rquickjs::Result<()> {
                let account = account_args
                    .0
                    .into_iter()
                    .next()
                    .flatten()
                    .and_then(|value| non_empty_trimmed(&value));
                log::info!(
                    "[plugin:{pid_delete}] keychain delete: service={service}, account={}",
                    account
                        .as_deref()
                        .map(redact_value)
                        .unwrap_or_else(|| "default".to_string())
                );
                platform_keychain_delete(&service, account.as_deref()).map_err(|error| {
                    Exception::throw_message(
                        &ctx_inner,
                        &format!("keychain delete failed: {error}"),
                    )
                })
            },
        )?,
    )?;

    host.set("keychain", keychain_obj)?;
    Ok(())
}

fn inject_ls<'js>(ctx: &Ctx<'js>, host: &Object<'js>) -> rquickjs::Result<()> {
    let ls_obj = Object::new(ctx.clone())?;
    ls_obj.set(
        "_discoverRaw",
        Function::new(
            ctx.clone(),
            move |ctx_inner: Ctx<'_>, req_json: String| -> rquickjs::Result<String> {
                let request: LsDiscoverRequest =
                    serde_json::from_str(&req_json).map_err(|error| {
                        Exception::throw_message(
                            &ctx_inner,
                            &format!("invalid language-server discovery request: {error}"),
                        )
                    })?;
                let response = discover_language_server(&request);
                serde_json::to_string(&response)
                    .map_err(|error| Exception::throw_message(&ctx_inner, &error.to_string()))
            },
        )?,
    )?;
    host.set("ls", ls_obj)?;
    Ok(())
}

fn patch_ls_wrapper(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    ctx.eval::<(), _>(
        r#"
        (function() {
            if (!__usagestat_ctx.host.ls || !__usagestat_ctx.host.ls._discoverRaw) return;
            var rawFn = __usagestat_ctx.host.ls._discoverRaw;
            __usagestat_ctx.host.ls.discover = function(opts) {
                var response = rawFn(JSON.stringify(opts || {}));
                if (response === "null") return null;
                try { return JSON.parse(response); } catch (_) { return null; }
            };
        })();
        "#
        .as_bytes(),
    )
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
            var raw = __usagestat_ctx.host.http._requestRaw;
            __usagestat_ctx.host.http.request = function(req) {
                var response = raw(JSON.stringify({
                    url: req.url,
                    method: req.method || "GET",
                    headers: req.headers || {},
                    bodyText: req.bodyText || null,
                    timeoutMs: req.timeoutMs || 10000
                }));
                return JSON.parse(response);
            };
            if (__usagestat_ctx.host.command && __usagestat_ctx.host.command._runRaw) {
                var runRaw = __usagestat_ctx.host.command._runRaw;
                __usagestat_ctx.host.command.run = function(req) {
                    var response = runRaw(JSON.stringify({
                        program: req.program,
                        args: req.args || [],
                        timeoutMs: req.timeoutMs || 10000
                    }));
                    return JSON.parse(response);
                };
            }
        })();
        "#
        .as_bytes(),
    )
}

fn inject_utils(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    ctx.eval::<(), _>(
        r#"
        (function() {
            var ctx = __usagestat_ctx;

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
                    if (opts.detail) line.detail = opts.detail;
                    if (opts.color) line.color = opts.color;
                    return line;
                },
                barChart: function(opts) {
                    var line = { type: "barChart", label: opts.label, points: opts.points || [] };
                    if (opts.note) line.note = opts.note;
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
                    if (typeof value === "string") {
                        var parsed = Date.parse(value);
                        if (Number.isFinite(parsed)) return new Date(parsed).toISOString();
                    }
                    var n = Number(value);
                    if (!Number.isFinite(n)) return null;
                    if (Math.abs(n) < 10000000000) n = n * 1000;
                    return new Date(n).toISOString();
                },
                retryOnceOnAuth: function(opts) {
                    var first = opts.request(null);
                    if (!ctx.util.isAuthStatus(first.status)) return first;
                    var refreshed = opts.refresh();
                    if (!refreshed) return first;
                    return opts.request(refreshed);
                },
                needsRefreshByExpiry: function(opts) {
                    if (!opts) return true;
                    if (opts.expiresAtMs === null || opts.expiresAtMs === undefined) return true;
                    var nowMs = Number(opts.nowMs);
                    var expiresAtMs = Number(opts.expiresAtMs);
                    var bufferMs = Number(opts && opts.bufferMs);
                    if (!Number.isFinite(nowMs)) return true;
                    if (!Number.isFinite(expiresAtMs)) return true;
                    if (!Number.isFinite(bufferMs)) bufferMs = 0;
                    return nowMs + bufferMs >= expiresAtMs;
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
                },
                dollars: function(value) {
                    var n = Number(value);
                    if (!Number.isFinite(n)) return 0;
                    return Math.round(n * 100) / 100;
                }
            };

            var b64chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            ctx.base64 = {
                decode: function(str) {
                    str = String(str || "").replace(/-/g, "+").replace(/_/g, "/");
                    while (str.length % 4) str += "=";
                    str = str.replace(/=+$/, "");
                    var result = "";
                    var len = str.length;
                    var i = 0;
                    while (i < len) {
                        var remaining = len - i;
                        var a = b64chars.indexOf(str.charAt(i++));
                        var b = b64chars.indexOf(str.charAt(i++));
                        var c = remaining > 2 ? b64chars.indexOf(str.charAt(i++)) : 0;
                        var d = remaining > 3 ? b64chars.indexOf(str.charAt(i++)) : 0;
                        if (a < 0 || b < 0 || c < 0 || d < 0) return "";
                        var n = (a << 18) | (b << 12) | (c << 6) | d;
                        result += String.fromCharCode((n >> 16) & 0xff);
                        if (remaining > 2) result += String.fromCharCode((n >> 8) & 0xff);
                        if (remaining > 3) result += String.fromCharCode(n & 0xff);
                    }
                    return result;
                },
                encode: function(str) {
                    str = String(str || "");
                    var result = "";
                    var len = str.length;
                    var i = 0;
                    while (i < len) {
                        var chunkStart = i;
                        var a = str.charCodeAt(i++) & 0xff;
                        var b = i < len ? str.charCodeAt(i++) & 0xff : 0;
                        var c = i < len ? str.charCodeAt(i++) & 0xff : 0;
                        var bytesInChunk = i - chunkStart;
                        var n = (a << 16) | (b << 8) | c;
                        result += b64chars.charAt((n >> 18) & 63);
                        result += b64chars.charAt((n >> 12) & 63);
                        result += bytesInChunk < 2 ? "=" : b64chars.charAt((n >> 6) & 63);
                        result += bytesInChunk < 3 ? "=" : b64chars.charAt(n & 63);
                    }
                    return result;
                }
            };

            ctx.jwt = {
                decodePayload: function(token) {
                    if (typeof token !== "string") return null;
                    var parts = token.split(".");
                    if (parts.length < 2) return null;
                    try {
                        return JSON.parse(ctx.base64.decode(parts[1]));
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

fn inject_sqlite<'js>(ctx: &Ctx<'js>, host: &Object<'js>) -> rquickjs::Result<()> {
    let sqlite_obj = Object::new(ctx.clone())?;

    sqlite_obj.set(
        "query",
        Function::new(
            ctx.clone(),
            move |ctx_inner: Ctx<'_>, db_path: String, sql: String| -> rquickjs::Result<String> {
                if sql.lines().any(|line| line.trim_start().starts_with('.')) {
                    return Err(Exception::throw_message(
                        &ctx_inner,
                        "sqlite3 dot-commands are not allowed",
                    ));
                }
                let expanded = expand_path(&db_path);
                sqlite_query_impl(&expanded.to_string_lossy(), &sql)
                    .map_err(|e| Exception::throw_message(&ctx_inner, &e))
            },
        )?,
    )?;

    host.set("sqlite", sqlite_obj)?;
    Ok(())
}

fn inject_ccusage<'js>(
    ctx: &Ctx<'js>,
    host: &Object<'js>,
    plugin_id: &str,
) -> rquickjs::Result<()> {
    let ccusage_obj = Object::new(ctx.clone())?;
    let pid = plugin_id.to_string();

    ccusage_obj.set(
        "_queryRaw",
        Function::new(
            ctx.clone(),
            move |_ctx_inner: Ctx<'_>, opts_json: String| -> rquickjs::Result<String> {
                let opts: CcusageQueryOpts = serde_json::from_str(&opts_json).unwrap_or_default();
                Ok(query_status_json(&opts, &pid))
            },
        )?,
    )?;

    host.set("ccusage", ccusage_obj)?;
    Ok(())
}

fn inject_usage_daily<'js>(
    ctx: &Ctx<'js>,
    host: &Object<'js>,
    plugin_id: &str,
) -> rquickjs::Result<()> {
    let usage_daily_obj = Object::new(ctx.clone())?;
    let pid = plugin_id.to_string();

    usage_daily_obj.set(
        "_ingestRaw",
        Function::new(
            ctx.clone(),
            move |_ctx_inner: Ctx<'_>, payload_json: String| -> rquickjs::Result<()> {
                if let Err(error) = usagestat_core::usage_daily::ingest_json(&pid, &payload_json) {
                    log::warn!("[plugin:{}] usageDaily.ingest failed: {}", pid, error);
                }
                Ok(())
            },
        )?,
    )?;

    host.set("usageDaily", usage_daily_obj)?;
    Ok(())
}

fn inject_cursor_logs<'js>(ctx: &Ctx<'js>, host: &Object<'js>) -> rquickjs::Result<()> {
    let cursor_logs_obj = Object::new(ctx.clone())?;

    cursor_logs_obj.set(
        "_queryRaw",
        Function::new(
            ctx.clone(),
            move |_ctx_inner: Ctx<'_>, opts_json: String| -> rquickjs::Result<String> {
                let since = serde_json::from_str::<JsonValue>(&opts_json)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("since")
                            .and_then(|since| since.as_str())
                            .map(str::to_string)
                    })
                    .unwrap_or_default();
                let (status, daily) = crate::cursor_usage_logs::query_daily_since(&since);
                let status = match status {
                    crate::cursor_usage_logs::CursorLogsStatus::Ok => "ok",
                    crate::cursor_usage_logs::CursorLogsStatus::NoData => "no_data",
                };
                Ok(serde_json::json!({
                    "status": status,
                    "data": { "daily": daily }
                })
                .to_string())
            },
        )?,
    )?;

    host.set("cursorLogs", cursor_logs_obj)?;
    Ok(())
}

fn inject_cursor_usage_export<'js>(ctx: &Ctx<'js>, host: &Object<'js>) -> rquickjs::Result<()> {
    let export_obj = Object::new(ctx.clone())?;

    export_obj.set(
        "_queryMtdRaw",
        Function::new(
            ctx.clone(),
            move |_ctx_inner: Ctx<'_>, opts_json: String| -> rquickjs::Result<String> {
                Ok(crate::cursor_usage_export::query_mtd_host_json(&opts_json))
            },
        )?,
    )?;
    export_obj.set(
        "_queryStatsRaw",
        Function::new(
            ctx.clone(),
            move |_ctx_inner: Ctx<'_>, opts_json: String| -> rquickjs::Result<String> {
                Ok(crate::cursor_usage_export::query_usage_stats_host_json(
                    &opts_json,
                ))
            },
        )?,
    )?;
    export_obj.set(
        "_queryDailyRaw",
        Function::new(
            ctx.clone(),
            move |_ctx_inner: Ctx<'_>, opts_json: String| -> rquickjs::Result<String> {
                Ok(crate::cursor_usage_export::query_daily_billing_host_json(
                    &opts_json,
                ))
            },
        )?,
    )?;

    host.set("cursorUsageExport", export_obj)?;
    Ok(())
}

fn firectl_runner_candidates() -> [&'static str; 3] {
    [
        "firectl",
        "/opt/homebrew/bin/firectl",
        "/usr/local/bin/firectl",
    ]
}

fn resolve_firectl_runner() -> Option<String> {
    static FIRECTL_RUNNER: OnceLock<Option<String>> = OnceLock::new();
    FIRECTL_RUNNER
        .get_or_init(|| {
            for candidate in firectl_runner_candidates() {
                if Command::new(candidate)
                    .arg("--help")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .map(|status| status.success())
                    .unwrap_or(false)
                {
                    return Some(candidate.to_string());
                }
            }
            None
        })
        .clone()
}

fn fireworks_auth_ini_contents(api_key: &str) -> String {
    format!("[fireworks]\napi_key = {}\n", api_key)
}

fn write_fireworks_auth_ini(auth_root: &Path, api_key: &str) -> std::io::Result<PathBuf> {
    let fireworks_dir = auth_root.join(".fireworks");
    std::fs::create_dir_all(&fireworks_dir)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fireworks_dir, std::fs::Permissions::from_mode(0o700))?;
    }

    let auth_ini_path = fireworks_dir.join("auth.ini");
    std::fs::write(&auth_ini_path, fireworks_auth_ini_contents(api_key))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&auth_ini_path, std::fs::Permissions::from_mode(0o600))?;
    }

    Ok(auth_ini_path)
}

fn cleanup_fireworks_export_dir(path: &Path) {
    let _ = std::fs::remove_dir_all(path);
}

fn run_fireworks_billing_export_timeout(
    command: &mut Command,
    plugin_id: &str,
    timeout: Duration,
) -> Result<std::process::Output, &'static str> {
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            log::warn!(
                "[plugin:{}] failed to spawn firectl billing export: {}",
                plugin_id,
                error
            );
            return Err("runner_failed");
        }
    };

    let mut stdout_reader = child.stdout.take().map(|mut stdout| {
        std::thread::spawn(move || read_stream_capped(&mut stdout, COMMAND_OUTPUT_LIMIT_BYTES))
    });
    let mut stderr_reader = child.stderr.take().map(|mut stderr| {
        std::thread::spawn(move || read_stream_capped(&mut stderr, COMMAND_OUTPUT_LIMIT_BYTES))
    });

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = stdout_reader
                    .take()
                    .and_then(|reader| reader.join().ok())
                    .unwrap_or_default();
                let stderr = stderr_reader
                    .take()
                    .and_then(|reader| reader.join().ok())
                    .unwrap_or_default();
                return Ok(std::process::Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stdout_reader.take().and_then(|reader| reader.join().ok());
                    let _ = stderr_reader.take().and_then(|reader| reader.join().ok());
                    log::warn!(
                        "[plugin:{}] firectl billing export timed out after {}s",
                        plugin_id,
                        timeout.as_secs()
                    );
                    return Err("timed_out");
                }
                std::thread::sleep(Duration::from_millis(FIRECTL_POLL_INTERVAL_MS));
            }
            Err(error) => {
                log::warn!(
                    "[plugin:{}] firectl billing export wait failed: {}",
                    plugin_id,
                    error
                );
                let _ = stdout_reader.take().and_then(|reader| reader.join().ok());
                let _ = stderr_reader.take().and_then(|reader| reader.join().ok());
                return Err("runner_failed");
            }
        }
    }
}

fn run_fireworks_billing_export(
    request: &FireworksBillingExportRequest,
    plugin_id: &str,
) -> JsonValue {
    if request.api_key.trim().is_empty()
        || request.account_id.trim().is_empty()
        || request.start_time.trim().is_empty()
        || request.end_time.trim().is_empty()
    {
        return serde_json::json!({ "status": "invalid_opts" });
    }

    let Some(program) = resolve_firectl_runner() else {
        log::warn!(
            "[plugin:{}] firectl not found for billing export",
            plugin_id
        );
        return serde_json::json!({ "status": "no_runner" });
    };

    let workspace_name = format!(
        "usagestat-fireworks-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    );
    let temp_dir = std::env::temp_dir().join(&workspace_name);
    if let Err(error) = std::fs::create_dir_all(&temp_dir) {
        log::warn!(
            "[plugin:{}] failed to create Fireworks export temp dir: {}",
            plugin_id,
            error
        );
        return serde_json::json!({ "status": "runner_failed" });
    }

    if let Err(error) = write_fireworks_auth_ini(&temp_dir, request.api_key.trim()) {
        cleanup_fireworks_export_dir(&temp_dir);
        log::warn!(
            "[plugin:{}] failed to prepare Fireworks auth config: {}",
            plugin_id,
            error
        );
        return serde_json::json!({ "status": "runner_failed" });
    }

    let file_name = format!("{workspace_name}.csv");
    let output_path = temp_dir.join(&file_name);
    let mut command = Command::new(&program);
    command
        .current_dir(&temp_dir)
        .env("HOME", &temp_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args([
            "billing",
            "export-metrics",
            "--account-id",
            request.account_id.trim(),
            "--start-time",
            request.start_time.trim(),
            "--end-time",
            request.end_time.trim(),
            "--filename",
            file_name.as_str(),
        ]);

    let result = run_fireworks_billing_export_timeout(
        &mut command,
        plugin_id,
        Duration::from_secs(FIRECTL_TIMEOUT_SECS),
    );
    let read_csv = || std::fs::read_to_string(&output_path).ok();
    let cleanup = || cleanup_fireworks_export_dir(&temp_dir);

    match result {
        Ok(output) if output.status.success() => {
            let csv = read_csv();
            cleanup();
            match csv {
                Some(text) if !text.trim().is_empty() => {
                    serde_json::json!({ "status": "ok", "csv": text })
                }
                _ => {
                    log::warn!(
                        "[plugin:{}] billing export succeeded but no CSV was produced",
                        plugin_id
                    );
                    serde_json::json!({ "status": "empty" })
                }
            }
        }
        Ok(output) => {
            cleanup();
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::warn!(
                "[plugin:{}] firectl billing export failed: {}",
                plugin_id,
                stderr.lines().next().unwrap_or("unknown error").trim()
            );
            serde_json::json!({ "status": "runner_failed" })
        }
        Err(status) => {
            cleanup();
            serde_json::json!({ "status": status })
        }
    }
}

fn inject_fireworks<'js>(
    ctx: &Ctx<'js>,
    host: &Object<'js>,
    plugin_id: &str,
) -> rquickjs::Result<()> {
    let fireworks_obj = Object::new(ctx.clone())?;
    let pid = plugin_id.to_string();
    fireworks_obj.set(
        "_exportBillingMetricsRaw",
        Function::new(
            ctx.clone(),
            move |_ctx_inner: Ctx<'_>, opts_json: String| -> rquickjs::Result<String> {
                let request: FireworksBillingExportRequest = serde_json::from_str(&opts_json)
                    .unwrap_or(FireworksBillingExportRequest {
                        api_key: String::new(),
                        account_id: String::new(),
                        start_time: String::new(),
                        end_time: String::new(),
                    });
                serde_json::to_string(&run_fireworks_billing_export(&request, &pid))
                    .map_err(|error| Exception::throw_message(&_ctx_inner, &error.to_string()))
            },
        )?,
    )?;
    host.set("fireworks", fireworks_obj)?;
    Ok(())
}

fn patch_ccusage_wrapper(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    ctx.eval::<(), _>(
        r#"
        (function() {
            if (__usagestat_ctx.host.ccusage && __usagestat_ctx.host.ccusage._queryRaw) {
                var rawFn = __usagestat_ctx.host.ccusage._queryRaw;
                __usagestat_ctx.host.ccusage.query = function(opts) {
                    var result = rawFn(JSON.stringify(opts || {}));
                    try {
                        var parsed = JSON.parse(result);
                        if (parsed && typeof parsed === "object" && typeof parsed.status === "string") {
                            return parsed;
                        }
                    } catch (e) {}
                    return { status: "runner_failed" };
                };
            }
        })();
        "#
        .as_bytes(),
    )
}

fn patch_usage_daily_wrapper(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    ctx.eval::<(), _>(
        r#"
        (function() {
            if (!__usagestat_ctx.host.usageDaily || !__usagestat_ctx.host.usageDaily._ingestRaw) return;
            var rawFn = __usagestat_ctx.host.usageDaily._ingestRaw;
            __usagestat_ctx.host.usageDaily.ingest = function(opts) {
                rawFn(JSON.stringify(opts || {}));
            };
        })();
        "#
        .as_bytes(),
    )
}

fn patch_cursor_logs_wrapper(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    ctx.eval::<(), _>(
        r#"
        (function() {
            if (!__usagestat_ctx.host.cursorLogs || !__usagestat_ctx.host.cursorLogs._queryRaw) return;
            var rawFn = __usagestat_ctx.host.cursorLogs._queryRaw;
            __usagestat_ctx.host.cursorLogs.queryDaily = function(opts) {
                var result = rawFn(JSON.stringify(opts || {}));
                try {
                    var parsed = JSON.parse(result);
                    if (parsed && typeof parsed === "object" && typeof parsed.status === "string") {
                        return parsed;
                    }
                } catch (_) {}
                return { status: "no_data", data: { daily: [] } };
            };
        })();
        "#
        .as_bytes(),
    )
}

fn patch_cursor_usage_export_wrapper(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    ctx.eval::<(), _>(
        r#"
        (function() {
            if (!__usagestat_ctx.host.cursorUsageExport) return;
            function cursorExportOpts(opts) {
                var o = opts && typeof opts === "object" ? Object.assign({}, opts) : {};
                if (!o.pluginId) {
                    o.pluginId = globalThis.__OPENUSAGE_PLUGIN_REGISTRATION_ID__ ||
                        (__usagestat_ctx.provider && __usagestat_ctx.provider.id) ||
                        "cursor";
                }
                return JSON.stringify(o);
            }

            var mtdRawFn = __usagestat_ctx.host.cursorUsageExport._queryMtdRaw;
            if (typeof mtdRawFn === "function") {
                __usagestat_ctx.host.cursorUsageExport.queryMtd = function(opts) {
                    var result = mtdRawFn(cursorExportOpts(opts));
                    try {
                        var parsed = JSON.parse(result);
                        if (parsed && typeof parsed === "object" && typeof parsed.status === "string") {
                            return parsed;
                        }
                    } catch (_) {}
                    return { status: "error", message: "invalid MTD response" };
                };
            }

            var statsRawFn = __usagestat_ctx.host.cursorUsageExport._queryStatsRaw;
            if (typeof statsRawFn === "function") {
                __usagestat_ctx.host.cursorUsageExport.queryStats = function(opts) {
                    var result = statsRawFn(cursorExportOpts(opts));
                    try {
                        var parsed = JSON.parse(result);
                        if (parsed && typeof parsed === "object" && typeof parsed.status === "string") {
                            return parsed;
                        }
                    } catch (_) {}
                    return { status: "error", message: "invalid usage stats response" };
                };
            }

            var dailyRawFn = __usagestat_ctx.host.cursorUsageExport._queryDailyRaw;
            if (typeof dailyRawFn === "function") {
                __usagestat_ctx.host.cursorUsageExport.queryDaily = function(opts) {
                    var result = dailyRawFn(cursorExportOpts(opts));
                    try {
                        var parsed = JSON.parse(result);
                        if (parsed && typeof parsed === "object" && typeof parsed.status === "string") {
                            return parsed;
                        }
                    } catch (_) {}
                    return { status: "error", message: "invalid daily billing response" };
                };
            }
        })();
        "#
        .as_bytes(),
    )
}

fn patch_fireworks_wrapper(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    ctx.eval::<(), _>(
        r#"
        (function() {
            if (!__usagestat_ctx.host.fireworks || !__usagestat_ctx.host.fireworks._exportBillingMetricsRaw) return;
            var rawFn = __usagestat_ctx.host.fireworks._exportBillingMetricsRaw;
            __usagestat_ctx.host.fireworks.exportBillingMetrics = function(opts) {
                var result = rawFn(JSON.stringify(opts || {}));
                try {
                    var parsed = JSON.parse(result);
                    if (parsed && typeof parsed === "object" && typeof parsed.status === "string") {
                        return parsed;
                    }
                } catch (_) {}
                return { status: "runner_failed" };
            };
        })();
        "#
        .as_bytes(),
    )
}

fn first_existing_path(paths: &[String]) -> Option<String> {
    paths
        .iter()
        .find(|path| expand_path(path).exists())
        .cloned()
}

fn first_existing_app_support_path(relative: &str) -> Option<String> {
    first_existing_path(&app_support_path_candidates(relative))
}

fn app_support_path_candidates(relative: &str) -> Vec<String> {
    let rel = relative
        .trim()
        .trim_start_matches('/')
        .trim_start_matches('\\');
    if rel.is_empty() {
        return Vec::new();
    }

    let mut paths = Vec::new();
    let mut push_join = |base: Option<String>| {
        if let Some(base) = base.and_then(|value| non_empty_trimmed(&value)) {
            paths.push(format!("{}/{}", base.trim_end_matches(['/', '\\']), rel));
        }
    };

    push_join(std::env::var("APPDATA").ok());
    push_join(std::env::var("LOCALAPPDATA").ok());
    push_join(std::env::var("XDG_CONFIG_HOME").ok());

    if let Some(home) = home_dir() {
        let home = home.to_string_lossy();
        paths.push(format!("{home}/.config/{rel}"));
        paths.push(format!("{home}/Library/Application Support/{rel}"));
        paths.push(format!("{home}/AppData/Roaming/{rel}"));
        paths.push(format!("{home}/AppData/Local/{rel}"));
    }

    paths
}

fn discover_language_server(request: &LsDiscoverRequest) -> Option<LsDiscoverResponse> {
    if request.process_name.trim().is_empty() {
        return None;
    }

    let output = if cfg!(target_os = "windows") {
        return None;
    } else if cfg!(target_os = "macos") {
        Command::new("ps").args(["-axo", "pid=,command="]).output()
    } else {
        Command::new("ps").args(["-eo", "pid=,args="]).output()
    }
    .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if !line.contains(&request.process_name) {
            continue;
        }
        if !request
            .markers
            .iter()
            .all(|marker| marker.trim().is_empty() || line.contains(marker))
        {
            continue;
        }

        let csrf = request
            .csrf_flag
            .as_deref()
            .and_then(|flag| extract_flag_value(line, flag))
            .and_then(|value| non_empty_trimmed(&value))?;

        let extension_port = request
            .port_flag
            .as_deref()
            .and_then(|flag| extract_flag_value(line, flag))
            .and_then(|value| value.parse::<u16>().ok());

        let mut ports = Vec::new();
        if let Some(port) = extension_port {
            ports.push(port);
        }
        if ports.is_empty() {
            continue;
        }

        return Some(LsDiscoverResponse {
            csrf,
            ports,
            extension_port,
        });
    }

    None
}

fn extract_flag_value(command: &str, flag: &str) -> Option<String> {
    let flag = flag.trim();
    if flag.is_empty() {
        return None;
    }
    let flag_eq = format!("{flag}=");
    let mut parts = command.split_whitespace().peekable();
    while let Some(part) = parts.next() {
        if part == flag {
            return parts.next().map(clean_flag_value);
        }
        if let Some(rest) = part.strip_prefix(&flag_eq) {
            return Some(clean_flag_value(rest));
        }
    }
    None
}

fn clean_flag_value(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

fn non_empty_trimmed(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn current_keychain_account() -> String {
    std::env::var("USER")
        .ok()
        .and_then(|value| non_empty_trimmed(&value))
        .or_else(|| {
            std::env::var("USERNAME")
                .ok()
                .and_then(|value| non_empty_trimmed(&value))
        })
        .or_else(|| read_command_stdout("id", &["-un"]))
        .unwrap_or_else(|| "usagestat-user".to_string())
}

fn read_command_stdout(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    non_empty_trimmed(&String::from_utf8_lossy(&output.stdout))
}

fn log_keychain_read(plugin_id: &str, service: &str, account: Option<&str>) {
    if let Some(account) = account {
        log::info!(
            "[plugin:{plugin_id}] keychain read: service={service}, account={}",
            redact_value(account)
        );
    } else {
        log::info!("[plugin:{plugin_id}] keychain read: service={service}");
    }
}

fn platform_keychain_read(service: &str, account: Option<&str>) -> Result<String, String> {
    if cfg!(target_os = "macos") {
        macos_keychain_read(service, account)
    } else if cfg!(target_os = "linux") {
        linux_secret_tool_read(service, account)
    } else {
        Err("keychain access is not supported on this platform".to_string())
    }
}

fn platform_keychain_read_generic_item(service: &str) -> Result<KeychainPasswordItem, String> {
    if cfg!(target_os = "macos") {
        macos_keychain_read_generic_item(service)
    } else {
        Err("keychain item account lookup is not supported on this platform".to_string())
    }
}

fn platform_keychain_read_internet_password(server: &str) -> Result<KeychainPasswordItem, String> {
    if cfg!(target_os = "macos") {
        macos_keychain_read_internet_password(server)
    } else {
        Err("internet password keychain access is not supported on this platform".to_string())
    }
}

fn platform_keychain_write(
    service: &str,
    account: Option<&str>,
    value: &str,
) -> Result<(), String> {
    if cfg!(target_os = "macos") {
        macos_keychain_write(service, account, value)
    } else if cfg!(target_os = "linux") {
        linux_secret_tool_write(service, account, value)
    } else {
        Err("keychain access is not supported on this platform".to_string())
    }
}

fn platform_keychain_delete(service: &str, account: Option<&str>) -> Result<(), String> {
    if cfg!(target_os = "macos") {
        macos_keychain_delete(service, account)
    } else if cfg!(target_os = "linux") {
        linux_secret_tool_delete(service, account)
    } else {
        Err("keychain access is not supported on this platform".to_string())
    }
}

fn macos_keychain_read(service: &str, account: Option<&str>) -> Result<String, String> {
    let mut command = Command::new("security");
    command.arg("find-generic-password");
    if let Some(account) = account {
        command.args(["-a", account]);
    }
    command.args(["-s", service, "-w"]);
    let output = command.output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(command_error(&output));
    }
    non_empty_trimmed(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| "empty keychain item".to_string())
}

fn macos_keychain_read_generic_item(service: &str) -> Result<KeychainPasswordItem, String> {
    let mut inspect = Command::new("security");
    inspect.args(["find-generic-password", "-s", service]);
    let inspect_output = inspect.output().map_err(|error| error.to_string())?;
    if !inspect_output.status.success() {
        return Err(command_error(&inspect_output));
    }
    let inspect_text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&inspect_output.stdout),
        String::from_utf8_lossy(&inspect_output.stderr)
    );
    let account = parse_macos_security_attribute(&inspect_text, "acct")
        .ok_or_else(|| "keychain item has no account attribute".to_string())?;
    let password = macos_keychain_read(service, Some(&account))?;
    Ok(KeychainPasswordItem { account, password })
}

fn macos_keychain_read_internet_password(server: &str) -> Result<KeychainPasswordItem, String> {
    let mut inspect = Command::new("security");
    inspect.args(["find-internet-password", "-s", server]);
    let inspect_output = inspect.output().map_err(|error| error.to_string())?;
    if !inspect_output.status.success() {
        return Err(command_error(&inspect_output));
    }
    let inspect_text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&inspect_output.stdout),
        String::from_utf8_lossy(&inspect_output.stderr)
    );
    let account = parse_macos_security_attribute(&inspect_text, "acct")
        .ok_or_else(|| "keychain item has no account attribute".to_string())?;

    let mut password_command = Command::new("security");
    password_command.args(["find-internet-password", "-s", server, "-a", &account, "-w"]);
    let password_output = password_command
        .output()
        .map_err(|error| error.to_string())?;
    if !password_output.status.success() {
        return Err(command_error(&password_output));
    }
    let password = non_empty_trimmed(&String::from_utf8_lossy(&password_output.stdout))
        .ok_or_else(|| "empty keychain item".to_string())?;
    Ok(KeychainPasswordItem { account, password })
}

fn macos_keychain_write(service: &str, account: Option<&str>, value: &str) -> Result<(), String> {
    let mut command = Command::new("security");
    command.args(["add-generic-password", "-U"]);
    if let Some(account) = account {
        command.args(["-a", account]);
    }
    command.args(["-s", service, "-w", value]);
    let output = command.output().map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_error(&output))
    }
}

fn macos_keychain_delete(service: &str, account: Option<&str>) -> Result<(), String> {
    let mut command = Command::new("security");
    command.arg("delete-generic-password");
    if let Some(account) = account {
        command.args(["-a", account]);
    }
    command.args(["-s", service]);
    let output = command.output().map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_error(&output))
    }
}

fn linux_secret_tool_path() -> Option<&'static str> {
    ["/usr/bin/secret-tool", "secret-tool"]
        .into_iter()
        .find(|candidate| {
            candidate.contains('/') && std::path::Path::new(candidate).is_file()
                || !candidate.contains('/')
        })
}

fn linux_secret_tool_read(service: &str, account: Option<&str>) -> Result<String, String> {
    let Some(secret_tool) = linux_secret_tool_path() else {
        return Err("secret-tool not installed".to_string());
    };
    let mut command = Command::new(secret_tool);
    command.args(["lookup", "service", service]);
    if let Some(account) = account {
        command.args(["username", account]);
    }
    let output = command.output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(command_error(&output));
    }
    non_empty_trimmed(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| "secret-tool returned empty secret".to_string())
}

fn linux_secret_tool_write(
    service: &str,
    account: Option<&str>,
    value: &str,
) -> Result<(), String> {
    let Some(secret_tool) = linux_secret_tool_path() else {
        return Err("secret-tool not installed".to_string());
    };
    let mut command = Command::new(secret_tool);
    command.args(["store", "--label", service, "service", service]);
    if let Some(account) = account {
        command.args(["username", account]);
    }
    command.stdin(Stdio::piped());
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(value.as_bytes())
            .map_err(|error| error.to_string())?;
    }
    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_error(&output))
    }
}

fn linux_secret_tool_delete(service: &str, account: Option<&str>) -> Result<(), String> {
    let Some(secret_tool) = linux_secret_tool_path() else {
        return Err("secret-tool not installed".to_string());
    };
    let mut command = Command::new(secret_tool);
    command.args(["clear", "service", service]);
    if let Some(account) = account {
        command.args(["username", account]);
    }
    let output = command.output().map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_error(&output))
    }
}

fn parse_macos_security_attribute(output: &str, attr: &str) -> Option<String> {
    let quoted = format!("\"{attr}\"");
    for line in output.lines() {
        let trimmed = line.trim();
        if !trimmed.contains(&quoted) {
            continue;
        }
        let Some(eq_index) = trimmed.find('=') else {
            continue;
        };
        let raw = trimmed[eq_index + 1..].trim();
        if raw == "<NULL>" {
            return None;
        }
        if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
            return Some(raw[1..raw.len() - 1].to_string());
        }
        if !raw.is_empty() {
            return Some(raw.to_string());
        }
    }
    None
}

fn run_command_with_capped_output(
    command: &mut Command,
    timeout: Duration,
    poll_interval: Duration,
    output_limit: usize,
) -> Result<std::process::Output, String> {
    let mut child = command
        .spawn()
        .map_err(|error| format!("command failed to start: {error}"))?;

    let mut stdout_reader = child.stdout.take().map(move |mut stdout| {
        std::thread::spawn(move || read_stream_capped(&mut stdout, output_limit))
    });
    let mut stderr_reader = child.stderr.take().map(move |mut stderr| {
        std::thread::spawn(move || read_stream_capped(&mut stderr, output_limit))
    });

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = stdout_reader
                    .take()
                    .and_then(|reader| reader.join().ok())
                    .unwrap_or_default();
                let stderr = stderr_reader
                    .take()
                    .and_then(|reader| reader.join().ok())
                    .unwrap_or_default();
                return Ok(std::process::Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.take().and_then(|reader| reader.join().ok());
                let _ = stderr_reader.take().and_then(|reader| reader.join().ok());
                return Err(format!("command timed out after {}ms", timeout.as_millis()));
            }
            Ok(None) => std::thread::sleep(poll_interval),
            Err(error) => return Err(format!("command wait failed: {error}")),
        }
    }
}

fn read_stream_capped<R: Read>(reader: &mut R, limit: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(limit.min(8192));
    let mut buffer = [0_u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                if output.len() < limit {
                    let remaining = limit - output.len();
                    output.extend_from_slice(&buffer[..read.min(remaining)]);
                }
            }
            Err(_) => break,
        }
    }
    output
}

fn decode_capped_output(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(error) if error.error_len().is_none() => {
            String::from_utf8_lossy(&bytes[..error.valid_up_to()]).into_owned()
        }
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn command_error(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    stderr
        .lines()
        .chain(stdout.lines())
        .find_map(non_empty_trimmed)
        .unwrap_or_else(|| format!("command exited with status {}", output.status))
}

fn redact_value(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= 12 {
        return "[REDACTED]".to_string();
    }
    let first: String = chars.iter().take(4).collect();
    let last: String = chars
        .iter()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{first}...{last}")
}

fn sqlite_query_impl(path: &str, sql: &str) -> Result<String, String> {
    let conn = match Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(c) => c,
        Err(e) => {
            let encoded = path
                .replace('%', "%25")
                .replace(' ', "%20")
                .replace('#', "%23")
                .replace('?', "%3F");
            let uri = format!("file:{}?immutable=1", encoded);
            Connection::open_with_flags(
                &uri,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
            )
            .map_err(|e2| format!("sqlite open failed: {e} (fallback: {e2})"))?
        }
    };
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let col_names: Vec<String> = stmt.column_names().into_iter().map(String::from).collect();
    let rows = stmt
        .query_map([], |row| {
            let mut obj = Map::new();
            for (i, name) in col_names.iter().enumerate() {
                let v: rusqlite::types::Value = row
                    .get(i)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                obj.insert(name.clone(), rusqlite_value_to_json(v));
            }
            Ok(JsonValue::Object(obj))
        })
        .map_err(|e| e.to_string())?;
    let arr: Result<Vec<_>, _> = rows.collect();
    let arr = arr.map_err(|e| e.to_string())?;
    serde_json::to_string(&arr).map_err(|e| e.to_string())
}

fn rusqlite_value_to_json(v: rusqlite::types::Value) -> JsonValue {
    match v {
        rusqlite::types::Value::Null => JsonValue::Null,
        rusqlite::types::Value::Integer(i) => JsonValue::Number(serde_json::Number::from(i)),
        rusqlite::types::Value::Real(f) => serde_json::Number::from_f64(f)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        rusqlite::types::Value::Text(s) => JsonValue::String(s),
        rusqlite::types::Value::Blob(b) => {
            JsonValue::String(String::from_utf8_lossy(&b).into_owned())
        }
    }
}

fn aws_sigv4_headers(
    method: &str,
    url: &str,
    body: &[u8],
    access_key_id: &str,
    secret_access_key: &str,
    session_token: Option<&str>,
) -> Result<BTreeMap<String, String>, String> {
    let parsed = reqwest::Url::parse(url).map_err(|error| format!("invalid AWS URL: {error}"))?;
    let host = aws_host_header(&parsed)?;
    let now = chrono::Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();
    let body_hash = sha256_hex(body);

    let mut headers = BTreeMap::new();
    headers.insert(
        "content-type".to_string(),
        "application/x-amz-json-1.1".to_string(),
    );
    headers.insert("host".to_string(), host);
    headers.insert("x-amz-content-sha256".to_string(), body_hash.clone());
    headers.insert("x-amz-date".to_string(), amz_date.clone());
    if let Some(session_token) = session_token.and_then(non_empty_trimmed) {
        headers.insert("x-amz-security-token".to_string(), session_token);
    }
    headers.insert(
        "x-amz-target".to_string(),
        AWS_COST_EXPLORER_TARGET.to_string(),
    );

    let signed_headers = headers.keys().cloned().collect::<Vec<_>>().join(";");
    let canonical_headers = headers
        .iter()
        .map(|(name, value)| format!("{name}:{}\n", normalize_aws_header_value(value)))
        .collect::<String>();
    let path = if parsed.path().is_empty() {
        "/"
    } else {
        parsed.path()
    };
    let canonical_request = [
        method.to_ascii_uppercase(),
        aws_uri_encode(path, false),
        aws_canonical_query(&parsed),
        canonical_headers,
        signed_headers.clone(),
        body_hash,
    ]
    .join("\n");

    let credential_scope =
        format!("{date_stamp}/{AWS_COST_EXPLORER_REGION}/{AWS_COST_EXPLORER_SERVICE}/aws4_request");
    let string_to_sign = [
        "AWS4-HMAC-SHA256".to_string(),
        amz_date,
        credential_scope.clone(),
        sha256_hex(canonical_request.as_bytes()),
    ]
    .join("\n");
    let signing_key = aws_signature_key(
        secret_access_key,
        &date_stamp,
        AWS_COST_EXPLORER_REGION,
        AWS_COST_EXPLORER_SERVICE,
    )?;
    let signature = hex_lower(&hmac_sha256(&signing_key, string_to_sign.as_bytes())?);
    headers.insert(
        "authorization".to_string(),
        format!(
            "AWS4-HMAC-SHA256 Credential={access_key_id}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}"
        ),
    );
    Ok(headers)
}

fn aws_host_header(url: &reqwest::Url) -> Result<String, String> {
    let host = url
        .host_str()
        .ok_or_else(|| "AWS URL is missing a host".to_string())?;
    match url.port() {
        Some(port) if !(url.scheme() == "https" && port == 443) => Ok(format!("{host}:{port}")),
        _ => Ok(host.to_string()),
    }
}

fn aws_canonical_query(url: &reqwest::Url) -> String {
    let mut pairs = url
        .query_pairs()
        .map(|(key, value)| {
            format!(
                "{}={}",
                aws_uri_encode(&key, true),
                aws_uri_encode(&value, true)
            )
        })
        .collect::<Vec<_>>();
    pairs.sort();
    pairs.join("&")
}

fn normalize_aws_header_value(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex_lower(&digest)
}

fn aws_signature_key(
    secret_key: &str,
    date_stamp: &str,
    region: &str,
    service: &str,
) -> Result<Vec<u8>, String> {
    let k_date = hmac_sha256(
        format!("AWS4{secret_key}").as_bytes(),
        date_stamp.as_bytes(),
    )?;
    let k_region = hmac_sha256(&k_date, region.as_bytes())?;
    let k_service = hmac_sha256(&k_region, service.as_bytes())?;
    hmac_sha256(&k_service, b"aws4_request")
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
    let mut mac =
        HmacSha256::new_from_slice(key).map_err(|error| format!("invalid HMAC key: {error}"))?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn aws_uri_encode(value: &str, encode_slash: bool) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char);
            }
            b'/' if !encode_slash => encoded.push('/'),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn execute_http_request(request: HttpRequest) -> Result<HttpResponse, reqwest::Error> {
    let _ = rustls::crypto::ring::default_provider().install_default();

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

fn execute_aws_cost_explorer_request(
    request: AwsCostExplorerRequest,
) -> Result<AwsCostExplorerResponse, String> {
    let access_key_id = non_empty_trimmed(&request.access_key_id)
        .ok_or_else(|| "AWS access key id is missing".to_string())?;
    let secret_access_key = non_empty_trimmed(&request.secret_access_key)
        .ok_or_else(|| "AWS secret access key is missing".to_string())?;
    let session_token = request.session_token.as_deref().and_then(non_empty_trimmed);
    let start_date = non_empty_trimmed(&request.start_date)
        .ok_or_else(|| "Cost Explorer start date is missing".to_string())?;
    let end_date = non_empty_trimmed(&request.end_date)
        .ok_or_else(|| "Cost Explorer end date is missing".to_string())?;
    let granularity = match request.granularity.trim().to_ascii_uppercase().as_str() {
        "DAILY" => "DAILY",
        "MONTHLY" => "MONTHLY",
        _ => return Err("Cost Explorer granularity must be DAILY or MONTHLY".to_string()),
    };
    let url = request
        .api_url
        .as_deref()
        .and_then(non_empty_trimmed)
        .unwrap_or_else(|| AWS_COST_EXPLORER_DEFAULT_URL.to_string());
    if !url.to_ascii_lowercase().starts_with("https://") {
        return Err("AWS Cost Explorer URL must be HTTPS".to_string());
    }

    let mut body = serde_json::json!({
        "TimePeriod": {
            "Start": start_date,
            "End": end_date,
        },
        "Granularity": granularity,
        "Metrics": ["UnblendedCost"],
        "GroupBy": [
            { "Type": "DIMENSION", "Key": "SERVICE" }
        ],
    });
    if let Some(next_page_token) = request
        .next_page_token
        .as_deref()
        .and_then(non_empty_trimmed)
    {
        body["NextPageToken"] = JsonValue::String(next_page_token);
    }
    let body_text = serde_json::to_string(&body)
        .map_err(|error| format!("invalid AWS request body: {error}"))?;

    let headers = aws_sigv4_headers(
        "POST",
        &url,
        body_text.as_bytes(),
        &access_key_id,
        &secret_access_key,
        session_token.as_deref(),
    )?;

    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| error.to_string())?;
    let mut builder = client.post(&url);
    for (name, value) in headers {
        builder = builder.header(name, value);
    }
    let response = builder
        .body(body_text)
        .send()
        .map_err(|error| format!("AWS Cost Explorer request failed: {error}"))?;
    let status = response.status().as_u16();
    let body_text = response.text().map_err(|error| error.to_string())?;
    Ok(AwsCostExplorerResponse { status, body_text })
}

fn execute_command_request(request: CommandRequest) -> Result<CommandResponse, String> {
    if request.program != "gh" {
        return Err(format!("command not allowed: {}", request.program));
    }
    if request.timeout_ms > 30_000 {
        return Err("command timeout exceeds 30000ms".to_string());
    }

    let mut command = Command::new(&request.program);
    command
        .args(&request.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = run_command_with_capped_output(
        &mut command,
        Duration::from_millis(request.timeout_ms),
        Duration::from_millis(COMMAND_POLL_INTERVAL_MS),
        COMMAND_OUTPUT_LIMIT_BYTES,
    )?;

    Ok(CommandResponse {
        status: output.status.code().unwrap_or(-1),
        stdout: decode_capped_output(&output.stdout),
        stderr: decode_capped_output(&output.stderr),
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

fn default_command_timeout_ms() -> u64 {
    10_000
}

fn default_cost_explorer_granularity() -> String {
    "MONTHLY".to_string()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capped_output_decode_drops_incomplete_trailing_utf8() {
        let mut bytes = "command ".as_bytes().to_vec();
        bytes.extend_from_slice(&[0xF0, 0x9F, 0x98]);

        assert_eq!(decode_capped_output(&bytes), "command ");
    }

    #[test]
    fn aws_uri_encoding_preserves_path_slashes_only_when_requested() {
        assert_eq!(aws_uri_encode("/cost explorer", false), "/cost%20explorer");
        assert_eq!(aws_uri_encode("/cost explorer", true), "%2Fcost%20explorer");
    }

    #[test]
    fn aws_host_header_includes_non_default_port() {
        let url = reqwest::Url::parse("https://ce.example.test:8443").unwrap();

        assert_eq!(aws_host_header(&url).unwrap(), "ce.example.test:8443");
    }
}
