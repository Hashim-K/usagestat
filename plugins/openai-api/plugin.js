(function () {
  var API_URL = "https://api.openai.com/v1/dashboard/billing/credit_grants";

  function loadApiKey(ctx) {
    var names = ["OPENAI_API_KEY", "OPENAI_PLATFORM_API_KEY"];
    for (var i = 0; i < names.length; i++) {
      var v = ctx.host.env.get(names[i]);
      if (typeof v === "string" && v.trim()) return v.trim();
    }
    return null;
  }

  function probe(ctx) {
    var apiKey = loadApiKey(ctx);
    if (!apiKey) {
      throw "OpenAI API key not found. Set OPENAI_API_KEY or OPENAI_PLATFORM_API_KEY.";
    }

    var result = ctx.util.requestJson({
      method: "GET",
      url: API_URL,
      headers: { Authorization: "Bearer " + apiKey, Accept: "application/json" },
      timeoutMs: 15000,
    });

    if (ctx.util.isAuthStatus(result.resp.status)) {
      throw "API key invalid or expired. Check your OpenAI API key.";
    }
    if (result.resp.status < 200 || result.resp.status >= 300) {
      throw "OpenAI API error (HTTP " + result.resp.status + ").";
    }
    if (!result.json) throw "Could not parse OpenAI credit grants response.";

    var json = result.json;
    var granted = typeof json.total_granted === "number" ? json.total_granted : 0;
    var used = typeof json.total_used === "number" ? json.total_used : 0;
    var available = typeof json.total_available === "number" ? json.total_available : Math.max(0, granted - used);

    var usedPct = granted > 0 ? Math.min(100, (used / granted) * 100) : (available > 0 ? 0 : 100);

    // Find earliest expiry from grants
    var expiresAt = null;
    var grants = json.grants && Array.isArray(json.grants.data) ? json.grants.data : [];
    var now = Date.now();
    for (var i = 0; i < grants.length; i++) {
      var ts = grants[i].expires_at;
      if (typeof ts === "number" && ts * 1000 > now) {
        if (expiresAt === null || ts < expiresAt) expiresAt = ts;
      }
    }

    var opts = {
      label: "Credits",
      used: usedPct,
      limit: 100,
      format: { kind: "percent" },
    };
    if (expiresAt !== null) opts.resetsAt = ctx.util.toIso(expiresAt * 1000);

    var lines = [ctx.line.progress(opts)];
    lines.push(ctx.line.text({ label: "Available", value: "$" + available.toFixed(2) }));

    return { lines: lines };
  }

  globalThis.__openusage_plugin = { id: "openai-api", probe: probe };
})();
