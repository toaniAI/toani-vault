const fs = require('fs/promises');
const path = require('path');
const { chromium } = require('/Users/yvan/AIWorkspace/credbridge/frontend/node_modules/playwright');

const RUN_ID = 'tee-full-20260413T212736';
const CASE_ID = 'case03';
const PREFIX = 'tee-full-case03-20260413T212736';
const BASE_URL = 'https://dev-credbridge.bitkinetic.com/';
const EMAIL = 'test-7226@privy.io';
const OTP = '450192';
const OUT_DIR = '/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T212736/web-credentials';
const RESULT_PATH = path.join(OUT_DIR, 'result.json');
const NOTES_PATH = path.join(OUT_DIR, 'notes.md');
const NETWORK_PATH = path.join(OUT_DIR, 'network-summary.json');
const STORAGE_PATH = path.join(OUT_DIR, 'storage-state.json');
const DOM_DUMP_PATH = path.join(OUT_DIR, 'dom-dump.html');
const BODY_TEXT_PATH = path.join(OUT_DIR, 'body-text.txt');
const TRACE_PATH = path.join(OUT_DIR, 'trace.zip');

const created = {
  credentialId: null,
  serviceId: `${PREFIX}-svc`,
};

const summary = {
  runId: RUN_ID,
  caseId: CASE_ID,
  baseUrl: BASE_URL,
  startedAt: new Date().toISOString(),
  actor: EMAIL,
  objectPrefix: PREFIX,
  claimResult: 'inconclusive',
  failureClass: null,
  notes: [],
  screenshots: [],
  createdIds: [],
  cleanup: {
    attempted: false,
    supportedByUi: false,
    deleted: false,
  },
  network: {
    session: [],
    credentialList: [],
    credentialCreate: [],
    credentialGet: [],
    credentialDelete: [],
    decrypt: [],
    otherInteresting: [],
  },
  observedContract: {
    credentialsPageReached: false,
    createSucceeded: false,
    listShowsMetadataOnly: false,
    detailEntryPresent: false,
    detailRequestObserved: false,
    decryptEntryPresent: false,
    decryptRequestObserved: false,
  },
  ui: {
    currentUrl: null,
    titles: [],
    visibleTexts: {},
  },
  artifacts: {},
};

function sanitizeForJson(value) {
  if (value === null || value === undefined) return value;
  if (Array.isArray(value)) return value.map(sanitizeForJson);
  if (typeof value === 'object') {
    const next = {};
    for (const [k, v] of Object.entries(value)) {
      if (typeof v === 'function') continue;
      next[k] = sanitizeForJson(v);
    }
    return next;
  }
  return value;
}

function clip(value, max = 800) {
  if (typeof value !== 'string') return value;
  if (value.length <= max) return value;
  return `${value.slice(0, max)}…`;
}

async function writeJson(filePath, value) {
  await fs.writeFile(filePath, JSON.stringify(sanitizeForJson(value), null, 2));
}

async function appendCommand(cmd) {
  await fs.appendFile(path.join(OUT_DIR, 'commands.txt'), `${new Date().toISOString()} ${cmd}\n`);
}

async function markScreenshot(name) {
  const filePath = path.join(OUT_DIR, name);
  summary.screenshots.push(filePath);
  return filePath;
}

function pickBucket(url) {
  if (url.includes('/api/v1/auth/session')) return 'session';
  if (url.match(/\/api\/v1\/credentials(\?.*)?$/)) return 'credentialList';
  if (url.match(/\/api\/v1\/credentials\/[^/]+$/)) return 'credentialGet';
  if (url.match(/\/api\/v1\/credentials\/[^/]+\/decrypt$/)) return 'decrypt';
  return 'otherInteresting';
}

async function recordResponse(response, bodyText) {
  const request = response.request();
  const url = response.url();
  if (!url.includes('/api/')) return;

  const entry = {
    url,
    method: request.method(),
    status: response.status(),
    requestHeaders: request.headers(),
    requestPostData: clip(request.postData() || '', 1200),
    responseHeaders: response.headers(),
    responseBodyPreview: clip(bodyText || '', 1800),
    observedAt: new Date().toISOString(),
  };

  const bucket = pickBucket(url);
  if (request.method() === 'POST' && url.match(/\/api\/v1\/credentials$/)) {
    summary.network.credentialCreate.push(entry);
    return;
  }
  if (request.method() === 'DELETE' && url.match(/\/api\/v1\/credentials\/[^/]+$/)) {
    summary.network.credentialDelete.push(entry);
    return;
  }
  summary.network[bucket].push(entry);
}

