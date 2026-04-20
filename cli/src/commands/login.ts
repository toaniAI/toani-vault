import {
  confirm,
  intro,
  isCancel,
  log,
  note,
  outro,
  password,
  select,
  spinner,
} from "@clack/prompts";
import clipboard from "clipboardy";
import { readFileSync } from "node:fs";
import readline from "node:readline";
import { join } from "node:path";
import open from "open";
import pc from "picocolors";
import type { CliConfig } from "../types/cli.js";
import {
  BRAND,
  BRAND_SPINNER_FRAMES,
  hex,
  LOGO_LINES,
  WAVE_CHARS,
} from "../lib/brand.js";
import { readTokenFromEnv } from "../lib/env.js";
import { keychain } from "../lib/keychain.js";
import {
  getDefaultSkillInstallChoice,
  getSkillInstallTargetLabel,
  installBundledSkill,
  type SkillInstallChoice,
} from "../lib/skill-installer.js";
import {
  DASHBOARD_BASE_URL,
  DASHBOARD_CREDENTIALS_URL,
  DASHBOARD_LOGIN_URL,
  DASHBOARD_TOKENS_URL,
  DEFAULT_API_BASE_URL,
  isPasetoToken,
  validateToken,
  type ValidationResult,
} from "../lib/validate.js";
import { parseOptions } from "./common.js";

const brand = (text: string) => hex(BRAND.primary, text);
const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

function heartbeat(): string {
  const now = Date.now() % 1200;
  const phase = now < 600 ? now / 600 : (1200 - now) / 600;
  return phase > 0.5 ? brand("●") : hex(BRAND.hint, "●");
}

function wave(offset: number): string {
  let output = "";
  for (let index = 0; index < 12; index += 1) {
    output += WAVE_CHARS[(offset + index) % WAVE_CHARS.length];
  }
  return brand(output);
}

function brandSpinner() {
  return spinner({
    indicator: "dots",
    frames: BRAND_SPINNER_FRAMES.map((frame) => brand(frame)),
    delay: 150,
  });
}

async function printLogo(): Promise<void> {
  const wordmarkRow = 6;
  const taglineRow = 7;
  const wordmark = "TOANI  VAULT";
  const tagline = "secrets, guarded.";
  const rendered = new Array<string | null>(LOGO_LINES.length).fill(null);

  const redraw = () => {
    process.stdout.write(`\x1b[${LOGO_LINES.length}A\r`);
    for (let index = 0; index < LOGO_LINES.length; index += 1) {
      process.stdout.write("\x1b[2K");
      process.stdout.write((rendered[index] ?? "") + "\n");
    }
  };

  console.log();
  for (let index = 0; index < LOGO_LINES.length; index += 1) {
    process.stdout.write("\n");
  }

  for (let index = 0; index < LOGO_LINES.length; index += 1) {
    rendered[index] = `  ${hex(BRAND.primary, LOGO_LINES[index])}`;
    redraw();
    await sleep(45);
  }

  for (let index = 0; index <= wordmark.length; index += 1) {
    rendered[wordmarkRow] = `  ${hex(BRAND.primary, LOGO_LINES[wordmarkRow])}   ${pc.bold(wordmark.slice(0, index))}`;
    redraw();
    await sleep(35);
  }

  for (let index = 0; index <= tagline.length; index += 1) {
    rendered[taglineRow] = `  ${hex(BRAND.primary, LOGO_LINES[taglineRow])}   ${pc.dim(tagline.slice(0, index))}`;
    redraw();
    await sleep(35);
  }

  console.log();
}

