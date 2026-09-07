#!/usr/bin/env python3
"""Exercise installed Rust binaries with isolated, credential-free fixtures.

No shell, provider credentials, service registration, or external requests are
used by default. --https-url is a separately reported, optional network check.
"""
from __future__ import annotations

import argparse
from contextlib import closing
import http.server
import json
import os
from pathlib import Path
import platform
import secrets
import shutil
import socket
import sqlite3
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.request

FIXTURES = Path(__file__).resolve().parent / "fixtures"


def isolated_env(root: Path) -> dict[str, str]:
    env = {key: value for key, value in os.environ.items()
           if not key.upper().startswith(("USAGESTAT_", "AI_USAGE_"))
           and not key.upper().endswith("_PROXY")}
    for name, directory in {
        "HOME": "home", "USERPROFILE": "home", "XDG_CONFIG_HOME": "xdg-config",
        "XDG_DATA_HOME": "xdg-data", "APPDATA": "roaming", "LOCALAPPDATA": "local",
        "USAGESTAT_CONFIG_DIR": "config 使用", "USAGESTAT_DATA_DIR": "data 使用",
    }.items():
        path = root / directory
        path.mkdir(parents=True, exist_ok=True)
        env[name] = str(path)
    # Windows Known Folder APIs do not follow synthetic APPDATA/HOME values;
    # the explicit app overrides above are what isolate backend state there.
    env["NO_PROXY"] = "*"
    env["RUST_BACKTRACE"] = "1"
    env.pop("USAGESTAT_MANAGEMENT_KEY", None)
    return env


def run(binary: Path, args: list[str], cwd: Path, env: dict[str, str]) -> str:
    result = subprocess.run([str(binary), *args], cwd=cwd, env=env, text=True,
                            encoding="utf-8", errors="replace", capture_output=True, timeout=60)
    if result.returncode:
        raise RuntimeError(f"{binary.name} {args}: exit {result.returncode}\n{result.stdout}\n{result.stderr}")
    return result.stdout


def check_snapshot(snapshots: list[dict], nonce: str) -> None:
    assert len(snapshots) == 1, snapshots
    snapshot = snapshots[0]
    assert snapshot["providerId"] == "native-smoke", snapshot
    assert not snapshot.get("error"), snapshot
    metrics = {metric["label"]: metric for metric in snapshot["metrics"]}
    assert metrics["Fixture"]["used"] == 42, snapshot
    assert metrics["Fixture"]["limit"] == 100, snapshot
    assert metrics["Nonce"]["value"] == nonce, snapshot
    expected_os = {"Darwin": "macos", "Windows": "windows", "Linux": "linux"}[platform.system()]
    assert metrics["Platform"]["value"] == expected_os, snapshot


