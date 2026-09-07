#!/usr/bin/env python3
"""Discover only disposable same-user process fixtures, without real IDE tokens."""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
from native_smoke import isolated_env, run
ROOT = Path(__file__).resolve().parents[2]

def check(cli: Path) -> dict:
    with tempfile.TemporaryDirectory(prefix='usagestat IDE 使用 ') as directory:
        root = Path(directory)
        env = isolated_env(root)
        binary = root / ('language_server_usagestat_fixture' + ('.exe' if os.name == 'nt' else ''))
        subprocess.run(['rustc', str(ROOT / 'tools/portability/fixtures/native_ls.rs'), '-o', str(binary)], check=True, timeout=90)
        plugin = root / 'plugins/ide-fixture'
        plugin.mkdir(parents=True)
        (plugin / 'plugin.json').write_text(json.dumps({'id': 'ide-fixture', 'name': 'IDE fixture', 'entry': 'plugin.js', 'enabledByDefault': True}), encoding='utf-8')
        children = []
        def launch():
            with socket.socket() as held:
                held.bind(('127.0.0.1', 0))
                port = held.getsockname()[1]
            child = subprocess.Popen([str(binary), '--csrf_token', 'synthetic quoted "token" 使用', '--port', str(port),
                '--data', str(root / 'fixture-ide data & (literal)')], cwd=root, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            children.append(child)
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                assert child.poll() is None, child.communicate()
                try:
                    with socket.create_connection(('127.0.0.1', port), timeout=0.2): return child, port
                except OSError: time.sleep(0.05)
            raise AssertionError('Fixture did not start')
        def probe(pid=None, expected='ready'):
            request = {'processName': 'language_server_usagestat_fixture', 'markers': ['fixture-ide'], 'csrfFlag': '--csrf_token', 'portFlag': '--port'}
            if pid: request['pid'] = pid
            script = '''globalThis.__usagestat_plugin = {probe: function(ctx) {
              var report = ctx.host.ls.discoverStatus(REQUEST);
              if (report.status !== EXPECTED) throw 'Unexpected discovery state: ' + report.status;
              if (report.result) {
                var response = ctx.host.http.request({url: 'http://127.0.0.1:' + report.result.ports[0],
                  method: 'GET', headers: {'x-fixture-token': report.result.csrf}, timeoutMs: 2000});
                if (response.status !== 200) throw 'Discovered token was not preserved';
                var legacy = ctx.host.ls.discover(REQUEST);
                if (!legacy || legacy.pid !== report.result.pid) throw 'Legacy discovery differs';
              }
              return {metrics: [{type:'text', label:'Discovery', value:report.status}]};
            }};'''.replace('REQUEST', json.dumps(request)).replace('EXPECTED', json.dumps(expected))
            (plugin / 'plugin.js').write_text(script, encoding='utf-8')
            snapshots = json.loads(run(cli, ['usage', 'ide-fixture', '--json'], root, env))
            assert snapshots[0]['metrics'][0].get('value') == expected, snapshots
            assert 'synthetic quoted' not in json.dumps(snapshots)
        try:
            first, _ = launch()
            probe(first.pid)
            second, _ = launch()
            probe(expected='ambiguous')
            probe(second.pid)
            first.kill(); first.communicate(timeout=5)
            probe(first.pid, expected='missing')
        finally:
            for child in children:
                if child.poll() is None: child.kill()
                child.communicate(timeout=5)
    return {'checks': ['native-argument-boundaries-and-local-token-request', 'multiple-instances-require-pid', 'stale-pid-rejected', 'legacy-discovery-compatible']}

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', type=Path, default=ROOT / 'target/debug/usagestat')
    args = parser.parse_args()
    print(json.dumps(check(args.cli.resolve()), indent=2))
