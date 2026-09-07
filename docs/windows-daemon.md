# Windows per-user daemon

Implementation and qualification: [#9](https://github.com/hashimkarim/usagestat/issues/9).
The isolated scheduled-task, crash-supervision and process-tree tests pass on
Windows Server 2025 x64 ([run 34075747116](https://github.com/hashimkarim/usagestat/actions/runs/34075747116),
2026-09-07). Standard-user session qualification below remains pending; this
describes feature-branch code.

`usagestat daemon enable` registers a current-user Task Scheduler task named
`usagestat-<SID>` (`usagestat-dev-<SID>` for dev builds). Its logon trigger and
interactive-token principal use the current user's SID with the least-privilege
run level. No password, administrator permission, or SYSTEM identity is requested.
Foreground `usagestatd.exe` remains available independently of Task Scheduler.

The task uses absolute executable, saved-settings, and working-directory paths.
Task Scheduler expands percent-delimited environment variables in action fields;
paths containing percent signs are therefore rejected with a directory-selection
instruction. Unicode, spaces, and ampersands are passed directly without a shell.
The backend reads its saved config, plugins and helper environment before polling.
Task definitions contain file references rather than credentials.

Windows packages must include `usagestat-service.exe` beside `usagestatd.exe` and
`usagestat.exe`. This internal GUI-subsystem launcher starts the backend without
a console and assigns it atomically to a Windows Job Object. If Task Scheduler
forces the launcher to stop, closing that job also terminates the backend and
its helper descendants. Normal disable first uses the private authenticated
shutdown endpoint. Logs are private files under the saved local data directory.
Dev packages use `usagestat-service-dev.exe` beside `usagestatd-dev.exe`.

The launcher restarts an unsuccessfully exited backend after five seconds; a
successful authenticated shutdown exits normally. Tasks have no execution time
limit, allow battery operation and ignore duplicate starts. Task Scheduler also
has a one-minute failure retry policy (up to 999 attempts), but backend crash
recovery does not depend on that policy: a demand-started task did not recover
within 90 seconds in the initial native test.
Disable stops the owned task and retains its disabled registration, settings,
data and keys. T3 changes retain autostart intent and do not start a stopped
daemon. Re-enable updates moved/upgraded executable paths. Native COM properties
provide status and validate task ownership; localized command output is unused.
An existing task with a different marker, principal, profile, or action is preserved.

The native gate exercises console-free startup, private output, crash supervision,
forced launcher tree cleanup, repeated task enable/disable, actual crash restart,
T3 changes, moved paths and unmanaged task preservation. Fixtures own unique task
names and synthetic profiles. A standard-user login/reboot, minimum supported
Windows version, credential access, dev/release coexistence and installation
upgrade qualification still require explicit native-session coverage.

Microsoft documents the [interactive logon task model](https://learn.microsoft.com/en-us/windows/win32/taskschd/logon-trigger-example--c---)
and [action path/environment handling](https://learn.microsoft.com/en-us/windows/win32/api/taskschd/nn-taskschd-iexecaction).
Native archive, Windows installer and npm packaging work must include the
Windows-only launcher; it is not a separate user-facing install command.
