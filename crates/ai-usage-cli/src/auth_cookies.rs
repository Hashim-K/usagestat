use aes::Aes128;
use cbc::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use pbkdf2::pbkdf2_hmac;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use sha1::Sha1;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

type Aes128CbcDec = cbc::Decryptor<Aes128>;

const SESSION_NOT_FOUND: &str = "SESSION_NOT_FOUND";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieImportResult {
    pub provider_id: String,
    pub cookie_header: String,
    pub source: String,
    pub profile: String,
}

#[derive(Debug, Serialize)]
pub struct CookieImportError {
    pub error: String,
    pub message: String,
}

#[derive(Debug)]
struct BrowserCandidate {
    id: &'static str,
    config_dir: PathBuf,
    secret_app_ids: &'static [&'static str],
}

#[derive(Debug)]
struct ProfileCandidate {
    browser_id: &'static str,
    profile: String,
    cookies_db: PathBuf,
    secret_app_ids: &'static [&'static str],
}

#[derive(Debug, Clone)]
struct CookieRecord {
    name: String,
    value: String,
    domain: String,
    path: String,
    expires_utc: i64,
    secure: bool,
    http_only: bool,
}

struct TempCookieDb {
    dir: PathBuf,
    db: PathBuf,
}

impl Drop for TempCookieDb {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Extract the registrable hostname from a URL (e.g. "https://claude.ai/foo" → "claude.ai").
fn host_from_url(url: &str) -> Option<String> {
    let after_scheme = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let host = after_scheme.split('/').next()?;
    let host = host.split(':').next()?; // strip port if any
    if host.is_empty() {
        None
    } else {
        Some(host.to_lowercase())
    }
}

pub fn import_cookies(provider_id: &str, web_url: &str) -> Result<CookieImportResult, CookieImportError> {
    let Some(host) = host_from_url(web_url) else {
        return Err(CookieImportError {
            error: "INVALID_WEB_URL".to_string(),
            message: format!("Provider '{provider_id}' has an invalid webUrl: {web_url}"),
        });
    };

    for profile in discover_profiles() {
        let Ok(cookies) = read_profile_cookies(&profile, &host) else {
            continue;
        };
        if cookies.is_empty() {
            continue;
        }
        let cookie_header = build_cookie_header(cookies, &host);
        if cookie_header.is_empty() {
            continue;
        }
        return Ok(CookieImportResult {
            provider_id: provider_id.to_string(),
            cookie_header,
            source: profile.browser_id.to_string(),
            profile: profile.profile,
        });
    }

    Err(CookieImportError {
        error: SESSION_NOT_FOUND.to_string(),
        message: format!("No browser cookies found for {host}."),
    })
}

fn discover_profiles() -> Vec<ProfileCandidate> {
    let Some(config_home) = dirs::config_dir() else {
        return Vec::new();
    };
    let browsers = [
        BrowserCandidate {
            id: "chrome",
            config_dir: config_home.join("google-chrome"),
            secret_app_ids: &["chrome", "google-chrome"],
        },
        BrowserCandidate {
            id: "brave",
            config_dir: config_home.join("BraveSoftware").join("Brave-Browser"),
            secret_app_ids: &["brave", "brave-browser"],
        },
        BrowserCandidate {
            id: "chromium",
            config_dir: config_home.join("chromium"),
            secret_app_ids: &["chromium"],
        },
    ];

    let mut profiles = Vec::new();
    for browser in browsers {
        if !browser.config_dir.is_dir() {
            continue;
        }
        let mut dirs = vec![browser.config_dir.join("Default")];
        if let Ok(entries) = fs::read_dir(&browser.config_dir) {
            let mut profile_dirs: Vec<PathBuf> = entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("Profile "))
                })
                .collect();
            profile_dirs.sort();
            dirs.extend(profile_dirs);
        }

        for dir in dirs {
            let cookies_db = if dir.join("Network").join("Cookies").is_file() {
                dir.join("Network").join("Cookies")
            } else if dir.join("Cookies").is_file() {
                dir.join("Cookies")
            } else {
                continue;
            };
            let profile = dir
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Default")
                .to_string();
            profiles.push(ProfileCandidate {
                browser_id: browser.id,
                profile,
                cookies_db,
                secret_app_ids: browser.secret_app_ids,
            });
        }
    }
    profiles
}

fn read_profile_cookies(profile: &ProfileCandidate, host: &str) -> Result<Vec<CookieRecord>, String> {
    let temp = copy_cookie_db(&profile.cookies_db)?;
    let conn = Connection::open_with_flags(
        &temp.db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("open cookie db: {error}"))?;

    let mut stmt = conn
        .prepare(
            "SELECT host_key, name, value, encrypted_value, path, expires_utc, is_secure, is_httponly
             FROM cookies
             WHERE host_key LIKE ?1",
        )
        .map_err(|error| format!("prepare cookie query: {error}"))?;
    let domain_pattern = format!("%{host}");
    let rows = stmt
        .query_map([&domain_pattern], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)? != 0,
                row.get::<_, i64>(7)? != 0,
            ))
        })
        .map_err(|error| format!("query cookies: {error}"))?;

    let passwords = browser_passwords(profile.secret_app_ids);
    let mut cookies = Vec::new();
    for row in rows {
        let (domain, name, value, encrypted_value, path, expires_utc, secure, http_only) =
            row.map_err(|error| format!("read cookie row: {error}"))?;
        let Some(cookie_value) = cookie_value(&value, &encrypted_value, &passwords) else {
            continue;
        };
        if cookie_value.is_empty() || name.is_empty() {
            continue;
        }
        cookies.push(CookieRecord {
            name,
            value: cookie_value,
            domain,
            path,
            expires_utc,
            secure,
            http_only,
        });
    }
    Ok(cookies)
}

