(function () {
  function probe(ctx) {
    if (!ctx.host.env.get("AWS_ACCESS_KEY_ID") || !ctx.host.env.get("AWS_SECRET_ACCESS_KEY")) {
      throw "AWS credentials not configured. Set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY.";
    }
    throw "AWS Bedrock Cost Explorer probe is scaffolded, but AWS SigV4 requests are not implemented yet.";
  }

  globalThis.__openusage_plugin = { id: "aws-bedrock", probe: probe };
})();
