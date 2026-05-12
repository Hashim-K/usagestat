(function () {
  var API_URL = "https://api.augmentcode.com/v1/user/usage";
  var AUTH_PATHS = [
    "~/.augment/auth.json",
    "~/.config/Code/User/globalStorage/augment.augment-vscode/auth.json",
  ];

  function loadToken(ctx) {
    for (var i = 0; i < AUTH_PATHS.length; i++) {
      try {
        if (!ctx.host.fs.exists(AUTH_PATHS[i])) continue;
        var json = ctx.util.tryParseJson(ctx.host.fs.readText(AUTH_PATHS[i]));
        if (!json) continue;
        var t = json.access_token || json.accessToken;
        if (typeof t === "string" && t.trim()) return t.trim();
      } catch (e) {
        ctx.host.log.warn("augment: failed to read " + AUTH_PATHS[i] + ": " + e);
      }
    }
    return null;
  }

  function probe(ctx) {
    var token = loadToken(ctx);
    if (!token) {
      throw "Augment auth token not found. Sign in to Augment Code in VS Code.";
    }

    var result = ctx.util.requestJson({
      method: "GET",
      url: API_URL,
      headers: { Authorization: "Bearer " + token, Accept: "application/json" },
      timeoutMs: 30000,
    });

    if (ctx.util.isAuthStatus(result.resp.status) || !result.resp.status || result.resp.status >= 400) {
      throw "Augment auth expired. Re-sign in to Augment Code.";
    }

    var json = result.json || {};
    var used = (json.used_credits || json.usage || 0);
    var limit = (json.credit_limit || json.limit || 100);
    var usedPct = limit > 0 ? Math.min(100, (used / limit) * 100) : 0;

    var plan = (json.plan || json.subscription || "Augment");
    var lines = [
      ctx.line.progress({
        label: "Credits",
        used: usedPct,
        limit: 100,
        format: { kind: "percent" },
      }),
    ];

    return { plan: plan, lines: lines };
  }

  globalThis.__openusage_plugin = { id: "augment", probe: probe };
})();
