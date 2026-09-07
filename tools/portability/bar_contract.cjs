#!/usr/bin/env node
'use strict';
// Execute the pinned bar's actual CLI call and normalization code against the
// native backend. The Node facade supplies process/filesystem APIs, not a UI.
const assert = require('node:assert/strict');
const child = require('node:child_process');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const vm = require('node:vm');

async function check(cli) {
  const repository = path.resolve(__dirname, '../..');
  const fixtures = path.join(repository, 'tests/fixtures/bar-client');
  const source = JSON.parse(fs.readFileSync(path.join(fixtures, 'source.json'), 'utf8'));
  for (const [name, hash] of Object.entries(source.files)) {
    assert.equal(crypto.createHash('sha256').update(fs.readFileSync(path.join(fixtures, name))).digest('hex'), hash);
  }
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'usagestat bar 使用 # '));
  const calls = [];
  const env = Object.fromEntries(Object.entries(process.env).filter(([name]) => !/^(?:USAGESTAT_|AI_USAGE_)/i.test(name)));
  for (const name of ['HOME', 'USERPROFILE', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'APPDATA', 'LOCALAPPDATA', 'USAGESTAT_CONFIG_DIR', 'USAGESTAT_DATA_DIR']) {
    env[name] = path.join(root, name); fs.mkdirSync(env[name]);
  }
  delete env.DBUS_SESSION_BUS_ADDRESS;
  delete env.XDG_RUNTIME_DIR;
  if (process.platform === 'win32') delete env.HOME;
  const timers = new Set();
  const processes = new Set();
  try {
    const pluginDir = path.join(root, 'plugins 使用');
    const configFile = path.join(root, 'selected config 使用.toml');
    fs.writeFileSync(configFile, 'pluginDirs = [' + JSON.stringify(path.join(repository, 'plugins')) + ']\n');
    const scripts = {
      'bar-ok': `return {displayName:'Fixture account', plan:'Fixture plan', source:ctx.sourceMode,
        metrics:[{type:'progress',label:'Session',used:1,limit:4,format:{kind:'count'},resetsAt:'2026-10-01T12:00:00Z',periodDurationMs:300000},
                 {type:'text',label:'Account',value:'Synthetic'}, {type:'badge',label:'Status',text:'Ready',color:'green'}]};`,
      'bar-empty': 'return {metrics:[], plan:"Fixture empty"};',
      'bar-denied': 'throw {code:"missing-auth", message:"Synthetic sign-in required"};',
      'bar-unsupported': 'throw {code:"unsupported", message:"Synthetic method unavailable on this platform"};',
    };
    for (const [id, script] of Object.entries(scripts)) {
      const directory = path.join(pluginDir, id); fs.mkdirSync(directory, {recursive: true});
      fs.writeFileSync(path.join(directory, 'plugin.json'), JSON.stringify({id, name:id, entry:'plugin.js', supportedModes:['api'], autoMode:'api', enabledByDefault:true}));
      fs.writeFileSync(path.join(directory, 'plugin.js'), `globalThis.__usagestat_plugin={probe:function(ctx){${script}}};`);
    }
    const Gio = {SubprocessFlags:{STDOUT_PIPE:1, STDERR_PIPE:2}, Subprocess:{new(argv) {
      assert.equal(argv[0], cli, 'Fixture must use the explicit native executable');
      calls.push(argv);
      const proc = child.spawn(argv[0], argv.slice(1), {env, cwd:root, windowsHide:true, shell:false});
      processes.add(proc);
      let stdout = '', stderr = '', failure;
      proc.stdout.on('data', data => {stdout += data; if (stdout.length > 1024 * 1024) proc.kill();});
      proc.stderr.on('data', data => {stderr += data; if (stderr.length > 1024 * 1024) proc.kill();});
      proc.on('error', error => {failure = error;});
      const finished = new Promise(resolve => proc.on('close', () => {processes.delete(proc); resolve();}));
      return {
        communicate_utf8_async(_input, _cancel, callback) {finished.then(() => callback(this, null));},
        communicate_utf8_finish() {if (failure) throw failure; return [true, stdout, stderr];},
        get_if_exited() {return proc.exitCode !== null;}, get_exit_status() {return proc.exitCode;},
        get_term_sig() {return 15;}, force_exit() {proc.kill();},
      };
    }}};
    const date = offset => ({add_days: n => date(offset + n), format() {
      const now = new Date(); now.setDate(now.getDate() + offset);
      return `${now.getFullYear()}-${String(now.getMonth()+1).padStart(2,'0')}-${String(now.getDate()).padStart(2,'0')}`;
    }});
    const GLib = {FileTest:{IS_EXECUTABLE:1}, PRIORITY_DEFAULT:0, SOURCE_REMOVE:false,
      file_test: filename => fs.existsSync(filename), getenv:name => env[name], get_home_dir:() => root,
      DateTime:{new_now_local:() => date(0)},
      timeout_add(_priority, ms, callback) {const timer = setTimeout(callback, ms); timers.add(timer); return timer;},
      source_remove(timer) {clearTimeout(timer); timers.delete(timer);},
    };
    const sandbox = vm.createContext({Gio, GLib, TextEncoder, TextDecoder});
    const script = ['provider-id.js', 'cli.js'].map(name => fs.readFileSync(path.join(fixtures, name), 'utf8')
      .replace(/^import .*;\n/gm, '').replace(/^export /gm, '')).join('\n');
    vm.runInContext(script + '\nglobalThis.client={fetchProviderUsage,fetchProviderManifests,normalizeCostSummary,providerBaseId};', sandbox);
    const client = sandbox.client;
    const options = {cliPath:cli, pluginDir, configFile};
    const manifests = await client.fetchProviderManifests(null, options);
    for (const id of Object.keys(scripts)) assert(manifests.some(provider => provider.id === id));
    for (const provider of manifests) {
      const icon = provider.icon?.path;
      if (icon) assert(path.isAbsolute(icon) && fs.statSync(icon).isFile());
    }
    assert.equal(client.providerBaseId({id:'opencodego'}), 'opencode-go');
    const snapshot = await client.fetchProviderUsage({id:'bar-ok', source:'api'}, null, options);
    assert.equal(snapshot.provider, 'bar-ok');
    assert.equal(snapshot.displayName, 'Fixture account');
    assert.equal(snapshot.plan, 'Fixture plan');
    assert.equal(snapshot.source, 'api');
    assert.equal(snapshot.usage.primary.usedPercent, 25);
    assert.equal(snapshot.usage.primary.windowMinutes, 5);
    assert.equal(snapshot.usage.primary.resetsAt, '2026-10-01T12:00:00Z');
    assert.equal(snapshot.usage.extraTextLines[0].value, 'Synthetic');
    assert.equal(snapshot.usage.badges[0].text, 'Ready');
    assert.equal(snapshot.rawMetrics.length, 3);
    const empty = await client.fetchProviderUsage({id:'bar-empty', source:'api'}, null, options);
    assert.equal(empty.plan, 'Fixture empty');
    assert.equal(empty.usage.primary, undefined);
    for (const [id, state, message] of [['bar-denied','missing-auth','Synthetic sign-in required'],
      ['bar-unsupported','unsupported','Synthetic method unavailable']]) {
      // The baseline backend represents probe failures as Error badge metrics.
      // The additive state survives the old client's spread/normalization.
      const failed = await client.fetchProviderUsage({id, source:'api'}, null, options);
      assert.equal(failed.state, state);
      assert.equal(failed.source, 'error');
      assert.equal(failed.usage.primary, undefined);
      assert.equal(failed.usage.badges[0].label, 'Error');
      assert(failed.usage.badges[0].text.includes(message));
    }
    await assert.rejects(client.fetchProviderUsage({id:'bar-ok'}, null, {...options,cliPath:path.join(root,'missing-cli')}), /CLI was not found/);
    // The existing bar treats provider cost as optional. Its failed cost request
    // must not discard a valid quota snapshot.
    assert(calls.some(argv => argv.includes('cost') && argv.includes('bar-ok')));
    for (const argv of calls) {
      assert.equal(argv[1], '--json');
      assert.equal(argv[argv.indexOf('--config') + 1], configFile);
      assert.equal(argv[argv.indexOf('--plugin-dir') + 1], pluginDir);
      if (argv.includes('usage')) assert.equal(argv[argv.indexOf('--source') + 1], 'api');
    }
    const cost = client.normalizeCostSummary({currency:'EUR',periodDays:7,totals:{totalCost:2.5,totalTokens:1250}});
    assert.equal(cost.lines[2].cost, 2.5);
    assert.equal(cost.lines[2].tokens, 1250);
    assert.equal(cost.currency, 'EUR');
    assert.equal(client.normalizeCostSummary({totals:{totalCost:0,totalTokens:0}}), null);
    return {barCommit:source.commit, checks:['actual-client-list-and-argument-contract','actual-client-native-usage-normalization',
      'selected-config-source-and-unicode-plugin-root','empty-usage-without-fabricated-quota',
      'missing-auth-and-unsupported-errors','optional-cost-failure-retains-quota','pinned-client-cost-normalization'],
      nativeFrontend:'pending-usagestat-bar-14-and-15'};
  } finally {
    for (const timer of timers) clearTimeout(timer);
    for (const proc of processes) proc.kill();
    fs.rmSync(root, {recursive:true,force:true});
  }
}

const cli = process.argv[2];
if (!cli || !path.isAbsolute(cli)) throw new Error('Pass an absolute native CLI path');
check(cli).then(report => process.stdout.write(JSON.stringify(report, null, 2) + '\n'))
  .catch(error => {process.stderr.write(String(error.stack || error) + '\n'); process.exitCode = 1;});
