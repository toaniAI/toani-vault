/**
 * CredBridge SDK Auth 服务测试
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { AuthService } from "../src/auth.js";
import { CredBridgeClient } from "../src/client.js";

describe("AuthService", () => {
  const mockBaseUrl = "https://vault.credbridge.io";
  const mockToken =
    "v4.local.eyJzdWIiOiJ0ZW5hbnQxOnVzZXIxIiwiZXhwIjoxNzA0MDY3MjAwLCJpYXQiOjE3MDQwNjM2MDAsImp0aSI6InRva2VuMTIzIiwic2NvcGUiOiJjcmVkZW50aWFsOnJlYWQgY3JlZGVudGlhbDp3cml0ZSJ9.signature";

  let client: CredBridgeClient;
  let service: AuthService;

  beforeEach(() => {
    client = new CredBridgeClient({
      baseUrl: mockBaseUrl,
      token: mockToken,
    });
    service = new AuthService(client);
    vi.resetAllMocks();
  });

  it("应该创建会话并映射响应字段", async () => {
    vi.spyOn(client, "post").mockResolvedValue({
      user: {
        id: "user-123",
        display_name: "Alice",
        status: "active",
        onboarding_completed: true,
        default_tenant_id: "tenant-123",
        identities: [
          {
            provider: "privy",
            subject: "wallet:0xabc",
            wallet_address: "0xabc",
            email: "alice@example.com",
            is_verified: true,
            is_primary: true,
          },
        ],
      },
      session: {
        id: "session-123",
        session_token: "session-token",
        expires_at: "2026-04-10T00:00:00Z",
        mfa_status: "not_required",
      },
      memberships: [
        {
          id: "membership-123",
          tenant_id: "tenant-123",
          role: "owner",
          status: "active",
          scopes: ["admin"],
          joined_at: "2026-04-01T00:00:00Z",
        },
      ],
      current_tenant: {
        id: "tenant-123",
        name: "Primary Tenant",
      },
      current_membership: {
        id: "membership-123",
        tenant_id: "tenant-123",
        role: "owner",
        status: "active",
        scopes: ["admin"],
        joined_at: "2026-04-01T00:00:00Z",
      },
    });

    const result = await service.createSession({
      privyAccessToken: "privy-token",
      invitationToken: "invite-token",
    });

    expect(result.session.sessionToken).toBe("session-token");
    expect(result.user.displayName).toBe("Alice");
    expect(result.currentTenant?.id).toBe("tenant-123");
    expect(result.currentMembership?.tenantId).toBe("tenant-123");
    expect(result.memberships[0]?.joinedAt).toBe("2026-04-01T00:00:00Z");
    expect(client.post).toHaveBeenCalledWith(
      "/auth/session",
      {
        privy_access_token: "privy-token",
        invitation_token: "invite-token",
      },
      undefined,
    );
  });

  it("应该获取当前用户信息", async () => {
    vi.spyOn(client, "get").mockResolvedValue({
      user: {
        id: "user-123",
        display_name: "Alice",
        status: "active",
        onboarding_completed: false,
        default_tenant_id: "tenant-123",
        identities: [],
      },
      current_tenant: {
        id: "tenant-123",
        name: "Primary Tenant",
      },
      current_membership: {
        id: "membership-123",
        tenant_id: "tenant-123",
        role: "owner",
        status: "active",
        scopes: ["admin"],
      },
      memberships: [],
      mfa_status: "not_required",
    });

    const result = await service.me();

    expect(result.user.displayName).toBe("Alice");
    expect(result.mfaStatus).toBe("not_required");
    expect(result.currentMembership?.role).toBe("owner");
    expect(client.get).toHaveBeenCalledWith("/auth/me", undefined);
  });

  it("应该签发 access token 并映射响应字段", async () => {
    vi.spyOn(client, "post").mockResolvedValue({
      access_token: "v4.local.access-token",
      token_id: "token-123",
      token_type: "Bearer",
      expires_at: 1770000000,
      expires_in: 900,
      granted_scopes: ["credential:read", "audit:read"],
    });

    const result = await service.createAccessToken({
      scopes: ["credential:read", "audit:read"],
      ttlSeconds: 900,
    });

    expect(result.accessToken).toBe("v4.local.access-token");
    expect(result.grantedScopes).toEqual(["credential:read", "audit:read"]);
    expect(client.post).toHaveBeenCalledWith(
      "/auth/access-token",
      {
        scopes: ["credential:read", "audit:read"],
        ttl_seconds: 900,
      },
      undefined,
    );
  });

  it("应该撤销 access token", async () => {
    vi.spyOn(client, "post").mockResolvedValue({ revoked: true });

    const result = await service.revokeAccessToken("token-123");

    expect(result).toBe(true);
    expect(client.post).toHaveBeenCalledWith(
      "/tokens/token-123/revoke",
      {},
      undefined,
    );
  });

  it("应该注销当前会话", async () => {
    vi.spyOn(client, "post").mockResolvedValue({ success: true });

    const result = await service.logout();

    expect(result.success).toBe(true);
    expect(client.post).toHaveBeenCalledWith(
      "/auth/logout",
      undefined,
      undefined,
    );
  });

  it("应该获取成员资格列表", async () => {
    vi.spyOn(client, "get").mockResolvedValue({
      memberships: [
        {
          id: "membership-123",
          tenant_id: "tenant-123",
          role: "owner",
          status: "active",
          scopes: ["admin"],
          joined_at: "2026-04-01T00:00:00Z",
        },
      ],
    });

    const result = await service.memberships();

    expect(result.memberships).toHaveLength(1);
    expect(result.memberships[0]?.tenantId).toBe("tenant-123");
    expect(client.get).toHaveBeenCalledWith("/auth/memberships", undefined);
  });
});
