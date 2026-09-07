# Backend/bar integration contract

Tracker: [backend #17](https://github.com/hashimkarim/usagestat/issues/17).
The native frontends are explicit completion dependencies:
[Windows bar #14](https://github.com/hashimkarim/usagestat-bar/issues/14) and
[macOS bar #15](https://github.com/hashimkarim/usagestat-bar/issues/15).

`tools/portability/bar_contract.cjs` runs the **committed bar CLI client** at
[`72b062b90f2dd5e86d0ad35587f93794167aa495`](https://github.com/hashimkarim/usagestat-bar/tree/72b062b90f2dd5e86d0ad35587f93794167aa495)
against the executing native backend. `tests/fixtures/bar-client` preserves its
MIT license, exact CLI source and the unmodified provider-ID helper functions,
with hashes and extraction provenance. The fixture supplies a Node adapter for
Gio process creation and GLib timers/dates; it does not simulate or implement the
GNOME, Windows or macOS UI. Source fixtures never enter native/npm packages.

Run it against a built CLI with an absolute path:

```sh
node tools/portability/bar_contract.cjs /absolute/path/to/usagestat
```

All five native CI targets run it after building. It uses temporary config/data,
Unicode/spaces in config/plugin paths, synthetic providers, explicit executable
selection and subprocess argument arrays. Windows runs without HOME. It never
probes a real provider, registers a login service or edits the bar checkout.

The compatibility baseline remains backend 1.0.3 CLI JSON:

| Client operation | Required behavior |
| --- | --- |
| `--json [--config PATH] [--plugin-dir PATH] list` | Array of provider manifests with IDs and usable absolute icon paths |
| `--json [options] usage --provider ID --source MODE` | Array of usage snapshots; the current single-provider bar consumes the first |
| Progress metrics | Preserve used/limit, format, reset timestamp and period duration; bar derives bounded percentages and window minutes |
| Text/badge metrics | Preserve account/status text; no fabricated progress for absent data |
| Probe failure | Existing `source: error` plus Error badge remains usable; additive `state` distinguishes missing auth, unsupported, denied, unavailable and failure |
| Optional `cost --provider ID` | Failure cannot discard a valid quota snapshot; client cost normalization retains currency/tokens/totals |
| Missing executable | Clear installation/path error before a provider request |

The committed client carries unknown/additive snapshot fields through normalization.
It does not yet negotiate `capabilities --json`; native frontend work must do so
before exposing an optional feature. `schemaVersion` and `apiVersion` identify the
protocol; `backendVersion` identifies the binary. Keep baseline list/usage behavior
for compatible versions instead of requiring identical frontend/backend version
numbers. A missing/unknown feature should give a targeted update or manual-route
message, while working baseline features remain usable. `implemented`, `runtime`
and `qualification` are separate: `not-checked` is not signed-out, and native
fixtures are not evidence of a working live account.

Native frontend discovery must prefer a configured executable, then its matching
bundled backend or durable installed locations, without an interactive shell.
Homebrew exposes `bin/usagestat` and `opt/usagestat/bin/usagestat`; native Windows
archives expose `usagestat.exe`. A global npm installation provides a Node launcher
that verifies/selects its native optional package. Windows clients must use Node
with the launcher and an argument array, or a verified resolved native executable,
rather than assuming an npm `.cmd` shim is a native PE executable. The package's
`launcher.cjs` exports `resolveNative(command, packageRoot)` for integration with
an already identified installation; it validates package identity and payload
hashes. Do not guess a writable cwd-relative plugin tree or persist a temporary
`npm exec` cache as a service owner.

Before starting a background backend, inspect `daemon status --json`. Reuse a
compatible healthy managed installation. An external/wrong-owner/wrong-version
status must identify the required selection/update; transfer requires the user's
explicit `daemon enable --switch-owner`. Installation itself does not enable
autostart. T3 management credentials remain private and saved mode survives
restart. Frontends should retain provider preferences/history when the daemon is
temporarily unavailable, and distinguish an unsupported auth method from an empty
quota. [Capabilities](capabilities.md), [npm distribution](npm-distribution.md),
and [macOS distribution](macos-distribution.md) define the backend details.

Remaining acceptance includes native frontend executable discovery, capability
presentation, duplicate prevention, active upgrade/reconnect, bundled/standalone
coexistence and live credential flows. The Node contract fixture establishes the
existing client's CLI compatibility only; those end-to-end checks stay open in
#17 and #20.
