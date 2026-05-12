mod batch_probe;
mod history;

use ai_usage_core::{AppConfig, LoadedProvider, MetricLine, ProgressFormat, ProviderSummary, UsageSnapshot, paths};
use ai_usage_plugins::discover_providers;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use tabled::{Table, Tabled, settings::Style};

#[derive(Debug, Parser)]
#[command(name = "ai-usage")]
#[command(about = "AI usage backend CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(long, global = true)]
    json: bool,

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
    /// List discovered providers and their enabled status
    List,
    /// Probe one or more providers and show live usage
    Probe {
        provider_ids: Vec<String>,
        /// Append results to ~/.local/share/ai-usage/history.jsonl
        #[arg(long)]
        save: bool,
    },
    /// Export usage as JSON or CSV (live probe, or read prior JSONL history)
    Export {
        #[arg(long, value_enum, default_value_t = ExportFormat::Json)]
        format: ExportFormat,
        /// Read from a JSONL history file instead of probing live
        #[arg(long)]
        from_file: Option<PathBuf>,
        provider_ids: Vec<String>,
    },
    Plugin {
        #[command(subcommand)]
        command: PluginCommand,
    },
}

#[derive(Debug, Subcommand)]
enum PluginCommand {
    Validate,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum ExportFormat {
    #[default]
    Json,
    Csv,
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();
    let config_path = cli.config.clone().unwrap_or_else(paths::config_file);
    let config = AppConfig::load_optional(&config_path)
        .with_context(|| format!("load config {}", config_path.display()))?;
    let plugin_dirs = paths::plugin_dirs(&config, &cli.plugin_dirs);
    let providers = discover_providers(&plugin_dirs);

    match cli.command.unwrap_or(Command::List) {
        Command::List => run_list(&providers, &config, cli.json, cli.plain),
        Command::Probe { provider_ids, save } => {
            run_probe(&providers, &config, &provider_ids, cli.all, cli.json, cli.plain, save)
        }
        Command::Export {
            format,
            from_file,
            provider_ids,
        } => run_export(&providers, &config, &provider_ids, cli.all, format, from_file),
        Command::Plugin {
            command: PluginCommand::Validate,
        } => run_validate(&providers, &config, cli.json),
    }
}

// ── list ─────────────────────────────────────────────────────────────────────

#[derive(Tabled)]
struct ListRow {
    id: String,
    name: String,
    status: String,
}

fn run_list(
    providers: &[LoadedProvider],
    config: &AppConfig,
    json: bool,
    plain: bool,
) -> Result<()> {
    let mut summaries = provider_summaries(providers, config);
    summaries.sort_by(|a, b| a.id.cmp(&b.id));

    if json {
        println!("{}", serde_json::to_string_pretty(&summaries)?);
        return Ok(());
    }

    if plain {
        for s in &summaries {
            println!("{}\t{}\t{}", s.id, s.name, if s.enabled { "enabled" } else { "disabled" });
        }
        return Ok(());
    }

    let rows: Vec<ListRow> = summaries
        .iter()
        .map(|s| ListRow {
            id: s.id.clone(),
            name: s.name.clone(),
            status: if s.enabled { "enabled".into() } else { "disabled".into() },
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
    for (i, provider) in selected.iter().enumerate() {
        if !json {
            eprintln!("ai-usage:   [{}/{}] {}…", i + 1, n, provider.manifest.id);
        }
        let snap = batch_probe::run_probe_with_timeout(provider, Some(&interrupt));
        if save {
            let rec = history::record_from_snapshot(&snap);
            if let Err(e) = history::append_jsonl(&rec) {
                eprintln!("ai-usage: warning: failed to save history: {e}");
            }
        }
        snapshots.push(snap);
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&snapshots)?);
        return Ok(());
    }

    for snap in &snapshots {
        print_snapshot(snap, plain);
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
        MetricLine::Text { label, value, subtitle, .. } => {
            let mut v = value.clone();
            if let Some(s) = subtitle {
                v.push_str(&format!(" ({s})"));
            }
            (label.clone(), v)
        }
        MetricLine::Badge { label, text, subtitle, .. } => {
            let mut v = text.clone();
            if let Some(s) = subtitle {
                v.push_str(&format!(" ({s})"));
            }
            (label.clone(), v)
        }
        MetricLine::Progress { label, used, limit, format, resets_at, .. } => {
            let pct = if *limit > 0.0 { used / limit * 100.0 } else { 0.0 };
            let mut v = match format {
                ProgressFormat::Percent => format!("{pct:.1}% ({used:.0} / {limit:.0})"),
                ProgressFormat::Dollars => format!("${used:.2} / ${limit:.2}"),
                ProgressFormat::Count { suffix } => format!("{used:.0} / {limit:.0} {suffix}"),
            };
            if let Some(dt) = resets_at {
                v.push_str(&format!("  resets {}", dt.format("%Y-%m-%d %H:%M UTC")));
            }
            (label.clone(), v)
        }
    }
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
        eprintln!(
            "ai-usage: probing {n} provider(s) for export… (up to {tmax}s each)"
        );

        let mut recs = Vec::new();
        for (i, provider) in selected.iter().enumerate() {
            eprintln!("ai-usage:   [{}/{}] {}…", i + 1, n, provider.manifest.id);
            let snap = batch_probe::run_probe_with_timeout(provider, Some(&interrupt));
            recs.push(history::record_from_snapshot(&snap));
        }
        recs
    };

    // Filter by provider_ids when reading from file
    if !provider_ids.is_empty() && from_file.is_some() {
        let ids: std::collections::HashSet<&str> =
            provider_ids.iter().map(String::as_str).collect();
        records.retain(|r| ids.contains(r.provider_id.as_str()));
    }

    match format {
        ExportFormat::Json => println!("{}", serde_json::to_string_pretty(&records)?),
        ExportFormat::Csv => history::print_csv(&records)?,
    }
    Ok(())
}

// ── plugin validate ───────────────────────────────────────────────────────────

fn run_validate(
    providers: &[LoadedProvider],
    config: &AppConfig,
    json: bool,
) -> Result<()> {
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

// ── helpers ───────────────────────────────────────────────────────────────────

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

fn select_providers(
    providers: Vec<LoadedProvider>,
    config: &AppConfig,
    provider_ids: &[String],
    include_disabled: bool,
) -> Vec<LoadedProvider> {
    providers
        .into_iter()
        .filter(|p| {
            provider_ids.is_empty() || provider_ids.iter().any(|id| id == &p.manifest.id)
        })
        .filter(|p| {
            include_disabled
                || config.is_enabled(&p.manifest.id, p.manifest.enabled_by_default)
        })
        .collect()
}
