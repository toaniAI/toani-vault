import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { runConfig } from "../src/commands/config.js";
import type { CliConfig } from "../src/types/cli.js";

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
      "https://dev-credbridge.bitkinetic.com/",
    ]);

    const saved = readConfigFile();
    expect(saved?.baseUrl).toBe("https://dev-credbridge.bitkinetic.com/");
    expect(saved?.profiles?.default?.baseUrl).toBe(
      "https://dev-credbridge.bitkinetic.com/",
    );
  });

  it("config init preserves token from runtime config when only url is passed", async () => {
    const runtimeConfig: CliConfig = {
      baseUrl: "https://example.com/",
      token: "test-token-abc123",
      output: "table",
      timeout: 30000,
      currentProfile: "default",
      profiles: { default: {} },
      credentialSource: "token",
    };

    await runConfig(runtimeConfig, [
      "init",
      "--url",
      "https://dev-credbridge.bitkinetic.com/",
    ]);

    const saved = readConfigFile();
    expect(saved?.baseUrl).toBe("https://dev-credbridge.bitkinetic.com/");
    expect(saved?.token).toBe("test-token-abc123");
  });
});
