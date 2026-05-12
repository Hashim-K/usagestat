# GNOME Extension — Migration Guide

This guide covers migrating a GNOME Shell extension from a CodexBar / CrossUsage
backend to `ai-usage-backend`.

The key change: instead of shelling out to a CLI tool on every panel refresh,
the extension talks to a local HTTP daemon that handles polling in the background.
The extension becomes a pure display layer.

---

## Architecture

```
GNOME Shell extension
        │  HTTP (loopback)
        ▼
ai-usage-daemon  (127.0.0.1:6736)
        │  JS plugin runtime
        ▼
Provider plugins  (~/.config/ai-usage/plugins)
```

The daemon polls all enabled providers on a configurable interval (default 60 s)
and caches the last successful snapshot for each one. The extension reads that
cache on demand — no blocking, no per-refresh network calls.

---

## Daemon Setup

### Build and install

```bash
cargo build --release -p ai-usage-daemon -p ai-usage-cli
install -Dm755 target/release/ai-usage-daemon ~/.local/bin/ai-usage-daemon
install -Dm755 target/release/ai-usage          ~/.local/bin/ai-usage
```

### Run manually

```bash
ai-usage-daemon
# or with overrides:
ai-usage-daemon --bind 127.0.0.1:6736 --refresh-sec 30
```

### Autostart with systemd user service

Create `~/.config/systemd/user/ai-usage-daemon.service`:

```ini
[Unit]
Description=AI Usage Daemon
After=network.target

[Service]
ExecStart=%h/.local/bin/ai-usage-daemon
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

```bash
systemctl --user daemon-reload
systemctl --user enable --now ai-usage-daemon
```

---

## HTTP API

All endpoints are `GET` except `/v1/refresh`. CORS headers are included on every
response so the extension can call them directly from a GJS `Soup.Session`.

### `GET /health`

```json
{ "status": "ok" }
```

Use this to check whether the daemon is running before rendering anything.

### `GET /v1/providers`

Returns the list of discovered providers and their enabled state.

```json
[
  { "id": "claude",    "name": "Claude",    "enabled": true },
  { "id": "copilot",   "name": "Copilot",   "enabled": true },
  { "id": "deepseek",  "name": "DeepSeek",  "enabled": false }
]
```

### `GET /v1/usage`

Returns all cached snapshots for enabled providers.

```json
[
  {
    "providerId": "claude",
    "displayName": "Claude",
    "source": "api",
    "plan": "Pro",
    "metrics": [ ... ],
    "fetchedAt": "2026-05-12T17:00:00Z"
  }
]
```

### `GET /v1/usage/:providerId`

Returns a single cached snapshot, or `404` if the provider is unknown or has
never been probed.

```json
{
  "providerId": "claude",
  "displayName": "Claude",
  "source": "api",
  "plan": "Pro",
  "metrics": [ ... ],
  "fetchedAt": "2026-05-12T17:00:00Z"
}
```

### `POST /v1/refresh`

Triggers an immediate re-probe cycle. Returns as soon as the flag is set — the
actual probing happens asynchronously.

```json
{ "status": "refresh_scheduled" }
```

Call this when the user clicks a refresh button in the panel.

---

## Snapshot JSON Shape

```typescript
interface UsageSnapshot {
  providerId:  string;
  displayName: string;
  source?:     string;          // "api" | "web" | "cli" | "local" | "error"
  plan?:       string;          // e.g. "Pro", "Team"
  metrics:     MetricLine[];
  fetchedAt:   string;          // UTC ISO 8601
}
```

A `source` of `"error"` means the last probe failed. The `metrics` array will
contain a single `Badge` line with the error message. Display it as a warning
state rather than hiding the provider.

---

## MetricLine Types

Each item in `metrics` has a `type` discriminant.

### `progress`

A usage bar with a used/limit pair and an optional reset time.

```json
{
  "type": "progress",
  "label": "Credits",
  "used": 45.0,
  "limit": 100.0,
  "format": { "kind": "percent" },
  "resetsAt": "2026-06-01T00:00:00Z",
  "periodDurationMs": 2592000000
}
```

`format` variants:

| `kind`     | Rendering hint                        |
|------------|---------------------------------------|
| `percent`  | `used/limit × 100` → percentage bar   |
| `dollars`  | `$used / $limit`                      |
| `count`    | `used / limit <suffix>` (e.g. `req`)  |

Rendering a progress bar:

```js
const pct = Math.min(100, (line.used / line.limit) * 100);
// pct in [0, 100]
```

If `resetsAt` is present, show a countdown or formatted date in the tooltip.
`periodDurationMs` tells you the cadence (e.g. `86400000` = daily) which you
can use to pick an appropriate time format.

### `text`

A key/value label, used for balance amounts, token counts, etc.

```json
{
  "type": "text",
  "label": "Balance",
  "value": "$3.50 (Paid: $3.00 / Granted: $0.50)"
}
```

### `badge`

A status tag, used for errors, plan names, rate-limited states.

```json
{
  "type": "badge",
  "label": "Status",
  "text": "Rate limited",
  "color": "#ef4444"
}
```

`color` is an optional hex hint. Map it to your theme's semantic colors if you
prefer — it is provided as guidance, not a hard requirement.

---

## Picking the Primary Metric

For a compact panel indicator, use the first `progress` line in `metrics` as the
primary value. If there is no `progress` line, show the provider name with a
neutral icon. If `source === "error"`, show a warning icon.

```js
function primaryPercent(snapshot) {
  const line = snapshot.metrics.find(m => m.type === 'progress');
  if (!line) return null;
  return Math.min(100, (line.used / line.limit) * 100);
}
```

---

## Degraded States

| Condition                        | Recommended behaviour                     |
|----------------------------------|-------------------------------------------|
| Daemon not reachable             | Show a "daemon offline" indicator; retry  |
| `source === "error"` on snapshot | Show warning icon; surface badge text     |
| `metrics` is empty               | Show provider name with a neutral icon    |
| `fetchedAt` older than 5 min     | Add a staleness indicator (⚠)            |

---

## Provider Configuration

Enable providers in `~/.config/ai-usage/config.toml`:

```toml
refreshSec = 60

