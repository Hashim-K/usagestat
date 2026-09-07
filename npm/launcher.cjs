'use strict';

const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');

function failure(code, message) {
  const error = new Error(message);
  error.code = code;
  throw error;
}

function selectTarget(platform, architecture, glibc) {
  if (platform === 'linux') {
    if (!glibc) failure('unsupported-libc', 'This package requires glibc 2.39 or newer; musl/Alpine is not supported.');
    const [major, minor] = glibc.split('.').map(Number);
    if (!(major > 2 || (major === 2 && minor >= 39))) {
      failure('glibc-too-old', `Detected glibc ${glibc}; the native backend requires glibc 2.39 or newer.`);
    }
  }
  const key = `${platform}-${architecture}${platform === 'linux' ? '-gnu' : ''}`;
  if (!['linux-x64-gnu', 'linux-arm64-gnu', 'darwin-x64', 'darwin-arm64', 'win32-x64'].includes(key)) {
    failure('unsupported-target', `No native backend is available for ${platform}/${architecture}.`);
  }
  return key;
}

function sha256(bytes) { return crypto.createHash('sha256').update(bytes).digest('hex'); }

function resolveNative(command, packageRoot = __dirname) {
  if (!['usagestat', 'usagestatd'].includes(command)) failure('invalid-command', 'Unknown backend command.');
  if (Number(process.versions.node.split('.')[0]) < 24) failure('node-too-old', 'Use Node.js 24 or newer.');
  const own = JSON.parse(fs.readFileSync(path.join(packageRoot, 'package.json'), 'utf8'));
  const platforms = JSON.parse(fs.readFileSync(path.join(packageRoot, 'platforms.json'), 'utf8'));
  const glibc = process.platform === 'linux' ? process.report.getReport().header.glibcVersionRuntime : undefined;
  const target = selectTarget(process.platform, process.arch, glibc);
  const selected = platforms[target];
  if (!selected) failure('target-not-qualified', `This release does not include a qualified ${target} backend. Use a supported release channel.`);
  if (process.platform === 'darwin' && Number(os.release().split('.')[0]) < 22) {
    failure('macos-too-old', 'This npm wrapper uses Node.js 24, which requires macOS 13.5 or newer.');
  }
  if (process.platform === 'win32' && Number(os.release().split('.')[0]) < 10) {
    failure('windows-too-old', 'The native backend requires Windows 10 / Server 2016 or newer.');
  }
  let manifestPath;
  try { manifestPath = require.resolve(`${selected.package}/package.json`, { paths: [packageRoot] }); }
  catch { failure('native-package-missing', `Missing ${selected.package}@${own.version}. Reinstall ${own.name} with optional dependencies enabled (--include=optional). Installation scripts are not required.`); }
  const nativeRoot = fs.realpathSync(path.dirname(manifestPath));
  const nativePackage = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  if (nativePackage.name !== selected.package || nativePackage.version !== own.version) {
    failure('native-version-mismatch', 'CLI and native payload versions differ. Reinstall the exact main-package version with optional dependencies enabled.');
  }
  const manifestBytes = fs.readFileSync(path.join(nativeRoot, 'native-manifest.json'));
  if (sha256(manifestBytes) !== selected.manifestSha256) failure('native-integrity', 'Native manifest integrity failed. Reinstall this package.');
  const native = JSON.parse(manifestBytes);
  if (native.schemaVersion !== 1 || native.version !== own.version || native.os !== process.platform || native.arch !== process.arch) {
    failure('native-metadata-mismatch', 'The installed native payload does not match this version and platform.');
  }
  for (const item of native.files) {
    if (!item.path || item.path.includes('\\') || item.path.includes(':') || item.path.split('/').some(p => ['', '.', '..'].includes(p))) {
      failure('native-integrity', 'Invalid native resource path. Reinstall this package.');
    }
    const file = path.join(nativeRoot, ...item.path.split('/'));
    const stat = fs.lstatSync(file);
    if (!stat.isFile() || !fs.realpathSync(file).startsWith(nativeRoot + path.sep) || stat.size !== item.size || sha256(fs.readFileSync(file)) !== item.sha256) {
      failure('native-integrity', 'Native binary or resource integrity failed. Reinstall this package.');
    }
    if (item.executable && process.platform !== 'win32' && !(stat.mode & 0o111)) {
      failure('native-not-executable', 'Native executable permissions are missing. Reinstall this package.');
    }
  }
  const filename = command + (process.platform === 'win32' ? '.exe' : '');
  if (!native.executables[filename]) failure('native-integrity', 'Native executable is absent from the manifest.');
  return path.join(nativeRoot, filename);
}

function launch(command) {
  try {
    const binary = resolveNative(command);
    // Native siblings/resources remain together. The CLI therefore registers
    // the real daemon path, never this Node wrapper or an interactive PATH shim.
    const child = spawn(binary, process.argv.slice(2), { stdio: 'inherit', shell: false });
    const handlers = new Map();
    for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
      const handler = () => {
        if (child.exitCode !== null || child.signalCode !== null) return;
        // Windows console events already reach the native child. Node's kill
        // emulation is an abrupt termination, so allow console cleanup first.
        if (process.platform === 'win32' && signal !== 'SIGTERM') {
          const timer = setTimeout(() => { if (child.exitCode === null && child.signalCode === null) child.kill(); }, 5000);
          timer.unref();
        } else { child.kill(signal); }
      };
      handlers.set(signal, handler);
      process.on(signal, handler);
    }
    const cleanup = () => { for (const [signal, handler] of handlers) process.removeListener(signal, handler); };
    child.once('error', error => {
      cleanup();
      process.stderr.write(`usagestat npm: native-start-failed: ${error.code || 'unknown'}. Reinstall the matching native package.\n`);
      process.exitCode = 1;
    });
    child.once('exit', (code, signal) => {
      cleanup();
      if (signal && process.platform !== 'win32') process.kill(process.pid, signal);
      else process.exitCode = code === null ? 1 : code;
    });
  } catch (error) {
    // Do not dump JSON/native files or inherited environment on failure.
    const known = error.code && !['ENOENT', 'EACCES', 'EPERM'].includes(error.code);
    process.stderr.write(`usagestat npm: ${known ? error.code : 'native-package-unusable'}: ${known ? error.message : 'Native package files are missing or inaccessible; reinstall with optional dependencies enabled.'}\n`);
    process.exitCode = 1;
  }
}

module.exports = { launch, resolveNative, selectTarget };
