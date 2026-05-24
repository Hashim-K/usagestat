(function () {
  var URL = "https://api.manus.im/user.v1.UserService/GetAvailableCredits";

  function cookie(ctx) {
    var raw = ctx.host.env.get("MANUS_COOKIE") || ctx.host.env.get("MANUS_SESSION_TOKEN");
    if (!raw) return null;
    raw = String(raw).trim();
    return raw.indexOf("=") >= 0 ? raw : "session_id=" + raw;
  }

  function pickNumber(obj, names) {
    for (var i = 0; i < names.length; i++) {
      var v = obj && obj[names[i]];
      if (typeof v === "number") return v;
      if (typeof v === "string" && v.trim() !== "" && !isNaN(Number(v))) return Number(v);
    }
    return null;
  }

  function probe(ctx) {
    var c = cookie(ctx);
    if (!c) throw "Manus session not configured. Set MANUS_SESSION_TOKEN or MANUS_COOKIE.";

    var result = ctx.util.requestJson({
      method: "POST",
      url: URL,
      headers: { Cookie: c, Accept: "application/json", "Content-Type": "application/json" },
      bodyText: "{}",
      timeoutMs: 15000,
    });
    if (ctx.util.isAuthStatus(result.resp.status)) throw "Manus session expired or invalid.";
    if (result.resp.status < 200 || result.resp.status >= 300) throw "Manus API error (HTTP " + result.resp.status + ").";

    var json = result.json || {};
    var data = json.data || json.result || json;
    var available = pickNumber(data, ["availableCredits", "available_credits", "credits", "balance"]);
    var total = pickNumber(data, ["totalCredits", "total_credits", "limit"]);
    if (available === null) throw "Manus response did not include a recognizable credit balance.";

    var lines = [];
    if (total !== null && total > 0) {
      lines.push(ctx.line.progress({ label: "Credits", used: Math.max(0, total - available), limit: total, format: { kind: "count", suffix: "credits" } }));
    } else {
      lines.push(ctx.line.text({ label: "Available", value: String(available) + " credits" }));
    }
    return { lines: lines };
  }

  globalThis.__openusage_plugin = { id: "manus", probe: probe };
})();
