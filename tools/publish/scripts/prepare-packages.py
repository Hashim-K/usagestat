#!/usr/bin/env python3
"""Validate published archives and render package recipes without credentials."""
import argparse
import hashlib
import re
import shutil
import tarfile
from pathlib import Path


def stable_version(tag):
    if not re.fullmatch(r'v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', tag):
        raise ValueError('Package repositories require a stable vMAJOR.MINOR.PATCH tag')
    return tag[1:]


def unpack(assets, arch, destination):
    name = f'usagestat-linux-{arch}.tar.gz'
    archive = assets / name
    checksum = (assets / (name + '.sha256')).read_text().split()
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    if checksum != [digest, name]:
        raise ValueError(f'Invalid checksum for {name}')
    with tarfile.open(archive) as tar:
        members = tar.getmembers()
        names = set()
        for member in members:
            path = Path(member.name)
            if (path.is_absolute() or '..' in path.parts or member.name in names
                    or not (member.isfile() or member.isdir())
                    or path.parts[0] not in {'usagestat', 'usagestatd', 'LICENSE', 'plugins'}):
                raise ValueError(f'Unsafe archive member: {member.name}')
            names.add(member.name)
        for binary in ('usagestat', 'usagestatd'):
            member = tar.getmember(binary)
            data = tar.extractfile(member).read(20)
            machine = {'x86_64': 62, 'aarch64': 183}[arch]
            if not member.mode & 0o111 or data[:6] != b'\x7fELF\x02\x01' or int.from_bytes(data[18:20], 'little') != machine:
                raise ValueError(f'Invalid {arch} binary: {binary}')
        if 'LICENSE' not in names or not any(n.endswith('/plugin.json') for n in names):
            raise ValueError('Archive is missing its license or provider plugins')
        tar.extractall(destination, filter='data')
    return digest


def replace_one(pattern, replacement, text):
    text, count = re.subn(pattern, replacement, text, flags=re.MULTILINE)
    if count != 1:
        raise ValueError(f'Expected one recipe field matching {pattern}, found {count}')
    return text


def prepare(tag, assets, output, root):
    version = stable_version(tag)
    output.mkdir(parents=True, exist_ok=False)
    hashes = {arch: unpack(assets, arch, output / arch) for arch in ('x86_64', 'aarch64')}
    # Both release builds must carry exactly the same provider data and license.
    for item in ('plugins', 'LICENSE'):
        def contents(arch):
            base = output / arch
            paths = (base / item).rglob('*') if item == 'plugins' else [base / item]
            return {str(p.relative_to(base)): p.read_bytes() for p in paths if p.is_file()}
        if contents('x86_64') != contents('aarch64'):
            raise ValueError(f'Architecture archives disagree on {item}')
    aur = (root / 'packaging/aur/usagestat-bin/PKGBUILD').read_text()
    aur = replace_one(r'^pkgver=.*$', f'pkgver={version}', aur)
    aur = replace_one(r'^pkgrel=.*$', 'pkgrel=1', aur)
    aur = replace_one(r'^sha256sums_x86_64=.*$', f'sha256sums_x86_64=("{hashes["x86_64"]}")', aur)
    (output / 'PKGBUILD').write_text(aur)
    brew = (root / 'packaging/homebrew/Formula/usagestat.rb').read_text()
    brew = replace_one(r'^  version ".*"$', f'  version "{version}"', brew)
    values = iter([hashes['aarch64'], hashes['x86_64']])
    brew, count = re.subn(r'sha256 "[a-f0-9]+"', lambda _: f'sha256 "{next(values)}"', brew)
    if count != 2:
        raise ValueError('Expected two Homebrew checksums')
    (output / 'usagestat.rb').write_text(brew)
    rpm = (root / 'packaging/rpm/usagestat.spec').read_text()
    rpm = replace_one(r'^Version:.*$', f'Version:        {version}', rpm)
    (output / 'usagestat.spec').write_text(rpm)
    source = output / f'usagestat-{version}'
    for arch, debarch in [('x86_64', 'amd64'), ('aarch64', 'arm64')]:
        dest = source / 'bin' / debarch
        dest.mkdir(parents=True)
        for binary in ('usagestat', 'usagestatd'):
            shutil.copy2(output / arch / binary, dest / binary)
    shutil.copytree(output / 'x86_64/plugins', source / 'plugins')
    shutil.copy2(output / 'x86_64/LICENSE', source / 'LICENSE')
    # Fixed timestamps make retries produce an identical upstream source tarball.
    def normalize(info):
        info.uid = info.gid = info.mtime = 0
        info.uname = info.gname = ''
        return info
    import gzip
    with (output / f'usagestat_{version}.orig.tar.gz').open('wb') as raw:
        with gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as gz:
            with tarfile.open(fileobj=gz, mode='w') as tar:
                tar.add(source, arcname=source.name, filter=normalize)
    shutil.copytree(root / 'packaging/debian', source / 'debian')
    print(f'Validated {tag}; package inputs are in {output}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('tag')
    parser.add_argument('assets', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    prepare(args.tag, args.assets, args.output, Path(__file__).resolve().parents[3])
