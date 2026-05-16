mod auth_cookies;
mod batch_probe;
mod history;

use ai_usage_core::{
    AppConfig, LoadedProvider, MetricLine, NormalizedMetrics, ProgressFormat, ProviderSummary,
    UsageSnapshot, paths,
};
use ai_usage_plugins::discover_providers;
use anyhow::{Context, Result};
use chrono::{Local, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use tabled::{Table, Tabled, settings::Style};

#[derive(Debug, Parser)]
#[command(name = "ai-usage")]
#[command(about = "AI usage backend CLI")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(long, global = true)]
    json: bool,

    /// Emit JSON only. CodexBar-compatible alias for --json.
    #[arg(long = "json-only", global = true)]
    json_only: bool,

    /// Pretty-print JSON output. JSON is currently pretty-printed by default.
    #[arg(long, global = true)]
    pretty: bool,

    /// Accept CodexBar-compatible structured log flag.
    #[arg(long = "json-output", global = true)]
    json_output: bool,

    /// Accept CodexBar-compatible log level.
    #[arg(long = "log-level", global = true)]
    log_level: Option<String>,

    /// Enable verbose logging.
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Disable ANSI colors in text output.
    #[arg(long = "no-color", global = true)]
    no_color: bool,

    /// No tables or ANSI; plain text output
    #[arg(long, global = true)]
    plain: bool,

    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,

    #[arg(long = "plugin-dir", global = true, value_name = "DIR")]
    plugin_dirs: Vec<PathBuf>,

    /// Include disabled providers
    #[arg(long, global = true)]
    all: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Probe one or more providers and show live usage
    Usage {
        provider_ids: Vec<String>,
        /// Provider to query. Accepts a provider id, all, or both.
        #[arg(long)]
        provider: Option<String>,
        /// Output format: text or json.
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
        /// Token account label to use. Accepted for compatibility.
        #[arg(long)]
        account: Option<String>,
        /// Token account index, 1-based. Accepted for compatibility.
        #[arg(long = "account-index")]
        account_index: Option<usize>,
        /// Fetch all token accounts. Accepted for compatibility.
        #[arg(long = "all-accounts")]
        all_accounts: bool,
        /// Append results to ~/.local/share/ai-usage/history.jsonl
        #[arg(long)]
        save: bool,
        /// Fetch and print provider status-page state when available
        #[arg(long)]
        status: bool,
        /// Override source mode for all selected providers (auto, web, cli, oauth, api, local).
        #[arg(long, value_enum)]
        source: Option<SourceMode>,
        /// Alias for --source web.
        #[arg(long)]
        web: bool,
        /// Accepted for compatibility; provider web timeouts use AI_USAGE_PROBE_TIMEOUT_SEC today.
        #[arg(long = "web-timeout")]
        web_timeout: Option<f64>,
        /// Accepted for compatibility.
        #[arg(long = "web-debug-dump-html")]
        web_debug_dump_html: bool,
        /// Accepted for compatibility.
        #[arg(long = "antigravity-plan-debug")]
        antigravity_plan_debug: bool,
        /// Accepted for compatibility.
        #[arg(long = "augment-debug")]
        augment_debug: bool,
        /// Accepted for compatibility.
        #[arg(long = "no-credits")]
        no_credits: bool,
    },
    /// List discovered providers and their enabled status
    List {
        provider_ids: Vec<String>,
        /// Provider to show. Accepts a provider id, all, or both.
        #[arg(long)]
        provider: Option<String>,
    },
    /// Probe one or more providers and show live usage
    Probe {
        provider_ids: Vec<String>,
        /// Provider to query. Accepts a provider id, all, or both.
        #[arg(long)]
        provider: Option<String>,
        /// Output format: text or json.
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
        /// Token account label to use. Accepted for compatibility.
        #[arg(long)]
        account: Option<String>,
        /// Token account index, 1-based. Accepted for compatibility.
        #[arg(long = "account-index")]
        account_index: Option<usize>,
        /// Fetch all token accounts. Accepted for compatibility.
        #[arg(long = "all-accounts")]
        all_accounts: bool,
        /// Append results to ~/.local/share/ai-usage/history.jsonl
        #[arg(long)]
        save: bool,
        /// Fetch and print provider status-page state when available
        #[arg(long)]
        status: bool,
        /// Override source mode for all selected providers (auto, web, cli, oauth, api, local).
        #[arg(long, value_enum)]
        source: Option<SourceMode>,
        /// Alias for --source web.
        #[arg(long)]
        web: bool,
        /// Accepted for compatibility; provider web timeouts use AI_USAGE_PROBE_TIMEOUT_SEC today.
        #[arg(long = "web-timeout")]
        web_timeout: Option<f64>,
        /// Accepted for compatibility.
        #[arg(long = "web-debug-dump-html")]
        web_debug_dump_html: bool,
        /// Accepted for compatibility.
        #[arg(long = "antigravity-plan-debug")]
        antigravity_plan_debug: bool,
        /// Accepted for compatibility.
        #[arg(long = "augment-debug")]
        augment_debug: bool,
        /// Accepted for compatibility.
        #[arg(long = "no-credits")]
        no_credits: bool,
    },
    /// Fetch provider status-page state without probing usage
    Status {
        provider_ids: Vec<String>,
        /// Provider to check. Accepts a provider id, all, or both.
        #[arg(long)]
        provider: Option<String>,
    },
    /// Print normalized cost data from live snapshots or saved history
    Cost {
        provider_ids: Vec<String>,
        /// Provider to query. Accepts a provider id, all, or both.
        #[arg(long)]
        provider: Option<String>,
        /// Output format. Defaults to text.
        #[arg(long, value_enum, default_value_t = CostFormat::Text)]
        format: CostFormat,
        /// Read from a JSONL history file instead of probing live
        #[arg(long)]
        from_file: Option<PathBuf>,
        /// Force refresh by ignoring cached scans. Accepted for compatibility.
        #[arg(long)]
        refresh: bool,
        /// Number of days of history to return (JSON mode only; default 30).
        #[arg(long, default_value_t = 30)]
        days: u32,
    },
    /// Export usage as JSON or CSV (live probe, or read prior JSONL history)
    Export {
        #[arg(long, value_enum, default_value_t = ExportFormat::Json)]
        format: ExportFormat,
        /// Read from a JSONL history file instead of probing live
        #[arg(long)]
        from_file: Option<PathBuf>,
        /// Provider to query. Accepts a provider id, all, or both.
        #[arg(long)]
        provider: Option<String>,
        provider_ids: Vec<String>,
    },
    Plugin {
        #[command(subcommand)]
        command: PluginCommand,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Cache {
        #[command(subcommand)]
        command: CacheCommand,
    },
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
}

#[derive(Debug, Subcommand)]
enum PluginCommand {
    Validate,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Validate the config file
    Validate {
        /// Output format: text or json.
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
    },
    /// Print normalized config JSON
    Dump {
        /// Output format: text or json. Dump always emits JSON.
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
    },
}

#[derive(Debug, Subcommand)]
enum CacheCommand {
    /// Clear cached snapshots and/or saved history
    Clear {
        /// Clear ~/.local/share/ai-usage/snapshots.json
        #[arg(long)]
        snapshots: bool,
        /// Clear ~/.local/share/ai-usage/history.jsonl
        #[arg(long)]
        history: bool,
        /// Clear all backend caches
        #[arg(long)]
        all: bool,
        /// CodexBar-compatible alias for clearing cookie caches. No-op until cookie caches exist.
        #[arg(long)]
        cookies: bool,
        /// CodexBar-compatible alias for clearing cost caches. Maps to history today.
        #[arg(long)]
        cost: bool,
        /// CodexBar-compatible provider cache scope. No-op until per-provider caches exist.
        #[arg(long)]
        provider: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum AuthCommand {
    /// Import browser cookies into a raw Cookie header
    ImportCookies {
        /// Provider whose browser cookies should be imported.
        #[arg(long)]
        provider: String,
        /// Output format. Defaults to text.
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum ExportFormat {
    #[default]
    Json,
    Csv,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum OutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum CostFormat {
    #[default]
    Text,
    Json,
    Csv,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum SourceMode {
    Auto,
    Web,
    Cli,
    Oauth,
    Api,
    Local,
}

impl SourceMode {
    fn as_str(self) -> &'static str {
        match self {
            SourceMode::Auto => "auto",
            SourceMode::Web => "web",
            SourceMode::Cli => "cli",
            SourceMode::Oauth => "oauth",
            SourceMode::Api => "api",
            SourceMode::Local => "local",
        }
    }
}

fn resolve_source_mode(
    cli_source: Option<SourceMode>,
    web: bool,
    provider_id: &str,
    config: &AppConfig,
) -> String {
    if web {
        return "web".to_string();
    }
    if let Some(mode) = cli_source {
        return mode.as_str().to_string();
    }
    config.source_mode(provider_id).to_string()
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse_from(effective_args());
    let json = cli.json || cli.json_only;
    let _compat_globals = (
        &cli.pretty,
        &cli.json_output,
        &cli.log_level,
        &cli.verbose,
        &cli.no_color,
    );
    let config_path = cli.config.clone().unwrap_or_else(paths::config_file);
    let config = AppConfig::load_optional(&config_path)
        .with_context(|| format!("load config {}", config_path.display()))?;
    let plugin_dirs = paths::plugin_dirs(&config, &cli.plugin_dirs);
    let providers = discover_providers(&plugin_dirs);

    match cli.command.unwrap_or(Command::Usage {
        provider_ids: Vec::new(),
        provider: None,
        format: None,
        account: None,
        account_index: None,
        all_accounts: false,
        save: false,
        status: false,
        source: None,
        web: false,
        web_timeout: None,
        web_debug_dump_html: false,
        antigravity_plan_debug: false,
        augment_debug: false,
        no_credits: false,
    }) {
        Command::Usage {
            provider_ids,
            provider,
            format,
            account,
            account_index,
            all_accounts,
            save,
            status,
            source,
            web,
            web_timeout,
            web_debug_dump_html,
            antigravity_plan_debug,
            augment_debug,
            no_credits,
        } => {
            let selection = provider_selection(provider_ids, provider);
            warn_unsupported_usage_compat(
                json,
                web_timeout,
                account.as_deref(),
                account_index,
                all_accounts,
                web_debug_dump_html,
                antigravity_plan_debug,
                augment_debug,
                no_credits,
            );
            let json_output = json || matches!(format, Some(OutputFormat::Json));
            run_probe(
                &providers,
                &config,
                &selection.ids,
                cli.all || selection.include_disabled,
                json_output,
                cli.plain,
                save,
                status,
                source,
                web,
            )
        }
        Command::List {
            provider_ids,
            provider,
        } => {
            let selection = provider_selection(provider_ids, provider);
            run_list(&providers, &config, &selection.ids, json, cli.plain)
        }
        Command::Probe {
            provider_ids,
            provider,
            format,
            account,
            account_index,
            all_accounts,
            save,
            status,
            source,
            web,
            web_timeout,
            web_debug_dump_html,
            antigravity_plan_debug,
            augment_debug,
            no_credits,
        } => {
            let selection = provider_selection(provider_ids, provider);
            warn_unsupported_usage_compat(
                json,
                web_timeout,
                account.as_deref(),
                account_index,
                all_accounts,
                web_debug_dump_html,
                antigravity_plan_debug,
                augment_debug,
                no_credits,
            );
            let json_output = json || matches!(format, Some(OutputFormat::Json));
            run_probe(
                &providers,
                &config,
                &selection.ids,
                cli.all || selection.include_disabled,
                json_output,
                cli.plain,
                save,
                status,
                source,
                web,
            )
        }
        Command::Status {
            provider_ids,
            provider,
        } => {
            let selection = provider_selection(provider_ids, provider);
            run_status(
                &providers,
                &config,
                &selection.ids,
                cli.all || selection.include_disabled,
                json,
                cli.plain,
            )
        }
        Command::Cost {
            provider_ids,
            provider,
            format,
            from_file,
            refresh,
            days,
        } => {
            let selection = provider_selection(provider_ids, provider);
            if refresh && !json {
                eprintln!(
                    "ai-usage: --refresh is accepted for compatibility; live snapshot cost has no cache to bypass"
                );
            }
            let json_output = json || matches!(format, CostFormat::Json);
            if json_output && from_file.is_none() {
                run_cost_historical(&selection.ids, days)
            } else {
                run_cost(
                    &providers,
                    &config,
                    &selection.ids,
                    cli.all || selection.include_disabled,
                    format,
                    from_file,
                    json,
                    cli.plain,
                )
            }
        }
        Command::Export {
            format,
            from_file,
            provider,
            provider_ids,
        } => {
            let selection = provider_selection(provider_ids, provider);
            run_export(
                &providers,
                &config,
                &selection.ids,
                cli.all || selection.include_disabled,
                format,
                from_file,
            )
        }
        Command::Plugin {
            command: PluginCommand::Validate,
        } => run_validate(&providers, &config, json),
        Command::Config {
            command: ConfigCommand::Validate { format },
        } => run_config_validate(
            &config_path,
            &config,
            json || matches!(format, Some(OutputFormat::Json)),
        ),
        Command::Config {
            command: ConfigCommand::Dump { format },
        } => run_config_dump(&config, json || matches!(format, Some(OutputFormat::Json))),
        Command::Cache {
            command:
                CacheCommand::Clear {
                    snapshots,
                    history,
                    all,
                    cookies,
                    cost,
                    provider,
                },
        } => run_cache_clear(snapshots, history, all, cookies, cost, provider, json),
        Command::Auth {
            command: AuthCommand::ImportCookies { provider, format },
        } => run_auth_import_cookies(
            &providers,
            provider,
            json || matches!(format, OutputFormat::Json),
        ),
    }
}

fn effective_args() -> Vec<OsString> {
    let args: Vec<OsString> = std::env::args_os().collect();
    if args.len() <= 1 {
        return args;
    }
    if args.iter().skip(1).any(|arg| {
        arg.to_str()
            .is_some_and(|s| matches!(s, "-h" | "--help" | "-V" | "--version"))
    }) {
        return args;
    }

    let has_command = args.iter().skip(1).any(|arg| {
        arg.to_str().is_some_and(|s| {
            matches!(
                s,
                "usage"
                    | "list"
                    | "probe"
                    | "status"
                    | "cost"
                    | "export"
                    | "plugin"
                    | "config"
                    | "cache"
                    | "auth"
                    | "help"
            )
        })
    });
    if has_command {
        return args;
    }

    let mut normalized = Vec::with_capacity(args.len() + 1);
    normalized.push(args[0].clone());
    normalized.push(OsString::from("usage"));
    normalized.extend(args.into_iter().skip(1));
    normalized
}

// ── list ─────────────────────────────────────────────────────────────────────

#[derive(Tabled)]
struct ListRow {
    id: String,
    name: String,
    status: String,
    modes: String,
}

fn run_list(
    providers: &[LoadedProvider],
    config: &AppConfig,
    provider_ids: &[String],
    json: bool,
    plain: bool,
) -> Result<()> {
    let mut summaries = provider_summaries(providers, config);
    if !provider_ids.is_empty() {
        summaries.retain(|s| {
            provider_ids
                .iter()
                .map(|id| normalize_provider_id(id))
                .any(|id| id == s.id)
        });
    }
    summaries.sort_by(|a, b| a.id.cmp(&b.id));

    if json {
        println!("{}", serde_json::to_string_pretty(&summaries)?);
        return Ok(());
    }

    if plain {
        for s in &summaries {
            println!(
                "{}\t{}\t{}\t{}",
                s.id,
                s.name,
                if s.enabled { "enabled" } else { "disabled" },
                format_modes(s),
            );
        }
        return Ok(());
    }

    let rows: Vec<ListRow> = summaries
        .iter()
        .map(|s| ListRow {
            id: s.id.clone(),
            name: s.name.clone(),
            status: if s.enabled {
                "enabled".into()
            } else {
                "disabled".into()
            },
            modes: format_modes(s),
        })
        .collect();

    let mut table = Table::new(rows);
    table.with(Style::rounded());
    println!("{table}");
    Ok(())
}

// ── probe ─────────────────────────────────────────────────────────────────────

fn run_probe(
    providers: &[LoadedProvider],
    config: &AppConfig,
    provider_ids: &[String],
    include_disabled: bool,
    json: bool,
    plain: bool,
    save: bool,
    include_status: bool,
    cli_source: Option<SourceMode>,
    web: bool,
) -> Result<()> {
    let selected = select_providers(providers.to_vec(), config, provider_ids, include_disabled);
    if selected.is_empty() {
        eprintln!("ai-usage: no providers to probe");
        return Ok(());
    }

    let interrupt = batch_probe::register_interrupt_flag()?;
    let n = selected.len();
    let tmax = batch_probe::probe_timeout_secs();
    if !json {
        eprintln!(
            "ai-usage: probing {n} provider(s)… (up to {tmax}s each; AI_USAGE_PROBE_TIMEOUT_SEC to override)"
        );
    }

    let mut snapshots: Vec<UsageSnapshot> = Vec::new();
    let mut statuses = Vec::new();
    for (i, provider) in selected.iter().enumerate() {
        if !json {
            eprintln!("ai-usage:   [{}/{}] {}…", i + 1, n, provider.manifest.id);
        }
        let source = resolve_source_mode(cli_source, web, &provider.manifest.id, config);
        let mut snap = batch_probe::run_probe_with_timeout(provider, &source, Some(&interrupt));
        snap.status_page_url = status_page_url_for(&provider.manifest.id);
        snap.pace = snap.compute_pace();
        if save {
            let rec = history::record_from_snapshot(&snap);
            if let Err(e) = history::append_jsonl(&rec) {
                eprintln!("ai-usage: warning: failed to save history: {e}");
            }
        }
        if include_status {
            statuses.push(fetch_provider_status(provider));
        }
        snapshots.push(snap);
    }

    if json {
        if include_status {
            let payload: Vec<UsageWithStatus> = snapshots
                .into_iter()
                .zip(statuses)
                .map(|(usage, status)| UsageWithStatus { usage, status })
                .collect();
            println!("{}", serde_json::to_string_pretty(&payload)?);
        } else {
            println!("{}", serde_json::to_string_pretty(&snapshots)?);
        }
        return Ok(());
    }

    for (idx, snap) in snapshots.iter().enumerate() {
        print_snapshot(snap, plain);
        if let Some(status) = statuses.get(idx) {
            print_status(status, plain);
        }
    }
    Ok(())
}

#[derive(Tabled)]
struct MetricRow {
    label: String,
    value: String,
}

fn format_metric_value(line: &MetricLine) -> (String, String) {
    match line {
        MetricLine::Text {
            label,
            value,
            subtitle,
            ..
        } => {
            let mut v = value.clone();
            if let Some(s) = subtitle {
                v.push_str(&format!(" ({s})"));
            }
            (label.clone(), v)
        }
        MetricLine::Badge {
            label,
            text,
            subtitle,
            ..
        } => {
            let mut v = text.clone();
            if let Some(s) = subtitle {
                v.push_str(&format!(" ({s})"));
            }
            (label.clone(), v)
        }
        MetricLine::Progress {
            label,
            used,
            limit,
            format,
            resets_at,
            ..
        } => {
            let pct = if *limit > 0.0 {
                used / limit * 100.0
            } else {
                0.0
            };
            let mut v = match format {
                ProgressFormat::Percent => format!("{pct:.1}% ({used:.0} / {limit:.0})"),
                ProgressFormat::Dollars => format!("${used:.2} / ${limit:.2}"),
                ProgressFormat::Count { suffix } => format!("{used:.0} / {limit:.0} {suffix}"),
            };
            if let Some(dt) = resets_at {
                v.push_str(&format!("  {}", format_reset_text(dt)));
            }
            (label.clone(), v)
        }
    }
}

fn format_reset_text(dt: &chrono::DateTime<Utc>) -> String {
    let now = Utc::now();
    if *dt > now {
        let delta = *dt - now;
        if delta.num_hours() < 24 {
            let hours = delta.num_hours();
            let minutes = (delta.num_minutes() - hours * 60).max(0);
            if hours > 0 {
                return format!("resets in {hours}h {minutes}m");
            }
            return format!("resets in {minutes}m");
        }
    }

    let local = dt.with_timezone(&Local);
    local.format("resets %a %-I:%M %p").to_string()
}

fn print_snapshot(snap: &UsageSnapshot, plain: bool) {
    let title = format!("{}  ({})", snap.display_name, snap.provider_id);
    if plain {
        println!("=== {title} ===");
    } else {
        println!("{title}");
    }
    if let Some(ref plan) = snap.plan {
        println!("Plan: {plan}");
    }

    let rows: Vec<MetricRow> = snap
        .metrics
        .iter()
        .map(|l| {
            let (label, value) = format_metric_value(l);
            MetricRow { label, value }
        })
        .collect();

    if rows.is_empty() {
        println!("  (no metrics)");
    } else if plain {
        for r in &rows {
            println!("  {}: {}", r.label, r.value);
        }
    } else {
        let mut table = Table::new(&rows);
        table.with(Style::rounded());
        println!("{table}");
    }
    println!();
}

// ── status ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderStatus {
    provider_id: String,
    display_name: String,
    status_url: Option<String>,
    indicator: String,
    description: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UsageWithStatus {
    usage: UsageSnapshot,
    status: ProviderStatus,
}

#[derive(Tabled)]
struct StatusRow {
    id: String,
    name: String,
    indicator: String,
    description: String,
}

fn run_status(
    providers: &[LoadedProvider],
    config: &AppConfig,
    provider_ids: &[String],
    include_disabled: bool,
    json: bool,
    plain: bool,
) -> Result<()> {
    let selected = select_providers(providers.to_vec(), config, provider_ids, include_disabled);
    if selected.is_empty() {
        eprintln!("ai-usage: no providers to check");
        return Ok(());
    }

    let statuses: Vec<ProviderStatus> = selected.iter().map(fetch_provider_status).collect();
    if json {
        println!("{}", serde_json::to_string_pretty(&statuses)?);
        return Ok(());
    }

    if plain {
        for status in &statuses {
            print_status(status, true);
        }
        return Ok(());
    }

    let rows: Vec<StatusRow> = statuses
        .iter()
        .map(|s| StatusRow {
            id: s.provider_id.clone(),
            name: s.display_name.clone(),
            indicator: s.indicator.clone(),
            description: s
                .description
                .clone()
                .or_else(|| s.error.clone())
                .unwrap_or_default(),
        })
        .collect();
    let mut table = Table::new(rows);
    table.with(Style::rounded());
    println!("{table}");
    Ok(())
}

fn print_status(status: &ProviderStatus, plain: bool) {
    let description = status
        .description
        .as_deref()
        .or(status.error.as_deref())
        .unwrap_or("");
    if plain {
        println!(
            "{}\t{}\t{}\t{}",
            status.provider_id, status.display_name, status.indicator, description
        );
    } else {
        println!("Status: {} {}", status.indicator, description);
    }
}

fn fetch_provider_status(provider: &LoadedProvider) -> ProviderStatus {
    let url = provider
        .manifest
        .links
        .iter()
        .find(|l| l.label.eq_ignore_ascii_case("status"))
        .map(|l| l.url.clone());

    let Some(status_url) = url else {
        return ProviderStatus {
            provider_id: provider.manifest.id.clone(),
            display_name: provider.manifest.name.clone(),
            status_url: None,
            indicator: "unknown".to_string(),
            description: Some("No status URL in provider manifest".to_string()),
            error: None,
        };
    };

    let api_url = format!("{}/api/v2/status.json", status_url.trim_end_matches('/'));
    let result = reqwest::blocking::Client::new()
        .get(&api_url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .and_then(|resp| resp.error_for_status())
        .and_then(|resp| resp.json::<serde_json::Value>());

    match result {
        Ok(json) => {
            let indicator = json
                .pointer("/status/indicator")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let description = json
                .pointer("/status/description")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            ProviderStatus {
                provider_id: provider.manifest.id.clone(),
                display_name: provider.manifest.name.clone(),
                status_url: Some(status_url),
                indicator,
                description,
                error: None,
            }
        }
        Err(error) => ProviderStatus {
            provider_id: provider.manifest.id.clone(),
            display_name: provider.manifest.name.clone(),
            status_url: Some(status_url),
            indicator: "unknown".to_string(),
            description: None,
            error: Some(error.to_string()),
        },
    }
}

// ── cost historical ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CostHistoricalResponse {
    provider: String,
    currency: &'static str,
    daily: Vec<DailyEntry>,
    totals: DailyTotals,
    period_days: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DailyEntry {
    date: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
    total_tokens: u64,
    total_cost: f64,
    model_breakdowns: Vec<ModelBreakdown>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DailyTotals {
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
    total_tokens: u64,
    total_cost: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelBreakdown {
    model_name: String,
    cost: f64,
    total_tokens: u64,
}

#[derive(Debug, Serialize)]
struct CostUnsupportedResponse {
    error: CostUnsupportedError,
}

#[derive(Debug, Serialize)]
struct CostUnsupportedError {
    code: &'static str,
    message: &'static str,
}

fn unsupported_cost_response() -> CostUnsupportedResponse {
    CostUnsupportedResponse {
        error: CostUnsupportedError {
            code: "UNSUPPORTED",
            message: "Cost data not available for this provider",
        },
    }
}

fn run_cost_historical(provider_ids: &[String], days: u32) -> Result<()> {
    let targets: Vec<&str> = if provider_ids.is_empty() {
        vec!["claude", "codex"]
    } else {
        provider_ids
            .iter()
            .map(|id| match normalize_provider_id(id) {
                "openai" | "chatgpt" => "codex",
                other => other,
            })
            .collect()
    };

    let cutoff = chrono::Utc::now().date_naive() - chrono::Duration::days(days as i64);

    let mut results: Vec<serde_json::Value> = Vec::new();
    for &target in &targets {
        let canonical = match target {
            "claude" | "anthropic" => "claude",
            "codex" | "openai" | "chatgpt" => "codex",
            _ => {
                results.push(serde_json::to_value(unsupported_cost_response())?);
                continue;
            }
        };

        let output = std::process::Command::new("codexbar")
            .args(["cost", "--provider", canonical, "--format", "json"])
            .output();

        let output = match output {
            Err(_) => {
                results.push(serde_json::to_value(unsupported_cost_response())?);
                continue;
            }
            Ok(o) => o,
        };

        let raw: serde_json::Value = match serde_json::from_slice(&output.stdout) {
            Ok(v) => v,
            Err(_) => {
                results.push(serde_json::to_value(unsupported_cost_response())?);
                continue;
            }
        };

        // codexbar outputs a JSON array; take the first element
        let entry = match raw.as_array().and_then(|a| a.first()) {
            Some(e) => e,
            None => {
                results.push(serde_json::to_value(unsupported_cost_response())?);
                continue;
            }
        };

        // If the entry itself has an error field, it failed
        if entry.get("error").is_some() {
            results.push(serde_json::to_value(unsupported_cost_response())?);
            continue;
        }

        let daily_raw = entry.get("daily").and_then(|d| d.as_array());
        let mut daily: Vec<DailyEntry> = daily_raw
            .map(|arr| {
                arr.iter()
                    .filter_map(|d| {
                        let date = d.get("date")?.as_str()?.to_string();
                        // Filter to requested window
                        if let Ok(parsed) = date.parse::<chrono::NaiveDate>() {
                            if parsed <= cutoff {
                                return None;
                            }
                        }
                        let input_tokens = d
                            .get("inputTokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        let output_tokens = d
                            .get("outputTokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        let cache_read_tokens = d
                            .get("cacheReadTokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        let cache_creation_tokens = d
                            .get("cacheCreationTokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        let total_tokens = d
                            .get("totalTokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(
                                input_tokens
                                    + output_tokens
                                    + cache_read_tokens
                                    + cache_creation_tokens,
                            );
                        let total_cost = d
                            .get("totalCost")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        let model_breakdowns = d
                            .get("modelBreakdowns")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|m| {
                                        Some(ModelBreakdown {
                                            model_name: m
                                                .get("modelName")?
                                                .as_str()?
                                                .to_string(),
                                            cost: m
                                                .get("cost")
                                                .and_then(|v| v.as_f64())
                                                .unwrap_or(0.0),
                                            total_tokens: m
                                                .get("totalTokens")
                                                .and_then(|v| v.as_u64())
                                                .unwrap_or(0),
                                        })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        Some(DailyEntry {
                            date,
                            input_tokens,
                            output_tokens,
                            cache_read_tokens,
                            cache_creation_tokens,
                            total_tokens,
                            total_cost,
                            model_breakdowns,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Sort oldest → newest
        daily.sort_by(|a, b| a.date.cmp(&b.date));

        // Build totals from the filtered daily entries
        let totals = DailyTotals {
            input_tokens: daily.iter().map(|d| d.input_tokens).sum(),
            output_tokens: daily.iter().map(|d| d.output_tokens).sum(),
            cache_read_tokens: daily.iter().map(|d| d.cache_read_tokens).sum(),
            cache_creation_tokens: daily.iter().map(|d| d.cache_creation_tokens).sum(),
            total_tokens: daily.iter().map(|d| d.total_tokens).sum(),
            total_cost: daily.iter().map(|d| d.total_cost).sum(),
        };

        results.push(serde_json::to_value(CostHistoricalResponse {
            provider: canonical.to_string(),
            currency: "USD",
            daily,
            totals,
            period_days: days,
        })?);
    }

    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

// ── cost ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CostRecord {
    provider_id: String,
    display_name: String,
    source: String,
    cost: Option<f64>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    fetched_at: String,
}

#[derive(Tabled)]
struct CostRow {
    provider: String,
    cost: String,
    input_tokens: String,
    output_tokens: String,
    fetched_at: String,
}

fn run_cost(
    providers: &[LoadedProvider],
    config: &AppConfig,
    provider_ids: &[String],
    include_disabled: bool,
    format: CostFormat,
    from_file: Option<PathBuf>,
    json: bool,
    plain: bool,
) -> Result<()> {
    let records = if let Some(path) = from_file {
        cost_records_from_history(&path, provider_ids)?
    } else {
        let selected = select_providers(providers.to_vec(), config, provider_ids, include_disabled);
        if selected.is_empty() {
            eprintln!("ai-usage: no providers for cost");
            return Ok(());
        }
        let interrupt = batch_probe::register_interrupt_flag()?;
        selected
            .iter()
            .map(|provider| {
                let source = config.source_mode(&provider.manifest.id).to_string();
                let snap = batch_probe::run_probe_with_timeout(provider, &source, Some(&interrupt));
                cost_record_from_snapshot(&snap)
            })
            .collect()
    };

    match if json { CostFormat::Json } else { format } {
        CostFormat::Text => {
            if plain {
                for r in &records {
                    println!(
                        "{}\t{}\t{}\t{}\t{}\t{}",
                        r.provider_id,
                        r.display_name,
                        r.cost.map(|c| format!("{c:.6}")).unwrap_or_default(),
                        r.input_tokens.map(|n| n.to_string()).unwrap_or_default(),
                        r.output_tokens.map(|n| n.to_string()).unwrap_or_default(),
                        r.fetched_at,
                    );
                }
            } else {
                print_cost_table(&records);
            }
        }
        CostFormat::Json => println!("{}", serde_json::to_string_pretty(&records)?),
        CostFormat::Csv => print_cost_csv(&records)?,
    }
    Ok(())
}

fn cost_records_from_history(
    path: &std::path::Path,
    provider_ids: &[String],
) -> Result<Vec<CostRecord>> {
    let records = history::read_jsonl(path)?;
    let ids: std::collections::HashSet<&str> = provider_ids
        .iter()
        .map(|id| normalize_provider_id(id))
        .collect();
    Ok(records
        .into_iter()
        .filter(|r| ids.is_empty() || ids.contains(r.provider_id.as_str()))
        .map(|r| CostRecord {
            provider_id: r.provider_id,
            display_name: r.display_name,
            source: "history".to_string(),
            cost: r.cost,
            input_tokens: r.input_tokens,
            output_tokens: r.output_tokens,
            fetched_at: r.ts,
        })
        .collect())
}

fn cost_record_from_snapshot(snapshot: &UsageSnapshot) -> CostRecord {
    let metrics = NormalizedMetrics::from_snapshot(snapshot);
    CostRecord {
        provider_id: snapshot.provider_id.clone(),
        display_name: snapshot.display_name.clone(),
        source: snapshot
            .source
            .clone()
            .unwrap_or_else(|| "live".to_string()),
        cost: metrics.cost,
        input_tokens: metrics.input_tokens,
        output_tokens: metrics.output_tokens,
        fetched_at: snapshot.fetched_at.to_rfc3339(),
    }
}

fn print_cost_csv(records: &[CostRecord]) -> Result<()> {
    use std::io::Write;
    let mut w = std::io::stdout().lock();
    writeln!(
        w,
        "provider_id,display_name,source,cost,input_tokens,output_tokens,fetched_at"
    )?;
    for r in records {
        writeln!(
            w,
            "{},{},{},{},{},{},{}",
            csv_cell(&r.provider_id),
            csv_cell(&r.display_name),
            csv_cell(&r.source),
            r.cost.map(|c| format!("{c:.6}")).unwrap_or_default(),
            r.input_tokens.map(|n| n.to_string()).unwrap_or_default(),
            r.output_tokens.map(|n| n.to_string()).unwrap_or_default(),
            csv_cell(&r.fetched_at),
        )?;
    }
    Ok(())
}

fn csv_cell(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn print_cost_table(records: &[CostRecord]) {
    let rows: Vec<CostRow> = records
        .iter()
        .map(|r| CostRow {
            provider: r.provider_id.clone(),
            cost: r
                .cost
                .map(|c| format!("${c:.2}"))
                .unwrap_or_else(|| "-".to_string()),
            input_tokens: r
                .input_tokens
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".to_string()),
            output_tokens: r
                .output_tokens
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".to_string()),
            fetched_at: r.fetched_at.clone(),
        })
        .collect();
    let mut table = Table::new(rows);
    table.with(Style::rounded());
    println!("{table}");
}

// ── export ────────────────────────────────────────────────────────────────────

fn run_export(
    providers: &[LoadedProvider],
    config: &AppConfig,
    provider_ids: &[String],
    include_disabled: bool,
    format: ExportFormat,
    from_file: Option<PathBuf>,
) -> Result<()> {
    let mut records = if let Some(ref path) = from_file {
        history::read_jsonl(path)?
    } else {
        let selected = select_providers(providers.to_vec(), config, provider_ids, include_disabled);
        if selected.is_empty() {
            eprintln!("ai-usage: no providers to export");
            return Ok(());
        }

        let interrupt = batch_probe::register_interrupt_flag()?;
        let n = selected.len();
        let tmax = batch_probe::probe_timeout_secs();
        eprintln!("ai-usage: probing {n} provider(s) for export… (up to {tmax}s each)");

        let mut recs = Vec::new();
        for (i, provider) in selected.iter().enumerate() {
            eprintln!("ai-usage:   [{}/{}] {}…", i + 1, n, provider.manifest.id);
            let source = config.source_mode(&provider.manifest.id).to_string();
            let snap = batch_probe::run_probe_with_timeout(provider, &source, Some(&interrupt));
            recs.push(history::record_from_snapshot(&snap));
        }
        recs
    };

    // Filter by provider_ids when reading from file
    if !provider_ids.is_empty() && from_file.is_some() {
        let ids: std::collections::HashSet<&str> = provider_ids
            .iter()
            .map(|id| normalize_provider_id(id))
            .collect();
        records.retain(|r| ids.contains(r.provider_id.as_str()));
    }

    match format {
        ExportFormat::Json => println!("{}", serde_json::to_string_pretty(&records)?),
        ExportFormat::Csv => history::print_csv(&records)?,
    }
    Ok(())
}

// ── plugin validate ───────────────────────────────────────────────────────────

fn run_validate(providers: &[LoadedProvider], config: &AppConfig, json: bool) -> Result<()> {
    let summaries = provider_summaries(providers, config);
    if json {
        println!("{}", serde_json::to_string_pretty(&summaries)?);
    } else {
        println!("validated {} plugin(s)", summaries.len());
        for s in &summaries {
            println!("{}\t{}", s.id, s.name);
        }
    }
    Ok(())
}

// ── config ───────────────────────────────────────────────────────────────────

fn run_config_validate(
    config_path: &std::path::Path,
    _config: &AppConfig,
    json: bool,
) -> Result<()> {
    #[derive(Serialize)]
    struct ConfigValidation<'a> {
        path: String,
        valid: bool,
        issues: Vec<&'a str>,
    }

    let result = ConfigValidation {
        path: config_path.display().to_string(),
        valid: true,
        issues: Vec::new(),
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("Config: OK ({})", config_path.display());
    }
    Ok(())
}

fn run_config_dump(config: &AppConfig, _json: bool) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(config)?);
    Ok(())
}

// ── cache ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct CacheClearResult {
    cache: String,
    path: String,
    cleared: bool,
    error: Option<String>,
}

fn run_cache_clear(
    snapshots: bool,
    history_flag: bool,
    all: bool,
    cookies: bool,
    cost: bool,
    provider: Option<String>,
    json: bool,
) -> Result<()> {
    let clear_snapshots = all || snapshots;
    let clear_history = all || history_flag || cost;

    if provider.is_some() && !json {
        eprintln!(
            "ai-usage: --provider cache scoping is accepted for compatibility; backend caches are not per-provider yet"
        );
    }
    if cookies && !json {
        eprintln!(
            "ai-usage: --cookies is accepted for compatibility; no backend cookie cache exists yet"
        );
    }

    if !clear_snapshots && !clear_history && !cookies {
        anyhow::bail!("Specify --snapshots, --history, --cookies, --cost, or --all.");
    }

    let mut results = Vec::new();
    if cookies {
        results.push(CacheClearResult {
            cache: "cookies".to_string(),
            path: provider
                .as_ref()
                .map(|p| format!("provider:{p}"))
                .unwrap_or_else(|| "all providers".to_string()),
            cleared: false,
            error: None,
        });
    }
    if clear_snapshots {
        results.push(clear_file("snapshots", paths::cache_file()));
    }
    if clear_history {
        results.push(clear_file("history", history::history_jsonl_path()));
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        for result in &results {
            if let Some(error) = &result.error {
                println!(
                    "{}: failed to clear ({}) - {}",
                    result.cache, result.path, error
                );
            } else if result.cleared {
                println!("{}: cleared ({})", result.cache, result.path);
            } else {
                println!("{}: nothing to clear ({})", result.cache, result.path);
            }
        }
    }

    if results.iter().any(|r| r.error.is_some()) {
        anyhow::bail!("failed to clear one or more caches");
    }
    Ok(())
}

fn clear_file(cache: &str, path: PathBuf) -> CacheClearResult {
    match fs::remove_file(&path) {
        Ok(()) => CacheClearResult {
            cache: cache.to_string(),
            path: path.display().to_string(),
            cleared: true,
            error: None,
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => CacheClearResult {
            cache: cache.to_string(),
            path: path.display().to_string(),
            cleared: false,
            error: None,
        },
        Err(error) => CacheClearResult {
            cache: cache.to_string(),
            path: path.display().to_string(),
            cleared: false,
            error: Some(error.to_string()),
        },
    }
}

// ── auth ─────────────────────────────────────────────────────────────────────

fn run_auth_import_cookies(providers: &[LoadedProvider], provider: String, json: bool) -> Result<()> {
    let matched = providers
        .iter()
        .find(|p| p.manifest.id.eq_ignore_ascii_case(&provider));

    let web_url = match matched.and_then(|p| p.manifest.web_url.as_deref()) {
        Some(url) => url.to_string(),
        None => {
            let err = auth_cookies::CookieImportError {
                error: "NO_WEB_URL".to_string(),
                message: format!(
                    "Provider '{provider}' does not have a webUrl configured and does not support cookie import."
                ),
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&err)?);
                std::process::exit(1);
            }
            anyhow::bail!("{}: {}", err.error, err.message);
        }
    };

    let result = auth_cookies::import_cookies(&provider, &web_url);
    match result {
        Ok(imported) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&imported)?);
            } else {
                println!(
                    "Imported cookies for {} from {} profile {}.",
                    imported.provider_id, imported.source, imported.profile
                );
            }
            Ok(())
        }
        Err(error) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&error)?);
                std::process::exit(1);
            }
            anyhow::bail!("{}: {}", error.error, error.message);
        }
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

struct ProviderSelection {
    ids: Vec<String>,
    include_disabled: bool,
}

fn provider_selection(positional: Vec<String>, provider: Option<String>) -> ProviderSelection {
    let Some(raw) = provider else {
        return ProviderSelection {
            ids: positional,
            include_disabled: false,
        };
    };

    match raw.trim().to_lowercase().as_str() {
        "all" => ProviderSelection {
            ids: Vec::new(),
            include_disabled: false,
        },
        "both" => ProviderSelection {
            ids: vec!["codex".to_string(), "claude".to_string()],
            include_disabled: false,
        },
        id => ProviderSelection {
            ids: vec![normalize_provider_id(id).to_string()],
            include_disabled: false,
        },
    }
}

fn format_modes(s: &ProviderSummary) -> String {
    if s.supported_modes.is_empty() {
        return String::new();
    }
    let auto_suffix = if s.auto_mode.is_empty() {
        String::new()
    } else {
        format!(" (auto={})", s.auto_mode)
    };
    format!("{}{}", s.supported_modes.join(", "), auto_suffix)
}

fn status_page_url_for(provider_id: &str) -> Option<String> {
    let url = match provider_id {
        "claude" | "anthropic" => "https://status.anthropic.com/",
        "openai" | "codex" | "chatgpt" => "https://status.openai.com/",
        "github-copilot" | "copilot" => "https://www.githubstatus.com/",
        "gemini" | "google" => "https://status.cloud.google.com/",
        "cursor" => "https://www.cursor-status.com/",
        _ => return None,
    };
    Some(url.to_string())
}

fn normalize_provider_id(id: &str) -> &str {
    match id {
        "opencodego" | "opencode-go" => "opencode-go",
        "kimi-k2" | "kimik2" => "kimi-k2",
        "jetbrains" | "jetbrains-ai-assistant" => "jetbrains-ai-assistant",
        "z-ai" | "zai" => "zai",
        other => other,
    }
}

#[allow(clippy::too_many_arguments)]
fn warn_unsupported_usage_compat(
    json: bool,
    web_timeout: Option<f64>,
    account: Option<&str>,
    account_index: Option<usize>,
    all_accounts: bool,
    web_debug_dump_html: bool,
    antigravity_plan_debug: bool,
    augment_debug: bool,
    no_credits: bool,
) {
    if json {
        return;
    }
    if web_timeout.is_some() {
        eprintln!(
            "ai-usage: --web-timeout is accepted for compatibility; use AI_USAGE_PROBE_TIMEOUT_SEC for backend probe timeouts"
        );
    }
    if account.is_some() || account_index.is_some() || all_accounts {
        eprintln!(
            "ai-usage: account selection flags are accepted for compatibility; token account routing is not implemented yet"
        );
    }
    if web_debug_dump_html || antigravity_plan_debug || augment_debug {
        eprintln!(
            "ai-usage: provider debug flags are accepted for compatibility; debug payloads are not implemented yet"
        );
    }
    if no_credits {
        eprintln!(
            "ai-usage: --no-credits is accepted for compatibility; credit visibility is currently provider-plugin controlled"
        );
    }
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
        })
        .collect()
}

fn select_providers(
    providers: Vec<LoadedProvider>,
    config: &AppConfig,
    provider_ids: &[String],
    include_disabled: bool,
) -> Vec<LoadedProvider> {
    providers
        .into_iter()
        .filter(|p| {
            provider_ids.is_empty()
                || provider_ids
                    .iter()
                    .map(|id| normalize_provider_id(id))
                    .any(|id| id == p.manifest.id.as_str())
        })
        .filter(|p| {
            include_disabled || config.is_enabled(&p.manifest.id, p.manifest.enabled_by_default)
        })
        .collect()
}
