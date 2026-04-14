async (page) => {
  const routes = ['/', '/credentials', '/tokens', '/developer', '/profile', '/users', '/audit', '/tenants', '/settings'];
  const results = {};

  for (const route of routes) {
    await page.goto(`https://dev-credbridge.bitkinetic.com${route}`, {
      waitUntil: 'domcontentloaded',
      timeout: 60000,
    });
    await page.waitForLoadState('networkidle', { timeout: 60000 }).catch(() => {});
    results[route] = {
      finalUrl: page.url(),
      title: await page.title(),
      bodyText: (await page.locator('body').innerText().catch(() => '')).slice(0, 2000),
    };
  }

  const localStorage = await page.evaluate(() => {
    const out = {};
    for (let i = 0; i < window.localStorage.length; i += 1) {
      const key = window.localStorage.key(i);
      if (!key) continue;
      out[key] = window.localStorage.getItem(key);
    }
    return out;
  });

  const sessionStorage = await page.evaluate(() => {
    const out = {};
    for (let i = 0; i < window.sessionStorage.length; i += 1) {
      const key = window.sessionStorage.key(i);
      if (!key) continue;
      out[key] = window.sessionStorage.getItem(key);
    }
    return out;
  });

  return {
    currentUrl: page.url(),
    currentTitle: await page.title(),
    routes: results,
    localStorage,
    sessionStorage,
  };
}
