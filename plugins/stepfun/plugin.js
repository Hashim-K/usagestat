(function () {
  function probe(ctx) {
    if (!ctx.host.env.get("STEPFUN_OASIS_TOKEN") && !ctx.host.env.get("STEPFUN_COOKIE")) {
      throw "StepFun session not configured. Set STEPFUN_OASIS_TOKEN or STEPFUN_COOKIE.";
    }
    throw "StepFun Step Plan probe is scaffolded, but not implemented yet.";
  }

  globalThis.__openusage_plugin = { id: "stepfun", probe: probe };
})();
