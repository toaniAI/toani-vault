#!/usr/bin/env node
import { loadConfig, saveConfig } from "./config/store.js";
import {
  runAudit,
  runAuth,
  runConfig,
  runCredentials,
  runSandbox,
  runServiceAccounts,
  runTokens,
} from "./commands/index.js";
import { printResult } from "./output/print.js";
import type { OutputFormat } from "./types/cli.js";

const BASE_URL_ENV_KEYS = ["TOANI_BASE_URL", "CREDBRIDGE_BASE_URL"] as const;

function printHelp(): void {
  console.log(`toani - Toani Vault CLI

Usage:
  toani [--output json|table] [--base-url URL] [--token TOKEN] <group> <command> [options]

Groups:
  auth         status/logout/me/memberships/use-tenant/token/access-token
  config       init/show/set/get
  credentials  list/get/create/update/delete/decrypt/versions/rollback
  tokens       create/list/get/verify/stats/revoke
  service-accounts  create/list/get/update/token
  sandbox      create-session/list-sessions/get-session/terminate/execute/get-operation/stats
  audit        logs/export/verify
`);
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
  const runtimeConfig = {
    ...config,
    output: globals.output ?? config.output,
    baseUrl: globals.baseUrl ?? envBaseUrl ?? config.baseUrl,
    token: globals.token ?? config.token,
  };

  if (globals.baseUrl || globals.token || globals.output) {
    saveConfig(runtimeConfig);
  }

  switch (group) {
    case "auth":
      await runAuth(runtimeConfig, subArgs);
      return;
    case "config":
      await runConfig(runtimeConfig, subArgs);
      return;
    case "credentials":
      await runCredentials(runtimeConfig, subArgs);
      return;
    case "tokens":
      await runTokens(runtimeConfig, subArgs);
      return;
    case "service-accounts":
      await runServiceAccounts(runtimeConfig, subArgs);
      return;
    case "sandbox":
      await runSandbox(runtimeConfig, subArgs);
      return;
    case "audit":
      await runAudit(runtimeConfig, subArgs);
      return;
    case "--version":
    case "-v":
      printResult(
        { name: "@toani/vault-cli", version: "0.0.2" },
        runtimeConfig.output,
      );
      return;
    default:
      throw new Error(`Unknown command group: ${group}`);
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
