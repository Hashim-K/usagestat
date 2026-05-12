(function () {
  var API_URL = "https://nano-gpt.com/api/subscription/v1/usage";
  var DAY_MS = 24 * 60 * 60 * 1000;
  var MONTH_MS = 30 * DAY_MS;

  function loadApiKey(ctx) {
    var v = ctx.host.env.get("NANOGPT_API_KEY");
    if (typeof v === "string" && v.trim()) return v.trim();
    return null;
  }

  function probe(ctx) {
    var apiKey = loadApiKey(ctx);
    if (!apiKey) {
      throw "NanoGPT API key not found. Set NANOGPT_API_KEY.";
    }

    var result = ctx.util.requestJson({
      method: "GET",
      url: API_URL,
      headers: { Authorization: "Bearer " + apiKey, Accept: "application/json" },
      timeoutMs: 15000,
    });

    if (ctx.util.isAuthStatus(result.resp.status)) {
      throw "API key invalid or expired.";
    }
    if (result.resp.status < 200 || result.resp.status >= 300) {
      throw "NanoGPT API error (HTTP " + result.resp.status + ").";
    }
    if (!result.json) throw "Could not parse NanoGPT usage response.";

    var json = result.json;
    if (!json.active) {
      throw "NanoGPT subscription inactive. Check your subscription at nano-gpt.com.";
    }

    var limits = json.limits || {};
    var daily = json.daily || {};
    var monthly = json.monthly || {};

    var dailyPct = typeof daily.percentUsed === "number" ? Math.min(100, daily.percentUsed * 100) : 0;
    var dailyOpts = {
      label: "Daily",
      used: dailyPct,
      limit: 100,
      format: { kind: "percent" },
      periodDurationMs: DAY_MS,
    };
    if (daily.resetAt) {
      var dailyReset = ctx.util.toIso(daily.resetAt);
      if (dailyReset) dailyOpts.resetsAt = dailyReset;
    }

    var monthlyPct = typeof monthly.percentUsed === "number" ? Math.min(100, monthly.percentUsed * 100) : 0;
    var monthlyOpts = {
      label: "Monthly",
      used: monthlyPct,
      limit: 100,
      format: { kind: "percent" },
      periodDurationMs: MONTH_MS,
    };
    if (monthly.resetAt) {
      var monthlyReset = ctx.util.toIso(monthly.resetAt);
      if (monthlyReset) monthlyOpts.resetsAt = monthlyReset;
    }

    var stateLabel = json.state || "active";
    if (json.graceUntil) stateLabel += " (grace until " + json.graceUntil + ")";

    var lines = [
      ctx.line.progress(dailyOpts),
      ctx.line.progress(monthlyOpts),
    ];

    return { plan: stateLabel, lines: lines };
  }

  globalThis.__openusage_plugin = { id: "nanogpt", probe: probe };
})();
