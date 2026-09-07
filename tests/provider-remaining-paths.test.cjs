const {test} = require('node:test');
const assert = require('node:assert/strict');
const {providerHarness} = require('./provider-harness.cjs');
const json = body => ({status: 200, bodyText: JSON.stringify(body)});
const quota = {userStatus: {planStatus: {dailyQuotaRemainingPercent: 80, weeklyQuotaRemainingPercent: 60, planInfo: {planName: 'Fixture'}}}};
const models = {models: {fixture: {displayName: 'Gemini Pro', quotaInfo: {remainingFraction: 0.75}}}};
function protoField(id, value) {
  const data = Buffer.isBuffer(value) ? value : Buffer.from(value);
  assert(data.length < 128);
  return Buffer.concat([Buffer.from([id * 8 + 2, data.length]), data]);
}
function oauthRow(token = 'synthetic-access', refresh = 'synthetic-refresh') {
  const inner = Buffer.concat([protoField(1, token), protoField(3, refresh)]).toString('base64');
  return [{value: protoField(1, Buffer.concat([protoField(1, 'oauthTokenInfoSentinelKey'), protoField(2, protoField(1, inner))])).toString('base64')}];
}
for (const platform of ['linux', 'macos', 'windows']) {
  test(`${platform}: Devin native CLI roots, explicit files and IDE accounts are isolated`, () => {
    const h = providerHarness('devin', {platform, http: () => json(quota)});
    const credentials = platform === 'windows' ? h.ctx.host.fs.localAppDataPath('devin/credentials.toml') : '~/.local/share/devin/credentials.toml';
    h.files.set(h.normalize(credentials), 'windsurf_api_key = "synthetic-cli"');
    assert.deepEqual(Array.from(h.probe().lines, x => x.used), [20, 40]);
    h.databases.set(h.normalize(h.ctx.host.fs.appSupportPath('Devin/User/globalStorage/state.vscdb')), [{value: '{"apiKey":"synthetic-ide"}'}]);
    const before = h.calls.http.length;
    assert.throws(() => h.probe(), e => /Multiple Devin accounts/.test(e.message));
    assert.equal(h.calls.http.length, before);
    h.ctx.provider.settings = {ideVariant: 'devin'};
    assert.deepEqual(Array.from(h.probe().lines, x => x.used), [20, 40]);
    assert.equal(JSON.parse(h.calls.http.at(-1).bodyText).metadata.apiKey, 'synthetic-ide');
    h.ctx.provider.settings = {credentialsPath: h.home + '/missing 使用.toml'};
    assert.throws(() => h.probe(), e => /login/.test(String(e)));
    assert.equal(h.calls.http.length, before + 1);
    // A malformed selected file cannot fall through to the other IDE account.
    h.files.set(h.normalize(h.ctx.provider.settings.credentialsPath), 'garbage');
    assert.throws(() => h.probe(), e => e.code === 'credential-malformed');
    h.ctx.provider.settings = {authSource: 'cli'};
    h.ctx.host.http.request = request => { h.calls.http.push(request); return {status: 401, bodyText: '{}'}; };
    const queries = h.calls.sqlite.length;
    assert.throws(() => h.probe(), e => /login/.test(String(e)));
    assert.equal(h.calls.sqlite.length, queries);
    if (platform === 'windows') assert(!h.calls.files.some(p => p.includes('.local')));
  });

  test(`${platform}: Antigravity selects native database and rejects other profiles and stale cache`, () => {
    const h = providerHarness('antigravity', {platform, settings: {ideVariant: 'antigravity'}, http: () => json(models),
      host: {ls: {discoverStatus() { throw new Error('Explicit database unexpectedly used process discovery'); }}}});
    h.ctx.app.pluginDataDir = h.home + '/isolated plugin state';
    const standard = h.ctx.host.fs.appSupportPath('Antigravity/User/globalStorage/state.vscdb');
    const ide = h.ctx.host.fs.appSupportPath('Antigravity IDE/User/globalStorage/state.vscdb');
    h.databases.set(h.normalize(standard), oauthRow());
    h.databases.set(h.normalize(ide), oauthRow('other-account', 'other-refresh'));
    h.files.set(h.normalize(h.ctx.app.pluginDataDir + '/auth.json'), JSON.stringify({accessToken: 'stale-other-account', expiresAtMs: Date.now() + 3600000}));
    assert.equal(h.probe().lines[0].used, 25);
    assert(h.calls.http.every(request => request.headers.Authorization === 'Bearer synthetic-access'));
    assert.equal(h.calls.sqlite.length, 1);
    // No selected profile: both native databases are found and ambiguity is explicit.
    h.ctx.provider.settings = {};
    h.ctx.host.ls = {discoverStatus: () => ({status: 'missing'})};
    const before = h.calls.http.length;
    assert.throws(() => h.probe(), e => /Multiple Antigravity credential/.test(e.message));
    assert.equal(h.calls.http.length, before);
    h.ctx.provider.settings = {userDataDir: h.home + '/missing profile'};
    assert.throws(() => h.probe(), e => /Start Antigravity/.test(String(e)));
    assert.equal(h.calls.http.length, before);
    // A selected account's failed auth cannot use the other profile's cache/keychain.
    h.ctx.provider.settings = {ideVariant: 'antigravity'};
    h.ctx.host.keychain = {readGenericPassword() { throw new Error('Other account queried'); }};
    h.ctx.host.http.request = request => {h.calls.http.push(request); return {status: 401, bodyText: '{}'};};
    assert.throws(() => h.probe(), e => /Start Antigravity/.test(String(e)));
    assert(!h.calls.http.slice(before).some(request => request.headers.Authorization === 'Bearer stale-other-account'));
    h.ctx.provider.settings = {};
    h.ctx.host.ls = {discoverStatus: () => ({status: 'ambiguous'})};
    assert.throws(() => h.probe(), e => /Multiple Antigravity processes/.test(e.message));
  });

  test(`${platform}: Perplexity cache reader reports its actual platform limitation`, () => {
    const h = providerHarness('perplexity', {platform, settings: {cacheDbPath: 'selected-cache.db'}});
    if (platform === 'macos') {
      assert.throws(() => h.probe(), e => /Not logged in/.test(String(e)));
      assert.deepEqual(h.calls.files, [h.normalize('selected-cache.db')]);
    } else {
      assert.throws(() => h.probe(), e => e.code === 'unsupported' && /CFNetwork/.test(e.message));
      assert.equal(h.calls.files.length, 0);
    }
    assert.equal(h.calls.http.length, 0);
  });
}
