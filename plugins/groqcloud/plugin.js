(function () {
  function probe(ctx) {
    if (!ctx.host.env.get("GROQ_API_KEY") && !ctx.host.env.get("GROQCLOUD_API_KEY")) {
      throw "GroqCloud API key not found. Set GROQ_API_KEY or GROQCLOUD_API_KEY.";
    }
    throw "GroqCloud Prometheus metrics probe is scaffolded, but not implemented yet.";
  }

  globalThis.__openusage_plugin = { id: "groqcloud", probe: probe };
})();
