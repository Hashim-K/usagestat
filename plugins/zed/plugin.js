(function () {
  var DEFAULT_SERVICE_URL = "https://zed.dev";
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

  function pathValue(ctx, name, fromEnv) {
    var value = fromEnv ? ctx.host.env.get(name) : (ctx.provider.settings || {})[name];
    if (value === undefined || value === null || value === "") return null;
    var absolute = typeof value === "string" && (ctx.app.platform === "windows"
      ? /^(?:[A-Za-z]:[\\/]|[\\/]{2}[^\\/]+[\\/][^\\/]+)/.test(value) : value[0] === "/");
    if (!absolute || value.indexOf("\u0000") >= 0) throw {code: "failed", message: "Zed " + name + " must be an absolute path."};
    return value;
  }

  function settingsPath(ctx) {
    var file = pathValue(ctx, "settingsPath", false);
    if (file) return file;
    var custom = pathValue(ctx, "userDataDir", false);
    if (custom) return custom.replace(/[\\/]+$/, "") + "/config/settings.json";
    if (ctx.app.platform === "macos") return ctx.host.fs.homeDir + "/.config/zed/settings.json";
    var flatpak = ctx.app.platform === "linux" ? pathValue(ctx, "FLATPAK_XDG_CONFIG_HOME", true) : null;
    if (flatpak) return flatpak.replace(/\/+$/, "") + "/zed/settings.json";
    var path = ctx.host.fs.appSupportPath((ctx.app.platform === "windows" ? "Zed" : "zed") + "/settings.json");
    if (!path) throw {code: "failed", message: "Zed config directory unavailable. Set settingsPath or userDataDir."};
    return path;
  }

  // Zed settings are JSON with comments and trailing commas. Keep comment-like
  // text inside strings intact, including escaped quotes and URL slashes.
  function parseSettings(text) {
    var clean = "", quoted = false, escaped = false;
    text = text.replace(/^\uFEFF/, "");
    for (var i = 0; i < text.length; i++) {
      var c = text[i], next = text[i + 1];
      if (quoted) {
        clean += c;
        if (escaped) escaped = false;
        else if (c === "\\") escaped = true;
        else if (c === '"') quoted = false;
      } else if (c === '"') { quoted = true; clean += c; }
      else if (c === "/" && next === "/") {
        while (i + 1 < text.length && text[i + 1] !== "\n") i++;
        clean += " ";
      } else if (c === "/" && next === "*") {
        var end = text.indexOf("*/", i + 2);
        if (end < 0) throw new Error("Unterminated settings comment");
        i = end + 1; clean += " ";
      } else clean += c;
    }
    var result = ""; quoted = false; escaped = false;
    for (var j = 0; j < clean.length; j++) {
      var ch = clean[j];
      if (quoted) {
        result += ch;
        if (escaped) escaped = false;
        else if (ch === "\\") escaped = true;
        else if (ch === '"') quoted = false;
      } else if (ch === '"') { quoted = true; result += ch; }
      else if (ch === "," && /^\s*[}\]]/.test(clean.slice(j + 1))) continue;
      else result += ch;
    }
    var parsed = JSON.parse(result);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) throw new Error("Settings must be an object");
    return parsed;
  }

  function readSettings(ctx) {
    var path = settingsPath(ctx);
    if (!ctx.host.fs.exists(path)) {
      if ((ctx.provider.settings || {}).settingsPath || (ctx.provider.settings || {}).userDataDir)
        throw {code: "credential-unavailable", message: "Selected Zed settings file is missing. Check settingsPath/userDataDir."};
      return {};
    }
    try {
      return parseSettings(ctx.host.fs.readText(path));
    } catch (_) {
      throw {code: "credential-unavailable", message: "Cannot read valid JSONC from the selected Zed settings file."};
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
