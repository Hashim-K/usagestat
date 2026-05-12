use crate::host_api;
use ai_usage_core::{LoadedProvider, MetricLine, ProgressFormat, UsageSnapshot, paths};
use chrono::{DateTime, Utc};
use rquickjs::{Array, Context, Ctx, Object, Runtime, Value};

pub fn probe_provider(provider: &LoadedProvider) -> UsageSnapshot {
    let fallback = || {
        UsageSnapshot::error(
            provider.manifest.id.clone(),
            provider.manifest.name.clone(),
            "plugin runtime error",
        )
    };

    let Ok(rt) = Runtime::new() else {
        return fallback();
    };
    let Ok(ctx) = Context::full(&rt) else {
        return fallback();
    };

    ctx.with(|ctx| {
        run_in_context(ctx, provider).unwrap_or_else(|message| {
            UsageSnapshot::error(
                provider.manifest.id.clone(),
                provider.manifest.name.clone(),
                message,
            )
        })
    })
}

fn run_in_context(ctx: Ctx<'_>, provider: &LoadedProvider) -> Result<UsageSnapshot, String> {
    inject_context(&ctx, &provider.manifest.id)
        .map_err(|_| "host api injection failed".to_string())?;

    ctx.eval::<(), _>(provider.entry_script.as_bytes())
        .map_err(|_| "script eval failed".to_string())?;

    let globals = ctx.globals();
    let plugin_obj: Object = globals
        .get("__ai_usage_plugin")
        .or_else(|_| globals.get("__openusage_plugin"))
        .map_err(|_| "missing plugin export".to_string())?;
    let probe_fn: rquickjs::Function = plugin_obj
        .get("probe")
        .map_err(|_| "missing probe()".to_string())?;
    let probe_ctx: Value = globals
        .get("__ai_usage_ctx")
        .unwrap_or_else(|_| Value::new_undefined(ctx.clone()));
    let result: Object = probe_fn
        .call((probe_ctx,))
        .map_err(|_| extract_error_string(&ctx))?;

    let display_name = result
        .get::<_, String>("displayName")
        .unwrap_or_else(|_| provider.manifest.name.clone());
    let source = result.get::<_, String>("source").ok();
    let plan = result.get::<_, String>("plan").ok();
    let metrics = parse_metrics(&result)?;

    Ok(UsageSnapshot {
        provider_id: provider.manifest.id.clone(),
        display_name,
        source,
        plan,
        metrics,
        fetched_at: Utc::now(),
    })
}

fn extract_error_string(ctx: &Ctx<'_>) -> String {
    let exc = ctx.catch();
    if exc.is_null() || exc.is_undefined() {
        return "The plugin failed.".to_string();
    }
    if let Some(value) = exc.as_string() {
        let message = value.to_string().unwrap_or_default();
        let trimmed = message.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    "The plugin failed.".to_string()
}

fn inject_context(ctx: &Ctx<'_>, plugin_id: &str) -> rquickjs::Result<()> {
    let globals = ctx.globals();
    let app = Object::new(ctx.clone())?;
    app.set("version", env!("CARGO_PKG_VERSION"))?;
    app.set("platform", std::env::consts::OS)?;
    let app_data_dir = paths::data_dir();
    let plugin_data_dir = app_data_dir.join("plugins").join(plugin_id);
    let _ = std::fs::create_dir_all(&plugin_data_dir);
    app.set("appDataDir", app_data_dir.to_string_lossy().to_string())?;
    app.set(
        "pluginDataDir",
        plugin_data_dir.to_string_lossy().to_string(),
    )?;

    let probe_ctx = Object::new(ctx.clone())?;
    probe_ctx.set("nowIso", Utc::now().to_rfc3339())?;
    probe_ctx.set("app", app)?;
    globals.set("__ai_usage_ctx", probe_ctx.clone())?;
    host_api::inject(ctx, &probe_ctx, plugin_id)?;
    Ok(())
}

fn parse_metrics(result: &Object<'_>) -> Result<Vec<MetricLine>, String> {
    let lines: Array = result
        .get("metrics")
        .or_else(|_| result.get("lines"))
        .map_err(|_| "missing metrics".to_string())?;
    let mut out = Vec::new();

    for idx in 0..lines.len() {
        let line: Object = lines
            .get(idx)
            .map_err(|_| format!("invalid metric at index {idx}"))?;
        let line_type: String = line.get("type").unwrap_or_default();
        let label: String = line.get("label").unwrap_or_default();
        let color = line.get::<_, String>("color").ok();
        let subtitle = line.get::<_, String>("subtitle").ok();

        match line_type.as_str() {
            "text" => out.push(MetricLine::Text {
                label,
                value: line.get::<_, String>("value").unwrap_or_default(),
                color,
                subtitle,
            }),
            "badge" => out.push(MetricLine::Badge {
                label,
                text: line.get::<_, String>("text").unwrap_or_default(),
                color,
                subtitle,
            }),
            "progress" => out.push(MetricLine::Progress {
                label,
                used: line.get::<_, f64>("used").unwrap_or(0.0),
                limit: line.get::<_, f64>("limit").unwrap_or(100.0),
                format: parse_progress_format(&line),
                resets_at: parse_optional_datetime(line.get::<_, String>("resetsAt").ok()),
                period_duration_ms: line.get::<_, u64>("periodDurationMs").ok(),
                color,
            }),
            _ => return Err(format!("unknown metric type at index {idx}: {line_type}")),
        }
    }

    if out.is_empty() {
        return Err("plugin returned no metrics".to_string());
    }

    Ok(out)
}

fn parse_progress_format(line: &Object<'_>) -> ProgressFormat {
    let Ok(format) = line.get::<_, Object>("format") else {
        return ProgressFormat::Percent;
    };
    let kind: String = format.get("kind").unwrap_or_else(|_| "percent".to_string());
    match kind.as_str() {
        "dollars" => ProgressFormat::Dollars,
        "count" => ProgressFormat::Count {
            suffix: format.get::<_, String>("suffix").unwrap_or_default(),
        },
        _ => ProgressFormat::Percent,
    }
}

fn parse_optional_datetime(value: Option<String>) -> Option<DateTime<Utc>> {
    value
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
}
