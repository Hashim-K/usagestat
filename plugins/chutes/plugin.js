(function () {
  var DEFAULT_BASE = "https://api.chutes.ai";

  function env(ctx, name) {
    try {
      var value = ctx.host.env.get(name);
      return typeof value === "string" && value.trim() ? value.trim() : null;
    } catch (_) {
      return null;
    }
  }

  function apiKey(ctx) {
    var configured = ctx.provider && typeof ctx.provider.apiKey === "string" ? ctx.provider.apiKey.trim() : "";
    return configured || env(ctx, "CHUTES_API_KEY");
  }

  function apiBase(ctx) {
    var configured = ctx.provider && ctx.provider.settings && typeof ctx.provider.settings.apiUrl === "string"
      ? ctx.provider.settings.apiUrl.trim()
      : "";
    var base = configured || env(ctx, "CHUTES_API_URL") || DEFAULT_BASE;
    base = base.replace(/\/+$/, "");
    if (!/^https:\/\//i.test(base)) throw "Chutes API URL must be HTTPS.";
    return base;
  }

  function numberValue(value) {
    if (typeof value === "number" && Number.isFinite(value)) return value;
    if (typeof value === "string" && value.trim()) {
      var parsed = Number(value.replace(/,/g, ""));
      if (Number.isFinite(parsed)) return parsed;
    }
    return null;
  }

  function boolValue(value) {
    if (typeof value === "boolean") return value;
    if (typeof value === "string") {
      var lower = value.trim().toLowerCase();
      if (lower === "true" || lower === "active") return true;
      if (lower === "false" || lower === "inactive") return false;
    }
    return null;
  }

  function first(obj, keys) {
    if (!obj || typeof obj !== "object") return undefined;
    for (var i = 0; i < keys.length; i++) {
      if (obj[keys[i]] !== undefined && obj[keys[i]] !== null) return obj[keys[i]];
    }
    return undefined;
  }

  function requestJson(ctx, base, path, key) {
    var resp = ctx.util.request({
      method: "GET",
      url: base + path,
      headers: { Authorization: "Bearer " + key, Accept: "application/json" },
      timeoutMs: 15000,
    });
    if (ctx.util.isAuthStatus(resp.status)) throw "Chutes API key was rejected.";
    if (resp.status < 200 || resp.status >= 300) {
      throw "Chutes API request failed (HTTP " + resp.status + ").";
    }
    var json = ctx.util.tryParseJson(resp.bodyText);
    if (!json) throw "Chutes response was not valid JSON.";
    return json;
  }

  function dataRoot(root) {
    if (root && root.data && typeof root.data === "object" && !Array.isArray(root.data)) return root.data;
    if (root && root.result && typeof root.result === "object" && !Array.isArray(root.result)) return root.result;
    return root && typeof root === "object" ? root : {};
  }

  function parseDate(value) {
    if (typeof value === "number" && Number.isFinite(value)) return new Date(value > 10000000000 ? value : value * 1000);
    if (typeof value === "string" && value.trim()) {
      var numeric = Number(value);
      if (Number.isFinite(numeric)) return parseDate(numeric);
      var ms = Date.parse(value);
      if (Number.isFinite(ms)) return new Date(ms);
    }
    return null;
  }

  function normalizePercent(value) {
    var n = numberValue(value);
    if (n == null) return null;
    if (n <= 1 && n >= 0) n = n * 100;
    return Math.max(0, Math.min(100, n));
  }

  function windowMinutes(payload) {
    var minutes = numberValue(first(payload, ["window_minutes", "windowMinutes", "period_minutes", "periodMinutes"]));
    if (minutes != null) return Math.round(minutes);
    var hours = numberValue(first(payload, ["window_hours", "windowHours", "period_hours", "periodHours"]));
    if (hours != null) return Math.round(hours * 60);
    var days = numberValue(first(payload, ["window_days", "windowDays", "period_days", "periodDays"]));
    if (days != null) return Math.round(days * 24 * 60);
    var seconds = numberValue(first(payload, ["window_seconds", "windowSeconds", "period_seconds", "periodSeconds"]));
    if (seconds != null) return Math.round(seconds / 60);
    var text = first(payload, ["window", "period", "duration"]);
    if (typeof text === "string") {
      var match = text.trim().toLowerCase().match(/^(\d+(?:\.\d+)?)\s*(m|min|minute|minutes|h|hr|hour|hours|d|day|days|mo|month|months)/);
      if (match) {
        var value = Number(match[1]);
        var unit = match[2];
        if (unit[0] === "m" && unit !== "mo" && unit.indexOf("month") !== 0) return Math.round(value);
        if (unit[0] === "h") return Math.round(value * 60);
        if (unit[0] === "d") return Math.round(value * 24 * 60);
        return Math.round(value * 30 * 24 * 60);
      }
    }
    return null;
  }

  function parseQuota(payload, fallbackLabel, fallbackWindow) {
    if (!payload || typeof payload !== "object") return null;
    var limit = numberValue(first(payload, ["limit", "quota", "max", "monthly_limit", "monthlyLimit"]));
    var used = numberValue(first(payload, ["used", "usage", "consumed", "current_usage", "currentUsage"]));
    var remaining = numberValue(first(payload, ["remaining", "available", "left"]));
    var percent = normalizePercent(first(payload, ["used_percent", "usedPercent", "usage_percent", "usagePercent", "percent_used", "percentUsed"]));
    var remainingPercent = normalizePercent(first(payload, ["remaining_percent", "remainingPercent", "percent_remaining", "percentRemaining"]));
    if (percent == null && remainingPercent != null) percent = 100 - remainingPercent;
    if (limit == null && used != null && remaining != null) limit = used + remaining;
    if (used == null && limit != null && remaining != null) used = Math.max(0, limit - remaining);
    if (percent == null && used != null && limit > 0) percent = (used / limit) * 100;
    if (percent == null) return null;

    return {
      label: String(first(payload, ["label", "name", "quota_name", "quotaName", "chute_id", "chuteId"]) || fallbackLabel || "Quota"),
      used: used,
      limit: limit,
      percent: Math.max(0, Math.min(100, percent)),
      windowMinutes: windowMinutes(payload) || fallbackWindow || null,
      resetsAt: parseDate(first(payload, ["resets_at", "resetsAt", "reset_at", "resetAt", "renews_at", "renewsAt", "expires_at", "expiresAt"])),
      unit: String(first(payload, ["unit", "units"]) || "credits"),
    };
  }

  function collectQuotaObjects(value, out) {
    if (!value || typeof value !== "object") return;
    if (Array.isArray(value)) {
      for (var i = 0; i < value.length; i++) collectQuotaObjects(value[i], out);
      return;
    }
    if (
      first(value, ["limit", "quota", "used", "usage", "remaining", "used_percent", "usedPercent", "usage_percent", "usagePercent"]) !== undefined
    ) {
      out.push(value);
    }
    var keys = Object.keys(value);
    for (var j = 0; j < keys.length; j++) collectQuotaObjects(value[keys[j]], out);
  }

  function kindFor(window) {
    var label = (window.label + " " + (window.unit || "")).toLowerCase();
    if (label.indexOf("rolling") >= 0 || label.indexOf("4-hour") >= 0 || label.indexOf("4h") >= 0 || window.windowMinutes === 240) return "rolling";
    if (label.indexOf("month") >= 0 || label.indexOf("billing") >= 0 || label.indexOf("subscription") >= 0 || (window.windowMinutes || 0) >= 28 * 24 * 60) return "monthly";
    return null;
  }

  function appendQuotaLine(lines, quota, label) {
    if (!quota) return;
    var text = "";
    if (quota.used != null && quota.limit != null) text = fmtAmount(quota.used) + " / " + fmtAmount(quota.limit) + " " + quota.unit;
    lines.push(ctxLineProgress(label || quota.label, quota.percent, quota.resetsAt, quota.windowMinutes));
    if (text) lines.push({ type: "text", label: (label || quota.label) + " Used", value: text });
  }

  function ctxLineProgress(label, percent, resetsAt, windowMinutes) {
    var line = {
      type: "progress",
      label: label,
      used: Math.max(0, Math.min(100, percent)),
      limit: 100,
      format: { kind: "percent" },
    };
    if (resetsAt) line.resetsAt = resetsAt.toISOString();
    if (windowMinutes) line.periodDurationMs = windowMinutes * 60 * 1000;
    return line;
  }

  function fmtAmount(value) {
    var n = Number(value) || 0;
    if (Math.abs(n - Math.round(n)) < 0.0001) return String(Math.round(n));
    return n.toFixed(2).replace(/\.?0+$/, "");
  }

  function parseSnapshot(root) {
    var data = dataRoot(root);
    var all = [];
    collectQuotaObjects(root, all);
    var windows = all.map(function (payload) { return parseQuota(payload, null, null); }).filter(Boolean);
    var rolling = parseQuota(first(data, ["rolling", "rolling_window", "rollingWindow", "four_hour", "fourHour"]), "4-hour quota", 240) ||
      windows.find(function (w) { return kindFor(w) === "rolling"; });
    var monthly = parseQuota(first(data, ["monthly", "monthly_window", "monthlyWindow", "subscription", "subscription_usage", "subscriptionUsage"]), "Monthly quota", 30 * 24 * 60) ||
      windows.find(function (w) { return kindFor(w) === "monthly"; });
    var active = boolValue(first(data, ["active", "is_active", "isActive"]));
    var status = String(first(data, ["status", "subscription_state", "subscriptionState"]) || "").trim();
    var plan = String(first(data, ["plan", "plan_name", "planName", "tier", "subscription_tier", "subscriptionTier"]) || "").trim();
    return { rolling: rolling, monthly: monthly, windows: windows, active: active, status: status, plan: plan };
  }

  function probe(ctx) {
    var key = apiKey(ctx);
    if (!key) throw "Missing Chutes API key. Set CHUTES_API_KEY or provider apiKey.";
    var base = apiBase(ctx);
    var snapshot = parseSnapshot(requestJson(ctx, base, "/users/me/subscription_usage", key));
    if (!snapshot.rolling || !snapshot.monthly) {
      try {
        var fallback = parseSnapshot(requestJson(ctx, base, "/users/me/quotas", key));
        snapshot.rolling = snapshot.rolling || fallback.rolling;
        snapshot.monthly = snapshot.monthly || fallback.monthly;
        snapshot.windows = snapshot.windows.concat(fallback.windows || []);
      } catch (e) {
        ctx.host.log.warn("chutes quota fallback failed: " + String(e));
      }
    }

    var lines = [];
    appendQuotaLine(lines, snapshot.rolling, "4-hour quota");
    appendQuotaLine(lines, snapshot.monthly, "Monthly quota");
    for (var i = 0; i < snapshot.windows.length && lines.length < 8; i++) {
      var w = snapshot.windows[i];
      if (w === snapshot.rolling || w === snapshot.monthly) continue;
      appendQuotaLine(lines, w, w.label);
    }
    if (!lines.length) lines.push({ type: "badge", label: "Status", text: snapshot.active === false ? "No active subscription" : "No usage data", color: "yellow" });
    return { displayName: "Chutes", source: "api", plan: snapshot.plan || snapshot.status || null, lines: lines };
  }

  globalThis.__openusage_plugin = { id: "chutes", probe: probe };
})();
