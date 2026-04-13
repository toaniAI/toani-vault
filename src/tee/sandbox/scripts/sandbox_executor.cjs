const readline = require('readline');
const { chromium } = require('playwright');

let context = null;
let page = null;

async function applyMaskSelectors(currentPage, selectors) {
  if (!Array.isArray(selectors) || selectors.length === 0) {
    return;
  }

  await currentPage.evaluate(maskSelectors => {
    for (const selector of maskSelectors) {
      const element = document.querySelector(selector);
      if (!element) {
        continue;
      }

      element.setAttribute('data-credbridge-masked', 'true');
      element.setAttribute(
        'style',
        `${element.getAttribute('style') || ''}; background:#111 !important; color:transparent !important; text-shadow:none !important; -webkit-text-security:disc !important;`
      );
    }
  }, selectors);
}

function reply(payload) {
  process.stdout.write(`${JSON.stringify(payload)}\n`);
}

async function ensurePage() {
  if (!context) {
    throw new Error('browser context not initialized');
  }

  if (!page || page.isClosed()) {
    const pages = context.pages();
    page = pages[0] || (await context.newPage());
  }

  return page;
}

function selectorLooksSensitive(selector) {
  return typeof selector === 'string' && /pass(word)?|secret|token|otp/i.test(selector);
}

function redactSecretInString(value, secrets) {
  if (typeof value !== 'string' || secrets.length === 0) {
    return value;
  }

  let redacted = value;
  for (const secret of secrets) {
    if (!secret) continue;
    redacted = redacted.split(secret).join('[REDACTED]');
  }
  return redacted;
}

function redactResult(value, secrets) {
  if (typeof value === 'string') {
    return redactSecretInString(value, secrets);
  }
  if (Array.isArray(value)) {
    return value.map(item => redactResult(item, secrets));
  }
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value).map(([key, nested]) => [key, redactResult(nested, secrets)])
    );
  }
  return value;
}

function createCredbridgeHelper(currentPage, bindings) {
  return {
    async fill(selector, field) {
      const value = bindings[field];
      if (typeof value !== 'string') {
        throw new Error(`unknown credential field: ${field}`);
      }
      await currentPage.fill(selector, value);
      return true;
    },
    async click(selector) {
      await currentPage.click(selector);
      return true;
    },
    async waitFor(selector, timeoutMs = 30000) {
      if (typeof selector === 'string' && selector) {
        await currentPage.waitForSelector(selector, { timeout: timeoutMs });
      } else {
        await currentPage.waitForTimeout(timeoutMs);
      }
      return true;
    },
    async getText(selector) {
      if (selectorLooksSensitive(selector)) {
        throw new Error(`get_text blocked for sensitive selector: ${selector}`);
      }
      const locator = currentPage.locator(selector).first();
      const tagName = await locator.evaluate(el => el.tagName.toLowerCase());
      if (tagName === 'input' || tagName === 'textarea') {
        const inputType = await locator.evaluate(el =>
          (el.getAttribute('type') || '').toLowerCase()
        );
        if (inputType === 'password') {
          throw new Error(`get_text blocked for password input: ${selector}`);
        }
        return await locator.inputValue();
      }
      return await locator.innerText();
    },
    async setCookie(valueField, nameField) {
      const raw = bindings[valueField];
      if (typeof raw !== 'string') {
        throw new Error(`unknown credential field: ${valueField}`);
      }
      const cookieName = typeof nameField === 'string' ? bindings[nameField] || nameField : null;
      const cookieString = cookieName ? `${cookieName}=${raw}` : raw;
      await currentPage.evaluate(cookie => {
        document.cookie = cookie;
      }, cookieString);
      return true;
    },
    async navigate(url) {
      await currentPage.goto(url, { waitUntil: 'domcontentloaded', timeout: 30000 });
      return currentPage.url();
    },
  };
}

