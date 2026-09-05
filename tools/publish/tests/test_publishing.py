import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'tools/publish/scripts' / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


prepare = load('prepare', 'prepare-packages.py')
publication = load('publication', 'publication-state.py')


def make_archive(assets, arch, extra=None, plugin=b'{}'):
    path = assets / f'usagestat-linux-{arch}.tar.gz'
    with tarfile.open(path, 'w:gz') as archive:
        machine = {'x86_64': 62, 'aarch64': 183}[arch]
        elf = b'\x7fELF\x02\x01' + bytes(12) + machine.to_bytes(2, 'little')
        for name, data in [('usagestat', elf), ('usagestatd', elf), ('LICENSE', b'MIT'), ('plugins/codex/plugin.json', plugin)]:
            member = tarfile.TarInfo(name)
            member.size = len(data)
            member.mode = 0o755 if name.startswith('usagestat') else 0o644
            archive.addfile(member, io.BytesIO(data))
        if extra:
            archive.addfile(extra)
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    path.with_name(path.name + '.sha256').write_text(f'{digest}  {path.name}\n')


class PackageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.assets = self.base / 'assets'
        self.assets.mkdir()
        for arch in ['x86_64', 'aarch64']:
            make_archive(self.assets, arch)

    def test_rejects_prerelease_and_injected_tags(self):
        for tag in ['1.0.3', 'v1.0.3-beta.2', 'v01.0.3', 'v1.0.3\n', 'v1.0.3;echo bad', '../main']:
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                prepare.stable_version(tag)
        self.assertEqual(prepare.stable_version('v12.3.0'), '12.3.0')

    def test_rejects_corrupted_archive_and_checksum_filename(self):
        checksum = self.assets / 'usagestat-linux-x86_64.tar.gz.sha256'
        original = checksum.read_text()
        for invalid in ['0' * 64 + '  usagestat-linux-x86_64.tar.gz', original.replace('usagestat-linux-x86_64.tar.gz', '../bad')]:
            checksum.write_text(invalid)
            with self.assertRaises(ValueError):
                prepare.unpack(self.assets, 'x86_64', self.base / 'out')

    def test_rejects_traversal_links_and_duplicate_members(self):
        for name, kind in [('../escape', tarfile.REGTYPE), ('plugins/link', tarfile.SYMTYPE), ('usagestat', tarfile.REGTYPE)]:
            member = tarfile.TarInfo(name)
            member.type = kind
            member.linkname = '/etc/passwd'
            make_archive(self.assets, 'x86_64', extra=member)
            with self.subTest(name=name), self.assertRaises(ValueError):
                prepare.unpack(self.assets, 'x86_64', self.base / 'out')
        self.assertFalse((self.base / 'escape').exists())

    def test_wrong_architecture_is_rejected(self):
        src = self.assets / 'usagestat-linux-aarch64.tar.gz'
        dest = self.assets / 'usagestat-linux-x86_64.tar.gz'
        dest.write_bytes(src.read_bytes())
        dest.with_name(dest.name + '.sha256').write_text(hashlib.sha256(dest.read_bytes()).hexdigest() + '  ' + dest.name)
        with self.assertRaises(ValueError):
            prepare.unpack(self.assets, 'x86_64', self.base / 'out')

    def test_architecture_plugins_must_match(self):
        make_archive(self.assets, 'aarch64', plugin=b'{"different":true}')
        with self.assertRaises(ValueError):
            prepare.prepare('v1.0.3', self.assets, self.base / 'out', ROOT)

    def test_recipes_and_reproducible_ppa_source(self):
        for name in ['first', 'second']:
            prepare.prepare('v9.8.7', self.assets, self.base / name, ROOT)
        out = self.base / 'first'
        self.assertIn('pkgver=9.8.7', (out / 'PKGBUILD').read_text())
        self.assertIn('version "9.8.7"', (out / 'usagestat.rb').read_text())
        self.assertIn('Version:        9.8.7', (out / 'usagestat.spec').read_text())
        self.assertEqual((out / 'usagestat_9.8.7.orig.tar.gz').read_bytes(), (self.base / 'second/usagestat_9.8.7.orig.tar.gz').read_bytes())
        with tarfile.open(out / 'usagestat_9.8.7.orig.tar.gz') as archive:
            names = archive.getnames()
            for arch in ['amd64', 'arm64']:
                self.assertIn(f'usagestat-9.8.7/bin/{arch}/usagestatd', names)
            self.assertFalse(any('/debian' in name for name in names))


class PublicationTests(unittest.TestCase):
    def test_copr_selects_latest_attempt_and_waits_for_pending(self):
        builds = [{'id': 1, 'source_package': {'version': '1.0.3-1'}, 'state': 'succeeded'}, {'id': 2, 'source_package': {'version': '1.0.3-1'}, 'state': 'running'}, {'id': 3, 'source_package': None, 'state': 'failed'}]
        with patch.object(publication, 'get', return_value={'items': builds}):
            self.assertEqual(publication.state('copr', '1.0.3'), 'pending')
            builds[1]['state'] = 'succeeded'
            self.assertEqual(publication.state('copr', '1.0.3'), 'done')
            builds[1]['state'] = 'failed'
            self.assertEqual(publication.state('copr', '1.0.3'), 'failed')
            self.assertEqual(publication.state('copr', '9.8.7'), 'missing')

    def test_network_failure_is_not_treated_as_missing(self):
        with patch.object(publication, 'get', side_effect=OSError('network failed')):
            with self.assertRaises(OSError):
                publication.state('copr', '1.0.3')

    def test_ppa_requires_binaries_published_for_every_build(self):
        entry = {'source_package_version': '1.0.3-1ppa1', 'status': 'Published', 'date_created': '2026-09-06', 'self_link': 'source'}
        builds = [{'buildstate': 'Successfully built', 'distro_series_link': 'noble', 'arch_tag': arch} for arch in ['amd64', 'arm64']]
        binaries = [{'binary_package_version': '1.0.3-1ppa1', 'distro_arch_series_link': 'noble/amd64'}]
        def get(url):
            if 'getPublishedSources' in url:
                return {'entries': [entry]}
            if 'getBuilds' in url:
                return {'entries': builds}
            return {'entries': binaries}
        with patch.object(publication, 'get', side_effect=get):
            self.assertEqual(publication.state('ppa', '1.0.3'), 'pending')
            binaries.append({'binary_package_version': '1.0.3-1ppa1', 'distro_arch_series_link': 'noble/arm64'})
            self.assertEqual(publication.state('ppa', '1.0.3'), 'done')
            builds[1]['buildstate'] = 'Failed to build'
            with self.assertRaises(RuntimeError):
                publication.state('ppa', '1.0.3')


if __name__ == '__main__':
    unittest.main()
