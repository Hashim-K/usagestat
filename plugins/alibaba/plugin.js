(function () {
  var USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/149.0.0.0 Safari/537.36";
  var INTL_PROFILE = {
    gateway: "https://modelstudio.console.alibabacloud.com",
    apiAction: "IntlBroadScopeAspnGateway",
    apiProduct: "sfm_bailian",
    apiMethod: "zeldaEasy.broadscope-bailian.codingPlan.queryCodingPlanInstanceInfoV2",
    commodityCode: "sfm_codingplan_public_intl",
    switchAgent: 313762,
    switchUserType: 3,
    consoleSite: "MODELSTUDIO_ALBABACLOUD",
    consoleDomain: "modelstudio.console.alibabacloud.com",
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

  function settings(ctx) {
    return ctx.provider && ctx.provider.settings ? ctx.provider.settings : {};
  }

  function regionFromValue(raw) {
    var value = String(raw || "").trim().toLowerCase();
    if (value === "us" || value === "us-east-1" || value === "useast" || value === "us-west-1") return "us";
    if (value === "germany" || value === "eu" || value === "eu-central-1" || value === "frankfurt") return "germany";
    if (value === "hongkong" || value === "hong-kong" || value === "hk" || value === "cn-hongkong") return "hongkong";
    if (value === "cn" || value === "china" || value === "china-mainland" || value === "china_mainland" || value === "mainland") return "cn";
    return "singapore";
  }

  function region(ctx) {
    var s = settings(ctx);
    return regionFromValue(
      trim(ctx.provider && ctx.provider.region) ||
      trim(s.region) ||
      trim(s.apiRegion) ||
      trim(s.api_region) ||
      env(ctx, "ALIBABA_CODING_PLAN_REGION")
    );
  }

  function regionCode(region) {
    return {
      singapore: "ap-southeast-1",
      us: "us-east-1",
      germany: "eu-central-1",
      hongkong: "cn-hongkong",
      cn: "cn-hangzhou",
    }[region] || "ap-southeast-1";
  }

  function profile(region, ctx) {
    var host = trim(settings(ctx).host) || env(ctx, "ALIBABA_CODING_PLAN_HOST");
    var base = region === "cn" ? "https://bailian.console.alibabacloud.com" : INTL_PROFILE.gateway;
    if (host) {
      base = /^https:\/\//i.test(host) ? host : "https://" + host;
      base = base.replace(/\/+$/, "");
    }
    if (region === "cn") {
      var cn = {};
      Object.keys(INTL_PROFILE).forEach(function (key) { cn[key] = INTL_PROFILE[key]; });
      cn.gateway = base;
      cn.consoleDomain = base.replace(/^https:\/\//i, "");
      return cn;
    }
    var intl = {};
    Object.keys(INTL_PROFILE).forEach(function (key) { intl[key] = INTL_PROFILE[key]; });
    intl.gateway = base;
    intl.consoleDomain = base.replace(/^https:\/\//i, "");
    return intl;
  }

  function dashboardUrl(region, profile) {
    return region === "cn" ? profile.gateway : profile.gateway + "/" + regionCode(region);
  }

  function cookieHeader(ctx) {
    var s = settings(ctx);
    var cookie = trim(ctx.provider && ctx.provider.cookieHeader) ||
      trim(s.cookieHeader) ||
      trim(s.cookie) ||
      env(ctx, "ALIBABA_CODING_PLAN_COOKIE") ||
      env(ctx, "ALIBABA_COOKIE");
    if (cookie) return cookie;
    if (env(ctx, "ALIBABA_CODING_PLAN_API_KEY")) {
      throw "Alibaba Coding Plan quota requires browser cookies; API keys cannot read console quotas.";
    }
    throw "Alibaba session not configured. Set ALIBABA_COOKIE or ALIBABA_CODING_PLAN_COOKIE.";
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

  function resolveSecToken(ctx, cookie, regionName, requestProfile) {
    try {
      var resp = ctx.util.request({
        method: "GET",
        url: dashboardUrl(regionName, requestProfile) + "?tab=plan",
        headers: {
          Cookie: cookie,
          "User-Agent": USER_AGENT,
          Accept: "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        },
        timeoutMs: 10000,
      });
      if (resp.status >= 200 && resp.status < 300) {
        return extractSecToken(resp.bodyText) || cookieValue("sec_token", cookie);
      }
    } catch (_) {}
    return cookieValue("sec_token", cookie);
  }

  function randomId() {
    return Math.random().toString(16).slice(2) + Date.now().toString(16);
  }

  function quotaUrl(ctx, requestProfile) {
    var override = trim(settings(ctx).quotaUrl) || env(ctx, "ALIBABA_CODING_PLAN_QUOTA_URL");
    if (override) {
      if (!/^https:\/\//i.test(override)) override = "https://" + override;
      return override;
    }
    return requestProfile.gateway + "/data/api.json?action=" + encodeURIComponent(requestProfile.apiAction) +
      "&product=" + encodeURIComponent(requestProfile.apiProduct) + "&_tag=";
  }

  function formEncode(pairs) {
    return pairs.map(function (pair) {
      return encodeURIComponent(pair[0]) + "=" + encodeURIComponent(pair[1]);
    }).join("&");
  }

  function fetchQuota(ctx, cookie, regionName, requestProfile, secToken) {
    var referer = dashboardUrl(regionName, requestProfile) + "?tab=plan";
    var feUrl = referer + "#/efm/subscription/coding-plan";
    var params = {
      Api: requestProfile.apiMethod,
      V: "1.0",
      Data: {
        queryCodingPlanInstanceInfoRequest: {
          commodityCode: requestProfile.commodityCode,
          onlyLatestOne: true,
        },
        cornerstoneParam: {
          feTraceId: randomId(),
          feURL: feUrl,
          protocol: "V2",
          console: "ONE_CONSOLE",
          productCode: "p_efm",
          switchAgent: requestProfile.switchAgent,
          switchUserType: requestProfile.switchUserType,
          domain: requestProfile.consoleDomain,
          consoleSite: requestProfile.consoleSite,
          userNickName: "",
          userPrincipalName: "",
          xsp_lang: "en-US",
          "X-Anonymous-Id": cookieValue("cna", cookie) || "",
        },
      },
    };
    var form = [
      ["action", requestProfile.apiAction],
      ["product", requestProfile.apiProduct],
      ["api", requestProfile.apiMethod],
      ["_v", "undefined"],
      ["params", JSON.stringify(params)],
      ["region", regionCode(regionName)],
    ];
    if (secToken) form.push(["sec_token", secToken]);
    var resp = ctx.util.request({
      method: "POST",
      url: quotaUrl(ctx, requestProfile),
      headers: {
        Cookie: cookie,
        "Content-Type": "application/x-www-form-urlencoded",
        Accept: "*/*",
        Origin: requestProfile.gateway,
        Referer: referer,
        "User-Agent": USER_AGENT,
        "sec-fetch-site": "same-origin",
        "sec-fetch-mode": "cors",
        "sec-fetch-dest": "empty",
      },
      bodyText: formEncode(form),
      timeoutMs: 20000,
    });
    if (ctx.util.isAuthStatus(resp.status)) throw "Alibaba Coding Plan login required.";
    if (resp.status < 200 || resp.status >= 300) throw "Alibaba Coding Plan API error: HTTP " + resp.status + ".";
    if (/^\s*</.test(String(resp.bodyText || ""))) throw "Alibaba Coding Plan login required.";
    var json = ctx.util.tryParseJson(resp.bodyText);
    if (!json) throw "Alibaba Coding Plan response was not valid JSON.";
    return json;
  }

  function pointer(value, path) {
    var current = value;
    for (var i = 0; i < path.length; i++) {
      if (!current || typeof current !== "object") return undefined;
      current = current[path[i]];
    }
    return current;
  }

  function numberValue(value) {
    if (typeof value === "number" && Number.isFinite(value)) return value;
    if (typeof value === "string" && value.trim()) {
      var parsed = Number(value.replace(/,/g, ""));
      if (Number.isFinite(parsed)) return parsed;
    }
    return null;
  }

  function formatTokens(value) {
    var rounded = Math.round(value || 0);
    return String(rounded).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  }

  function dateFromMs(ctx, value) {
    var ms = numberValue(value);
    return ms && ms > 0 ? ctx.util.toIso(ms) : null;
  }

  function progress(ctx, label, quota, usedKey, totalKey, resetKey, periodMs) {
    var used = numberValue(quota[usedKey]) || 0;
    var total = numberValue(quota[totalKey]) || 0;
    var line = {
      label: label,
      used: used,
      limit: total > 0 ? total : 1,
      format: { kind: "count", suffix: "tokens" },
      detail: formatTokens(used) + " / " + formatTokens(total) + " tokens",
      periodDurationMs: periodMs,
    };
    var reset = dateFromMs(ctx, quota[resetKey]);
    if (reset) line.resetsAt = reset;
    return ctx.line.progress(line);
  }

  function parseQuota(ctx, json) {
    var code = trim(json && json.code);
    if (code && code !== "200") {
      if (code === "401" || code === "403") throw "Alibaba Coding Plan login required.";
      throw "Alibaba Coding Plan API error: " + (trim(json.message) || code);
    }
    var ret = pointer(json, ["data", "DataV2", "ret"]);
    if (Array.isArray(ret)) {
      var joined = ret.join(";");
      if (joined.indexOf("No Authority") >= 0 || joined.indexOf("10032390") >= 0 || joined.indexOf("NeedLogin") >= 0) {
        throw "Alibaba Coding Plan login required.";
      }
    }
    var instances = pointer(json, ["data", "DataV2", "data", "data", "codingPlanInstanceInfos"]);
    if (!Array.isArray(instances) || instances.length === 0) {
      throw "Alibaba Coding Plan response missing codingPlanInstanceInfos.";
    }
    var instance = instances[0];
    for (var i = 0; i < instances.length; i++) {
      if (instances[i] && instances[i].status === "VALID") {
        instance = instances[i];
        break;
      }
    }
    var quota = instance && instance.codingPlanQuotaInfo;
    if (!quota || typeof quota !== "object") throw "Alibaba Coding Plan response missing quota info.";
    return {
      plan: trim(instance.instanceName) || "Coding Plan",
      quota: quota,
    };
  }

  function probe(ctx) {
    var regionName = region(ctx);
    var requestProfile = profile(regionName, ctx);
    var cookie = cookieHeader(ctx);
    var secToken = resolveSecToken(ctx, cookie, regionName, requestProfile);
    var parsed = parseQuota(ctx, fetchQuota(ctx, cookie, regionName, requestProfile, secToken));
    return {
      displayName: "Alibaba",
      source: "web",
      plan: parsed.plan,
      lines: [
        progress(ctx, "5-Hour", parsed.quota, "per5HourUsedQuota", "per5HourTotalQuota", "per5HourQuotaNextRefreshTime", 5 * 60 * 60 * 1000),
        progress(ctx, "Weekly", parsed.quota, "perWeekUsedQuota", "perWeekTotalQuota", "perWeekQuotaNextRefreshTime", 7 * 24 * 60 * 60 * 1000),
        progress(ctx, "Monthly", parsed.quota, "perBillMonthUsedQuota", "perBillMonthTotalQuota", "perBillMonthQuotaNextRefreshTime", 30 * 24 * 60 * 60 * 1000),
      ],
    };
  }

  globalThis.__openusage_plugin = { id: "alibaba", probe: probe };
})();