async function saveTextArtifacts(page) {
  await fs.writeFile(BODY_TEXT_PATH, await page.locator('body').innerText());
  await fs.writeFile(DOM_DUMP_PATH, await page.content());
  summary.artifacts.bodyText = BODY_TEXT_PATH;
  summary.artifacts.domDump = DOM_DUMP_PATH;
}

async function tryClick(page, candidates) {
  for (const locator of candidates) {
    const count = await locator.count();
    if (count > 0) {
      await locator.first().click();
      return true;
    }
  }
  return false;
}

async function loginIfNeeded(page) {
  await page.goto(BASE_URL, { waitUntil: 'networkidle', timeout: 120000 });
  summary.ui.currentUrl = page.url();
  summary.ui.titles.push(await page.title());

  if (page.url().includes('/credentials')) {
    summary.observedContract.credentialsPageReached = true;
    return;
  }

  await page.screenshot({ path: await markScreenshot('01-login-page.png'), fullPage: true });

  const loginClicked = await tryClick(page, [
    page.getByRole('button', { name: /Sign in with Email Code/i }),
    page.getByRole('button', { name: /邮箱验证码登录/i }),
    page.getByRole('button', { name: /Connect Email/i }),
    page.locator('button').filter({ hasText: /Email/i }),
  ]);

  if (!loginClicked) {
    throw new Error('Could not find login button on login page');
  }

  await page.waitForTimeout(2500);

  const modal = page.locator('#privy-dialog');

  const emailLocator = modal.locator('#email-input, input[type="email"], input[autocomplete="email"]').first();
  await emailLocator.waitFor({ state: 'attached', timeout: 30000 });
  await emailLocator.fill(EMAIL);

  const submitClicked = await tryClick(page, [
    modal.getByRole('button', { name: /^Submit$/i }),
    modal.locator('button').filter({ hasText: /^Submit$/i }),
    modal.locator('button').filter({ hasText: /Send code|Continue|Next/i }),
  ]);

  if (!submitClicked) {
    throw new Error('Could not submit email in Privy modal');
  }

  await page.waitForTimeout(4000);

  let otpFilled = false;
  const otpInputs = modal.locator(
    'input[inputmode="numeric"], input[autocomplete="one-time-code"], input[name*="code" i], input[placeholder*="code" i]'
  );
  const count = await otpInputs.count();
  if (count >= 6) {
    for (let i = 0; i < Math.min(6, OTP.length); i += 1) {
      await otpInputs.nth(i).fill(OTP[i]);
    }
    otpFilled = true;
  } else if (count === 1) {
    await otpInputs.first().fill(OTP);
    otpFilled = true;
  }

  if (!otpFilled) {
    const modalShot = await markScreenshot('02-login-modal-missing-otp.png');
    await page.screenshot({ path: modalShot, fullPage: true });
    throw new Error('Could not find OTP input in Privy modal');
  }

  await page.waitForLoadState('networkidle', { timeout: 120000 }).catch(() => {});
  await page.waitForTimeout(4000);

  const maybeCredentials = page.url().includes('/credentials') || (await page.getByText(/凭证管理|Credential/i).count()) > 0;
  if (!maybeCredentials) {
    const goToCredentials = page.getByRole('link', { name: /Credentials|凭证管理/i });
    if (await goToCredentials.count()) {
      await goToCredentials.first().click();
      await page.waitForLoadState('networkidle', { timeout: 120000 }).catch(() => {});
    }
  }

  await page.waitForURL(/\/credentials/, { timeout: 120000 });
  summary.observedContract.credentialsPageReached = true;
  summary.ui.currentUrl = page.url();
  summary.ui.titles.push(await page.title());
  await page.screenshot({ path: await markScreenshot('03-credentials-page-before-create.png'), fullPage: true });
}