async function logoFlash(): Promise<void> {
  const flashColors = [
    BRAND.highlight,
    BRAND.secondary,
    BRAND.primary,
    BRAND.highlight,
    BRAND.primary,
  ];

  console.log();
  for (let index = 0; index < LOGO_LINES.length; index += 1) {
    process.stdout.write("\n");
  }

  for (const color of flashColors) {
    process.stdout.write(`\x1b[${LOGO_LINES.length}A`);
    for (const line of LOGO_LINES) {
      process.stdout.write(`\x1b[2K  ${hex(color, line)}\n`);
    }
    await sleep(120);
  }

  process.stdout.write(`\x1b[${LOGO_LINES.length}A`);
  for (let index = 0; index < LOGO_LINES.length; index += 1) {
    process.stdout.write("\x1b[2K\n");
  }
  process.stdout.write(`\x1b[${LOGO_LINES.length}A`);
}

async function confetti(durationMs = 1200): Promise<void> {
  const chars = ["·", "✱", "*", "✦", "✧", "⋆", "✳"];
  const colors = [BRAND.primary, BRAND.secondary, BRAND.hint, BRAND.highlight];
  const width = Math.min(process.stdout.columns || 60, 60);
  const rows = 6;
  const columns = Array.from({ length: width }, () => ({
    startAt: Math.random() * (durationMs - 400),
    char: chars[Math.floor(Math.random() * chars.length)],
    color: colors[Math.floor(Math.random() * colors.length)],
  }));
  const startedAt = Date.now();

  for (let index = 0; index < rows; index += 1) {
    process.stdout.write("\n");
  }
  process.stdout.write(`\x1b[${rows}A`);

  while (Date.now() - startedAt < durationMs) {
    const elapsed = Date.now() - startedAt;
    let frame = "";
    for (let row = 0; row < rows; row += 1) {
      let line = "";
      for (let column = 0; column < width; column += 1) {
        const current = columns[column];
        const columnElapsed = elapsed - current.startAt;
        if (columnElapsed < 0) {
          line += " ";
          continue;
        }

        const position = Math.floor(columnElapsed / 80);
        line += position === row ? hex(current.color, current.char) : " ";
      }
      frame += `${line}\n`;
    }

    process.stdout.write(frame);
    process.stdout.write(`\x1b[${rows}A`);
    await sleep(60);
  }

  for (let row = 0; row < rows; row += 1) {
    process.stdout.write("\x1b[2K\n");
  }
  process.stdout.write(`\x1b[${rows}A`);
}

function cancel(): never {
  outro(pc.dim("Cancelled. Run `toani login` to try again."));
  process.exit(0);
}

function handleValidationError(result: ValidationResult, baseUrl: string): void {
  if (result.reason === "invalid_or_expired") {
    note(
      `The token was rejected by the server (HTTP 401).\n\nPossible causes:\n  • Token expired\n  • Token revoked by your admin\n  • Wrong / partial token\n\n${pc.bold("Fix:")} generate a new token at\n  ${DASHBOARD_TOKENS_URL}\n\nThen run ${pc.green("toani login")} again.`,
      pc.red("Token invalid"),
    );
  } else if (result.reason === "insufficient_scope") {
    const required = String(result.body?.required_scope ?? "<unknown>");
    const current = Array.isArray(result.body?.current_scopes)
      ? (result.body?.current_scopes as unknown[]).join(", ")
      : "<unknown>";
    note(
      `Token is valid but missing required scope (HTTP 403).\n\n  Required:  ${required}\n  Current:   ${current}\n\n${pc.bold("Fix:")} re-issue with broader scope at\n  ${DASHBOARD_TOKENS_URL}`,
      pc.yellow("Insufficient scope"),
    );
  } else if (result.reason === "dns") {
    note(
      `Cannot resolve hostname.\n\n  Base URL: ${baseUrl}\n\nPossible causes:\n  • Wrong URL (typo)\n  • No internet\n  • Behind firewall\n\nTry:  ${pc.green(`curl ${baseUrl}`)}`,
      pc.red("DNS error"),
    );
  } else if (result.reason === "refused") {
    note(
      `Connection refused.\n\n  Base URL: ${baseUrl}\n\n  • If using local dev: start the backend first\n  • If using staging: check VPN`,
      pc.red("Connection refused"),
    );
  } else if (result.reason === "timeout") {
    note(
      `Server didn't respond in 8 seconds.\n\n  Base URL: ${baseUrl}\n\n  • Slow network or VPN issue\n  • Backend overloaded`,
      pc.red("Timeout"),
    );
  } else {
    note(`HTTP ${result.status} from ${baseUrl}`, pc.red("Unexpected response"));
  }

  outro(pc.dim("Login incomplete."));
}

