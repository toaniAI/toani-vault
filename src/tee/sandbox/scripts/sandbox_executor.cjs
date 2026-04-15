const readline = require('readline');
const net = require('net');
const { spawn } = require('child_process');
const puppeteer = require('puppeteer-core');

let browser = null;
let page = null;
let lightpandaProcess = null;

function reply(payload) {
  process.stdout.write(`${JSON.stringify(payload)}\n`);
}

async function ensurePage() {
  if (!browser) {
    throw new Error('browser not initialized');
  }

  if (!page || page.isClosed()) {
    const pages = await browser.pages();
    page = pages[0] || (await browser.newPage());
  }

  return page;
}

async function allocateFreePort() {
  return await new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      server.close(error => {
        if (error) {
          reject(error);
          return;
        }
        resolve(address.port);
      });
    });
  });
}

async function waitForPort(port, timeoutMs = 5000) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const connected = await new Promise(resolve => {
      const socket = net.createConnection({ host: '127.0.0.1', port }, () => {
        socket.destroy();
        resolve(true);
      });
      socket.once('error', () => {
        socket.destroy();
        resolve(false);
      });
    });
    if (connected) {
      return;
    }
    if (Date.now() >= deadline) {
      throw new Error(`lightpanda cdp server did not open port ${port}`);
    }
    await new Promise(resolve => setTimeout(resolve, 100));
  }
}

