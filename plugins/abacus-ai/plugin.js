(function () {
  var COMPUTE_URL = "https://apps.abacus.ai/api/_getOrganizationComputePoints";
  var BILLING_URL = "https://apps.abacus.ai/api/_getBillingInfo";

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

  function cookieHeader(ctx) {
    var settings = ctx.provider && ctx.provider.settings ? ctx.provider.settings : {};
    var value = trim(ctx.provider && ctx.provider.cookieHeader) ||
      trim(settings.cookieHeader) ||
      trim(settings.cookie) ||
      env(ctx, "ABACUS_COOKIE");
    if (!value) throw "Abacus AI session not configured. Set ABACUS_COOKIE.";
    return value;
  }

  function numberValue(value) {
    if (typeof value === "number" && Number.isFinite(value)) return value;
    if (typeof value === "string" && value.trim()) {
      var parsed = Number(value);
      if (Number.isFinite(parsed)) return parsed;
    }
    return null;
  }

  function requestEnvelope(ctx, opts) {
    var resp = ctx.util.request(opts);
    if (ctx.util.isAuthStatus(resp.status)) throw "Abacus AI session expired. Log in again and update ABACUS_COOKIE.";
    if (resp.status < 200 || resp.status >= 300) {
      throw "Abacus AI API returned HTTP " + resp.status + ".";
    }
    var json = ctx.util.tryParseJson(resp.bodyText);
    if (!json || typeof json !== "object") throw "Abacus AI response was not valid JSON.";
    if (json.success !== true) {
      var message = typeof json.error === "string" && json.error.trim() ? json.error.trim() : "request failed";
      if (/expired|session|login|authenticate|unauthori[sz]ed|forbidden/i.test(message)) {
        throw "Abacus AI session expired. Log in again and update ABACUS_COOKIE.";
      }
      throw "Abacus AI API error: " + message;
    }
    if (!json.result || typeof json.result !== "object") {
      throw "Abacus AI response missing result.";
    }
    return json.result;
  }

  function fetchCompute(ctx, cookie) {
    return requestEnvelope(ctx, {
      method: "GET",
      url: COMPUTE_URL,
      headers: {
        Cookie: cookie,
        Accept: "application/json",
      },
      timeoutMs: 15000,
    });
  }

  function fetchBilling(ctx, cookie) {
    try {
      return requestEnvelope(ctx, {
        method: "POST",
        url: BILLING_URL,
        headers: {
          Cookie: cookie,
          Accept: "application/json",
          "Content-Type": "application/json",
        },
        bodyText: "{}",
        timeoutMs: 5000,
      });
    } catch (e) {
      ctx.host.log.warn("Abacus AI billing info unavailable: " + String(e));
      return null;
    }
  }

  function probe(ctx) {
    var cookie = cookieHeader(ctx);
    var compute = fetchCompute(ctx, cookie);
    var billing = fetchBilling(ctx, cookie);
    var total = numberValue(compute.totalComputePoints);
    var left = numberValue(compute.computePointsLeft);
    if (total === null || left === null) {
      throw "Abacus AI response missing credit fields.";
    }
    total = Math.max(0, total);
    left = Math.max(0, left);
    var used = Math.max(0, total - left);
    var opts = {
      label: "Credits",
      used: used,
      limit: total > 0 ? total : 1,
      format: { kind: "count", suffix: "cp" },
      detail: Math.round(used) + "/" + Math.round(total) + " cp",
    };
    var reset = billing && ctx.util.toIso(billing.nextBillingDate);
    if (reset) opts.resetsAt = reset;
    var plan = billing && typeof billing.currentTier === "string" && billing.currentTier.trim()
      ? billing.currentTier.trim()
      : null;
    return {
      displayName: "Abacus AI",
      source: "web",
      plan: plan,
      lines: [ctx.line.progress(opts)],
    };
  }

  globalThis.__openusage_plugin = { id: "abacus-ai", probe: probe };
})();