async function processToken(
  token: string,
  baseUrl: string,
  skipValidate: boolean,
): Promise<void> {
  if (!skipValidate) {
    const status = brandSpinner();
    status.start(`${wave(0)}  Validating against ${pc.dim(baseUrl)}...`);

    let offset = 0;
    const timer = setInterval(() => {
      offset = (offset + 1) % WAVE_CHARS.length;
      status.message(`${wave(offset)}  Validating against ${pc.dim(baseUrl)}...`);
    }, 80);

    const result = await validateToken(baseUrl, token);
    clearInterval(timer);

    if (!result.ok) {
      status.stop(pc.red(`✗ Validation failed (${result.reason})`));
      handleValidationError(result, baseUrl);
      return;
    }

    status.stop(pc.green("✓ Token valid"));
  } else {
    log.info(pc.dim("(--skip-validate flag set, skipping API check)"));
  }

  const saving = brandSpinner();
  saving.start("Saving to OS Keychain...");

  try {
    keychain.set(token);
    saving.stop(
      `${pc.green("✓ Saved")}${pc.dim(" (service: toani-vault-cli, account: default)")}`,
    );
  } catch (error) {
    const rendered = error instanceof Error ? error.message : String(error);
    saving.stop(pc.yellow(`⚠ Could not write to keychain: ${rendered}`));
    log.warn(
      pc.dim(
        "Token was not persisted. Re-run `toani login` when keychain access is available or use TOANI_VAULT_TOKEN explicitly.",
      ),
    );
  }

  await maybeInstallBundledSkill();
  await logoFlash();
  await confetti(1200);

  outro(`${pc.green(pc.bold("🎉 Connected!"))}\n\n  ${pc.dim("Storage:")}  OS Keychain (encrypted by macOS / libsecret / Windows Credential Manager)\n  ${pc.dim("API:")}      ${baseUrl}\n\n  ${pc.bold("What's next?")}\n    ${pc.dim("$")} ${pc.green("toani sandbox stats")}          ${pc.dim("— test connectivity")}\n    ${pc.dim("$")} ${pc.green("toani sandbox create-session")} ${pc.dim('--service-id <id> --original-intent "..."')}\n    ${pc.dim("$")} ${pc.green("toani --help")}                  ${pc.dim("— see all commands")}\n\n  ${pc.dim("Token expired? Just run `toani login` again.")}`);
}