def smoke(bin_dir: Path, https_url: str | None = None, temp_dir: Path | None = None) -> dict:
    suffix = ".exe" if os.name == "nt" else ""
    binaries = [bin_dir / (name + suffix) for name in ["usagestat", "usagestatd"]]
    for binary in binaries:
        if not binary.is_file():
            raise FileNotFoundError(binary)
    result = {"os": platform.system(), "architecture": platform.machine(),
              "https": "not requested", "checks": []}
    with tempfile.TemporaryDirectory(prefix="usagestat native 使用 ", dir=temp_dir) as directory:
        root = Path(directory).resolve()
        env = isolated_env(root)
        unrelated = root / "unrelated working directory"
        unrelated.mkdir()
        install = root / "installed 使用"
        install.mkdir()
        for binary in binaries:
            shutil.copy2(binary, install / binary.name)
        cli, daemon = [install / binary.name for binary in binaries]
        shutil.copytree(FIXTURES, install / "plugins")
        nonce = secrets.token_hex(16)
        database = root / "fixture 使用.sqlite"
        # sqlite3's transaction context commits but does not close the handle.
        # Close explicitly before the backend opens it and before Windows cleanup.
        with closing(sqlite3.connect(database)) as connection:
            with connection:
                connection.execute("CREATE TABLE smoke (value INTEGER NOT NULL)")
                connection.execute("INSERT INTO smoke VALUES (40)")

        class Fixture(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                body = json.dumps({"nonce": nonce, "value": 2}).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *_args):
                pass

        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Fixture)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            settings = {"dataDir": env["USAGESTAT_DATA_DIR"], "database": str(database),
                        "url": f"http://127.0.0.1:{server.server_port}/", "nonce": nonce}
            if https_url:
                if not https_url.startswith("https://"):
                    raise ValueError("--https-url must use HTTPS")
                settings["httpsUrl"] = https_url
            config = Path(env["USAGESTAT_CONFIG_DIR"]) / "config.toml"
            config.write_text('refreshSec = 1\n[[providers]]\nid = "native-smoke"\nenabled = true\n'
                              'source = "local"\n[providers.settings]\n' +
                              "".join(f"{key} = {json.dumps(value, ensure_ascii=False)}\n"
                                      for key, value in settings.items()), encoding="utf-8")
            version = run(cli, ["--version"], unrelated, env).strip()
            assert version.startswith("usagestat "), version
            run(daemon, ["--help"], unrelated, env)
            run(cli, ["config", "validate"], unrelated, env)
            providers = json.loads(run(cli, ["--json", "list"], unrelated, env))
            assert [provider["id"] for provider in providers] == ["native-smoke"], providers
            icon = Path(providers[0]["icon"]["path"])
            # Rust canonical paths may carry Windows' extended-length prefix.
            # Compare file identity instead of textual path prefixes.
            assert icon.is_absolute() and icon.samefile(install / "plugins/native-smoke/icon.svg"), icon
            result["checks"].extend(["cli-version", "daemon-help", "config-validation", "installed-plugin-discovery"])
            check_snapshot(json.loads(run(cli, ["--json", "usage", "--provider", "native-smoke",
                                                "--source", "local"], unrelated, env)), nonce)
            marker = Path(env["USAGESTAT_DATA_DIR"]) / "plugins/native-smoke/runtime.txt"
            assert marker.read_text(encoding="utf-8") == nonce
            result["checks"].extend(["quickjs", "sqlite", "host-http", "host-filesystem", "isolated-data"])
            if https_url:
                result["https"] = "passed"

            # Require a fresh probe by the daemon, not a CLI-produced cache hit.
            (Path(env["USAGESTAT_DATA_DIR"]) / "snapshots.json").unlink(missing_ok=True)
            marker.unlink()

            # Select an available loopback port. Fail explicitly if another
            # process wins the bind race; never accept a foreign daemon's data.
            with socket.socket() as reservation:
                reservation.bind(("127.0.0.1", 0))
                port = reservation.getsockname()[1]
            opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
            with (root / "daemon.log").open("w+", encoding="utf-8") as log:
                process = subprocess.Popen([str(daemon), "--bind", f"127.0.0.1:{port}"],
                                           cwd=unrelated, env=env, stdout=log, stderr=log)
                try:
                    deadline = time.monotonic() + 30
                    while True:
                        if process.poll() is not None:
                            log.seek(0)
                            raise RuntimeError(f"Daemon exited {process.returncode}: {log.read()}")
                        try:
                            with opener.open(f"http://127.0.0.1:{port}/health", timeout=2) as response:
                                assert json.load(response)["status"] == "ok"
                            with opener.open(f"http://127.0.0.1:{port}/v1/usage", timeout=2) as response:
                                snapshots = json.load(response)
                            if snapshots:
                                check_snapshot(snapshots, nonce)
                                break
                        except (urllib.error.URLError, TimeoutError, ConnectionError):
                            pass
                        if time.monotonic() >= deadline:
                            log.seek(0)
                            raise TimeoutError(f"Daemon did not produce its fixture snapshot: {log.read()}")
                        time.sleep(0.1)
                    result["checks"].extend(["daemon-health", "daemon-polling", "daemon-usage-json"])
                    assert marker.read_text(encoding="utf-8") == nonce
                finally:
                    if process.poll() is None:
                        process.terminate()
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=10)
            assert not (install / "snapshots.json").exists()
            assert not (unrelated / "usagestat").exists()
            result["version"] = version
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, default=Path("target/debug"))
    parser.add_argument("--report", type=Path)
    parser.add_argument("--https-url")
    parser.add_argument("--temp-dir", type=Path, help="Scratch volume with room for both debug binaries")
    args = parser.parse_args()
    result = smoke(args.bin_dir.resolve(), args.https_url, args.temp_dir)
    encoded = json.dumps(result, indent=2) + "\n"
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(encoded, encoding="utf-8")
    print(encoded, end="")


if __name__ == "__main__":
    main()
