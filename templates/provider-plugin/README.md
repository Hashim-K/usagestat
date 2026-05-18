# UsageStat Provider Plugin Template

Copy this directory to one of the plugin directories and rename the provider id:

```bash
cp -a templates/provider-plugin ~/.local/share/usagestat/plugins/my-provider
```

For dev builds:

```bash
cp -a templates/provider-plugin ~/.local/share/usagestat-dev/plugins/my-provider
```

Then edit:

- `plugin.json`: provider id, display name, supported modes, web URL.
- `plugin.js`: provider-specific auth, parsing, and metric mapping.

Test with:

```bash
usagestat-dev --all --plugin-dir ~/.local/share/usagestat-dev/plugins list
usagestat-dev --all --plugin-dir ~/.local/share/usagestat-dev/plugins usage --provider my-provider --source api --json
```

The template is `enabledByDefault: false`, so use `--all` until the provider is
enabled in config or the manifest default is changed.

## Source Modes

UsageStat passes the requested source mode as `ctx.sourceMode`.

- `api`: API key or token from an allowlisted environment variable.
- `oauth`: OAuth access/refresh token from a local file or provider-specific state.
- `local`: local app database/config/log file read from disk.
- `cli`: result from a supported local CLI integration. The current backend only allows `gh`.
- `web`: browser/web session data, usually cookies imported by the extension or user.

Return metrics as:

- `ctx.line.progress({ label, used, limit, format, resetsAt })`
- `ctx.line.text({ label, value })`
- `ctx.line.badge({ label, text, color })`

The template includes one example implementation for each source mode.
