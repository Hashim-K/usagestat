"""Prove console-free launch, exit propagation, and kernel cleanup on Windows."""
from __future__ import annotations
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
from native_smoke import isolated_env
from probe_cancellation import read_address, assert_stopped

ROOT = Path(__file__).resolve().parents[2]

def check(launcher: Path) -> dict:
    assert os.name == "nt"
    result = {"checks": []}
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel.OpenProcess.restype = wintypes.HANDLE
    kernel.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    with tempfile.TemporaryDirectory(prefix="usagestat launcher 使用 & space ") as directory:
        root = Path(directory)
        env = isolated_env(root)
        binary = root / "synthetic daemon.exe"
        subprocess.run(["rustc", "--edition=2024", str(ROOT / "tools/portability/fixtures/windows_service.rs"),
                        "-o", str(binary)], check=True, capture_output=True, timeout=60)
        settings = root / "daemon.json"
        settings.write_text(json.dumps({"t3Mode": "off", "installation": {
            "owner": str(root), "binary": str(binary), "bind": "127.0.0.1:6736", "config": str(root / "unused-config.toml"),
            "pluginDirs": [], "environment": {"USAGESTAT_DATA_DIR": env["USAGESTAT_DATA_DIR"]},
            "managementKeyFile": str(root / "unused-management"), "controlKeyFile": str(root / "unused-control"),
        }}), encoding="utf-8")
        mode = root / "fixture-mode"
        mode.write_text("exit")
        child = subprocess.Popen([str(launcher), "--service-settings", str(settings)], env=env, cwd=root)
        try:
            deadline = time.monotonic() + 8
            while not (root / "parent.pid").exists():
                assert child.poll() is None, "launcher failed before its backend started"
                assert time.monotonic() < deadline, "synthetic backend did not start"
                time.sleep(0.02)
            crashed_pid = int((root / "parent.pid").read_text())
            mode.write_text("tree")
            address = read_address(root / "descendant.ready", time.monotonic() + 12)
            assert int((root / "parent.pid").read_text()) != crashed_pid, "launcher did not restart its failed backend"
        finally:
            if child.poll() is None:
                child.kill()
                child.wait(timeout=5)
        assert_stopped(address)
        logs = Path(env["USAGESTAT_DATA_DIR"]) / "logs"
        assert "synthetic daemon stdout" in (logs / "daemon.stdout.log").read_text()
        assert "synthetic daemon stderr" in (logs / "daemon.stderr.log").read_text()
        result["checks"].append("no-console-backend-crash-restart-and-private-output")
        (root / "descendant.ready").unlink()
        mode.write_text("tree")
        child = subprocess.Popen([str(launcher), "--service-settings", str(settings)], env=env, cwd=root)
        parent = None
        try:
            address = read_address(root / "descendant.ready", time.monotonic() + 8)
            parent = kernel.OpenProcess(0x100000, False, int((root / "parent.pid").read_text()))
            assert parent, "synthetic backend process not available"
            child.kill()  # Only the GUI launcher; the job must remove both descendants.
            child.wait(timeout=5)
            assert kernel.WaitForSingleObject(parent, 2000) == 0, "backend survived launcher termination"
            assert_stopped(address)
            result["checks"].append("forced-launcher-stop-removes-backend-and-descendants")
        finally:
            if parent:
                kernel.CloseHandle(parent)
            if child.poll() is None:
                subprocess.run([str(Path(os.environ["SystemRoot"]) / "System32/taskkill.exe"),
                                "/PID", str(child.pid), "/T", "/F"], capture_output=True, timeout=10)
                child.wait(timeout=5)
        mode.write_text("success")
        completed = subprocess.run([str(launcher), "--service-settings", str(settings)], env=env, cwd=root, timeout=10)
        assert completed.returncode == 0, "successful backend shutdown was restarted"
        result["checks"].append("successful-backend-shutdown-exits-supervisor")
    return result
