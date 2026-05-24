(function () {
  function probe(ctx) {
    var hasCookie = !!(ctx.host.env.get("ALIBABA_COOKIE") || ctx.host.env.get("ALIBABA_CODING_PLAN_COOKIE"));
    var hasApiKey = !!ctx.host.env.get("ALIBABA_CODING_PLAN_API_KEY");
    if (!hasCookie && !hasApiKey) {
      throw "Alibaba session not configured. Set ALIBABA_COOKIE or ALIBABA_CODING_PLAN_API_KEY.";
    }
    throw "Alibaba Coding Plan probe is scaffolded, but the console RPC parser is not implemented yet.";
  }

  globalThis.__openusage_plugin = { id: "alibaba", probe: probe };
})();
