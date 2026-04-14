async (page) => {
  const baseUrl = 'https://dev-credbridge.bitkinetic.com';
  const email = 'test-7226@privy.io';
  const otp = '450192';

  const wait = ms => page.waitForTimeout(ms);

  await page.goto(`${baseUrl}/login`, { waitUntil: 'domcontentloaded', timeout: 60000 });
  await page.waitForLoadState('networkidle', { timeout: 60000 }).catch(() => {});

  const emailButton = page.getByRole('button', { name: /邮箱验证码登录/i }).first();
  await emailButton.waitFor({ state: 'visible', timeout: 30000 });
  await emailButton.click();

  const emailInput = page.locator('#email-input').first();
  await emailInput.waitFor({ state: 'visible', timeout: 30000 });
  await emailInput.fill(email);

  const submitButton = page.getByRole('button', { name: /^Submit$/ }).first();
  await submitButton.waitFor({ state: 'visible', timeout: 30000 });
  await submitButton.click();

  for (let i = 0; i < otp.length; i += 1) {
    const input = page.locator(`input[name="code-${i}"]`).first();
    await input.waitFor({ state: 'visible', timeout: 30000 });
    await input.fill(otp[i]);
    await wait(100);
  }

  const deadline = Date.now() + 90000;
  while (Date.now() < deadline) {
    const url = page.url();
    if (!url.includes('/login')) {
      break;
    }
    await wait(1000);
  }

  await page.waitForLoadState('networkidle', { timeout: 60000 }).catch(() => {});

  const shellCandidates = [
    page.getByRole('link', { name: /credentials/i }).first(),
    page.getByRole('link', { name: /tokens/i }).first(),
    page.locator('nav').first(),
    page.locator('[data-sidebar]').first(),
    page.getByText(/toani vault/i).first(),
  ];

  let shellVisible = false;
  for (const locator of shellCandidates) {
    if ((await locator.count()) === 0) {
      continue;
    }
    if (await locator.isVisible().catch(() => false)) {
      shellVisible = true;
      break;
    }
  }

  return {
    finalUrl: page.url(),
    title: await page.title(),
    shellVisible,
    bodyText: (await page.locator('body').innerText().catch(() => '')).slice(0, 4000),
  };
}