async function maybeInstallBundledSkill(): Promise<void> {
  const installChoice = await select({
    message: "Install the bundled Toani CLI skill for your coding agent?",
    options: [
      {
        value: "codex",
        label: "Install for Codex",
        hint: "~/.codex/skills/toani-vault-cli",
      },
      {
        value: "claude",
        label: "Install for Claude Code",
        hint: "~/.claude/skills/toani-vault-cli",
      },
      {
        value: "both",
        label: "Install for both",
        hint: "write both user-level skill dirs",
      },
      {
        value: "skip",
        label: "Skip for now",
        hint: "login stays complete",
      },
    ],
    initialValue: getDefaultSkillInstallChoice(),
  });

  if (
    isCancel(installChoice) ||
    installChoice === "skip" ||
    installChoice === undefined
  ) {
    log.info(pc.dim("Skipped agent skill install."));
    return;
  }

  const status = brandSpinner();
  status.start("Installing bundled agent skill...");

  try {
    const report = installBundledSkill(
      installChoice as Exclude<SkillInstallChoice, "skip">,
    );

    if (report.outcomes.length > 0 && report.failures.length === 0) {
      status.stop(pc.green("✓ Agent skill installed"));
    } else if (report.outcomes.length > 0) {
      status.stop(pc.yellow("⚠ Agent skill installed with warnings"));
    } else {
      status.stop(pc.yellow("⚠ Agent skill not installed"));
    }

    for (const outcome of report.outcomes) {
      const verb = outcome.status === "updated" ? "Updated" : "Installed";
      log.success(
        `${verb} ${getSkillInstallTargetLabel(outcome.target)} skill at ${pc.cyan(outcome.destinationFile)}`,
      );
    }

    for (const failure of report.failures) {
      log.warn(
        `${getSkillInstallTargetLabel(failure.target)} skill install failed: ${failure.message}`,
      );
    }
  } catch (error) {
    const rendered = error instanceof Error ? error.message : String(error);
    status.stop(pc.yellow("⚠ Agent skill install failed"));
    log.warn(pc.dim(rendered));
  }
}

async function manualPaste(
  baseUrl: string,
  skipValidate: boolean,
): Promise<void> {
  await requestTokenInput(baseUrl, skipValidate);
}

async function requestTokenInput(
  baseUrl: string,
  skipValidate: boolean,
  message = "How do you want to provide the token?",
): Promise<void> {
  const envToken = readTokenFromEnv();
  if (envToken && isPasetoToken(envToken.token)) {
    log.success(
      `Found token in ${pc.cyan(envToken.source)} ${pc.dim(`(${envToken.token.length} chars)`)}`,
    );
    await processToken(envToken.token, baseUrl, skipValidate);
    return;
  }

  const how = await select({
    message,
    options: [
      { value: "paste", label: "Paste it here now", hint: "masked input" },
      {
        value: "env",
        label: "Set TOANI_VAULT_TOKEN in .env",
        hint: "edit file, come back",
      },
      { value: "cancel", label: "Cancel" },
    ],
    initialValue: "paste",
  });

  if (isCancel(how) || how === "cancel") {
    cancel();
  }

  if (how === "env") {
    note(
      `Add this line to ${pc.cyan(".env")} (in your current directory):\n\n  ${pc.green('TOANI_VAULT_TOKEN="v4.local.your-token-here..."')}\n\nSave the file, then come back here.`,
      brand("Configure .env"),
      { format: (value) => value },
    );

    for (let attempt = 0; attempt < 3; attempt += 1) {
      const ready = await confirm({
        message:
          attempt === 0
            ? "I've added it — re-check .env now?"
            : `Still not found. Tried ${attempt} time${attempt > 1 ? "s" : ""}. Check again?`,
        initialValue: true,
      });

      if (!ready || isCancel(ready)) {
        cancel();
      }

      const retry = readTokenFromEnv();
      if (retry && isPasetoToken(retry.token)) {
        log.success(
          `Found token in ${pc.cyan(retry.source)} ${pc.dim(`(${retry.token.length} chars)`)}`,
        );
        await processToken(retry.token, baseUrl, skipValidate);
        return;
      }

      log.warn(
        `Didn't find ${pc.cyan("TOANI_VAULT_TOKEN")} in .env (or it's not a PASETO v4.local token).`,
      );
    }

    outro(pc.dim("Too many retries. Check .env format then rerun `toani login`."));
    return;
  }

  const token = await password({
    message: "Paste your bearer token here:",
    validate: (value) => {
      if (!value) {
        return "Token is required";
      }
      if (!isPasetoToken(value.trim())) {
        return "Not a valid PASETO token (should start with v4.local. or v4.public.)";
      }
      return;
    },
  });

  if (isCancel(token)) {
    cancel();
  }

  await processToken(token.trim(), baseUrl, skipValidate);
}

