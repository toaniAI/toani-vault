import fs from 'node:fs';
import path from 'node:path';
import { chromium } from '/Users/yvan/AIWorkspace/credbridge/frontend/node_modules/playwright/index.mjs';

const BASE = process.env.TOANI_TEST_BASE_URL ?? 'https://dev-credbridge.bitkinetic.com';
const EMAIL = process.env.TOANI_TEST_EMAIL ?? 'test-7226@privy.io';
const OTP = process.env.TOANI_TEST_OTP ?? '450192';
const SERVICE_ID =
  process.env.TOANI_TEST_SERVICE_ID ??
  `cli-sandbox-smoke-${new Date().toISOString().replace(/[-:TZ.]/g, '').slice(0, 12)}`;
const OUT_DIR =
  process.env.TOANI_TEST_OUT_DIR ?? '/Users/yvan/AIWorkspace/credbridge/output/playwright';

fs.mkdirSync(OUT_DIR, { recursive: true });

const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({
  baseURL: BASE,
  viewport: { width: 1440, height: 1100 },
});
const page = await context.newPage();

const output = {
  baseUrl: BASE,
  email: EMAIL,
  serviceId: SERVICE_ID,
  createdCredential: null,
  createdToken: null,
};

page.on('response', async (response) => {
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
    // Best-effort evidence capture only.
  }
});

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

async function createCredential() {
  await page.goto('/credentials', { waitUntil: 'networkidle' });
  await page.getByRole('button', { name: /new credential/i }).click();
  const modalInputs = page.locator('input');
  await modalInputs.nth(0).fill(SERVICE_ID);
  await modalInputs.nth(1).fill('ui-qa-user');
  await modalInputs.nth(2).fill('UiQaPass!2026');
  await page.getByRole('button', { name: /安全创建|create|submit/i }).click();
  await page.waitForTimeout(3_000);
  await page.screenshot({
    path: path.join(OUT_DIR, 'toani-credentials-after-create.png'),
    fullPage: true,
  });
}

async function createToken() {
  await page.goto('/tokens', { waitUntil: 'networkidle' });
  await page.locator('button').filter({ hasText: SERVICE_ID }).first().click();
  await page.getByRole('button', { name: /generate token|生成 token/i }).click();
  await page.waitForTimeout(3_000);
  await page.screenshot({
    path: path.join(OUT_DIR, 'toani-tokens-after-create.png'),
    fullPage: true,
  });
}

try {
  await login();
  await createCredential();
  await createToken();

  const outputPath = path.join(OUT_DIR, 'toani-ui-setup.json');
  fs.writeFileSync(outputPath, JSON.stringify(output, null, 2));
  console.log(JSON.stringify({ outputPath, ...output }, null, 2));
} finally {
  await context.close();
  await browser.close();
}
