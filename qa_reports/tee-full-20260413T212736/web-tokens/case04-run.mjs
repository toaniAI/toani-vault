import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from '../../../frontend/node_modules/playwright/index.mjs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const BASE_URL = 'https://dev-credbridge.bitkinetic.com';
const EMAIL = 'test-7226@privy.io';
const OTP = '450192';
const PREFIX = 'tee-full-case04-20260413T212736';

const paths = {
  screenshots: path.join(__dirname, 'screenshots'),
  responses: path.join(__dirname, 'responses'),
  summaries: path.join(__dirname, 'request-response-summaries.json'),
  session: path.join(__dirname, 'storage-state.json'),
  har: path.join(__dirname, 'session.har'),
  observations: path.join(__dirname, 'playwright-observations.json'),
};

await fs.mkdir(paths.screenshots, { recursive: true });
await fs.mkdir(paths.responses, { recursive: true });

const requestSummaries = [];
const observations = {
  base_url: BASE_URL,
  email: EMAIL,
  prefix: PREFIX,
  created_token_id: null,
  cleanup_supported: false,
  cleanup_status: 'not_attempted',
  constraints: {},
  blocker: null,
};

function simplifyHeaders(headers) {
  const keep = ['content-type', 'location', 'x-request-id'];
  return Object.fromEntries(
    Object.entries(headers).filter(([key]) => keep.includes(key.toLowerCase()))
  );
}

async function captureResponse(response) {
  const url = response.url();
  if (!url.includes('/api/')) return;
  const req = response.request();
  const summary = {
    url,
    method: req.method(),
    status: response.status(),
    status_text: response.statusText(),
    request_headers: simplifyHeaders(req.headers()),
    response_headers: simplifyHeaders(await response.allHeaders()),
    post_data: req.postData() ?? null,
  };
  try {
    const bodyText = await response.text();
    summary.response_body = bodyText;
    const safeName = `${requestSummaries.length.toString().padStart(2, '0')}-${req.method()}-${new URL(url).pathname.replace(/[^a-zA-Z0-9]+/g, '_')}.txt`;
    await fs.writeFile(path.join(paths.responses, safeName), bodyText);
    summary.body_path = path.join(paths.responses, safeName);
  } catch (error) {
    summary.response_body_error = String(error);
  }
  requestSummaries.push(summary);
}

const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({
  ignoreHTTPSErrors: true,
  recordHar: { path: paths.har, content: 'embed' },
  viewport: { width: 1440, height: 1100 },
});
const page = await context.newPage();
context.on('response', response => {
  captureResponse(response).catch(() => {});
});

