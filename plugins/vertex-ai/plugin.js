(function () {
  var TOKEN_URL = "https://oauth2.googleapis.com/token";
  var RESOURCE_MANAGER_URL = "https://cloudresourcemanager.googleapis.com/v1/projects/";

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

  function readJson(ctx, path) {
    if (!path || !ctx.host.fs.exists(path)) return null;
    try {
      return ctx.util.tryParseJson(ctx.host.fs.readText(path));
    } catch (_) {
      return null;
    }
  }

  function adcPath(ctx) {
    var explicit = env(ctx, "GOOGLE_APPLICATION_CREDENTIALS");
    if (explicit) return explicit;
    var configDir = env(ctx, "CLOUDSDK_CONFIG");
    if (configDir) return configDir.replace(/\/+$/, "") + "/application_default_credentials.json";
    return (ctx.host.fs.homeDir || "~") + "/.config/gcloud/application_default_credentials.json";
  }

  function gcloudPropertiesPath(ctx) {
    var configDir = env(ctx, "CLOUDSDK_CONFIG");
    if (configDir) return configDir.replace(/\/+$/, "") + "/properties";
    return (ctx.host.fs.homeDir || "~") + "/.config/gcloud/properties";
  }

  function refreshAccessToken(ctx, creds) {
    if (!creds || !trim(creds.refresh_token) || !trim(creds.client_id) || !trim(creds.client_secret)) return null;
    var resp = ctx.util.request({
      method: "POST",
      url: TOKEN_URL,
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      bodyText:
        "client_id=" + encodeURIComponent(creds.client_id) +
        "&client_secret=" + encodeURIComponent(creds.client_secret) +
        "&refresh_token=" + encodeURIComponent(creds.refresh_token) +
        "&grant_type=refresh_token",
      timeoutMs: 15000,
    });
    if (ctx.util.isAuthStatus(resp.status) || resp.status === 400) {
      throw "Vertex AI credentials expired. Run `gcloud auth application-default login` again.";
    }
    if (resp.status < 200 || resp.status >= 300) {
      ctx.host.log.warn("Vertex AI token refresh returned HTTP " + resp.status);
      return null;
    }
    var body = ctx.util.tryParseJson(resp.bodyText);
    return body && trim(body.access_token);
  }

  function tokenFromGcloud(ctx, args) {
    try {
      if (!ctx.host.command || typeof ctx.host.command.run !== "function") return null;
      var result = ctx.host.command.run({ program: "gcloud", args: args, timeoutMs: 15000 });
      if (result && result.status === 0 && typeof result.stdout === "string") {
        return trim(result.stdout);
      }
      if (result && result.stderr) ctx.host.log.info("gcloud token command failed: " + String(result.stderr).trim());
    } catch (e) {
      ctx.host.log.info("gcloud token command unavailable: " + String(e));
    }
    return null;
  }

  function accessToken(ctx) {
    var configured = setting(ctx, ["accessToken", "token"]) || (ctx.provider && trim(ctx.provider.apiKey));
    if (configured) return configured;

    var creds = readJson(ctx, adcPath(ctx));
    if (creds && typeof creds === "object") {
      if (trim(creds.access_token)) return trim(creds.access_token);
      var refreshed = refreshAccessToken(ctx, creds);
      if (refreshed) return refreshed;
    }

    return tokenFromGcloud(ctx, ["auth", "application-default", "print-access-token"]) ||
      tokenFromGcloud(ctx, ["auth", "print-access-token"]);
  }

  function projectFromProperties(ctx) {
    var path = gcloudPropertiesPath(ctx);
    if (!ctx.host.fs.exists(path)) return null;
    try {
      var lines = ctx.host.fs.readText(path).split(/\r?\n/);
      for (var i = 0; i < lines.length; i++) {
        var line = lines[i].trim();
        if (line.indexOf("project") === 0 && line.indexOf("=") >= 0) {
          return trim(line.split("=").slice(1).join("="));
        }
      }
    } catch (_) {}
    return null;
  }

  function projectId(ctx) {
    var configured = setting(ctx, ["projectId", "project", "googleCloudProject"]) ||
      env(ctx, "GOOGLE_CLOUD_PROJECT") ||
      env(ctx, "GCLOUD_PROJECT") ||
      projectFromProperties(ctx);
    if (configured) return configured;
    var creds = readJson(ctx, adcPath(ctx));
    return creds && trim(creds.project_id);
  }

  function validateProject(ctx, token, project) {
    if (!project) return null;
    try {
      var resp = ctx.util.request({
        method: "GET",
        url: RESOURCE_MANAGER_URL + encodeURIComponent(project),
        headers: {
          Authorization: "Bearer " + token,
          Accept: "application/json",
        },
        timeoutMs: 15000,
      });
      if (ctx.util.isAuthStatus(resp.status)) throw "Vertex AI credentials expired. Run `gcloud auth application-default login` again.";
      if (resp.status < 200 || resp.status >= 300) {
        ctx.host.log.warn("Vertex AI project validation returned HTTP " + resp.status);
        return null;
      }
      var json = ctx.util.tryParseJson(resp.bodyText);
      return json && (trim(json.name) || trim(json.projectId));
    } catch (e) {
      if (typeof e === "string") throw e;
      ctx.host.log.warn("Vertex AI project validation failed: " + String(e));
      return null;
    }
  }

  function probe(ctx) {
    var token = accessToken(ctx);
    if (!token) {
      throw "Vertex AI credentials not configured. Run gcloud auth application-default login or set GOOGLE_APPLICATION_CREDENTIALS.";
    }
    var project = projectId(ctx);
    var projectName = validateProject(ctx, token, project) || project || "configured";
    return {
      displayName: "Vertex AI",
      source: project ? "oauth" : "cli",
      plan: project ? "Vertex AI (" + project + ")" : "Vertex AI",
      lines: [
        ctx.line.badge({ label: "Status", text: "Authenticated", color: "#22c55e" }),
        ctx.line.text({ label: "Project", value: projectName }),
      ],
    };
  }

  globalThis.__openusage_plugin = { id: "vertex-ai", probe: probe };
})();
