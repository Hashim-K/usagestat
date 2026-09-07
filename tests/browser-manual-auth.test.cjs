const {test} = require('node:test');
const assert = require('node:assert/strict');
const {providerHarness} = require('./provider-harness.cjs');
for (const platform of ['linux', 'macos', 'windows']) {
  test(`${platform}: T3 Chat manual cookie and full-cURL context survive a probe`, () => {
    for (const capture of [false, true]) {
      const h = providerHarness('t3chat', {platform, http: () => ({status: 200, bodyText: JSON.stringify({usageFourHourPercentage: 20, usageMonthPercentage: 40})})});
      const credential = capture
        ? "curl 'https://t3.chat/api/trpc/getCustomerData' -H 'Cookie: session=synthetic-only' -H 'User-Agent: Fixture Browser' -H 'x-client-context: fixture' -H 'Authorization: must-not-forward'"
        : 'session=synthetic-only';
      h.ctx.provider.cookieHeader = credential;
      assert.deepEqual(Array.from(h.probe().lines, line => line.used), [20, 40]);
      const request = h.calls.http[0];
      assert.equal(request.headers.Cookie, 'session=synthetic-only');
      if (capture) {
        assert.equal(request.headers['User-Agent'], 'Fixture Browser');
        assert.equal(request.headers['x-client-context'], 'fixture');
      }
      assert.equal(request.headers.Authorization, undefined);
      assert.equal(h.ctx.provider.cookieHeader, credential);
      assert.equal(h.calls.writes.length, 0);
      assert(!JSON.stringify(h.calls.logs).includes('synthetic-only'));
      h.ctx.host.http.request = () => ({status: 403, bodyText: 'Vercel challenge', headers: {}});
      assert.throws(() => h.probe(), error => /full browser cURL/.test(String(error)));
      assert.equal(h.ctx.provider.cookieHeader, credential);
      h.ctx.host.http.request = () => ({status: 401, bodyText: '{}', headers: {}});
      assert.throws(() => h.probe(), error => /invalid or expired/.test(String(error)));
      assert.equal(h.ctx.provider.cookieHeader, credential);
    }
  });
}
