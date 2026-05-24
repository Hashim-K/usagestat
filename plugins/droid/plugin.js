(function () {
  function probe(ctx) {
    if (!ctx.host.env.get("DROID_COOKIE") && !ctx.host.env.get("FACTORY_COOKIE")) {
      throw "Droid/Factory session not configured. Set DROID_COOKIE or FACTORY_COOKIE.";
    }
    throw "Droid web-session probe is scaffolded; use the Factory provider until the Droid alias parser is implemented.";
  }

  globalThis.__openusage_plugin = { id: "droid", probe: probe };
})();
