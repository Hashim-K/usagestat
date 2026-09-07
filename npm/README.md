# usagestat native backend through npm

This distribution is under development. `@hashimkarim/usagestat` is the selected
package name; it has not been published. These commands describe the intended
installation after qualification and first publication:

```sh
npm install --global @hashimkarim/usagestat --include=optional --ignore-scripts
usagestat --version
usagestat doctor
usagestat --json list
```

Use Node.js 24 or newer and npm 11.5.1 or newer. Exact-version optional platform
packages contain the Rust CLI, daemon, plugins, icons and license notices.
Installation needs no Rust compiler, shell scripts or download hooks. It does
not start a daemon or install a login service. Native packages also remain
available separately for users who do not need Node.

Candidate npm targets are Linux x64/ARM64 with glibc 2.39+, macOS 13.5+ on Intel/Apple
Silicon, and Windows 10 / Server 2016+ x64. musl/Alpine, Windows ARM64 and 32-bit
systems are unsupported. Stable packages include only qualified targets;
macOS/Windows minimum-system qualification and signing are still pending.
The Rust macOS artifact targets 11.0; the npm wrapper's higher floor comes from
[Node.js 24's platform requirements](https://github.com/nodejs/node/blob/v24.x/BUILDING.md#platform-list).

To use a background daemon after a durable global installation:

```sh
usagestat daemon enable
usagestat daemon status
usagestat dashboard
```

The native CLI registers its actual sibling backend with the current user's
systemd, LaunchAgent or Task Scheduler. Login startup does not require Node or
an interactive shell. Configuration, history, T3 keys and credentials stay in
the usual per-user locations outside npm's package/cache directories. Existing
native/Homebrew/bar installations retain ownership until an explicit owner
switch. Use `daemon enable --switch-owner` only when intentionally transferring
this profile to the npm installation.

For an explicit update, first stop and disable the daemon. This releases Windows
executable locks. Install the desired version, then enable it again if it was
previously enabled:

```sh
usagestat daemon disable
npm install --global @hashimkarim/usagestat@VERSION --include=optional --ignore-scripts
usagestat daemon enable
usagestat doctor
```

Keep a stopped daemon stopped by omitting the final enable. T3 intent, keys and
user data survive this sequence. After an interrupted update, reinstall the
same exact version before enabling. When changing a Node version manager or npm
global prefix, disable the old installation first, install into the new durable
prefix, and explicitly transfer ownership to it.

Before removal:

```sh
usagestat daemon disable
npm uninstall --global @hashimkarim/usagestat --ignore-scripts
```

User data is retained. Lifecycle hooks are never required for cleanup. Do not
remove the files of a running Windows service. A one-off
`npm exec --package=@hashimkarim/usagestat -- usagestat --version` is suitable for
CLI use; persistent startup from the temporary `_npx`/`_cacache` path is rejected.
Project-local installations can own startup only while their directory remains
durable. Prefer a global installation for services.

`native-package-missing` means optional dependencies were omitted: reinstall with
`--include=optional`. Version/integrity failures require reinstalling the exact
main-package version. A package cannot remove native OS/glibc requirements.
On Windows, use npm's generated `.cmd` command shims from Command Prompt, or the
`.cmd` command from PowerShell when its script policy blocks the generated `.ps1`.