async function executeOperation(operationType, parameters) {
  const currentPage = await ensurePage();

  switch (operationType) {
    case 'navigate': {
      const url = parameters?.url;
      if (typeof url !== 'string' || !url) {
        throw new Error('navigate requires a string url');
      }
      await currentPage.goto(url, {
        waitUntil: 'domcontentloaded',
        timeout: parameters?.timeout_ms ?? 30000,
      });
      return { data: { final_url: currentPage.url() } };
    }
    case 'click': {
      const selector = parameters?.selector;
      if (typeof selector !== 'string' || !selector) {
        throw new Error('click requires selector');
      }
      await currentPage.click(selector, { timeout: parameters?.timeout_ms ?? 30000 });
      return { data: { clicked: true, selector, final_url: currentPage.url() } };
    }
    case 'fill': {
      const selector = parameters?.selector;
      const value = parameters?.value;
      if (typeof selector !== 'string' || !selector) {
        throw new Error('fill requires selector');
      }
      if (typeof value !== 'string') {
        throw new Error('fill requires resolved string value');
      }
      await currentPage.fill(selector, value, { timeout: parameters?.timeout_ms ?? 30000 });
      return { data: { filled: true, selector } };
    }
    case 'wait': {
      if (typeof parameters?.selector === 'string') {
        await currentPage.waitForSelector(parameters.selector, {
          timeout: parameters?.timeout_ms ?? 30000,
        });
        return { data: { waited_for: parameters.selector } };
      }
      const durationMs = Number(parameters?.duration_ms ?? parameters?.timeout_ms ?? 1000);
      await currentPage.waitForTimeout(durationMs);
      return { data: { waited_ms: durationMs } };
    }
    case 'get_text': {
      const selector = parameters?.selector;
      if (typeof selector !== 'string' || !selector) {
        throw new Error('get_text requires selector');
      }
      if (selectorLooksSensitive(selector)) {
        throw new Error('get_text blocked for sensitive selector');
      }
      const locator = currentPage.locator(selector).first();
      const tagName = await locator.evaluate(el => el.tagName.toLowerCase());
      if (tagName === 'input' || tagName === 'textarea') {
        const inputType = await locator.evaluate(el =>
          (el.getAttribute('type') || '').toLowerCase()
        );
        if (inputType === 'password') {
          throw new Error('get_text blocked for password input');
        }
        const value = await locator.inputValue();
        return { data: { text: value } };
      }
      const text = await locator.innerText({ timeout: parameters?.timeout_ms ?? 30000 });
      return { data: { text } };
    }
    case 'selector_metadata': {
      const selector = parameters?.selector;
      if (typeof selector !== 'string' || !selector) {
        throw new Error('selector_metadata requires selector');
      }
      const locator = currentPage.locator(selector).first();
      const tagName = await locator.evaluate(el => el.tagName.toLowerCase()).catch(() => null);
      if (!tagName) {
        return { data: { ok: false, reason: 'selector_not_found' } };
      }
      const inputType = await locator
        .evaluate(el => (el.getAttribute('type') || '').toLowerCase())
        .catch(() => '');
      const value =
        tagName === 'input' || tagName === 'textarea'
          ? await locator.inputValue().catch(() => '')
          : await locator.innerText({ timeout: parameters?.timeout_ms ?? 30000 }).catch(() => '');
      return { data: { ok: true, tag: tagName, type: inputType, value } };
    }
    case 'screenshot': {
      await applyMaskSelectors(currentPage, parameters?.maskSelectors);
      const screenshot = await currentPage.screenshot({ fullPage: true });
      return {
        data: { captured: true, final_url: currentPage.url() },
        screenshot_base64: screenshot.toString('base64'),
      };
    }
    case 'export': {
      const selectors = Array.isArray(parameters?.selectors) ? parameters.selectors : [];
      const maskSelectors = Array.isArray(parameters?.maskSelectors)
        ? parameters.maskSelectors
        : [];
      const rows = [];
      for (const selector of selectors) {
        if (typeof selector !== 'string' || !selector) {
          continue;
        }
        const text =
          selectorLooksSensitive(selector) || maskSelectors.includes(selector)
            ? '[REDACTED]'
            : await currentPage
                .locator(selector)
                .first()
                .innerText({ timeout: parameters?.timeout_ms ?? 30000 })
                .catch(() => '');
        rows.push({ selector, text });
      }
      return { data: { rows, final_url: currentPage.url() } };
    }
    case 'browser_state': {
      return {
        data: {
          url: currentPage.url(),
          title: await currentPage.title().catch(() => ''),
        },
      };
    }
    case 'execute_script': {
      const script = parameters?.script;
      if (typeof script !== 'string' || !script.trim()) {
        throw new Error('execute_script requires script');
      }

      const bindings =
        parameters?.bindings && typeof parameters.bindings === 'object' ? parameters.bindings : {};
      const secretValues = Object.values(bindings).filter(value => typeof value === 'string');

      const helper = createCredbridgeHelper(currentPage, bindings);
      const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
      const fn = new AsyncFunction('credbridge', `"use strict"; ${script}`);
      let result;
      try {
        result = await fn(helper);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        throw new Error(redactSecretInString(message, secretValues));
      }

      return {
        data: {
          result: redactResult(result, secretValues),
          used_credentials: Object.keys(bindings),
        },
      };
    }
    case 'close_runtime': {
      if (context) {
        await context.close();
        context = null;
        page = null;
      }
      return { data: { closed: true } };
    }
    default:
      throw new Error(`unsupported operation: ${operationType}`);
  }
}

const rl = readline.createInterface({
  input: process.stdin,
  crlfDelay: Infinity,
});

rl.on('line', async line => {
  try {
    const message = JSON.parse(line);

    if (message.type === 'init') {
      context = await chromium.launchPersistentContext(message.profileDir, {
        headless: true,
      });
      page = context.pages()[0] || (await context.newPage());
      reply({ ok: true, data: { ready: true } });
      return;
    }

    if (message.type === 'execute') {
      const result = await executeOperation(message.operationType, message.parameters || {});
      reply({ ok: true, ...result });
      return;
    }

    if (message.type === 'close') {
      if (context) {
        await context.close();
      }
      reply({ ok: true, data: { closed: true } });
      process.exit(0);
      return;
    }

    reply({ ok: false, error: `unsupported message type: ${message.type}` });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    reply({ ok: false, error: redactSecretInString(message, []) });
  }
});

rl.on('close', async () => {
  if (context) {
    await context.close().catch(() => {});
  }
  process.exit(0);
});
