# macOS per-user daemon

Implementation and qualification: [#8](https://github.com/hashimkarim/usagestat/issues/8).
The isolated native lifecycle suite passes on macOS 15 Apple Silicon and Intel
([run 34075010608](https://github.com/hashimkarim/usagestat/actions/runs/34075010608),
2026-09-07). Session qualification below remains pending. This describes the
feature branch, not a published macOS package.

The macOS adapter installs a user LaunchAgent at
`~/Library/LaunchAgents/com.usagestat.daemon.plist`. Dev builds use
`com.usagestat.daemon.dev`. Enable starts it in the current user's `gui/<uid>`
domain and enables startup for subsequent GUI logins. It requires neither root
nor a system LaunchDaemon. Foreground `usagestatd` remains available when no GUI
login domain is present.

The plist supplies an absolute backend and saved-settings path, private log files,
a working directory, a five-second exit/restart throttle, and restart after
unsuccessful exit. The backend reads its explicit config, plugin and helper
environment from saved settings before provider polling. Startup does not wait
for providers or Keychain authentication to succeed.

Disable stops only the owned registered job and disables its login startup while
retaining its plist, settings, data and keys. T3 changes use the shared lifecycle
semantics. Repeated enable/disable is supported. Enable reloads the agent so a
moved/upgraded absolute binary path takes effect. An existing unmarked plist or a
loaded job whose arguments differ from the owned plist is preserved and reported.
The running job's dictionary is read through ServiceManagement; control uses
`launchctl bootstrap`, `bootout`, `enable`, `disable`, and `kickstart`.

The native matrix includes an isolated LaunchAgent test on both Mac architectures:
enable twice, disable twice, crash restart, stopped/running T3 changes, retained
keys, moved binaries, and unmanaged plist preservation. The test uses its own
label, paths and synthetic state; no real providers or credentials are probed.
Login/logout, minimum supported macOS versions, and disposable Keychain
prompt/locked-store behavior still require explicit session qualification.

Apple documents the per-user agent model in
[Creating Launch Daemons and Agents](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html).
The compatibility job-dictionary API is documented as
[SMJobCopyDictionary](https://developer.apple.com/documentation/servicemanagement/smjobcopydictionary(_:_:));
it is deprecated, so continued native execution coverage is required while the
support floor includes macOS 11.
