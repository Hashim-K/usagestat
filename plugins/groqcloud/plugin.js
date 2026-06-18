(function () {
  var DEFAULT_API_BASE = "https://api.groq.com/openai/v1";
  var QUERIES = {
    requests: "sum(model_project_id_status_code:requests:rate5m)",
    inputTokens: "sum(model_project_id:tokens_in:rate5m)",
    outputTokens: "sum(model_project_id:tokens_out:rate5m)",
    cacheHits: "sum(model_project_id:prompt_cache_hits:rate5m)",
  };

  function trim(value) {
    return typeof value === "string" && value.trim() ? value.trim() : null;
  }

  function env(ctx, name) {
    try {
      return trim(ctx.host.env.get(name));
    } catch (_) {
      return null;
    }
  }

  function setting(ctx, names) {
    var settings = ctx.provider && ctx.provider.settings ? ctx.provider.settings : {};
    for (var i = 0; i < names.length; i++) {
      var value = trim(settings[names[i]]);
      if (value) return value;
    }
    return null;
  }

  function apiKey(ctx) {
    var key = setting(ctx, ["apiKey", "token"]) ||
      (ctx.provider && trim(ctx.provider.apiKey)) ||
      env(ctx, "GROQ_API_KEY") ||
      env(ctx, "GROQCLOUD_API_KEY");
    if (!key) throw "GroqCloud API key not found. Set GROQ_API_KEY or GROQCLOUD_API_KEY.";
    return key;
  }

  function apiBase(ctx) {
    return (setting(ctx, ["apiUrl", "apiBase", "baseUrl"]) || env(ctx, "GROQ_API_URL") || DEFAULT_API_BASE).replace(/\/+$/, "");
  }

  function prometheusValue(value) {
    if (typeof value === "number" && Number.isFinite(value)) return value;
    if (typeof value === "string" && value.trim()) {
      var parsed = Number(value);
      return Number.isFinite(parsed) ? parsed : null;
    }
    return null;
  }

  function parseScalar(ctx, bodyText) {
    var decoded = ctx.util.tryParseJson(bodyText);
    if (!decoded || typeof decoded !== "object") throw "Failed to parse Groq metrics.";
    if (decoded.status !== "success") {
      throw (typeof decoded.error === "string" && decoded.error.trim()) || "Groq metrics query failed.";
    }
    var result = decoded.data && Array.isArray(decoded.data.result) ? decoded.data.result : [];
    var total = 0;
    for (var i = 0; i < result.length; i++) {
      var series = result[i];
      var values = series && Array.isArray(series.value) ? series.value : null;
      if (!values || values.length === 0) continue;
      var n = prometheusValue(values[values.length - 1]);
      if (n !== null) total += n;
    }
    return total;
  }

  function queryScalar(ctx, base, key, query) {
    var resp = ctx.util.request({
      method: "GET",
      url: base + "/metrics/prometheus/api/v1/query?query=" + encodeURIComponent(query),
      headers: {
        Authorization: "Bearer " + key,
        Accept: "application/json",
      },
      timeoutMs: 15000,
    });
    if (ctx.util.isAuthStatus(resp.status)) throw "GroqCloud API key is invalid or unauthorized.";
    if (resp.status < 200 || resp.status >= 300) {
      throw "Groq metrics API returned HTTP " + resp.status + ".";
    }
    return parseScalar(ctx, resp.bodyText);
  }

  function fmt(value) {
    if (value >= 100) return value.toFixed(0);
    if (value >= 10) return value.toFixed(1);
    return value.toFixed(2);
  }

  function probe(ctx) {
    var key = apiKey(ctx);
    var base = apiBase(ctx);
    var requestRate = queryScalar(ctx, base, key, QUERIES.requests);
    var inputRate = queryScalar(ctx, base, key, QUERIES.inputTokens);
    var outputRate = queryScalar(ctx, base, key, QUERIES.outputTokens);
    var cacheRate = queryScalar(ctx, base, key, QUERIES.cacheHits);
    var requestsPerMinute = requestRate * 60;
    var tokensPerMinute = (inputRate + outputRate) * 60;
    var cacheHitsPerMinute = cacheRate * 60;
    return {
      displayName: "GroqCloud",
      source: "api",
      plan: "Prometheus metrics",
      lines: [
        ctx.line.text({ label: "Requests", value: fmt(requestsPerMinute) + " req/min", subtitle: "5 minute rate" }),
        ctx.line.text({ label: "Tokens", value: fmt(tokensPerMinute) + " tok/min", subtitle: "Input + output" }),
        ctx.line.text({ label: "Cache hits", value: fmt(cacheHitsPerMinute) + " cache/min", subtitle: "Prompt cache" }),
      ],
    };
  }

  globalThis.__openusage_plugin = { id: "groqcloud", probe: probe };
})();
