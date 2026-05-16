# AI Usage CLI

`usagestat` is a scriptable command line interface for discovering provider
plugins, probing live usage, checking provider status pages, exporting usage
snapshots, and managing backend config/cache state.

The CLI is intended to grow into a superset of CodexBar's CLI capabilities.
`usage`, `status`, `cost`, `config`, and `cache` mirror CodexBar's command
families where the backend already has equivalent data.

## Running The CLI

From a checkout, run the binary through Cargo:

```bash
cargo run -p usagestat-cli -- list
```

For repeated use, build or install the CLI and then run `usagestat` directly:

```bash
cargo build -p usagestat-cli
./target/debug/usagestat list
```

```bash
cargo install --path crates/ai-usage-cli
usagestat list
```

When running through Cargo, everything after `--` is passed to `usagestat`.

## Quick Start

List discovered providers:

```bash
usagestat list
```

List enabled/disabled state for specific providers:

```bash
usagestat list claude codex
```

Check live status pages for specific providers:

```bash
usagestat status claude codex
```

Probe every enabled provider:

```bash
usagestat usage
```

Probe one provider:

```bash
usagestat usage claude
usagestat usage --provider claude
```

Probe multiple providers:

```bash
usagestat usage claude codex gemini
```

Emit JSON for scripts:

```bash
usagestat --json usage claude
```

Save a history record while probing:

```bash
usagestat usage claude --save
```

Export current live data as CSV:

```bash
usagestat export --format csv
```

Export saved history:

```bash
usagestat export --from-file ~/.local/share/usagestat/history.jsonl --format csv
```

Validate or dump config:

```bash
usagestat config validate
usagestat config dump
```

Import ChatGPT/OpenAI browser cookies for Codex web usage:

```bash
usagestat auth import-cookies --provider codex --format json
```

## Global Options

Global options can be placed before or after the subcommand.

```text
--json              Print JSON when the command supports it.
--json-only         CodexBar-compatible alias for --json.
--pretty            Accepted; JSON is currently pretty-printed by default.
--json-output       Accepted CodexBar-compatible structured log flag.
--log-level <LEVEL> Accepted CodexBar-compatible log-level flag.
-v, --verbose       Accepted CodexBar-compatible verbose flag.
--no-color          Disable ANSI colors in text output.
--plain             Use plain text instead of tables.
--config <PATH>     Read config from a custom TOML file.
--plugin-dir <DIR>  Add a plugin discovery directory. May be repeated.
--all               Include disabled providers.
-h, --help          Show help.
-V, --version       Show version.
```

Examples:

```bash
usagestat --config ./config.toml list
usagestat --plugin-dir ./plugins --plugin-dir ~/usagestat-plugins list
usagestat --all probe openrouter
```

## Commands

### `usage`

Runs provider plugins and prints live usage snapshots.

```bash
usagestat usage [PROVIDER_IDS]...
```

If no provider IDs are supplied, `usage` runs every enabled provider. Each
provider has its own timeout. The default timeout is 120 seconds.

```bash
USAGESTAT_PROBE_TIMEOUT_SEC=30 usagestat usage claude
```

Options:

```text
--save              Append results to ~/.local/share/usagestat/history.jsonl.
--status            Fetch provider status-page state and include it with output.
--provider <ID>     Provider to query. Also accepts all enabled providers, or both.
--format text|json  Output format.
--source <SOURCE>   Compatibility option: auto|web|cli|oauth|api|local.
--web               Alias for --source web.
--account <LABEL>   Accepted for compatibility; routing is not implemented yet.
--account-index <N> Accepted for compatibility; routing is not implemented yet.
--all-accounts      Accepted for compatibility; routing is not implemented yet.
```

`--source` is accepted so scripts can use the CodexBar-style shape, but source
selection is currently implemented inside provider plugins. Native Rust
providers should make this a strict provider fetch-mode selector.

CodexBar provider aliases are accepted where plugin IDs differ:

```text
opencodego -> opencode-go
kimik2     -> kimi-k2
jetbrains  -> jetbrains-ai-assistant
z-ai       -> zai
```

Examples:

```bash
usagestat usage
usagestat --provider claude
usagestat --format json --provider all
usagestat usage claude codex
usagestat usage --provider all
usagestat usage --provider both
usagestat --json usage gemini
usagestat usage claude --save
usagestat usage claude --status
```

### `probe`

Alias-compatible usage probing command. It supports the same options as
`usage`.

```bash
usagestat probe [PROVIDER_IDS]...
```

