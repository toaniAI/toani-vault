import fs from 'node:fs/promises';
import path from 'node:path';
import { chromium } from '/Users/yvan/AIWorkspace/credbridge/frontend/node_modules/playwright/index.mjs';

const BASE = process.env.TEST_BASE || 'https://dev-credbridge.bitkinetic.com';
const EMAIL = process.env.TEST_EMAIL || 'test-7226@privy.io';
const OTP = process.env.TEST_OTP || '450192';
const OUT_DIR =
  process.env.OUT_DIR ||
  '/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-developer-center';

const artifacts = {
  login: path.join(OUT_DIR, '01-login.png'),
  developerOverview: path.join(OUT_DIR, '02-developer-overview.png'),
  apiDocs: path.join(OUT_DIR, '03-api-docs.png'),
  sdkExamples: path.join(OUT_DIR, '04-sdk-examples.png'),
  apiText: path.join(OUT_DIR, 'api-docs.txt'),
  sdkText: path.join(OUT_DIR, 'sdk-examples.txt'),
  browserJson: path.join(OUT_DIR, 'browser-findings.json'),
};

let sessionResponse = null;

async function ensureDir(dir) {
  await fs.mkdir(dir, { recursive: true });
}

async function saveJson(file, value) {
  await fs.writeFile(file, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
}

function extractSnippets(text, patterns) {
  const lines = text
    .split('\n')
    .map(line => line.trim())
    .filter(Boolean);
  return patterns.flatMap(pattern => {
    const needle = pattern.toLowerCase();
    return lines
      .filter(line => line.toLowerCase().includes(needle))
      .slice(0, 4)
      .map(line => ({ pattern, line }));
  });
}

async function clickFirstVisible(page, candidates) {
  for (const candidate of candidates) {
    const locator =
      candidate.kind === 'role'
        ? page.getByRole(candidate.role, { name: candidate.name }).first()
        : page.getByText(candidate.name).first();
    if ((await locator.count()) === 0) {
      continue;
    }
    try {
      await locator.waitFor({ state: 'visible', timeout: 5000 });
      await locator.click({ timeout: 5000 });
      return true;
    } catch {}
  }
  return false;
}

async function fillEmailOtpLogin(page) {
  await page.goto(`${BASE}/login`, { waitUntil: 'domcontentloaded', timeout: 60000 });
  await page.waitForTimeout(2000);

  await clickFirstVisible(page, [
    { kind: 'role', role: 'button', name: /sign in with email code/i },
    { kind: 'role', role: 'button', name: /continue with email/i },
    { kind: 'text', name: /sign in with email code/i },
    { kind: 'text', name: /continue with email/i },
    { kind: 'text', name: /email/i },
  ]);
  await page.waitForTimeout(2500);

  const emailSelectors = [
    'input[type="email"]',
    'input[name*="email" i]',
    'input[placeholder*="mail" i]',
  ];

  let emailInput = null;
  for (const selector of emailSelectors) {
    const locator = page.locator(selector).first();
    try {
      await locator.waitFor({ state: 'visible', timeout: 15000 });
      emailInput = locator;
      break;
    } catch {}
  }
  if (!emailInput) {
    throw new Error('email input not found');
  }

  await emailInput.fill(EMAIL);

  const sendClicked = await clickFirstVisible(page, [
    { kind: 'role', role: 'button', name: /continue/i },
    { kind: 'role', role: 'button', name: /send/i },
    { kind: 'role', role: 'button', name: /code/i },
    { kind: 'role', role: 'button', name: /email/i },
    { kind: 'role', role: 'button', name: /登录|继续|发送/i },
  ]);
  if (!sendClicked) {
    await page.keyboard.press('Enter');
  }

  let otpInput = null;
  try {
    otpInput = page.locator('input[name="code-0"]').first();
    await otpInput.waitFor({ state: 'visible', timeout: 60000 });
  } catch {
    const otpSelectors = [
      'input[name^="code-"]',
      'input[inputmode="numeric"]',
      'input[autocomplete="one-time-code"]',
      'input[name*="code" i]',
      'input[type="tel"]',
      'input[maxlength="1"]',
    ];
    for (const selector of otpSelectors) {
      const locator = page.locator(selector).first();
      try {
        await locator.waitFor({ state: 'visible', timeout: 5000 });
        otpInput = locator;
        break;
      } catch {}
    }
  }

  if (!otpInput) {
    throw new Error('otp input not found');
  }

  const splitInputs = page.locator('input[name^="code-"]');
  const splitCount = await splitInputs.count();
  if (splitCount >= OTP.length) {
    for (let index = 0; index < OTP.length; index += 1) {
      await splitInputs.nth(index).fill(OTP[index], { timeout: 5000 });
    }
  } else {
    await otpInput.click({ timeout: 5000 });
    await page.keyboard.type(OTP, { delay: 80 });
  }

  await page.screenshot({ path: artifacts.login, fullPage: true });

  await clickFirstVisible(page, [
    { kind: 'role', role: 'button', name: /verify/i },
    { kind: 'role', role: 'button', name: /sign in/i },
    { kind: 'role', role: 'button', name: /continue/i },
    { kind: 'role', role: 'button', name: /登录|验证|继续/i },
  ]);

  await page.waitForFunction(() => !window.location.pathname.includes('/login'), {
    timeout: 90000,
  });
}

async function openDeveloperTab(page, tabName) {
  const clicked = await clickFirstVisible(page, [
    { kind: 'role', role: 'tab', name: new RegExp(tabName, 'i') },
    { kind: 'role', role: 'button', name: new RegExp(tabName, 'i') },
    { kind: 'text', name: new RegExp(tabName, 'i') },
  ]);
  if (!clicked) {
    throw new Error(`unable to open tab: ${tabName}`);
  }
  await page.waitForTimeout(1200);
}

async function main() {
  await ensureDir(OUT_DIR);

  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1440, height: 1200 } });
  const page = await context.newPage();

  page.on('response', async response => {
    if (!response.url().includes('/api/v1/auth/session')) {
      return;
    }
    if (response.request().method() !== 'POST') {
      return;
    }
    try {
      const body = await response.text();
      sessionResponse = {
        url: response.url(),
        status: response.status(),
        bodyPreview: body.slice(0, 500),
      };
    } catch {}
  });

  try {
    await page.goto(`${BASE}/developer`, { waitUntil: 'domcontentloaded', timeout: 60000 });
    await page.waitForTimeout(2500);

    if (page.url().includes('/login') || (await page.locator('input[type="email"]').count()) > 0) {
      await fillEmailOtpLogin(page);
      await page.goto(`${BASE}/developer`, { waitUntil: 'domcontentloaded', timeout: 60000 });
      await page.waitForTimeout(2500);
    }

    await page.getByText(/Developer Center/i).first().waitFor({ state: 'visible', timeout: 30000 });
    await page.screenshot({ path: artifacts.developerOverview, fullPage: true });

    await openDeveloperTab(page, 'API Docs');
    const apiText = await page.locator('main').innerText();
    await fs.writeFile(artifacts.apiText, `${apiText}\n`, 'utf8');
    await page.screenshot({ path: artifacts.apiDocs, fullPage: true });

    await openDeveloperTab(page, 'SDK Examples');
    const sdkText = await page.locator('main').innerText();
    await fs.writeFile(artifacts.sdkText, `${sdkText}\n`, 'utf8');
    await page.screenshot({ path: artifacts.sdkExamples, fullPage: true });

    const keywordPatterns = [
      'automation token',
      'dashboard token',
      'manual token',
      'decrypt',
      'sandbox',
      'access token',
      'not support',
      'dashboard tokens',
    ];

    const findings = {
      baseUrl: BASE,
      finalUrl: page.url(),
      sessionResponse,
      apiSnippets: extractSnippets(apiText, keywordPatterns),
      sdkSnippets: extractSnippets(sdkText, keywordPatterns),
      apiContainsDecrypt: /decrypt/i.test(apiText),
      sdkContainsDecrypt: /decrypt/i.test(sdkText),
      apiContainsAutomationToken: /automation token|dashboard token|manual token/i.test(apiText),
      sdkContainsAutomationToken: /automation token|dashboard token|manual token/i.test(sdkText),
      apiTextLength: apiText.length,
      sdkTextLength: sdkText.length,
    };

    await saveJson(artifacts.browserJson, findings);
    console.log(JSON.stringify(findings, null, 2));
  } finally {
    await context.close();
    await browser.close();
  }
}

await main();
