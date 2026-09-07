#!/usr/bin/env python3
"""Credential-free CLI probe deadline, allowlist, and Unix signal integration."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import time

from native_smoke import isolated_env

ROOT = Path(__file__).resolve().parents[2]


def read_address(path: Path, deadline: float) -> tuple[str, int]:
    while time.monotonic() < deadline:
        try:
            host, port = path.read_text().strip().rsplit(":", 1)
            return host, int(port)
        except (FileNotFoundError, ValueError):
            time.sleep(0.01)
    raise TimeoutError("Helper descendant did not start")


def assert_stopped(address: tuple[str, int]) -> None:
    deadline = time.monotonic() + 2
    while time.monotonic() < deadline:
        try:
            with socket.create_connection(address, timeout=0.1):
                pass
        except OSError:
            return
        time.sleep(0.01)
    raise AssertionError("Probe left its helper descendant running")


def check(binary: Path) -> dict:
    result = {"checks": []}
    with tempfile.TemporaryDirectory(prefix="usagestat probe cancellation ") as directory:
        root = Path(directory)
        env = isolated_env(root)
        env["USAGESTAT_PROBE_TIMEOUT_SEC"] = "1"
        helper = root / ("gh.exe" if os.name == "nt" else "gh")
        subprocess.run(["rustc", "--edition=2024", "--crate-name=probe_fixture",
                        str(ROOT / "crates/ai-usage-core/tests/fixtures/process_helper.rs"),
                        "-o", str(helper)], check=True, capture_output=True, timeout=60)
        env["USAGESTAT_HELPER_PATH"] = str(root)
        plugins = root / "plugins/probe-cancellation"
        plugins.mkdir(parents=True)
        (plugins / "plugin.json").write_text(json.dumps({
            "id": "probe-cancellation", "name": "Probe cancellation fixture", "entry": "plugin.js",
            "enabledByDefault": False, "supportedModes": ["local"], "autoMode": "local",
        }), encoding="utf-8")
        (plugins / "plugin.js").write_text("""
globalThis.__usagestat_plugin = { probe(ctx) {
  const settings = ctx.provider.settings;
  if (settings.mode === 'spin') { while (true) {} }
  ctx.host.command.run({ program: settings.mode === 'forbidden' ? 'unapproved-helper' : 'gh',
    args: ['tree', settings.ready], timeoutMs: 30000 });
  return { metrics: [] };
} };
""", encoding="utf-8")
        config = Path(env["USAGESTAT_CONFIG_DIR"]) / "config.toml"

        def configure(mode: str, name: str) -> Path:
            ready = root / (name + ".ready")
            config.write_text('[[providers]]\nid="probe-cancellation"\nenabled=true\nsource="local"\n'
                              '[providers.settings]\nmode=' + json.dumps(mode) + '\nready=' +
                              json.dumps(str(ready)) + '\n', encoding="utf-8")
            return ready

        argv = [str(binary), "--json", "--plugin-dir", str(plugins.parent), "usage", "probe-cancellation"]
        for mode in ["helper", "spin", "forbidden"]:
            ready = configure(mode, mode)
            completed = subprocess.run(argv, cwd=root, env=env, capture_output=True, text=True,
                                       encoding="utf-8", timeout=6)
            assert completed.returncode == 0, completed.stderr
            snapshot, = json.loads(completed.stdout)
            assert snapshot["source"] == "error", snapshot
            expected = "command not allowed" if mode == "forbidden" else "Probe timed out"
            assert expected in snapshot["metrics"][0]["text"], snapshot
            if mode == "helper":
                assert_stopped(read_address(ready, time.monotonic() + 1))
            result["checks"].append(mode)

        if os.name != "nt":
            env["USAGESTAT_PROBE_TIMEOUT_SEC"] = "30"
            for signum in [signal.SIGINT, signal.SIGTERM]:
                ready = configure("helper", signum.name)
                child = subprocess.Popen(argv, cwd=root, env=env, stdout=subprocess.PIPE,
                                         stderr=subprocess.PIPE, start_new_session=True)
                try:
                    address = read_address(ready, time.monotonic() + 8)
                    child.send_signal(signum)
                    _, stderr = child.communicate(timeout=5)
                    assert child.returncode == 130, (child.returncode, stderr)
                    assert_stopped(address)
                    result["checks"].append(signum.name)
                finally:
                    if child.poll() is None:
                        child.kill()
                        child.wait(timeout=5)
        else:
            env["USAGESTAT_PROBE_TIMEOUT_SEC"] = "30"
            for event in ["ctrl-c", "ctrl-break", "close"]:
                ready = configure("helper", event)
                child = subprocess.Popen(argv, cwd=root, env=env, stdout=subprocess.PIPE,
                                         stderr=subprocess.PIPE, creationflags=subprocess.CREATE_NEW_CONSOLE)
                try:
                    address = read_address(ready, time.monotonic() + 8)
                    sender = subprocess.run([sys.executable, str(Path(__file__).with_name("windows_console.py")),
                                             str(child.pid), event], capture_output=True, timeout=5)
                    # Closing that console can also terminate its sender.
                    assert sender.returncode == 0 or event == "close", sender.stderr
                    _, stderr = child.communicate(timeout=6)
                    assert child.returncode == 130, (event, child.returncode, stderr)
                    assert_stopped(address)
                    result["checks"].append(event)
                finally:
                    if child.poll() is None:
                        child.kill()
                        child.wait(timeout=5)
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    suffix = ".exe" if os.name == "nt" else ""
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug" / ("usagestat" + suffix))
    args = parser.parse_args()
    print(json.dumps(check(args.binary.resolve()), indent=2))