[[providers]]
id = "claude"
enabled = true

[[providers]]
id = "copilot"
enabled = true

[[providers]]
id = "openrouter"
enabled = true
# apiKey = "sk-or-..."   # or set OPENROUTER_API_KEY
```

Most providers are disabled by default and require explicit opt-in. See the
`plugins/` directory for the full list — each plugin's `plugin.json` names the
env vars or files it reads for credentials.

---

## Polling from GJS

Minimal example using `Soup.Session`:

```js
import Soup from 'gi://Soup?version=3.0';
import GLib from 'gi://GLib';

const BASE = 'http://127.0.0.1:6736';
const _session = new Soup.Session();

async function fetchUsage() {
  const msg = Soup.Message.new('GET', `${BASE}/v1/usage`);
  return new Promise((resolve, reject) => {
    _session.send_and_read_async(msg, GLib.PRIORITY_DEFAULT, null, (s, res) => {
      try {
        const bytes = s.send_and_read_finish(res);
        resolve(JSON.parse(new TextDecoder().decode(bytes.get_data())));
      } catch (e) {
        reject(e);
      }
    });
  });
}

async function triggerRefresh() {
  const msg = Soup.Message.new('POST', `${BASE}/v1/refresh`);
  _session.send_and_read_async(msg, GLib.PRIORITY_DEFAULT, null, () => {});
}
```

---

## Migration Checklist

- [ ] Install `ai-usage-daemon` and start it (or enable the systemd unit)
- [ ] Enable desired providers in `~/.config/ai-usage/config.toml`
- [ ] Replace any `GLib.spawn_command_line_sync('codexbar ...')` calls with
      `fetchUsage()` against the daemon
- [ ] Map `MetricLine` types to your existing UI components
- [ ] Wire a refresh button to `POST /v1/refresh`
- [ ] Handle `source === "error"` and daemon-offline states
- [ ] Remove the CodexBar / CrossUsage dependency from the extension's metadata
