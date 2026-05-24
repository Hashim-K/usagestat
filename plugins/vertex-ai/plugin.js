(function () {
  function probe(ctx) {
    if (!ctx.host.env.get("GOOGLE_APPLICATION_CREDENTIALS") && !ctx.host.env.get("CLOUDSDK_CONFIG")) {
      throw "Vertex AI credentials not configured. Run gcloud auth application-default login or set GOOGLE_APPLICATION_CREDENTIALS.";
    }
    throw "Vertex AI Cloud Monitoring quota probe is scaffolded, but Google ADC/OAuth probing is not implemented yet.";
  }

  globalThis.__openusage_plugin = { id: "vertex-ai", probe: probe };
})();
