# ADR 0001: One native Rust backend across desktop platforms

Status: accepted implementation direction; platform qualification pending.

The CLI, daemon, embedded JavaScript providers, SQLite readers, and HTTP client
already form a shared backend. Desktop support requires native paths, process
execution, credentials, services, and packaging around that implementation.

Keep `usagestat` and `usagestatd` in the existing Rust workspace. Preserve the
current CLI JSON and HTTP contracts consumed by the bar. Platform adapters own
service and credential behavior; portable domain and provider logic remains
shared. Native CI must execute the actual binaries and embedded runtime on each
supported architecture.

Use one installation owner per user/profile, explicit autostart, and a loopback
default. Desktop bundles carry a matching backend; standalone installs expose a
durable CLI that the bar can discover. Updates act on the recorded owner and
preserve configuration/history.

Include npm as a distribution channel with a small Node launcher and exact-version
optional native packages containing both binaries and resources. Installation
must work with npm lifecycle scripts disabled. Native desktop bundles and archives
remain usable without Node; temporary npm execution paths cannot own autostart.

These choices require target-specific native artifacts, validated plugin resource
layouts, explicit service ownership, and qualification of credential acquisition
methods. The detailed target floors, path precedence, client interfaces, release
gates, and outstanding decisions are in the [support contract](../platform-support.md).
