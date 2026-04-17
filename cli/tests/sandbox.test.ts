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
  bootstrapPage: vi.fn(),
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

  it("preserves credential references for top-level fill values", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    sandboxMock.executeOperation.mockResolvedValue({
      operationId: "op-fill",
      status: "success",
      executionTimeMs: 1,
    });

    await runSandbox(testConfig, [
      "execute",
      "session-1",
      "--operation-type",
      "fill",
      "--params",
      '{"selector":"input[name=password]","value":{"$credential":"password"}}',
    ]);

    expect(sandboxMock.executeOperation).toHaveBeenCalledWith("session-1", {
      operationType: "fill",
      description: undefined,
      parameters: {
        selector: "input[name=password]",
        value: { $credential: "password" },
      },
      selector: "input[name=password]",
      value: { $credential: "password" },
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

  it("preserves plain-string execute_script bindings", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    sandboxMock.executeOperation.mockResolvedValue({
      operationId: "op-script",
      status: "success",
      executionTimeMs: 1,
    });

    await runSandbox(testConfig, [
      "execute",
      "session-1",
      "--operation-type",
      "execute_script",
      "--params",
      '{"script":"return document.querySelector(bindings.selector)?.textContent ?? null","bindings":{"selector":"h1"}}',
    ]);

    expect(sandboxMock.executeOperation).toHaveBeenCalledWith("session-1", {
      operationType: "execute_script",
      description: undefined,
      parameters: {
        script:
          "return document.querySelector(bindings.selector)?.textContent ?? null",
        bindings: { selector: "h1" },
      },
      selector: undefined,
      value: undefined,
      url: undefined,
      method: undefined,
      headers: undefined,
      body: undefined,
      script:
        "return document.querySelector(bindings.selector)?.textContent ?? null",
      bindings: { selector: "h1" },
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

  it("builds a fixed bootstrap_page request body from the dedicated subcommand", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    sandboxMock.bootstrapPage.mockResolvedValue({
      operationId: "op-bootstrap",
      status: "success",
      executionTimeMs: 1,
    });

    await runSandbox(testConfig, [
      "bootstrap-page",
      "--session-id",
      "session-1",
      "--mode",
      "rocket_loader",
      "--script-selectors",
      '["script[src][type$=\\"-text/javascript\\"]"]',
      "--include-plain-scripts",
      "false",
      "--replay-lifecycle-events",
      "true",
      "--wait-selector",
      'input[name="email"]',
      "--wait-timeout-ms",
      "15000",
    ]);

    expect(sandboxMock.bootstrapPage).toHaveBeenCalledWith("session-1", {
      mode: "rocket_loader",
      scriptSelectors: ['script[src][type$="-text/javascript"]'],
      includePlainScripts: false,
      replayLifecycleEvents: true,
      waitSelector: 'input[name="email"]',
      waitTimeoutMs: 15000,
    });
  });

  it("leaves bootstrap script discovery defaults to the backend when selectors are omitted", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    sandboxMock.bootstrapPage.mockResolvedValue({
      operationId: "op-bootstrap",
      status: "success",
      executionTimeMs: 1,
    });

    await runSandbox(testConfig, [
      "bootstrap-page",
      "session-1",
      "--mode",
      "rocket_loader",
      "--include-plain-scripts",
      "true",
    ]);

    expect(sandboxMock.bootstrapPage).toHaveBeenCalledWith("session-1", {
      mode: "rocket_loader",
      scriptSelectors: undefined,
      includePlainScripts: true,
      replayLifecycleEvents: undefined,
      waitSelector: undefined,
      waitTimeoutMs: undefined,
    });
  });

  it("rejects raw params and bindings for bootstrap-page", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});

    await expect(
      runSandbox(testConfig, [
        "bootstrap-page",
        "session-1",
        "--mode",
        "rocket_loader",
        "--params",
        '{"script":"alert(1)"}',
      ]),
    ).rejects.toThrow(
      "bootstrap-page only accepts fixed bootstrap flags; raw scripts, bindings, and --params are not supported",
    );
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
