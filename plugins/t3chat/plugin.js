(function () {
  function probe(ctx) {
    if (!ctx.host.env.get("T3CHAT_COOKIE") && !ctx.host.env.get("T3_CHAT_COOKIE")) {
      throw "T3 Chat session not configured. Set T3CHAT_COOKIE or T3_CHAT_COOKIE.";
    }
    throw "T3 Chat web-session usage probe is scaffolded, but the tRPC parser is not implemented yet.";
  }

  globalThis.__openusage_plugin = { id: "t3chat", probe: probe };
})();
