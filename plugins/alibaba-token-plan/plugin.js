(function () {
  function probe(ctx) {
    if (!ctx.host.env.get("ALIBABA_TOKEN_PLAN_COOKIE") && !ctx.host.env.get("ALIBABA_COOKIE")) {
      throw "Alibaba Token Plan session not configured. Set ALIBABA_TOKEN_PLAN_COOKIE.";
    }
    throw "Alibaba Token Plan usage probe is scaffolded, but the Bailian web parser is not implemented yet.";
  }

  globalThis.__openusage_plugin = { id: "alibaba-token-plan", probe: probe };
})();