async function createCredential(page) {
  await page.getByRole('button', { name: /New credential|新建凭证/i }).click();
  await page.getByRole('heading', { name: /Create credential|新建凭证/i }).waitFor({
    timeout: 30000,
  });

  await page.getByLabel(/Service|服务标识/i).fill(created.serviceId);
  const username = `${PREFIX}-user`;
  const password = `${PREFIX}-pw-!23`;
  await page.getByLabel(/Username|用户名/i).fill(username);
  await page.getByLabel(/^Password$|密码/i).fill(password);

  await page.getByRole('button', { name: /Secure Create|安全创建/i }).click();
  await page.waitForLoadState('networkidle', { timeout: 120000 }).catch(() => {});
  await page.waitForTimeout(3000);

  await page.screenshot({ path: await markScreenshot('04-credentials-page-after-create.png'), fullPage: true });

  const card = page.locator('[class*="Card"], article, div').filter({ hasText: created.serviceId }).first();
  await card.waitFor({ timeout: 30000 });

  const bodyText = await page.locator('body').innerText();
  summary.ui.visibleTexts.credentialsPage = clip(bodyText, 4000);
  summary.observedContract.createSucceeded = bodyText.includes(created.serviceId);
  summary.observedContract.listShowsMetadataOnly =
    bodyText.includes(created.serviceId) &&
    !bodyText.includes(`${PREFIX}-pw-!23`) &&
    !bodyText.includes(`${PREFIX}-user`);

  const listEntryText = await card.innerText().catch(() => '');
  summary.ui.visibleTexts.createdCard = clip(listEntryText, 1200);

  const createResponse = summary.network.credentialCreate.at(-1);
  if (createResponse) {
    try {
      const parsed = JSON.parse(createResponse.responseBodyPreview);
      created.credentialId = parsed.credential_id || parsed.data?.credential_id || null;
    } catch {}
  }

  if (!created.credentialId) {
    const idMatch = bodyText.match(/\b[a-f0-9]{8}\.\.\./i);
    if (idMatch) {
      summary.notes.push(`UI only shows truncated id fragment: ${idMatch[0]}`);
    }
  }

  if (created.credentialId) {
    summary.createdIds.push(created.credentialId);
  }
}

async function inspectDetailAndDecrypt(page) {
  const bodyText = await page.locator('body').innerText();
  const hasDetailEntry =
    /View|Details|详情|查看/.test(bodyText) &&
    (await page.getByRole('button', { name: /View|Details|详情|查看/i }).count()) > 0;
  const hasDecryptEntry =
    /Decrypt|解密/.test(bodyText) &&
    (await page.getByRole('button', { name: /Decrypt|解密/i }).count()) > 0;

  summary.observedContract.detailEntryPresent = hasDetailEntry;
  summary.observedContract.decryptEntryPresent = hasDecryptEntry;
  summary.observedContract.detailRequestObserved = summary.network.credentialGet.length > 0;
  summary.observedContract.decryptRequestObserved = summary.network.decrypt.length > 0;
}

async function deleteIfSupported(page) {
  const deleteButton = page
    .locator('button')
    .filter({ hasText: /Delete|删除/i })
    .filter({ has: page.locator('svg') });

  if ((await deleteButton.count()) === 0) {
    summary.cleanup.supportedByUi = false;
    summary.notes.push('No delete entry found in UI; recorded as current contract.');
    return;
  }

  summary.cleanup.supportedByUi = true;
  summary.cleanup.attempted = true;

  const targetCard = page.locator('div').filter({ hasText: created.serviceId }).first();
  const targetDelete = targetCard.locator('button').filter({ hasText: /Delete|删除/i }).first();
  if ((await targetDelete.count()) === 0) {
    summary.notes.push('Delete button exists on page but not within created credential card.');
    return;
  }

  await targetDelete.click();
  const dialog = page.getByRole('dialog');
  await dialog.waitFor({ timeout: 30000 });
  await page.screenshot({ path: await markScreenshot('05-delete-dialog.png'), fullPage: true });

  await dialog.getByRole('button', { name: /Confirm Delete|确认删除/i }).click();
  await page.waitForLoadState('networkidle', { timeout: 120000 }).catch(() => {});
  await page.waitForTimeout(3000);

  const bodyText = await page.locator('body').innerText();
  summary.cleanup.deleted = !bodyText.includes(created.serviceId);
  await page.screenshot({ path: await markScreenshot('06-after-delete.png'), fullPage: true });
}

