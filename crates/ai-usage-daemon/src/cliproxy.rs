//! Read-only subset of CLIProxyAPI consumed by T3 Code's usage limit sources.
//! See docs/t3-code.md for the upstream contract and its display limitations.
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;
use std::collections::BTreeMap;
use usagestat_core::{MetricLine, ProgressFormat, UsageSnapshot};

use crate::{AppState, http_request::Request, response_json, response_no_content};

const STATUS_PATH: &str = "/v0/management/quota-scheduler/status";

#[derive(Default)]
pub struct ManagementApi {
    key: Option<String>,
}

impl ManagementApi {
    pub fn load(key_file: Option<&Path>) -> Result<Self> {
        let key = match key_file {
            Some(path) => Some(std::fs::read_to_string(path).context("read management key file")?),
            None => match std::env::var("USAGESTAT_MANAGEMENT_KEY") {
                Ok(value) => Some(value),
                Err(std::env::VarError::NotPresent) => None,
                Err(_) => bail!("USAGESTAT_MANAGEMENT_KEY must contain valid text"),
            },
        };
        Self::from_key(key)
    }

    fn from_key(key: Option<String>) -> Result<Self> {
        let key = key.map(|value| value.trim().to_string());
        if let Some(key) = &key {
            if key.is_empty() || !key.bytes().all(|b| b.is_ascii_graphic()) {
                bail!("management key must be nonempty ASCII text without whitespace");
            }
        }
        Ok(Self { key })
    }

    pub fn route(&self, request: &Request, state: &Arc<Mutex<AppState>>) -> Option<String> {
        if request.path != STATUS_PATH {
            return None;
        }
        let Some(key) = &self.key else {
            return Some(response_json(
                404,
                "Not Found",
                r#"{"error":"management_api_disabled"}"#,
            ));
        };
        if request.method == "OPTIONS" {
            return Some(response_no_content());
        }
        let supplied = if request
            .headers
            .iter()
            .any(|(name, _)| name == "authorization")
        {
            request.header("authorization").and_then(|value| {
                let (scheme, key) = value.split_once(' ')?;
                scheme.eq_ignore_ascii_case("bearer").then_some(key)
            })
        } else {
            request.header("x-management-key")
        };
        if !supplied.is_some_and(|supplied| keys_equal(key, supplied)) {
            return Some(response_json(
                401,
                "Unauthorized",
                r#"{"error":"invalid_management_key"}"#,
            ));
        }
        if request.method != "GET" {
            return Some(response_json(
                405,
                "Method Not Allowed",
                r#"{"error":"method_not_allowed"}"#,
            ));
        }
        let status = quota_status(&state.lock().expect("app state poisoned"));
        Some(response_json(
            200,
            "OK",
            &serde_json::to_string(&status).expect("serializable quota status"),
        ))
    }
}

fn keys_equal(expected: &str, supplied: &str) -> bool {
    // Verify fixed-size MACs in constant time instead of comparing key prefixes.
    fn mac(key: &str) -> Hmac<Sha256> {
        let mut mac = Hmac::<Sha256>::new_from_slice(b"usagestat management authentication")
            .expect("HMAC accepts any key size");
        mac.update(key.as_bytes());
        mac
    }
    mac(expected)
        .verify_slice(&mac(supplied).finalize().into_bytes())
        .is_ok()
}

#[derive(Serialize)]
struct QuotaStatus {
    accounts: BTreeMap<String, QuotaAccount>,
}

#[derive(Serialize)]
struct QuotaAccount {
    provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan: Option<String>,
    fetched_at: String,
    #[serde(flatten)]
    windows: BTreeMap<&'static str, QuotaWindow>,
}

#[derive(Serialize)]
struct QuotaWindow {
    used_percent: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    reset_at: Option<String>,
    known: bool,
    hard_limited: bool,
}

fn quota_status(state: &AppState) -> QuotaStatus {
    let mut accounts = BTreeMap::new();
    // Require an enabled, discovered provider, including during startup. Cached
    // snapshots for disabled or removed providers must not reappear in T3.
    for provider in &state.providers {
        if !provider.enabled || !matches!(provider.id.as_str(), "claude" | "codex") {
            continue;
        }
        if let Some(snapshot) = state.cache.get(&provider.id) {
            // These are stable identifiers, not real credential files. The
            // daemon currently keeps one snapshot per provider, not per login.
            accounts.insert(format!("{}.json", provider.id), quota_account(snapshot));
        }
    }
    QuotaStatus { accounts }
}

