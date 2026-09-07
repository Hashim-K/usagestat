# Native Codex authentication

Implementation for #10/#11, based on the upstream storage sources checked on
2026-09-07. Credentialed app/version qualification is still pending.

The provider now delegates auth selection to the native host. It resolves one
`CODEX_HOME` (default native home + `.codex`) and reads that profile's
`config.toml`. `cli_auth_credentials_store` selects `file`, `keyring`, `auto` or
`ephemeral`. File mode reads only `auth.json`; ephemeral mode reports unsupported.
Auto mode falls back to the same profile's file only for missing credentials.
Denied, inaccessible, malformed and mismatched credentials remain distinct
errors instead of selecting a stale fallback.

`features.secret_auth_storage` selects the encrypted keyring backend. Its default
is enabled on Windows and disabled on macOS/Linux. To select a legacy direct
keyring explicitly, use provider `settings.authStorage = "keyring"`; other
supported overrides are `file`, `encrypted` and `auto` (follow the profile).
Advanced upstream config layers or a CLI-selected configuration profile may
require this override; the host reads the selected root's config, not managed
organization policy or a running Codex process's private configuration.

Both keyring mappings hash the exact Rust-canonicalized native home path with
SHA-256, using the first 16 hexadecimal characters. The direct store uses
service `Codex Auth` and account `cli|<hash>`. On Windows the exact target is
`cli|<hash>.Codex Auth`, with an account check and UTF-16LE JSON payload.

The newer encrypted store reads `secrets/codex_auth.age`, service `codex`, and
account `secrets|<hash>` (Windows target `secrets|<hash>.codex`, UTF-16LE passphrase).
It uses the upstream age format and extracts only `global/CODEX_AUTH` from schema
version 0 or 1. It never generates a missing master key, migrates credentials or
reads other secret namespaces. Input/plaintext size is limited to 2 MiB, only a
single scrypt recipient is accepted, and the maximum scrypt work factor is 20.
An excessive work factor returns an explicit unsupported state.

Encrypted auth is read-only. The provider can use a current token or reload a
token refreshed by Codex. When that is insufficient it asks the user to sign in
through Codex; it does not rotate a token it cannot persist back to the encrypted
store. File/direct-keyring refreshes check the original profile, store, content
revision and account before updating. Windows preserves credential metadata.
These checks detect intervening updates but cannot provide an atomic
compare-and-swap against another application's credential-store write.

Pure fixtures cover store selection, exact account derivation, fallback rules,
age authentication/tampering, newer schema rejection and missing master keys.
The Windows native fixture creates disposable direct/master credential entries,
reads both formats, checks accounts and removes its own entries. Node fixtures
exercise actual provider selection, error states and refresh behavior on all
three OS conventions. No real provider credentials are used in these tests.

Sources: [Codex auth storage](https://github.com/openai/codex/blob/main/codex-rs/core/src/auth/storage.rs),
[encrypted auth backend](https://github.com/openai/codex/blob/main/codex-rs/secrets/src/local.rs),
[master-key account mapping](https://github.com/openai/codex/blob/main/codex-rs/secrets/src/lib.rs),
[keyring adapter](https://github.com/openai/codex/blob/main/codex-rs/keyring-store/src/lib.rs),
[Windows keyring mapping](https://docs.rs/keyring/3.6.3/keyring/windows/index.html),
[age scrypt limits](https://docs.rs/age/0.12.1/age/scrypt/struct.Identity.html).
