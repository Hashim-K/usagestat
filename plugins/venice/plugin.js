(function () {
  var API_URL = "https://api.venice.ai/api/v1/billing/balance";

  function loadApiKey(ctx) {
    var v = ctx.host.env.get("VENICE_API_KEY");
    if (typeof v === "string" && v.trim()) return v.trim();
    return null;
  }

  function probe(ctx) {
    var apiKey = loadApiKey(ctx);
    if (!apiKey) {
      throw "Venice API key not found. Set VENICE_API_KEY.";
    }

    var result = ctx.util.requestJson({
      method: "GET",
      url: API_URL,
      headers: { Authorization: "Bearer " + apiKey, Accept: "application/json" },
      timeoutMs: 15000,
    });

    if (ctx.util.isAuthStatus(result.resp.status)) {
      throw "API key invalid or expired.";
    }
    if (result.resp.status < 200 || result.resp.status >= 300) {
      throw "Venice API error (HTTP " + result.resp.status + ").";
    }
    if (!result.json) throw "Could not parse Venice balance response.";

    var json = result.json;
    var canConsume = json.canConsume !== false;
    var currency = (json.consumptionCurrency || "").toUpperCase();
    var balances = json.balances || {};
    var diem = typeof balances.diem === "number" ? balances.diem : null;
    var usd = typeof balances.usd === "number" ? balances.usd : null;
    var epochAlloc = typeof json.diemEpochAllocation === "number" ? json.diemEpochAllocation : null;

    var lines = [];

    if (!canConsume) {
      lines.push(ctx.line.badge({ label: "Balance", text: "Balance unavailable for API calls", color: "#ef4444" }));
      return { lines: lines };
    }

    if (currency === "USD" && usd !== null && usd > 0) {
      lines.push(ctx.line.text({ label: "Balance", value: "$" + usd.toFixed(2) + " USD" }));
    } else if (currency !== "USD" && diem !== null && epochAlloc !== null && epochAlloc > 0) {
      var usedPct = Math.max(0, Math.min(100, (epochAlloc - diem) / epochAlloc * 100));
      lines.push(ctx.line.progress({
        label: "DIEM",
        used: usedPct,
        limit: 100,
        format: { kind: "percent" },
      }));
      lines.push(ctx.line.text({ label: "Allocation", value: "DIEM " + diem.toFixed(2) + " / " + epochAlloc.toFixed(2) }));
    } else if (diem !== null && diem > 0) {
      lines.push(ctx.line.text({ label: "Balance", value: "DIEM " + diem.toFixed(2) }));
    } else if (usd !== null && usd > 0) {
      lines.push(ctx.line.text({ label: "Balance", value: "$" + usd.toFixed(2) + " USD" }));
    } else {
      lines.push(ctx.line.badge({ label: "Balance", text: "No Venice API balance", color: "#a3a3a3" }));
    }

    return { lines: lines };
  }

  globalThis.__openusage_plugin = { id: "venice", probe: probe };
})();
