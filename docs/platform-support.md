# Native backend support contract

Implementation tracker: [#1](https://github.com/Hashim-K/usagestat/issues/1).
Architecture decision: [ADR 0001](decisions/0001-native-backend.md).
This document defines the port; it does **not** announce Windows or macOS package
availability. Existing published Linux packages retain their current support.

| Target | Candidate build floor | Native CI | Release status |
| --- | --- | --- | --- |
| `x86_64-unknown-linux-gnu` | Existing packages: glibc 2.39+ | Ubuntu 24.04 x64 | Existing distribution |
| `aarch64-unknown-linux-gnu` | Existing packages: glibc 2.39+ | Ubuntu 24.04 ARM64 | Existing distribution |
| `aarch64-apple-darwin` | macOS 11 | macOS 15 ARM64 | Qualification pending |
| `x86_64-apple-darwin` | macOS 11 | macOS 15 Intel | Qualification pending |
| `x86_64-pc-windows-msvc` | Windows 10 / Server 2016 | Windows Server 2025 x64 | Qualification pending |

Windows ARM64, 32-bit platforms, and musl/Alpine are separate future targets.
Installing through npm does not add support for an unqualified OS, architecture,
or libc. A successful build on a new runner does not verify an older OS floor.
Before advertising those floors, run the installed runtime tests on the minimum
OS and record the result in [#20](https://github.com/Hashim-K/usagestat/issues/20).

The initial CI toolchain is Rust 1.89.0, matching the tested Linux checkout.
Rust's [Apple target documentation](https://doc.rust-lang.org/rustc/platform-support/apple-darwin.html)
allows macOS 10.12 for Intel and 11 for ARM64; the common candidate deployment
target is deliberately 11. Rust's [target table](https://doc.rust-lang.org/rustc/platform-support.html)
lists Windows 10+/Server 2016+ for MSVC x64. These are Rust floors, not proof for
all our dependencies. The pinned rquickjs 0.11 README describes MSVC as
experimental; QuickJS C compilation, bundled SQLite, ring/rustls, and signal-hook
must pass on each native target before it is qualified.
Explicit runner labels follow the [runner image inventory](https://github.com/actions/runner-images/blob/main/README.md);
the gate checks both the Rust host triple and the executing Python architecture.

## Backend and client ownership

Keep one Rust implementation: `usagestat` provides CLI JSON; `usagestatd` polls
providers and serves the dashboard and HTTP API. The backend is a per-user
process. Foreground execution remains available everywhere; normal autostart is
explicitly enabled through the CLI using systemd user units on Linux, a
LaunchAgent on macOS, and a per-user scheduled task on Windows. Installation
alone must not enable autostart. Native adapter fixtures pass; normal desktop
login/reboot, minimum-OS and active upgrade qualification remain pending.

The default endpoint stays `127.0.0.1:6736`. The management/T3 API stays opt-in and
requires its management key. A configured non-loopback bind must remain explicit.
Separate dev/release profiles must use separate state and service identities;
running both daemons also requires different bind addresses/ports.

The current `usagestat-bar` client calls `--json list` and
`--json usage --provider ID --source MODE`, with `--config` and `--plugin-dir`
where configured. Preserve those arrays, field names, provider IDs, absolute icon
paths, error representation, and exit behavior. Preserve optional `cost` calls and
the existing `/health`, `/v1/providers`, `/v1/usage`, and history routes. HTTP does
not replace the current bar's CLI integration. Additive capability/version fields
will be specified in [#13](https://github.com/Hashim-K/usagestat/issues/13); do not
silently interpret an unsupported feature as a logged-out provider.

The compatibility baseline is the CLI/HTTP shape in backend 1.0.3. Exact bar and
backend release pairs require integration fixtures in
[#17](https://github.com/Hashim-K/usagestat/issues/17), rather than assuming equal
version numbers. A missing capability must show an actionable unsupported result.
A version mismatch must retain working baseline features and identify which
requested features need an update. Define the actual capability schema before
adding a new client requirement.

Desktop distributions should bundle a matching backend and plugin set. Standalone
bar installs should allow an explicit CLI path, then discover a durable installed
CLI (including a global npm installation). Each selected profile has one recorded
installation owner and resolved binary/plugin paths. Reuse a compatible managed
daemon. If another installation owns the profile or port, show its identity and
offer an explicit switch; do not start a duplicate or kill an unrelated process.
Implement persistence and conflict handling in
[#7](https://github.com/Hashim-K/usagestat/issues/7).

The bar integration deliverables are: platform-native executable discovery without
`bash`, compatible CLI/API fixtures, capability and version handling, one-owner
startup/stop behavior, and distribution-specific update tests. The current Linux
GJS/D-Bus bar bridge is a client concern, not a backend service protocol.

## Filesystem and profiles

The `dirs` crate supplies native user directories. Windows uses Known Folder
APIs, so guessed home-relative `AppData` paths and environment-only tests do not
establish redirection support.

| Content | Linux | macOS | Windows |
| --- | --- | --- | --- |
| Config | `$XDG_CONFIG_HOME/usagestat`, default `~/.config/usagestat` | `~/Library/Application Support/usagestat` | Roaming AppData `usagestat` |
| Local data/history/cache | `$XDG_DATA_HOME/usagestat`, default `~/.local/share/usagestat` | `~/Library/Application Support/usagestat` | Local AppData `usagestat` |
| Config file | `config.toml` inside config directory | Same | Same |
| Plugin writable state | `plugins/<provider-id>` inside data directory | Same | Same |

Executable names `usagestat-dev`/`usagestatd-dev`, including `.exe`, select the
`usagestat-dev` profile. Identity comes from the executable, not the name of an
overridden directory. An override is the full application directory; callers
running dev and release together must provide distinct override paths.

Path precedence:

1. `--config PATH` selects the config file; it does not move writable data.
2. Nonempty `USAGESTAT_CONFIG_DIR` and `USAGESTAT_DATA_DIR` override the respective
   application directories, otherwise use the native defaults above.
3. Plugin directories: CLI `--plugin-dir`, config `pluginDirs`, then
   `USAGESTAT_PLUGIN_DIR` (legacy `AI_USAGE_PLUGIN_DIR` fallback), config `plugins`,
   and dev-profile data `plugins`.
4. Installed resources: prefix `share/<profile>/plugins`, `lib/<profile>/plugins`,
   prefix `plugins` for a `bin/` layout; app `Contents/Resources/plugins`;
   executable-adjacent `plugins`. The source checkout's relative `plugins` remains
   the last fallback. Canonically identical directories are deduplicated.

CLI-supplied relative paths resolve from the caller's directory. Service setup
must persist absolute paths and these explicit overrides; it must not depend on
a login shell, shell initialization files, or npm's temporary execution cache.
Helpers need a recorded, platform-appropriate environment and executable search
path; authentication should use configured credentials or native stores.

Existing Linux management keys remain at `config/t3-management-key`. Windows
private state and its management key need owner-only access in local data;
macOS private files need owner-only access in Application Support. Implement
private directories, ACLs, safe replacement, and missing-directory diagnostics in
[#6](https://github.com/Hashim-K/usagestat/issues/6). The portable random source
alone does not qualify Windows secret storage. Shared home expansion, redirected
provider paths, missing `HOME`, and writable-state error reporting remain in
[#4](https://github.com/Hashim-K/usagestat/issues/4) and
[#11](https://github.com/Hashim-K/usagestat/issues/11).

## Distribution, including npm

Supported distribution channels will include existing Linux packages, Homebrew,
native archives/installers, desktop bundles, and **npm** for each qualified target.
Track npm implementation in [#21](https://github.com/Hashim-K/usagestat/issues/21).

The npm main package exposes `usagestat` and `usagestatd` through small Node `bin`
launchers. Exact-version optional packages contain each native OS/CPU/libc build,
both Rust binaries, and plugins/icons. This keeps the backend in Rust and avoids a
compiler requirement on the user's machine. Packages must work with lifecycle
scripts disabled. Use npm's [package metadata](https://docs.npmjs.com/cli/v11/configuring-npm/package-json)
for `os`/`cpu`/`libc` constraints and clear unsupported
target/missing optional-package errors; never silently select an incompatible
binary or download executable code during startup.

Package naming and registry ownership must be verified in #21 before first
publication. Define one controlled namespace and publish native dependencies
before the matching launcher version. Test with Node 24 LTS initially; the final
Node/npm engine ranges must match the launcher/install test matrix. Node is needed
for the npm launcher, but native archives and bundled desktop backends have no
Node runtime dependency.

A global npm install is a durable installation owner. `npx`/`npm exec` can run
foreground commands, but a temporary package cache cannot become a service path.
Autostart setup must resolve the native binary and assets to durable absolute paths.
Updates must stop the owned daemon, replace matching binaries/assets, refresh its
recorded paths, restart, and check health/version; define rollback on failure.
Before uninstall, explicitly disable the owned service. Because npm scripts may
be disabled, lifecycle hooks cannot be the only cleanup mechanism: document the
CLI cleanup command and detect stale registrations. Preserve user config/history
unless explicitly requested otherwise. Test coexistence with Homebrew, archives,
and bundled installations.

## Provider and release qualification

[The provider inventory](provider-compatibility.md) records every bundled manifest
and declared source mode. Its baseline is conservative: no live-provider/native
verification is inferred from compilation, fixture tests, or a manifest entry.
Audit each actual credential acquisition path in #11; a source mode alone is not
a complete authentication contract.

Required representative beta integrations:

| Integration | Validation |
| --- | --- |
| OpenAI API | Explicit configured/environment API key with suitable usage permissions; HTTP/TLS behavior |
| Codex | Local log ingestion and actual OAuth credential discovery; same report fixtures on each OS |
| Cursor | Stable and Nightly SQLite/profile paths, including locked/copied database behavior |
| Claude | Local logs plus actual macOS Keychain and supported Windows credentials/fallbacks |
| Browser-backed sources | Manual supported credentials separately from automatic cookie import |

macOS browser import is [#18](https://github.com/Hashim-K/usagestat/issues/18).
Windows browser authentication is [#19](https://github.com/Hashim-K/usagestat/issues/19):
upstream encryption restrictions may make automatic import unsupported. Document
supported manual/OAuth/API alternatives per provider. Never report a restricted
import mechanism as proof that the entire provider is unsupported.

Foundation exit: both binaries build and execute natively on all initial targets,
including clean-cache and reused-cache runs; runtime fixtures pass; remaining
behavioral gaps have owning issues. Desktop beta additionally requires native
service lifecycle, private storage, the representative providers, signed/native
distribution, npm, and bar ownership/version integration. Stable requires the full
applicable suites, explicit provider/auth classifications, minimum-OS testing,
upgrade/rollback/uninstall coverage, and credentialed real-device evidence. A
runner's passing synthetic provider does not qualify a user's browser or IDE.

## Qualification evidence

Native build, runtime, and installed-layout checks are tracked in #3. They must
record toolchain/dependency versions and execute both binaries outside the source
checkout, with isolated synthetic credentials and profiles. Deterministic local
HTTP fixtures and external HTTPS/trust-store checks must be reported separately.
The implementation issues remain open until their native acceptance checks pass.

Run `python tools/portability/native_checks.py --target <native-rust-triple>` for
the native build, Rust tests, dashboard tests, provider inventory, installed
runtime smoke, and probe cancellation checks. Reports are saved under
`target/native-results`. Use `--smoke-temp-dir` for a larger scratch volume when
the default temporary filesystem cannot hold the debug binaries. Permission
tests need a filesystem with native permission semantics.

The shared path APIs now return an actionable error if a native config/data
directory is unavailable; they never select the working directory as a fallback.
Nonempty explicit overrides retain their established precedence. Cursor's native
SQLite discovery uses the OS configuration directory and accepts
`CURSOR_NIGHTLY_STATE_DB`, with `CURSOR_STATE_DB` as its legacy fallback.

[Helper execution and cancellation](helper-processes.md) documents the native
process runner, Windows shim handling, and the remaining lifecycle checks.
[Private state](private-state.md) documents access restrictions, atomic writes,
concurrent key creation, and temporary credential handling.
