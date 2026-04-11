import { describe, expect, it, beforeEach, afterEach, vi } from "vitest";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { runConfig } from "../src/commands/config.js";
import type { CliConfig } from "../src/types/cli.js";

const CONFIG_DIR = path.join(os.homedir(), ".toani");
const CONFIG_PATH = path.join(CONFIG_DIR, "config.json");

// Helper to read config file
function readConfigFile(): CliConfig | null {
  if (!fs.existsSync(CONFIG_PATH)) return null;
  return JSON.parse(fs.readFileSync(CONFIG_PATH, "utf8")) as CliConfig;
}

// Helper to clean up config
function cleanupConfig(): void {
  if (fs.existsSync(CONFIG_PATH)) {
    fs.unlinkSync(CONFIG_PATH);
  }
  if (fs.existsSync(CONFIG_DIR)) {
    fs.rmdirSync(CONFIG_DIR);
  }
}

describe("runConfig init", () => {
  beforeEach(() => {
    cleanupConfig();
    vi.spyOn(console, "log").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
    cleanupConfig();
  });

  it("writes token to config when --url and --token are passed", async () => {
    const testConfig: CliConfig = {
      baseUrl: "https://example.com/",
      output: "table",
      timeout: 30000,
      currentProfile: "default",
      profiles: { default: {} },
      credentialSource: "none",
    };

    // Simulate the scenario: global --token was captured in runtimeConfig.token
    // but options.token in runConfig is undefined (stripped by parseGlobalArgs)
    await runConfig(testConfig, ["init", "--url", "https://dev-credbridge.bitkinetic.com/"]);

    const saved = readConfigFile();
    expect(saved).not.toBeNull();
    expect(saved?.baseUrl).toBe("https://dev-credbridge.bitkinetic.com/");
    // token should fallback to config.token (which was set by runtimeConfig)
    expect(saved?.token).toBeUndefined(); // testConfig had no token
  });

  it("preserves token from runtimeConfig when global --token was used", async () => {
    // This simulates the real bug scenario:
    // index.ts sets runtimeConfig.token = globals.token (e.g., "test-token-abc123")
    // then calls runConfig(runtimeConfig, subArgs) where subArgs has no --token
    const runtimeConfig: CliConfig = {
      baseUrl: "https://example.com/",
      token: "test-token-abc123", // This was set by globals.token
      output: "table",
      timeout: 30000,
      currentProfile: "default",
      profiles: { default: {} },
      credentialSource: "token",
    };

    // subArgs only has --url, no --token (it was stripped by parseGlobalArgs)
    await runConfig(runtimeConfig, ["init", "--url", "https://dev-credbridge.bitkinetic.com/"]);

    const saved = readConfigFile();
    expect(saved).not.toBeNull();
    expect(saved?.baseUrl).toBe("https://dev-credbridge.bitkinetic.com/");
    // KEY FIX: token must fallback to config.token (runtimeConfig.token)
    expect(saved?.token).toBe("test-token-abc123");
    expect(saved?.profiles?.default?.token).toBe("test-token-abc123");
  });

  it("writes explicit --token from subArgs when present", async () => {
    const testConfig: CliConfig = {
      baseUrl: "https://example.com/",
      output: "table",
      timeout: 30000,
      currentProfile: "default",
      profiles: { default: {} },
      credentialSource: "none",
    };

    // When user passes --token directly to config init (uncommon but valid)
    await runConfig(testConfig, ["init", "--url", "https://dev-credbridge.bitkinetic.com/", "--token", "explicit-token"]);

    const saved = readConfigFile();
    expect(saved).not.toBeNull();
    expect(saved?.token).toBe("explicit-token");
  });

  it("preserves existing token when no --url or --token passed", async () => {
    const testConfig: CliConfig = {
      baseUrl: "https://example.com/",
      token: "existing-token",
      output: "table",
      timeout: 30000,
      currentProfile: "default",
      profiles: { default: {} },
      credentialSource: "token",
    };

    await runConfig(testConfig, ["init"]);

    const saved = readConfigFile();
    expect(saved).not.toBeNull();
    expect(saved?.baseUrl).toBe("https://example.com/");
    expect(saved?.token).toBe("existing-token");
  });
});