(function () {
  function probe(ctx) {
    var authPath = ctx.host.fs.homeDir + "/.grok/auth.json";
    if (!ctx.host.fs.exists(authPath) && !ctx.host.env.get("GROK_COOKIE")) {
      throw "Grok auth not found. Run grok login or set GROK_COOKIE.";
    }
    throw "Grok billing probe is scaffolded, but grok CLI JSON-RPC/browser billing probing is not implemented yet.";
  }

  globalThis.__openusage_plugin = { id: "grok", probe: probe };
})();
