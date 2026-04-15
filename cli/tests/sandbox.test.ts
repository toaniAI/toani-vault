import { afterEach, describe, expect, it, vi } from "vitest";
import { runSandbox } from "../src/commands/sandbox.js";
import type { CliConfig } from "../src/types/cli.js";

const sandboxMock = vi.hoisted(() => ({
  createSession: vi.fn(),
  listSessions: vi.fn(),
  getSession: vi.fn(),
  closeSession: vi.fn(),
  pauseSession: vi.fn(),
  resumeSession: vi.fn(),
  executeOperation: vi.fn(),
  exportDom: vi.fn(),
  exportData: vi.fn(),
  getOperation: vi.fn(),
  getStats: vi.fn(),
}));

vi.mock("../src/commands/common.js", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("../src/commands/common.js")>();
  return {
    ...actual,
    createSdk: vi.fn(() => ({ sandbox: sandboxMock })),
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

describe("runSandbox", () => {
  afterEach(() => {
    vi.clearAllMocks();
    vi.restoreAllMocks();
  });

  it("passes execute params through to preserve backend operation fields", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    sandboxMock.executeOperation.mockResolvedValue({
      operationId: "op-1",
      status: "success",
      executionTimeMs: 1,
    });

    await runSandbox(testConfig, [
      "execute",
      "session-1",
      "--operation-type",
      "export",
      "--description",
      "export selectors",
      "--params",
      '{"selectors":[".row"],"duration_ms":250,"sensitive":true}',
    ]);

    expect(sandboxMock.executeOperation).toHaveBeenCalledWith("session-1", {
      operationType: "export",
      description: "export selectors",
      parameters: {
        selectors: [".row"],
        duration_ms: 250,
        sensitive: true,
      },
      selector: undefined,
      value: undefined,
      url: undefined,
      method: undefined,
      headers: undefined,
      body: undefined,
      script: undefined,
      bindings: undefined,
      attribute: undefined,
      timeout: undefined,
      waitCondition: undefined,
    });
  });

  it("exposes pause and resume session commands", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    sandboxMock.pauseSession.mockResolvedValue({ sessionId: "session-1" });
    sandboxMock.resumeSession.mockResolvedValue({ sessionId: "session-1" });

    await runSandbox(testConfig, ["pause", "session-1"]);
    await runSandbox(testConfig, ["resume", "session-1"]);

    expect(sandboxMock.pauseSession).toHaveBeenCalledWith("session-1");
    expect(sandboxMock.resumeSession).toHaveBeenCalledWith("session-1");
  });

  it("exposes export-data using the backend selectors contract", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    sandboxMock.exportData.mockResolvedValue({
      exportId: "export-1",
      dataBase64: "e30=",
      format: "json",
      filename: "export.json",
      sizeBytes: 2,
    });

    await runSandbox(testConfig, [
      "export-data",
      "session-1",
      "--format",
      "json",
      "--selectors",
      '[".balance",".status"]',
    ]);

    expect(sandboxMock.exportData).toHaveBeenCalledWith("session-1", {
      format: "json",
      selectors: [".balance", ".status"],
    });
  });
});
