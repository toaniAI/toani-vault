import fs from 'node:fs';
import path from 'node:path';
import { chromium } from '/Users/yvan/AIWorkspace/credbridge/frontend/node_modules/playwright/index.mjs';

const BASE = process.env.TEST_BASE || 'https://dev-credbridge.bitkinetic.com';
const EMAIL = process.env.TEST_EMAIL || 'test-7226@privy.io';
const OTP = process.env.TEST_OTP || '450192';
const OUT_DIR =
  process.env.OUT_DIR ||
  '/Users/yvan/AIWorkspace/credbridge/docs/qa_reports/tee-real-regression-20260413-185715/playwright';

fs.mkdirSync(OUT_DIR, { recursive: true });

const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({
  baseURL: BASE,
  recordHar: {
    path: path.join(OUT_DIR, 'session.har'),
    mode: 'full',
  },
  viewport: { width: 1440, height: 1100 },
});
const page = await context.newPage();

const observations = {
  baseUrl: BASE,
  email: EMAIL,
  captured: {
    backendSessionToken: null,
    privyAccessToken: null,
  },
  pages: {},
  network: {
    authSessionStatus: null,
    authSessionBody: null,
  },
};

page.on('response', async response => {
  try {
    const url = response.url();
    if (!url.includes('/api/v1/auth/session')) {
      return;
    }
    observations.network.authSessionStatus = response.status();
    if (response.request().method() !== 'POST') {
      return;
    }
    const text = await response.text();
    observations.network.authSessionBody = text;
    const body = JSON.parse(text);
    const token = body?.session?.session_token ?? null;
    if (token) {
      observations.captured.backendSessionToken = token;
    }
  } catch {
    // Keep the run going; raw evidence is enough when JSON parsing fails.
  }
});

async function maybeClick(regexes) {
  for (const regex of regexes) {
    const button = page.getByRole('button', { name: regex }).first();
    if ((await button.count()) === 0) {
      continue;
    }
    try {
      await button.click({ timeout: 5_000 });
      return true;
    } catch {
      // Keep trying the remaining candidates.
    }
  }
  return false;
}

async function waitForVisible(candidates, timeout = 15_000) {
  for (const selector of candidates) {
    const locator = page.locator(selector).first();
    try {
      await locator.waitFor({ state: 'visible', timeout });
      return locator;
    } catch {
      // Try next selector.
    }
  }
  return null;
}

async function fillOtpCode(code) {
  const splitInputs = page.locator('input[name^="code-"]');
  const count = await splitInputs.count();
  if (count >= code.length) {
    for (let i = 0; i < code.length; i += 1) {
      await splitInputs.nth(i).fill(code[i]);
    }
    return;
  }

  const fallbackInput = await waitForVisible([
    'input[autocomplete="one-time-code"]',
    'input[inputmode="numeric"]',
    'input[name*="code" i]',
    'input[type="tel"]',
    'input[maxlength="6"]',
  ]);
  if (!fallbackInput) {
    throw new Error('OTP input not found');
  }
  await fallbackInput.click({ timeout: 5_000 });
  await page.keyboard.type(code, { delay: 50 });
}

async function capturePage(name, route, expectations = {}) {
  await page.goto(route, { waitUntil: 'networkidle', timeout: 60_000 });
  await page.screenshot({
    path: path.join(OUT_DIR, `${name}.png`),
    fullPage: true,
  });

  const text = await page.locator('body').innerText();
  observations.pages[name] = {
    url: page.url(),
    title: await page.title(),
    bodyExcerpt: text.slice(0, 2_000),
    checks: {},
  };

  for (const [key, pattern] of Object.entries(expectations)) {
    observations.pages[name].checks[key] =
      typeof pattern === 'string'
        ? text.includes(pattern)
        : pattern instanceof RegExp
          ? pattern.test(text)
          : typeof pattern === 'function'
            ? pattern(text)
            : Boolean(pattern);
  }
}

try {
  await page.goto('/login', { waitUntil: 'networkidle', timeout: 60_000 });
  await page.screenshot({
    path: path.join(OUT_DIR, 'login.png'),
    fullPage: true,
  });

  await maybeClick([
    /sign in with email code/i,
    /email/i,
    /continue/i,
    /登录/i,
    /邮箱/i,
  ]);

  const emailInput = await waitForVisible([
    'input[type="email"]',
    'input[name*="email" i]',
    'input[placeholder*="mail" i]',
  ]);
  if (!emailInput) {
    throw new Error('Email input not found');
  }
  await emailInput.fill(EMAIL);

  const sent = await maybeClick([/submit/i, /continue/i, /send/i, /登录|继续|发送/i]);
  if (!sent) {
    await page.keyboard.press('Enter');
  }

  await fillOtpCode(OTP);
  await maybeClick([/verify/i, /sign in/i, /continue/i, /登录|验证|继续/i]);

  const start = Date.now();
  while (!observations.captured.backendSessionToken && Date.now() - start < 90_000) {
    await page.waitForTimeout(500);
  }

  if (!observations.captured.backendSessionToken) {
    throw new Error('Backend session token was not captured');
  }

  observations.captured.privyAccessToken = await page.evaluate(async () => {
    const maybePrivy = window?.localStorage;
    if (!maybePrivy) {
      return null;
    }
    const entries = [];
    for (let i = 0; i < localStorage.length; i += 1) {
      const key = localStorage.key(i);
      if (!key) {
        continue;
      }
      const value = localStorage.getItem(key);
      if (key.toLowerCase().includes('privy') || String(value).includes('privy')) {
        entries.push([key, value]);
      }
    }
    return entries;
  });

  await capturePage('credentials', '/credentials', {
    noDecryptAction: text => !/decrypt credential|查看凭证|解密|confirm view/i.test(text),
  });
  await capturePage('tokens', '/tokens', {
    hasCredentialRead: /credential:read/i,
    noVerifyToken: text => !/verify token/i.test(text),
    noAutomationToken: text => !/automation token/i.test(text),
  });
  await capturePage('profile', '/profile', {
    noAutomationToken: text => !/automation token/i.test(text),
  });
  await capturePage('developer-center', '/developer', {
    noDecryptExample: text =>
      !/sdk\.credentials\.decrypt|decrypt a credential payload|解密凭证内容/i.test(text),
    noVerifyTokenExample: text => !/verify token/i.test(text),
    noAutomationTokenExample: text => !/automation token/i.test(text),
  });

  fs.writeFileSync(
    path.join(OUT_DIR, 'ui-observations.json'),
    JSON.stringify(observations, null, 2)
  );
  console.log(JSON.stringify(observations, null, 2));
} finally {
  await context.close();
  await browser.close();
}
