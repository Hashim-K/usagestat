#!/usr/bin/env python3
"""Read-only installed diagnostics and synthetic provider-state contract checks."""
from __future__ import annotations
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import platform
import shutil
import socket
import tempfile
import threading
from native_smoke import isolated_env, run

ROOT = Path(__file__).resolve().parents[2]

def check(bin_dir: Path, temp_dir: Path | None = None) -> dict:
    result = {"checks": []}
    suffix = ".exe" if os.name == "nt" else ""
    with tempfile.TemporaryDirectory(prefix="usagestat diagnostics 使用 ", dir=temp_dir) as directory:
        root = Path(directory)
        env = isolated_env(root)
        # A standalone installed CLI must diagnose missing sibling resources.
        cli = root / ("usagestat" + suffix)
        shutil.copy2(bin_dir / cli.name, cli)
        env["PATH"] = str(root / "missing-helpers")
        config_root, data_root = Path(env["USAGESTAT_CONFIG_DIR"]), Path(env["USAGESTAT_DATA_DIR"])
        config = config_root / "config.toml"
        with socket.socket() as reserved:
            reserved.bind(("127.0.0.1", 0))
            bind = f"127.0.0.1:{reserved.getsockname()[1]}"
        settings = config_root / "daemon.json"
        installation = {"owner": str(root), "binary": str(root / ("usagestatd" + suffix)), "bind": bind,
            "config": str(config), "pluginDirs": [], "environment": {},
            "managementKeyFile": str(config_root / "key"), "controlKeyFile": str(config_root / "control")}
        settings.write_text(json.dumps({"t3Mode": "auto", "installation": installation}), encoding="utf-8")
        sentinel = "SYNTHETIC-CREDENTIAL-NEVER-PRINT-71ae8"
        for name in ["key", "control"]:
            (config_root / name).write_text(sentinel, encoding="utf-8")

        def tree():
            return {str(path.relative_to(root)): (path.stat().st_mtime_ns,
                hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None)
                for path in root.rglob("*")}

        def doctor():
            before = tree()
            output = run(cli, ["doctor", "--json"], root, env)
            assert sentinel not in output
            assert tree() == before, "doctor modified the isolated profile"
            report = json.loads(output)
            assert report["schemaVersion"] == 1 and report["readOnly"]
            assert report["service"].get("t3") in (None, "not-checked")
            assert report["capabilities"]["features"]["credentials.genericPassword"]["runtime"] == "not-checked"
            return report, {item["id"]: item["code"] for item in report["checks"]}

        report, codes = doctor()
        assert codes["config"] == "config-missing", codes
        assert codes["resources"] == "resources-missing" and codes["backend"] == "binary-missing", codes
        assert all(value == "not-found" for value in report["capabilities"]["helpers"].values())
        assert report["service"]["code"] in ("service-stopped", "service-manager-unavailable")
        if os.name == "nt" or platform.system() == "Darwin":
            assert "systemctl" not in run(cli, ["doctor"], root, env)
            assert codes["browser.automaticImport"] == "not-checked"
        result["checks"].append("missing-config-binary-resources-helpers-read-only")

        config.write_text('providers = "' + sentinel + '"', encoding="utf-8")
        assert doctor()[1]["config"] == "config-invalid"
        config.write_text("providers = []\n", encoding="utf-8")
        original = settings.read_text(encoding="utf-8")
        settings.write_text('{"t3Mode": "' + sentinel + '"}', encoding="utf-8")
        assert doctor()[1]["service"] == "service-settings-unavailable"
        settings.write_text(original, encoding="utf-8")
        result["checks"].append("invalid-config-and-settings-omit-values")

        snapshots = {state: {"providerId": state, "displayName": sentinel, "metrics": [
            {"type": "badge", "label": "Error", "text": sentinel}], "fetchedAt": "2026-01-01T00:00:00Z",
            **({"state": state} if state != "unknown" else {})}
            for state in ["credential-denied", "credential-unavailable", "missing-auth", "no-data", "unknown"]}
        (data_root / "snapshots.json").write_text(json.dumps({"snapshots": snapshots}), encoding="utf-8")
        assert doctor()[0]["cachedProviderStates"] == dict.fromkeys(snapshots, 1)
        result["checks"].append("cached-denied-locked-missing-auth-no-data-without-store-access")

        class Fixture(http.server.BaseHTTPRequestHandler):
            body = {}
            def do_GET(self):
                assert self.path == "/health", "doctor attempted a credentialed endpoint"
                payload = json.dumps(self.body).encode()
                self.send_response(200)
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
            def log_message(self, *_args):
                pass
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Fixture)
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            installation["bind"] = f"127.0.0.1:{server.server_port}"
            settings.write_text(json.dumps({"t3Mode": "auto", "installation": installation}), encoding="utf-8")
            for body, code in [
                ({"application": "usagestat", "status": "ok", "version": sentinel, "owner": str(root)}, "wrong-version"),
                ({"application": "other", "status": "ok"}, "unhealthy"),
            ]:
                Fixture.body = body
                assert doctor()[0]["service"]["code"] == code
        finally:
            server.shutdown()
            server.server_close()
            worker.join(timeout=5)
        result["checks"].append("wrong-version-and-occupied-port-without-key-requests")

        plugin = root / "plugins" / "diagnostic-fixture"
        plugin.mkdir(parents=True)
        manifest = {"id": "diagnostic-fixture", "name": "Fixture", "entry": "plugin.js",
                    "enabledByDefault": True, "supportedModes": ["local"], "autoMode": "local"}
        (plugin / "plugin.json").write_text(json.dumps(manifest), encoding="utf-8")
        cases = [
            ('return {metrics: []}', "no-data"),
            ('return {metrics: [{type: "text", label: "Fixture", value: "ok"}]}', "ready"),
            ('throw {code: "missing-auth", message: "Sign in"}', "missing-auth"),
            ('throw {code: "credential-denied", message: "Denied"}', "credential-denied"),
            ('throw {code: "credential-unavailable", message: "Locked"}', "credential-unavailable"),
            ('throw "unsupported: unavailable source"', "unsupported"),
            ('throw "unknown error"', "failed"),
        ]
        for script, state in cases:
            (plugin / "plugin.js").write_text('globalThis.__usagestat_plugin = {probe: function() {' + script + '}}', encoding="utf-8")
            snapshot = json.loads(run(cli, ["usage", "diagnostic-fixture", "--json"], root, env))[0]
            assert snapshot["state"] == state, snapshot
            assert "metrics" in snapshot and "providerId" in snapshot
        snapshot = json.loads(run(cli, ["usage", "diagnostic-fixture", "--source", "web", "--json"], root, env))[0]
        assert snapshot["state"] == "unsupported", snapshot
        caps = json.loads(run(cli, ["capabilities", "--json"], root, env))
        assert caps["schemaVersion"] == 1 and caps["providers"][0]["authentication"] == "not-checked"
        assert caps["providers"][0]["qualification"] == "unverified"
        result["checks"].append("synthetic-plugin-states-and-additive-cli-capabilities")
    return result

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, default=ROOT / "target/debug")
    parser.add_argument("--temp-dir", type=Path)
    args = parser.parse_args()
    print(json.dumps(check(args.bin_dir.resolve(), args.temp_dir), indent=2))
