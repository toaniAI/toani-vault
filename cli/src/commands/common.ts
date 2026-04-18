import { ToaniVaultSDK } from "../../../sdk-typescript/src/index.js";
import type { CliConfig, ParsedOptions } from "../types/cli.js";
import { fail } from "../output/print.js";

const DASHBOARD_BASE_URL = "https://dev-credbridge.bitkinetic.com";
const DASHBOARD_LOGIN_URL = `${DASHBOARD_BASE_URL}/login`;
const DASHBOARD_TOKENS_URL = `${DASHBOARD_BASE_URL}/tokens`;

export function parseOptions(argv: string[]): ParsedOptions {
  const options: ParsedOptions = { _: [] };
  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];
    if (token.startsWith("--")) {
      const key = token.slice(2);
      const next = argv[i + 1];
      if (!next || next.startsWith("--")) {
        options[key] = true;
      } else {
        options[key] = next;
        i += 1;
      }
      continue;
    }
    options._.push(token);
  }
  return options;
}

function missingTokenMessage(config: CliConfig): string {
  return [
    "未检测到 CLI 可用的 API Token。",
    `请先前往 Dashboard 注册或登录账号：${DASHBOARD_LOGIN_URL}`,
    `登录后到 Dashboard Tokens 页面创建或复制访问凭证：${DASHBOARD_TOKENS_URL}`,
    `然后执行：toani config init --url ${config.baseUrl} --token <BEARER_TOKEN>`,
  ].join("\n");
}

export function createSdk(config: CliConfig): ToaniVaultSDK {
  if (!config.token?.trim()) {
    fail(missingTokenMessage(config));
  }
  return new ToaniVaultSDK({
    baseUrl: config.baseUrl,
    token: config.token,
    timeout: config.timeout,
  });
}

export function requireArg(
  options: ParsedOptions,
  key: string,
  message?: string,
): string {
  const value = options[key];
  if (typeof value !== "string" || value.length === 0) {
    fail(message ?? `Missing required option --${key}`);
  }
  return value;
}

export function parseJsonOption(
  options: ParsedOptions,
  key: string,
): Record<string, unknown> {
  const raw = options[key];
  if (typeof raw !== "string") {
    fail(`Missing required option --${key}`);
  }
  try {
    return JSON.parse(raw) as Record<string, unknown>;
  } catch (error) {
    fail(`Invalid JSON for --${key}: ${(error as Error).message}`);
  }
}

export function capabilityMissing(name: string): never {
  fail(`SDK capability missing: ${name}`);
}