fn copy_cookie_db(path: &Path) -> Result<TempCookieDb, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("ai-usage-cookies-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&dir).map_err(|error| format!("create temp dir: {error}"))?;
    let db = dir.join("Cookies");
    fs::copy(path, &db).map_err(|error| format!("copy cookie db: {error}"))?;

    for suffix in ["-wal", "-shm"] {
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let sidecar = path.with_file_name(format!("{file_name}{suffix}"));
        if sidecar.is_file() {
            let target = dir.join(format!("Cookies{suffix}"));
            let _ = fs::copy(sidecar, target);
        }
    }

    Ok(TempCookieDb { dir, db })
}

fn browser_passwords(app_ids: &[&str]) -> Vec<String> {
    let mut passwords = Vec::new();
    for app_id in app_ids {
        if let Some(secret) = secret_tool_lookup(app_id) {
            passwords.push(secret);
        }
    }
    passwords.push("peanuts".to_string());
    passwords
}

fn secret_tool_lookup(app_id: &str) -> Option<String> {
    let output = Command::new("secret-tool")
        .args(["lookup", "application", app_id])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let secret = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if secret.is_empty() {
        None
    } else {
        Some(secret)
    }
}

fn cookie_value(value: &str, encrypted_value: &[u8], passwords: &[String]) -> Option<String> {
    if !value.is_empty() {
        return Some(value.to_string());
    }
    if encrypted_value.is_empty() {
        return None;
    }
    for password in passwords {
        if let Some(decrypted) = decrypt_chromium_cookie(encrypted_value, password) {
            return Some(decrypted);
        }
    }
    None
}

fn decrypt_chromium_cookie(encrypted_value: &[u8], password: &str) -> Option<String> {
    if encrypted_value.starts_with(b"v10") || encrypted_value.starts_with(b"v11") {
        decrypt_aes128_cbc(&encrypted_value[3..], password)
    } else {
        String::from_utf8(encrypted_value.to_vec()).ok()
    }
}

fn decrypt_aes128_cbc(payload: &[u8], password: &str) -> Option<String> {
    let mut key = [0_u8; 16];
    pbkdf2_hmac::<Sha1>(password.as_bytes(), b"saltysalt", 1, &mut key);
    let iv = [b' '; 16];
    let decrypted = Aes128CbcDec::new(&key.into(), &iv.into())
        .decrypt_padded_vec_mut::<Pkcs7>(payload)
        .ok()?;
    clean_decrypted_cookie_value(&decrypted)
}

fn clean_decrypted_cookie_value(decrypted: &[u8]) -> Option<String> {
    let mut candidates: Vec<&[u8]> = Vec::new();
    if let Some(jwt_start) = decrypted.windows(3).position(|window| window == b"eyJ") {
        if jwt_start < 48 {
            candidates.push(&decrypted[jwt_start..]);
        }
    }
    for offset in [32, 28, 0] {
        if decrypted.len() > offset {
            candidates.push(&decrypted[offset..]);
        }
    }

    candidates.into_iter().find_map(clean_cookie_candidate)
}

fn clean_cookie_candidate(candidate: &[u8]) -> Option<String> {
    let mut value = String::from_utf8_lossy(candidate)
        .chars()
        .filter(|character| *character != '\r' && *character != '\n' && *character != '\t')
        .collect::<String>();

    value = value
        .chars()
        .filter(|character| character.is_ascii() && !character.is_ascii_control())
        .collect();

    let value = value
        .trim_matches(|character: char| !is_cookie_value_character(character))
        .to_string();

    if value.is_empty() {
        return None;
    }

    let sample_len = value.len().min(10);
    if !value.as_bytes()[..sample_len]
        .iter()
        .all(|byte| byte.is_ascii_graphic() || *byte == b' ')
    {
        return None;
    }

    Some(value)
}

fn is_cookie_value_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || "-_.~%|=/+".contains(character)
}

fn build_cookie_header(cookies: Vec<CookieRecord>, host: &str) -> String {
    let mut seen = HashSet::new();
    let mut ordered = cookies;
    ordered.sort_by(|a, b| {
        cookie_rank(b, host)
            .cmp(&cookie_rank(a, host))
            .then_with(|| a.name.cmp(&b.name))
    });
    ordered
        .into_iter()
        .filter(|cookie| seen.insert(cookie.name.clone()))
        .map(|cookie| format!("{}={}", cookie.name, cookie.value))
        .collect::<Vec<_>>()
        .join("; ")
}

fn cookie_rank(cookie: &CookieRecord, host: &str) -> i32 {
    let mut rank = 0;
    // Cookies on the exact host rank higher than subdomain cookies.
    if cookie.domain.trim_start_matches('.') == host {
        rank += 10;
    }
    if cookie.secure {
        rank += 2;
    }
    if cookie.http_only {
        rank += 2;
    }
    if cookie.path == "/" {
        rank += 1;
    }
    if cookie.expires_utc == 0 {
        rank += 1;
    }
    rank
}
