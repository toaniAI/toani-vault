async page => {
  const OUT_DIR =
    '/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/web-credentials';
  const BASE = 'https://dev-credbridge.bitkinetic.com';
  const PREFIX = 'tee-full-20260413T232909-webcred';
  const stamp = new Date().toISOString().replace(/[-:.TZ]/g, '').slice(0, 14);
  const serviceId = `${PREFIX}-${stamp}`;
  const username = `${PREFIX}-user`;
  const password = `${PREFIX}-Pw!2026`;

  const responseLog = [];
  const createdIds = [];
  let createResponse = null;
  let deleteResponse = null;
  let detailSurface = {
    surfaced: false,
    mode: 'not_available',
    finalUrl: '',
    bodyExcerpt: '',
  };

  const jsonSafe = async response => {
    try {
      const text = await response.text();
      try {
        return { text, json: JSON.parse(text) };
      } catch {
        return { text, json: null };
      }
    } catch (error) {
      return {
        text: `__read_error__: ${error instanceof Error ? error.message : String(error)}`,
        json: null,
      };
    }
  };

  page.on('response', async response => {
    const url = response.url();
    if (!url.includes('/api/v1/credentials')) return;
    const method = response.request().method();
    const payload = await jsonSafe(response);
    const entry = {
      url,
      method,
      status: response.status(),
      ok: response.ok(),
      bodyText: payload.text,
      bodyJson: payload.json,
    };
    responseLog.push(entry);
    if (method === 'POST') {
      createResponse = entry;
      if (payload.json?.credential_id) {
        createdIds.push({
          credential_id: payload.json.credential_id,
          label: payload.json.service_id ?? serviceId,
        });
      }
    }
    if (method === 'DELETE') {
      deleteResponse = entry;
    }
  });

  const textHasPlaintext = text =>
    Boolean(
      text &&
        [username, password, 'plaintext_data', '"password"', '"username"'].some(token => text.includes(token)),
    );

  const bodyText = async () => (await page.locator('body').innerText().catch(() => '')).slice(0, 8000);

  await page.goto(`${BASE}/credentials`, { waitUntil: 'domcontentloaded', timeout: 60000 });
  await page.waitForLoadState('networkidle', { timeout: 60000 }).catch(() => {});
  await page.screenshot({
    path: `${OUT_DIR}/credentials-before-create.png`,
    fullPage: true,
  });

  const beforeBody = await bodyText();
  const listVisible = await page
    .getByRole('heading', { name: /凭证管理|Credentials/i })
    .first()
    .isVisible()
    .catch(() => false);

  await page.getByRole('button', { name: /新建凭证|new credential/i }).first().click();
  await page.waitForTimeout(1000);
  await page.screenshot({
    path: `${OUT_DIR}/new-credential-modal.png`,
    fullPage: true,
  });

  const inputs = page.locator('input');
  await inputs.nth(0).fill(serviceId);
  await inputs.nth(1).fill(username);
  await inputs.nth(2).fill(password);

  await page
    .getByRole('button', { name: /安全创建|创建|create|submit/i })
    .last()
    .click();

  await page.waitForTimeout(3000);
  await page.waitForLoadState('networkidle', { timeout: 30000 }).catch(() => {});
  await page.screenshot({
    path: `${OUT_DIR}/credentials-after-create.png`,
    fullPage: true,
  });

  const createdHeading = page.getByRole('heading', { name: new RegExp(serviceId, 'i') }).first();
  const createdVisible = await createdHeading.isVisible().catch(() => false);
  const afterCreateBody = await bodyText();

  if (createdVisible) {
    const startUrl = page.url();
    await createdHeading.click({ timeout: 5000 }).catch(() => {});
    await page.waitForTimeout(1500);
    const currentUrl = page.url();
    const afterClickBody = await bodyText();
    const dialogVisible = await page.locator('[role="dialog"]').first().isVisible().catch(() => false);

    detailSurface = {
      surfaced: currentUrl !== startUrl || dialogVisible,
      mode: currentUrl !== startUrl ? 'route' : dialogVisible ? 'dialog' : 'not_available',
      finalUrl: currentUrl,
      bodyExcerpt: afterClickBody,
    };

    await page.screenshot({
      path: `${OUT_DIR}/credentials-after-detail-attempt.png`,
      fullPage: true,
    });
  }

  let deleteSurface = { surfaced: false, confirmed: false, stillVisible: createdVisible };
  const createdCard = page
    .locator('div')
    .filter({
      has: page.getByRole('heading', { name: new RegExp(serviceId, 'i') }),
    })
    .filter({
      has: page.getByRole('button', { name: /删除|delete/i }),
    })
    .first();

  if ((await createdCard.count()) > 0) {
    const deleteButton = createdCard.getByRole('button', { name: /删除|delete/i }).first();
    deleteSurface.surfaced = await deleteButton.isVisible().catch(() => false);
    if (deleteSurface.surfaced) {
      await deleteButton.click({ timeout: 5000 }).catch(() => {});
      await page.waitForTimeout(1000);
      const confirm = page.getByRole('button', { name: /确认|确定|删除|confirm/i }).last();
      if (await confirm.isVisible().catch(() => false)) {
        await confirm.click({ timeout: 5000 }).catch(() => {});
        deleteSurface.confirmed = true;
      }
      await page.waitForTimeout(2500);
      await page.waitForLoadState('networkidle', { timeout: 30000 }).catch(() => {});
      deleteSurface.stillVisible = await createdHeading.isVisible().catch(() => false);
      await page.screenshot({
        path: `${OUT_DIR}/credentials-after-delete-attempt.png`,
        fullPage: true,
      });
    }
  }

  const plaintextInUi =
    textHasPlaintext(afterCreateBody) ||
    textHasPlaintext(detailSurface.bodyExcerpt) ||
    textHasPlaintext(beforeBody);

  const plaintextInResponses = responseLog.some(entry => textHasPlaintext(entry.bodyText));

  const output = {
    target: BASE,
    service_id: serviceId,
    username,
    password,
    list_rendered: listVisible,
    created_visible_in_ui: createdVisible,
    detail_surface: detailSurface,
    delete_surface: deleteSurface,
    plaintext_exposed_in_ui: plaintextInUi,
    plaintext_exposed_in_responses: plaintextInResponses,
    create_response: createResponse,
    delete_response: deleteResponse,
    created_ids: createdIds,
    response_log: responseLog,
    page_url: page.url(),
    title: await page.title(),
    body_excerpt: afterCreateBody,
  };
  return output;
}
