import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { runConfig } from "../src/commands/config.js";
import type { CliConfig } from "../src/types/cli.js";

const keychainMock = vi.hoisted(() => ({
  set: vi.fn(),
}));

vi.mock("../src/lib/keychain.js", () => ({
  keychain: keychainMock,
}));

const CONFIG_DIR = path.join(os.homedir(), ".toani");
const CONFIG_PATH = path.join(CONFIG_DIR, "config.json");

function readConfigFile(): CliConfig | null {
  if (!fs.existsSync(CONFIG_PATH)) return null;
  return JSON.parse(fs.readFileSync(CONFIG_PATH, "utf8")) as CliConfig;
}

function cleanupConfig(): void {
  if (fs.existsSync(CONFIG_PATH)) {
    fs.unlinkSync(CONFIG_PATH);
  }
  if (fs.existsSync(CONFIG_DIR)) {
    fs.rmdirSync(CONFIG_DIR);
  }
}

describe("runConfig", () => {
  const configuredBaseUrl = "https://api.example.com/";

  beforeEach(() => {
    cleanupConfig();
    vi.spyOn(console, "log").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
    cleanupConfig();
  });

  it("config init persists the explicit service url", async () => {
    const testConfig: CliConfig = {
      baseUrl: "https://example.com/",
      output: "table",
      timeout: 30000,
      currentProfile: "default",
      profiles: { default: {} },
      credentialSource: "none",
    };

    await runConfig(testConfig, [
      "init",
      "--url",
      configuredBaseUrl,
    ]);

    const saved = readConfigFile();
    expect(saved?.baseUrl).toBe(configuredBaseUrl);
    expect(saved?.profiles?.default?.baseUrl).toBe(configuredBaseUrl);
  });

  it("config init preserves token from runtime config when only url is passed", async () => {
    const runtimeConfig: CliConfig = {
      baseUrl: "https://example.com/",
      token: "test-token-abc123",
      output: "table",
      timeout: 30000,
      currentProfile: "default",
      profiles: { default: {} },
      credentialSource: "keychain",
    };

    await runConfig(runtimeConfig, [
      "init",
      "--url",
      configuredBaseUrl,
    ]);

    const saved = readConfigFile();
    expect(saved?.baseUrl).toBe(configuredBaseUrl);
    expect(saved?.token).toBeUndefined();
  });

  it("config init writes explicit tokens to keychain instead of config.json", async () => {
    const runtimeConfig: CliConfig = {
      baseUrl: "https://example.com/",
      output: "table",
      timeout: 30000,
      currentProfile: "default",
      profiles: { default: {} },
      credentialSource: "none",
    };

    await runConfig(runtimeConfig, [
      "init",
      "--url",
      configuredBaseUrl,
      "--token",
      "v4.local.test-token",
    ]);

    expect(keychainMock.set).toHaveBeenCalledWith("v4.local.test-token");
    const saved = readConfigFile();
    expect(saved?.token).toBeUndefined();
  });
});
