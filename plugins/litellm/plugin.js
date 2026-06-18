(function () {
  function env(ctx, name) {
    try {
      var value = ctx.host.env.get(name);
      return typeof value === "string" && value.trim() ? value.trim() : null;
    } catch (_) {
      return null;
    }
  }

  function apiKey(ctx) {
    var configured = ctx.provider && typeof ctx.provider.apiKey === "string" ? ctx.provider.apiKey.trim() : "";
    return configured || env(ctx, "LITELLM_API_KEY");
  }

  function setting(ctx, names) {
    var settings = ctx.provider && ctx.provider.settings ? ctx.provider.settings : {};
    for (var i = 0; i < names.length; i++) {
      var value = settings[names[i]];
      if (typeof value === "string" && value.trim()) return value.trim();
    }
    return null;
  }

  function baseUrl(ctx) {
    var configured = setting(ctx, ["enterpriseHost", "baseUrl", "baseURL", "apiUrl", "apiURL"]);
    var base = configured || env(ctx, "LITELLM_BASE_URL");
    if (!base) throw "Missing LiteLLM base URL. Set LITELLM_BASE_URL or provider settings.enterpriseHost.";
    base = base.trim().replace(/\/+$/, "");
    if (base.toLowerCase().endsWith("/v1")) base = base.slice(0, -3).replace(/\/+$/, "");
    if (!/^https?:\/\//i.test(base)) throw "LiteLLM base URL is invalid.";
    return base;
  }

  function numberValue(value) {
    if (typeof value === "number" && Number.isFinite(value)) return value;
    if (typeof value === "string" && value.trim()) {
      var parsed = Number(value.replace(/[$,]/g, ""));
      if (Number.isFinite(parsed)) return parsed;
    }
    return null;
  }

  function nonEmpty(value) {
    return typeof value === "string" && value.trim() ? value.trim() : null;
  }

  function parseDate(value) {
    if (typeof value !== "string" || !value.trim()) return null;
    var ms = Date.parse(value);
    return Number.isFinite(ms) ? new Date(ms) : null;
  }

  function requestJson(ctx, base, path, key) {
    var resp = ctx.util.request({
      method: "GET",
      url: base + path,
      headers: { Authorization: "Bearer " + key, Accept: "application/json" },
      timeoutMs: 15000,
    });
    if (ctx.util.isAuthStatus(resp.status)) throw "LiteLLM API key was rejected.";
    if (resp.status < 200 || resp.status >= 300) {
      throw "LiteLLM API request failed (HTTP " + resp.status + ").";
    }
    var json = ctx.util.tryParseJson(resp.bodyText);
    if (!json) throw "LiteLLM response was not valid JSON.";
    return json;
  }

  function keyInfo(json) {
    var info = json && json.info ? json.info : json;
    var userID = nonEmpty(info && (info.user_id || info.userID));
    var teamID = nonEmpty(info && (info.team_id || info.teamID));
    if (!userID && !teamID) throw "LiteLLM key info did not include a user_id or team_id.";
    return {
      userID: userID,
      teamID: teamID,
      keyName: nonEmpty(info.key_name || info.keyName),
      spendUSD: numberValue(info.spend) || 0,
      expiresAt: parseDate(info.expires),
    };
  }

  function findTeam(teams, teamID) {
    if (!Array.isArray(teams) || !teamID) return null;
    for (var i = 0; i < teams.length; i++) {
      if (String(teams[i].team_id || teams[i].teamID || "") === teamID) return teams[i];
    }
    return null;
  }

  function usd(value) {
    return "$" + (Number(value) || 0).toFixed(2);
  }

  function spendDetail(spend, budget, prefix) {
    var value = budget != null && budget > 0 ? usd(spend) + " / " + usd(budget) : usd(spend);
    return prefix ? prefix + ": " + value : value;
  }

  function spendLabel(label) {
    return label.replace(/\s+budget$/i, " spend");
  }

  function addBudget(lines, label, spend, budget, resetAt, detailPrefix) {
    if (budget != null && budget > 0) {
      var line = ctxLineProgress(label, (spend / budget) * 100);
      if (resetAt) line.resetsAt = resetAt.toISOString();
      line.detail = spendDetail(spend, budget, detailPrefix);
      lines.push(line);
      return true;
    } else if (spend > 0) {
      lines.push({ type: "text", label: spendLabel(label), value: spendDetail(spend, null, detailPrefix) });
      return true;
    }
    return false;
  }

  function ctxLineProgress(label, percent) {
    return {
      type: "progress",
      label: label,
      used: Math.max(0, Math.min(100, percent)),
      limit: 100,
      format: { kind: "percent" },
    };
  }

  function userSnapshot(ctx, base, key, info) {
    var encoded = encodeURIComponent(info.userID);
    var json = requestJson(ctx, base, "/user/info?user_id=" + encoded, key);
    var user = json.user_info || json.userInfo || json;
    var responseID = nonEmpty(user.user_id || user.userID || json.user_id || json.userID);
    if (responseID && responseID !== info.userID) throw "LiteLLM user_id did not match /key/info.";
    var team = findTeam(json.teams, info.teamID);
    return {
      userID: info.userID,
      email: nonEmpty(user.user_email || user.userEmail || user.user_alias || user.userAlias || (user.metadata && user.metadata.preferred_username)),
      personalSpendUSD: numberValue(user.spend) || 0,
      personalBudgetUSD: numberValue(user.max_budget || user.maxBudget),
      personalResetAt: parseDate(user.budget_reset_at || user.budgetResetAt),
      team: team ? {
        id: String(team.team_id || team.teamID),
        alias: nonEmpty(team.team_alias || team.teamAlias),
        spendUSD: numberValue(team.spend) || 0,
        budgetUSD: numberValue(team.max_budget || team.maxBudget),
        resetAt: parseDate(team.budget_reset_at || team.budgetResetAt),
      } : null,
      keyName: info.keyName,
      keyExpiresAt: info.expiresAt,
    };
  }

  function teamSnapshot(ctx, base, key, info) {
    var encoded = encodeURIComponent(info.teamID);
    var json = requestJson(ctx, base, "/team/info?team_id=" + encoded, key);
    var team = json.team_info || json.teamInfo || json;
    var responseID = nonEmpty(team.team_id || team.teamID || json.team_id || json.teamID);
    if (responseID && responseID !== info.teamID) throw "LiteLLM team_id did not match /key/info.";
    return {
      userID: null,
      email: null,
      personalSpendUSD: 0,
      personalBudgetUSD: null,
      personalResetAt: null,
      team: {
        id: info.teamID,
        alias: nonEmpty(team.team_alias || team.teamAlias),
        spendUSD: numberValue(team.spend) || 0,
        budgetUSD: numberValue(team.max_budget || team.maxBudget),
        resetAt: parseDate(team.budget_reset_at || team.budgetResetAt),
      },
      keyName: info.keyName,
      keyExpiresAt: info.expiresAt,
    };
  }

  function linesFor(snapshot) {
    var lines = [];
    addBudget(lines, "Personal budget", snapshot.personalSpendUSD, snapshot.personalBudgetUSD, snapshot.personalResetAt);
    if (snapshot.team) addBudget(lines, "Team budget", snapshot.team.spendUSD, snapshot.team.budgetUSD, snapshot.team.resetAt, snapshot.team.alias ? "Team " + snapshot.team.alias : "Team");
    if (!lines.length) lines.push({ type: "text", label: "Spend", value: usd(snapshot.personalSpendUSD || (snapshot.team && snapshot.team.spendUSD) || 0) });
    if (snapshot.keyName) lines.push({ type: "text", label: "Key", value: snapshot.keyName });
    if (snapshot.keyExpiresAt) lines.push({ type: "text", label: "Key Expires", value: snapshot.keyExpiresAt.toISOString().slice(0, 10) });
    return lines;
  }

  function probe(ctx) {
    var key = apiKey(ctx);
    if (!key) throw "Missing LiteLLM API key. Set LITELLM_API_KEY or provider apiKey.";
    var base = baseUrl(ctx);
    var info = keyInfo(requestJson(ctx, base, "/key/info", key));
    var snapshot = info.userID ? userSnapshot(ctx, base, key, info) : teamSnapshot(ctx, base, key, info);
    var plan = snapshot.team && snapshot.team.alias ? snapshot.team.alias : null;
    return { displayName: "LiteLLM", source: "api", plan: plan, lines: linesFor(snapshot) };
  }

  globalThis.__openusage_plugin = { id: "litellm", probe: probe };
})();
