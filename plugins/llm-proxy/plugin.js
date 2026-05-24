(function () {
  function apiKey(ctx) {
    return ctx.host.env.get("LLM_PROXY_API_KEY") || ctx.host.env.get("LLMPROXY_API_KEY");
  }

  function baseUrl(ctx) {
    return (ctx.host.env.get("LLM_PROXY_API_URL") || ctx.host.env.get("LLM_PROXY_BASE_URL") || "").replace(/\/$/, "");
  }

  function probe(ctx) {
    var key = apiKey(ctx);
    var base = baseUrl(ctx);
    if (!key) throw "LLM Proxy API key not found. Set LLM_PROXY_API_KEY.";
    if (!base) throw "LLM Proxy base URL not configured. Set LLM_PROXY_API_URL.";

    var result = ctx.util.requestJson({
      method: "GET",
      url: base + "/v1/quota-stats",
      headers: { Authorization: "Bearer " + key, Accept: "application/json" },
      timeoutMs: 15000,
    });
    if (ctx.util.isAuthStatus(result.resp.status)) throw "LLM Proxy API key invalid or expired.";
    if (result.resp.status < 200 || result.resp.status >= 300) throw "LLM Proxy API error (HTTP " + result.resp.status + ").";

    var json = result.json || {};
    var used = Number(json.used || json.quota_used || 0);
    var limit = Number(json.limit || json.quota_limit || 0);
    var lines = [];
    if (limit > 0) {
      lines.push(ctx.line.progress({ label: "Quota", used: used, limit: limit, format: { kind: "count", suffix: "requests" } }));
    } else {
      lines.push(ctx.line.text({ label: "Used", value: String(used) }));
    }
    var providers = json.providers || json.provider_breakdown;
    if (Array.isArray(providers)) lines.push(ctx.line.text({ label: "Providers", value: String(providers.length) }));
    return { lines: lines };
  }

  globalThis.__openusage_plugin = { id: "llm-proxy", probe: probe };
})();
