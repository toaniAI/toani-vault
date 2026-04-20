import { describe, expect, it, vi } from "vitest";
import fs from "node:fs";
import { loadConfig, saveConfig } from "../src/config/store.js";
import type { CliConfig } from "../src/types/cli.js";

const keychainMock = vi.hoisted(() => ({
  get: vi.fn(),
}));

vi.mock("../src/lib/keychain.js", () => ({
  keychain: keychainMock,
}));

const testConfig: CliConfig = {
  baseUrl: "https://dev-credbridge.bitkinetic.com/",
  token: "v4.local.test",
  output: "json",
  timeout: 30000,
  currentProfile: "default",
  profiles: { default: {} },
  credentialSource: "keychain",
};

describe("saveConfig", () => {
  it("adds a clear path-aware error when config persistence fails", () => {
    vi.spyOn(fs, "existsSync").mockReturnValue(true);
    vi.spyOn(fs, "writeFileSync").mockImplementation(() => {
      throw new Error("EACCES: permission denied");
    });

    expect(() => saveConfig(testConfig)).toThrowError(
      /Failed to write CLI config at .*config\.json: EACCES: permission denied/,
    );
  });

  it("loads keychain token ahead of legacy config token", () => {
    keychainMock.get.mockReturnValue("v4.local.keychain-token");
    vi.spyOn(fs, "existsSync").mockReturnValue(true);
    vi.spyOn(fs, "readFileSync").mockReturnValue(
      JSON.stringify({
        baseUrl: "https://api.example.com/",
        token: "v4.local.legacy-token",
        currentProfile: "default",
        profiles: { default: {} },
      }),
    );

    const loaded = loadConfig();

    expect(loaded.token).toBe("v4.local.keychain-token");
    expect(loaded.legacyToken).toBe("v4.local.legacy-token");
    expect(loaded.credentialSource).toBe("keychain");
  });
});
