# Native foundation validation

The implementation branch passed the complete native foundation gate at
[`a01c9fc`](https://github.com/hashimkarim/usagestat/commit/a01c9fc) in
[run 34069426577](https://github.com/hashimkarim/usagestat/actions/runs/34069426577).
Both binaries build and execute on Linux x64/ARM64, macOS Intel/Apple Silicon,
and Windows x64 MSVC. This completes the build/runtime CI foundation in #3;
native services, credentials, distribution, and minimum OS versions have their
own remaining qualification requirements.

| Target | Native runner | Gate result |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | Ubuntu 24.04 x64 | Passed |
| `aarch64-unknown-linux-gnu` | Ubuntu 24.04 ARM64 | Passed |
| `aarch64-apple-darwin` | macOS 15 ARM64 | Passed |
| `x86_64-apple-darwin` | macOS 15 Intel | Passed |
| `x86_64-pc-windows-msvc` | Windows Server 2025 x64 | Passed; repeated with restored cache |

Each job uploads `report.json` and command logs containing the executing Rust
host, OS, runner image, Python/Node versions, native dependency versions, exact
test commands, exit codes, and runtime results. Native execution is required by
the gate; cross-compilation is not counted as runtime evidence.

The suite includes Rust tests, dashboard JavaScript tests, provider inventory,
both installed executables outside the checkout, absolute icon discovery,
QuickJS, bundled SQLite, local host HTTP/filesystem operations, isolated writable
state, daemon polling/health/JSON, helper timeouts, and spinning-probe cancellation.
Unix jobs also check SIGINT/SIGTERM. Windows executes native argument, npm shim,
batch shim, cancellation, and descendant cleanup fixtures. Console event coverage
is tracked separately in #5.

The first run, [34069032354](https://github.com/hashimkarim/usagestat/actions/runs/34069032354),
passed every Rust test on Windows but stopped when Python printed a Unicode Node
test glyph using its legacy console encoding. The harness now uses UTF-8 and
records each command's outcome before printing its log. Linux and macOS passed
that initial run and the subsequent run. Windows was rerun after its first
successful run to exercise the restored native build cache.

These fixtures use no provider credentials. The local HTTP fixture initializes
the production reqwest/rustls client but does not prove an external TLS handshake,
corporate proxy behavior, browser import, real provider authentication, or a
minimum OS floor. Record those separately during their owning qualification
issues instead of inferring support from this gate.
