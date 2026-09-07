#!/usr/bin/env python3
"""Verify npm publication inputs and recover exact-version partial publication."""
from __future__ import annotations
import argparse
import base64
import hashlib
import json
from pathlib import Path
import subprocess
import urllib.error
import urllib.parse
import urllib.request
from npm_packages import ROOT, npm_command

def validate(plan: dict, directory: Path) -> list[dict]:
    if plan['schemaVersion'] != 1 or plan['channel'] not in ('stable', 'prerelease'):
        raise ValueError('Only explicit stable/prerelease package inputs may be published')
    if ('-' in plan['version']) != (plan['channel'] == 'prerelease') or plan['distTag'] != ('next' if plan['channel'] == 'prerelease' else 'latest'):
        raise ValueError('Version and npm release channel disagree')
    packages = plan['packages']
    mains = [p for p in packages if p['role'] == 'main']
    platforms = [p for p in packages if p['role'] == 'platform']
    if len(mains) != 1 or not platforms or len({p['name'] for p in packages}) != len(packages):
        raise ValueError('Expected unique platform packages and one main package')
    if mains[0]['packageJson']['optionalDependencies'] != {p['name']: plan['version'] for p in platforms}:
        raise ValueError('Main optional dependencies must exactly match the staged platform packages')
    for package in packages:
        meta = package['packageJson']
        if meta['name'] != package['name'] or meta['version'] != plan['version'] or meta.get('private') or meta.get('scripts'):
            raise ValueError('Invalid or private npm publication metadata')
        relative = Path(package['tarball'])
        if relative.is_absolute() or '..' in relative.parts:
            raise ValueError('Invalid package tarball path')
        data = (directory / relative).read_bytes()
        integrity = 'sha512-' + base64.b64encode(hashlib.sha512(data).digest()).decode()
        if hashlib.sha256(data).hexdigest() != package['sha256'] or integrity != package['integrity']:
            raise ValueError('Packed npm integrity mismatch')
    return platforms + mains

def registry_version(registry: str, name: str, version: str):
    url = registry.rstrip('/') + '/' + urllib.parse.quote(name, safe='@') + '/' + urllib.parse.quote(version, safe='')
    try:
        with urllib.request.urlopen(url, timeout=30) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        if error.code == 404: return None
        raise ValueError(f'Registry verification failed with HTTP {error.code}') from None

def existing_matches(package: dict, remote) -> bool:
    if remote is None: return False
    if remote.get('dist', {}).get('integrity') != package['integrity']:
        raise ValueError(f"Refusing to replace different published bytes: {package['name']}")
    for field in ['name', 'version', 'optionalDependencies', 'os', 'cpu', 'libc', 'bin']:
        if remote.get(field) != package['packageJson'].get(field):
            raise ValueError(f"Published package metadata differs: {package['name']} ({field})")
    return True

def run(manifest: Path, publish: bool) -> None:
    settings = json.loads((ROOT / 'npm/distribution.json').read_text())
    plan = json.loads(manifest.read_text())
    packages = validate(plan, manifest.parent)
    if settings['name'] != packages[-1]['name'] or any(not p['name'].startswith(settings['platformPrefix']) for p in packages[:-1]):
        raise ValueError('Package names differ from the configured namespace')
    if publish and not settings['publicationEnabled']:
        raise ValueError('First publication and npm trusted-publisher setup are pending; publicationEnabled is false')
    registry = settings['registry']
    if registry != 'https://registry.npmjs.org/':
        raise ValueError('Production publication only supports the configured public npm registry')
    # Inspect every existing version before changing anything. A conflicting
    # partial publication must fail before uploading another package.
    states = {p['name']: existing_matches(p, registry_version(registry, p['name'], plan['version'])) for p in packages}
    if not publish:
        print(json.dumps({'version': plan['version'], 'distTag': plan['distTag'], 'publicationEnabled': settings['publicationEnabled'],
            'packages': [{'name': p['name'], 'state': 'identical' if states[p['name']] else 'missing'} for p in packages]}, indent=2))
        return
    for package in packages:
        if not states[package['name']]:
            # Each platform is verified before uploading the main package.
            # npm trusted publishing authenticates publish, not dist-tag edits;
            # set the final tag atomically in publish instead of a later edit.
            subprocess.run([*npm_command(), 'publish', str((manifest.parent / package['tarball']).resolve()),
                '--access', 'public', '--tag', plan['distTag'], '--ignore-scripts', '--provenance', '--registry', registry], check=True)
        if not existing_matches(package, registry_version(registry, package['name'], plan['version'])):
            raise ValueError('Published package was not visible; retry the same inputs after registry propagation')
    print('Verified all exact-version native payloads and published platform packages before the main package.')

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('manifest', type=Path)
    parser.add_argument('--publish', action='store_true')
    args = parser.parse_args()
    run(args.manifest.resolve(), args.publish)
