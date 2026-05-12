(function () {
  var API_URL = "https://api.moonshot.cn/v1/users/me/balance";

  function loadApiKey(ctx) {
    var names = ["MOONSHOT_API_KEY", "KIMI_API_KEY"];
    for (var i = 0; i < names.length; i++) {
      var v = ctx.host.env.get(names[i]);
      if (typeof v === "string" && v.trim()) return v.trim();
    }
    return null;
  }

  function probe(ctx) {
    var apiKey = loadApiKey(ctx);
    if (!apiKey) {
      throw "Kimi K2 / Moonshot API key not found. Set MOONSHOT_API_KEY or KIMI_API_KEY.";
    }

    var result = ctx.util.requestJson({
      method: "GET",
      url: API_URL,
      headers: { Authorization: "Bearer " + apiKey, Accept: "application/json" },
      timeoutMs: 30000,
    });

    if (ctx.util.isAuthStatus(result.resp.status)) {
      throw "API key invalid or expired.";
    }
    if (result.resp.status < 200 || result.resp.status >= 300) {
      throw "Kimi K2 API error (HTTP " + result.resp.status + ").";
    }
    if (!result.json) throw "Could not parse Kimi K2 balance response.";

    var data = result.json.data || result.json;
    var available = typeof data.available_balance === "number" ? data.available_balance
                  : typeof data.balance === "number" ? data.balance : 0;
    var total = typeof data.total_balance === "number" ? data.total_balance
              : typeof data.total === "number" ? data.total : 0;
    var used = typeof data.used_balance === "number" ? data.used_balance
             : typeof data.used === "number" ? data.used : Math.max(0, total - available);

    var usedPct = total > 0 ? Math.min(100, (used / total) * 100) : 0;

    var lines = [
      ctx.line.progress({
        label: "Credits",
        used: usedPct,
        limit: 100,
        format: { kind: "percent" },
      }),
      ctx.line.text({ label: "Balance", value: available.toFixed(4) + " CNY remaining" }),
    ];

    return { lines: lines };
  }

  globalThis.__openusage_plugin = { id: "kimi-k2", probe: probe };
})();
