#!/usr/bin/env python3
"""Verify the native plugin SQLite reader against a live synthetic IDE writer."""
import argparse
import json
from pathlib import Path
import sqlite3
import tempfile
import time
from native_smoke import isolated_env, run

ROOT = Path(__file__).resolve().parents[2]


def check(cli: Path) -> dict:
    with tempfile.TemporaryDirectory(prefix="usagestat provider storage 使用 ") as directory:
        root = Path(directory)
        env = isolated_env(root)
        db = root / "IDE state # % & (使用).vscdb"
        missing = root / "missing.vscdb"
        plugin = root / "plugins/storage-fixture"
        plugin.mkdir(parents=True)
        (plugin / "plugin.json").write_text(json.dumps({"id": "storage-fixture", "name": "Storage fixture", "entry": "plugin.js", "enabledByDefault": True}), encoding="utf-8")
        writer = sqlite3.connect(db)
        try:
            writer.execute("PRAGMA journal_mode=WAL")
            writer.execute("CREATE TABLE ItemTable (value INTEGER)")
            writer.execute("INSERT INTO ItemTable VALUES (42)")
            writer.commit()

            def probe(expected):
                script = '''globalThis.__usagestat_plugin = {probe: function(ctx) {
                    if (!ctx.host.fs.appSupportPath('usagestat-fixture/leaf')) throw 'Native app support root unavailable';
                    ['../escape', '/absolute', 'C:/drive', 'a\\\\b', 'a//b'].forEach(function(path) {
                        if (ctx.host.fs.appSupportPath(path) != null) throw 'Invalid app suffix accepted';
                    });
                    var result;
                    try { result = JSON.parse(ctx.host.sqlite.query(DB, 'SELECT value FROM ItemTable'))[0].value; }
                    catch (_) { result = 'busy'; }
                    var wrote = false;
                    try { ctx.host.sqlite.query(DB, 'UPDATE ItemTable SET value = 10000'); wrote = true; } catch (_) {}
                    if (wrote) throw 'Read-only database accepted a write';
                    var missingFailed = false;
                    try { ctx.host.sqlite.query(MISSING, 'SELECT 1'); } catch (_) { missingFailed = true; }
                    if (!missingFailed) throw 'Missing database opened';
                    return {metrics:[{type:'text',label:'Value',value:String(result)}]};
                }};'''.replace("MISSING", json.dumps(str(missing))).replace("DB", json.dumps(str(db)))
                (plugin / "plugin.js").write_text(script, encoding="utf-8")
                start = time.monotonic()
                output = json.loads(run(cli, ["usage", "storage-fixture", "--json"], root, env))
                assert output[0]["metrics"][0].get("value") == str(expected), output
                assert time.monotonic() - start < 5, "SQLite lock wait exceeded bounded fixture deadline"
                assert not missing.exists()

            probe(42)
            writer.execute("UPDATE ItemTable SET value=99")
            probe(42)
            writer.commit()
            probe(99)
            assert writer.execute("SELECT value FROM ItemTable").fetchone()[0] == 99
            writer.execute("PRAGMA journal_mode=DELETE")
            writer.execute("BEGIN EXCLUSIVE")
            probe("busy")
            writer.rollback()
            probe(99)
        finally:
            writer.close()
    return {"checks": ["native-sqlite-special-character-path", "active-wal-writer-committed-snapshot",
                       "read-only-query-and-no-missing-file-creation", "exclusive-lock-bounded-and-recovers"]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, default=ROOT / "target/debug/usagestat")
    args = parser.parse_args()
    print(json.dumps(check(args.cli.resolve()), indent=2))
