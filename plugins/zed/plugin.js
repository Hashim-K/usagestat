(function () {
  var DEFAULT_SERVICE_URL = "https://zed.dev";
  var DEFAULT_SERVER = "https://cloud.zed.dev";
  var SETTINGS_PATH = "~/.config/zed/settings.json";

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

  function settingsValue(settings, names) {
    if (!settings || typeof settings !== "object") return null;
    for (var i = 0; i < names.length; i++) {
      var value = settings[names[i]];
      if (typeof value === "string" && value.trim()) return value.trim();
    }
    return null;
  }

  function normalizeUrl(value) {
    if (typeof value !== "string" || !value.trim()) return null;
    return value.trim().replace(/\/+$/, "");
  }

  function readSettings(ctx) {
    try {
      var settings = ctx.util.tryParseJson(ctx.host.fs.readText(SETTINGS_PATH));
      return settings && typeof settings === "object" ? settings : {};
    } catch (_) {
      return {};
    }
  }

  function isTrustedServer(url) {
    return url === "https://zed.dev" || url === "https://staging.zed.dev";
  }

  function authConfig(ctx, zedSettings) {
    var serverUrl = normalizeUrl(
      setting(ctx, ["serverUrl", "serverURL"]) ||
      env(ctx, "ZED_SERVER_URL") ||
      settingsValue(zedSettings, ["server_url", "serverUrl", "serverURL"]) ||
      DEFAULT_SERVICE_URL
    );
    var credentialsUrl = normalizeUrl(
      setting(ctx, ["credentialsUrl", "credentialsURL"]) ||
      settingsValue(zedSettings, ["credentials_url", "credentialsUrl", "credentialsURL"]) ||
      serverUrl ||
      DEFAULT_SERVICE_URL
    );

    if (!serverUrl || !/^https:\/\//i.test(serverUrl)) throw "Zed server URL must be HTTPS.";
    if (!credentialsUrl || !/^https:\/\//i.test(credentialsUrl)) throw "Zed credentials URL must be HTTPS.";
    if (!isTrustedServer(serverUrl) && credentialsUrl !== serverUrl) {
      throw "Zed custom server credentials URL must match the server URL.";
    }

    return {
      serverBase: isTrustedServer(serverUrl) ? DEFAULT_SERVER : serverUrl,
      credentialsUrl: credentialsUrl,
    };
  }

  function explicitCredentials(ctx) {
    var userID = setting(ctx, ["userId", "userID"]) ||
      (ctx.provider && typeof ctx.provider.workspaceId === "string" ? ctx.provider.workspaceId.trim() : "") ||
      env(ctx, "ZED_USER_ID");
    var accessToken = (ctx.provider && typeof ctx.provider.apiKey === "string" ? ctx.provider.apiKey.trim() : "") ||
      setting(ctx, ["accessToken"]) ||
      env(ctx, "ZED_ACCESS_TOKEN");
    return userID && accessToken ? { userID: userID, accessToken: accessToken, source: "explicit" } : null;
  }

  function keychainCredentialFromRaw(ctx, raw) {
    var parsed = typeof raw === "string" ? ctx.util.tryParseJson(raw) : raw;
    if (!parsed || typeof parsed !== "object") return null;
    var account = typeof parsed.account === "string" && parsed.account.trim() ? parsed.account.trim() : null;
    var password = typeof parsed.password === "string" && parsed.password.trim() ? parsed.password.trim() : null;
    if (!account || !password) return null;
    return { userID: account, accessToken: password, source: "keychain" };
  }

  function readKeychainCredentials(ctx, credentialsUrl) {
    var keychain = ctx.host.keychain;
    if (!keychain) return null;

    if (typeof keychain.readInternetPassword === "function") {
      try {
        var internet = keychainCredentialFromRaw(ctx, keychain.readInternetPassword(credentialsUrl));
        if (internet) return internet;
      } catch (e) {
        ctx.host.log.info("Zed internet keychain read failed: " + String(e));
      }
    }

    if (typeof keychain.readGenericPasswordItem === "function") {
      try {
        var generic = keychainCredentialFromRaw(ctx, keychain.readGenericPasswordItem(credentialsUrl));
        if (generic) return generic;
      } catch (e) {
        ctx.host.log.info("Zed generic keychain read failed: " + String(e));
      }
    }

    return null;
  }

  function credentials(ctx, credentialsUrl) {
    var explicit = explicitCredentials(ctx);
    if (explicit) return explicit;

    var keychainCredentials = readKeychainCredentials(ctx, credentialsUrl);
    if (keychainCredentials) {
      ctx.host.log.info("Zed credentials loaded from keychain");
      return keychainCredentials;
    }

    throw "Not signed in to Zed. Sign in from the Zed editor app with GitHub, or set ZED_USER_ID and ZED_ACCESS_TOKEN.";
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
    var zedSettings = readSettings(ctx);
    var config = authConfig(ctx, zedSettings);
    var creds = credentials(ctx, config.credentialsUrl);
    var json = requestUser(ctx, creds, config.serverBase);
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
