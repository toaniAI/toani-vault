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

const tableConfig: CliConfig = {
  ...testConfig,
  output: "table",
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

  it("prints approval guidance for credential get in table mode", async () => {
    const logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
    vi.spyOn(console, "table").mockImplementation(() => {});
    credentialsMock.get.mockResolvedValue({
      credentialId: "cred-approval",
      serviceId: "svc-1",
      credentialType: "api_key",
      createdAt: "2026-04-18T00:00:00Z",
      requiresApproval: true,
      isDeleted: false,
    });

    await runCredentials(tableConfig, ["get", "cred-approval"]);

    const output = logSpy.mock.calls.map(([value]) => String(value)).join("\n");
    expect(output).toContain("Approval required before sandbox execution:");
    expect(output).toContain("toani-vault approvals generate-request-id");
    expect(output).toContain(
      "toani-vault approvals create --business-type credential_runtime_access --business-id <request_id> [--wait]",
    );
    expect(output).toContain(
      "toani-vault sandbox request --operation-type http_request --credential-id cred-approval --request-id <request_id> --params '{...}'",
    );
    expect(output).toContain("request_id is single-use");
  });

  it("prints approval summary for credential list in table mode", async () => {
    const logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
    vi.spyOn(console, "table").mockImplementation(() => {});
    credentialsMock.list.mockResolvedValue({
      items: [
        {
          credentialId: "cred-approval",
          serviceId: "svc-1",
          credentialType: "api_key",
          requiresApproval: true,
        },
      ],
      page: 1,
      pageSize: 20,
      total: 1,
      totalPages: 1,
    });

    await runCredentials(tableConfig, ["list"]);

    const output = logSpy.mock.calls.map(([value]) => String(value)).join("\n");
    expect(output).toContain(
      "Some listed credentials require runtime approval before sandbox execution.",
    );
    expect(output).toContain("toani-vault credentials get <credentialId>");
    expect(output).toContain("toani-vault approvals generate-request-id");
    expect(output).toContain("toani-vault sandbox request --request-id <request_id>");
  });

  it("does not print approval guidance for credential get in json mode", async () => {
    const logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
    credentialsMock.get.mockResolvedValue({
      credentialId: "cred-approval",
      serviceId: "svc-1",
      credentialType: "api_key",
      createdAt: "2026-04-18T00:00:00Z",
      requiresApproval: true,
      isDeleted: false,
    });

    await runCredentials(testConfig, ["get", "cred-approval"]);

    expect(logSpy).toHaveBeenCalledTimes(1);
    expect(String(logSpy.mock.calls[0]?.[0])).toContain("\"requiresApproval\": true");
  });

  it("fails with stable usage when get is missing the credential id", async () => {
    await expect(runCredentials(testConfig, ["get"])).rejects.toThrow(
      "Usage: toani-vault credentials get <credentialId>",
    );
  });

  it("rejects invalid boolean values for only-valid", async () => {
    await expect(
      runCredentials(testConfig, ["list", "--only-valid", "maybe"]),
    ).rejects.toThrow("Invalid boolean for --only-valid: maybe");
  });
});
