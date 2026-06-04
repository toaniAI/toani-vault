import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CliConfig } from "../src/types/cli.js";

const keychainMock = vi.hoisted(() => ({
  get: vi.fn(),
}));
const validateTokenMock = vi.hoisted(() => vi.fn());
const checkBaseUrlReachabilityMock = vi.hoisted(() => vi.fn());

vi.mock("../src/lib/keychain.js", () => ({
  keychain: keychainMock,
}));

vi.mock("../src/lib/validate.js", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("../src/lib/validate.js")>();
  return {
    ...actual,
    checkBaseUrlReachability: checkBaseUrlReachabilityMock,
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
    checkBaseUrlReachabilityMock.mockResolvedValue({
      ok: true,
      status: 200,
      url: "https://api.example.com/api/v1/health",
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.clearAllMocks();
  });

  it("prompts login when no token is available", async () => {
    keychainMock.get.mockReturnValue(null);

    await runDoctor(baseConfig, []);

    expect(console.log).toHaveBeenCalledWith(expect.stringContaining("Quick fix:"));
    expect(console.log).toHaveBeenCalledWith(expect.stringContaining("toani-vault login"));
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
      expect.stringContaining("Run `toani-vault login`"),
    );
  });

  it("shows usage-mode focus for credential tokens", async () => {
    keychainMock.get.mockReturnValue(`v4.local.${"u".repeat(120)}`);
    validateTokenMock.mockResolvedValue({
      ok: true,
      status: 200,
      mode: "usage",
      tokenKind: "api_access",
      probes: {
        authMe: {
          ok: false,
          path: "/api/v1/auth/me",
          reason: "insufficient_permissions",
          status: 403,
        },
        credentials: {
          ok: true,
          path: "/api/v1/credentials?page=1&page_size=1",
          status: 200,
        },
        tokens: {
          ok: false,
          path: "/api/v1/tokens?page=1&page_size=1",
          reason: "insufficient_permissions",
          status: 403,
        },
      },
    });

    await runDoctor(baseConfig, []);

    expect(console.log).toHaveBeenCalledWith(
      expect.stringContaining("Usage permissions"),
    );
    expect(console.log).toHaveBeenCalledWith(
      expect.stringContaining("API access token"),
    );
    expect(console.log).toHaveBeenCalledWith(
      expect.stringContaining("credential and sandbox access"),
    );
  });

  it("shows the configured production base URL", async () => {
    keychainMock.get.mockReturnValue(null);

    await runDoctor(
      {
        ...baseConfig,
        baseUrl: "https://dashboard.toani.ai",
      },
      [],
    );

    expect(console.log).toHaveBeenCalledWith(
      expect.stringContaining("https://dashboard.toani.ai"),
    );
  });

  it("checks base URL reachability even without a token", async () => {
    keychainMock.get.mockReturnValue(null);

    await runDoctor(baseConfig, []);

    expect(checkBaseUrlReachabilityMock).toHaveBeenCalledWith(
      "https://api.example.com",
    );
    expect(console.log).toHaveBeenCalledWith(
      expect.stringContaining("Base URL reachable"),
    );
  });

  it("reports base URL connectivity failures and skips token validation", async () => {
    keychainMock.get.mockReturnValue(`v4.local.${"c".repeat(120)}`);
    checkBaseUrlReachabilityMock.mockResolvedValue({
      ok: false,
      status: 0,
      url: "https://api.example.com/api/v1/health",
      reason: "refused",
    });

    await runDoctor(baseConfig, []);

    expect(console.log).toHaveBeenCalledWith(
      expect.stringContaining("Connection refused"),
    );
    expect(validateTokenMock).not.toHaveBeenCalled();
  });
});
