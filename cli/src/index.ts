#!/usr/bin/env node
import { existsSync, readFileSync, realpathSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { loadConfig, saveConfig } from "./config/store.js";
import { runCredentials } from "./commands/credentials.js";
import { runApprovals } from "./commands/approvals.js";
import { runConfig } from "./commands/config.js";
import { runDoctor } from "./commands/doctor.js";
import { runLogin } from "./commands/login.js";
import { runSandbox } from "./commands/index.js";
import { printResult } from "./output/print.js";
import type { CliConfig, CredentialSource, OutputFormat } from "./types/cli.js";

const BASE_URL_ENV_KEYS = ["TOANI_BASE_URL", "CREDBRIDGE_BASE_URL"] as const;
const TOKEN_ENV_KEYS = ["TOANI_VAULT_TOKEN", "CREDBRIDGE_TOKEN"] as const;
const PACKAGE_JSON_URL = new URL("../package.json", import.meta.url);

export const HELP_TEXT = `toani-vault - Toani Vault CLI

Usage:
  toani-vault [--output json|table] [--base-url URL] [--token TOKEN] <group> <command> [options]

Groups:
  login        interactive onboarding with browser assist + keychain storage
  doctor       health checks for CLI, token storage, and API reachability
  config       init/show
  credentials  list/get
  approvals    create/status/wait/generate-request-id
  sandbox      request/get-request
`;

export function getCliVersion(): string {
  const packageJson = JSON.parse(
    readFileSync(PACKAGE_JSON_URL, "utf8"),
  ) as { version?: string };
  return packageJson.version ?? "0.0.0";
}

export function formatCliVersion(version: string): string {
  return `v${version}`;
}

export function resolveVersionOutputFormat(
  runtimeOutput: OutputFormat,
  argv: string[],
): OutputFormat {
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === "--output") {
      return ((argv[i + 1] as OutputFormat | undefined) ?? runtimeOutput);
    }
  }

  return runtimeOutput;
}

function printHelp(): void {
  console.log(HELP_TEXT);
}

const REQUEST_ID_HINT =
  "Tip: this command can auto-generate one with UUID, or pass it explicitly with --request-id when you need to reuse the same ID for retries and audit correlation.";

function enhanceErrorMessage(error: unknown): string {
  const rendered =
    error instanceof Error
      ? error.message || error.stack || error.name || "Unknown CLI error"
      : String(error);

  if (rendered.toLowerCase().includes("request_id is required")) {
    return `${rendered}\n${REQUEST_ID_HINT}`;
  }

  return rendered;
}

function resolveBaseUrlFromEnv(): string | undefined {
  for (const key of BASE_URL_ENV_KEYS) {
    const value = process.env[key]?.trim();
    if (value) {
      return value;
    }
  }
  return undefined;
}

function resolveTokenFromEnv(): string | undefined {
  for (const key of TOKEN_ENV_KEYS) {
    const value = process.env[key]?.trim();
    if (value) {
      return value;
    }
  }
  return undefined;
}

export function parseGlobalArgs(argv: string[]): {
  rest: string[];
  output?: OutputFormat;
  baseUrl?: string;
  token?: string;
} {
  const rest: string[] = [];
  let output: OutputFormat | undefined;
  let baseUrl: string | undefined;
  let token: string | undefined;
  let seenGroup = false;

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (!seenGroup && arg === "--output") {
      output = (argv[i + 1] as OutputFormat) ?? "table";
      i += 1;
    } else if (!seenGroup && arg === "--base-url") {
      baseUrl = argv[i + 1];
      i += 1;
    } else if (!seenGroup && arg === "--token") {
      token = argv[i + 1];
      i += 1;
    } else if (!seenGroup && (arg === "-h" || arg === "--help")) {
      printHelp();
      process.exit(0);
    } else {
      seenGroup = true;
      rest.push(arg);
    }
  }

  return { rest, output, baseUrl, token };
}

async function main(): Promise<void> {
  const globals = parseGlobalArgs(process.argv.slice(2));
  const [group, ...subArgs] = globals.rest;
  if (!group) {
    printHelp();
    process.exit(1);
  }

  const config = loadConfig();
  const envBaseUrl = resolveBaseUrlFromEnv();
  const envToken = resolveTokenFromEnv();
  const credentialSource: CredentialSource = globals.token
    ? "explicit"
    : envToken
      ? "env"
      : config.token && config.credentialSource
        ? config.credentialSource
        : "none";
  const runtimeConfig: CliConfig = {
    ...config,
    output: globals.output ?? config.output,
    baseUrl: globals.baseUrl ?? envBaseUrl ?? config.baseUrl,
    token: globals.token ?? envToken ?? config.token,
    credentialSource,
  };

  if (globals.baseUrl || globals.token || globals.output) {
    saveConfig(runtimeConfig);
  }

  switch (group) {
    case "login":
      await runLogin(runtimeConfig, subArgs);
      return;
    case "doctor":
      await runDoctor(runtimeConfig, subArgs);
      return;
    case "config":
      await runConfig(runtimeConfig, subArgs);
      return;
    case "credentials":
      await runCredentials(runtimeConfig, subArgs);
      return;
    case "approvals":
      await runApprovals(runtimeConfig, subArgs);
      return;
    case "sandbox":
      await runSandbox(runtimeConfig, subArgs);
      return;
    case "--version":
    case "-v":
      {
        const version = getCliVersion();
        const output = resolveVersionOutputFormat(runtimeConfig.output, subArgs);
        if (output === "json") {
          printResult({ name: "@toani/vault-cli", version }, "json");
        } else {
          console.log(formatCliVersion(version));
        }
      }
      return;
    default:
      throw new Error(`Unknown command group: ${group}`);
  }
}

export function isDirectExecution(
  executedPath: string | undefined,
  moduleUrl: string,
): boolean {
  if (!executedPath || !existsSync(executedPath)) {
    return false;
  }

  try {
    return realpathSync(executedPath) === realpathSync(fileURLToPath(moduleUrl));
  } catch {
    return false;
  }
}

if (isDirectExecution(process.argv[1], import.meta.url)) {
  main().catch((error) => {
    console.error(enhanceErrorMessage(error));
    process.exit(1);
  });
}
