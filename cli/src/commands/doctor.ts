import { readFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import pc from "picocolors";
import type { CliConfig } from "../types/cli.js";
import { getCliVersion } from "../index.js";
import { keychain } from "../lib/keychain.js";
import {
  checkBaseUrlReachability,
  DEFAULT_API_BASE_URL,
  isPasetoToken,
  type ValidationProbeResult,
  type ValidationResult,
  validateToken,
} from "../lib/validate.js";
import { parseOptions } from "./common.js";

const REQUIRED_NODE_MAJOR = 22;

function summarizeProbeUrl(baseUrl: string, probeUrl: string): string {
  try {
    const base = new URL(baseUrl);
    const probe = new URL(probeUrl);
    if (base.origin === probe.origin) {
      return probe.pathname || "/";
    }
  } catch {
    return probeUrl;
  }

  return probeUrl;
}

function row(
  status: "ok" | "warn" | "err" | "info",
  label: string,
  detail: string,
): void {
  const icons = {
    ok: pc.green("✓"),
    warn: pc.yellow("⚠"),
    err: pc.red("✗"),
    info: pc.dim("ⓘ"),
  };
  console.log(`  ${icons[status]} ${label.padEnd(22)} ${pc.dim(detail)}`);
}

function probeStatus(
  probe: ValidationProbeResult | undefined,
): "ok" | "warn" | "info" {
  if (probe?.ok) {
    return "ok";
  }

  if (
    probe?.reason === "insufficient_scope" ||
    probe?.reason === "insufficient_permissions"
  ) {
    return "info";
  }

  return "warn";
}

function describeProbe(probe: ValidationProbeResult | undefined): string {
  if (!probe) {
    return "Not checked";
  }

  if (probe.ok) {
    return `OK (${probe.path})`;
  }

  if (probe.reason === "insufficient_scope") {
    const required = String(probe.body?.required_scope ?? "<unknown>");
    return `Missing scope for ${probe.path} (${required})`;
  }

  if (probe.reason === "insufficient_permissions") {
    return `Not applicable for this token type (${probe.path})`;
  }

  if (probe.reason === "invalid_or_expired") {
    return `Rejected by server (${probe.path})`;
  }

  return `Unexpected response at ${probe.path}`;
}

function describeAccessMode(result: ValidationResult): {
  focus: string;
  label: string;
} {
  switch (result.mode) {
    case "usage":
      return {
        focus:
          "Emphasize credential and sandbox access; web-session profile checks are secondary.",
        label: "Usage permissions",
      };
    case "management":
      return {
        focus:
          "Emphasize token and tenant management APIs; credential access may need a separate runtime token.",
        label: "Management permissions",
      };
    case "mixed":
      return {
        focus:
          "Emphasize both credential usage and token administration because this token can do both.",
        label: "Mixed permissions",
      };
    case "session_profile":
      return {
        focus:
          "Emphasize web-session profile health first; usage and token-management scopes are limited.",
        label: "Session profile only",
      };
    default:
      return {
        focus:
          "Token reached the service, but this CLI cannot confirm useful usage or management permissions yet.",
        label: "Unclear permissions",
      };
  }
}

function describeTokenKind(result: ValidationResult): string {
  if (result.tokenKind === "web_session") {
    return "Web session token";
  }

  if (result.tokenKind === "api_access") {
    return "API access token";
  }

  return "Unknown token type";
}

async function readLegacyToken(): Promise<string | null> {
  const configPath = path.join(os.homedir(), ".toani", "config.json");
  try {
    const raw = await readFile(configPath, "utf8");
    const parsed = JSON.parse(raw) as { token?: string };
    return typeof parsed.token === "string" ? parsed.token : null;
  } catch {
    return null;
  }
}

export async function runDoctor(
  config: CliConfig,
  argv: string[],
): Promise<void> {
  const options = parseOptions(argv);
  const baseUrl =
    (options["base-url"] as string | undefined) ??
    config.baseUrl ??
    DEFAULT_API_BASE_URL;

  console.log(`\n  ${pc.bold("Toani Vault — health check")}\n`);

  let pass = 0;
  let warn = 0;
  let fail = 0;

  row("ok", "CLI version", getCliVersion());
  pass += 1;

  const nodeMajor = Number.parseInt(process.versions.node.split(".")[0] ?? "0", 10);
  if (nodeMajor >= REQUIRED_NODE_MAJOR) {
    row(
      "ok",
      "Node.js version",
      `${process.versions.node} (>= ${REQUIRED_NODE_MAJOR} required)`,
    );
    pass += 1;
  } else {
    row(
      "err",
      "Node.js version",
      `${process.versions.node} (need >= ${REQUIRED_NODE_MAJOR})`,
    );
    fail += 1;
  }

  let token = keychain.get();
  let tokenSource = token ? "OS Keychain" : "";

  if (!token) {
    const legacyToken = config.legacyToken ?? (await readLegacyToken());
    if (legacyToken) {
      token = legacyToken;
      tokenSource =
        "~/.toani/config.json (plaintext, consider migrating to keychain)";
    }
  }

  if (token) {
    const isPlaintext = tokenSource.includes("plaintext");
    row(isPlaintext ? "warn" : "ok", "Token storage", tokenSource);
    if (isPlaintext) {
      warn += 1;
    } else {
      pass += 1;
    }
  } else {
    row("err", "Token storage", "No token found anywhere");
    fail += 1;
  }

  if (token) {
    if (isPasetoToken(token)) {
      row("ok", "Token format", `PASETO v4.local (${token.length} chars)`);
      pass += 1;
    } else {
      row("err", "Token format", "Invalid (doesn't look like PASETO)");
      fail += 1;
    }
  } else {
    row("info", "Token format", "Skipped (no token)");
  }

  row("ok", "Base URL", baseUrl);
  pass += 1;

  process.stdout.write(`  ${pc.dim("⏳")} Checking base URL... `);
  const reachabilityStartedAt = Date.now();
  const reachability = await checkBaseUrlReachability(baseUrl);
  const reachabilityDuration = Date.now() - reachabilityStartedAt;
  process.stdout.write(`\r${" ".repeat(60)}\r`);

  if (reachability.ok) {
    const probePath = summarizeProbeUrl(baseUrl, reachability.url);
    const detail =
      reachability.status === 200
        ? `${reachabilityDuration}ms via ${probePath}`
        : `${reachabilityDuration}ms via ${probePath} (HTTP ${reachability.status})`;
    const status = reachability.status === 200 ? "ok" : "warn";
    row(status, "Base URL reachable", detail);
    if (status === "ok") {
      pass += 1;
    } else {
      warn += 1;
    }
  } else if (reachability.reason === "dns") {
    row(
      "err",
      "Base URL reachable",
      `DNS lookup failed for ${new URL(baseUrl).hostname}`,
    );
    fail += 1;
  } else if (reachability.reason === "refused") {
    row("err", "Base URL reachable", "Connection refused");
    fail += 1;
  } else if (reachability.reason === "timeout") {
    row("err", "Base URL reachable", "Timeout (>8s)");
    fail += 1;
  } else {
    const error = reachability.error as
      | { cause?: { code?: string }; code?: string }
      | undefined;
    row(
      "err",
      "Base URL reachable",
      `Network error: ${error?.cause?.code ?? error?.code ?? "unknown"} (VPN required?)`,
    );
    fail += 1;
  }

  if (token && isPasetoToken(token) && reachability.ok) {
    process.stdout.write(`  ${pc.dim("⏳")} Validating token... `);
    const startedAt = Date.now();
    const result = await validateToken(baseUrl, token);
    const duration = Date.now() - startedAt;
    process.stdout.write(`\r${" ".repeat(60)}\r`);

    if (result.ok) {
      row("ok", "Server reachable", `${duration}ms`);
      row("ok", "Token valid", "Accepted by API probes");
      row("ok", "Token type", describeTokenKind(result));
      const accessMode = describeAccessMode(result);
      row("ok", "Access mode", accessMode.label);
      row("info", "Doctor focus", accessMode.focus);
      row(
        probeStatus(result.probes?.authMe),
        "Web session APIs",
        describeProbe(result.probes?.authMe),
      );
      row(
        probeStatus(result.probes?.credentials),
        "Usage APIs",
        describeProbe(result.probes?.credentials),
      );
      row(
        probeStatus(result.probes?.tokens),
        "Management APIs",
        describeProbe(result.probes?.tokens),
      );
      pass += 2;
    } else if (result.reason === "invalid_or_expired") {
      row("ok", "Server reachable", `${duration}ms`);
      row("err", "Token valid", "HTTP 401 — expired or revoked. Run `toani-vault login`");
      pass += 1;
      fail += 1;
    } else if (result.reason === "insufficient_scope") {
      row("ok", "Server reachable", `${duration}ms`);
      row("warn", "Token valid", "HTTP 403 — insufficient scope (token works but limited)");
      pass += 1;
      warn += 1;
    } else if (result.reason === "insufficient_permissions") {
      row("ok", "Server reachable", `${duration}ms`);
      row(
        "warn",
        "Token valid",
        "HTTP 403 — token reached the service but does not match the required token type",
      );
      row(
        "info",
        "Hint",
        "Usage/API tokens commonly skip /auth/me; run a usage or management probe instead.",
      );
      pass += 1;
      warn += 1;
    } else if (result.reason === "dns") {
      row("err", "Server reachable", `DNS lookup failed for ${new URL(baseUrl).hostname}`);
      row("info", "Token valid", "Skipped (server unreachable)");
      fail += 1;
    } else if (result.reason === "refused") {
      row("err", "Server reachable", "Connection refused");
      row("info", "Token valid", "Skipped (server unreachable)");
      fail += 1;
    } else if (result.reason === "timeout") {
      row("err", "Server reachable", "Timeout (>8s)");
      row("info", "Token valid", "Skipped (server unreachable)");
      fail += 1;
    } else if (result.reason === "network") {
      const error = result.error as { cause?: { code?: string }; code?: string } | undefined;
      row(
        "err",
        "Server reachable",
        `Network error: ${error?.cause?.code ?? error?.code ?? "unknown"} (VPN required?)`,
      );
      row("info", "Token valid", "Skipped (server unreachable)");
      fail += 1;
    } else {
      row("warn", "Server reachable", `HTTP ${result.status}`);
      row("info", "Token valid", "Unexpected response");
      warn += 1;
    }
  } else if (token && isPasetoToken(token) && !reachability.ok) {
    row("info", "Token valid", "Skipped (base URL unreachable)");
  }

  console.log(`\n  ${pc.dim("─".repeat(60))}`);
  if (fail === 0 && warn === 0) {
    console.log(`  ${pc.green(`✓ All ${pass} checks passed.`)}  ${pc.dim("Everything is healthy.")}`);
  } else if (fail === 0) {
    console.log(
      `  ${pc.yellow(`⚠ ${pass} passed, ${warn} warning${warn > 1 ? "s" : ""}.`)}  ${pc.dim("CLI is usable but consider the warnings above.")}`,
    );
  } else {
    console.log(
      `  ${pc.red(`✗ ${fail} check${fail > 1 ? "s" : ""} failed.`)}  ${pc.dim("See suggestions above to fix.")}`,
    );
    if (!token) {
      console.log(`\n  ${pc.bold("Quick fix:")}  ${pc.green("toani-vault login")}`);
    }
  }
  console.log("");
}