fn quota_account(snapshot: &UsageSnapshot) -> QuotaAccount {
    let mut account = QuotaAccount {
        provider: snapshot.provider_id.clone(),
        plan: snapshot
            .plan
            .as_ref()
            .map(|plan| match plan.trim().to_ascii_lowercase().as_str() {
                "pro 5x" => "prolite".to_string(),
                "pro 20x" => "pro".to_string(),
                plan => plan.replace([' ', '-'], "_"),
            }),
        fetched_at: snapshot.fetched_at.to_rfc3339(),
        windows: BTreeMap::new(),
    };
    if snapshot.source.as_deref() == Some("error") {
        return account;
    }
    for metric in &snapshot.metrics {
        let MetricLine::Progress {
            label,
            used,
            limit,
            format: ProgressFormat::Percent,
            resets_at,
            period_duration_ms,
            ..
        } = metric
        else {
            continue;
        };
        // Match the provider plugin's explicit labels, never other model quotas,
        // token counts or dollar budgets that happen to have the same duration.
        let (key, duration_ms) = match (snapshot.provider_id.as_str(), label.as_str()) {
            ("claude", "Session") | ("codex", "Session") => ("five_hour", 5 * 60 * 60 * 1000),
            ("claude", "Weekly") => ("seven_day", 7 * 24 * 60 * 60 * 1000),
            ("codex", "Weekly") => ("weekly", 7 * 24 * 60 * 60 * 1000),
            ("claude", "Fable") => ("fable", 7 * 24 * 60 * 60 * 1000),
            _ => continue,
        };
        if !used.is_finite()
            || !limit.is_finite()
            || *limit <= 0.0
            || period_duration_ms.is_some_and(|duration| duration != duration_ms)
        {
            continue;
        }
        let percent = (used / limit).clamp(0.0, 1.0) * 100.0;
        account.windows.insert(
            key,
            QuotaWindow {
                used_percent: percent,
                reset_at: resets_at.map(|reset| reset.to_rfc3339()),
                known: true,
                hard_limited: percent >= 100.0,
            },
        );
    }
    account
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_request::read_request;
    use serde_json::{Value, json};
    use usagestat_core::ProviderSummary;

    fn progress(label: &str, used: f64, limit: f64) -> MetricLine {
        MetricLine::Progress {
            label: label.to_string(),
            used,
            limit,
            format: ProgressFormat::Percent,
            resets_at: Some("2026-09-07T07:59:59Z".parse().unwrap()),
            period_duration_ms: None,
            detail: None,
            color: None,
        }
    }

    fn snapshot(id: &str) -> UsageSnapshot {
        UsageSnapshot {
            provider_id: id.to_string(),
            display_name: id.to_string(),
            source: Some("oauth".to_string()),
            plan: Some("Plus".to_string()),
            metrics: vec![
                progress("Session", 5.0, 20.0),
                progress("Weekly", 51.0, 100.0),
            ],
            fetched_at: "2026-09-05T12:00:00Z".parse().unwrap(),
            status_page_url: None,
            pace: None,
        }
    }

    fn state() -> Arc<Mutex<AppState>> {
        let mut state = AppState::default();
        for id in ["claude", "codex", "gemini"] {
            state.providers.push(ProviderSummary {
                id: id.to_string(),
                name: id.to_string(),
                enabled: true,
                supported_modes: vec![],
                auto_mode: String::new(),
                web_url: None,
                status_page_url: None,
                usage_dashboard_url: None,
                icon: None,
            });
            state.cache.upsert(snapshot(id));
        }
        Arc::new(Mutex::new(state))
    }

    fn api() -> ManagementApi {
        ManagementApi::from_key(Some("test-key".to_string())).unwrap()
    }

    fn request(method: &str, headers: &str) -> Request {
        read_request(format!("{method} {STATUS_PATH} HTTP/1.1\r\n{headers}\r\n").as_bytes())
            .unwrap()
    }

    #[test]
    fn t3_contract_maps_both_providers_and_preserves_timestamps() {
        let response = api()
            .route(
                &request("GET", "Authorization: Bearer test-key\r\n"),
                &state(),
            )
            .unwrap();
        assert!(response.starts_with("HTTP/1.1 200"));
        let body: Value = serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(
            body,
            json!({"accounts": {
                "claude.json": {"provider": "claude", "plan": "plus", "fetched_at": "2026-09-05T12:00:00+00:00",
                    "five_hour": {"used_percent": 25.0, "known": true, "hard_limited": false, "reset_at": "2026-09-07T07:59:59+00:00"},
                    "seven_day": {"used_percent": 51.0, "known": true, "hard_limited": false, "reset_at": "2026-09-07T07:59:59+00:00"}},
                "codex.json": {"provider": "codex", "plan": "plus", "fetched_at": "2026-09-05T12:00:00+00:00",
                    "five_hour": {"used_percent": 25.0, "known": true, "hard_limited": false, "reset_at": "2026-09-07T07:59:59+00:00"},
                    "weekly": {"used_percent": 51.0, "known": true, "hard_limited": false, "reset_at": "2026-09-07T07:59:59+00:00"}}
            }})
        );
    }

    #[test]
    fn authentication_and_read_only_methods() {
        let state = state();
        for header in [
            "",
            "Authorization: Bearer wrong\r\n",
            "Authorization: Basic test-key\r\n",
            "Authorization: Bearer test-key\r\nAuthorization: Bearer wrong\r\n",
            "Authorization: Bearer wrong\r\nX-Management-Key: test-key\r\n",
            "X-Management-Key: test-key\r\nX-Management-Key: wrong\r\n",
        ] {
            let response = api().route(&request("GET", header), &state).unwrap();
            assert!(response.starts_with("HTTP/1.1 401"), "{header}");
            assert!(!response.contains("accounts"));
        }
        for header in [
            "authorization: bearer test-key\r\n",
            "X-Management-Key: test-key\r\n",
        ] {
            assert!(
                api()
                    .route(&request("GET", header), &state)
                    .unwrap()
                    .starts_with("HTTP/1.1 200")
            );
        }
        assert!(
            api()
                .route(
                    &request("POST", "Authorization: Bearer test-key\r\n"),
                    &state
                )
                .unwrap()
                .starts_with("HTTP/1.1 405")
        );
        assert!(
            api()
                .route(&request("OPTIONS", ""), &state)
                .unwrap()
                .starts_with("HTTP/1.1 204")
        );
        assert!(
            ManagementApi::default()
                .route(
                    &request("GET", "Authorization: Bearer test-key\r\n"),
                    &state
                )
                .unwrap()
                .starts_with("HTTP/1.1 404")
        );
    }

    #[test]
    fn key_configuration_rejects_empty_or_invalid_keys() {
        for key in ["", "\n", "two words", "secret\ninjected", "non-ascii-🔑"] {
            assert!(ManagementApi::from_key(Some(key.to_string())).is_err());
        }
        let from_file = ManagementApi::from_key(Some("test-key\n".to_string())).unwrap();
        assert!(
            from_file
                .route(
                    &request("GET", "Authorization: Bearer test-key\r\n"),
                    &state()
                )
                .unwrap()
                .starts_with("HTTP/1.1 200")
        );
    }

    #[test]
    fn key_comparison_requires_the_entire_exact_value() {
        assert!(keys_equal("test-key", "test-key"));
        for supplied in ["", "test", "test-key-extra", "Test-key", "test-key\0"] {
            assert!(!keys_equal("test-key", supplied));
        }
    }

    #[test]
    fn ignores_disabled_removed_and_unsupported_cached_providers() {
        let state = state();
        let mut state = state.lock().unwrap();
        state.providers.retain(|provider| provider.id != "claude");
        state
            .providers
            .iter_mut()
            .find(|provider| provider.id == "codex")
            .unwrap()
            .enabled = false;
        assert!(quota_status(&state).accounts.is_empty());
        assert!(quota_status(&AppState::default()).accounts.is_empty());
    }

    #[test]
    fn unknown_failed_and_invalid_metrics_are_not_zero_usage() {
        let mut snapshot = snapshot("claude");
        snapshot.metrics = vec![
            progress("Session", f64::NAN, 100.0),
            progress("Weekly", 10.0, 0.0),
            progress("Sonnet", 80.0, 100.0),
        ];
        assert!(quota_account(&snapshot).windows.is_empty());
        snapshot.metrics = vec![
            progress("Session", 0.0, f64::INFINITY),
            progress("Weekly", f64::INFINITY, 100.0),
        ];
        assert!(quota_account(&snapshot).windows.is_empty());
        snapshot.metrics = vec![progress("Session", 10.0, 100.0)];
        if let MetricLine::Progress {
            period_duration_ms, ..
        } = &mut snapshot.metrics[0]
        {
            *period_duration_ms = Some(60 * 60 * 1000);
        }
        assert!(quota_account(&snapshot).windows.is_empty());
        snapshot.metrics = vec![progress("Session", 10.0, 100.0)];
        if let MetricLine::Progress { format, .. } = &mut snapshot.metrics[0] {
            *format = ProgressFormat::Dollars;
        }
        assert!(quota_account(&snapshot).windows.is_empty());
        snapshot.source = Some("error".to_string());
        snapshot.metrics = vec![progress("Session", 10.0, 100.0)];
        assert!(quota_account(&snapshot).windows.is_empty());
    }

    #[test]
    fn clamps_quotas_and_leaves_missing_resets_absent() {
        let mut snapshot = snapshot("claude");
        snapshot.metrics = vec![
            progress("Session", -10.0, 100.0),
            progress("Weekly", 130.0, 100.0),
            progress("Fable", 0.0, 100.0),
        ];
        if let MetricLine::Progress { resets_at, .. } = &mut snapshot.metrics[0] {
            *resets_at = None;
        }
        let account = serde_json::to_value(quota_account(&snapshot)).unwrap();
        assert_eq!(
            account["five_hour"],
            json!({"used_percent": 0.0, "known": true, "hard_limited": false})
        );
        assert_eq!(account["seven_day"]["used_percent"], 100.0);
        assert_eq!(account["seven_day"]["hard_limited"], true);
        assert_eq!(account["fable"]["used_percent"], 0.0);
        assert!(account.get("weekly").is_none());
    }

    #[test]
    fn converts_codex_display_plans_back_to_t3_slugs() {
        let mut snapshot = snapshot("codex");
        for (display, slug) in [
            ("Plus", "plus"),
            ("Pro 5x", "prolite"),
            ("Pro 20x", "pro"),
            ("Team", "team"),
        ] {
            snapshot.plan = Some(display.to_string());
            assert_eq!(quota_account(&snapshot).plan.as_deref(), Some(slug));
        }
    }

    #[test]
    fn serves_management_and_existing_routes_over_http() {
        use std::io::{Read, Write};
        use std::net::{TcpListener, TcpStream};
        use std::sync::atomic::{AtomicBool, Ordering};

        let state = state();
        let flag = Arc::new(AtomicBool::new(false));
        let management = Arc::new(api());
        for (method, path, header, status) in [
            (
                "GET",
                STATUS_PATH,
                "Authorization: Bearer test-key\r\n",
                200,
            ),
            ("GET", STATUS_PATH, "", 401),
            ("GET", "/health", "", 200),
            ("GET", "/v1/providers", "", 200),
            ("GET", "/v1/usage/codex", "", 200),
            ("POST", "/v1/refresh", "", 200),
            ("GET", "/unknown", "", 404),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let (state, flag, management) = (
                Arc::clone(&state),
                Arc::clone(&flag),
                Arc::clone(&management),
            );
            let server = std::thread::spawn(move || {
                let (stream, _) = listener.accept().unwrap();
                crate::handle_connection(stream, state, flag, management);
            });
            let mut client = TcpStream::connect(address).unwrap();
            client
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            write!(
                client,
                "{method} {path} HTTP/1.1\r\nHost: localhost\r\n{header}\r\n"
            )
            .unwrap();
            let mut response = String::new();
            client.read_to_string(&mut response).unwrap();
            assert!(
                response.starts_with(&format!("HTTP/1.1 {status}")),
                "{response}"
            );
            let (headers, body) = response.split_once("\r\n\r\n").unwrap();
            assert!(headers.contains(&format!("Content-Length: {}", body.len())));
            serde_json::from_str::<Value>(body).unwrap();
            server.join().unwrap();
        }
        assert!(flag.load(Ordering::Relaxed));
    }
}
