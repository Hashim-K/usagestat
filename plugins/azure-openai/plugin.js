(function () {
  var DEFAULT_API_VERSION = "2024-10-21";

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

  function parseSavedConfig(ctx, raw) {
    var text = trim(raw);
    if (!text) return null;
    var parsed = ctx.util.tryParseJson(text);
    if (parsed && typeof parsed === "object") {
      return {
        apiKey: trim(parsed.apiKey || parsed.api_key),
        endpoint: trim(parsed.endpoint),
        deployment: trim(parsed.deployment || parsed.deploymentName || parsed.deployment_name),
        apiVersion: trim(parsed.apiVersion || parsed.api_version),
      };
    }
    var parts = text.split("|").map(function (part) { return part.trim(); });
    if (parts.length >= 3) {
      return {
        apiKey: trim(parts[0]),
        endpoint: trim(parts[1]),
        deployment: trim(parts[2]),
        apiVersion: trim(parts[3]),
      };
    }
    return { apiKey: text };
  }

  function resolveConfig(ctx) {
    var s = settings(ctx);
    var saved = parseSavedConfig(ctx, ctx.provider && ctx.provider.apiKey);
    var config = {
      apiKey: trim(s.apiKey) || (saved && saved.apiKey) || env(ctx, "AZURE_OPENAI_API_KEY"),
      endpoint: trim(s.endpoint) || (saved && saved.endpoint) || env(ctx, "AZURE_OPENAI_ENDPOINT"),
      deployment: trim(s.deployment) || trim(s.deploymentName) || (saved && saved.deployment) ||
        env(ctx, "AZURE_OPENAI_DEPLOYMENT") || env(ctx, "AZURE_OPENAI_DEPLOYMENT_NAME"),
      apiVersion: trim(s.apiVersion) || trim(s.api_version) || (saved && saved.apiVersion) ||
        env(ctx, "AZURE_OPENAI_API_VERSION") || DEFAULT_API_VERSION,
    };
    if (!config.apiKey) throw "Azure OpenAI API key not configured. Set AZURE_OPENAI_API_KEY.";
    if (!config.endpoint) throw "Azure OpenAI endpoint not configured. Set AZURE_OPENAI_ENDPOINT.";
    if (!config.deployment) throw "Azure OpenAI deployment not configured. Set AZURE_OPENAI_DEPLOYMENT.";
    return config;
  }

  function endpointBase(raw) {
    var value = String(raw || "").trim().replace(/\/+$/, "");
    if (!/^https?:\/\//i.test(value)) value = "https://" + value;
    return value;
  }

  function stripSuffix(value, suffix) {
    var lower = value.toLowerCase();
    return lower.endsWith(suffix) ? value.slice(0, value.length - suffix.length) : value;
  }

  function chatUrl(config) {
    var base = endpointBase(config.endpoint);
    if (String(config.apiVersion).trim().toLowerCase() === "v1") {
      base = stripSuffix(stripSuffix(base, "/openai/v1"), "/openai");
      return base + "/openai/v1/chat/completions";
    }
    base = stripSuffix(base, "/openai");
    return base + "/openai/deployments/" + encodeURIComponent(config.deployment) +
      "/chat/completions?api-version=" + encodeURIComponent(config.apiVersion);
  }

  function validationBody(config) {
    if (String(config.apiVersion).trim().toLowerCase() === "v1") {
      return JSON.stringify({
        model: config.deployment,
        messages: [{ role: "user", content: "ping" }],
        max_completion_tokens: 1,
      });
    }
    return JSON.stringify({
      messages: [{ role: "user", content: "ping" }],
      max_tokens: 1,
    });
  }

  function responseSummary(bodyText) {
    var collapsed = String(bodyText || "").split(/\s+/).filter(Boolean).join(" ");
    return collapsed.length > 240 ? collapsed.slice(0, 240) + "... [truncated]" : collapsed;
  }

  function probe(ctx) {
    var config = resolveConfig(ctx);
    var resp;
    try {
      resp = ctx.util.request({
        method: "POST",
        url: chatUrl(config),
        headers: {
          "api-key": config.apiKey,
          Accept: "application/json",
          "Content-Type": "application/json",
        },
        bodyText: validationBody(config),
        timeoutMs: 15000,
      });
    } catch (_) {
      throw "Azure OpenAI validation request failed. Check your connection.";
    }

    if (ctx.util.isAuthStatus(resp.status)) {
      throw "Azure OpenAI API key is invalid or unauthorized.";
    }
    if (resp.status < 200 || resp.status >= 300) {
      throw "Azure OpenAI API error: HTTP " + resp.status + ": " + responseSummary(resp.bodyText);
    }

    var parsed = ctx.util.tryParseJson(resp.bodyText);
    if (!parsed || typeof parsed !== "object") {
      throw "Azure OpenAI response was not valid JSON.";
    }
    var model = trim(parsed.model);
    var subtitle = model ? "Deployment: " + config.deployment + " / Model: " + model : "Deployment: " + config.deployment;
    return {
      displayName: "Azure OpenAI",
      source: "api",
      plan: config.deployment,
      lines: [
        ctx.line.badge({ label: "Deployment", text: "Validated", color: "#22c55e", subtitle: subtitle }),
      ],
    };
  }

  globalThis.__openusage_plugin = { id: "azure-openai", probe: probe };
})();
