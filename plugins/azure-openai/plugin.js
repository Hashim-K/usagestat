(function () {
  function probe(ctx) {
    if (!ctx.host.env.get("AZURE_OPENAI_API_KEY")) {
      throw "Azure OpenAI API key not configured. Set AZURE_OPENAI_API_KEY.";
    }
    if (!ctx.host.env.get("AZURE_OPENAI_ENDPOINT")) {
      throw "Azure OpenAI endpoint not configured. Set AZURE_OPENAI_ENDPOINT.";
    }
    if (!ctx.host.env.get("AZURE_OPENAI_DEPLOYMENT")) {
      throw "Azure OpenAI deployment not configured. Set AZURE_OPENAI_DEPLOYMENT.";
    }
    throw "Azure OpenAI deployment validation is scaffolded, but the API probe is not implemented yet.";
  }

  globalThis.__openusage_plugin = { id: "azure-openai", probe: probe };
})();
