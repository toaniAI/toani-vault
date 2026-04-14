import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import type { CliConfig, CliProfile, OutputFormat } from "../types/cli.js";

const CONFIG_DIR = path.join(os.homedir(), ".toani");
const CONFIG_PATH = path.join(CONFIG_DIR, "config.json");

const DEFAULT_CONFIG: CliConfig = {
  baseUrl: "https://dev-credbridge.bitkinetic.com/",
  output: "table",
  timeout: 30000,
  currentProfile: "default",
  profiles: { default: {} },
  credentialSource: "none",
};

export function getConfigPath(): string {
  return CONFIG_PATH;
}

export function loadConfig(): CliConfig {
  if (!fs.existsSync(CONFIG_PATH)) {
    return { ...DEFAULT_CONFIG };
  }
  const raw = fs.readFileSync(CONFIG_PATH, "utf8");
  const parsed = JSON.parse(raw) as Partial<CliConfig>;
  const currentProfile = parsed.currentProfile ?? "default";
  const profiles = parsed.profiles ?? { default: {} };
  const activeProfile: CliProfile = profiles[currentProfile] ?? {};
  const output =
    (activeProfile.output ?? parsed.output ?? DEFAULT_CONFIG.output) === "json"
      ? "json"
      : "table";
  const timeout =
    typeof activeProfile.timeout === "number"
      ? activeProfile.timeout
      : typeof parsed.timeout === "number"
        ? parsed.timeout
        : DEFAULT_CONFIG.timeout;
  const baseUrl =
    activeProfile.baseUrl ?? parsed.baseUrl ?? DEFAULT_CONFIG.baseUrl;
  const token = activeProfile.token ?? parsed.token;
  const sessionToken = activeProfile.sessionToken ?? parsed.sessionToken;
  const currentTenantId =
    activeProfile.currentTenantId ?? parsed.currentTenantId;
  return {
    ...DEFAULT_CONFIG,
    ...parsed,
    baseUrl,
    token,
    sessionToken,
    currentTenantId,
    currentProfile,
    profiles,
    output: output as OutputFormat,
    timeout,
    credentialSource: token
      ? "token"
      : sessionToken
        ? "legacy"
        : "none",
  };
}

export function saveConfig(config: CliConfig): void {
  if (!fs.existsSync(CONFIG_DIR)) {
    fs.mkdirSync(CONFIG_DIR, { recursive: true });
  }
  const currentProfile = config.currentProfile ?? "default";
  const profiles = {
    ...(config.profiles ?? {}),
    [currentProfile]: {
      ...(config.profiles?.[currentProfile] ?? {}),
      baseUrl: config.baseUrl,
      token: config.token,
      currentTenantId: config.currentTenantId,
      output: config.output,
      timeout: config.timeout,
    },
  };
  fs.writeFileSync(
    CONFIG_PATH,
    JSON.stringify(
      {
        baseUrl: config.baseUrl,
        token: config.token,
        currentTenantId: config.currentTenantId,
        output: config.output,
        timeout: config.timeout,
        profiles,
        currentProfile,
      },
      null,
      2,
    ),
    "utf8",
  );
}
