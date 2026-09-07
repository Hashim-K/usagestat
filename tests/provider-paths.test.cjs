const {test} = require('node:test');
const assert = require('node:assert/strict');
const {providerHarness} = require('./provider-harness.cjs');

const quotaXml = '<option name="quotaInfo" value="{&quot;maximum&quot;:100,&quot;current&quot;:25,&quot;available&quot;:75}"/>';
const cloud = {userStatus: {planStatus: {dailyQuotaRemainingPercent: 80, weeklyQuotaRemainingPercent: 60,
  dailyQuotaResetAtUnix: 1785672000, weeklyQuotaResetAtUnix: 1786190400, planInfo: {planName: 'Fixture'}}}};
const variantDirs = {'windsurf': 'Windsurf', 'windsurf-next': 'Windsurf - Next', 'devin-windsurf': 'Devin', 'devin-next-windsurf': 'Devin - Next'};
const jsonResponse = body => ({status: 200, bodyText: JSON.stringify(body)});

for (const platform of ['linux', 'macos', 'windows']) {
  test(`${platform}: Kiro reads native or explicitly selected IDE data`, () => {
    for (const custom of [false, true]) {
      const settings = custom ? {userDataDir: platform === 'windows' ? 'E:\\Separate IDE 使用' : '/separate IDE 使用'} : {};
      const harness = providerHarness('kiro', {platform, settings});
      const app = custom ? settings.userDataDir : harness.ctx.host.fs.appSupportPath('Kiro');
      harness.files.set(harness.normalize('~/.aws/sso/cache/kiro-auth-token.json'), JSON.stringify({refreshToken: 'synthetic-refresh'}));
      harness.databases.set(harness.normalize(app + '/User/globalStorage/state.vscdb'), [{value: JSON.stringify({
        'kiro.resourceNotifications.usageState': {timestamp: Date.parse(harness.ctx.nowIso), usageBreakdowns: [{resourceType: 'CREDIT', currentUsage: 25, usageLimit: 100}]},
      })}]);
      assert.equal(harness.probe().lines[0].used, 25);
      assert.equal(harness.calls.sqlite[0].path, harness.normalize(app + '/User/globalStorage/state.vscdb'));
      assert(harness.calls.files.includes(harness.normalize(app + '/logs')));
      assert(harness.calls.files.includes(harness.normalize(app + '/User/globalStorage/kiro.kiroagent/profile.json')));
      assert.equal(harness.calls.http.length, 0);
      assert.equal(harness.calls.writes.length, 0);
    }
  });

  test(`${platform}: Windsurf and Devin variants stay separate`, () => {
    for (const [variant, appDir] of Object.entries(variantDirs)) {
      const harness = providerHarness('windsurf', {platform, settings: {ideVariant: variant}, http: () => jsonResponse(cloud)});
      // All variants have different accounts, but only the selected one is read.
      for (const [key, directory] of Object.entries(variantDirs)) {
        harness.databases.set(harness.normalize(harness.ctx.host.fs.appSupportPath(directory) + '/User/globalStorage/state.vscdb'), [{value: JSON.stringify({apiKey: 'synthetic-' + key})}]);
      }
      const output = harness.probe();
      assert.deepEqual(Array.from(output.lines, line => line.used), [20, 40]);
      assert.equal(harness.calls.sqlite.length, 1);
      assert.equal(harness.calls.sqlite[0].path, harness.normalize(harness.ctx.host.fs.appSupportPath(appDir) + '/User/globalStorage/state.vscdb'));
      assert.equal(JSON.parse(harness.calls.http[0].bodyText).metadata.apiKey, 'synthetic-' + variant);
    }
    const ambiguous = providerHarness('windsurf', {platform});
    for (const directory of ['Windsurf', 'Devin']) {
      ambiguous.databases.set(ambiguous.normalize(ambiguous.ctx.host.fs.appSupportPath(directory) + '/User/globalStorage/state.vscdb'), [{value: '{"apiKey":"synthetic"}'}]);
    }
    assert.throws(() => ambiguous.probe(), error => /Multiple.*installations/.test(error.message));
    assert.equal(ambiguous.calls.http.length, 0);
  });

  test(`${platform}: JetBrains uses redirected roots and explicit IDE configuration`, () => {
    const harness = providerHarness('jetbrains-ai-assistant', {platform});
    const app = harness.ctx.host.fs.appSupportPath('JetBrains');
    const first = app + '/IntelliJIdea2026.2';
    harness.files.set(harness.normalize(first + '/options/AIAssistantQuotaManager2.xml'), quotaXml);
    assert.equal(harness.probe().lines[0].used, 25);
    harness.files.set(harness.normalize(app + '/PyCharm2026.2/options/AIAssistantQuotaManager2.xml'), quotaXml);
    assert.throws(() => harness.probe(), error => /Multiple.*quota profiles/.test(error.message));
    harness.ctx.provider.settings.configDir = first;
    assert.equal(harness.probe().lines[0].used, 25);
    harness.ctx.provider.settings.configDir = app + '/missing profile';
    assert.throws(() => harness.probe(), error => /not detected/.test(String(error)));
    assert.equal(harness.calls.http.length, 0);
  });
}