### `list`

Shows every discovered provider and whether it is enabled by config.

```bash
usagestat list
usagestat list claude codex
usagestat list --plain
usagestat list --json
```

This command does not contact provider APIs. It only loads plugin manifests and
config. Pass provider IDs to show only those providers.

### `status`

Fetches provider status-page state without probing usage.

```bash
usagestat status [PROVIDER_IDS]...
```

The command uses `Status` links in plugin manifests and reads the common
Statuspage endpoint at `/api/v2/status.json`.

```bash
usagestat status claude codex
usagestat status --provider claude
usagestat status claude codex --plain
usagestat status claude codex --json
```

### `cost`

Prints normalized cost/token data from live snapshots, or from saved history
when `--from-file` is supplied.

```bash
usagestat cost [PROVIDER_IDS]...
```

Options:

```text
--format text|json|csv   Output format. Defaults to text.
--from-file <PATH>       Read JSONL history instead of probing live.
--refresh                Accepted for compatibility; live snapshot cost has no cache to bypass.
```

This is not yet equivalent to CodexBar's native local cost scanner. It only
reports fields the backend can normalize from provider snapshots/history. Native
Rust cost scanners for Claude/Codex logs should replace this with true local
cost parity.

```bash
usagestat cost claude codex
usagestat cost --provider claude
usagestat cost --from-file ~/.local/share/usagestat/history.jsonl --format csv
```

### `export`

Exports either live probe results or records from a saved JSONL history file.

```bash
usagestat export [PROVIDER_IDS]...
```

Options:

```text
--format json|csv        Output format. Defaults to json.
--from-file <PATH>       Read JSONL history instead of probing live.
```

When `--from-file` is used, provider IDs filter the file contents:

```bash
usagestat export --from-file ~/.local/share/usagestat/history.jsonl claude codex
```

CSV output columns:

```text
ts,provider_id,display_name,plan,primary_percent,input_tokens,output_tokens,cost,reset_time
```

Examples:

```bash
usagestat export --format json
usagestat export claude --format csv
usagestat export --from-file ~/.local/share/usagestat/history.jsonl --format csv
```

### `plugin validate`

Loads all discovered plugin manifests and reports the providers that can be
validated by the loader.

```bash
usagestat plugin validate
usagestat plugin validate --json
```

This is useful after adding a plugin directory or editing a manifest.

### `config validate`

Parses the configured TOML file and reports success. Parse errors are reported
before the command runs.

```bash
usagestat config validate
usagestat config validate --json
```

### `config dump`

Prints the normalized config as JSON.

```bash
usagestat config dump
```

### `cache clear`

Clears backend cache files.

```bash
usagestat cache clear --snapshots
usagestat cache clear --history
usagestat cache clear --cookies
usagestat cache clear --cost
usagestat cache clear --cookies --provider claude
usagestat cache clear --all
usagestat cache clear --all --json
```

Current backend caches:

```text
snapshots   ~/.local/share/usagestat/snapshots.json
history     ~/.local/share/usagestat/history.jsonl
```

### `auth import-cookies`

Imports browser cookies into a raw `Cookie` header suitable for `cookieHeader`
config fields. The command currently supports Codex/OpenAI cookies from
Chromium-family browser profiles on Linux: Chrome, Brave, and Chromium.

```bash
usagestat auth import-cookies --provider codex --format json
```

The importer copies locked SQLite cookie databases to a temporary directory
before reading, tries Linux Secret Service through `secret-tool`, and falls back
to Chromium's legacy `peanuts` key where applicable. Cookie values are only
printed in the success payload.

Successful JSON output:

```json
{
  "providerId": "codex",
  "cookieHeader": "name=value; other=value",
  "source": "chrome",
  "profile": "Default"
}
```

If no usable ChatGPT/OpenAI session cookie is found, the command exits non-zero:

```json
{
  "error": "SESSION_NOT_FOUND",
  "message": "No usable ChatGPT/OpenAI browser cookies found."
}
```

## Plugin Discovery

Plugins are discovered in this order:

1. Directories passed with `--plugin-dir`
2. `pluginDirs` from the config file
3. `USAGESTAT_PLUGIN_DIR`
4. `~/.config/usagestat/plugins`
5. `./plugins`

Duplicate directory paths are ignored after their first occurrence.

When running `usagestat` outside the repository, `./plugins` is relative to your
current shell directory. For an installed user-local binary, either copy plugins
to `~/.config/usagestat/plugins`, set `USAGESTAT_PLUGIN_DIR`, or add the repo's
plugin directory to config:

