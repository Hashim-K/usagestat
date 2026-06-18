(function () {
  var GATEWAY_BASE_URL = "https://bailian.console.aliyun.com";
  var DASHBOARD_URL = "https://bailian.console.aliyun.com/cn-beijing?tab=plan#/efm/subscription/token-plan";
  var TOKEN_PLAN_PRODUCT_CODE = "sfm_tokenplanteams_dp_cn";
  var CURRENT_REGION_ID = "cn-beijing";
  var BSS_SERVICE_CODE = "BssOpenAPI-V3";
  var ACTION = "GetSubscriptionSummary";
  var USER_AGENT = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36";

  var PLAN_NAME_KEYS = [
    "planName", "plan_name", "packageName", "package_name", "commodityName", "commodity_name",
    "instanceName", "instance_name", "displayName", "display_name", "name", "title",
    "planType", "plan_type", "ProductName", "productName",
  ];
  var USED_QUOTA_KEYS = [
    "usedQuota", "used_quota", "usedCredits", "usedCredit", "consumedCredits", "usage", "used",
    "usedAmount", "consumeAmount", "usedValue", "UsedValue", "consumedValue", "ConsumedValue",
  ];
  var TOTAL_QUOTA_KEYS = [
    "totalQuota", "total_quota", "totalCredits", "totalCredit", "quota", "creditLimit",
    "creditsTotal", "monthlyTotalQuota", "amount", "totalValue", "TotalValue", "totalCount",
    "TotalCount", "subscriptionTotalNumber", "SubscriptionTotalNumber",
  ];
  var REMAINING_QUOTA_KEYS = [
    "remainingQuota", "remainQuota", "remainingCredits", "remainingCredit", "availableCredits",
    "balance", "remaining", "availableAmount", "remainAmount", "totalSurplusValue",
    "TotalSurplusValue", "surplusValue", "SurplusValue",
  ];
  var RESET_DATE_KEYS = [
    "nextRefreshTime", "resetTime", "periodEndTime", "billingCycleEnd", "billCycleEndTime",
    "expireTime", "expirationTime", "endTime", "validEndTime", "instanceEndTime",
    "nearestExpireDate", "NearestExpireDate",
  ];

  function trim(value) {
    return typeof value === "string" && value.trim() ? value.trim() : null;
  }

  function cleaned(value) {
    var text = trim(value);
    if (!text) return null;
    if ((text[0] === "\"" && text[text.length - 1] === "\"") || (text[0] === "'" && text[text.length - 1] === "'")) {
      text = text.slice(1, -1).trim();
    }
    return text || null;
  }

  function env(ctx, name) {
    try {
      return cleaned(ctx.host.env.get(name));
    } catch (_) {
      return null;
    }
  }

  function settings(ctx) {
    return ctx.provider && ctx.provider.settings ? ctx.provider.settings : {};
  }

  function normalizeCookie(raw) {
    var text = cleaned(raw);
    if (!text) return null;
    if (text.slice(0, 7).toLowerCase() === "cookie:") text = text.slice(7).trim();
    return text && text.indexOf("=") >= 0 ? text : null;
  }

  function cookieHeader(ctx) {
    var s = settings(ctx);
    var value = normalizeCookie(ctx.provider && ctx.provider.cookieHeader) ||
      normalizeCookie(s.cookieHeader) ||
      normalizeCookie(s.cookie) ||
      normalizeCookie(env(ctx, "ALIBABA_TOKEN_PLAN_COOKIE")) ||
      normalizeCookie(env(ctx, "ALIBABA_TOKEN_PLAN_COOKIE_HEADER")) ||
      normalizeCookie(env(ctx, "BAILIAN_TOKEN_PLAN_COOKIE")) ||
      normalizeCookie(env(ctx, "ALIBABA_COOKIE"));
    if (!value) throw "Alibaba Token Plan session not configured. Set ALIBABA_TOKEN_PLAN_COOKIE.";
    return value;
  }

  function cookieValue(name, cookie) {
    var parts = String(cookie || "").split(";");
    for (var i = 0; i < parts.length; i++) {
      var idx = parts[i].indexOf("=");
      if (idx < 0) continue;
      var key = parts[i].slice(0, idx).trim();
      var value = parts[i].slice(idx + 1).trim();
      if (key === name && value) return value;
    }
    return null;
  }

  function hostBase(ctx) {
    var raw = cleaned(settings(ctx).host) || env(ctx, "ALIBABA_TOKEN_PLAN_HOST") || GATEWAY_BASE_URL;
    if (!/^https:\/\//i.test(raw)) raw = "https://" + raw;
    return raw.replace(/\/+$/, "");
  }

  function quotaUrl(ctx) {
    var raw = cleaned(settings(ctx).quotaUrl) || env(ctx, "ALIBABA_TOKEN_PLAN_QUOTA_URL");
    if (raw) {
      if (!/^https:\/\//i.test(raw)) raw = "https://" + raw;
      return raw;
    }
    return hostBase(ctx) + "/data/api.json?action=" + encodeURIComponent(ACTION) +
      "&product=" + encodeURIComponent(BSS_SERVICE_CODE) + "&_tag=";
  }

  function dashboardUrl(ctx) {
    var base = hostBase(ctx);
    return base + "/cn-beijing?tab=plan#/efm/subscription/token-plan";
  }

  function extractSecToken(html) {
    var patterns = [
      /"secToken"\s*:\s*"([^"]+)"/,
      /"sec_token"\s*:\s*"([^"]+)"/,
      /secToken['"]?\s*[:=]\s*['"]([^'"]+)['"]/,
      /sec_token['"]?\s*[:=]\s*['"]([^'"]+)['"]/,
    ];
    for (var i = 0; i < patterns.length; i++) {
      var match = patterns[i].exec(String(html || ""));
      if (match && match[1] && match[1].trim()) return match[1].trim();
    }
    return null;
  }

  function resolveSecToken(ctx, cookie) {
    try {
      var resp = ctx.util.request({
        method: "GET",
        url: dashboardUrl(ctx),
        headers: {
          Cookie: cookie,
          Accept: "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
          "User-Agent": USER_AGENT,
        },
        timeoutMs: 10000,
      });
      if (resp.status >= 200 && resp.status < 300) {
        var token = extractSecToken(resp.bodyText);
        if (token) return token;
      }
    } catch (_) {}

    try {
      var base = hostBase(ctx);
      var userInfo = ctx.util.request({
        method: "GET",
        url: base + "/tool/user/info.json",
        headers: {
          Cookie: cookie,
          Accept: "application/json, text/plain, */*",
          Referer: base + "/",
          "User-Agent": USER_AGENT,
        },
        timeoutMs: 10000,
      });
      if (userInfo.status >= 200 && userInfo.status < 300) {
        var expanded = expandJsonStrings(ctx.util.tryParseJson(userInfo.bodyText));
        var fromInfo = findFirstString(expanded, ["secToken", "sec_token"]);
        if (fromInfo) return fromInfo;
      }
    } catch (_) {}

    return cookieValue("sec_token", cookie);
  }

  function formBody(secToken) {
    var parts = [
      ["product", BSS_SERVICE_CODE],
      ["action", ACTION],
      ["params", JSON.stringify({ ProductCode: TOKEN_PLAN_PRODUCT_CODE })],
      ["region", CURRENT_REGION_ID],
    ];
    if (secToken) parts.push(["sec_token", secToken]);
    return parts.map(function (part) {
      return encodeURIComponent(part[0]) + "=" + encodeURIComponent(part[1]);
    }).join("&");
  }

  function isLoginText(text) {
    var lower = String(text || "").toLowerCase();
    return lower.indexOf("needlogin") >= 0 || lower.indexOf("login") >= 0 ||
      lower.indexOf("postonlyortokenerror") >= 0 || lower.indexOf("tokenerror") >= 0 ||
      lower.indexOf("request has expired") >= 0 || lower.indexOf("refresh page") >= 0 ||
      lower.indexOf("请求已经过期") >= 0;
  }

  function fetchPayload(ctx, cookie) {
    var headers = {
      Cookie: cookie,
      Accept: "*/*",
      "Content-Type": "application/x-www-form-urlencoded",
      Origin: hostBase(ctx),
      Referer: dashboardUrl(ctx),
      "User-Agent": USER_AGENT,
      "X-Requested-With": "XMLHttpRequest",
    };
    var csrf = cookieValue("login_aliyunid_csrf", cookie) || cookieValue("csrf", cookie);
    if (csrf) {
      headers["x-xsrf-token"] = csrf;
      headers["x-csrf-token"] = csrf;
    }
    var resp = ctx.util.request({
      method: "POST",
      url: quotaUrl(ctx),
      headers: headers,
      bodyText: formBody(resolveSecToken(ctx, cookie)),
      timeoutMs: 20000,
    });
    if (ctx.util.isAuthStatus(resp.status)) throw "Alibaba Token Plan login required.";
    if (resp.status < 200 || resp.status >= 300) throw "Alibaba Token Plan API error: HTTP " + resp.status + ".";
    var json = ctx.util.tryParseJson(resp.bodyText);
    if (!json) {
      if (/<html/i.test(String(resp.bodyText || "")) && isLoginText(resp.bodyText)) {
        throw "Alibaba Token Plan login required.";
      }
      throw "Could not parse Alibaba Token Plan usage: Invalid JSON response.";
    }
    return expandJsonStrings(json);
  }

  function expandJsonStrings(value) {
    if (Array.isArray(value)) return value.map(expandJsonStrings);
    if (value && typeof value === "object") {
      var out = {};
      Object.keys(value).forEach(function (key) { out[key] = expandJsonStrings(value[key]); });
      return out;
    }
    if (typeof value === "string") {
      var text = value.trim();
      if ((text[0] === "{" && text[text.length - 1] === "}") || (text[0] === "[" && text[text.length - 1] === "]")) {
        try {
          return expandJsonStrings(JSON.parse(text));
        } catch (_) {}
      }
    }
    return value;
  }

  function directString(obj, keys) {
    if (!obj || typeof obj !== "object" || Array.isArray(obj)) return null;
    for (var i = 0; i < keys.length; i++) {
      var value = obj[keys[i]];
      if (typeof value === "string" && value.trim()) return value.trim();
    }
    return null;
  }

  function parseNumber(value) {
    if (typeof value === "number" && Number.isFinite(value)) return value;
    if (typeof value === "string" && value.trim()) {
      var parsed = Number(value.replace(/,/g, ""));
      if (Number.isFinite(parsed)) return parsed;
    }
    return null;
  }

  function directNumber(obj, keys) {
    if (!obj || typeof obj !== "object" || Array.isArray(obj)) return null;
    for (var i = 0; i < keys.length; i++) {
      var value = parseNumber(obj[keys[i]]);
      if (value !== null) return value;
    }
    return null;
  }

  function parseBool(value) {
    if (typeof value === "boolean") return value;
    if (typeof value === "number") return value !== 0;
    if (typeof value === "string") {
      var lower = value.trim().toLowerCase();
      if (/^(true|1|yes|active|valid|normal)$/.test(lower)) return true;
      if (/^(false|0|no|inactive|invalid|expired)$/.test(lower)) return false;
    }
    return null;
  }

  function findFirstString(value, keys) {
    if (Array.isArray(value)) {
      for (var i = 0; i < value.length; i++) {
        var inArray = findFirstString(value[i], keys);
        if (inArray) return inArray;
      }
      return null;
    }
    if (!value || typeof value !== "object") return null;
    var direct = directString(value, keys);
    if (direct) return direct;
    var objectKeys = Object.keys(value);
    for (var j = 0; j < objectKeys.length; j++) {
      var nested = findFirstString(value[objectKeys[j]], keys);
      if (nested) return nested;
    }
    return null;
  }

  function findFirstNumber(value, keys) {
    if (Array.isArray(value)) {
      for (var i = 0; i < value.length; i++) {
        var inArray = findFirstNumber(value[i], keys);
        if (inArray !== null) return inArray;
      }
      return null;
    }
    if (!value || typeof value !== "object") return null;
    var direct = directNumber(value, keys);
    if (direct !== null) return direct;
    var objectKeys = Object.keys(value);
    for (var j = 0; j < objectKeys.length; j++) {
      var nested = findFirstNumber(value[objectKeys[j]], keys);
      if (nested !== null) return nested;
    }
    return null;
  }

  function findFirstBool(value, keys) {
    if (Array.isArray(value)) {
      for (var i = 0; i < value.length; i++) {
        var inArray = findFirstBool(value[i], keys);
        if (inArray !== null) return inArray;
      }
      return null;
    }
    if (!value || typeof value !== "object") return null;
    for (var k = 0; k < keys.length; k++) {
      var parsed = parseBool(value[keys[k]]);
      if (parsed !== null) return parsed;
    }
    var objectKeys = Object.keys(value);
    for (var j = 0; j < objectKeys.length; j++) {
      var nested = findFirstBool(value[objectKeys[j]], keys);
      if (nested !== null) return nested;
    }
    return null;
  }

  function parseDate(ctx, value) {
    var n = parseNumber(value);
    if (n !== null && n > 1000000000) return ctx.util.toIso(n);
    var text = trim(value);
    if (!text) return null;
    if (/^\d{4}-\d{2}-\d{2}$/.test(text)) return ctx.util.toIso(text + "T00:00:00Z");
    if (/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}/.test(text)) return ctx.util.toIso(text.replace(" ", "T") + "Z");
    return ctx.util.toIso(text);
  }

  function findFirstDate(ctx, value, keys) {
    if (Array.isArray(value)) {
      for (var i = 0; i < value.length; i++) {
        var inArray = findFirstDate(ctx, value[i], keys);
        if (inArray) return inArray;
      }
      return null;
    }
    if (!value || typeof value !== "object") return null;
    for (var k = 0; k < keys.length; k++) {
      var direct = parseDate(ctx, value[keys[k]]);
      if (direct) return direct;
    }
    var objectKeys = Object.keys(value);
    for (var j = 0; j < objectKeys.length; j++) {
      var nested = findFirstDate(ctx, value[objectKeys[j]], keys);
      if (nested) return nested;
    }
    return null;
  }

  function hasAnyKey(obj, keyGroups) {
    if (!obj || typeof obj !== "object" || Array.isArray(obj)) return false;
    var keys = [].concat.apply([], keyGroups);
    for (var i = 0; i < keys.length; i++) {
      if (Object.prototype.hasOwnProperty.call(obj, keys[i])) return true;
    }
    return false;
  }

  function activeSignalScore(obj) {
    var status = (directString(obj, ["status", "instanceStatus", "state"]) || "").toUpperCase();
    if (/^(VALID|ACTIVE|NORMAL)$/.test(status)) return 3;
    if (/^(EXPIRED|INVALID|INACTIVE|DISABLED|TERMINATED|STOPPED)$/.test(status)) return -1;
    var active = parseBool(obj && (obj.isActive !== undefined ? obj.isActive : obj.active));
    return active === null ? 0 : active ? 3 : -1;
  }

  function findObjectByKeys(value, keys) {
    if (Array.isArray(value)) {
      for (var i = 0; i < value.length; i++) {
        var inArray = findObjectByKeys(value[i], keys);
        if (inArray) return inArray;
      }
      return null;
    }
    if (!value || typeof value !== "object") return null;
    for (var k = 0; k < keys.length; k++) {
      if (value[keys[k]] && typeof value[keys[k]] === "object" && !Array.isArray(value[keys[k]])) return value[keys[k]];
    }
    var objectKeys = Object.keys(value);
    for (var j = 0; j < objectKeys.length; j++) {
      var nested = findObjectByKeys(value[objectKeys[j]], keys);
      if (nested) return nested;
    }
    return null;
  }

  function findArrayByKeys(value, keys) {
    if (Array.isArray(value)) {
      for (var i = 0; i < value.length; i++) {
        var inArray = findArrayByKeys(value[i], keys);
        if (inArray) return inArray;
      }
      return null;
    }
    if (!value || typeof value !== "object") return null;
    for (var k = 0; k < keys.length; k++) {
      if (Array.isArray(value[keys[k]])) return value[keys[k]];
    }
    var objectKeys = Object.keys(value);
    for (var j = 0; j < objectKeys.length; j++) {
      var nested = findArrayByKeys(value[objectKeys[j]], keys);
      if (nested) return nested;
    }
    return null;
  }

  function findObjectWithQuotaKeys(value) {
    if (Array.isArray(value)) {
      for (var i = 0; i < value.length; i++) {
        var inArray = findObjectWithQuotaKeys(value[i]);
        if (inArray) return inArray;
      }
      return null;
    }
    if (!value || typeof value !== "object") return null;
    if (hasAnyKey(value, [USED_QUOTA_KEYS, TOTAL_QUOTA_KEYS, REMAINING_QUOTA_KEYS])) return value;
    var objectKeys = Object.keys(value);
    for (var j = 0; j < objectKeys.length; j++) {
      var nested = findObjectWithQuotaKeys(value[objectKeys[j]]);
      if (nested) return nested;
    }
    return null;
  }

  function findTokenPlanInstance(value) {
    var object = findObjectByKeys(value, ["tokenPlanInstanceInfo", "token_plan_instance_info", "instanceInfo", "instance_info"]);
    if (object) return object;
    var array = findArrayByKeys(value, [
      "tokenPlanInstanceInfos", "token_plan_instance_infos", "instanceInfos", "instances", "Data", "data", "successResponse",
    ]);
    if (!array) return null;
    var best = null;
    var bestScore = -999;
    for (var i = 0; i < array.length; i++) {
      if (!array[i] || typeof array[i] !== "object" || Array.isArray(array[i])) continue;
      var score = activeSignalScore(array[i]);
      if (!best || score > bestScore) {
        best = array[i];
        bestScore = score;
      }
    }
    return best;
  }

  function throwIfErrorPayload(value) {
    var success = findFirstBool(value, ["success", "Success"]);
    if (success === false) {
      var msg = findFirstString(value, ["message", "msg", "Message", "errorMessage", "Code", "code"]) || "request failed";
      if (isLoginText(msg)) throw "Alibaba Token Plan login required.";
      throw "Alibaba Token Plan API error: " + msg;
    }
    var codeText = (findFirstString(value, ["code", "status", "statusCode"]) || "").toLowerCase();
    var messageText = (findFirstString(value, ["message", "msg", "statusMessage"]) || "").toLowerCase();
    if (isLoginText(codeText + " " + messageText)) throw "Alibaba Token Plan login required.";
    var statusCode = findFirstNumber(value, ["statusCode", "status_code", "code"]);
    if (statusCode !== null && statusCode !== 0 && statusCode !== 200) {
      if (statusCode === 401 || statusCode === 403) throw "Alibaba Token Plan login required.";
      throw "Alibaba Token Plan API error: " + (messageText || "status code " + statusCode);
    }
  }

  function formatQuota(value) {
    if (!Number.isFinite(value)) return "";
    var rounded = Math.round(value);
    if (Math.abs(rounded - value) < 0.000001) return String(rounded).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
    return value.toFixed(2).replace(/\.?0+$/, "").replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  }

  function usageSnapshot(ctx, payload) {
    throwIfErrorPayload(payload);
    var instance = findTokenPlanInstance(payload);
    var scope = instance || payload;
    var quota = findObjectByKeys(scope, ["quotaInfo", "quota_info", "tokenPlanQuotaInfo", "token_plan_quota_info"]) ||
      findObjectWithQuotaKeys(scope) ||
      findObjectWithQuotaKeys(payload);
    var planName = directString(scope, PLAN_NAME_KEYS) || findFirstString(payload, PLAN_NAME_KEYS);
    var used = quota ? directNumber(quota, USED_QUOTA_KEYS) : null;
    var total = quota ? directNumber(quota, TOTAL_QUOTA_KEYS) : null;
    var remaining = quota ? directNumber(quota, REMAINING_QUOTA_KEYS) : null;
    if (used === null) used = findFirstNumber(scope, USED_QUOTA_KEYS);
    if (total === null) total = findFirstNumber(scope, TOTAL_QUOTA_KEYS);
    if (remaining === null) remaining = findFirstNumber(scope, REMAINING_QUOTA_KEYS);
    if (used === null && total !== null && remaining !== null) used = Math.max(0, total - remaining);
    var resetsAt = findFirstDate(ctx, scope, RESET_DATE_KEYS) || findFirstDate(ctx, payload, RESET_DATE_KEYS);
    if (!planName && (total !== null || used !== null || remaining !== null)) planName = "TOKEN PLAN";
    if (planName === null && total === null && used === null && remaining === null) {
      throw "Could not parse Alibaba Token Plan usage: missing token plan data.";
    }
    if (total === null || total <= 0 || used === null) {
      throw "Could not parse Alibaba Token Plan usage: quota totals missing.";
    }
    return {
      planName: planName,
      used: Math.max(0, Math.min(total, used)),
      total: total,
      remaining: remaining,
      resetsAt: resetsAt,
    };
  }

  function probe(ctx) {
    var snapshot = usageSnapshot(ctx, fetchPayload(ctx, cookieHeader(ctx)));
    var detail = formatQuota(snapshot.used) + " / " + formatQuota(snapshot.total) + " credits used";
    var progress = {
      label: "Credits",
      used: snapshot.used,
      limit: snapshot.total,
      format: { kind: "count", suffix: "credits" },
      detail: detail,
      periodDurationMs: 30 * 24 * 60 * 60 * 1000,
    };
    if (snapshot.resetsAt) progress.resetsAt = snapshot.resetsAt;
    return {
      displayName: "Alibaba Token Plan",
      source: "web",
      plan: snapshot.planName,
      lines: [
        ctx.line.progress(progress),
        ctx.line.text({ label: "Usage", value: detail }),
      ],
    };
  }

  globalThis.__openusage_plugin = { id: "alibaba-token-plan", probe: probe };
})();
