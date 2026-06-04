import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApprovalsService } from "../src/approvals.js";
import { CredBridgeClient } from "../src/client.js";

describe("ApprovalsService", () => {
  const mockBaseUrl = "https://vault.credbridge.io";
  const mockToken =
    "v4.local.eyJzdWIiOiJ0ZW5hbnQxOnVzZXIxIiwiZXhwIjoxNzA0MDY3MjAwLCJpYXQiOjE3MDQwNjM2MDAsImp0aSI6InRva2VuMTIzIiwic2NvcGUiOiJ0ZW5hbnRfYWRtaW4ifQ.signature";

  let client: CredBridgeClient;
  let service: ApprovalsService;

  beforeEach(() => {
    client = new CredBridgeClient({
      baseUrl: mockBaseUrl,
      token: mockToken,
    });
    service = new ApprovalsService(client);
    vi.resetAllMocks();
  });

  it("creates approvals asynchronously and preserves business references", async () => {
    vi.spyOn(client, "post").mockResolvedValue({
      approval_id: "approval-1",
      status: "pending",
    });

    const result = await service.create({
      businessType: "oauth_binding_access",
      businessId: "binding-1",
    });

    expect(result).toEqual({
      approval_id: "approval-1",
      status: "pending",
      business_type: "oauth_binding_access",
      business_id: "binding-1",
    });
    expect(client.post).toHaveBeenCalledWith(
      "/approvals",
      {
        business_type: "oauth_binding_access",
        business_id: "binding-1",
      },
      undefined,
    );
  });

  it("queries approval status by approval id", async () => {
    vi.spyOn(client, "get").mockResolvedValue({
      approval_id: "approval-1",
      tenant_id: "tenant-1",
      requested_by: "user-1",
      business_type: "oauth_binding_access",
      business_id: "binding-1",
      status: "approved",
      created_at: "2026-05-15T04:00:00Z",
      updated_at: "2026-05-15T04:05:00Z",
      processed_by: "user-2",
      processed_at: "2026-05-15T04:05:00Z",
      remark: "approved",
      result_code: "oauth_binding_access_granted",
      result_payload: { granted: true },
      business_result_written_at: "2026-05-15T04:05:00Z",
    });

    const result = await service.get("approval-1");

    expect(result.status).toBe("approved");
    expect(result.business_type).toBe("oauth_binding_access");
    expect(client.get).toHaveBeenCalledWith("/approvals/approval-1", undefined);
  });
});
