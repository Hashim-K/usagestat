(function () {
  var SETTINGS_URL = "https://ollama.com/settings"
  var SESSION_COOKIE = "__Secure-session"

  function getCookieHeader(ctx) {
    var raw = ctx.host.env.get("OLLAMA_COOKIE")
    if (!raw || !raw.trim()) {
      throw "Set OLLAMA_COOKIE to your " + SESSION_COOKIE + " cookie value from ollama.com/settings."
    }
    var val = raw.trim()
    // Strip optional "cookie:" prefix
    if (val.toLowerCase().slice(0, 7) === "cookie:") {
      val = val.slice(7).trim()
    }
    // If just a bare token value with no key=value format, wrap it
    if (val.indexOf("=") < 0) {
      return SESSION_COOKIE + "=" + val
    }
    return val
  }

  function parsePercent(html, labels) {
    for (var i = 0; i < labels.length; i++) {
      var idx = html.indexOf(labels[i])
      if (idx < 0) continue
      var window = html.slice(idx, idx + 800)

      var usedMatch = window.match(/(\d+(?:\.\d+)?)\s*%\s*used/)
      if (usedMatch) return parseFloat(usedMatch[1])

      var widthMatch = window.match(/width:\s*(\d+(?:\.\d+)?)%/)
      if (widthMatch) return parseFloat(widthMatch[1])
    }
    return null
  }

  function probe(ctx) {
    var cookieHeader = getCookieHeader(ctx)

    var resp = ctx.host.http.request({
      method: "GET",
      url: SETTINGS_URL,
      headers: {
        "Cookie": cookieHeader,
        "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        "User-Agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/124.0.0.0 Safari/537.36",
      },
      timeoutMs: 20000,
    })

    if (resp.status === 401 || resp.status === 403) {
      throw "Session expired. Update OLLAMA_COOKIE with a fresh session token."
    }

    var html = resp.bodyText || ""

    if (!html.includes("Session usage") && !html.includes("Weekly usage") && !html.includes("Cloud Usage")) {
      if (html.includes("Sign in") || resp.status !== 200) {
        throw "Session expired. Update OLLAMA_COOKIE with a fresh session token."
      }
    }

    var sessionPct = parsePercent(html, ["Session usage", "Hourly usage"])
    var weeklyPct = parsePercent(html, ["Weekly usage"])

    if (sessionPct === null && weeklyPct === null) {
      throw "Could not find usage data on Ollama settings page."
    }

    var lines = []
    if (sessionPct !== null) {
      lines.push(ctx.line.progress({
        label: "Session",
        used: sessionPct,
        limit: 100,
        format: { kind: "percent" },
      }))
    }
    if (weeklyPct !== null) {
      lines.push(ctx.line.progress({
        label: "Weekly",
        used: weeklyPct,
        limit: 100,
        format: { kind: "percent" },
      }))
    }

    return { lines: lines }
  }

  globalThis.__openusage_plugin = { id: "ollama", probe: probe }
})()