async function startLightpanda(profileDir) {
  const executablePath = process.env.LIGHTPANDA_BINARY_PATH;
  if (!executablePath) {
    throw new Error('LIGHTPANDA_BINARY_PATH is required');
  }

  const port = await allocateFreePort();
  lightpandaProcess = spawn(
    executablePath,
    ['serve', '--host', '127.0.0.1', '--port', String(port)],
    {
      env: {
        ...process.env,
        LIGHTPANDA_DISABLE_TELEMETRY:
          process.env.LIGHTPANDA_DISABLE_TELEMETRY || 'true',
      },
      stdio: ['ignore', 'ignore', 'pipe'],
      cwd: profileDir,
    }
  );
  let stderr = '';
  lightpandaProcess.stderr.on('data', chunk => {
    stderr += chunk.toString();
  });
  lightpandaProcess.once('exit', code => {
    if (code !== null && code !== 0 && !browser) {
      reply({ ok: false, error: `lightpanda exited early: ${stderr}` });
    }
  });

  await waitForPort(port);
  browser = await puppeteer.connect({
    browserWSEndpoint: `ws://127.0.0.1:${port}`,
    protocolTimeout: 30000,
  });
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
      await currentPage.$eval(
        selector,
        (element, nextValue) => {
          if (!element) return;
          element.value = nextValue;
          element.dispatchEvent(new Event('input', { bubbles: true }));
          element.dispatchEvent(new Event('change', { bubbles: true }));
        },
        value
      );
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
        await new Promise(resolve => setTimeout(resolve, timeoutMs));
      }
      return true;
    },
    async getText(selector) {
      if (selectorLooksSensitive(selector)) {
        throw new Error(`get_text blocked for sensitive selector: ${selector}`);
      }
      return await currentPage.$eval(selector, element => {
        const tagName = element.tagName.toLowerCase();
        const inputType = (element.getAttribute('type') || '').toLowerCase();
        if ((tagName === 'input' || tagName === 'textarea') && inputType === 'password') {
          throw new Error('get_text blocked for password input');
        }
        if (tagName === 'input' || tagName === 'textarea') {
          return element.value || '';
        }
        return element.innerText || element.textContent || '';
      });
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
      await currentPage.waitForSelector(selector, { timeout: parameters?.timeout_ms ?? 30000 });
      await currentPage.$eval(
        selector,
        (element, nextValue) => {
          if (!element) return;
          element.value = nextValue;
          element.dispatchEvent(new Event('input', { bubbles: true }));
          element.dispatchEvent(new Event('change', { bubbles: true }));
        },
        value
      );
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
      await new Promise(resolve => setTimeout(resolve, durationMs));
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
      const text = await currentPage.$eval(selector, element => {
        const tagName = element.tagName.toLowerCase();
        const inputType = (element.getAttribute('type') || '').toLowerCase();
        if ((tagName === 'input' || tagName === 'textarea') && inputType === 'password') {
          throw new Error('get_text blocked for password input');
        }
        if (tagName === 'input' || tagName === 'textarea') {
          return element.value || '';
        }
        return element.innerText || element.textContent || '';
      });
      return { data: { text } };
    }
    case 'selector_metadata': {
      const selector = parameters?.selector;
      if (typeof selector !== 'string' || !selector) {
        throw new Error('selector_metadata requires selector');
      }
      const metadata = await currentPage
        .$eval(selector, element => {
          const tag = element.tagName.toLowerCase();
          const inputType = (element.getAttribute('type') || '').toLowerCase();
          const value =
            tag === 'input' || tag === 'textarea'
              ? element.value || ''
              : element.innerText || element.textContent || '';
          return { ok: true, tag, type: inputType, value };
        })
        .catch(() => ({ ok: false, reason: 'selector_not_found' }));
      return { data: metadata };
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
                .$eval(selector, element => element.innerText || element.textContent || '')
                .catch(() => '');
        rows.push({ selector, text });
      }
      return { data: { rows, final_url: currentPage.url() } };
    }
    case 'dom_export': {
      const rootSelector = parameters?.root_selector || parameters?.rootSelector || 'html';
      const format = parameters?.format || 'html';
      const includeText = parameters?.include_text ?? parameters?.includeText ?? true;
      const includeMetadata =
        parameters?.include_metadata ?? parameters?.includeMetadata ?? false;
      const maskSelectors = Array.isArray(parameters?.maskSelectors)
        ? parameters.maskSelectors
        : [];

      if (!['html', 'text', 'json'].includes(format)) {
        throw new Error(`unsupported dom_export format: ${format}`);
      }

      const content = await currentPage.evaluate(
        ({ rootSelector, format, includeText, includeMetadata, maskSelectors }) => {
          function redactDomText(value) {
            if (typeof value !== 'string') return value;
            const trimmed = value.trim();
            if (!trimmed) return '';
            const looksSecret =
              trimmed.startsWith('Bearer ') ||
              trimmed.startsWith('sk_') ||
              trimmed.startsWith('ghp_') ||
              trimmed.startsWith('eyJ') ||
              (trimmed.length >= 24 &&
                !/\s/.test(trimmed) &&
                /[0-9]/.test(trimmed) &&
                /[A-Za-z]/.test(trimmed));
            return looksSecret ? '********' : trimmed;
          }

          function domElementLooksSensitive(element) {
            if (!element || element.nodeType !== 1) return false;
            if (Array.isArray(maskSelectors)) {
              for (const selector of maskSelectors) {
                if (typeof selector !== 'string' || !selector) continue;
                try {
                  if (element.matches(selector) || element.closest(selector)) return true;
                } catch (_) {
                  continue;
                }
              }
            }

            const fields = ['id', 'name', 'class', 'autocomplete', 'aria-label', 'type'];
            const haystack = fields
              .map(name => element.getAttribute(name) || '')
              .join(' ')
              .toLowerCase();
            return /pass(word)?|passwd|secret|token|api[-_]?key|apikey|cookie|otp/.test(
              haystack
            );
          }

          function sanitizeDomAttributes(element, isSensitive) {
            for (const attribute of Array.from(element.attributes || [])) {
              if (
                isSensitive &&
                ['value', 'placeholder', 'title', 'aria-label'].includes(attribute.name)
              ) {
                element.setAttribute(attribute.name, '********');
                continue;
              }
              element.setAttribute(attribute.name, redactDomText(attribute.value));
            }

            if (element.tagName === 'INPUT' || element.tagName === 'TEXTAREA') {
              const value = isSensitive ? '********' : redactDomText(element.value || '');
              element.setAttribute('value', value);
            }
          }

          function sanitizeDomClone(root) {
            const clone = root.cloneNode(true);
            const walker = document.createTreeWalker(
              clone,
              NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT
            );
            const nodes = [clone];
            while (walker.nextNode()) nodes.push(walker.currentNode);

            for (const node of nodes) {
              if (node.nodeType === Node.TEXT_NODE) {
                const parentSensitive = domElementLooksSensitive(node.parentElement);
                node.nodeValue = parentSensitive ? '********' : redactDomText(node.nodeValue || '');
                continue;
              }
              if (node.nodeType === Node.ELEMENT_NODE) {
                const isSensitive = domElementLooksSensitive(node);
                sanitizeDomAttributes(node, isSensitive);
                if (isSensitive && node.children.length === 0 && node.textContent.trim()) {
                  node.textContent = '********';
                }
              }
            }

            return clone;
          }

          function serializeDomElement(element) {
            const isSensitive = domElementLooksSensitive(element);
            const result = { tag: element.tagName.toLowerCase() };

            if (includeMetadata) {
              result.attributes = {};
              for (const attribute of Array.from(element.attributes || [])) {
                result.attributes[attribute.name] = isSensitive
                  ? '********'
                  : redactDomText(attribute.value);
              }
            }

            if (includeText) {
              result.text = isSensitive ? '********' : redactDomText(element.innerText || '');
            }

            result.children = Array.from(element.children).map(serializeDomElement);
            return result;
          }

          const root = document.querySelector(rootSelector);
          if (!root) {
            throw new Error(`dom_export root not found: ${rootSelector}`);
          }

          if (format === 'json') {
            return serializeDomElement(root);
          }

          const clone = sanitizeDomClone(root);
          if (format === 'text') {
            return clone.innerText || clone.textContent || '';
          }
          return clone.outerHTML;
        },
        { rootSelector, format, includeText, includeMetadata, maskSelectors }
      );

      return {
        data: {
          root_selector: rootSelector,
          format,
          include_text: includeText,
          include_metadata: includeMetadata,
          content,
          url: currentPage.url(),
          title: await currentPage.title().catch(() => ''),
          truncated: false,
        },
      };
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
      await cleanupRuntime();
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

async function cleanupRuntime() {
  if (browser) {
    await browser.disconnect().catch(() => {});
    browser = null;
  }
  page = null;
  if (lightpandaProcess) {
    lightpandaProcess.kill('SIGTERM');
    lightpandaProcess = null;
  }
}

rl.on('line', async line => {
  try {
    const message = JSON.parse(line);

    if (message.type === 'init') {
      await startLightpanda(message.profileDir);
      page = (await browser.pages())[0] || (await browser.newPage());
      reply({ ok: true, data: { ready: true } });
      return;
    }

    if (message.type === 'execute') {
      const result = await executeOperation(message.operationType, message.parameters || {});
      reply({ ok: true, ...result });
      return;
    }

    if (message.type === 'close') {
      await cleanupRuntime();
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
  await cleanupRuntime();
  process.exit(0);
});
