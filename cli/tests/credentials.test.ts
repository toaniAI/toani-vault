import { afterEach, describe, expect, it, vi } from "vitest";
import { runCredentials } from "../src/commands/credentials.js";
import type { CliConfig } from "../src/types/cli.js";

const credentialsMock = vi.hoisted(() => ({
  list: vi.fn(),
  get: vi.fn(),
}));

vi.mock("../src/commands/common.js", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("../src/commands/common.js")>();
  return {
    ...actual,
    createSdk: vi.fn(() => ({ credentials: credentialsMock })),
  };
});

const testConfig: CliConfig = {
  baseUrl: "https://example.com",
  output: "json",
  timeout: 30000,
  currentProfile: "default",
  profiles: { default: {} },
  credentialSource: "none",
};

describe("runCredentials", () => {
  afterEach(() => {
    vi.clearAllMocks();
    vi.restoreAllMocks();
  });

  it("maps list filters directly to the SDK", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    credentialsMock.list.mockResolvedValue({
      credentials: [],
      total: 0,
    });

    await runCredentials(testConfig, [
      "list",
      "--service-id",
      "svc-1",
      "--credential-type",
      "api_key",
      "--only-valid",
      "true",
    ]);

    expect(credentialsMock.list).toHaveBeenCalledWith({
      serviceId: "svc-1",
      credentialType: "api_key",
      onlyValid: true,
    });
  });

  it("supports list with no filters", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    credentialsMock.list.mockResolvedValue({
      credentials: [],
      total: 0,
    });

    await runCredentials(testConfig, ["list"]);

    expect(credentialsMock.list).toHaveBeenCalledWith({
      serviceId: undefined,
      credentialType: undefined,
      onlyValid: undefined,
    });
  });

  it("passes credential id positionally for get", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    credentialsMock.get.mockResolvedValue({
      credentialId: "cred-1",
      serviceId: "svc-1",
      credentialType: "api_key",
      createdAt: "2026-04-18T00:00:00Z",
      isDeleted: false,
    });

    await runCredentials(testConfig, ["get", "cred-1"]);

    expect(credentialsMock.get).toHaveBeenCalledWith("cred-1");
  });

  it("fails with stable usage when get is missing the credential id", async () => {
    await expect(runCredentials(testConfig, ["get"])).rejects.toThrow(
      "Usage: toani credentials get <credentialId>",
    );
  });

  it("rejects invalid boolean values for only-valid", async () => {
    await expect(
      runCredentials(testConfig, ["list", "--only-valid", "maybe"]),
    ).rejects.toThrow("Invalid boolean for --only-valid: maybe");
  });
});
