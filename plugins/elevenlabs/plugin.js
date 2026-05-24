(function () {
  function apiKey(ctx) {
    return ctx.host.env.get("ELEVENLABS_API_KEY") || ctx.host.env.get("XI_API_KEY");
  }

  function apiBase(ctx) {
    return ctx.host.env.get("ELEVENLABS_API_URL") || "https://api.elevenlabs.io";
  }

  function probe(ctx) {
    var key = apiKey(ctx);
    if (!key) throw "ElevenLabs API key not found. Set ELEVENLABS_API_KEY or XI_API_KEY.";

    var result = ctx.util.requestJson({
      method: "GET",
      url: apiBase(ctx).replace(/\/$/, "") + "/v1/user/subscription",
      headers: { "xi-api-key": key, Accept: "application/json" },
      timeoutMs: 15000,
    });
    if (ctx.util.isAuthStatus(result.resp.status)) throw "ElevenLabs API key invalid or expired.";
    if (result.resp.status < 200 || result.resp.status >= 300) throw "ElevenLabs API error (HTTP " + result.resp.status + ").";
    if (!result.json) throw "Could not parse ElevenLabs subscription response.";

    var sub = result.json;
    var used = Number(sub.character_count || 0);
    var limit = Number(sub.character_limit || 0);
    var lines = [];
    var opts = {
      label: "Characters",
      used: used,
      limit: limit > 0 ? limit : Math.max(used, 1),
      format: { kind: "count", suffix: "chars" },
    };
    if (sub.next_character_count_reset_unix) opts.resetsAt = ctx.util.toIso(Number(sub.next_character_count_reset_unix) * 1000);
    lines.push(ctx.line.progress(opts));

    var voiceUsed = Number(sub.voice_count || 0);
    var voiceLimit = Number(sub.voice_limit || sub.professional_voice_limit || 0);
    if (voiceLimit > 0 || voiceUsed > 0) {
      lines.push(ctx.line.text({ label: "Voice slots", value: voiceUsed + " / " + voiceLimit }));
    }
    if (sub.tier) lines.push(ctx.line.badge({ label: "Plan", text: String(sub.tier), color: "#22c55e" }));

    return { lines: lines };
  }

  globalThis.__openusage_plugin = { id: "elevenlabs", probe: probe };
})();
