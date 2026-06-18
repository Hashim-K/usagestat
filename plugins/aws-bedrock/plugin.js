(function () {
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
      if (typeof value === "number" && Number.isFinite(value)) return String(value);
    }
    return null;
  }

  function numberValue(value) {
    if (typeof value === "number" && Number.isFinite(value)) return value;
    if (typeof value === "string" && value.trim()) {
      var parsed = Number(value.replace(/[$,]/g, ""));
      if (Number.isFinite(parsed)) return parsed;
    }
    return null;
  }

  function credentials(ctx) {
    var accessKeyID = setting(ctx, ["accessKeyId", "accessKeyID"]) || env(ctx, "AWS_ACCESS_KEY_ID");
    var secretAccessKey = setting(ctx, ["secretAccessKey"]) || env(ctx, "AWS_SECRET_ACCESS_KEY");
    var sessionToken = setting(ctx, ["sessionToken"]) || env(ctx, "AWS_SESSION_TOKEN");
    if (accessKeyID && secretAccessKey) {
      return { accessKeyID: accessKeyID, secretAccessKey: secretAccessKey, sessionToken: sessionToken };
    }
    if (env(ctx, "AWS_PROFILE")) {
      throw "AWS profile auth is not supported yet. Export AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY, or run through an AWS credential wrapper that sets them.";
    }
    throw "AWS credentials not configured. Set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY.";
  }

  function configuredRegion(ctx) {
    return setting(ctx, ["region"]) || env(ctx, "AWS_REGION") || env(ctx, "AWS_DEFAULT_REGION") || "us-east-1";
  }

  function configuredBudget(ctx) {
    return numberValue(setting(ctx, ["budget", "budgetUsd", "budgetUSD"]) || env(ctx, "USAGESTAT_BEDROCK_BUDGET") || env(ctx, "CODEXBAR_BEDROCK_BUDGET"));
  }

  function configuredApiUrl(ctx) {
    var url = setting(ctx, ["apiUrl", "apiURL"]) || env(ctx, "USAGESTAT_BEDROCK_API_URL") || env(ctx, "CODEXBAR_BEDROCK_API_URL");
    return url && url.trim() ? url.trim().replace(/\/+$/, "") : null;
  }

  function dateString(date) {
    return date.toISOString().slice(0, 10);
  }

  function currentMonthRange() {
    var now = new Date();
    var start = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), 1));
    var tomorrow = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate() + 1));
    return { start: dateString(start), end: dateString(tomorrow) };
  }

  function isDataUnavailable(status, body) {
    if (status !== 400 || !body || typeof body !== "object") return false;
    var nested = body.Error || body.error || {};
    var codes = [body.__type, body.code, body.Code, nested.Code, nested.code];
    for (var i = 0; i < codes.length; i++) {
      if (typeof codes[i] === "string" && codes[i].split("#").pop() === "DataUnavailableException") return true;
    }
    return false;
  }

  function sanitizeBody(text) {
    var compact = String(text || "").replace(/\s+/g, " ").trim();
    return compact.length > 180 ? compact.slice(0, 180) + "..." : compact;
  }

  function callCostExplorerPage(ctx, creds, range, nextPageToken) {
    if (!ctx.host.aws || typeof ctx.host.aws._costExplorerRaw !== "function") {
      throw "AWS Cost Explorer host API is unavailable.";
    }
    var raw = ctx.host.aws._costExplorerRaw(JSON.stringify({
      accessKeyId: creds.accessKeyID,
      secretAccessKey: creds.secretAccessKey,
      sessionToken: creds.sessionToken || null,
      apiUrl: configuredApiUrl(ctx),
      startDate: range.start,
      endDate: range.end,
      granularity: "DAILY",
      nextPageToken: nextPageToken || null,
    }));
    var response = ctx.util.tryParseJson(raw);
    if (!response) throw "AWS Cost Explorer host response was invalid.";
    var body = ctx.util.tryParseJson(response.bodyText || "");
    if (response.status === 200) {
      if (!body) throw "AWS Cost Explorer response was not valid JSON.";
      return body;
    }
    if (isDataUnavailable(response.status, body)) {
      ctx.host.log.info("AWS Cost Explorer data unavailable, treating Bedrock spend as zero.");
      return { ResultsByTime: [] };
    }
    throw "AWS Cost Explorer API returned HTTP " + response.status + ": " + sanitizeBody(response.bodyText);
  }

  function callCostExplorer(ctx, creds, range) {
    var pages = [];
    var nextPageToken = null;
    var seen = {};
    do {
      var page = callCostExplorerPage(ctx, creds, range, nextPageToken);
      pages.push(page);
      nextPageToken = typeof page.NextPageToken === "string" && page.NextPageToken.trim() ? page.NextPageToken.trim() : null;
      if (nextPageToken) {
        if (seen[nextPageToken]) throw "AWS Cost Explorer returned a repeated page token.";
        seen[nextPageToken] = true;
      }
    } while (nextPageToken);
    return pages;
  }

  function parsePages(pages) {
    var total = 0;
    var byDay = {};
    var services = {};
    for (var p = 0; p < pages.length; p++) {
      var results = Array.isArray(pages[p].ResultsByTime) ? pages[p].ResultsByTime : [];
      for (var r = 0; r < results.length; r++) {
        var date = results[r].TimePeriod && results[r].TimePeriod.Start;
        var groups = Array.isArray(results[r].Groups) ? results[r].Groups : [];
        for (var g = 0; g < groups.length; g++) {
          var keys = Array.isArray(groups[g].Keys) ? groups[g].Keys : [];
          var service = keys.length ? String(keys[0]) : "";
          if (service.toLowerCase().indexOf("bedrock") < 0) continue;
          var metric = groups[g].Metrics && groups[g].Metrics.UnblendedCost;
          var amount = numberValue(metric && metric.Amount);
          if (amount === null || amount <= 0) continue;
          total += amount;
          services[service] = true;
          if (typeof date === "string" && date) byDay[date] = (byDay[date] || 0) + amount;
        }
      }
    }
    var daily = [];
    var days = Object.keys(byDay).sort();
    for (var i = 0; i < days.length; i++) {
      daily.push({ date: days[i], costUsd: byDay[days[i]] });
    }
    return { total: total, daily: daily, services: Object.keys(services).sort() };
  }

  function ingestDaily(ctx, daily) {
    if (!daily.length || !ctx.host.usageDaily || typeof ctx.host.usageDaily.ingest !== "function") return;
    try {
      ctx.host.usageDaily.ingest({ displayName: "AWS Bedrock", source: "billing", daily: daily });
    } catch (e) {
      ctx.host.log.warn("AWS Bedrock daily ingest failed: " + String(e));
    }
  }

  function usd(value) {
    return "$" + (Number(value) || 0).toFixed(2);
  }

  function linesFor(total, budget, services) {
    var lines = [];
    if (budget !== null && budget > 0) {
      lines.push({
        type: "progress",
        label: "Monthly budget",
        used: total,
        limit: budget,
        format: { kind: "dollars" },
        detail: "Month to date",
      });
    } else {
      lines.push({ type: "text", label: "Month spend", value: usd(total) });
    }
    if (services && services.length) {
      lines.push({ type: "text", label: "Services", value: services.slice(0, 3).join(", ") });
    }
    return lines;
  }

  function probe(ctx) {
    var creds = credentials(ctx);
    var region = configuredRegion(ctx);
    var budget = configuredBudget(ctx);
    var range = currentMonthRange();
    var pages = callCostExplorer(ctx, creds, range);
    var parsed = parsePages(pages);
    ingestDaily(ctx, parsed.daily);
    return {
      displayName: "AWS Bedrock",
      source: "api",
      plan: region,
      lines: linesFor(parsed.total, budget, parsed.services),
    };
  }

  globalThis.__openusage_plugin = { id: "aws-bedrock", probe: probe };
})();