async function waitForClipboard(
  baseUrl: string,
  skipValidate: boolean,
): Promise<void> {
  let lastSeen = "";
  try {
    lastSeen = await clipboard.read();
  } catch {
    lastSeen = "";
  }

  const status = brandSpinner();
  status.start(
    `${heartbeat()} Watching clipboard...  ${pc.dim("(or press P to paste manually,  Q to quit)")}`,
  );

  let pasteRequested = false;
  let cancelRequested = false;
  const isTTY = process.stdin.isTTY === true;
  const onKeypress = (_input: string, key: readline.Key | undefined) => {
    if (!key) {
      return;
    }
    if ((key.ctrl && key.name === "c") || key.name === "q") {
      cancelRequested = true;
    } else if (key.name === "p") {
      pasteRequested = true;
    }
  };

  if (isTTY) {
    readline.emitKeypressEvents(process.stdin);
    process.stdin.setRawMode(true);
    process.stdin.resume();
    process.stdin.on("keypress", onKeypress);
  }

  const cleanup = () => {
    if (isTTY) {
      try {
        process.stdin.setRawMode(false);
      } catch {
        // Ignore raw mode reset failures.
      }
      process.stdin.pause();
      process.stdin.removeListener("keypress", onKeypress);
    }
  };

  const startedAt = Date.now();
  const timeoutMs = 5 * 60 * 1000;
  let pulseFrame = -1;

  try {
    while (true) {
      if (cancelRequested) {
        status.stop(pc.dim("Cancelled."));
        cleanup();
        outro(pc.dim("Run `toani login` to try again."));
        return;
      }

      if (pasteRequested) {
        status.stop(pc.cyan("Switching to manual paste..."));
        cleanup();
        await manualPaste(baseUrl, skipValidate);
        return;
      }

      let current = "";
      try {
        current = await clipboard.read();
      } catch {
        current = "";
      }

      if (current && current !== lastSeen && isPasetoToken(current.trim())) {
        const token = current.trim();
        status.stop(
          `✓ Token detected from clipboard ${pc.dim(`(PASETO v4.local, ${token.length} chars)`)}`,
        );
        cleanup();
        await processToken(token, baseUrl, skipValidate);
        return;
      }

      if (current && current !== lastSeen && !isPasetoToken(current.trim())) {
        const preview = current.slice(0, 30).replace(/\n/g, " ");
        status.message(
          `Watching clipboard...  ${pc.yellow(`(saw "${preview}..." — not a token)`)}  ${pc.dim("· P=paste · Q=quit")}`,
        );
      }
      lastSeen = current;

      const elapsedMs = Date.now() - startedAt;
      const elapsedSeconds = Math.floor(elapsedMs / 1000);
      const currentFrame = Math.floor(elapsedMs / 300);
      if (currentFrame !== pulseFrame) {
        pulseFrame = currentFrame;
        const remaining = Math.max(0, Math.ceil((timeoutMs - elapsedMs) / 1000));
        const mins = Math.floor(remaining / 60);
        const secs = remaining % 60;
        const timeLeft = `${mins}:${String(secs).padStart(2, "0")}`;
        const indicator = heartbeat();

        if (elapsedSeconds >= 30 && elapsedSeconds < 31) {
          status.message(
            `${indicator} Still watching...  ${pc.dim(`did you click [📋 Copy]?  · ${timeLeft} left  · P=paste · Q=quit`)}`,
          );
        } else if (elapsedSeconds > 0 && elapsedSeconds % 30 === 0) {
          status.message(
            `${indicator} Watching clipboard...  ${pc.dim(`${timeLeft} left  · P=paste · Q=quit`)}`,
          );
        } else if (elapsedSeconds < 30) {
          status.message(
            `${indicator} Watching clipboard...  ${pc.dim(`(P=paste · Q=quit · ${timeLeft} left)`)}`,
          );
        }
      }

      if (elapsedMs > timeoutMs) {
        status.stop(pc.yellow("⏱  Timeout — no token in 5 minutes"));
        cleanup();
        outro(pc.dim("Run `toani login` to try again."));
        return;
      }

      await sleep(500);
    }
  } finally {
    cleanup();
  }
}

