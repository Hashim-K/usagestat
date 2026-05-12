globalThis.__ai_usage_plugin = {
  probe(ctx) {
    const apiKey = readApiKey(ctx);
    const apiBase = readApiBase(ctx);

    const credits = requestJson(ctx, {
      method: "GET",
      url: apiBase + "/credits",
      headers: {
        "Authorization": "Bearer " + apiKey,
        "Accept": "application/json"
      },
      timeoutMs: 15000
    });

    if (credits.resp.status === 401 || credits.resp.status === 403) {
      throw "OpenRouter API key is invalid.";
    }
    if (credits.resp.status < 200 || credits.resp.status >= 300) {
      throw "OpenRouter credits request failed (HTTP " + credits.resp.status + ").";
    }

    const data = credits.json && credits.json.data ? credits.json.data : {};
    const totalCredits = readNumber(data.total_credits);
    const totalUsage = readNumber(data.total_usage);

    if (totalCredits === null || totalUsage === null) {
      throw "OpenRouter credits response did not include usage totals.";
    }

    const remaining = Math.max(0, totalCredits - totalUsage);
    const usedPercent = totalCredits > 0 ? clamp((totalUsage / totalCredits) * 100, 0, 100) : 0;
    const metrics = [
      {
        type: "progress",
        label: "Credits",
        used: round2(totalUsage),
        limit: round2(totalCredits),
        format: { kind: "dollars" }
      },
      {
        type: "text",
        label: "Balance",
        value: "$" + round2(remaining).toFixed(2)
      },
      {
        type: "text",
        label: "Used",
        value: "$" + round2(totalUsage).toFixed(2) + " (" + round1(usedPercent) + "%)"
      }
    ];

    try {
      const key = requestJson(ctx, {
        method: "GET",
        url: apiBase + "/key",
        headers: {
          "Authorization": "Bearer " + apiKey,
          "Accept": "application/json"
        },
        timeoutMs: 15000
      });
      if (key.resp.status >= 200 && key.resp.status < 300 && key.json && key.json.data) {
        appendKeyMetrics(metrics, key.json.data);
      }
    } catch (error) {
      ctx.host.log.warn("OpenRouter key details request failed: " + String(error));
    }

    return {
      displayName: "OpenRouter",
      source: "api",
      metrics
    };
  }
};

function readApiKey(ctx) {
  const value = ctx.host.env.get("OPENROUTER_API_KEY");
  if (typeof value === "string" && value.trim()) {
    return value.trim();
  }
  throw "No OPENROUTER_API_KEY found.";
}

function readApiBase(ctx) {
  const value = ctx.host.env.get("OPENROUTER_API_BASE");
  if (typeof value === "string" && value.trim()) {
    return value.trim().replace(/\/+$/, "");
  }
  return "https://openrouter.ai/api/v1/auth";
}

function requestJson(ctx, request) {
  let resp;
  try {
    resp = ctx.host.http.request(request);
  } catch (error) {
    ctx.host.log.error("HTTP request failed: " + String(error));
    throw "OpenRouter request failed. Check your connection.";
  }

  let json = null;
  try {
    json = resp.bodyText ? JSON.parse(resp.bodyText) : null;
  } catch {
    throw "OpenRouter response was not valid JSON.";
  }

  return { resp, json };
}

function appendKeyMetrics(metrics, data) {
  const limit = readNumber(data.limit);
  const usage = readNumber(data.usage);
  if (limit !== null && usage !== null && limit > 0) {
    metrics.push({
      type: "progress",
      label: "Key Limit",
      used: round2(usage),
      limit: round2(limit),
      format: { kind: "dollars" }
    });
  }

  const rateLimit = data.rate_limit || data.rateLimit;
  if (rateLimit && rateLimit.requests !== undefined) {
    const interval = typeof rateLimit.interval === "string" ? " / " + rateLimit.interval : "";
    metrics.push({
      type: "text",
      label: "Rate Limit",
      value: String(rateLimit.requests) + " requests" + interval
    });
  }
}

function readNumber(value) {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string" && value.trim()) {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) return parsed;
  }
  return null;
}

function clamp(value, min, max) {
  return Math.max(min, Math.min(max, value));
}

function round1(value) {
  return Math.round(value * 10) / 10;
}

function round2(value) {
  return Math.round(value * 100) / 100;
}
