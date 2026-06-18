(function () {
  var API_URL = "https://platform.stepfun.com/api/step.openapi.devcenter.Dashboard/QueryStepPlanRateLimit";
  var PLAN_URL = "https://platform.stepfun.com/api/step.openapi.devcenter.Dashboard/GetStepPlanStatus";
  var WEB_ID = "c8a1002d2c457e758785a9979832217c7c0b884c";
  var APP_ID = "10300";

  function env(ctx, name) {
    try {
      var value = ctx.host.env.get(name);
      return typeof value === "string" && value.trim() ? value.trim() : null;
    } catch (_) {
      return null;
    }
  }

  function setting(ctx, names) {
    var settings = ctx.provider && ctx.provider.settings ? ctx.provider.settings : {};
    for (var i = 0; i < names.length; i++) {
      var value = settings[names[i]];
      if (typeof value === "string" && value.trim()) return value.trim();
    }
    return null;
  }

  function normalizeToken(raw) {
    var trimmed = typeof raw === "string" ? raw.trim() : "";
    if (!trimmed) return null;
    var marker = "Oasis-Token=";
    var idx = trimmed.indexOf(marker);
    if (idx >= 0) {
      var value = trimmed.slice(idx + marker.length).split(";")[0].trim();
      return value || null;
    }
    return trimmed;
  }

  function token(ctx) {
    var raw = setting(ctx, ["token", "oasisToken", "cookie"]) ||
      (ctx.provider && typeof ctx.provider.apiKey === "string" ? ctx.provider.apiKey.trim() : "") ||
      env(ctx, "STEPFUN_TOKEN") ||
      env(ctx, "STEPFUN_OASIS_TOKEN") ||
      env(ctx, "STEPFUN_COOKIE");
    var normalized = normalizeToken(raw);
    if (normalized) return normalized;
    if (env(ctx, "STEPFUN_USERNAME") || env(ctx, "STEPFUN_PASSWORD")) {
      throw "StepFun username/password login is not supported yet. Set STEPFUN_TOKEN or STEPFUN_OASIS_TOKEN.";
    }
    throw "StepFun session not configured. Set STEPFUN_TOKEN or STEPFUN_OASIS_TOKEN.";
  }

  function headers(token) {
    return {
      "content-type": "application/json",
      "oasis-appid": APP_ID,
      "oasis-platform": "web",
      "oasis-webid": WEB_ID,
      "user-agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36",
      "Cookie": "Oasis-Token=" + token + "; Oasis-Webid=" + WEB_ID,
    };
  }

  function requestJson(ctx, url, token) {
    var resp = ctx.util.request({
      method: "POST",
      url: url,
      headers: headers(token),
      bodyText: "{}",
      timeoutMs: 15000,
    });
    if (ctx.util.isAuthStatus(resp.status)) throw "StepFun token is invalid or expired.";
    if (resp.status < 200 || resp.status >= 300) throw "StepFun API returned HTTP " + resp.status + ".";
    var json = ctx.util.tryParseJson(resp.bodyText);
    if (!json) throw "StepFun response was not valid JSON.";
    return json;
  }

  function numberValue(value) {
    if (typeof value === "number" && Number.isFinite(value)) return value;
    if (typeof value === "string" && value.trim()) {
      var parsed = Number(value);
      if (Number.isFinite(parsed)) return parsed;
    }
    return null;
  }

  function timestamp(value) {
    var seconds = numberValue(value);
    if (seconds === null || seconds <= 0) return null;
    return new Date(seconds * 1000);
  }

  function parseUsage(json) {
    if (!json || typeof json !== "object") throw "StepFun response was invalid.";
    if (json.status !== 1) {
      var message = (typeof json.message === "string" && json.message.trim()) ||
        (typeof json.desc === "string" && json.desc.trim()) ||
        (json.code != null ? String(json.code) : "unknown");
      throw "StepFun API error: " + message;
    }
    var fiveHourLeft = numberValue(json.five_hour_usage_left_rate);
    var weeklyLeft = numberValue(json.weekly_usage_left_rate);
    var fiveHourReset = timestamp(json.five_hour_usage_reset_time);
    var weeklyReset = timestamp(json.weekly_usage_reset_time);
    if (fiveHourLeft === null || weeklyLeft === null || !fiveHourReset || !weeklyReset) {
      throw "StepFun response missing usage rate or reset fields.";
    }
    return {
      fiveHourLeft: Math.max(0, Math.min(1, fiveHourLeft)),
      weeklyLeft: Math.max(0, Math.min(1, weeklyLeft)),
      fiveHourReset: fiveHourReset,
      weeklyReset: weeklyReset,
    };
  }

  function planName(ctx, token) {
    try {
      var json = requestJson(ctx, PLAN_URL, token);
      var subscription = json && json.subscription;
      var name = subscription && typeof subscription.name === "string" ? subscription.name.trim() : "";
      return name || null;
    } catch (e) {
      ctx.host.log.info("StepFun plan status unavailable: " + String(e));
      return null;
    }
  }

  function progress(label, leftRate, resetDate, periodMs) {
    return {
      type: "progress",
      label: label,
      used: Math.max(0, Math.min(100, (1 - leftRate) * 100)),
      limit: 100,
      format: { kind: "percent" },
      resetsAt: resetDate.toISOString(),
      periodDurationMs: periodMs,
    };
  }

  function probe(ctx) {
    var tok = token(ctx);
    var usage = parseUsage(requestJson(ctx, API_URL, tok));
    var plan = planName(ctx, tok);
    return {
      displayName: "StepFun",
      source: "web",
      plan: plan,
      lines: [
        progress("5h Step Plan", usage.fiveHourLeft, usage.fiveHourReset, 5 * 60 * 60 * 1000),
        progress("Weekly Step Plan", usage.weeklyLeft, usage.weeklyReset, 7 * 24 * 60 * 60 * 1000),
      ],
    };
  }

  globalThis.__openusage_plugin = { id: "stepfun", probe: probe };
})();
