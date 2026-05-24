(function () {
  function probe(ctx) {
    if (!ctx.host.env.get("MIMO_COOKIE") && !ctx.host.env.get("XIAOMI_MIMO_COOKIE")) {
      throw "Xiaomi MiMo session not configured. Set MIMO_COOKIE or XIAOMI_MIMO_COOKIE.";
    }
    throw "Xiaomi MiMo balance/token-plan web probe is scaffolded, but not implemented yet.";
  }

  globalThis.__openusage_plugin = { id: "mimo", probe: probe };
})();
