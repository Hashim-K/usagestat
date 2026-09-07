#!/usr/bin/env python3
"""Read only synthetic Claude/Codex histories through a disposable native daemon."""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
from native_smoke import isolated_env

ROOT = Path(__file__).resolve().parents[2]


def check(daemon: Path) -> dict:
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    checks = []
    with tempfile.TemporaryDirectory(prefix="usagestat local usage 使用 ") as directory:
        root = Path(directory)
        env = isolated_env(root)
        native_home = Path(env["HOME"])
        profiles = {provider: root / (provider + " account & (使用)") for provider in ["claude", "codex"]}
        env["CLAUDE_CONFIG_DIR"] = str(profiles["claude"])
        env["CODEX_HOME"] = str(profiles["codex"])
        # On Windows HOME is absent; the explicit provider profiles alone must
        # work without consulting the developer/runner's real Known Folder home.
        if os.name == "nt":
            env.pop("HOME", None)
        config = Path(env["USAGESTAT_CONFIG_DIR"]) / "config.toml"
        config.write_text("providers = []\n", encoding="utf-8")
        control = root / "control-key"
        control.write_text("synthetic-local-usage-control", encoding="utf-8")

        def write_history(profile, provider, tokens):
            if provider == "claude":
                path = profile / "projects/fixture/session.jsonl"
                rows = [{"timestamp": "2026-08-01T12:00:00Z", "sessionId": "fixture-session",
                         "cwd": str(root / "project 使用"),
                         "message": {"model": "claude-sonnet-4", "usage": {
                             "input_tokens": tokens, "output_tokens": 20,
                             "cache_read_input_tokens": 3, "cache_creation_input_tokens": 4}}}]
            else:
                path = profile / "sessions/2026/08/01/rollout-fixture.jsonl"
                rows = [{"timestamp": "2026-08-01T12:00:00Z", "payload": {
                    "id": "fixture-session", "cwd": str(root / "project 使用"), "model": "gpt-5",
                    "info": {"last_token_usage": {"input_tokens": tokens, "output_tokens": 20,
                             "cached_input_tokens": 3, "reasoning_output_tokens": 4}}}}]
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("\nnot-json\n" + "\n".join(map(json.dumps, rows)) + "\n{}\n", encoding="utf-8")
            (path.parent / "empty.jsonl").write_text("", encoding="utf-8")
            return path

        for provider, profile in profiles.items():
            write_history(profile, provider, 100)
            # Detect accidental fallback/merging on Unix and synthetic-home
            # implementations. Actual Windows Known Folders are never modified.
            write_history(native_home / ("." + provider), provider, 9000)
        write_history(native_home / "Library/Developer/Xcode/CodingAssistant/ClaudeAgentConfig", "claude", 9000)
        archived = profiles["codex"] / "archived_sessions/archived.jsonl"
        archived.parent.mkdir()
        original = write_history(root / "archived fixture", "codex", 50)
        archived.write_bytes(original.read_bytes())

        with socket.socket() as held:
            held.bind(("127.0.0.1", 0))
            bind = f"127.0.0.1:{held.getsockname()[1]}"
        base = "http://" + bind

        def request(path, method="GET"):
            headers = {"Authorization": "Bearer synthetic-local-usage-control"} if method == "POST" else {}
            with opener.open(urllib.request.Request(base + path, method=method, headers=headers), timeout=3) as response:
                return json.load(response)

        child = subprocess.Popen([str(daemon), "--bind", bind, "--config", str(config),
                                  "--control-key-file", str(control)],
                                 cwd=root, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                assert child.poll() is None, child.communicate()
                try:
                    if request("/health").get("pid") == child.pid:
                        break
                except (OSError, urllib.error.URLError):
                    time.sleep(0.03)
            else:
                raise AssertionError("Disposable daemon readiness timed out")
            for provider, expected in [("claude", 100), ("codex", 150)]:
                for report, field in [("daily", "daily"), ("weekly", "weekly"), ("monthly", "monthly"), ("session", "sessions")]:
                    rows = request(f"/v1/local-usage/{provider}/{report}")[field]
                    assert len(rows) == 1 and rows[0]["inputTokens"] == expected, rows
                    assert rows[0]["costUsd"] > 0, rows
                checks.append(provider + "-explicit-profile-and-normalized-reports")
            checks.append("codex-archived-history-and-malformed-empty-lines")
            # Removing an explicit Codex directory must report its failure,
            # never select the default account or an earlier saved result.
            profiles["codex"].rename(root / "removed codex profile")
            invalid = request("/v1/local-usage/codex/daily")
            assert invalid["error"]["code"] == "LOCAL_USAGE_PATH_UNAVAILABLE", invalid
            checks.append("invalid-explicit-profile-reports-error-without-fallback")
            request("/v1/daemon/shutdown", method="POST")
            child.communicate(timeout=8)
            assert child.returncode == 0
        finally:
            if child.poll() is None:
                child.kill()
                child.communicate(timeout=5)
    return {"checks": checks}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--daemon", type=Path, default=ROOT / "target/debug/usagestatd")
    args = parser.parse_args()
    print(json.dumps(check(args.daemon.resolve()), indent=2))
