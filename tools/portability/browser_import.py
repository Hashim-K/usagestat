#!/usr/bin/env python3
"""Native CLI cookie import against disposable browser stores; no real secrets."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
from native_smoke import isolated_env

ROOT = Path(__file__).resolve().parents[2]

def check(cli: Path) -> dict:
    with tempfile.TemporaryDirectory(prefix='usagestat browser 使用 ') as directory:
        root = Path(directory)
        env = isolated_env(root)
        plugin = root / 'plugins/browser-fixture'
        plugin.mkdir(parents=True)
        (plugin / 'plugin.json').write_text(json.dumps({'id':'browser-fixture','name':'Browser fixture','version':'1.0.0','entry':'plugin.js','webUrl':'https://chat.example.test/api/usage','supportedModes':['web'],'autoMode':'web'}),encoding='utf-8')
        # Manual config wiring is exercised with a synthetic token only.
        (plugin / 'plugin.js').write_text('globalThis.__usagestat_plugin={probe:function(ctx){if(ctx.provider.cookieHeader!=="session=synthetic-original")throw "Manual credentials changed";return {metrics:[{type:"text",label:"Manual",value:"preserved"}]};}};',encoding='utf-8')
        config = Path(env['USAGESTAT_CONFIG_DIR']) / 'config.toml'
        config.write_text('[[providers]]\nid="browser-fixture"\nsourceMode="web"\ncookieHeader="session=synthetic-original"\n',encoding='utf-8')
        if os.name != 'nt': config.chmod(0o600)
        original = config.read_bytes()
        browser = root / 'Browser profile # % & 使用'
        profile = browser / 'Profile 使用 with spaces'
        profile.mkdir(parents=True)
        db = profile / 'Cookies'
        writer = sqlite3.connect(db)
        writer.executescript("CREATE TABLE meta(key TEXT,value TEXT); INSERT INTO meta VALUES('version','24'); CREATE TABLE cookies(host_key TEXT,name TEXT,value TEXT,encrypted_value BLOB,path TEXT,expires_utc INTEGER,is_secure INTEGER,top_frame_site_key TEXT);")
        writer.execute("INSERT INTO cookies VALUES('.example.test','session','synthetic-original',x'','/',0,1,'')")
        writer.commit()
        args = ['auth','import-cookies','--provider','browser-fixture','--browser','chrome','--profile',profile.name,'--user-data-dir',str(browser),'--format','json']
        def command(arguments, code=None):
            result = subprocess.run([str(cli),*arguments],cwd=root,env=env,capture_output=True,text=True,encoding='utf-8',timeout=45)
            try: payload=json.loads(result.stdout)
            except ValueError: raise AssertionError('CLI failed without structured JSON') from None
            if code:
                assert result.returncode != 0 and payload.get('error') == code, (result.returncode,payload.get('error'))
                assert 'synthetic-original' not in result.stdout+result.stderr
            else: assert result.returncode == 0, (result.returncode, payload.get('error') if isinstance(payload,dict) else 'snapshot')
            assert config.read_bytes() == original, 'Import modified configured credentials'
            return payload
        try:
            writer.execute('PRAGMA journal_mode=WAL')
            writer.execute("UPDATE cookies SET value='synthetic-uncommitted'")
            payload=command(args)
            assert payload['cookieHeader']=='session=synthetic-original'
            assert payload['profile']==profile.name and payload['source']=='chrome' and payload['providerId']=='browser-fixture'
            writer.rollback()
            # No browser password is requested for plain data or known unsupported ciphertext.
            writer.execute("UPDATE cookies SET value='',encrypted_value=?",(b'v20synthetic-bound-cookie',));writer.commit()
            command(args,'APP_BOUND_UNSUPPORTED' if os.name=='nt' else 'COOKIE_FORMAT_UNSUPPORTED')
            writer.execute("UPDATE cookies SET value='synthetic-expired',encrypted_value=x'',expires_utc=1");writer.commit()
            command(args,'SESSION_NOT_FOUND')
            writer.execute("UPDATE cookies SET value='synthetic-original',expires_utc=0");writer.commit()
            writer.execute('PRAGMA journal_mode=DELETE');writer.execute('BEGIN EXCLUSIVE')
            command(args,'COOKIE_DB_UNAVAILABLE');writer.rollback()
            # Failed imports do not alter the explicit credential used by later probes.
            payload=command(['usage','browser-fixture','--json'])
            assert payload[0]['metrics'][0]['value']=='preserved'
            assert sorted(p.name for p in profile.iterdir())==['Cookies']
            assert not list(root.glob('usagestat-cookies-*'))
        finally: writer.close()
    return {'checks':['native-cookie-cli-profile-and-unicode-paths','live-wal-snapshot-and-bounded-lock','unsupported-encryption-and-expiry-states','manual-config-preserved-after-import-failure','no-temporary-cookie-databases']}

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--cli',type=Path,default=ROOT/'target/debug/usagestat');args=parser.parse_args();print(json.dumps(check(args.cli.resolve()),indent=2))
