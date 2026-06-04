import { describe, expect, it, vi } from "vitest";
import { SandboxService } from "../src/sandbox.js";
import { OperationStatus, OperationType } from "../src/types.js";

function createMockClient() {
  return {
    post: vi.fn(),
    get: vi.fn(),
    delete: vi.fn(),
  };
}

describe("SandboxService broker-only surface", () => {
  it("submits broker request to /sandbox/http-requests", async () => {
    const client = createMockClient();
    client.post.mockResolvedValue({
      operation_id: "op-1",
      status: OperationStatus.Success,
      data: { status: 200 },
      execution_time_ms: 12,
    });
    const service = new SandboxService(client as never);

    const result = await service.request({
      operationType: OperationType.HttpRequest,
      credentialId: "cred-1",
      serviceId: "svc-1",
      requestId: "req-1",
      description: "health check",
      parameters: { url: "https://api.example.com/health", method: "GET" },
      method: "GET",
    });

    expect(client.post).toHaveBeenCalledWith(
      "/sandbox/http-requests",
      {
        operation_type: "http_request",
        credential_id: "cred-1",
        service_id: "svc-1",
        request_id: "req-1",
        description: "health check",
        parameters: {
          url: "https://api.example.com/health",
          method: "GET",
        },
      },
      undefined,
    );
    expect(result.operationId).toBe("op-1");
    expect(result.status).toBe(OperationStatus.Success);
  });

  it("reads broker request detail from /sandbox/http-requests/:operationId", async () => {
    const client = createMockClient();
    client.get.mockResolvedValue({
      operation_id: "op-1",
      session_id: "",
      operation_type: "http_request",
      status: "success",
      started_at: "2026-05-17T00:00:00Z",
      completed_at: "2026-05-17T00:00:01Z",
      execution_time_ms: 100,
    });
    const service = new SandboxService(client as never);

    const result = await service.getRequest("op-1");

    expect(client.get).toHaveBeenCalledWith("/sandbox/http-requests/op-1", undefined);
    expect(result.operationId).toBe("op-1");
    expect(result.operationType).toBe("http_request");
  });
});
