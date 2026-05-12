# ai-usage-backend

Backend for local AI usage data.

This starts from CrossUsage's architecture, but uses separate project names and
contracts:

- `ai-usage-core`: shared models, config, paths, cache.
- `ai-usage-plugins`: JavaScript provider plugin loader/runtime.
- `ai-usage-cli`: scriptable CLI.
- `ai-usage-daemon`: local polling daemon with an HTTP API.

## Development

```bash
cargo run -p ai-usage-cli -- list
cargo run -p ai-usage-cli -- --json probe mock
cargo run -p ai-usage-cli -- plugin validate
cargo run -p ai-usage-daemon
curl http://127.0.0.1:6736/v1/usage
```

Plugins are discovered from:

1. `AI_USAGE_PLUGIN_DIR`
2. `~/.config/ai-usage/plugins`
3. `./plugins`

Bundled providers:

- `mock`: development fixture.
- `host-smoke`: disabled fixture for host API checks.
- `copilot`: GitHub Copilot usage via `COPILOT_API_TOKEN`, `GITHUB_TOKEN`, or `GH_TOKEN`.
- `gemini`: Gemini CLI OAuth quota.
- `openrouter`: OpenRouter API credits via `OPENROUTER_API_KEY`.

## Config

Default config path:

```text
~/.config/ai-usage/config.toml
```

Default cache path:

```text
~/.local/share/ai-usage/snapshots.json
```

Example:

```toml
refreshSec = 60
pluginDirs = ["/path/to/more/plugins"]

[[providers]]
id = "mock"
enabled = true
```

Both binaries accept overrides:

```bash
cargo run -p ai-usage-cli -- --config ./config.toml --plugin-dir ./plugins list
cargo run -p ai-usage-daemon -- --config ./config.toml --refresh-sec 30
```

HTTP endpoints currently implemented:

- `GET /health`
- `GET /v1/providers`
- `GET /v1/usage`
- `GET /v1/usage/:providerId`

## Plugin Host API

Provider plugins export `globalThis.__ai_usage_plugin.probe(ctx)`.

Available context:

- `ctx.nowIso`
- `ctx.app.version`
- `ctx.app.platform`
- `ctx.host.log.info|warn|error(message)`
- `ctx.host.env.get(name)` for allowlisted variables
- `ctx.host.fs.homeDir`
- `ctx.host.fs.exists(path)`
- `ctx.host.fs.readText(path)`
- `ctx.host.fs.listDir(path)`
- `ctx.host.http.request({ url, method, headers, bodyText, timeoutMs })`

Host HTTP responses use:

```json
{
  "status": 200,
  "headers": {},
  "bodyText": "{}"
}
```
