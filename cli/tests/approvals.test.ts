import { afterEach, describe, expect, it, vi } from "vitest";
import { runApprovals } from "../src/commands/approvals.js";
import type { CliConfig } from "../src/types/cli.js";

const approvalsMock = vi.hoisted(() => ({
  create: vi.fn(),
  get: vi.fn(),
}));

vi.mock("../src/commands/common.js", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("../src/commands/common.js")>();
  return {
    ...actual,
    createSdk: vi.fn(() => ({ approvals: approvalsMock })),
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

function approvalDetail(status: string) {
  return {
    approval_id: "approval-1",
    tenant_id: "tenant-1",
    requested_by: "user-1",
    business_type: "oauth_binding_access",
    business_id: "binding-1",
    status,
    created_at: "2026-05-15T05:35:29.000Z",
    updated_at: "2026-05-15T05:35:29.000Z",
    processed_by: status === "pending" ? undefined : "operator-1",
    processed_at:
      status === "pending" ? undefined : "2026-05-15T05:35:30.000Z",
    remark: status === "pending" ? undefined : `${status} by test`,
  };
}

function expectLastPrintedJson(): Record<string, unknown> {
  const lastCall = vi.mocked(console.log).mock.calls.at(-1);
  expect(lastCall).toBeDefined();
  return JSON.parse(String(lastCall?.[0])) as Record<string, unknown>;
}

describe("runApprovals", () => {
  afterEach(() => {
    process.exitCode = undefined;
    vi.useRealTimers();
    vi.clearAllMocks();
    vi.restoreAllMocks();
  });

  it("maps create args to the approvals SDK and preserves business references", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    approvalsMock.create.mockResolvedValue({
      approval_id: "approval-1",
      status: "pending",
      business_type: "oauth_binding_access",
      business_id: "binding-1",
    });

    await runApprovals(testConfig, [
      "create",
      "--business-type",
      "oauth_binding_access",
      "--business-id",
      "binding-1",
    ]);

    expect(approvalsMock.create).toHaveBeenCalledWith({
      businessType: "oauth_binding_access",
      businessId: "binding-1",
    });
  });

  it("passes approval id positionally for status", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    approvalsMock.get.mockResolvedValue(approvalDetail("approved"));

    await runApprovals(testConfig, ["status", "approval-1"]);

    expect(approvalsMock.get).toHaveBeenCalledWith("approval-1");
  });

  it("generates a runtime approval request id without calling the SDK", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});

    await runApprovals(testConfig, ["generate-request-id"]);

    expect(approvalsMock.create).not.toHaveBeenCalled();
    expect(approvalsMock.get).not.toHaveBeenCalled();
    expect(expectLastPrintedJson()).toMatchObject({
      business_type: "credential_runtime_access",
      single_use: true,
    });
    expect(expectLastPrintedJson().request_id).toMatch(
      /^req_\d{13}_[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
  });

  it("waits on create when --wait is enabled and prints the terminal approval detail", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    approvalsMock.create.mockResolvedValue({
      approval_id: "approval-1",
      status: "pending",
      business_type: "oauth_binding_access",
      business_id: "binding-1",
    });
    approvalsMock.get.mockResolvedValue(approvalDetail("approved"));

    await runApprovals(testConfig, [
      "create",
      "--business-type",
      "oauth_binding_access",
      "--business-id",
      "binding-1",
      "--wait",
    ]);

    expect(approvalsMock.create).toHaveBeenCalledWith({
      businessType: "oauth_binding_access",
      businessId: "binding-1",
    });
    expect(approvalsMock.get).toHaveBeenCalledWith("approval-1");
    expect(expectLastPrintedJson()).toMatchObject({ status: "approved" });
    expect(process.exitCode).toBeUndefined();
  });

  it("polls wait until the approval reaches a terminal status", async () => {
    vi.useFakeTimers();
    vi.spyOn(console, "log").mockImplementation(() => {});
    approvalsMock.get
      .mockResolvedValueOnce(approvalDetail("pending"))
      .mockResolvedValueOnce(approvalDetail("approved"));

    const waitPromise = runApprovals(testConfig, [
      "wait",
      "approval-1",
      "--timeout-ms",
      "500",
      "--poll-interval-ms",
      "100",
    ]);

    await vi.advanceTimersByTimeAsync(100);
    await waitPromise;

    expect(approvalsMock.get).toHaveBeenCalledTimes(2);
    expect(expectLastPrintedJson()).toMatchObject({ status: "approved" });
  });

  it("returns a scriptable exit code for rejected approvals", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    approvalsMock.get.mockResolvedValue(approvalDetail("rejected"));

    await runApprovals(testConfig, ["wait", "approval-1"]);

    expect(expectLastPrintedJson()).toMatchObject({ status: "rejected" });
    expect(process.exitCode).toBe(20);
  });

  it("returns a scriptable exit code for cancelled approvals", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    approvalsMock.get.mockResolvedValue(approvalDetail("cancelled"));

    await runApprovals(testConfig, ["wait", "approval-1"]);

    expect(expectLastPrintedJson()).toMatchObject({ status: "cancelled" });
    expect(process.exitCode).toBe(21);
  });

  it("returns the last pending snapshot with wait timeout metadata", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    approvalsMock.get.mockResolvedValue(approvalDetail("pending"));
    const nowSpy = vi
      .spyOn(Date, "now")
      .mockReturnValueOnce(0)
      .mockReturnValueOnce(250);

    await runApprovals(testConfig, [
      "wait",
      "approval-1",
      "--timeout-ms",
      "250",
    ]);

    expect(nowSpy).toHaveBeenCalledTimes(2);
    expect(expectLastPrintedJson()).toMatchObject({
      status: "pending",
      wait_result: "timeout",
      timeout_ms: 250,
    });
    expect(process.exitCode).toBe(124);
  });

  it("fails with stable usage when create is missing required fields", async () => {
    await expect(runApprovals(testConfig, ["create"])).rejects.toThrow(
      "Usage: toani-vault approvals create --business-type <type> --business-id <id>",
    );
  });

  it("fails with stable usage when status is missing the approval id", async () => {
    await expect(runApprovals(testConfig, ["status"])).rejects.toThrow(
      "Usage: toani-vault approvals status <approvalId>",
    );
  });

  it("fails with stable usage when wait is missing the approval id", async () => {
    await expect(runApprovals(testConfig, ["wait"])).rejects.toThrow(
      "Usage: toani-vault approvals wait <approvalId> [--timeout-ms <ms>] [--poll-interval-ms <ms>]",
    );
  });

  it("fails with stable usage when generate-request-id receives extra args", async () => {
    await expect(
      runApprovals(testConfig, ["generate-request-id", "extra"]),
    ).rejects.toThrow("Usage: toani-vault approvals generate-request-id");
  });
});
