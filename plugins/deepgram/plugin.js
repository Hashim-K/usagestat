(function () {
  function probe(ctx) {
    var key = ctx.host.env.get("DEEPGRAM_API_KEY");
    if (!key) throw "Deepgram API key not found. Set DEEPGRAM_API_KEY.";

    var result = ctx.util.requestJson({
      method: "GET",
      url: "https://api.deepgram.com/v1/projects",
      headers: { Authorization: "Token " + key, Accept: "application/json" },
      timeoutMs: 15000,
    });
    if (ctx.util.isAuthStatus(result.resp.status)) throw "Deepgram API key invalid or expired.";
    if (result.resp.status < 200 || result.resp.status >= 300) throw "Deepgram API error (HTTP " + result.resp.status + ").";
    var projects = result.json && result.json.projects;
    if (!Array.isArray(projects)) throw "Could not parse Deepgram projects response.";
    return { lines: [ctx.line.text({ label: "Projects", value: String(projects.length) })] };
  }

  globalThis.__openusage_plugin = { id: "deepgram", probe: probe };
})();
