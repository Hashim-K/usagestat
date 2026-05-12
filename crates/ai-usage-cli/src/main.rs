use ai_usage_core::paths;
use ai_usage_plugins::{discover_providers, probe_provider};
use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "ai-usage")]
#[command(about = "AI usage backend CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(long, global = true)]
    json: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    List,
    Probe { provider_ids: Vec<String> },
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();
    let providers = discover_providers(&paths::plugin_dirs());

    match cli.command.unwrap_or(Command::List) {
        Command::List => {
            if cli.json {
                let manifests: Vec<_> = providers
                    .into_iter()
                    .map(|provider| provider.manifest)
                    .collect();
                println!("{}", serde_json::to_string_pretty(&manifests)?);
            } else {
                for provider in providers {
                    println!("{}\t{}", provider.manifest.id, provider.manifest.name);
                }
            }
        }
        Command::Probe { provider_ids } => {
            let selected: Vec<_> = if provider_ids.is_empty() {
                providers
            } else {
                providers
                    .into_iter()
                    .filter(|provider| provider_ids.iter().any(|id| id == &provider.manifest.id))
                    .collect()
            };
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
    }

    Ok(())
}
