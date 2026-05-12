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
