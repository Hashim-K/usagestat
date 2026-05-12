(function () {
  var TRPC_BASE = "https://app.kilo.ai/api/trpc";
  var PROCEDURES = "user.getCreditBlocks,kiloPass.getState,user.getAutoTopUpPaymentMethod";
  var AUTH_PATH = "~/.local/share/kilo/auth.json";

  function loadApiKey(ctx) {
    var v = ctx.host.env.get("KILO_API_KEY");
    if (typeof v === "string" && v.trim()) return v.trim();

    try {
      if (ctx.host.fs.exists(AUTH_PATH)) {
        var json = ctx.util.tryParseJson(ctx.host.fs.readText(AUTH_PATH));
        if (json) {
          var keys = ["apiKey", "api_key", "token"];
          for (var i = 0; i < keys.length; i++) {
            if (typeof json[keys[i]] === "string" && json[keys[i]].trim()) return json[keys[i]].trim();
          }
        }
      }
    } catch (e) {
      ctx.host.log.warn("kilo: failed to read auth.json: " + e);
    }
    return null;
  }

  function buildUrl() {
    var input = JSON.stringify({
      "0": { json: null, meta: { values: ["undefined"] } },
      "1": { json: null, meta: { values: ["undefined"] } },
      "2": { json: null, meta: { values: ["undefined"] } },
    });
    return TRPC_BASE + "/" + PROCEDURES + "?batch=1&input=" + encodeURIComponent(input);
  }

  function extractData(batch, index) {
    if (!Array.isArray(batch)) return null;
    var item = batch[index];
    return item && item.result && item.result.data && item.result.data.json;
  }

  function probe(ctx) {
    var apiKey = loadApiKey(ctx);
    if (!apiKey) {
      throw "Kilo API key not found. Set KILO_API_KEY or sign in with Kilo CLI.";
    }

    var result = ctx.util.requestJson({
      method: "GET",
      url: buildUrl(),
      headers: { Authorization: "Bearer " + apiKey, Accept: "application/json" },
      timeoutMs: 30000,
    });

    if (ctx.util.isAuthStatus(result.resp.status)) {
      throw "API key invalid or expired.";
    }
    if (result.resp.status < 200 || result.resp.status >= 300) {
      throw "Kilo API error (HTTP " + result.resp.status + ").";
    }
    if (!result.json) throw "Could not parse Kilo response.";

    var creditBlocks = extractData(result.json, 0);
    var kiloPass = extractData(result.json, 1);

    var totalMusd = 0, remainingMusd = 0;
    if (Array.isArray(creditBlocks)) {
      creditBlocks.forEach(function (block) {
        totalMusd += (block.amount_mUsd || 0);
        remainingMusd += (block.balance_mUsd || 0);
      });
    }

    var totalUsd = totalMusd / 1e6;
    var remainingUsd = remainingMusd / 1e6;
    var usedUsd = Math.max(0, totalUsd - remainingUsd);
    var usedPct = totalUsd > 0 ? Math.min(100, (usedUsd / totalUsd) * 100) : 0;

    var lines = [
      ctx.line.progress({
        label: "Credits",
        used: usedPct,
        limit: 100,
        format: { kind: "percent" },
      }),
      ctx.line.text({ label: "Balance", value: "$" + usedUsd.toFixed(2) + " / $" + totalUsd.toFixed(2) }),
    ];

    var plan = null;
    if (kiloPass) {
      var passUsage = kiloPass.currentPeriodUsageUsd || 0;
      var passBase = kiloPass.currentPeriodBaseCreditsUsd || 0;
      var passBonus = kiloPass.currentPeriodBonusCreditsUsd || 0;
      var passTotal = passBase + passBonus;
      if (passTotal > 0) {
        var passPct = Math.min(100, (passUsage / passTotal) * 100);
        lines.push(ctx.line.progress({
          label: "Pass",
          used: passPct,
          limit: 100,
          format: { kind: "percent" },
        }));
        lines.push(ctx.line.text({ label: "Pass Balance", value: "$" + passUsage.toFixed(2) + " / $" + passTotal.toFixed(2) }));
      }
      plan = kiloPass.planName || kiloPass.tier || kiloPass.status || null;
    }

    return { plan: plan, lines: lines };
  }

  globalThis.__openusage_plugin = { id: "kilo", probe: probe };
})();
