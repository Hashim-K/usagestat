globalThis.__usagestat_plugin = {
  probe(ctx) {
    ctx.host.log.info("running host API smoke probe");

    const home = ctx.host.fs.homeDir || "";
    const hasHome = home ? ctx.host.fs.exists(home) : false;
    const pluginDir = ctx.host.env.get("USAGESTAT_PLUGIN_DIR") || "";

    return {
      displayName: "Host API Smoke",
      source: "host-api",
      metrics: [
        {
          type: "badge",
          label: "Home",
          text: hasHome ? "available" : "missing"
        },
        {
          type: "text",
          label: "Plugin Dir Env",
          value: pluginDir ? "set" : "unset"
        }
      ]
    };
  }
};
