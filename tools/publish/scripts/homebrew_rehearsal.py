#!/usr/bin/env python3
"""Install/upgrade/remove one disposable Homebrew formula on a hosted macOS runner."""
import argparse
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import uuid
from homebrew_formula import generate


def check(directory):
    if platform.system() != 'Darwin' or os.environ.get('GITHUB_ACTIONS') != 'true':
        raise ValueError('Homebrew mutation rehearsal requires a disposable hosted macOS CI runner')
    brew = shutil.which('brew')
    if not brew:
        raise ValueError('Native Homebrew installation is required')
    env = {**os.environ, 'HOMEBREW_NO_AUTO_UPDATE': '1', 'HOMEBREW_NO_INSTALL_CLEANUP': '1',
           'HOMEBREW_NO_ANALYTICS': '1', 'HOMEBREW_NO_ENV_HINTS': '1',
           'HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK': '1'}
    def run(*args, capture=False):
        result = subprocess.run([brew, *args], env=env, check=True, timeout=300,
                                stdout=subprocess.PIPE if capture else None, text=True)
        return result.stdout.strip() if capture else None
    prefix = Path(run('--prefix', capture=True))
    for binary in ('usagestat', 'usagestatd'):
        if (prefix / 'bin' / binary).exists() or (prefix / 'bin' / binary).is_symlink():
            raise ValueError('Rehearsal will not replace an existing backend installation')
    suffix = uuid.uuid4().hex[:12]
    tap = 'usagestat-fixture/native-' + suffix
    name = 'usagestat-fixture-' + suffix
    formula_id = tap + '/' + name
    formula = generate(directory, rehearsal=True, formula_name=name)
    created = False
    with tempfile.TemporaryDirectory(prefix='usagestat brew profile 使用 ') as temporary:
        profile = Path(temporary)
        env['XDG_CONFIG_HOME'] = str(profile / 'homebrew-config')
        config = profile / 'config'; data = profile / 'data'
        config.mkdir(); data.mkdir()
        (config / 'retained-fixture').write_text('synthetic configuration')
        (data / 'retained-fixture').write_text('synthetic history')
        runtime_env = {**env, 'USAGESTAT_CONFIG_DIR': str(config), 'USAGESTAT_DATA_DIR': str(data)}
        # Do not pass the synthetic HOME to Homebrew itself; it needs its runner installation.
        runtime_env.pop('USAGESTAT_PLUGIN_DIR', None)
        runtime_env.pop('AI_USAGE_PLUGIN_DIR', None)
        try:
            run('tap-new', '--no-git', tap)
            created = True
            tap_dir = Path(run('--repository', tap, capture=True))
            recipe = tap_dir / 'Formula' / (name + '.rb')
            recipe.write_text(formula, encoding='utf-8')
            subprocess.run(['ruby', '-c', str(recipe)], check=True, timeout=30)
            # New Homebrew releases require trust for local tap code. Scope it
            # to this generated formula and a disposable configuration directory.
            if subprocess.run([brew, 'command', 'trust'], env=env, stdout=subprocess.DEVNULL,
                              stderr=subprocess.DEVNULL, timeout=30).returncode == 0:
                run('trust', '--formula', formula_id)
            run('install', '--formula', '--build-from-source', formula_id)
            run('test', formula_id)
            installed = prefix / 'opt' / name
            assert installed.resolve() == Path(run('--prefix', formula_id, capture=True)).resolve()
            assert (prefix / 'bin/usagestat').resolve() == (installed / 'bin/usagestat').resolve()
            first_keg = installed.resolve()
            def inspect():
                result = subprocess.check_output([str(installed / 'bin/usagestat'), '--json', 'list'],
                    env=runtime_env, cwd=profile, text=True, timeout=30)
                providers = json.loads(result)
                assert len(providers) == 61
                assert all(Path(p['icon']['path']).is_file() for p in providers if (p.get('icon') or {}).get('path'))
                assert (installed / 'share/usagestat/LICENSE').is_file()
                report = json.loads(subprocess.check_output([str(installed / 'bin/usagestat'), 'daemon', 'status', '--json'],
                    env=runtime_env, cwd=profile, text=True, timeout=30))
                assert not report['configured'] and not report['registered'], 'Install must not register startup'
                assert (config / 'retained-fixture').read_text() == 'synthetic configuration'
                assert (data / 'retained-fixture').read_text() == 'synthetic history'
            inspect()
            run('install', '--formula', formula_id)
            # A formula revision exercises a genuine versioned keg replacement,
            # using exactly the same verified payload instead of inventing a binary version.
            recipe.write_text(formula.replace('  license "MIT"', '  revision 1\n  license "MIT"'), encoding='utf-8')
            run('upgrade', '--formula', formula_id)
            assert installed.resolve() != first_keg
            run('test', formula_id)
            inspect()
            run('uninstall', '--formula', '--force', formula_id)
            assert not (prefix / 'bin/usagestat').exists()
            assert not (prefix / 'bin/usagestatd').exists()
            assert (config / 'retained-fixture').read_text() == 'synthetic configuration'
            assert (data / 'retained-fixture').read_text() == 'synthetic history'
        finally:
            if created:
                subprocess.run([brew, 'uninstall', '--formula', '--force', formula_id], env=env,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=120)
                run('untap', tap)
    return {'checks': ['native-formula-install-both-binaries-and-resources', 'durable-linked-cli-discovery',
                       'install-twice', 'versioned-keg-revision-upgrade', 'no-implicit-startup',
                       'uninstall-retains-user-data'], 'activeDaemonUpgrade': 'pending', 'signedDistribution': 'pending'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifacts', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    result = check(args.artifacts.resolve())
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
