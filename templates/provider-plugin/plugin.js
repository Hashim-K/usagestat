function errorResult(message) {
  return {
    displayName: "Custom Provider Template",
    source: "error",
    metrics: [
      { type: "badge", label: "Error", text: message, color: "red" }
    ]
  };
}

function apiExample(ctx) {
  var token = ctx.host.env.get("OPENAI_API_KEY");
  if (!token) return errorResult("Set OPENAI_API_KEY or update the template env var.");

  var result = ctx.util.requestJson({
    url: "https://api.example.com/v1/usage",
    method: "GET",
    headers: { Authorization: "Bearer " + token },
    timeoutMs: 10000
  });

  if (ctx.util.isAuthStatus(result.resp.status)) {
    return errorResult("API token was rejected.");
  }
  if (result.resp.status < 200 || result.resp.status >= 300) {
    return errorResult("API request returned HTTP " + result.resp.status + ".");
  }

  var usage = result.json || {};
  var used = Number(usage.used || 0);
  var limit = Number(usage.limit || 100);

  return {
    displayName: "Custom Provider Template",
    source: "api",
    plan: ctx.fmt.planLabel(usage.plan || "api plan"),
    metrics: [
      ctx.line.progress({
        label: "API",
        used: used,
        limit: limit,
        format: { kind: "count", suffix: "requests" },
        resetsAt: ctx.util.toIso(usage.resetsAt)
      })
    ]
  };
}

function oauthExample(ctx) {
  var tokenPath = ctx.app.pluginDataDir + "/oauth.json";
  if (!ctx.host.fs.exists(tokenPath)) {
    return errorResult("Missing OAuth state at " + tokenPath + ".");
  }

  var state = ctx.util.tryParseJson(ctx.host.fs.readText(tokenPath));
  if (!state || !state.accessToken) return errorResult("OAuth state is invalid.");

  var result = ctx.util.requestJson({
    url: "https://api.example.com/v1/me/usage",
    method: "GET",
    headers: { Authorization: "Bearer " + state.accessToken },
    timeoutMs: 10000
  });

  if (ctx.util.isAuthStatus(result.resp.status)) {
    return errorResult("OAuth token needs refresh.");
  }

  var usage = result.json || {};
  return {
    displayName: "Custom Provider Template",
    source: "oauth",
    plan: ctx.fmt.planLabel(usage.plan || state.plan || "oauth plan"),
    metrics: [
      ctx.line.progress({
        label: "Session",
        used: Number(usage.percentUsed || 0),
        limit: 100,
        format: { kind: "percent" },
        resetsAt: ctx.util.toIso(usage.resetsAt)
      })
    ]
  };
}

function localExample(ctx) {
  var path = ctx.host.fs.homeDir + "/.config/example-provider/usage.json";
  if (!ctx.host.fs.exists(path)) {
    return errorResult("Local usage file not found.");
  }

  var data = ctx.util.tryParseJson(ctx.host.fs.readText(path));
  if (!data) return errorResult("Local usage file is not valid JSON.");

  return {
    displayName: "Custom Provider Template",
    source: "local",
    metrics: [
      ctx.line.text({ label: "Account", value: String(data.account || "default") }),
      ctx.line.progress({
        label: "Local",
        used: Number(data.used || 0),
        limit: Number(data.limit || 100),
        format: { kind: "percent" },
        resetsAt: ctx.util.toIso(data.resetsAt)
      })
    ]
  };
}

function cliExample(ctx) {
  if (!ctx.host.command || typeof ctx.host.command.run !== "function") {
    return errorResult("Command host API is unavailable.");
  }

  // The backend currently only allows the `gh` command. Extend the host allowlist
  // before using another CLI here.
  var result = ctx.host.command.run({
    program: "gh",
    args: ["auth", "status"],
    timeoutMs: 10000
  });

  return {
    displayName: "Custom Provider Template",
    source: "cli",
    metrics: [
      ctx.line.badge({
        label: "CLI",
        text: result.status === 0 ? "available" : "unavailable",
        color: result.status === 0 ? "green" : "yellow"
      }),
      ctx.line.text({ label: "Exit", value: String(result.status) })
    ]
  };
}

function webExample(ctx) {
  var cookie = ctx.host.env.get("OLLAMA_COOKIE");
  if (!cookie) return errorResult("Set/import a web session cookie.");

  var result = ctx.util.requestJson({
    url: (ctx.webUrl || "https://example.com") + "/api/usage",
    method: "GET",
    headers: { Cookie: cookie },
    timeoutMs: 10000
  });

  if (ctx.util.isAuthStatus(result.resp.status)) {
    return errorResult("Web session is expired.");
  }

  var usage = result.json || {};
  return {
    displayName: "Custom Provider Template",
    source: "web",
    metrics: [
      ctx.line.progress({
        label: "Web",
        used: Number(usage.used || 0),
        limit: Number(usage.limit || 100),
        format: { kind: "percent" },
        resetsAt: ctx.util.toIso(usage.resetsAt)
      })
    ]
  };
}

globalThis.__usagestat_plugin = {
  probe(ctx) {
    if (ctx.sourceMode === "api" || ctx.sourceMode === "auto") return apiExample(ctx);
    if (ctx.sourceMode === "oauth") return oauthExample(ctx);
    if (ctx.sourceMode === "local") return localExample(ctx);
    if (ctx.sourceMode === "cli") return cliExample(ctx);
    if (ctx.sourceMode === "web") return webExample(ctx);
    return errorResult("Unsupported source mode: " + ctx.sourceMode);
  }
};