async function writeOutputs() {
  summary.endedAt = new Date().toISOString();
  summary.artifacts.result = RESULT_PATH;
  summary.artifacts.notes = NOTES_PATH;
  summary.artifacts.network = NETWORK_PATH;
  summary.artifacts.storageState = STORAGE_PATH;
  summary.artifacts.trace = TRACE_PATH;

  const notes = [
    `# Case03 Notes`,
    ``,
    `- Run ID: \`${RUN_ID}\``,
    `- Case: \`${CASE_ID}\``,
    `- Base URL: ${BASE_URL}`,
    `- Actor: \`${EMAIL}\``,
    `- Object prefix: \`${PREFIX}\``,
    `- Claim result: \`${summary.claimResult}\``,
    `- Failure class: \`${summary.failureClass || 'none'}\``,
    `- Created credential ID: \`${created.credentialId || 'not captured'}\``,
    `- Cleanup: attempted=\`${summary.cleanup.attempted}\`, supportedByUi=\`${summary.cleanup.supportedByUi}\`, deleted=\`${summary.cleanup.deleted}\``,
    ``,
    `## Contract observations`,
    `- Credentials page reached: \`${summary.observedContract.credentialsPageReached}\``,
    `- Create succeeded: \`${summary.observedContract.createSucceeded}\``,
    `- List showed metadata only: \`${summary.observedContract.listShowsMetadataOnly}\``,
    `- Detail entry present: \`${summary.observedContract.detailEntryPresent}\``,
    `- Detail request observed: \`${summary.observedContract.detailRequestObserved}\``,
    `- Decrypt entry present: \`${summary.observedContract.decryptEntryPresent}\``,
    `- Decrypt request observed: \`${summary.observedContract.decryptRequestObserved}\``,
    ``,
    `## Notes`,
    ...summary.notes.map(note => `- ${note}`),
    ``,
    `## Evidence`,
    ...summary.screenshots.map(file => `- ${file}`),
  ].join('\n');

  await fs.writeFile(NOTES_PATH, notes);
  await writeJson(NETWORK_PATH, summary.network);
  await writeJson(RESULT_PATH, summary);
}

async function main() {
  await fs.mkdir(OUT_DIR, { recursive: true });
  await appendCommand('rtk node qa_reports/.../web-credentials/run-case03.cjs');

  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({
    viewport: { width: 1440, height: 1100 },
    ignoreHTTPSErrors: true,
    recordVideo: { dir: OUT_DIR, size: { width: 1440, height: 900 } },
  });

  const page = await context.newPage();
  await context.tracing.start({ screenshots: true, snapshots: true, sources: false });

  page.on('response', async response => {
    try {
      const bodyText = await response.text().catch(() => '');
      await recordResponse(response, bodyText);
    } catch (error) {
      summary.notes.push(`Failed to record response ${response.url()}: ${error.message}`);
    }
  });

  try {
    await loginIfNeeded(page);
    await createCredential(page);
    await inspectDetailAndDecrypt(page);
    await deleteIfSupported(page);

    summary.claimResult =
      summary.observedContract.credentialsPageReached &&
      summary.observedContract.createSucceeded &&
      summary.observedContract.listShowsMetadataOnly &&
      !summary.observedContract.decryptEntryPresent &&
      !summary.observedContract.decryptRequestObserved
        ? 'passed'
        : 'failed';
  } catch (error) {
    summary.claimResult = 'failed';
    summary.failureClass = 'env';
    summary.notes.push(error.stack || error.message);
    await page.screenshot({
      path: await markScreenshot('99-failure.png'),
      fullPage: true,
    }).catch(() => {});
    await saveTextArtifacts(page).catch(() => {});
  } finally {
    await context.storageState({ path: STORAGE_PATH }).catch(() => {});
    await context.tracing.stop({ path: TRACE_PATH }).catch(() => {});
    await saveTextArtifacts(page).catch(() => {});
    await browser.close().catch(() => {});
    await writeOutputs();
  }
}

main().catch(async error => {
  summary.claimResult = 'failed';
  summary.failureClass = summary.failureClass || 'test_harness';
  summary.notes.push(error.stack || error.message);
  await writeOutputs().catch(() => {});
  process.exitCode = 1;
});
