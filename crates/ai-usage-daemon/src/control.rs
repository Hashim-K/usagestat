//! An optional, separate credential for stopping only a managed daemon.
use crate::{cliproxy, http_request::Request, response_json};
use anyhow::{Result, bail};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
pub struct ControlApi {
    key: Option<String>,
    shutdown: Arc<AtomicBool>,
}

impl std::fmt::Debug for ControlApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ControlApi")
            .field("enabled", &self.key.is_some())
            .finish()
    }
}

impl ControlApi {
    pub fn load(path: Option<&Path>, shutdown: Arc<AtomicBool>) -> Result<Self> {
        let key = path
            .map(usagestat_core::storage::read_private)
            .transpose()?
            .map(|key| key.trim().to_owned());
        if key
            .as_ref()
            .is_some_and(|key| key.is_empty() || !key.bytes().all(|b| b.is_ascii_graphic()))
        {
            bail!("daemon control key must be nonempty ASCII text without whitespace");
        }
        Ok(Self { key, shutdown })
    }

    pub fn route(&self, request: &Request) -> Option<String> {
        if request.path != "/v1/daemon/shutdown" {
            return None;
        }
        let Some(key) = &self.key else {
            return Some(response_json(
                404,
                "Not Found",
                r#"{"error":"daemon_control_disabled"}"#,
            ));
        };
        let supplied = request.header("authorization").and_then(|value| {
            let (scheme, value) = value.split_once(' ')?;
            scheme.eq_ignore_ascii_case("bearer").then_some(value)
        });
        if !supplied.is_some_and(|supplied| cliproxy::keys_equal(key, supplied)) {
            return Some(response_json(
                401,
                "Unauthorized",
                r#"{"error":"invalid_control_key"}"#,
            ));
        }
        if request.method != "POST" {
            return Some(response_json(
                405,
                "Method Not Allowed",
                r#"{"error":"method_not_allowed"}"#,
            ));
        }
        self.shutdown.store(true, Ordering::SeqCst);
        Some(response_json(200, "OK", r#"{"status":"stopping"}"#))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shutdown_requires_its_separate_key_and_post_method() {
        let flag = Arc::new(AtomicBool::new(false));
        let api = ControlApi {
            key: Some("synthetic-control".into()),
            shutdown: flag.clone(),
        };
        for (method, headers, status) in [
            ("POST", "", "401"),
            ("POST", "Authorization: Bearer synthetic-t3\r\n", "401"),
            (
                "POST",
                "Authorization: Bearer synthetic-control\r\nAuthorization: Bearer duplicate\r\n",
                "401",
            ),
            ("GET", "Authorization: Bearer synthetic-control\r\n", "405"),
        ] {
            let request = crate::http_request::read_request(
                format!("{method} /v1/daemon/shutdown HTTP/1.1\r\n{headers}\r\n").as_bytes(),
            )
            .unwrap();
            assert!(
                api.route(&request)
                    .unwrap()
                    .starts_with(&format!("HTTP/1.1 {status}"))
            );
            assert!(!flag.load(Ordering::SeqCst));
        }
        let request = crate::http_request::read_request(
            b"POST /v1/daemon/shutdown HTTP/1.1\r\nAuthorization: Bearer synthetic-control\r\n\r\n"
                .as_slice(),
        )
        .unwrap();
        assert!(
            ControlApi::default()
                .route(&request)
                .unwrap()
                .starts_with("HTTP/1.1 404")
        );
        assert!(api.route(&request).unwrap().starts_with("HTTP/1.1 200"));
        assert!(flag.load(Ordering::SeqCst));
    }
}
