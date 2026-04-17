import { describe, expect, it, vi } from "vitest";
import fs from "node:fs";
import { saveConfig } from "../src/config/store.js";
import type { CliConfig } from "../src/types/cli.js";

const testConfig: CliConfig = {
  baseUrl: "https://dev-credbridge.bitkinetic.com/",
  token: "v4.local.test",
  output: "json",
  timeout: 30000,
  currentProfile: "default",
  profiles: { default: {} },
  credentialSource: "token",
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
});
