# Windows backend installation and development

Tracker: [#16](https://github.com/hashimkarim/usagestat/issues/16).
Windows x64 artifacts are currently unsigned release candidates from the native
workflow. Windows minimum-version, standard-user desktop, active upgrade/rollback
and public distribution qualification remain pending. Windows ARM64 and 32-bit
builds are not included. npm is also a planned distribution channel; its native
installation rehearsal passed, but registry publication is still disabled.

The portable ZIP contains `usagestat.exe`, `usagestatd.exe`,
`usagestat-service.exe`, plugins/icons and license notices. Keep them together.
It runs without WSL, Bash, Node, Python, a Rust compiler or administrator rights.
The small GUI-subsystem service supervisor is needed for console-free scheduled
task startup. Portable ZIP installation does not change PATH or enable startup.

For a selected candidate downloaded with its matching `.sha256` sidecar into
the current directory, verify and extract using built-in Windows PowerShell:

```powershell
$archiveName = 'usagestat-windows-x86_64.zip'
$checksum = (Get-Content -Raw ($archiveName + '.sha256')).Trim() -split '\s+'
if ($checksum.Count -ne 2 -or $checksum[1] -cne $archiveName -or
    $checksum[0] -notmatch '^[0-9a-f]{64}$' -or
    (Get-FileHash $archiveName -Algorithm SHA256).Hash -ine $checksum[0]) {
    throw 'The archive and its checksum do not match.'
}
$installDirectory = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Programs\usagestat-preview'
if (Test-Path -LiteralPath $installDirectory) { throw 'Choose a fresh installation directory.' }
Expand-Archive -LiteralPath $archiveName -DestinationPath $installDirectory
& (Join-Path $installDirectory 'usagestat.exe') --version
& (Join-Path $installDirectory 'usagestat.exe') --json list
```

Use artifacts from the intended repository/run and retain the manifest/source
commit with the test result. A checksum detects modification; it does not supply
an Authenticode publisher identity. The current payload is unsigned, and public
installer trust/signing policy is not yet qualified. Do not label it signed based
on a successful native build or checksum comparison.

For foreground operation, run `usagestatd.exe` from that directory. To opt into
login startup, run its `usagestat.exe daemon enable`. The backend creates a
per-user interactive-token task at limited privileges and saves absolute paths;
it does not create a Windows system service, store a logon password or request
administrator rights. See `daemon status --json` and `doctor --json` for status.
The normal endpoint is `127.0.0.1:6736`; another profile needs a different port.

Before removing a managed current implementation, run `daemon unregister` from
the selected CLI. It stops/removes only that profile's owned task and leaves
config, history, key files and T3 mode intact. A retained installation owner can
be explicitly changed using `daemon enable --switch-owner`. An unrelated task or
independently started foreground process is not removed by unregister. Stop a
foreground daemon in its own terminal before deleting its binaries. The portable
instructions do not modify PATH; there is no PATH entry to remove.

Do not overwrite running `.exe` files. A complete Windows installer still needs
staging, owned-process stop, verified replacement, task-path update, health checks
and failure rollback. The current native lifecycle primitives and portable ZIP
are inputs to that work; manual extraction is not evidence for active upgrade
recovery. Native installer/upgrade qualification remains open in #16.

## Native development build

From a source checkout, install the Rust MSVC toolchain and its Visual Studio C++
build tools, plus Python 3.11 or newer. Rust 1.89.0 is the tested toolchain. The
normal user-facing native package does not require these development tools.

```powershell
python tools/portability/stage_dev.py --output target/native-dev-first
& .\target\native-dev-first\usagestat-dev.exe --json list
& .\target\native-dev-first\usagestat-dev.exe capabilities --json
& .\target\native-dev-first\usagestatd-dev.exe --bind 127.0.0.1:6737
```

The helper builds the entire native workspace, resolves Cargo's target directory,
verifies binary versions and stages `usagestat-dev.exe`, `usagestatd-dev.exe`,
`usagestat-service-dev.exe` and matching resources. Executable names select the
separate `usagestat-dev` config/data/service identity. It never registers a task,
changes PATH or writes a normal installed profile. Existing output directories
are refused; use a fresh directory for the next build so a running installation
is preserved. After stopping/unregistering a staged build, its directory can be
removed separately from retained development data.

The same Python command works on macOS/Linux and stages `usagestat-dev` and
`usagestatd-dev`. `--binary-dir PATH --target TRIPLE` accepts already-built native
binaries for CI; this is not a cross-compilation switch. The five-target native
gate checks executable names, dev profile identity, resource discovery from an
unrelated cwd, version consistency, existing-output protection and absence of
implicit login registration. It does not claim release-mode static-CRT packaging
for debug development binaries.

Remaining frontend integration is tracked in
[Windows bar #14](https://github.com/hashimkarim/usagestat-bar/issues/14).
The [bar contract](bar-integration.md) defines native executable/npm discovery,
capability handling and one-owner behavior. Record standard-user login, reboot,
file locks, redirected folders, dev/release coexistence and actual account results
in #20 before public qualification.