```toml
pluginDirs = ["/mnt/shared/Git/usagestat/plugins"]
```

## Config

The default config path is:

```text
~/.config/usagestat/config.toml
```

Minimal example:

```toml
refreshSec = 60
pluginDirs = ["/path/to/more/plugins"]

[[providers]]
id = "mock"
enabled = true

[[providers]]
id = "openrouter"
enabled = true
```

Provider entries override the plugin manifest default. Use this to disable a
provider that is enabled by default, or to enable one that is disabled by
default.

```toml
[[providers]]
id = "claude"
enabled = false

[[providers]]
id = "openrouter"
enabled = true
```

Provider entries can also carry the richer provider-page model used by desktop
frontends:

```toml
[[providers]]
id = "claude"
instanceId = "claude-web"
tabParent = "claude"
displayName = "Claude Web"
enabled = true
source = "web" # auto | web | cli | oauth | api | local | custom
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
settings.project = "my-project"
```

Supported provider config fields:

```text
id              Provider plugin id.
instanceId      Stable key for multiple instances of the same provider.
tabParent       Optional parent/group id for child sources under one tab.
displayName     User-facing label override.
enabled         Whether this provider instance should be probed by default.
source          auto, web, cli, oauth, api, local, or custom.
customCommand   Command for custom command-backed providers.
apiKey          Common API-key credential field.
cookieHeader    Common browser/session cookie field.
region          Common region field.
workspaceId     Common workspace/project field.
settings        Provider-specific TOML table for extra fields.
```

The current probe implementation still selects providers by plugin `id`, so
multiple instances and source-specific routing are schema-supported but not fully
executed yet.

## Provider Authentication

Most providers read credentials from the same local files used by their own
CLIs or apps. Some providers also support environment variables. Common examples
include:

```text
CLAUDE_AI_SESSION_KEY      Claude web sessionKey cookie fallback.
CLAUDE_WEB_SESSION_KEY     Alias for CLAUDE_AI_SESSION_KEY.
COPILOT_API_TOKEN          GitHub Copilot API token.
GITHUB_TOKEN               GitHub token fallback for Copilot.
GH_TOKEN                   GitHub token fallback for Copilot.
OPENROUTER_API_KEY         OpenRouter API key.
ARK_API_KEY                Doubao / Volcengine API key.
DOUBAO_API_KEY             Doubao API key alias.
VOLCENGINE_API_KEY         Doubao API key alias.
MISTRAL_COOKIE             Mistral web cookie.
OLLAMA_COOKIE              Ollama web cookie.
```

If a probe reports that it is not logged in, authenticate with that provider's
official CLI/app first or set the documented environment variable for the
plugin.

## Scripting

Use `--json` for complete normalized snapshots:

```bash
usagestat --json usage claude | jq '.[0].metrics'
```

Use `export` for one flat record per provider snapshot:

```bash
usagestat export claude codex --format csv > usage.csv
```

Use `probe --save` from cron or systemd timers, then export historical records:

```bash
usagestat usage claude codex --save
usagestat export --from-file ~/.local/share/usagestat/history.jsonl --format json
```

## CodexBar CLI Parity

Implemented command families:

```text
CodexBar usage          usagestat usage / usagestat probe
CodexBar usage --status usagestat usage --status, or usagestat status
CodexBar cost           usagestat cost, backed by normalized snapshots/history
CodexBar config validate usagestat config validate
CodexBar config dump    usagestat config dump
CodexBar cache clear    usagestat cache clear
```

Known gaps that need native backend work:

```text
Strict --source selection per provider
Token account routing: --account, --account-index, --all-accounts are accepted but not wired yet
Native Claude/Codex local cost log scanning
Provider debug flags such as --web-debug-dump-html and provider-specific plan/API dumps
CodexBar config import/migration from ~/.codexbar/config.json
Cookie cache clearing is accepted but no backend cookie cache exists yet
```

## Troubleshooting

Show discovered providers and enabled state:

```bash
usagestat list --all
```

Validate plugin manifests:

```bash
usagestat plugin validate
```

Use a shorter timeout while debugging a slow provider:

```bash
USAGESTAT_PROBE_TIMEOUT_SEC=10 usagestat usage claude
```

Probe a disabled provider explicitly:

```bash
usagestat --all usage openrouter
usagestat --all --provider openrouter
```

Run with a known plugin directory:

```bash
usagestat --plugin-dir ./plugins list
```
