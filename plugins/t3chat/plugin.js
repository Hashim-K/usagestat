(function () {
  const INPUT = '{"0":{"json":{"sessionId":null},"meta":{"values":{"sessionId":["undefined"]}}}}'
  const CUSTOMER_DATA_URL =
    "https://t3.chat/api/trpc/getCustomerData?batch=1&input=" + encodeURIComponent(INPUT)
  const FORWARDED_HEADERS = {
    "accept": "Accept",
    "accept-language": "Accept-Language",
    "cache-control": "Cache-Control",
    "pragma": "Pragma",
    "priority": "Priority",
    "referer": "Referer",
    "sec-fetch-dest": "Sec-Fetch-Dest",
    "sec-fetch-mode": "Sec-Fetch-Mode",
    "sec-fetch-site": "Sec-Fetch-Site",
    "trpc-accept": "trpc-accept",
    "user-agent": "User-Agent",
    "x-client-context": "x-client-context",
    "x-deployment-id": "X-Deployment-Id",
    "x-trpc-batch": "x-trpc-batch",
    "x-trpc-source": "x-trpc-source",
  }

  function rawCredential(ctx) {
    return (
      (ctx.provider && ctx.provider.cookieHeader) ||
      ctx.host.env.get("T3CHAT_COOKIE") ||
      ctx.host.env.get("T3_CHAT_COOKIE") ||
      ""
    ).trim()
  }

  function unquoteShell(value) {
    value = String(value || "").trim()
    if (!value) return ""
    if ((value[0] === "'" && value[value.length - 1] === "'") || (value[0] === '"' && value[value.length - 1] === '"')) {
      value = value.slice(1, -1)
    }
    return value.replace(/\\'/g, "'").replace(/\\"/g, '"').replace(/\\\\/g, "\\")
  }

  function headerFieldsFromCurl(raw) {
    const fields = []
    const text = String(raw || "")
    const re = /(?:^|\s)(?:-H|--header)(?:\s+|=)(?:"([^"]*)"|'([^']*)'|(\S+))/g
    let match
    while ((match = re.exec(text)) !== null) {
      fields.push(unquoteShell(match[1] || match[2] || match[3] || ""))
    }
    return fields
  }

  function cookieFieldsFromCurl(raw) {
    const fields = []
    const text = String(raw || "")
    const re = /(?:^|\s)(?:-b|--cookie)(?:\s+|=)(?:"([^"]*)"|'([^']*)'|(\S+))/g
    let match
    while ((match = re.exec(text)) !== null) {
      fields.push(unquoteShell(match[1] || match[2] || match[3] || ""))
    }
    return fields
  }

  function requestContext(raw) {
    raw = String(raw || "").trim()
    if (!raw) return null
    const fields = headerFieldsFromCurl(raw)
    const cookieFields = cookieFieldsFromCurl(raw)
    if (!fields.length && !cookieFields.length) return { cookieHeader: raw, headers: {} }

    let cookie = ""
    const headers = {}
    if (cookieFields.length) {
      cookie = cookieFields[cookieFields.length - 1]
    }
    for (let i = 0; i < fields.length; i += 1) {
      const field = fields[i]
      const idx = field.indexOf(":")
      if (idx === -1) continue
      const name = field.slice(0, idx).trim()
      const value = field.slice(idx + 1).trim()
      if (!name || !value) continue
      if (name.toLowerCase() === "cookie") {
        cookie = value
        continue
      }
      const canonical = FORWARDED_HEADERS[name.toLowerCase()]
      if (canonical) headers[canonical] = value
    }
    return cookie ? { cookieHeader: cookie, headers: headers } : null
  }

  function findCustomerData(value) {
    if (!value || typeof value !== "object") return null
    if (
      value.usageFourHourPercentage !== undefined ||
      value.usageMonthPercentage !== undefined ||
      (value.subscription && value.usageBand !== undefined)
    ) {
      return value
    }
    if (Array.isArray(value)) {
      for (let i = 0; i < value.length; i += 1) {
        const found = findCustomerData(value[i])
        if (found) return found
      }
      return null
    }
    for (const key in value) {
      const found = findCustomerData(value[key])
      if (found) return found
    }
    return null
  }

  function parseCustomerData(text) {
    const lines = String(text || "").split(/\r?\n/)
    for (let i = 0; i < lines.length; i += 1) {
      const line = lines[i].trim()
      if (!line) continue
      const parsed = ctxSafeJson(line)
      const customer = findCustomerData(parsed)
      if (customer) return customer
    }
    const parsed = ctxSafeJson(text)
    const customer = findCustomerData(parsed)
    if (customer) return customer
    throw "Could not parse T3 Chat usage: missing customer data object."
  }

  function ctxSafeJson(text) {
    try {
      return JSON.parse(text)
    } catch (_) {
      return null
    }
  }

  function percent(value) {
    const n = Number(value)
    if (!Number.isFinite(n)) return 0
    return Math.max(0, Math.min(100, n))
  }

  function isoFromEpoch(value) {
    const n = Number(value)
    if (!Number.isFinite(n) || n <= 0) return null
    const ms = n > 10000000000 ? n : n * 1000
    return new Date(ms).toISOString()
  }

  function planLabel(data) {
    const raw =
      (data.subscription && data.subscription.productName) ||
      data.subTier ||
      ""
    return String(raw).trim().replace(/[-_]+/g, " ").replace(/\b\w/g, function (c) {
      return c.toUpperCase()
    })
  }

  function probe(ctx) {
    const requestCtx = requestContext(rawCredential(ctx))
    if (!requestCtx || !requestCtx.cookieHeader) {
      throw "T3 Chat session not configured. Set T3CHAT_COOKIE or T3_CHAT_COOKIE.";
    }

    const headers = {
      Accept: "*/*",
      "Accept-Language": "en-US,en;q=0.9",
      "Cache-Control": "no-cache",
      Cookie: requestCtx.cookieHeader,
      Pragma: "no-cache",
      Priority: "u=4",
      Referer: "https://t3.chat/settings/customization",
      "Sec-Fetch-Dest": "empty",
      "Sec-Fetch-Mode": "cors",
      "Sec-Fetch-Site": "same-origin",
      "User-Agent":
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36",
      "trpc-accept": "application/jsonl",
      "x-trpc-batch": "true",
      "x-trpc-source": "web-client",
    }
    for (const name in requestCtx.headers) headers[name] = requestCtx.headers[name]

    const resp = ctx.util.request({
      method: "GET",
      url: CUSTOMER_DATA_URL,
      headers: headers,
      timeoutMs: 15000,
    })

    if (ctx.util.isAuthStatus(resp.status)) throw "T3 Chat session cookie is invalid or expired."
    if (resp.status < 200 || resp.status >= 300) {
      if (String(resp.bodyText || "").indexOf("Vercel") !== -1) {
        throw "T3 Chat returned a Vercel security challenge. Paste the full browser cURL request, not just the Cookie header."
      }
      throw "T3 Chat API error (HTTP " + resp.status + ")."
    }

    const data = parseCustomerData(resp.bodyText)
    const lines = []
    lines.push(ctx.line.progress({
      label: "Base",
      used: percent(data.usageFourHourPercentage || data.usagePeriodPercentage),
      limit: 100,
      resetsAt: isoFromEpoch(data.usageFourHourNextResetAt || data.usageWindowNextResetAt),
      periodDurationMs: 4 * 60 * 60 * 1000,
      format: { kind: "percent" },
    }))
    lines.push(ctx.line.progress({
      label: "Overage",
      used: percent(data.usageMonthPercentage || data.usagePeriodPercentage),
      limit: 100,
      resetsAt: isoFromEpoch(data.subscription && data.subscription.currentPeriodEnd),
      format: { kind: "percent" },
    }))
    if (data.lifetimeBalance !== undefined && data.lifetimeBalance !== null) {
      lines.push(ctx.line.text({ label: "Balance", value: String(data.lifetimeBalance) }))
    }

    return {
      source: "web",
      plan: planLabel(data),
      lines: lines,
    }
  }

  globalThis.__openusage_plugin = { id: "t3chat", probe: probe };
})();
