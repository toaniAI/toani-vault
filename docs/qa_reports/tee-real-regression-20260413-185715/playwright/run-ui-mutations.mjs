import fs from 'node:fs';
import path from 'node:path';
import { chromium } from '/Users/yvan/AIWorkspace/credbridge/frontend/node_modules/playwright/index.mjs';

const BASE = 'https://dev-credbridge.bitkinetic.com';
const EMAIL = 'test-7226@privy.io';
const OTP = '450192';
const OUT_DIR =
  '/Users/yvan/AIWorkspace/credbridge/docs/qa_reports/tee-real-regression-20260413-185715/playwright';
const SERVICE_ID = 'ui-tee-real-20260413-185715';

const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({
  baseURL: BASE,
  viewport: { width: 1440, height: 1100 },
});
const page = await context.newPage();

const output = {
  serviceId: SERVICE_ID,
  createdCredential: null,
  createdToken: null,
};

page.on('response', async response => {
  try {
    const url = response.url();
    const method = response.request().method();
    if (url.includes('/api/v1/credentials') && method === 'POST') {
      output.createdCredential = {
        status: response.status(),
        body: await response.json(),
      };
    }
    if (url.includes('/api/v1/tokens') && method === 'POST') {
      output.createdToken = {
        status: response.status(),
        body: await response.json(),
      };
    }
  } catch {
    // Keep evidence collection best-effort.
  }
});

async function maybeClick(regexes) {
  for (const regex of regexes) {
    const button = page.getByRole('button', { name: regex }).first();
    if ((await button.count()) === 0) continue;
    try {
      await button.click({ timeout: 5_000 });
      return true;
    } catch {
      // Try next candidate.
    }
  }
  return false;
}

async function login() {
  await page.goto('/login', { waitUntil: 'networkidle', timeout: 60_000 });
  await page.getByRole('button', { name: /sign in with email code/i }).click();
  await page.locator('input[type="email"]').fill(EMAIL);
  await page.getByRole('button', { name: /submit/i }).click();
  const split = page.locator("input[name^='code-']");
  await split.first().waitFor({ state: 'visible', timeout: 60_000 });
  for (const [index, digit] of [...OTP].entries()) {
    await split.nth(index).fill(digit);
  }
  await page.waitForURL('**/credentials', { timeout: 90_000 });
}

try {
  await login();

  await page.goto('/credentials', { waitUntil: 'networkidle' });
  await page.getByRole('button', { name: /new credential/i }).click();
  const modalInputs = page.locator('input');
  await modalInputs.nth(0).fill(SERVICE_ID);
  await modalInputs.nth(1).fill('ui-qa-user');
  await modalInputs.nth(2).fill('UiQaPass!2026');
  await page.getByRole('button', { name: /安全创建|create|submit/i }).click();
  await page.waitForTimeout(3_000);
  await page.screenshot({
    path: path.join(OUT_DIR, 'credentials-after-ui-create.png'),
    fullPage: true,
  });

  await page.goto('/tokens', { waitUntil: 'networkidle' });
  await page.getByRole('button', { name: new RegExp(SERVICE_ID, 'i') }).click();
  await maybeClick([/generate token/i, /生成 token/i]);
  await page.waitForTimeout(3_000);
  await page.screenshot({
    path: path.join(OUT_DIR, 'tokens-after-ui-create.png'),
    fullPage: true,
  });

  fs.writeFileSync(path.join(OUT_DIR, 'ui-mutations.json'), JSON.stringify(output, null, 2));
  console.log(JSON.stringify(output, null, 2));
} finally {
  await context.close();
  await browser.close();
}
