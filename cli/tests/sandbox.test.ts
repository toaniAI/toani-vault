import { afterEach, describe, expect, it, vi } from "vitest";
import * as common from "../src/commands/common.js";
import { runSandbox } from "../src/commands/sandbox.js";
import type { CliConfig } from "../src/types/cli.js";

const sandboxMock = vi.hoisted(() => ({
  request: vi.fn(),
  getRequest: vi.fn(),
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

  it("auto-generates a request id when omitted", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    vi.spyOn(common, "generateRequestId").mockReturnValue("auto-uuid-111");
    sandboxMock.request.mockResolvedValue({
      operationId: "op-auto",
      status: "success",
      executionTimeMs: 2,
    });

    await runSandbox(testConfig, [
      "request",
      "--operation-type",
      "http_request",
      "--credential-id",
      "cred-1",
      "--params",
      '{"url":"https://api.example.com/health","method":"GET"}',
    ]);

    expect(sandboxMock.request).toHaveBeenCalledWith({
      operationType: "http_request",
      credentialId: "cred-1",
      serviceId: undefined,
      requestId: "auto-uuid-111",
      description: undefined,
      parameters: {
        url: "https://api.example.com/health",
        method: "GET",
      },
      method: "GET",
      headers: undefined,
      body: undefined,
      timeout: undefined,
    });
  });

  it("submits broker request payload without session lifecycle fields", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    sandboxMock.request.mockResolvedValue({
      operationId: "op-1",
      status: "success",
      executionTimeMs: 1,
    });

    await runSandbox(testConfig, [
      "request",
      "--operation-type",
      "http_request",
      "--description",
      "check upstream",
      "--params",
      '{"credential_id":"cred-from-params","service_id":"svc-from-params","request_id":"req-from-params","url":"https://api.example.com/health","method":"GET","headers":{"x-api-key":{"$credential":"api_key"}}}',
    ]);

    expect(sandboxMock.request).toHaveBeenCalledWith({
      operationType: "http_request",
      credentialId: "cred-from-params",
      serviceId: "svc-from-params",
      requestId: "req-from-params",
      description: "check upstream",
      parameters: {
        url: "https://api.example.com/health",
        method: "GET",
        headers: { "x-api-key": { $credential: "api_key" } },
      },
      method: "GET",
      headers: { "x-api-key": { $credential: "api_key" } },
      body: undefined,
      timeout: undefined,
    });
  });

  it("prefers explicit credential flags over params payload", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    sandboxMock.request.mockResolvedValue({
      operationId: "op-2",
      status: "success",
      executionTimeMs: 1,
    });

    await runSandbox(testConfig, [
      "request",
      "--operation-type",
      "http_request",
      "--credential-id",
      "cred-from-flag",
      "--service-id",
      "svc-from-flag",
      "--request-id",
      "req-from-flag",
      "--params",
      '{"credential_id":"cred-from-params","service_id":"svc-from-params","request_id":"req-from-params","url":"https://api.example.com/health","method":"GET"}',
    ]);

    expect(sandboxMock.request).toHaveBeenCalledWith({
      operationType: "http_request",
      credentialId: "cred-from-flag",
      serviceId: "svc-from-flag",
      requestId: "req-from-flag",
      description: undefined,
      parameters: {
        url: "https://api.example.com/health",
        method: "GET",
      },
      method: "GET",
      headers: undefined,
      body: undefined,
      timeout: undefined,
    });
  });

  it("supports get-request command", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    sandboxMock.getRequest.mockResolvedValue({ operationId: "op-1" });

    await runSandbox(testConfig, ["get-request", "op-1"]);

    expect(sandboxMock.getRequest).toHaveBeenCalledWith("op-1");
  });

  it("rejects unknown legacy sandbox subcommands", async () => {
    await expect(runSandbox(testConfig, ["stats"])).rejects.toThrow(
      "Usage: toani-vault sandbox <request|get-request> [options]",
    );
  });
});
