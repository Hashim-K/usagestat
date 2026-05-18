# usagestat

Backend for local agent usage data.

This starts from CrossUsage's architecture, but uses separate project names and
contracts:

- `usagestat-core`: shared models, config, paths, cache.
- `usagestat-plugins`: JavaScript provider plugin loader/runtime.
- `usagestat-cli`: scriptable CLI.
- `usagestat-daemon`: local polling daemon with an HTTP API.

## Development

```bash
cargo run -p usagestat-cli -- list
cargo run -p usagestat-cli -- --json usage mock
cargo run -p usagestat-cli -- usage --provider claude --save
cargo run -p usagestat-cli -- status claude codex
cargo run -p usagestat-cli -- export --format csv
cargo run -p usagestat-cli -- auth import-cookies --provider codex --format json
cargo run -p usagestat-cli -- config validate
cargo run -p usagestat-cli -- cache clear --history
cargo run -p usagestat-cli -- plugin validate
cargo run -p usagestat-daemon
curl http://127.0.0.1:6736/v1/usage
```

CLI usage docs: [docs/cli.md](docs/cli.md).

Plugins are discovered from:

1. `USAGESTAT_PLUGIN_DIR`
2. `~/.config/usagestat/plugins`
3. `./plugins`

Bundled providers:

- `mock`: development fixture.
- `host-smoke`: disabled fixture for host API checks.
- `claude`: Claude Code OAuth usage.
- `codex`: Codex/Openagent usage from Codex CLI OAuth auth.
- `copilot`: GitHub Copilot usage via `COPILOT_API_TOKEN`, `GITHUB_TOKEN`, or `GH_TOKEN`.
- `gemini`: Gemini CLI OAuth quota.
- `openrouter`: OpenRouter API credits via `OPENROUTER_API_KEY`.

## Config

Default config path:

```text
~/.config/usagestat/config.toml
```

Default cache path:

```text
~/.local/share/usagestat/snapshots.json
```

Example:

```toml
refreshSec = 60
pluginDirs = ["/path/to/more/plugins"]

[[providers]]
id = "mock"
enabled = true

[[providers]]
id = "claude"
instanceId = "claude-web"
displayName = "Claude Web"
enabled = true
source = "web"
cookieHeader = "sessionKey=..."

[[providers]]
id = "openai-api"
instanceId = "openai-api-eu"
displayName = "OpenAI API EU"
enabled = true
source = "api"
apiKey = "sk-..."
region = "eu"
workspaceId = "workspace-123"

[[providers]]
id = "custom"
instanceId = "local-script"
displayName = "Local Usage Script"
enabled = true
source = "custom"
customCommand = "/path/to/usage-script --json"
```

Both binaries accept overrides:

```bash
cargo run -p usagestat-cli -- --config ./config.toml --plugin-dir ./plugins list
cargo run -p usagestat-daemon -- --config ./config.toml --refresh-sec 30
```

HTTP endpoints currently implemented:

- `GET /health`
- `GET /v1/providers`
- `GET /v1/usage`
- `GET /v1/usage/:providerId`

## Plugin Host API

Provider plugins export `globalThis.__usagestat_plugin.probe(ctx)`.

A copyable plugin template lives at `templates/provider-plugin`. It includes
examples for `api`, `oauth`, `local`, `cli`, and `web` source modes. Dev-only
example providers live under `templates/dev-providers` and are not loaded or
packaged as production providers.

Available context:

- `ctx.nowIso`
- `ctx.sourceMode`
- `ctx.app.version`
- `ctx.app.platform`
- `ctx.app.appDataDir`
- `ctx.app.pluginDataDir`
- `ctx.host.log.info|warn|error(message)`
- `ctx.host.env.get(name)` for allowlisted variables
- `ctx.host.fs.homeDir`
- `ctx.host.fs.exists(path)`
- `ctx.host.fs.readText(path)`
- `ctx.host.fs.listDir(path)`
- `ctx.host.http.request({ url, method, headers, bodyText, timeoutMs })`
- `ctx.host.command.run({ program, args, timeoutMs })` for allowlisted commands

Host HTTP responses use:

```json
{
  "status": 200,
  "headers": {},
  "bodyText": "{}"
}
```
