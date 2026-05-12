(function () {
  var API_URL = "https://app.warp.dev/graphql/v2?op=GetRequestLimitInfo";
  var GRAPHQL_QUERY = "query GetRequestLimitInfo($requestContext: RequestContext!) { user(requestContext: $requestContext) { __typename ... on UserOutput { user { requestLimitInfo { isUnlimited nextRefreshTime requestLimit requestsUsedSinceLastRefresh } bonusGrants { requestCreditsGranted requestCreditsRemaining expiration } workspaces { bonusGrantsInfo { grants { requestCreditsGranted requestCreditsRemaining expiration } } } } } } }";

  function loadApiKey(ctx) {
    var v = ctx.host.env.get("WARP_API_KEY");
    if (typeof v === "string" && v.trim()) return v.trim();
    return null;
  }

  function probe(ctx) {
    var apiKey = loadApiKey(ctx);
    if (!apiKey) {
      throw "Warp API key not found. Set WARP_API_KEY.";
    }

    var body = JSON.stringify({
      query: GRAPHQL_QUERY,
      variables: {
        requestContext: {
          clientContext: {},
          osContext: { category: "Linux", name: "Linux", version: "6.0" },
        },
      },
      operationName: "GetRequestLimitInfo",
    });

    var result = ctx.util.requestJson({
      method: "POST",
      url: API_URL,
      headers: {
        "Content-Type": "application/json",
        Accept: "application/json",
        "x-warp-client-id": "warp-app",
        "x-warp-os-category": "Linux",
        "x-warp-os-name": "Linux",
        "x-warp-os-version": "6.0",
        Authorization: "Bearer " + apiKey,
        "User-Agent": "Warp/1.0",
      },
      bodyText: body,
      timeoutMs: 15000,
    });

    if (ctx.util.isAuthStatus(result.resp.status)) {
      throw "API key invalid or expired.";
    }
    if (result.resp.status < 200 || result.resp.status >= 300) {
      throw "Warp API error (HTTP " + result.resp.status + ").";
    }

    var json = result.json;
    if (!json) throw "Could not parse Warp response.";

    if (json.errors && json.errors.length > 0) {
      var msgs = json.errors.map(function (e) { return e.message || ""; }).filter(Boolean);
      throw msgs.length > 0 ? msgs.join(" | ") : "GraphQL request failed.";
    }

    var userData = json.data && json.data.user && json.data.user.user;
    if (!userData) throw "Missing user data in Warp response.";

    var limitInfo = userData.requestLimitInfo || {};
    var isUnlimited = limitInfo.isUnlimited === true;
    var requestLimit = limitInfo.requestLimit || 0;
    var requestsUsed = limitInfo.requestsUsedSinceLastRefresh || 0;

    var usedPct = isUnlimited ? 0 : (requestLimit > 0 ? Math.min(100, requestsUsed / requestLimit * 100) : 0);

    var progressOpts = {
      label: "Credits",
      used: usedPct,
      limit: 100,
      format: { kind: "percent" },
    };
    if (limitInfo.nextRefreshTime) {
      var t = ctx.util.toIso(limitInfo.nextRefreshTime);
      if (t) progressOpts.resetsAt = t;
    }

    var lines = [ctx.line.progress(progressOpts)];

    if (isUnlimited) {
      lines.push(ctx.line.badge({ label: "Plan", text: "Unlimited", color: "#22c55e" }));
    } else {
      lines.push(ctx.line.text({ label: "Usage", value: requestsUsed + " / " + requestLimit + " credits" }));
    }

    // Aggregate bonus grants from user + workspaces
    var allGrants = [];
    if (Array.isArray(userData.bonusGrants)) allGrants = allGrants.concat(userData.bonusGrants);
    if (Array.isArray(userData.workspaces)) {
      userData.workspaces.forEach(function (ws) {
        var grants = ws && ws.bonusGrantsInfo && ws.bonusGrantsInfo.grants;
        if (Array.isArray(grants)) allGrants = allGrants.concat(grants);
      });
    }
    var bonusTotal = allGrants.reduce(function (s, g) { return s + (g.requestCreditsGranted || 0); }, 0);
    var bonusRemaining = allGrants.reduce(function (s, g) { return s + (g.requestCreditsRemaining || 0); }, 0);
    if (bonusTotal > 0 || bonusRemaining > 0) {
      lines.push(ctx.line.text({ label: "Bonus", value: bonusRemaining + " / " + bonusTotal + " add-on credits" }));
    }

    return { lines: lines };
  }

  globalThis.__openusage_plugin = { id: "warp", probe: probe };
})();
