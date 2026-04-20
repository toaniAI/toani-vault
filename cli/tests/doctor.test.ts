import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CliConfig } from "../src/types/cli.js";

const keychainMock = vi.hoisted(() => ({
  get: vi.fn(),
}));
const validateTokenMock = vi.hoisted(() => vi.fn());

vi.mock("../src/lib/keychain.js", () => ({
  keychain: keychainMock,
}));

vi.mock("../src/lib/validate.js", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("../src/lib/validate.js")>();
  return {
    ...actual,
    validateToken: validateTokenMock,
  };
});

import { runDoctor } from "../src/commands/doctor.js";

const baseConfig: CliConfig = {
  baseUrl: "https://api.example.com",
  output: "table",
  timeout: 30000,
  currentProfile: "default",
  profiles: { default: {} },
  credentialSource: "none",
};

describe("runDoctor", () => {
  beforeEach(() => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    vi.spyOn(process.stdout, "write").mockImplementation(() => true);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.clearAllMocks();
  });

  it("prompts login when no token is available", async () => {
    keychainMock.get.mockReturnValue(null);

    await runDoctor(baseConfig, []);

    expect(console.log).toHaveBeenCalledWith(expect.stringContaining("Quick fix:"));
    expect(console.log).toHaveBeenCalledWith(expect.stringContaining("toani login"));
  });

  it("warns when only a legacy plaintext token is available", async () => {
    keychainMock.get.mockReturnValue(null);
    validateTokenMock.mockResolvedValue({ ok: true, status: 200 });

    await runDoctor(
      {
        ...baseConfig,
        legacyToken: `v4.local.${"a".repeat(120)}`,
        credentialSource: "config_legacy",
      },
      [],
    );

    expect(console.log).toHaveBeenCalledWith(
      expect.stringContaining("plaintext, consider migrating to keychain"),
    );
  });

  it("classifies a 401 token validation failure", async () => {
    keychainMock.get.mockReturnValue(`v4.local.${"b".repeat(120)}`);
    validateTokenMock.mockResolvedValue({
      ok: false,
      status: 401,
      reason: "invalid_or_expired",
    });

    await runDoctor(baseConfig, []);

    expect(console.log).toHaveBeenCalledWith(
      expect.stringContaining("HTTP 401"),
    );
    expect(console.log).toHaveBeenCalledWith(
      expect.stringContaining("Run `toani login`"),
    );
  });
});
