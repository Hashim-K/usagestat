# Native provider paths

Implementation and qualification work for #11. These changes preserve the
existing usage parsers, aggregation, reset calculations and pricing tables.

Claude's daemon collector reads `CLAUDE_CONFIG_DIR/projects` when configured.
Otherwise it uses the native home directory and `.claude/projects`; the Xcode
Claude history location is included only on macOS without an explicit profile.
An explicit profile excludes both defaults, including when it contains no data.

Codex's daemon collector reads `sessions` and `archived_sessions` in `CODEX_HOME`
or the native home's `.codex`. An explicit root must be an existing accessible
directory and is canonicalized, matching upstream resolution. A missing or
invalid explicit root returns `LOCAL_USAGE_PATH_UNAVAILABLE` from the local usage
endpoint; it does not fall back to another profile or a saved report.

Cursor stable and nightly use separate native application directories and
separate `CURSOR_STATE_DB` / `CURSOR_NIGHTLY_STATE_DB` overrides. Nightly does not
consume the stable override. An invalid explicit database does not fall back.
The shared IDE app-support resolver uses Windows roaming Known Folders, macOS
Application Support or Linux XDG config. It does not search copied trees from
another OS or the old Linux config root after XDG redirection.

Plugin SQLite reads open the real database read-only, with a 250 ms busy timeout
and connection-level query-only mode. They respect active WAL writers and
committed snapshots. The old immutable fallback is removed because its
assumptions do not hold for a running IDE. A locked or inaccessible database
reports a read failure; it is never created or opened writable.

`tools/portability/local_usage.py` runs synthetic histories through a disposable
daemon and checks the daily/weekly/monthly/session reports, account isolation,
Unicode paths, archived Codex history and malformed/empty lines. Windows runs
with no HOME environment variable. `provider_storage.py` queries a synthetic
database while a writer holds WAL and exclusive transactions, rejects mutation
and checks bounded lock recovery. Both fixtures run in the five-target native
CI gate. Real provider credentials, app versions and running-app qualification
remain separate release gates; no account is marked verified by these fixtures.

Resolution references checked 2026-09-07:
[Claude directory and override](https://code.claude.com/docs/en/claude-directory),
[Codex native home resolver](https://github.com/openai/codex/blob/main/codex-rs/utils/home-dir/src/lib.rs),
[SQLite immutable URI semantics](https://www.sqlite.org/uri.html#uriimmutable).
