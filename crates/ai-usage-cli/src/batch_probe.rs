use ai_usage_core::{LoadedProvider, UsageSnapshot};
use ai_usage_plugins::probe_provider;
use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub fn probe_timeout_secs() -> u64 {
    std::env::var("AI_USAGE_PROBE_TIMEOUT_SEC")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&s: &u64| s > 0)
        .unwrap_or(120)
}

/// Register SIGINT (and SIGTERM on Unix) to set the returned flag.
pub fn register_interrupt_flag() -> Result<Arc<AtomicBool>> {
    use signal_hook::consts::signal::SIGINT;
    use signal_hook::flag as signal_flag;

    let flag = Arc::new(AtomicBool::new(false));
    signal_flag::register(SIGINT, Arc::clone(&flag)).context("register SIGINT")?;
    #[cfg(unix)]
    {
        use signal_hook::consts::signal::SIGTERM;
        signal_flag::register(SIGTERM, Arc::clone(&flag)).context("register SIGTERM")?;
    }
    Ok(flag)
}

/// Run a probe with a wall-clock timeout. Checks the interrupt flag every 200ms.
///
/// On timeout the probe thread is left running in the background; it will exit when the
/// process exits after the batch command finishes.
pub fn run_probe_with_timeout(
    provider: &LoadedProvider,
    interrupt: Option<&Arc<AtomicBool>>,
) -> UsageSnapshot {
    let provider_id = provider.manifest.id.clone();
    let timeout_sec = probe_timeout_secs();
    let deadline = Instant::now() + Duration::from_secs(timeout_sec);

    let provider_thread = provider.clone();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(probe_provider(&provider_thread));
    });

    const TICK: Duration = Duration::from_millis(200);

    loop {
        if let Some(flag) = interrupt {
            if flag.load(Ordering::SeqCst) {
                eprintln!("\nai-usage: interrupted");
                std::process::exit(130);
            }
        }

        let now = Instant::now();
        if now >= deadline {
            eprintln!(
                "ai-usage: probe timed out after {timeout_sec}s for `{provider_id}` \
                 (set AI_USAGE_PROBE_TIMEOUT_SEC to override)"
            );
            return UsageSnapshot::error(
                &provider_id,
                &provider.manifest.name,
                format!("Probe timed out after {timeout_sec}s."),
            );
        }

        let remaining = deadline.saturating_duration_since(now);
        match rx.recv_timeout(TICK.min(remaining)) {
            Ok(snapshot) => return snapshot,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                return UsageSnapshot::error(
                    &provider_id,
                    &provider.manifest.name,
                    "Probe thread ended without a result (panic?).".to_string(),
                );
            }
        }
    }
}
