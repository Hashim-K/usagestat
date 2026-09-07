#!/usr/bin/env python3
"""Exercise native Codex auth selection and guarded file refresh with dummy data."""
import argparse
import json
from pathlib import Path
import tempfile
from native_smoke import isolated_env, run

ROOT = Path(__file__).resolve().parents[2]


def check(cli: Path) -> dict:
    with tempfile.TemporaryDirectory(prefix="usagestat Codex auth 使用 ") as directory:
        root = Path(directory)
        env = isolated_env(root)
        profile = root / "Codex profile & (使用)"
        profile.mkdir()
        env["CODEX_HOME"] = str(profile)
        auth = profile / "auth.json"
        auth.write_text(json.dumps({"tokens": {"access_token": "synthetic-original", "account_id": "fixture-account"}}), encoding="utf-8")
        plugin = root / "plugins/codex-auth-fixture"
        plugin.mkdir(parents=True)
        (plugin / "plugin.json").write_text(json.dumps({"id": "codex-auth-fixture", "name": "Auth fixture", "entry": "plugin.js", "enabledByDefault": True}), encoding="utf-8")
        (plugin / "plugin.js").write_text('''globalThis.__usagestat_plugin = {probe: function(ctx) {
            var state = JSON.parse(ctx.host.codex.readAuth('auto'));
            if (state.storage !== 'file' || state.auth.tokens.access_token !== 'synthetic-original') throw 'Wrong profile';
            state.auth.tokens.access_token = 'synthetic-updated';
            ctx.host.codex.writeAuth('auto', state.profileKey, state.revision, state.storage, JSON.stringify(state.auth));
            var current = JSON.parse(ctx.host.codex.readAuth('auto'));
            if (current.auth.tokens.access_token !== 'synthetic-updated') throw 'Update missing';
            var rejected = 0;
            try { ctx.host.codex.writeAuth('auto', state.profileKey, state.revision, state.storage, JSON.stringify(state.auth)); }
            catch (_) { rejected++; }
            current.auth.tokens.account_id = 'another-account';
            try { ctx.host.codex.writeAuth('auto', current.profileKey, current.revision, current.storage, JSON.stringify(current.auth)); }
            catch (_) { rejected++; }
            try { ctx.host.codex.writeAuth('auto', 'another-profile', current.revision, current.storage, JSON.stringify(state.auth)); }
            catch (_) { rejected++; }
            if (rejected !== 3) throw 'Conflicting auth update accepted';
            return {metrics:[{type:'text',label:'Auth',value:'guarded'}]};
        }};''', encoding="utf-8")
        output = json.loads(run(cli, ["usage", "codex-auth-fixture", "--json"], root, env))
        assert output[0]["metrics"][0].get("value") == "guarded", output
        saved = json.loads(auth.read_text(encoding="utf-8"))
        assert saved["tokens"] == {"access_token": "synthetic-updated", "account_id": "fixture-account"}

        (plugin / "plugin.js").write_text("globalThis.__usagestat_plugin = {probe: function(ctx) { ctx.host.codex.readAuth('auto'); throw 'Unexpected success'; }};", encoding="utf-8")
        def expect_state(state):
            output = json.loads(run(cli, ["usage", "codex-auth-fixture", "--json"], root, env))
            assert output[0]["state"] == state, output
            assert "synthetic-updated" not in json.dumps(output)
        (profile / "config.toml").write_text("cli_auth_credentials_store = 'ephemeral'\n", encoding="utf-8")
        expect_state("unsupported")
        (profile / "config.toml").unlink()
        auth.write_text("{}" + " " * (2 * 1024 * 1024), encoding="utf-8")
        expect_state("credential-malformed")
        auth.unlink()
        expect_state("missing-auth")
        assert not auth.exists() and not (profile / "secrets").exists()
    return {"checks": ["native-selected-profile-and-file-refresh", "revision-account-profile-conflicts-rejected",
                       "ephemeral-size-and-missing-auth-states", "no-credential-generation-or-secret-output"]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, default=ROOT / "target/debug/usagestat")
    args = parser.parse_args()
    print(json.dumps(check(args.cli.resolve()), indent=2))