async function pasteFromClipboard(
  baseUrl: string,
  skipValidate: boolean,
): Promise<void> {
  const envToken = readTokenFromEnv();
  if (envToken && isPasetoToken(envToken.token)) {
    log.success(
      `Found token in ${pc.cyan(envToken.source)} ${pc.dim(`(${envToken.token.length} chars)`)}`,
    );
    await processToken(envToken.token, baseUrl, skipValidate);
    return;
  }

  let current = "";
  try {
    current = (await clipboard.read()).trim();
  } catch {
    current = "";
  }

  if (isPasetoToken(current)) {
    log.success(`Found token in clipboard ${pc.dim(`(${current.length} chars)`)}`);
    await processToken(current, baseUrl, skipValidate);
    return;
  }

  await requestTokenInput(baseUrl, skipValidate);
}

async function guidedSetup(baseUrl: string, skipValidate: boolean): Promise<void> {
  const hasCredential = await select({
    message: "Do you already have a credential set up?",
    options: [
      { value: "no", label: "No, walk me through it", hint: "~30s in browser" },
      { value: "yes", label: "Yes, skip to token", hint: "go straight to /tokens" },
      { value: "unsure", label: "Not sure — what's a credential?" },
    ],
  });

  if (isCancel(hasCredential)) {
    cancel();
  }

  if (hasCredential === "unsure") {
    note(
      `A ${pc.green("credential")} is the actual secret you want Toani to safeguard\n${pc.cyan("— for example, your Gmail username + password.")}\n\nA ${pc.green("token")} is what this CLI uses to access that credential\n${pc.cyan("— short-lived, scope-limited, revocable.")}\n\nYou need to ${pc.bold("create a credential first")}, then ${pc.bold("generate a token")} for it.`,
      pc.cyan("Credential vs token"),
      { format: (value) => value },
    );
  }

  if (hasCredential !== "yes") {
    log.step(pc.bold("Step 1 of 2 — Create a credential"));
    log.info(pc.dim(`  Opening: ${DASHBOARD_CREDENTIALS_URL}`));
    await open(DASHBOARD_CREDENTIALS_URL);
    await sleep(800);

    note(
      `${pc.bold("In the browser:")}\n  ${pc.cyan("1.")} Click the orange ${pc.green('"+ New credential"')} button (top right)\n  ${pc.cyan("2.")} Fill in:\n       • Name        ${pc.cyan('(e.g. "Gmail")')}\n       • Type        ${pc.cyan("(Username / Password, API key, OAuth...)")}\n       • Username + Password\n       • Expiration  ${pc.cyan("(optional)")}\n  ${pc.cyan("3.")} Click orange ${pc.green('"Create securely"')}\n\n${pc.cyan("The credential is encrypted (AES-256-GCM) and stored in TEE.")}`,
      pc.cyan("What to do"),
      { format: (value) => value },
    );

    const done = await confirm({
      message: "Done? Ready to generate a token?",
      initialValue: true,
    });

    if (!done || isCancel(done)) {
      outro(pc.dim("Cancelled. Run `toani login` to resume later."));
      return;
    }
  }

  const stepLabel =
    hasCredential === "yes" ? "Generate a token" : "Step 2 of 2 — Generate a token";
  log.step(pc.bold(stepLabel));
  log.info(pc.dim(`  Opening: ${DASHBOARD_TOKENS_URL}`));
  await open(DASHBOARD_TOKENS_URL);
  await sleep(800);

  note(
    `${pc.bold("In the browser:")}\n  ${pc.cyan("1.")} Pick a ${pc.green("scope")}  ${pc.cyan('(start with "Read credentials" / credential:read)')}\n  ${pc.cyan("2.")} Select your credential under ${pc.green('"Allowed credentials"')}\n  ${pc.cyan("3.")} Pick token expiry  ${pc.cyan("(1 hour or 7 days)")}\n  ${pc.cyan("4.")} Click orange ${pc.green('"Generate token"')}\n  ${pc.cyan("5.")} On the right panel, click ${pc.green("[📋 Copy]")}  ${pc.yellow("← only shown ONCE")}\n\n${pc.yellow("I'll detect the copy automatically — no need to paste here.")}`,
    pc.cyan("What to do"),
    { format: (value) => value },
  );

  const how = await select({
    message: "How do you want to provide the token?",
    options: [
      {
        value: "auto",
        label: "Auto-detect from clipboard",
        hint: "I watch, you click Copy",
      },
      { value: "paste", label: "Paste it here now", hint: "masked input" },
      {
        value: "env",
        label: "Set TOANI_VAULT_TOKEN in .env",
        hint: "edit file, come back",
      },
      { value: "cancel", label: "Cancel" },
    ],
    initialValue: "auto",
  });

  if (isCancel(how) || how === "cancel") {
    cancel();
  }

  if (how === "auto") {
    await waitForClipboard(baseUrl, skipValidate);
    return;
  }

  await requestTokenInput(baseUrl, skipValidate);
}

