globalThis.__usagestat_plugin = {
  probe(ctx) {
    return {
      displayName: "Mock Provider",
      source: "mock",
      plan: "Dev",
      metrics: [
        {
          type: "progress",
          label: "Session",
          used: 42,
          limit: 100,
          format: { kind: "percent" },
          resetsAt: ctx.nowIso
        },
        {
          type: "text",
          label: "Updated",
          value: ctx.nowIso
        }
      ]
    };
  }
};
