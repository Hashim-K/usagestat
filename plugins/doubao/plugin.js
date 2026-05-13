(function () {
  var API_URL = "https://ark.cn-beijing.volces.com/api/coding/v3/chat/completions"
  var PROBE_MODELS = ["doubao-seed-2.0-code", "doubao-1.5-pro-32k", "doubao-lite-32k"]

  function getApiKey(ctx) {
    var key = ctx.host.env.get("ARK_API_KEY")
      || ctx.host.env.get("DOUBAO_API_KEY")
      || ctx.host.env.get("VOLCENGINE_API_KEY")
    if (!key) throw "Set ARK_API_KEY, DOUBAO_API_KEY, or VOLCENGINE_API_KEY to use Doubao."
    return key
  }

  function probeModel(ctx, apiKey, model) {
    var resp = ctx.host.http.request({
      method: "POST",
      url: API_URL,
      headers: {
        "Authorization": "Bearer " + apiKey,
        "Content-Type": "application/json",
        "Accept": "application/json",
      },
      bodyText: JSON.stringify({
        model: model,
        max_tokens: 1,
        messages: [{ role: "user", content: "hi" }],
      }),
      timeoutMs: 20000,
    })

    if (resp.status === 401 || resp.status === 403) {
      throw "Invalid API key. Check ARK_API_KEY."
    }

    // Accept 200 (success) or 429 (rate-limited) — both return useful headers
    if (resp.status !== 200 && resp.status !== 429) {
      return null
    }

    return resp
  }

  function parseResetMs(value) {
    if (!value) return null
    var trimmed = value.trim()
    var ts = Number(trimmed)
    if (Number.isFinite(ts) && ts > 0) return ts * 1000
    var ms = Date.parse(trimmed)
    return Number.isFinite(ms) ? ms : null
  }

  function probe(ctx) {
    var apiKey = getApiKey(ctx)

    var resp = null
    for (var i = 0; i < PROBE_MODELS.length; i++) {
      resp = probeModel(ctx, apiKey, PROBE_MODELS[i])
      if (resp) break
    }

    if (!resp) throw "All Doubao probe models failed. Check your API key and account."

    var headers = resp.headers || {}
    var remaining = parseInt(headers["x-ratelimit-remaining-requests"] || headers["X-Ratelimit-Remaining-Requests"], 10)
    var limit = parseInt(headers["x-ratelimit-limit-requests"] || headers["X-Ratelimit-Limit-Requests"], 10)
    var resetRaw = headers["x-ratelimit-reset-requests"] || headers["X-Ratelimit-Reset-Requests"]

    var lines = []

    if (Number.isFinite(remaining) && Number.isFinite(limit) && limit > 0) {
      var used = Math.max(0, limit - remaining)
      var resetMs = parseResetMs(resetRaw)
      lines.push(ctx.line.progress({
        label: "Requests",
        used: used,
        limit: limit,
        format: { kind: "count", suffix: "req" },
        resetsAt: ctx.util.toIso(resetMs ? resetMs / 1000 : null),
      }))
    } else {
      var json = ctx.util.tryParseJson(resp.bodyText) || {}
      var totalTokens = json.usage && json.usage.total_tokens
      var detail = totalTokens
        ? "Active — " + totalTokens + " tokens observed"
        : "Active — check dashboard for details"
      lines.push(ctx.line.badge({ label: "Status", text: detail, color: "#22c55e" }))
    }

    return { lines: lines }
  }

  globalThis.__openusage_plugin = { id: "doubao", probe: probe }
})()