export async function runLogin(
  config: CliConfig,
  argv: string[],
): Promise<void> {
  const options = parseOptions(argv);
  const baseUrl =
    (options["base-url"] as string | undefined) ??
    config.baseUrl ??
    DEFAULT_API_BASE_URL;
  const skipValidate = Boolean(options["skip-validate"]);

  await printLogo();
  intro(`${brand("✱")} ${pc.bold("Toani Vault")} ${pc.dim("— login")}`);

  const accountState = await select({
    message: "Do you have a Toani account?",
    options: [
      { value: "yes", label: "Yes, connect me" },
      {
        value: "no",
        label: "No, I'll sign up first",
        hint: "opens sign-up page",
      },
      {
        value: "paste",
        label: "I already have a token",
        hint: "skip browser, just paste",
      },
    ],
  });

  if (isCancel(accountState)) {
    cancel();
  }

  if (accountState === "no") {
    log.step("Opening sign-up page in your browser...");
    log.info(pc.dim(`  → ${DASHBOARD_LOGIN_URL}`));
    await open(DASHBOARD_LOGIN_URL);
    await sleep(800);

    note(
      `${pc.bold("In the browser:")}\n  ${pc.cyan("1.")} Enter your email\n  ${pc.cyan("2.")} Check inbox for OTP from privy.io  ${pc.cyan("(takes ~30s)")}\n  ${pc.cyan("3.")} Paste the 6-digit code\n  ${pc.cyan("4.")} ${pc.cyan("(First time only)")} Set Display Name → Complete Setup\n\n${pc.yellow("When you're done →")} come back here and press ${pc.green("Enter")} to continue.`,
      pc.cyan("Step 1 of 3 — Sign in / Sign up"),
      { format: (value) => value },
    );

    const ready = await confirm({
      message: "Signed in? Ready to create a credential?",
      initialValue: true,
    });

    if (!ready || isCancel(ready)) {
      outro(pc.dim("Cancelled. Run `toani login` to try again after signing up."));
      return;
    }
  }

  if (accountState === "paste") {
    await pasteFromClipboard(baseUrl, skipValidate);
    return;
  }

  await guidedSetup(baseUrl, skipValidate);
}

export const __testables = {
  confetti,
  handleValidationError,
  heartbeat,
  logoFlash,
  manualPaste,
  maybeInstallBundledSkill,
  pasteFromClipboard,
  printLogo,
  processToken,
  requestTokenInput,
  waitForClipboard,
};
