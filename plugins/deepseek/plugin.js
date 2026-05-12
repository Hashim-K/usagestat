(function () {
  var API_URL = "https://api.deepseek.com/user/balance";

  function loadApiKey(ctx) {
    var names = ["DEEPSEEK_API_KEY", "DEEPSEEK_KEY"];
    for (var i = 0; i < names.length; i++) {
      var v = ctx.host.env.get(names[i]);
      if (typeof v === "string" && v.trim()) return v.trim();
    }
    return null;
  }

  function probe(ctx) {
    var apiKey = loadApiKey(ctx);
    if (!apiKey) {
      throw "DeepSeek API key not found. Set DEEPSEEK_API_KEY or DEEPSEEK_KEY.";
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
      throw "DeepSeek API error (HTTP " + result.resp.status + ").";
    }
    if (!result.json) throw "Could not parse DeepSeek balance response.";

    var infos = result.json.balance_infos || [];
    var info = null;
    for (var i = 0; i < infos.length; i++) {
      if ((infos[i].currency || "").toUpperCase() === "USD") { info = infos[i]; break; }
    }
    if (!info && infos.length > 0) info = infos[0];

    var lines = [];
    if (!info) {
      lines.push(ctx.line.badge({ label: "Balance", text: "No balance data", color: "#a3a3a3" }));
      return { lines: lines };
    }

    var total = parseFloat(info.total_balance) || 0;
    var granted = parseFloat(info.granted_balance) || 0;
    var toppedUp = parseFloat(info.topped_up_balance) || 0;
    var symbol = (info.currency || "").toUpperCase() === "CNY" ? "¥" : "$";
    var available = result.json.is_available !== false;

    var detail;
    if (!available) {
      detail = "Balance unavailable for API calls";
    } else if (total <= 0) {
      detail = symbol + "0.00 — add credits at platform.deepseek.com";
    } else {
      detail = symbol + total.toFixed(2) + " (Paid: " + symbol + toppedUp.toFixed(2) + " / Granted: " + symbol + granted.toFixed(2) + ")";
    }

    lines.push(ctx.line.text({ label: "Balance", value: detail }));
    if (!available) {
      lines.push(ctx.line.badge({ label: "Status", text: "Unavailable", color: "#ef4444" }));
    }

    return { lines: lines };
  }

  globalThis.__openusage_plugin = { id: "deepseek", probe: probe };
})();
