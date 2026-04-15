#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { loadConfig, saveConfig } from "./config/store.js";
import { runConfig } from "./commands/config.js";
import { runSandbox } from "./commands/index.js";
import { printResult } from "./output/print.js";
import type { OutputFormat } from "./types/cli.js";

const BASE_URL_ENV_KEYS = ["TOANI_BASE_URL", "CREDBRIDGE_BASE_URL"] as const;
const TOKEN_ENV_KEYS = ["TOANI_VAULT_TOKEN", "CREDBRIDGE_TOKEN"] as const;
const PACKAGE_JSON_URL = new URL("../package.json", import.meta.url);

export const HELP_TEXT = `toani - Toani Vault CLI

Usage:
  toani [--output json|table] [--base-url URL] [--token TOKEN] <group> <command> [options]

Groups:
  config       init/show
  sandbox      create-session/list-sessions/get-session/terminate/pause/resume/execute/export-dom/export-data/get-operation/stats
`;

export function getCliVersion(): string {
  const packageJson = JSON.parse(
    readFileSync(PACKAGE_JSON_URL, "utf8"),
  ) as { version?: string };
  return packageJson.version ?? "0.0.0";
}

function printHelp(): void {
  console.log(HELP_TEXT);
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

function parseGlobalArgs(argv: string[]): {
  rest: string[];
  output?: OutputFormat;
  baseUrl?: string;
  token?: string;
} {
  const rest: string[] = [];
  let output: OutputFormat | undefined;
  let baseUrl: string | undefined;
  let token: string | undefined;

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--output") {
      output = (argv[i + 1] as OutputFormat) ?? "table";
      i += 1;
    } else if (arg === "--base-url") {
      baseUrl = argv[i + 1];
      i += 1;
    } else if (arg === "--token") {
      token = argv[i + 1];
      i += 1;
    } else if (arg === "-h" || arg === "--help") {
      printHelp();
      process.exit(0);
    } else {
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
  const runtimeConfig = {
    ...config,
    output: globals.output ?? config.output,
    baseUrl: globals.baseUrl ?? envBaseUrl ?? config.baseUrl,
    token: globals.token ?? envToken ?? config.token,
  };

  if (globals.baseUrl || globals.token || globals.output) {
    saveConfig(runtimeConfig);
  }

  switch (group) {
    case "config":
      await runConfig(runtimeConfig, subArgs);
      return;
    case "sandbox":
      await runSandbox(runtimeConfig, subArgs);
      return;
    case "--version":
    case "-v":
      printResult(
        { name: "@toani/vault-cli", version: getCliVersion() },
        runtimeConfig.output,
      );
      return;
    default:
      throw new Error(`Unknown command group: ${group}`);
  }
}

const isDirectExecution =
  process.argv[1] !== undefined &&
  import.meta.url === pathToFileURL(process.argv[1]).href;

if (isDirectExecution) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  });
}
