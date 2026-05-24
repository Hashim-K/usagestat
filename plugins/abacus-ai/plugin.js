(function () {
  function probe(ctx) {
    if (!ctx.host.env.get("ABACUS_COOKIE")) {
      throw "Abacus AI session not configured. Set ABACUS_COOKIE.";
    }
    throw "Abacus AI compute-credit web probe is scaffolded, but not implemented yet.";
  }

  globalThis.__openusage_plugin = { id: "abacus-ai", probe: probe };
})();
