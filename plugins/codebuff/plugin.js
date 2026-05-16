(function () {
  var API_BASE = "https://www.codebuff.com";
  var CRED_PATHS = [
    "~/.config/manicode/credentials.json",
  ];

  function loadApiKey(ctx) {
    var v = ctx.host.env.get("CODEBUFF_API_KEY");
    if (typeof v === "string" && v.trim()) return v.trim();

    for (var i = 0; i < CRED_PATHS.length; i++) {
      try {
        if (!ctx.host.fs.exists(CRED_PATHS[i])) continue;
        var json = ctx.util.tryParseJson(ctx.host.fs.readText(CRED_PATHS[i]));
        if (!json) continue;
        var keys = ["apiKey", "api_key", "token", "accessToken", "access_token"];
        for (var j = 0; j < keys.length; j++) {
          var t = json[keys[j]];
          if (typeof t === "string" && t.trim()) return t.trim();
        }
      } catch (e) {
        ctx.host.log.warn("codebuff: failed to read credentials: " + e);
      }
    }
    return null;
  }

  function numberAt(obj, keys) {
    for (var i = 0; i < keys.length; i++) {
      var v = obj && obj[keys[i]];
      if (typeof v === "number") return v;
      if (typeof v === "string") { var n = parseFloat(v); if (!isNaN(n)) return n; }
    }
    return null;
  }

  function strAt(obj, keys) {
    for (var i = 0; i < keys.length; i++) {
      if (obj && typeof obj[keys[i]] === "string") return obj[keys[i]];
    }
    return null;
  }

  function probe(ctx) {
    var apiKey = loadApiKey(ctx);
    if (!apiKey) {
      throw "Codebuff API key not found. Set CODEBUFF_API_KEY or sign in with Codebuff/Manicode.";
    }

    var usageResult = ctx.util.requestJson({
      method: "POST",
      url: API_BASE + "/api/v1/usage",
      headers: {
        Authorization: "Bearer " + apiKey,
        "Content-Type": "application/json",
        Accept: "application/json",
      },
      bodyText: JSON.stringify({ fingerprintId: "usagestat-probe" }),
      timeoutMs: 30000,
    });

    if (ctx.util.isAuthStatus(usageResult.resp.status)) {
      throw "Codebuff API key invalid or expired.";
    }
    if (usageResult.resp.status < 200 || usageResult.resp.status >= 300) {
      throw "Codebuff API error (HTTP " + usageResult.resp.status + ").";
    }

    var usage = usageResult.json || {};
    var usageData = usage.data || usage;

    var subResult = ctx.util.requestJson({
      method: "GET",
      url: API_BASE + "/api/user/subscription",
      headers: { Authorization: "Bearer " + apiKey, Accept: "application/json" },
      timeoutMs: 10000,
    });
    var sub = (subResult.json && (subResult.json.data || subResult.json)) || null;

    var used = numberAt(usageData, ["usage", "used"]) || 0;
    var remaining = numberAt(usageData, ["remainingBalance", "remaining"]);
    var total = numberAt(usageData, ["creditsTotal", "quota", "limit"]);
    if (total === null && remaining !== null) total = used + remaining;

    var pct = total && total > 0 ? Math.min(100, used / total * 100) : (remaining !== null ? 100 : 0);

    var lines = [];
    var progressOpts = {
      label: "Credits",
      used: pct,
      limit: 100,
      format: { kind: "percent" },
    };
    var resetAt = strAt(usageData, ["next_quota_reset", "nextQuotaReset"]);
    if (resetAt) { var iso = ctx.util.toIso(resetAt); if (iso) progressOpts.resetsAt = iso; }
    lines.push(ctx.line.progress(progressOpts));

    var detail = total !== null
      ? (used.toFixed(0) + "/" + total.toFixed(0) + " credits")
      : (remaining !== null ? remaining.toFixed(0) + " credits remaining" : null);
    if (detail) lines.push(ctx.line.text({ label: "Usage", value: detail }));

    var plan = null;
    if (sub) {
      var subObj = sub.subscription || sub;
      plan = strAt(subObj, ["displayName", "display_name", "scheduledTier", "scheduled_tier", "tier"]);
    }

    return { plan: plan, lines: lines };
  }

  globalThis.__openusage_plugin = { id: "codebuff", probe: probe };
})();
