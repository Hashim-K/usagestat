(function () {
  function probe(ctx) {
    throw "Droid reuses the Factory provider implementation. Check plugin.json entry configuration.";
  }

  globalThis.__openusage_plugin = { id: "droid", probe: probe };
})();
