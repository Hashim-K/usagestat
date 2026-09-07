const {test} = require('node:test');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const {providerHarness} = require('./provider-harness.cjs');

test('Claude explicit macOS profile reads only its NFC-hashed current-user service', () => {
  const profile = '/profile Cafe\u0301 使用 ';
  const services = [];
  const harness = providerHarness('claude', {platform: 'macos', env: {CLAUDE_CONFIG_DIR: profile}, keychain: {
    readGenericPasswordForCurrentUser(service) { services.push(service); throw new Error('credential-missing: fixture'); },
    readGenericPassword() { assert.fail('Unscoped legacy account lookup'); },
  }});
  harness.ctx.sourceMode = 'oauth';
  assert.throws(() => harness.probe(), error => /Not logged in/.test(String(error)));
  assert.deepEqual(services, ['Claude Code-credentials-' + crypto.createHash('sha256').update(profile.normalize('NFC')).digest('hex').slice(0, 8)]);
});

test('Claude denied or locked macOS credentials remain errors when no profile fallback exists', () => {
  for (const code of ['credential-denied', 'credential-unavailable', 'credential-account-mismatch']) {
    const harness = providerHarness('claude', {platform: 'macos', keychain: {
      readGenericPasswordForCurrentUser() { throw new Error(code + ': fixture'); },
      readGenericPassword() { assert.fail('Unscoped legacy account lookup'); },
    }});
    assert.throws(() => harness.probe(), error => String(error).includes(code));
    assert.equal(harness.calls.http.length, 0);
  }
});

for (const platform of ['linux', 'windows']) {
  test(`${platform}: Claude file-based auth does not depend on an unrelated OS credential store`, () => {
    let keychainReads = 0;
    const harness = providerHarness('claude', {platform, keychain: {
      readGenericPasswordForCurrentUser() { keychainReads++; throw new Error('unexpected store'); },
      readGenericPassword() { keychainReads++; throw new Error('unexpected store'); },
    }});
    harness.ctx.sourceMode = 'oauth';
    assert.throws(() => harness.probe(), error => /Not logged in/.test(String(error)));
    assert(harness.calls.files.includes(harness.normalize('~/.claude/.credentials.json')));
    assert.equal(keychainReads, 0);
  });
}

for (const platform of ['linux', 'macos', 'windows']) {
  for (const id of ['cursor', 'cursor-nightly']) {
    test(`${platform}: ${id} cannot bypass a missing explicitly selected native database`, () => {
      let keychainReads = 0;
      const harness = providerHarness(id, {platform, host: {cursorPaths: {resolveStateDb: () => null, sharedCredentialsAllowed: false}},
        keychain: {readGenericPassword() { keychainReads++; return null; }}});
      for (const app of ['Cursor', 'Cursor Nightly']) {
        harness.databases.set(harness.normalize(harness.ctx.host.fs.appSupportPath(app) + '/User/globalStorage/state.vscdb'), [{value: 'other-account'}]);
      }
      assert.throws(() => harness.probe(), error => /Not logged in/.test(String(error)));
      assert.equal(harness.calls.sqlite.length, 0);
      assert.equal(harness.calls.http.length, 0);
      assert.equal(keychainReads, 0);
    });
    test(`${platform}: ${id} uses database auth without consulting a different keychain account`, () => {
      let selected;
      let keychainReads = 0;
      const harness = providerHarness(id, {platform, host: {
        cursorPaths: {resolveStateDb: () => selected, sharedCredentialsAllowed: true},
        sqlite: {query(database, sql) { assert.equal(database, selected); return JSON.stringify(sql.includes('accessToken') ? [{value: 'synthetic-selected-access'}] : []); }},
      }, keychain: {readGenericPassword() { keychainReads++; return 'different-account-token'; }},
      http(request) { assert.equal(request.headers.Authorization, 'Bearer synthetic-selected-access'); throw new Error('End fixture before an external request'); }});
      selected = harness.ctx.host.fs.appSupportPath(id === 'cursor' ? 'Cursor' : 'Cursor Nightly') + '/User/globalStorage/state.vscdb';
      assert.throws(() => harness.probe(), error => /Usage request failed/.test(String(error)));
      assert.equal(harness.calls.http.length, 1);
      assert.equal(keychainReads, 0);
    });
  }
}
