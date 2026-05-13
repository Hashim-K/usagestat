(function () {
  var BASE_URL = "https://admin.mistral.ai"
  var USER_AGENT = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/124.0.0.0 Safari/537.36"

  function getCookieHeader(ctx) {
    var raw = ctx.host.env.get("MISTRAL_COOKIE")
    if (!raw || !raw.trim()) {
      throw "Set MISTRAL_COOKIE to your cookie header from admin.mistral.ai. Include csrftoken if present."
    }
    return raw.trim()
  }

  function extractCsrf(cookieHeader) {
    var parts = cookieHeader.split(";")
    for (var i = 0; i < parts.length; i++) {
      var pair = parts[i].trim()
      var eq = pair.indexOf("=")
      if (eq < 0) continue
      var name = pair.slice(0, eq).trim()
      if (name === "csrftoken") return pair.slice(eq + 1).trim()
    }
    return null
  }

  function currentMonthYear() {
    var now = new Date()
    return { month: now.getUTCMonth() + 1, year: now.getUTCFullYear() }
  }

  function aggregateModels(models, prices) {
    var total = 0
    var inputTokens = 0
    var outputTokens = 0
    if (!models) return { cost: 0, inputTokens: 0, outputTokens: 0 }
    var keys = Object.keys(models)
    for (var i = 0; i < keys.length; i++) {
      var data = models[keys[i]]
      total += sumEntries(data.input, prices)
      total += sumEntries(data.output, prices)
      total += sumEntries(data.cached, prices)
      inputTokens += countEntries(data.input)
      outputTokens += countEntries(data.output)
    }
    return { cost: total, inputTokens: inputTokens, outputTokens: outputTokens }
  }

  function sumEntries(entries, prices) {
    if (!entries) return 0
    var total = 0
    for (var i = 0; i < entries.length; i++) {
      var e = entries[i]
      var paid = e.value_paid != null ? e.value_paid : (e.value || 0)
      var key = e.billing_metric + "::" + e.billing_group
      var price = prices[key] || 0
      total += paid * price
    }
    return total
  }

  function countEntries(entries) {
    if (!entries) return 0
    var total = 0
    for (var i = 0; i < entries.length; i++) {
      var e = entries[i]
      total += e.value_paid != null ? e.value_paid : (e.value || 0)
    }
    return total
  }

  function buildPriceIndex(prices) {
    var index = {}
    if (!prices) return index
    for (var i = 0; i < prices.length; i++) {
      var p = prices[i]
      if (p.billing_metric && p.billing_group && p.price) {
        var key = p.billing_metric + "::" + p.billing_group
        index[key] = parseFloat(p.price) || 0
      }
    }
    return index
  }

  function probe(ctx) {
    var cookieHeader = getCookieHeader(ctx)
    var csrf = extractCsrf(cookieHeader)
    var my = currentMonthYear()

    var headers = {
      "Cookie": cookieHeader,
      "Accept": "*/*",
      "Origin": BASE_URL,
      "Referer": BASE_URL + "/organization/usage",
      "User-Agent": USER_AGENT,
    }
    if (csrf) headers["X-CSRFTOKEN"] = csrf

    var result = ctx.util.requestJson({
      method: "GET",
      url: BASE_URL + "/api/billing/v2/usage?month=" + my.month + "&year=" + my.year,
      headers: headers,
      timeoutMs: 20000,
    })

    if (result.resp.status === 401 || result.resp.status === 403) {
      throw "Session expired. Update MISTRAL_COOKIE with fresh cookies from admin.mistral.ai."
    }
    if (result.resp.status < 200 || result.resp.status >= 300) {
      throw "Mistral API returned HTTP " + result.resp.status + "."
    }

    var billing = result.json
    if (!billing) throw "Could not parse Mistral billing response."

    var prices = buildPriceIndex(billing.prices)
    var currency = billing.currency || "EUR"
    var symbol = billing.currency_symbol || "€"

    var totalCost = 0
    var totalInput = 0
    var totalOutput = 0

    if (billing.completion && billing.completion.models) {
      var r = aggregateModels(billing.completion.models, prices)
      totalCost += r.cost; totalInput += r.inputTokens; totalOutput += r.outputTokens
    }

    var extras = [billing.ocr, billing.connectors, billing.audio]
    for (var i = 0; i < extras.length; i++) {
      if (extras[i] && extras[i].models) {
        totalCost += aggregateModels(extras[i].models, prices).cost
      }
    }

    if (billing.libraries_api) {
      var lib = billing.libraries_api
      if (lib.pages && lib.pages.models) totalCost += aggregateModels(lib.pages.models, prices).cost
      if (lib.tokens && lib.tokens.models) totalCost += aggregateModels(lib.tokens.models, prices).cost
    }

    var costStr = symbol + totalCost.toFixed(4) + " this month (" + currency + ")"
    var tokenDetail = totalInput + " in / " + totalOutput + " out tokens"

    var lines = [
      ctx.line.text({ label: "Monthly spend", value: costStr }),
      ctx.line.text({ label: "Tokens", value: tokenDetail }),
    ]

    return { lines: lines }
  }

  globalThis.__openusage_plugin = { id: "mistral", probe: probe }
})()