try {
  await page.goto(`${BASE_URL}/login`, { waitUntil: 'domcontentloaded', timeout: 60000 });
  await page.waitForTimeout(4000);
  await page.screenshot({ path: path.join(paths.screenshots, '01-login.png'), fullPage: true });

  const loginButton = page.getByRole('button', { name: /email/i });
  await loginButton.click();
  await page.waitForTimeout(3000);
  await page.screenshot({ path: path.join(paths.screenshots, '02-after-login-click.png'), fullPage: true });

  const emailInput = page.locator('input[type="email"], input[name="email"]').first();
  if (await emailInput.isVisible({ timeout: 10000 }).catch(() => false)) {
    await emailInput.fill(EMAIL);
    await page.screenshot({ path: path.join(paths.screenshots, '03-email-filled.png'), fullPage: true });
    const submitButton = page.getByRole('button', { name: /submit|continue|send/i }).first();
    await submitButton.click();
    await page.waitForTimeout(5000);
    await page.screenshot({ path: path.join(paths.screenshots, '04-email-submitted.png'), fullPage: true });
  }

  const singleOtpInput = page
    .locator('input[inputmode="numeric"], input[name*="code"], input[id*="code"]')
    .first();
  const multiOtpInputs = page.locator('input[inputmode="numeric"]');
  if (await singleOtpInput.isVisible({ timeout: 15000 }).catch(() => false)) {
    const count = await multiOtpInputs.count();
    if (count >= 6) {
      for (let i = 0; i < Math.min(OTP.length, count); i += 1) {
        await multiOtpInputs.nth(i).fill(OTP[i]);
      }
    } else {
      await singleOtpInput.fill(OTP);
    }
    await page.screenshot({ path: path.join(paths.screenshots, '05-otp-filled.png'), fullPage: true });
    const verifyButton = page.getByRole('button', { name: /verify|continue|submit/i }).first();
    if (await verifyButton.isVisible().catch(() => false)) {
      await verifyButton.click();
    }
  }

  await page.waitForURL(/\/(credentials|tokens|dashboard|onboarding|tenants)/, { timeout: 60000 });
  await page.waitForTimeout(5000);
  const dialog = page.locator('[role="dialog"]').first();
  if (await dialog.isVisible({ timeout: 5000 }).catch(() => false)) {
    const dialogButtons = dialog.locator('button');
    const buttonCount = await dialogButtons.count().catch(() => 0);
    if (buttonCount > 0) {
      await dialogButtons.nth(buttonCount - 1).click().catch(() => {});
    } else {
      await page.keyboard.press('Escape').catch(() => {});
    }
    await page.waitForTimeout(1500);
  }
  await page.screenshot({ path: path.join(paths.screenshots, '06-post-login.png'), fullPage: true });

  await page.goto(`${BASE_URL}/tokens`, { waitUntil: 'domcontentloaded', timeout: 60000 });
  await page.waitForTimeout(5000);
  await page.screenshot({ path: path.join(paths.screenshots, '07-tokens-page.png'), fullPage: true });

  observations.constraints.scope_badges = await page.locator('text=credential:read').allTextContents().catch(() => []);
  observations.constraints.submit_initially_disabled = await page.getByRole('button', { name: /generate token|issue token|create token/i }).isDisabled().catch(() => null);

  const credentialCards = page.locator('button[type="button"]').filter({ has: page.locator('code') });
  const credentialCount = await credentialCards.count();
  observations.constraints.credential_card_count = credentialCount;

  if (credentialCount > 0) {
    await credentialCards.first().click();
    await page.waitForTimeout(1000);
    await page.screenshot({ path: path.join(paths.screenshots, '08-credential-selected.png'), fullPage: true });
  }

  observations.constraints.submit_after_selection_disabled = await page.getByRole('button', { name: /generate token|issue token|create token/i }).isDisabled().catch(() => null);
  observations.constraints.scope_panel_text = await page.locator('body').textContent();

  const issueButton = page.getByRole('button', { name: /generate token|issue token|create token/i });
  await issueButton.click();
  await page.waitForTimeout(5000);
  await page.screenshot({ path: path.join(paths.screenshots, '09-token-created.png'), fullPage: true });

  const tokenIdLocator = page.locator('text=Token ID').locator('..').locator('code').first();
  observations.created_token_id = (await tokenIdLocator.textContent().catch(() => null))?.trim() ?? null;
  observations.created_token_id && (observations.created_token_id = observations.created_token_id);

  const cleanupButton = page.getByRole('button', { name: /revoke|delete/i }).first();
  observations.cleanup_supported = await cleanupButton.isVisible({ timeout: 5000 }).catch(() => false);
  if (observations.cleanup_supported) {
    await cleanupButton.click();
    await page.waitForTimeout(2000);
    observations.cleanup_status = 'attempted';
    await page.screenshot({ path: path.join(paths.screenshots, '10-cleanup-attempt.png'), fullPage: true });
  } else {
    observations.cleanup_status = 'ui_not_supported';
  }

  await context.storageState({ path: paths.session });
} catch (error) {
  observations.blocker = String(error);
  await page.screenshot({ path: path.join(paths.screenshots, '99-error.png'), fullPage: true }).catch(() => {});
  throw error;
} finally {
  await fs.writeFile(paths.summaries, JSON.stringify(requestSummaries, null, 2));
  await fs.writeFile(paths.observations, JSON.stringify(observations, null, 2));
  await browser.close();
}
