(function () {
  var BASE_URL = "https://opencode.ai";
  var SERVER_URL = "https://opencode.ai/_server";
  var WORKSPACES_SERVER_ID = "def39973159c7f0483d8793a822b8dbb10d067e12c65455fcb4608459ba0234f";
  var SUBSCRIPTION_SERVER_ID = "7abeebee372f304e050aaaf92be863f4a86490e382f8c79db68fd94040d691b4";
  var FIVE_HOURS_MS = 5 * 60 * 60 * 1000;
  var WEEK_MS = 7 * 24 * 60 * 60 * 1000;
  var PERCENT_KEYS = [
    "usagePercent", "usedPercent", "percentUsed", "percent", "usage_percent", "used_percent",
    "utilization", "utilizationPercent", "utilization_percent", "usage",
  ];
  var RESET_IN_KEYS = [
    "resetInSec", "resetInSeconds", "resetSeconds", "reset_sec", "reset_in_sec",
    "resetsInSec", "resetsInSeconds", "resetIn", "resetSec",
  ];
  var RESET_AT_KEYS = [
    "resetAt", "resetsAt", "reset_at", "resets_at", "nextReset", "next_reset", "renewAt", "renew_at",
  ];

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

  function cookieHeader(ctx) {
    var cookie = trim(ctx.provider && ctx.provider.cookieHeader) ||
      setting(ctx, ["cookieHeader", "cookie"]) ||
      env(ctx, "OPENCODE_COOKIE");
    if (!cookie) throw "OpenCode session not configured. Set OPENCODE_COOKIE.";
    return cookie;
  }

  function workspaceOverride(ctx) {
    return normalizeWorkspaceId(
      setting(ctx, ["workspaceId", "workspace"]) ||
      (ctx.provider && trim(ctx.provider.workspaceId)) ||
      env(ctx, "OPENCODE_WORKSPACE_ID") ||
      env(ctx, "CODEXBAR_OPENCODE_WORKSPACE_ID")
    );
  }

  function normalizeWorkspaceId(raw) {
    var text = trim(raw);
    if (!text) return null;
    var match = /wrk_[A-Za-z0-9]+/.exec(text);
    return match ? match[0] : null;
  }

  function serverInstance() {
    return "server-fn:" + Math.random().toString(16).slice(2) + Date.now().toString(16);
  }

  function serverRequest(ctx, opts) {
    var method = opts.method || "GET";
    var url = SERVER_URL;
    if (method === "GET") {
      url += "?id=" + encodeURIComponent(opts.serverId);
      if (opts.args && opts.args.length) url += "&args=" + encodeURIComponent(JSON.stringify(opts.args));
    }
    var req = {
      method: method,
      url: url,
      headers: {
        Cookie: opts.cookie,
        "X-Server-Id": opts.serverId,
        "X-Server-Instance": serverInstance(),
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120.0.0.0 Safari/537.36",
        Origin: BASE_URL,
        Referer: opts.referer,
        Accept: "text/javascript, application/json;q=0.9, */*;q=0.8",
      },
      timeoutMs: 15000,
    };
    if (method !== "GET") {
      req.headers["Content-Type"] = "application/json";
      req.bodyText = JSON.stringify(opts.args || []);
    }
    var resp = ctx.util.request(req);
    var text = String(resp.bodyText || "");
    if (looksSignedOut(text) || ctx.util.isAuthStatus(resp.status)) {
      throw "OpenCode session cookie is invalid or expired.";
    }
    if (resp.status < 200 || resp.status >= 300) {
      throw "OpenCode API error: HTTP " + resp.status + ".";
    }
    return text;
  }

  function looksSignedOut(text) {
    var lower = String(text || "").toLowerCase();
    return lower.indexOf("login") >= 0 || lower.indexOf("sign in") >= 0 || lower.indexOf("auth/authorize") >= 0;
  }

  function collectWorkspaceIds(value, out) {
    if (typeof value === "string" && /^wrk_[A-Za-z0-9]+/.test(value) && out.indexOf(value) < 0) {
      out.push(value);
      return;
    }
    if (Array.isArray(value)) {
      for (var i = 0; i < value.length; i++) collectWorkspaceIds(value[i], out);
      return;
    }
    if (value && typeof value === "object") {
      Object.keys(value).forEach(function (key) { collectWorkspaceIds(value[key], out); });
    }
  }

  function parseWorkspaceIds(ctx, text) {
    var ids = [];
    var json = ctx.util.tryParseJson(text);
    if (json) collectWorkspaceIds(json, ids);
    var regex = /"id"\s*:\s*"(wrk_[^"]+)"|id\s*:\s*"(wrk_[^"]+)"/g;
    var match;
    while ((match = regex.exec(String(text || ""))) !== null) {
      var id = match[1] || match[2];
      if (id && ids.indexOf(id) < 0) ids.push(id);
    }
    return ids;
  }

  function fetchWorkspaceId(ctx, cookie) {
    var text = serverRequest(ctx, {
      method: "GET",
      serverId: WORKSPACES_SERVER_ID,
      cookie: cookie,
      referer: BASE_URL,
    });
    var ids = parseWorkspaceIds(ctx, text);
    if (ids.length) return ids[0];
    text = serverRequest(ctx, {
      method: "POST",
      serverId: WORKSPACES_SERVER_ID,
      args: [],
      cookie: cookie,
      referer: BASE_URL,
    });
    ids = parseWorkspaceIds(ctx, text);
    if (ids.length) return ids[0];
    throw "OpenCode parse error: missing workspace id.";
  }

  function fetchSubscription(ctx, workspaceId, cookie) {
    var referer = BASE_URL + "/workspace/" + workspaceId + "/billing";
    var text = serverRequest(ctx, {
      method: "GET",
      serverId: SUBSCRIPTION_SERVER_ID,
      args: [workspaceId],
      cookie: cookie,
      referer: referer,
    });
    if (parseSubscription(ctx, text, Date.now())) return text;
    return serverRequest(ctx, {
      method: "POST",
      serverId: SUBSCRIPTION_SERVER_ID,
      args: [workspaceId],
      cookie: cookie,
      referer: referer,
    });
  }

  function numberValue(value) {
    if (typeof value === "number" && Number.isFinite(value)) return value;
    if (typeof value === "string" && value.trim()) {
      var parsed = Number(value);
      if (Number.isFinite(parsed)) return parsed;
    }
    return null;
  }

  function firstNumber(obj, keys) {
    if (!obj || typeof obj !== "object") return null;
    for (var i = 0; i < keys.length; i++) {
      var n = numberValue(obj[keys[i]]);
      if (n !== null) return n;
    }
    return null;
  }

  function dateFromValue(ctx, value) {
    var n = numberValue(value);
    if (n !== null && n > 0) return ctx.util.toIso(n);
    return ctx.util.toIso(value);
  }

  function findDatetime(ctx, json, keys) {
    if (Array.isArray(json)) {
      for (var i = 0; i < json.length; i++) {
        var inArray = findDatetime(ctx, json[i], keys);
        if (inArray) return inArray;
      }
      return null;
    }
    if (!json || typeof json !== "object") return null;
    for (var k = 0; k < keys.length; k++) {
      if (json[keys[k]] !== undefined) {
        var parsed = dateFromValue(ctx, json[keys[k]]);
        if (parsed) return parsed;
      }
    }
    var objectKeys = Object.keys(json);
    for (var j = 0; j < objectKeys.length; j++) {
      var nested = findDatetime(ctx, json[objectKeys[j]], keys);
      if (nested) return nested;
    }
    return null;
  }

  function percentFromUsedLimit(obj) {
    var used = numberValue(obj && (obj.used !== undefined ? obj.used : obj.usage));
    var limit = numberValue(obj && (obj.limit !== undefined ? obj.limit : obj.total));
    if (used === null || limit === null || limit <= 0) return null;
    return (used / limit) * 100;
  }

  function parseWindow(ctx, obj, nowMs) {
    if (!obj || typeof obj !== "object") return null;
    var percent = firstNumber(obj, PERCENT_KEYS);
    if (percent === null) percent = percentFromUsedLimit(obj);
    if (percent === null) return null;
    if (percent <= 1) percent *= 100;
    percent = Math.max(0, Math.min(100, percent));

    var resetSeconds = firstNumber(obj, RESET_IN_KEYS);
    var resetIso = null;
    if (resetSeconds !== null) {
      resetIso = new Date(nowMs + Math.max(0, resetSeconds) * 1000).toISOString();
    } else {
      resetIso = findDatetime(ctx, obj, RESET_AT_KEYS);
    }
    return { percent: percent, resetsAt: resetIso };
  }

  function findUsageWindow(ctx, json, keys, nowMs) {
    if (Array.isArray(json)) {
      for (var i = 0; i < json.length; i++) {
        var inArray = findUsageWindow(ctx, json[i], keys, nowMs);
        if (inArray) return inArray;
      }
      return null;
    }
    if (!json || typeof json !== "object") return null;
    for (var k = 0; k < keys.length; k++) {
      var direct = parseWindow(ctx, json[keys[k]], nowMs);
      if (direct) return direct;
    }
    var objectKeys = Object.keys(json);
    for (var j = 0; j < objectKeys.length; j++) {
      var nested = findUsageWindow(ctx, json[objectKeys[j]], keys, nowMs);
      if (nested) return nested;
    }
    return null;
  }

  function parseSubscriptionJson(ctx, text, nowMs) {
    var json = ctx.util.tryParseJson(text);
    if (!json) return null;
    var rolling = findUsageWindow(ctx, json, ["rollingUsage", "rolling", "rolling_usage"], nowMs);
    var weekly = findUsageWindow(ctx, json, ["weeklyUsage", "weekly", "weekly_usage"], nowMs);
    if (!rolling || !weekly) return null;
    return {
      rolling: rolling,
      weekly: weekly,
      renewsAt: findDatetime(ctx, json, ["renewAt", "renew_at"]),
    };
  }

  function regexNumber(text, regex) {
    var match = regex.exec(String(text || ""));
    return match ? numberValue(match[1]) : null;
  }

  function parseSubscriptionRegex(ctx, text, nowMs) {
    var rollingPercent = regexNumber(text, /rollingUsage[^}]*?usagePercent\s*:\s*([0-9]+(?:\.[0-9]+)?)/);
    var rollingReset = regexNumber(text, /rollingUsage[^}]*?resetInSec\s*:\s*([0-9]+)/);
    var weeklyPercent = regexNumber(text, /weeklyUsage[^}]*?usagePercent\s*:\s*([0-9]+(?:\.[0-9]+)?)/);
    var weeklyReset = regexNumber(text, /weeklyUsage[^}]*?resetInSec\s*:\s*([0-9]+)/);
    if (rollingPercent === null || rollingReset === null || weeklyPercent === null || weeklyReset === null) return null;
    var renewMatch = /(?:"renewAt"|"renew_at"|renewAt|renew_at)\s*[:=]\s*"?([^",}\s]+)"?/.exec(String(text || ""));
    return {
      rolling: { percent: rollingPercent, resetsAt: new Date(nowMs + rollingReset * 1000).toISOString() },
      weekly: { percent: weeklyPercent, resetsAt: new Date(nowMs + weeklyReset * 1000).toISOString() },
      renewsAt: renewMatch ? dateFromValue(ctx, renewMatch[1]) : null,
    };
  }

  function parseSubscription(ctx, text, nowMs) {
    return parseSubscriptionJson(ctx, text, nowMs) || parseSubscriptionRegex(ctx, text, nowMs);
  }

  function probe(ctx) {
    var cookie = cookieHeader(ctx);
    var workspaceId = workspaceOverride(ctx) || fetchWorkspaceId(ctx, cookie);
    var nowMs = Date.now();
    var parsed = parseSubscription(ctx, fetchSubscription(ctx, workspaceId, cookie), nowMs);
    if (!parsed) {
      throw "OpenCode parse error: missing usage fields.";
    }
    var lines = [
      ctx.line.progress({
        label: "5-hour",
        used: parsed.rolling.percent,
        limit: 100,
        format: { kind: "percent" },
        resetsAt: parsed.rolling.resetsAt,
        periodDurationMs: FIVE_HOURS_MS,
      }),
      ctx.line.progress({
        label: "Weekly",
        used: parsed.weekly.percent,
        limit: 100,
        format: { kind: "percent" },
        resetsAt: parsed.weekly.resetsAt,
        periodDurationMs: WEEK_MS,
      }),
    ];
    if (parsed.renewsAt) lines.push(ctx.line.text({ label: "Renews", value: parsed.renewsAt }));
    return {
      displayName: "OpenCode",
      source: "web",
      plan: "OpenCode",
      lines: lines,
    };
  }

  globalThis.__openusage_plugin = { id: "opencode", probe: probe };
})();
