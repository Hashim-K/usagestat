use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use chrono::{Datelike, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;

use crate::paths;

const MAX_STORED_ROWS: usize = 25_000;

#[derive(Debug, Error)]
pub enum UsageDailyError {
    #[error("parse daily usage payload: {0}")]
    ParsePayload(#[from] serde_json::Error),
    #[error("read daily usage store {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("write daily usage store {path}: {source}")]
    Write {
        path: String,
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDailyRow {
    pub provider_id: String,
    pub display_name: String,
    pub date: String,
    pub source: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub total_tokens: u64,
    pub cost_usd: f64,
    pub ingested_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredUsageDaily {
    version: u32,
    rows: Vec<UsageDailyRow>,
}

impl Default for StoredUsageDaily {
    fn default() -> Self {
        Self {
            version: 1,
            rows: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct DailyAccumulator {
    date: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
    reasoning_output_tokens: u64,
    total_tokens: u64,
    cost_usd: f64,
}

pub fn ingest_json(provider_id: &str, payload_json: &str) -> Result<usize, UsageDailyError> {
    let payload: JsonValue = serde_json::from_str(payload_json)?;
    let Some(daily) = payload.get("daily").and_then(JsonValue::as_array) else {
        return Ok(0);
    };
    if daily.is_empty() {
        return Ok(0);
    }

    let provider_id = provider_id.trim();
    if provider_id.is_empty() {
        return Ok(0);
    }

    let display_name = string_field(&payload, &["displayName", "display_name"])
        .unwrap_or_else(|| provider_id.to_string());
    let source = string_field(&payload, &["source"]).unwrap_or_else(|| "ccusage".to_string());
    let ingested_at = Utc::now().to_rfc3339();

    let mut new_rows = Vec::new();
    for entry in daily {
        let Some(date) =
            string_field(entry, &["date", "day"]).and_then(|date| normalize_day_key(&date))
        else {
            continue;
        };
        let input_tokens = u64_field(entry, &["inputTokens", "input_tokens"]);
        let output_tokens = u64_field(entry, &["outputTokens", "output_tokens"]);
        let cache_read_tokens = u64_field(
            entry,
            &[
                "cacheReadTokens",
                "cache_read_tokens",
                "cacheReadInputTokens",
                "cache_read_input_tokens",
                "cachedInputTokens",
                "cached_input_tokens",
            ],
        );
        let cache_creation_tokens = u64_field(
            entry,
            &[
                "cacheCreationTokens",
                "cache_creation_tokens",
                "cacheCreationInputTokens",
                "cache_creation_input_tokens",
                "cacheCreateTokens",
                "cache_create_tokens",
            ],
        );
        let reasoning_output_tokens = u64_field(
            entry,
            &[
                "reasoningOutputTokens",
                "reasoning_output_tokens",
                "reasoningTokens",
                "reasoning_tokens",
            ],
        );
        let total_tokens = u64_field(entry, &["totalTokens", "total_tokens", "tokens"]);
        let computed_total = input_tokens
            + output_tokens
            + cache_read_tokens
            + cache_creation_tokens
            + reasoning_output_tokens;
        let total_tokens = total_tokens.max(computed_total);
        let cost_usd = f64_field(
            entry,
            &[
                "costUsd",
                "costUSD",
                "cost_usd",
                "totalCost",
                "total_cost",
                "cost",
            ],
        );

        new_rows.push(UsageDailyRow {
            provider_id: provider_id.to_string(),
            display_name: display_name.clone(),
            date,
            source: source.clone(),
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_creation_tokens,
            reasoning_output_tokens,
            total_tokens,
            cost_usd,
            ingested_at: ingested_at.clone(),
        });
    }

    if new_rows.is_empty() {
        return Ok(0);
    }

    let path = paths::usage_daily_file();
    let mut store = read_store(&path)?;
    for row in new_rows {
        upsert_row(&mut store.rows, row);
    }
    prune_rows(&mut store.rows);
    write_store(&path, &store)?;
    Ok(store.rows.len())
}

pub fn report_json(provider_id: &str, report: &str) -> Result<JsonValue, UsageDailyError> {
    let rows = selected_daily_rows(provider_id)?;
    if rows.is_empty() {
        return Ok(serde_json::json!({
            "error": {
                "code": "UNAVAILABLE",
                "message": "Saved daily usage is not available for this provider"
            }
        }));
    }

    match report {
        "daily" => Ok(serde_json::json!({ "daily": rows })),
        "weekly" => Ok(serde_json::json!({ "weekly": aggregate_rows(rows, Bucket::Week) })),
        "monthly" => Ok(serde_json::json!({ "monthly": aggregate_rows(rows, Bucket::Month) })),
        _ => Ok(serde_json::json!({
            "error": {
                "code": "BAD_REPORT",
                "message": "Saved daily usage only supports daily, weekly, and monthly reports"
            }
        })),
    }
}

pub fn selected_daily_rows(provider_id: &str) -> Result<Vec<UsageDailyRow>, UsageDailyError> {
    let provider_id = provider_id.trim();
    if provider_id.is_empty() {
        return Ok(Vec::new());
    }

    let mut by_day: BTreeMap<String, UsageDailyRow> = BTreeMap::new();
    for row in read_store(&paths::usage_daily_file())?.rows {
        if !row.provider_id.eq_ignore_ascii_case(provider_id) {
            continue;
        }
        match by_day.get(&row.date) {
            Some(existing) if !prefer_row(&row, existing) => {}
            _ => {
                by_day.insert(row.date.clone(), row);
            }
        }
    }
    Ok(by_day.into_values().collect())
}

pub fn all_selected_daily_rows() -> Result<Vec<UsageDailyRow>, UsageDailyError> {
    let mut by_provider_day: BTreeMap<(String, String), UsageDailyRow> = BTreeMap::new();
    for row in read_store(&paths::usage_daily_file())?.rows {
        let key = (row.provider_id.to_ascii_lowercase(), row.date.clone());
        match by_provider_day.get(&key) {
            Some(existing) if !prefer_row(&row, existing) => {}
            _ => {
                by_provider_day.insert(key, row);
            }
        }
    }
    Ok(by_provider_day.into_values().collect())
}

fn read_store(path: &Path) -> Result<StoredUsageDaily, UsageDailyError> {
    if !path.exists() {
        return Ok(StoredUsageDaily::default());
    }
    let text = std::fs::read_to_string(path).map_err(|source| UsageDailyError::Read {
        path: path.display().to_string(),
        source,
    })?;
    if text.trim().is_empty() {
        return Ok(StoredUsageDaily::default());
    }
    if let Ok(store) = serde_json::from_str::<StoredUsageDaily>(&text) {
        return Ok(store);
    }
    let rows = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<UsageDailyRow>(line).ok())
        .collect();
    Ok(StoredUsageDaily { version: 1, rows })
}

fn write_store(path: &Path, store: &StoredUsageDaily) -> Result<(), UsageDailyError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| UsageDailyError::Write {
            path: parent.display().to_string(),
            source,
        })?;
    }
    let temp_path = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(store).map_err(UsageDailyError::ParsePayload)?;
    {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&temp_path)
            .map_err(|source| UsageDailyError::Write {
                path: temp_path.display().to_string(),
                source,
            })?;
        file.write_all(&json)
            .map_err(|source| UsageDailyError::Write {
                path: temp_path.display().to_string(),
                source,
            })?;
        file.write_all(b"\n")
            .map_err(|source| UsageDailyError::Write {
                path: temp_path.display().to_string(),
                source,
            })?;
    }
    std::fs::rename(&temp_path, path).map_err(|source| UsageDailyError::Write {
        path: path.display().to_string(),
        source,
    })?;
    Ok(())
}

fn upsert_row(rows: &mut Vec<UsageDailyRow>, row: UsageDailyRow) {
    if let Some(existing) = rows.iter_mut().find(|existing| {
        existing.provider_id.eq_ignore_ascii_case(&row.provider_id)
            && existing.date == row.date
            && existing.source == row.source
    }) {
        *existing = row;
    } else {
        rows.push(row);
    }
}

fn prune_rows(rows: &mut Vec<UsageDailyRow>) {
    rows.sort_by(|a, b| {
        a.provider_id
            .cmp(&b.provider_id)
            .then(a.date.cmp(&b.date))
            .then(a.source.cmp(&b.source))
    });
    if rows.len() > MAX_STORED_ROWS {
        let remove = rows.len() - MAX_STORED_ROWS;
        rows.drain(0..remove);
    }
}

fn prefer_row(candidate: &UsageDailyRow, existing: &UsageDailyRow) -> bool {
    let candidate_priority = source_priority(&candidate.source);
    let existing_priority = source_priority(&existing.source);
    candidate_priority > existing_priority
        || (candidate_priority == existing_priority && candidate.ingested_at > existing.ingested_at)
}

fn source_priority(source: &str) -> u8 {
    let source = source.to_ascii_lowercase();
    if source.contains("billing") {
        40
    } else if source.contains("ccusage") {
        30
    } else if source.contains("transcript") {
        20
    } else {
        10
    }
}

#[derive(Clone, Copy)]
enum Bucket {
    Week,
    Month,
}

fn aggregate_rows(rows: Vec<UsageDailyRow>, bucket: Bucket) -> Vec<UsageDailyRow> {
    let mut map: BTreeMap<String, DailyAccumulator> = BTreeMap::new();
    for row in rows {
        let key = match bucket {
            Bucket::Week => week_key(&row.date).unwrap_or_else(|| row.date.clone()),
            Bucket::Month => row.date.get(0..7).unwrap_or(&row.date).to_string(),
        };
        let entry = map.entry(key.clone()).or_insert_with(|| DailyAccumulator {
            date: key,
            ..DailyAccumulator::default()
        });
        entry.input_tokens += row.input_tokens;
        entry.output_tokens += row.output_tokens;
        entry.cache_read_tokens += row.cache_read_tokens;
        entry.cache_creation_tokens += row.cache_creation_tokens;
        entry.reasoning_output_tokens += row.reasoning_output_tokens;
        entry.total_tokens += row.total_tokens;
        entry.cost_usd += row.cost_usd;
    }

    map.into_values()
        .map(|row| UsageDailyRow {
            provider_id: String::new(),
            display_name: String::new(),
            date: row.date,
            source: "saved_daily".to_string(),
            input_tokens: row.input_tokens,
            output_tokens: row.output_tokens,
            cache_read_tokens: row.cache_read_tokens,
            cache_creation_tokens: row.cache_creation_tokens,
            reasoning_output_tokens: row.reasoning_output_tokens,
            total_tokens: row.total_tokens,
            cost_usd: row.cost_usd,
            ingested_at: String::new(),
        })
        .collect()
}

fn week_key(date: &str) -> Option<String> {
    let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let week_start = date - Duration::days(i64::from(date.weekday().num_days_from_monday()));
    Some(week_start.format("%Y-%m-%d").to_string())
}

fn normalize_day_key(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.len() >= 10
        && trimmed.as_bytes().get(4) == Some(&b'-')
        && trimmed.as_bytes().get(7) == Some(&b'-')
    {
        return Some(trimmed[..10].to_string());
    }
    let digits: String = trimmed.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 8 {
        return Some(format!(
            "{}-{}-{}",
            &digits[0..4],
            &digits[4..6],
            &digits[6..8]
        ));
    }
    None
}

fn string_field(value: &JsonValue, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn u64_field(value: &JsonValue, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(json_to_u64)
        .unwrap_or(0)
}

fn f64_field(value: &JsonValue, keys: &[&str]) -> f64 {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(json_to_f64)
        .unwrap_or(0.0)
}

fn json_to_u64(value: &JsonValue) -> Option<u64> {
    if let Some(value) = value.as_u64() {
        return Some(value);
    }
    if let Some(value) = value.as_i64() {
        return (value >= 0).then_some(value as u64);
    }
    value
        .as_str()
        .and_then(|value| value.trim().replace(',', "").parse::<u64>().ok())
}

fn json_to_f64(value: &JsonValue) -> Option<f64> {
    let parsed = value.as_f64().or_else(|| {
        value
            .as_str()
            .and_then(|value| value.trim().replace(['$', ','], "").parse::<f64>().ok())
    })?;
    parsed.is_finite().then_some(parsed)
}
