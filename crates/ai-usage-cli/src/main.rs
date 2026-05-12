use ai_usage_core::{AppConfig, LoadedProvider, ProviderSummary, paths};
use ai_usage_plugins::{discover_providers, probe_provider};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "ai-usage")]
#[command(about = "AI usage backend CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(long, global = true)]
    json: bool,

    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,

    #[arg(long = "plugin-dir", global = true, value_name = "DIR")]
    plugin_dirs: Vec<PathBuf>,

    #[arg(long, global = true, help = "Include disabled providers")]
    all: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    List,
    Probe {
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

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();
    let config_path = cli.config.clone().unwrap_or_else(paths::config_file);
    let config = AppConfig::load_optional(&config_path)
        .with_context(|| format!("load config {}", config_path.display()))?;
    let plugin_dirs = paths::plugin_dirs(&config, &cli.plugin_dirs);
    let providers = discover_providers(&plugin_dirs);

    match cli.command.unwrap_or(Command::List) {
        Command::List => {
            let summaries = provider_summaries(&providers, &config);
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&summaries)?);
            } else {
                for provider in summaries {
                    let status = if provider.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    };
                    println!("{}\t{}\t{}", provider.id, provider.name, status);
                }
            }
        }
        Command::Probe { provider_ids } => {
            let selected = select_providers(providers, &config, &provider_ids, cli.all);
            let snapshots: Vec<_> = selected.iter().map(probe_provider).collect();
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&snapshots)?);
            } else {
                for snapshot in snapshots {
                    println!("{} ({})", snapshot.display_name, snapshot.provider_id);
                    for metric in snapshot.metrics {
                        println!("  {metric:?}");
                    }
                }
            }
        }
        Command::Plugin {
            command: PluginCommand::Validate,
        } => {
            let summaries = provider_summaries(&providers, &config);
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&summaries)?);
            } else {
                println!("validated {} plugin(s)", summaries.len());
                for summary in summaries {
                    println!("{}\t{}", summary.id, summary.name);
                }
            }
        }
    }

    Ok(())
}

fn provider_summaries(providers: &[LoadedProvider], config: &AppConfig) -> Vec<ProviderSummary> {
    providers
        .iter()
        .map(|provider| ProviderSummary {
            id: provider.manifest.id.clone(),
            name: provider.manifest.name.clone(),
            enabled: config.is_enabled(&provider.manifest.id, provider.manifest.enabled_by_default),
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
        .filter(|provider| {
            provider_ids.is_empty() || provider_ids.iter().any(|id| id == &provider.manifest.id)
        })
        .filter(|provider| {
            include_disabled
                || config.is_enabled(&provider.manifest.id, provider.manifest.enabled_by_default)
        })
        .collect()
}
