import fs from 'node:fs';
import path from 'node:path';
import { chromium } from '/Users/yvan/AIWorkspace/credbridge/frontend/node_modules/playwright/index.mjs';

const BASE = process.env.TEST_BASE || 'https://dev-credbridge.bitkinetic.com';
const EMAIL = process.env.TEST_EMAIL || 'test-7226@privy.io';
const OTP = process.env.TEST_OTP || '450192';
const OUT_DIR =
  process.env.OUT_DIR ||
  '/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-login';

fs.mkdirSync(OUT_DIR, { recursive: true });

const observation = {
  caseId: 'case02',
  runId: 'tee-full-20260413T212736',
  baseUrl: BASE,
  actor: {
    email: EMAIL,
  },
  startedAt: new Date().toISOString(),
  login: {
    before: null,
    after: null,
    landingUrl: null,
    landingTitle: null,
  },
  session: {
    backendSessionToken: null,
    cookies: [],
    storage: {
      localStorage: {},
      sessionStorage: {},
    },
  },
  pages: {},
  network: [],
  console: [],
  firstFailure: null,
  status: 'running',
};

const interestingResponse = url => {
  return [
    '/api/v1/auth/session',
    '/api/v1/auth/me',
    '/api/v1/auth/memberships',
    '/api/v1/credentials',
    '/api/v1/tokens',
    '/api/v1/audit',
    '/api/v1/profile',
  ].some(fragment => url.includes(fragment));
};

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

page.on('console', message => {
  observation.console.push({
    type: message.type(),
    text: message.text(),
    location: message.location(),
    capturedAt: new Date().toISOString(),
  });
});

page.on('response', async response => {
  const url = response.url();
  if (!interestingResponse(url)) {
    return;
  }

  let body = null;
  try {
    body = await response.text();
  } catch {
    body = null;
  }

  const entry = {
    url,
    method: response.request().method(),
    status: response.status(),
    statusText: response.statusText(),
    resourceType: response.request().resourceType(),
    capturedAt: new Date().toISOString(),
    bodySnippet: body ? body.slice(0, 4000) : null,
  };

  observation.network.push(entry);

  if (url.includes('/api/v1/auth/session') && response.request().method() === 'POST' && body) {
    try {
      const parsed = JSON.parse(body);
      observation.session.backendSessionToken = parsed?.session?.session_token ?? null;
    } catch {
      // Keep raw bodySnippet as evidence when parsing fails.
    }
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
      // Keep trying candidates.
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
      // Try next candidate.
    }
  }
  return null;
}

async function fillOtpCode(code) {
  const splitInputs = page.locator('input[name^="code-"]');
  const splitCount = await splitInputs.count();
  if (splitCount >= code.length) {
    for (let index = 0; index < code.length; index += 1) {
      await splitInputs.nth(index).fill(code[index]);
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

async function readStorage() {
  observation.session.storage = await page.evaluate(() => {
    const readEntries = storage => {
      const result = {};
      for (let i = 0; i < storage.length; i += 1) {
        const key = storage.key(i);
        if (!key) {
          continue;
        }
        result[key] = storage.getItem(key);
      }
      return result;
    };

    return {
      localStorage: readEntries(window.localStorage),
      sessionStorage: readEntries(window.sessionStorage),
    };
  });
}

async function screenshot(name) {
  const target = path.join(OUT_DIR, name);
  await page.screenshot({ path: target, fullPage: true });
  return target;
}

async function captureRoute(name, route) {
  await page.goto(route, { waitUntil: 'networkidle', timeout: 60_000 });
  const shotPath = await screenshot(`${name}.png`);
  const bodyText = await page.locator('body').innerText();
  observation.pages[name] = {
    requestedRoute: route,
    finalUrl: page.url(),
    title: await page.title(),
    screenshot: shotPath,
    bodyExcerpt: bodyText.slice(0, 2000),
    access: {
      sameRoute:
        page.url() === `${BASE}${route}` ||
        page.url() === `${BASE}${route}/`,
      redirectedToCredentials: page.url().includes('/credentials') && route !== '/credentials',
    },
  };
}

async function persistObservation() {
  fs.writeFileSync(
    path.join(OUT_DIR, 'playwright-observation.json'),
    JSON.stringify(observation, null, 2),
    'utf8'
  );
}

async function recordFirstFailure(error) {
  if (observation.firstFailure) {
    return;
  }
  try {
    const shotPath = await screenshot('first-failure.png');
    observation.firstFailure = {
      message: error instanceof Error ? error.message : String(error),
      url: page.url(),
      title: await page.title(),
      screenshot: shotPath,
      capturedAt: new Date().toISOString(),
    };
  } catch {
    observation.firstFailure = {
      message: error instanceof Error ? error.message : String(error),
      url: page.url(),
      capturedAt: new Date().toISOString(),
    };
  }
}

try {
  await page.goto('/login', { waitUntil: 'networkidle', timeout: 60_000 });
  observation.login.before = {
    url: page.url(),
    title: await page.title(),
    screenshot: await screenshot('before-login.png'),
  };
  await persistObservation();

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
  const submitted = await maybeClick([/submit/i, /continue/i, /send/i, /登录|继续|发送/i]);
  if (!submitted) {
    await page.keyboard.press('Enter');
  }

  await fillOtpCode(OTP);
  await maybeClick([/verify/i, /sign in/i, /continue/i, /登录|验证|继续/i]);

  const authDeadline = Date.now() + 90_000;
  while (Date.now() < authDeadline) {
    if (observation.session.backendSessionToken && !page.url().includes('/login')) {
      break;
    }
    await page.waitForTimeout(500);
  }

  if (!observation.session.backendSessionToken) {
    throw new Error('Backend session token was not captured');
  }

  observation.login.landingUrl = page.url();
  observation.login.landingTitle = await page.title();
  observation.login.after = {
    url: page.url(),
    title: await page.title(),
    screenshot: await screenshot('after-login.png'),
  };
  observation.session.cookies = await context.cookies();
  await readStorage();

  await captureRoute('credentials', '/credentials');
  await captureRoute('tokens', '/tokens');
  await captureRoute('audit', '/audit');
  await captureRoute('profile', '/profile');
  await captureRoute('developer-center', '/developer');

  observation.status = 'passed';
  observation.finishedAt = new Date().toISOString();
  await persistObservation();
} catch (error) {
  observation.status = 'failed';
  observation.finishedAt = new Date().toISOString();
  await recordFirstFailure(error);
  await persistObservation();
  throw error;
} finally {
  await context.close();
  await browser.close();
}
