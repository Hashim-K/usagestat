#!/usr/bin/env python3
"""Build and stage an isolated native development installation without Bash or service changes."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools/publish/scripts'))
from native_artifacts import binary_names, digest, native_target, resource_files, version


def build():
    details = subprocess.check_output(['rustc', '-vV'], text=True)
    target = next(line.removeprefix('host: ') for line in details.splitlines() if line.startswith('host: '))
    native_target(target)
    subprocess.run(['cargo', 'build', '--locked', '--workspace', '--target', target], cwd=ROOT, check=True)
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--no-deps', '--format-version=1'], cwd=ROOT, text=True))
    return Path(metadata['target_directory']) / target / 'debug', target


def stage(binary_dir, output, target):
    native_target(target)
    if output.exists() or output.is_symlink():
        raise ValueError('Development output already exists; choose a fresh directory to preserve any running installation')
    package_version = version()
    files = resource_files()
    for name in binary_names(target):
        binary = binary_dir / name
        if binary.is_symlink() or not binary.is_file():
            raise ValueError('Missing regular native build output: ' + name)
        reported = subprocess.check_output([str(binary), '--version'], text=True, encoding='utf-8', timeout=30).strip()
        if reported.rsplit(' ', 1)[-1] != package_version:
            raise ValueError('Development binary version differs from source: ' + name)
        renamed = binary.stem + '-dev' + binary.suffix if os.name == 'nt' else name + '-dev'
        files[renamed] = binary.read_bytes()
    executables = {name for name in files if name.startswith('usagestat') and '/' not in name}
    # Validate everything before creating the destination. A staged development
    # directory is disposable; it never contains user config, history or secrets.
    output.mkdir(mode=0o700, parents=True)
    try:
        for name, data in files.items():
            dest = output / name
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(data)
            dest.chmod(0o755 if name in executables else 0o644)
        report = {'schemaVersion':1, 'profile':'usagestat-dev', 'version':package_version, 'target':target,
            'sourceCommit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
            'sourceDirty':bool(subprocess.check_output(['git','status','--porcelain','--untracked-files=no'],cwd=ROOT).strip()),
            'files':[{'path':name,'sha256':digest(data),'size':len(data)} for name,data in sorted(files.items())]}
        (output / 'dev-installation.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    except BaseException:
        # This invocation created the directory exclusively and only wrote its
        # own staged files. Never remove a pre-existing installation on failure.
        shutil.rmtree(output)
        raise
    return report


if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True,help='Fresh user-writable installation directory')
    parser.add_argument('--binary-dir',type=Path,help='Use already-built native workspace binaries')
    parser.add_argument('--target',help='Required with --binary-dir; execution must match this target')
    args=parser.parse_args()
    output=args.output.resolve()
    if output.exists():
        parser.error('choose a fresh --output directory; existing installations are preserved')
    if args.binary_dir:
        if not args.target: parser.error('--binary-dir requires --target')
        directory,target=args.binary_dir.resolve(),args.target
    else:
        directory,target=build()
        if args.target and args.target!=target: parser.error('development staging is native only')
    report=stage(directory,output,target)
    print(json.dumps({'output':str(output),'profile':report['profile'],'version':report['version'],'target':target},indent=2))
