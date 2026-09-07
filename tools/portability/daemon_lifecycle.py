#!/usr/bin/env python3
"""Native, account-free saved-settings, status, ownership, and shutdown checks."""
from __future__ import annotations

import argparse
import http.server
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.request

from native_smoke import isolated_env, run

ROOT = Path(__file__).resolve().parents[2]


def check(bin_dir: Path) -> dict:
    suffix = ".exe" if os.name == "nt" else ""
    cli, daemon = [bin_dir / (name + suffix) for name in ["usagestat", "usagestatd"]]
    result = {"checks": []}
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with tempfile.TemporaryDirectory(prefix="usagestat lifecycle 使用 ") as directory:
        root = Path(directory)
        env = isolated_env(root)
        config_root = Path(env["USAGESTAT_CONFIG_DIR"])
        data_root = Path(env["USAGESTAT_DATA_DIR"])
        config = config_root / "config.toml"
        config.write_text("providers = []\n", encoding="utf-8")
        # No login service is installed or changed. All per-user bus variables
        # are removed by isolated_env, so these commands only persist intent.
        mode = json.loads(run(cli, ["--json", "daemon", "t3", "auto"], root, env))
        assert mode["t3Mode"] == "auto" and not mode["configured"]
        key_root = data_root if os.name == "nt" else config_root
        key = key_root / "t3-management-key"
        retained = key.read_text().strip()
        assert json.loads(run(cli, ["--json", "daemon", "key"], root, env))["managementKey"] == retained
        mode = json.loads(run(cli, ["--json", "daemon", "t3", "off"], root, env))
        assert mode["t3Mode"] == "off" and key.read_text().strip() == retained
        result["checks"].append("settings-and-key-without-login-manager")

        with socket.socket() as held:
            held.bind(("127.0.0.1", 0))
            bind = f"127.0.0.1:{held.getsockname()[1]}"
        base = "http://" + bind
        control = key_root / "daemon-control-key"
        control.write_text("synthetic-lifecycle-control", encoding="utf-8")
        saved = config_root / "daemon.json"
        installation = {"owner": str(root), "binary": str(daemon), "bind": bind,
                        "config": str(config), "pluginDirs": [],
                        "environment": {name: env[name] for name in ["USAGESTAT_CONFIG_DIR", "USAGESTAT_DATA_DIR", "HOME"]},
                        "managementKeyFile": str(key), "controlKeyFile": str(control)}
        saved.write_text(json.dumps({"t3Mode": "auto", "installation": installation}), encoding="utf-8")

        def request(path, token=None, method="GET"):
            headers = {"Authorization": "Bearer " + token} if token else {}
            with opener.open(urllib.request.Request(base + path, headers=headers, method=method), timeout=2) as response:
                return json.load(response)

        def launch():
            child = subprocess.Popen([str(daemon), "--service-settings", str(saved)], cwd=root, env=env,
                                     stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                if child.poll() is not None:
                    raise AssertionError(("daemon failed before health", child.communicate()))
                try:
                    body = request("/health")
                    if body.get("pid") == child.pid:
                        return child, body
                except (OSError, urllib.error.URLError):
                    pass
                time.sleep(0.02)
            child.kill()
            child.communicate(timeout=5)
            raise TimeoutError("managed daemon readiness")

        child, health = launch()
        try:
            capabilities = request("/v1/capabilities")
            assert capabilities["schemaVersion"] == 1 and capabilities["backendVersion"] == health["version"]
            assert isinstance(request("/v1/providers"), list)
            assert capabilities["features"]["credentials.genericPassword"]["runtime"] == "not-checked"
            result["checks"].append("additive-capabilities-preserve-health-and-provider-array")
            assert health["application"] == "usagestat" and health["owner"] == str(root)
            assert health["status"] == "ok" and health["version"]
            with socket.create_connection(("127.0.0.1", int(bind.rsplit(":", 1)[1])), timeout=2) as stream:
                for fragment in [b"GET /health HTTP/1.1\r\n", b"Host: localhost\r\n", b"Connection: close\r\n\r\n"]:
                    stream.sendall(fragment)
                    time.sleep(0.03)
                received = bytearray()
                while chunk := stream.recv(4096):
                    received.extend(chunk)
                headers, body = received.split(b"\r\n\r\n", 1)
                assert headers.startswith(b"HTTP/1.1 200") and json.loads(body)["pid"] == child.pid
            result["checks"].append("fragmented-native-http-headers")
            status = json.loads(run(cli, ["--json", "daemon", "status"], root, env))
            assert status["configured"] and status["healthy"] and status["condition"] == "healthy", status
            assert status["backendVersion"] == health["version"]
            dashboard = json.loads(run(cli, ["--json", "dashboard", "--url"], root, env))
            assert dashboard["dashboardUrl"] == base + "/dashboard", dashboard
            result["checks"].append("saved-endpoint-owner-version-and-dashboard")
            duplicate = subprocess.run([str(daemon), "--bind", "127.0.0.1:0"], cwd=root, env=env,
                                       capture_output=True, text=True, timeout=5)
            assert duplicate.returncode != 0 and "profile lock" in duplicate.stderr, duplicate.stderr
            other_env = dict(env, USAGESTAT_DATA_DIR=str(root / "other data"))
            conflict = subprocess.run([str(daemon), "--bind", bind], cwd=root, env=other_env,
                                      capture_output=True, text=True, timeout=5)
            assert conflict.returncode != 0 and "bind daemon" in conflict.stderr, conflict.stderr
            assert request("/health")["pid"] == child.pid
            result["checks"].append("duplicate-and-port-conflict-preserve-running-owner")
            for token in [None, retained, "invalid"]:
                try:
                    request("/v1/daemon/shutdown", token, "POST")
                    raise AssertionError("shutdown accepted the wrong credential")
                except urllib.error.HTTPError as error:
                    assert error.code == 401, error.code
            assert "accounts" in request("/v0/management/quota-scheduler/status", retained)
            result["checks"].append("t3-and-control-keys-are-separate")
            assert request("/v1/daemon/shutdown", "synthetic-lifecycle-control", "POST")["status"] == "stopping"
            child.communicate(timeout=5)
            assert child.returncode == 0
            result["checks"].append("authenticated-graceful-shutdown")
        finally:
            if child.poll() is None:
                child.kill()
                child.communicate(timeout=5)
        child, _ = launch()
        child.kill()
        child.communicate(timeout=5)
        child, _ = launch()
        request("/v1/daemon/shutdown", "synthetic-lifecycle-control", "POST")
        child.communicate(timeout=5)
        result["checks"].append("profile-lock-recovers-after-exit-and-termination")

        class Fixture(http.server.BaseHTTPRequestHandler):
            body = {}
            def do_GET(self):
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
            saved.write_text(json.dumps({"t3Mode": "off", "installation": installation}), encoding="utf-8")
            for body, expected in [({"application": "usagestat", "status": "ok", "version": "0.0.0", "owner": str(root)}, "wrong-version"),
                                   ({"application": "other", "status": "ok"}, "port-conflict")]:
                Fixture.body = body
                status = json.loads(run(cli, ["--json", "daemon", "status"], root, env))
                assert status["condition"] == expected, status
            result["checks"].append("wrong-version-and-unrelated-endpoint-status")
        finally:
            server.shutdown()
            server.server_close()
            worker.join(timeout=5)
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, default=ROOT / "target/debug")
    args = parser.parse_args()
    print(json.dumps(check(args.bin_dir.resolve()), indent=2))
