globalThis.__usagestat_plugin = {
  probe(ctx) {
    const settings = ctx.provider.settings;
    if (ctx.app.appDataDir !== settings.dataDir) throw new Error('Wrong data directory');
    const rows = JSON.parse(ctx.host.sqlite.query(settings.database, 'SELECT value FROM smoke'));
    const response = ctx.host.http.request({ url: settings.url, timeoutMs: 5000 });
    const received = JSON.parse(response.bodyText);
    if (response.status !== 200 || received.nonce !== settings.nonce) {
      throw new Error('HTTP fixture mismatch');
    }
    if (settings.httpsUrl) {
      const tls = ctx.host.http.request({ url: settings.httpsUrl, timeoutMs: 15000 });
      if (tls.status < 200 || tls.status >= 400) throw new Error('HTTPS check failed');
    }
    const marker = ctx.app.pluginDataDir + '/runtime.txt';
    ctx.host.fs.writeText(marker, settings.nonce);
    if (ctx.host.fs.readText(marker) !== settings.nonce) throw new Error('File round trip failed');
    return {
      displayName: 'Native runtime smoke test',
      source: 'local',
      metrics: [
        { type: 'progress', label: 'Fixture', used: rows[0].value + received.value, limit: 100,
          format: { kind: 'percent' } },
        { type: 'text', label: 'Platform', value: ctx.app.platform },
        { type: 'text', label: 'Nonce', value: settings.nonce },
      ],
    };
  },
};
