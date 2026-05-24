(function () {
  function probe(ctx) {
    if (!ctx.host.env.get("COMMAND_CODE_COOKIE") && !ctx.host.env.get("COMMANDCODE_COOKIE")) {
      throw "Command Code session not configured. Set COMMAND_CODE_COOKIE.";
    }
    throw "Command Code billing API probe is scaffolded, but not implemented yet.";
  }

  globalThis.__openusage_plugin = { id: "command-code", probe: probe };
})();
