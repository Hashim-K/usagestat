(function () {
  var DEFAULT_SERVER = "https://cloud.zed.dev";

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

  function credentials(ctx) {
    var userID = setting(ctx, ["userId", "userID"]) ||
      (ctx.provider && typeof ctx.provider.workspaceId === "string" ? ctx.provider.workspaceId.trim() : "") ||
      env(ctx, "ZED_USER_ID");
    var accessToken = (ctx.provider && typeof ctx.provider.apiKey === "string" ? ctx.provider.apiKey.trim() : "") ||
      setting(ctx, ["accessToken"]) ||
      env(ctx, "ZED_ACCESS_TOKEN");
    if (!userID || !accessToken) {
      throw "Missing Zed credentials. Set ZED_USER_ID and ZED_ACCESS_TOKEN, or provider workspaceId/apiKey.";
    }
    return { userID: userID, accessToken: accessToken };
  }

  function serverBase(ctx) {
    var configured = setting(ctx, ["serverUrl", "serverURL"]) || env(ctx, "ZED_SERVER_URL") || DEFAULT_SERVER;
    configured = configured.replace(/\/+$/, "");
    if (configured === "https://zed.dev" || configured === "https://staging.zed.dev") return DEFAULT_SERVER;
    if (!/^https:\/\//i.test(configured)) throw "Zed server URL must be HTTPS.";
    return configured;
  }

  function requestUser(ctx, creds, base) {
    var resp = ctx.util.request({
      method: "GET",
      url: base + "/client/users/me",
      headers: {
        Authorization: creds.userID + " " + creds.accessToken,
        Accept: "application/json",
      },
      timeoutMs: 15000,
    });
    if (ctx.util.isAuthStatus(resp.status)) throw "Zed credentials are invalid or expired. Sign in to Zed again.";
    if (resp.status < 200 || resp.status >= 300) throw "Zed cloud API returned HTTP " + resp.status + ".";
    var json = ctx.util.tryParseJson(resp.bodyText);
    if (!json) throw "Could not parse Zed account response.";
    return json;
  }

  function planLabel(plan) {
    var raw = String(plan && (plan.plan_v3 || plan.planV3 || plan.name || "") || "").trim();
    if (!raw) return null;
    return raw.split(/[_-]+/).map(function (part) {
      return part ? part.charAt(0).toUpperCase() + part.slice(1) : part;
    }).join(" ");
  }

  function parseDate(value) {
    if (typeof value !== "string" || !value.trim()) return null;
    var ms = Date.parse(value);
    return Number.isFinite(ms) ? new Date(ms) : null;
  }

  function usageLine(plan) {
    var usage = plan && plan.usage && plan.usage.edit_predictions;
    if (!usage) return { type: "badge", label: "Edit predictions", text: "Unavailable", color: "yellow" };
    var used = Number(usage.used) || 0;
    var limit = usage.limit;
    if (limit === "unlimited" || (limit && limit.unlimited)) {
      return { type: "text", label: "Edit predictions", value: String(Math.round(used)) + " used · unlimited" };
    }
    var numericLimit = typeof limit === "number" ? limit : Number(limit && limit.limited);
    if (!Number.isFinite(numericLimit) || numericLimit <= 0) {
      return { type: "text", label: "Edit predictions", value: String(Math.round(used)) + " used" };
    }
    var line = {
      type: "progress",
      label: "Edit predictions",
      used: used,
      limit: numericLimit,
      format: { kind: "count", suffix: "predictions" },
    };
    var ended = parseDate(plan.subscription_period && (plan.subscription_period.ended_at || plan.subscription_period.endedAt));
    if (ended) line.resetsAt = ended.toISOString();
    return line;
  }

  function probe(ctx) {
    var creds = credentials(ctx);
    var json = requestUser(ctx, creds, serverBase(ctx));
    var user = json.user || {};
    var plan = json.plan || {};
    var displayName = user.github_login || user.name ? "Zed (" + (user.github_login || user.name) + ")" : "Zed";
    var lines = [usageLine(plan)];
    if (plan.has_overdue_invoices || plan.hasOverdueInvoices) {
      lines.push({ type: "badge", label: "Invoices", text: "Overdue", color: "red" });
    }
    return { displayName: displayName, source: "api", plan: planLabel(plan), lines: lines };
  }

  globalThis.__openusage_plugin = { id: "zed", probe: probe };
})();
