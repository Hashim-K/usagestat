# AI Usage Backend Feature Roadmap

This backend should be a reusable local service for AI usage data, with a CLI,
daemon, plugin host, and stable HTTP contract that any desktop shell can consume.

## 1. Core Data Contract

- Normalized usage snapshots with stable JSON field names.
- Metric lines for progress, text, badges, balances, and reset windows.
- Provider metadata: id, display name, source, plan, account label, icon, version.
- Error snapshots that preserve previous successful cache entries.
- Stable schema versioning for API consumers.
- Timestamp policy: all persisted/API timestamps in UTC RFC 3339.
- Optional raw/debug payloads behind explicit debug flags only.

## 2. Provider System

- JavaScript provider plugin loading from configured directories.
- Provider manifests with id, name, entrypoint, version, capabilities, auth type.
- Provider enable/disable controls from config.
- Provider ordering from config.
- Per-provider timeout handling.
- Per-provider source selection: auto, api, web, cli, local.
- Multi-account provider instances.
- Native Rust provider escape hatch for providers that need system integration.
- Compatibility shims for importing CrossUsage/OpenUsage plugins.
- Provider test harness for fixture-based plugin tests.

## 3. Plugin Host API

- `ctx.nowIso`, platform, app version, app data dir, plugin data dir.
- Filesystem helpers scoped to known local paths.
- Environment variable reads with explicit allowlist.
- HTTP client with timeout, proxy support, headers, body, and JSON helpers.
- Cookie/header storage helpers.
- Browser cookie import helpers for Chromium and Firefox on Linux.
- Local command runner with timeout and safe stderr/stdout capture.
- JSONL/local log scanners for Codex, Claude, and Cursor.
- Structured plugin logging with secret redaction.
- Host API permission model per provider.

## 4. Config And Persistence

- `~/.config/ai-usage/config.toml` loading.
- Explicit `--config` override for CLI and daemon.
- Plugin directories from config, CLI flags, and `AI_USAGE_PLUGIN_DIR`.
- Cache directory under `~/.local/share/ai-usage`.
- Persist last successful snapshots.
- Persist provider history as JSONL for trends.
- Config migration/versioning.
- Import from `~/.codexbar/config.json`.
- Import from CrossUsage/OpenUsage config where useful.
- Secret storage strategy for Linux: libsecret first, encrypted file fallback.

## 5. CLI

- `ai-usage list` for known providers.
- `ai-usage probe [providers...]` for one-shot usage snapshots.
- `ai-usage export --format json|csv`.
- `ai-usage daemon` or separate daemon binary.
- `ai-usage config validate|dump|init`.
- `ai-usage plugin validate|test`.
- `ai-usage cache inspect|clear`.
- Human output for terminals and JSON output for scripts.
- Exit codes that distinguish config, provider, timeout, and runtime failures.

## 6. Daemon And HTTP API

- Local-only HTTP server on `127.0.0.1`.
- `GET /health`.
- `GET /v1/providers`.
- `GET /v1/usage`.
- `GET /v1/usage/:providerId`.
- `POST /v1/refresh` for manual refresh.
- Server-sent events or WebSocket stream for live updates.
- Configurable bind address and port.
- Background polling with per-provider intervals.
- Cache only successful probes unless explicitly asked for errors.
- API schema docs and sample payloads.

## 7. Provider Coverage

- Start with CrossUsage providers.
- Codex: OAuth/CLI/local logs/web-cookie fallback.
- Claude: OAuth/CLI/local logs/web-cookie fallback.
- Cursor: usage API and dashboard CSV token stats.
- Gemini, Copilot, Kiro, Kimi, MiniMax, OpenRouter, DeepSeek.
- CodexBar/Win-CodexBar parity providers over time.
- Provider status pages where available.

## 8. GNOME Extension Integration

- Stable daemon API that avoids shelling out on every panel refresh.
- Small JSON payloads for panel rendering.
- Provider icons or icon IDs.
- Manual refresh endpoint.
- Config editing path from preferences.
- Migration path from current CodexBar-backed extension.
- Clear degraded states when daemon is absent or provider auth is missing.

## 9. Security And Privacy

- Loopback-only by default.
- No telemetry.
- No secrets in logs, API responses, or debug snapshots.
- Config and cache file permissions set private on write.
- Explicit provider permissions for filesystem, env, command, browser cookies.
- Secret redaction helper shared by daemon, CLI, plugin logs.
- Document every known file path read by built-in providers.

## 10. Packaging And Operations

- Linux binary releases for x86_64 and aarch64.
- `.deb`, `.rpm`, and AppImage packaging.
- systemd user service template.
- GNOME autostart integration.
- Shell completions.
- Release checksums.
- Basic observability: logs, `GET /health`, version endpoint.

## Initial Implementation Order

1. Config loading, provider filtering, configurable plugin directories.
2. Host API basics: env, fs, HTTP.
3. Persisted cache and `/health` / `/v1/providers`.
4. Port first real provider plugin, likely Codex or Claude.
5. Add config validation and plugin validation commands.
