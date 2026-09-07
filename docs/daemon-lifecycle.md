# Shared daemon lifecycle

Tracking issue: [#7](https://github.com/hashimkarim/usagestat/issues/7).

`daemon.json` records T3 intent and, after service setup, the installation owner,
resolved binary, bind address, config, plugin paths, explicit environment, and
key file paths. Native service adapters launch `usagestatd --service-settings`
with that absolute file. The daemon reads it before starting workers. Only a
small named environment set is captured from the calling process; a full process
environment, which may contain provider credentials, is never copied into it.

`daemon key`, uninstalled-profile T3 settings, status, and dashboard URL resolution
work without a login service manager. T3 auto creates a key once; off retains it.
Changing a registered service's T3 mode restarts a running service and leaves a
stopped service stopped. Enable starts immediately and enables login startup;
disable stops and disables startup while retaining settings, keys and history.

Status retains existing fields and adds `manager`, `managerAvailable`,
`configured`, `registered`, `healthy`, `condition`, `owner`, `backendVersion`,
and `diagnostics`. Conditions distinguish an unavailable manager, unregistered or
stopped service, startup, healthy owned daemon, external daemon, wrong version,
and an unrelated process occupying the endpoint. `/health` retains `status: ok`
and adds application/version, PID, profile and installation owner. Readiness for
an enabled service requires the expected owner and backend version.

One kernel-held lock protects each data profile from duplicate daemon processes,
even at different ports. It releases on normal exit or process termination.
Mutating CLI commands take a separate settings lock. A bind conflict fails before
provider polling starts. Service setup refuses to replace a different recorded
installation unless `--switch-owner` is supplied. It rejects temporary npm
execution paths as autostart owners; Homebrew's package directory remains its
owner across versioned Cellar upgrades. Failed readiness returns an error and
leaves the service visible through status for diagnosis or disable/recovery.

Managed daemons expose `POST /v1/daemon/shutdown`, authenticated by a separate
private control key. T3 keys cannot stop the daemon; foreground processes without
control configuration return 404 for this route. Shutdown stops accepting new
connections and propagates cancellation into active providers/helpers, with a
three-second worker cleanup budget. In-flight HTTP provider requests retain
their transport deadlines. Native service adapters use their own registered
process identity; an unrelated process is never selected for termination by port.

## Linux migration and validation

The Linux adapter owns systemd paths, quoting and commands. It discovers the live
manager's `UnitPath` through `busctl` JSON, because a GUI application's overridden
XDG config directory may differ from the login manager's directory. Without an
available manager it can still report saved settings and diagnostics. Existing
managed units migrate binary/bind/config/plugin/environment/key choices; custom
unit lines remain intact when ExecStart switches to the saved settings file.
Unmanaged units, ambiguous commands, or dynamic legacy expansions are preserved
and reported for explicit migration.

Local validation includes the full native gate, fake service transitions,
quoted-path migration, unauthenticated/wrong-key shutdown rejection, duplicated
profile/port checks, status against wrong-version/unrelated endpoints, and lock
recovery after termination. An opt-in live systemd test passed enable/disable,
stopped/running T3 changes, authenticated bridge availability and key retention
using a unique temporary service. It cleans up only that service and retains no
test autostart registration. Run it with:

```sh
USAGESTAT_TEST_DAEMON_BINARY=/absolute/path/usagestatd cargo test --locked -p usagestat-cli isolated_real_user_service_lifecycle -- --ignored
```

The native gate also runs `tools/portability/daemon_lifecycle.py` with isolated
profiles and no login-manager connection. macOS and Windows native login service
adapters remain #8 and #9. Login/logout, reboot, Keychain/credential prompts and
minimum-OS qualification remain separate acceptance checks.

The complete gate passed on Linux x64/ARM64, macOS Intel/Apple Silicon and Windows
x64 MSVC at [`fb8efab`](https://github.com/hashimkarim/usagestat/commit/fb8efab), in
[run 34072863814](https://github.com/hashimkarim/usagestat/actions/runs/34072863814).
This includes all eight native lifecycle checks. Accepted sockets explicitly
restore blocking mode for bounded request reads; on Windows they otherwise
inherit the nonblocking listener mode. A delayed-header fixture covers that
timing difference. Authenticated shutdown queues its HTTP response before
signalling the main loop to exit.
