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
cargo run -p ai-usage-daemon
curl http://127.0.0.1:6736/v1/usage
```

Plugins are discovered from:

1. `AI_USAGE_PLUGIN_DIR`
2. `~/.config/ai-usage/plugins`
3. `./plugins`

## Config

Default config path:

```text
~/.config/ai-usage/config.toml
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
