(function () {
  var API_URL = "https://crof.ai/usage_api/";

  function loadApiKey(ctx) {
    var v = ctx.host.env.get("CROF_API_KEY");
    if (typeof v === "string" && v.trim()) return v.trim();
    return null;
  }

  function probe(ctx) {
    var apiKey = loadApiKey(ctx);
    if (!apiKey) {
      throw "Crof API key not found. Set CROF_API_KEY.";
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
      throw "Crof API error (HTTP " + result.resp.status + ").";
    }
    if (!result.json) throw "Could not parse Crof usage response.";

    var json = result.json;
    var credits = typeof json.credits === "number" ? json.credits : 0;
    var requestsPlan = typeof json.requests_plan === "number" ? json.requests_plan : 0;
    var usableRequests = typeof json.usable_requests === "number" ? json.usable_requests : 0;

    var usedRequests = Math.max(0, requestsPlan - usableRequests);
    var usedPct = requestsPlan > 0 ? Math.min(100, (usedRequests / requestsPlan) * 100) : 0;

    var lines = [
      ctx.line.progress({
        label: "Requests",
        used: usedRequests,
        limit: requestsPlan,
        format: { kind: "count", suffix: "requests" },
      }),
      ctx.line.text({ label: "Credits", value: credits.toFixed(2) + " credits remaining" }),
    ];

    return { lines: lines };
  }

  globalThis.__openusage_plugin = { id: "crof", probe: probe };
})();
