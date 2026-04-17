const readline = require('readline');
const net = require('net');
const { spawn } = require('child_process');
const puppeteer = require('puppeteer-core');

let browser = null;
let browserContext = null;
let page = null;
let lightpandaProcess = null;

const LIGHTPANDA_CDP_IDLE_TIMEOUT_SECS_ENV = 'LIGHTPANDA_CDP_IDLE_TIMEOUT_SECS';
const DEFAULT_LIGHTPANDA_CDP_IDLE_TIMEOUT_SECS = 60;
const DEFAULT_BOOTSTRAP_SCRIPT_SELECTORS = [
  'script[src]',
  'script[src][type$="-text/javascript"]',
  'script[src][type="text/javascript"]',
  'script[src][type="application/javascript"]',
  'script[src][type="module"]',
  'script[src][defer]',
  'script[src][nomodule]',
  'script[src]:not([type])',
];
const DEFAULT_BOOTSTRAP_DISCOVERY_TIMEOUT_MS = 5000;
const DEFAULT_BOOTSTRAP_RESCAN_DELAY_MS = 250;
const DEFAULT_BOOTSTRAP_SAMPLE_SCRIPT_LIMIT = 5;

function reply(payload) {
  process.stdout.write(`${JSON.stringify(payload)}\n`);
}

async function ensurePage() {
  if (!browser) {
    throw new Error('browser not initialized');
  }

  if (!page || page.isClosed()) {
    await resetPageContext();
  }

  return page;
}

async function resetPageContext() {
  if (page && !page.isClosed()) {
    await page.close().catch(() => {});
  }
  page = null;

  if (browserContext) {
    await browserContext.close().catch(() => {});
  }
  browserContext = null;

  if (!browser) {
    throw new Error('browser not initialized');
  }

  browserContext = await browser.createBrowserContext();
  page = await browserContext.newPage();
  return page;
}

