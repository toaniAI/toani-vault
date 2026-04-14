async (page) => {
  const wait = ms => page.waitForTimeout(ms);
  const baseUrl = 'https://dev-credbridge.bitkinetic.com';
  const prefix = 'tee-full-20260413T232909-webtok';
  const networkEvents = [];

  page.on('response', async (response) => {
    const url = response.url();
    if (!url.includes('/token') && !url.includes('/cred')) return;
    let body = null;
    try {
      body = await response.text();
    } catch {
      body = null;
    }
    networkEvents.push({
      url,
      status: response.status(),
      method: response.request().method(),
      body: body ? body.slice(0, 4000) : null,
    });
  });

  await page.goto(`${baseUrl}/tokens`, { waitUntil: 'domcontentloaded', timeout: 60000 });
  await page.waitForLoadState('networkidle', { timeout: 60000 }).catch(() => {});

  const credentialButtons = page.locator('main button').filter({ has: page.locator('code') });
  const credentialCount = await credentialButtons.count();

  const credentialChoices = [];
  for (let i = 0; i < credentialCount; i += 1) {
    const button = credentialButtons.nth(i);
    credentialChoices.push((await button.innerText()).trim().replace(/\s+/g, ' '));
  }

  let selectedCredential = null;
  if (credentialCount > 0) {
    const button = credentialButtons.first();
    selectedCredential = (await button.innerText()).trim().replace(/\s+/g, ' ');
    await button.click();
    await wait(300);
  }

  const generateButton = page.getByRole('button', { name: /生成 Token/i }).first();
  const generateInitiallyEnabled = await generateButton.isEnabled().catch(() => false);
  if (generateInitiallyEnabled) {
    await generateButton.click();
  }

  const issuedTokenLocator = page.locator('code, pre').filter({ hasText: /pt_/i }).first();
  const tokenTextVisible = await issuedTokenLocator.isVisible({ timeout: 20000 }).catch(() => false);

  let issuedToken = null;
  if (tokenTextVisible) {
    issuedToken = (await issuedTokenLocator.innerText()).trim();
  }

  let redactedToken = null;
  if (issuedToken) {
    redactedToken = issuedToken.length <= 12
      ? issuedToken
      : `${issuedToken.slice(0, 8)}...${issuedToken.slice(-4)}`;
  }

  const revokeButtons = await page.getByRole('button', { name: /撤销|吊销|删除/i }).evaluateAll(
    els => els.map(el => el.textContent?.trim()).filter(Boolean),
  ).catch(() => []);

  const curlExamples = await page.locator('pre, code').evaluateAll((els) =>
    els
      .map((el) => (el.textContent || '').trim())
      .filter((text) => /curl|toani|https?:\/\//i.test(text))
      .slice(0, 10),
  ).catch(() => []);

  const bodyText = (await page.locator('body').innerText().catch(() => '')).slice(0, 8000);

  let createdId = null;
  let createdLabel = null;
  for (const event of networkEvents) {
    if (!event.body) continue;
    try {
      const parsed = JSON.parse(event.body);
      createdId = parsed.id || parsed.token_id || parsed.tokenId || createdId;
      createdLabel = parsed.label || parsed.name || createdLabel;
      if (createdId) break;
    } catch {
      const idMatch = event.body.match(/"id"\s*:\s*"([^"]+)"/) || event.body.match(/"token_id"\s*:\s*"([^"]+)"/);
      if (idMatch) {
        createdId = idMatch[1];
        break;
      }
    }
  }

  return {
    selectedCredential,
    credentialChoices,
    generateInitiallyEnabled,
    createdId,
    createdLabel: createdLabel || `${prefix}-${Date.now()}`,
    issuedTokenRedacted: redactedToken,
    tokenTextVisible,
    revokeButtons,
    curlExamples,
    bodyText,
    networkEvents,
    currentUrl: page.url(),
    title: await page.title(),
  };
}
