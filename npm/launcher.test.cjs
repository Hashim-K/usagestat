'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const crypto = require('node:crypto');
const { selectTarget, resolveNative } = require('./launcher.cjs');

test('native selection rejects unsupported libc, architecture and old glibc', () => {
  for (const [platform, arch, libc, target] of [
    ['linux', 'x64', '2.39', 'linux-x64-gnu'], ['linux', 'arm64', '2.40', 'linux-arm64-gnu'],
    ['darwin', 'x64', undefined, 'darwin-x64'], ['darwin', 'arm64', undefined, 'darwin-arm64'],
    ['win32', 'x64', undefined, 'win32-x64'],
  ]) assert.equal(selectTarget(platform, arch, libc), target);
  for (const [platform, arch, libc, code] of [
    ['linux', 'x64', undefined, 'unsupported-libc'], ['linux', 'x64', '2.38', 'glibc-too-old'],
    ['linux', 'x64', 'broken', 'glibc-too-old'], ['win32', 'arm64', undefined, 'unsupported-target'],
    ['linux', 'ia32', '2.40', 'unsupported-target'], ['freebsd', 'x64', undefined, 'unsupported-target'],
  ]) assert.throws(() => selectTarget(platform, arch, libc), { code });
});

test('installed package resolution checks exact versions and every resource digest', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'usagestat npm fixture 使用 '));
  try {
    const own = {name: '@fixture/usagestat', version: '1.2.3'};
    const target = selectTarget(process.platform, process.arch, process.platform === 'linux' ? process.report.getReport().header.glibcVersionRuntime : undefined);
    const selected = '@fixture/native';
    const nativeRoot = path.join(root, 'node_modules', '@fixture', 'native');
    const json = (p, value) => fs.writeFileSync(p, JSON.stringify(value));
    json(path.join(root, 'package.json'), own);
    json(path.join(root, 'platforms.json'), {[target]: {package: selected, manifestSha256: 'missing'}});
    assert.throws(() => resolveNative('usagestat', root), {code: 'native-package-missing'});
    fs.mkdirSync(nativeRoot, {recursive: true});
    json(path.join(nativeRoot, 'package.json'), {name: selected, version: '0.0.0'});
    assert.throws(() => resolveNative('usagestat', root), {code: 'native-version-mismatch'});
    json(path.join(nativeRoot, 'package.json'), {name: selected, version: own.version});
    const filename = process.platform === 'win32' ? 'usagestat.exe' : 'usagestat';
    const bytes = Buffer.from('synthetic binary');
    fs.writeFileSync(path.join(nativeRoot, filename), bytes, {mode: 0o755});
    const sha = b => crypto.createHash('sha256').update(b).digest('hex');
    const manifest = {schemaVersion: 1, version: own.version, os: process.platform, arch: process.arch,
      executables: {[filename]: {}}, files: [{path: filename, size: bytes.length, sha256: sha(bytes), executable: true}]};
    json(path.join(nativeRoot, 'native-manifest.json'), manifest);
    const metadata = {[target]: {package: selected, manifestSha256: sha(fs.readFileSync(path.join(nativeRoot, 'native-manifest.json')))}};
    json(path.join(root, 'platforms.json'), metadata);
    assert.equal(resolveNative('usagestat', root), path.join(fs.realpathSync(nativeRoot), filename));
    fs.writeFileSync(path.join(nativeRoot, filename), 'tampered binary');
    assert.throws(() => resolveNative('usagestat', root), {code: 'native-integrity'});
    fs.writeFileSync(path.join(nativeRoot, 'native-manifest.json'), '{}');
    assert.throws(() => resolveNative('usagestat', root), {code: 'native-integrity'});
  } finally { fs.rmSync(root, {recursive: true, force: true}); }
});