function isRecoverableFrameError(error) {
  const message = error instanceof Error ? error.message : String(error);
  return /Navigating frame was detached|detached Frame|BrowserContextNotLoaded|Target closed|Session closed/i.test(
    message
  );
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

function resolveLightpandaCdpIdleTimeoutSeconds() {
  const configured = process.env[LIGHTPANDA_CDP_IDLE_TIMEOUT_SECS_ENV];
  if (configured === undefined || configured.trim() === '') {
    return DEFAULT_LIGHTPANDA_CDP_IDLE_TIMEOUT_SECS;
  }

  const parsed = Number(configured);
  if (!Number.isFinite(parsed) || parsed < 1) {
    throw new Error(`${LIGHTPANDA_CDP_IDLE_TIMEOUT_SECS_ENV} must be a positive number of seconds`);
  }

  return Math.ceil(parsed);
}

async function startLightpanda(profileDir) {
  const executablePath = process.env.LIGHTPANDA_BINARY_PATH;
  if (!executablePath) {
    throw new Error('LIGHTPANDA_BINARY_PATH is required');
  }

  const port = await allocateFreePort();
  const idleTimeoutSeconds = resolveLightpandaCdpIdleTimeoutSeconds();
  lightpandaProcess = spawn(
    executablePath,
    [
      'serve',
      '--host', '127.0.0.1',
      '--port', String(port),
      '--timeout',
      String(idleTimeoutSeconds),
    ],
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
  const startupFailure = new Promise((_, reject) => {
    lightpandaProcess.once('error', error => {
      reject(error);
    });
    lightpandaProcess.once('exit', code => {
      if (code !== null && code !== 0 && !browser) {
        reject(new Error(`lightpanda exited early: ${stderr}`));
      }
    });
  });

  try {
    await Promise.race([waitForPort(port), startupFailure]);
    browser = await puppeteer.connect({
      browserWSEndpoint: `ws://127.0.0.1:${port}`,
      protocolTimeout: 30000,
    });
  } catch (error) {
    await stopLightpanda();
    throw error;
  }
}

async function waitForProcessExit(child, timeoutMs = 5000) {
  if (!child || child.exitCode !== null || child.signalCode !== null) {
    return;
  }

  await new Promise(resolve => {
    const timer = setTimeout(() => {
      child.kill('SIGKILL');
      resolve();
    }, timeoutMs);
    child.once('exit', () => {
      clearTimeout(timer);
      resolve();
    });
  });
}

async function stopLightpanda() {
  if (!lightpandaProcess) {
    return;
  }

  const child = lightpandaProcess;
  lightpandaProcess = null;
  if (child.exitCode === null && child.signalCode === null) {
    child.kill('SIGTERM');
  }
  await waitForProcessExit(child);
}

async function cleanupRuntime() {
  if (page && !page.isClosed()) {
    await page.close().catch(() => {});
  }
  page = null;
  if (browserContext) {
    await browserContext.close().catch(() => {});
    browserContext = null;
  }
  if (browser) {
    await browser.disconnect().catch(() => {});
    browser = null;
  }
  await stopLightpanda();
}

function selectorLooksSensitive(selector) {
  return typeof selector === 'string' && /pass(word)?|secret|token|otp/i.test(selector);
}

async function bootstrapPageOnCurrentPage(currentPage, parameters) {
  const mode = parameters?.mode;
  if (mode !== 'rocket_loader') {
    throw new Error('invalid_request: bootstrap_page mode must be rocket_loader');
  }

  const rawSelectors = parameters?.script_selectors;
  if (
    rawSelectors !== undefined &&
    (!Array.isArray(rawSelectors) || rawSelectors.some(selector => typeof selector !== 'string'))
  ) {
    throw new Error('invalid_request: script_selectors must be an array of strings');
  }

  if (parameters?.wait_selector !== undefined && typeof parameters.wait_selector !== 'string') {
    throw new Error('invalid_request: wait_selector must be a string');
  }

  if (
    parameters?.include_plain_scripts !== undefined &&
    typeof parameters.include_plain_scripts !== 'boolean'
  ) {
    throw new Error('invalid_request: include_plain_scripts must be a boolean');
  }

  const waitTimeoutMs = Number(parameters?.wait_timeout_ms ?? 30000);
  if (!Number.isInteger(waitTimeoutMs) || waitTimeoutMs <= 0) {
    throw new Error('invalid_request: wait_timeout_ms must be a positive integer');
  }

  const includePlainScripts = parameters?.include_plain_scripts === true;
  const replayLifecycleEvents = parameters?.replay_lifecycle_events === true;
  const scriptSelectors =
    rawSelectors && rawSelectors.length > 0
      ? rawSelectors
      : DEFAULT_BOOTSTRAP_SCRIPT_SELECTORS;
  const bootstrapDiscoveryTimeoutMs = Math.min(
    waitTimeoutMs,
    DEFAULT_BOOTSTRAP_DISCOVERY_TIMEOUT_MS
  );

  await currentPage.waitForFunction(() => !!document.body, {
    timeout: bootstrapDiscoveryTimeoutMs,
  });

  await currentPage
    .waitForFunction(() => document.readyState !== 'loading', {
      timeout: Math.max(1, Math.floor(bootstrapDiscoveryTimeoutMs / 2)),
    })
    .catch(() => {});

  async function discoverScripts() {
    return currentPage.evaluate(
      ({ scriptSelectors, includePlainScripts, sampleLimit }) => {
        function normalizeType(typeValue) {
          if (typeof typeValue !== 'string') {
            return '';
          }
          return typeValue.trim().toLowerCase();
        }

        function isRocketLoaderScript(typeValue) {
          const normalized = normalizeType(typeValue);
          return normalized.endsWith('-text/javascript') && normalized !== 'text/javascript';
        }

        function isPlainExecutableType(typeValue) {
          const normalized = normalizeType(typeValue);
          return (
            normalized === '' ||
            normalized === 'text/javascript' ||
            normalized === 'application/javascript' ||
            normalized === 'module'
          );
        }

        function resolveInjectedType(typeValue, rocketLoader) {
          if (rocketLoader) {
            return 'text/javascript';
          }
          const normalized = normalizeType(typeValue);
          return normalized || null;
        }

        function resolveScriptSrc(node) {
          const attributeSrc = node.getAttribute('src');
          const normalizedAttributeSrc =
            typeof attributeSrc === 'string' ? attributeSrc.trim() : '';
          if (normalizedAttributeSrc) {
            try {
              return new URL(normalizedAttributeSrc, document.baseURI).href;
            } catch (error) {
              return normalizedAttributeSrc;
            }
          }

          if (typeof node.src === 'string' && node.src.trim()) {
            return node.src.trim();
          }

          return '';
        }

        function matchSelectors(node) {
          const matches = [];
          for (const selector of scriptSelectors) {
            if (typeof selector !== 'string' || !selector.trim()) {
              continue;
            }
            try {
              if (node.matches(selector.trim())) {
                matches.push(selector.trim());
              }
            } catch (error) {
              throw new Error(`invalid_request: invalid script selector ${selector}`);
            }
          }
          return matches;
        }

        const descriptors = [];
        const sampleScriptDescriptors = [];
        const matchedSelectors = new Set();
        let nextIndex = 0;

        for (const node of document.querySelectorAll('script')) {
          if (!(node instanceof HTMLScriptElement)) {
            continue;
          }

          const resolvedSrc = resolveScriptSrc(node);
          if (!resolvedSrc) {
            continue;
          }

          const nodeMatchedSelectors = matchSelectors(node);
          if (nodeMatchedSelectors.length === 0) {
            continue;
          }
          for (const selector of nodeMatchedSelectors) {
            matchedSelectors.add(selector);
          }

          const originalType = node.getAttribute('type') || '';
          const rocketLoader = isRocketLoaderScript(originalType);
          const plainExecutable = isPlainExecutableType(originalType);
          const included = rocketLoader || (includePlainScripts && plainExecutable);

          if (sampleScriptDescriptors.length < sampleLimit) {
            sampleScriptDescriptors.push({
              src: resolvedSrc,
              originalType,
              injectedType: resolveInjectedType(originalType, rocketLoader),
              rocketLoader,
              plainExecutable,
              included,
              matchedSelectors: nodeMatchedSelectors,
              defer: node.defer === true,
              noModule: node.noModule === true,
              crossOrigin: node.getAttribute('crossorigin'),
              referrerPolicy: node.getAttribute('referrerpolicy'),
            });
          }

          if (!included) {
            continue;
          }

          const marker =
            node.getAttribute('data-credbridge-bootstrap-id') || `credbridge-bootstrap-${nextIndex++}`;
          node.setAttribute('data-credbridge-bootstrap-id', marker);

          descriptors.push({
            marker,
            src: resolvedSrc,
            originalType,
            injectedType: resolveInjectedType(originalType, rocketLoader),
            rocketLoader,
            plainExecutable,
            matchedSelectors: nodeMatchedSelectors,
            async: node.async === true,
            defer: node.defer === true,
            noModule: node.noModule === true,
            crossOrigin: node.getAttribute('crossorigin'),
            referrerPolicy: node.getAttribute('referrerpolicy'),
          });
        }

        return {
          matchedSelectors: Array.from(matchedSelectors),
          descriptors,
          sampleScriptDescriptors,
        };
      },
      {
        scriptSelectors,
        includePlainScripts,
        sampleLimit: DEFAULT_BOOTSTRAP_SAMPLE_SCRIPT_LIMIT,
      }
    );
  }

  const readyStateBeforeScan = await currentPage.evaluate(() => document.readyState);
  let scriptPlan = await discoverScripts();
  if (scriptPlan.descriptors.length === 0) {
    await new Promise(resolve => setTimeout(resolve, DEFAULT_BOOTSTRAP_RESCAN_DELAY_MS));
    scriptPlan = await discoverScripts();
  }

  const injectedScripts = [];
  for (const descriptor of scriptPlan.descriptors) {
    const result = await currentPage.evaluate(async currentDescriptor => {
      const original = document.querySelector(
        `script[data-credbridge-bootstrap-id="${currentDescriptor.marker}"]`
      );
      if (!(original instanceof HTMLScriptElement)) {
        return { skipped: true, reason: 'original_script_missing', src: currentDescriptor.src };
      }

      const parent = original.parentNode;
      if (!parent) {
        return { skipped: true, reason: 'original_parent_missing', src: currentDescriptor.src };
      }

      const nextSibling = original.nextSibling;
      const injected = document.createElement('script');
      injected.src = currentDescriptor.src;
      if (typeof currentDescriptor.injectedType === 'string' && currentDescriptor.injectedType) {
        injected.type = currentDescriptor.injectedType;
      } else {
        injected.removeAttribute('type');
      }
      injected.async = currentDescriptor.async === true;
      injected.defer = currentDescriptor.defer === true;
      injected.noModule = currentDescriptor.noModule === true;
      if (typeof currentDescriptor.crossOrigin === 'string') {
        injected.crossOrigin = currentDescriptor.crossOrigin;
      } else {
        injected.removeAttribute('crossorigin');
      }
      if (typeof currentDescriptor.referrerPolicy === 'string') {
        injected.referrerPolicy = currentDescriptor.referrerPolicy;
      } else {
        injected.removeAttribute('referrerpolicy');
      }
      injected.setAttribute('data-credbridge-bootstrap-source', currentDescriptor.marker);

      await new Promise((resolve, reject) => {
        injected.addEventListener('load', () => resolve(), { once: true });
        injected.addEventListener(
          'error',
          () =>
            reject(new Error(`bootstrap_failed: failed_to_load_script:${currentDescriptor.src}`)),
          { once: true }
        );

        if (nextSibling) {
          parent.insertBefore(injected, nextSibling);
        } else {
          parent.appendChild(injected);
        }
      });

      return { skipped: false, src: currentDescriptor.src };
    }, descriptor);

    if (!result.skipped) {
      injectedScripts.push(result.src);
    }
  }

  if (replayLifecycleEvents) {
    await currentPage.evaluate(() => {
      document.dispatchEvent(new Event('readystatechange'));
      document.dispatchEvent(new Event('DOMContentLoaded', { bubbles: true, cancelable: true }));
      window.dispatchEvent(new Event('load'));
      window.dispatchEvent(new Event('pageshow'));
    });
  }

  const readyStateAfterInjection = await currentPage.evaluate(() => document.readyState);
  let waitSatisfied = true;
  const waitSelector = parameters?.wait_selector;
  if (typeof waitSelector === 'string' && waitSelector.trim()) {
    try {
      await currentPage.waitForSelector(waitSelector, { timeout: waitTimeoutMs });
    } catch (error) {
      const pageDiagnostics = await currentPage.evaluate(selector => {
        return {
          readyState: document.readyState,
          bodyPresent: !!document.body,
          selectorExists: !!document.querySelector(selector),
        };
      }, waitSelector.trim());
      const title = await currentPage.title().catch(() => '');
      throw new Error(
        `bootstrap_failed: selector_not_found: ${waitSelector.trim()} ` +
          `(url=${currentPage.url()}, title=${JSON.stringify(title)}, discovered_scripts=${scriptPlan.descriptors.length}, reinjected_scripts=${injectedScripts.length}, replay_lifecycle_events=${replayLifecycleEvents}, ready_state_before_scan=${readyStateBeforeScan}, ready_state_after_injection=${readyStateAfterInjection}, ready_state=${pageDiagnostics.readyState}, body_present=${pageDiagnostics.bodyPresent}, matched_selectors=${JSON.stringify(scriptPlan.matchedSelectors)}, sample_script_descriptors=${JSON.stringify(scriptPlan.sampleScriptDescriptors)}, selector_exists_at_failure=${pageDiagnostics.selectorExists})`
      );
    }
  }

  return {
    data: {
      injected_scripts: injectedScripts,
      final_url: currentPage.url(),
      title: await currentPage.title().catch(() => ''),
      wait_satisfied: waitSatisfied,
      diagnostics: {
        mode,
        selectors: scriptSelectors,
        matched_selectors: scriptPlan.matchedSelectors,
        ready_state_before_scan: readyStateBeforeScan,
        ready_state_after_injection: readyStateAfterInjection,
        discovered_scripts: scriptPlan.descriptors.length,
        reinjected_scripts: injectedScripts.length,
        sample_script_descriptors: scriptPlan.sampleScriptDescriptors,
        selector_exists_at_failure: null,
        include_plain_scripts: includePlainScripts,
        replay_lifecycle_events: replayLifecycleEvents,
      },
    },
  };
}

async function executeOperationOnPage(currentPage, operationType, parameters) {
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
          function eventWithFallback(eventName, options) {
            try {
              return new InputEvent(eventName, options);
            } catch (_) {
              return new Event(eventName, { bubbles: true });
            }
          }

          function setInputValue(element, nextValue) {
            if (!element) return;

            const tagName = element.tagName.toLowerCase();
            if (tagName !== 'input' && tagName !== 'textarea') {
              element.textContent = nextValue;
              element.dispatchEvent(
                eventWithFallback('input', { bubbles: true, inputType: 'insertText' })
              );
              element.dispatchEvent(new Event('change', { bubbles: true }));
              return;
            }

            const prototype =
              tagName === 'textarea' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
            const valueSetter = Object.getOwnPropertyDescriptor(prototype, 'value')?.set;

            element.focus();
            if (valueSetter) {
              valueSetter.call(element, nextValue);
            } else {
              element.value = nextValue;
            }
            element.dispatchEvent(
              eventWithFallback('input', {
                bubbles: true,
                composed: true,
                inputType: 'insertText',
                data: nextValue,
              })
            );
            element.dispatchEvent(new Event('change', { bubbles: true }));
          }

          setInputValue(element, nextValue);
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

      const rawBindings = parameters?.bindings;
      if (
        rawBindings !== undefined &&
        (rawBindings === null || Array.isArray(rawBindings) || typeof rawBindings !== 'object')
      ) {
        throw new Error('invalid_request: execute_script bindings must be plain strings');
      }

      const bindings = rawBindings || {};
      for (const value of Object.values(bindings)) {
        if (typeof value !== 'string') {
          throw new Error('invalid_request: execute_script bindings must be plain strings');
        }
      }

      let result;
      try {
        result = await currentPage.evaluate(
          async ({ script, bindings }) => {
            function selectorLooksSensitiveInPage(selector) {
              return typeof selector === 'string' && /pass(word)?|secret|token|otp/i.test(selector);
            }

            function waitForSelectorInPage(selector, timeoutMs = 30000) {
              return new Promise((resolve, reject) => {
                if (typeof selector !== 'string' || !selector) {
                  resolve(null);
                  return;
                }

                const existing = document.querySelector(selector);
                if (existing) {
                  resolve(existing);
                  return;
                }

                const timeout = setTimeout(() => {
                  observer.disconnect();
                  reject(new Error(`selector_not_found: ${selector}`));
                }, timeoutMs);

                const observer = new MutationObserver(() => {
                  const element = document.querySelector(selector);
                  if (!element) {
                    return;
                  }
                  clearTimeout(timeout);
                  observer.disconnect();
                  resolve(element);
                });

                observer.observe(document.documentElement || document, {
                  childList: true,
                  subtree: true,
                });
              });
            }

            const credbridge = {
              async click(selector) {
                const element = await waitForSelectorInPage(selector);
                element.click();
                return true;
              },
              async waitFor(selector, timeoutMs = 30000) {
                if (typeof selector === 'string' && selector) {
                  await waitForSelectorInPage(selector, timeoutMs);
                } else {
                  await new Promise(resolve => setTimeout(resolve, timeoutMs));
                }
                return true;
              },
              async getText(selector) {
                if (selectorLooksSensitiveInPage(selector)) {
                  throw new Error(`get_text blocked for sensitive selector: ${selector}`);
                }
                const element = await waitForSelectorInPage(selector);
                const tagName = element.tagName.toLowerCase();
                const inputType = (element.getAttribute('type') || '').toLowerCase();
                if ((tagName === 'input' || tagName === 'textarea') && inputType === 'password') {
                  throw new Error('get_text blocked for password input');
                }
                if (tagName === 'input' || tagName === 'textarea') {
                  return element.value || '';
                }
                return element.innerText || element.textContent || '';
              },
              async navigate(url) {
                window.location.href = url;
                return true;
              },
            };

            const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
            return await new AsyncFunction('credbridge', `"use strict"; ${script}`)(credbridge);
          },
          { script, bindings }
        );
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        throw new Error(message);
      }

      return {
        data: {
          result,
          used_credentials: Object.keys(bindings),
        },
      };
    }
    case 'bootstrap_page': {
      return bootstrapPageOnCurrentPage(currentPage, parameters);
    }
    case 'close_runtime': {
      await cleanupRuntime();
      return { data: { closed: true } };
    }
    default:
      throw new Error(`unsupported operation: ${operationType}`);
  }
}

async function executeOperation(operationType, parameters) {
  for (let attempt = 0; attempt < 2; attempt += 1) {
    const currentPage = await ensurePage();
    try {
      return await executeOperationOnPage(currentPage, operationType, parameters);
    } catch (error) {
      if (attempt === 0 && isRecoverableFrameError(error)) {
        await resetPageContext();
        continue;
      }
      throw error;
    }
  }

  throw new Error('operation failed after browser page reset');
}

const rl = readline.createInterface({
  input: process.stdin,
  crlfDelay: Infinity,
});

rl.on('line', async line => {
  try {
    const message = JSON.parse(line);

    if (message.type === 'init') {
      await startLightpanda(message.profileDir);
      await ensurePage();
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
    reply({ ok: false, error: message });
  }
});

rl.on('close', async () => {
  await cleanupRuntime();
  process.exit(0);
});
