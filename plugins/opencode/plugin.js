(function () {
  function probe(ctx) {
    if (!ctx.host.env.get("OPENCODE_COOKIE")) {
      throw "OpenCode session not configured. Set OPENCODE_COOKIE.";
    }
    throw "OpenCode web-dashboard probe is scaffolded. OpenCode Go remains available as a separate local provider.";
  }

  globalThis.__openusage_plugin = { id: "opencode", probe: probe };
})();
