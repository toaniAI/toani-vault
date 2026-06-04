/**
 * CredBridge SDK 凭证服务测试
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { CredentialsService } from "../src/credentials.js";
import { CredBridgeClient } from "../src/client.js";
import {
  CredentialType,
  CredBridgeError,
  CredBridgeErrorCode,
} from "../src/types.js";

describe("CredentialsService", () => {
  const mockBaseUrl = "https://vault.credbridge.io";
  const mockToken =
    "v4.local.eyJzdWIiOiJ0ZW5hbnQxOnVzZXIxIiwiZXhwIjoxNzA0MDY3MjAwLCJpYXQiOjE3MDQwNjM2MDAsImp0aSI6InRva2VuMTIzIiwic2NvcGUiOiJjcmVkZW50aWFsOnJlYWQgY3JlZGVudGlhbDp3cml0ZSJ9.signature";

  let client: CredBridgeClient;
  let service: CredentialsService;

  beforeEach(() => {
    client = new CredBridgeClient({
      baseUrl: mockBaseUrl,
      token: mockToken,
    });
    service = new CredentialsService(client);
    vi.resetAllMocks();
  });

  describe("创建凭证", () => {
    it("应该成功创建凭证", async () => {
      const mockResponse = {
        credential_id: "cred-123",
        service_id: "schwab",
        credential_type: "username_password",
        created_at: "1704067200",
      };

      vi.spyOn(client, "post").mockResolvedValue(mockResponse);

      const result = await service.create({
        serviceId: "schwab",
        credentialType: CredentialType.UsernamePassword,
        plaintextData: {
          username: "user@example.com",
          password: "secret",
        },
      });

      expect(result.credential_id).toBe("cred-123");
      expect(result.service_id).toBe("schwab");
      expect(client.post).toHaveBeenCalledWith(
        "/credentials",
        {
          service_id: "schwab",
          credential_type: CredentialType.UsernamePassword,
          plaintext_data: {
            username: "user@example.com",
            password: "secret",
          },
          provider: undefined,
          allowed_domains: undefined,
          custom_functions: undefined,
          requires_approval: undefined,
          expires_at: undefined,
        },
        undefined,
      );
    });

    it("应该透传 provider、allowedDomains 和 customFunctions", async () => {
      vi.spyOn(client, "post").mockResolvedValue({
        credential_id: "cred-okx",
        service_id: "okx",
        credential_type: "api_key",
        created_at: "1704067200",
        provider: "okx",
        allowed_domains: ["www.okx.com:443"],
        custom_functions: [
          {
            function_name: "build_auth_header",
            function_body:
              "export default function func(input) { return `Bearer ${input}`; }",
          },
        ],
      });

      await service.create({
        serviceId: "okx",
        credentialType: CredentialType.ApiKey,
        plaintextData: {
          api_key: "ak_test",
          secret_key: "sk_test",
          passphrase: "passphrase",
        },
        provider: "okx",
        allowedDomains: ["www.okx.com:443"],
        customFunctions: [
          {
            function_name: "build_auth_header",
            function_body:
              "export default function func(input) { return `Bearer ${input}`; }",
          },
        ],
      });

      expect(client.post).toHaveBeenCalledWith(
        "/credentials",
        {
          service_id: "okx",
          credential_type: CredentialType.ApiKey,
          plaintext_data: {
            api_key: "ak_test",
            secret_key: "sk_test",
            passphrase: "passphrase",
          },
          provider: "okx",
          allowed_domains: ["www.okx.com:443"],
          custom_functions: [
            {
              function_name: "build_auth_header",
              function_body:
                "export default function func(input) { return `Bearer ${input}`; }",
            },
          ],
          requires_approval: undefined,
          expires_at: undefined,
        },
        undefined,
      );
    });

    it("应该透传 requiresApproval", async () => {
      vi.spyOn(client, "post").mockResolvedValue({
        credential_id: "cred-approval",
        service_id: "schwab",
        credential_type: "username_password",
        created_at: "1704067200",
        requires_approval: true,
      });

      await service.create({
        serviceId: "schwab",
        credentialType: CredentialType.UsernamePassword,
        plaintextData: {
          username: "approver@example.com",
          password: "secret",
        },
        requiresApproval: true,
      });

      expect(client.post).toHaveBeenCalledWith(
        "/credentials",
        expect.objectContaining({
          requires_approval: true,
        }),
        undefined,
      );
    });

    it("应该支持过期时间", async () => {
      const mockResponse = {
        credentialId: "cred-123",
        serviceId: "schwab",
        credentialType: "username_password",
        createdAt: "1704067200",
        expiresAt: "1706659200",
      };

      vi.spyOn(client, "post").mockResolvedValue(mockResponse);

      const expiresAt = Math.floor(Date.now() / 1000) + 86400 * 30; // 30天后
      await service.create({
        serviceId: "schwab",
        credentialType: CredentialType.UsernamePassword,
        plaintextData: { username: "test", password: "pass" },
        expiresAt,
      });

      expect(client.post).toHaveBeenCalledWith(
        "/credentials",
        expect.objectContaining({
          expires_at: expiresAt,
        }),
        undefined,
      );
    });
  });

  describe("快捷创建方法", () => {
    it("createUsernamePassword 应该正确创建凭证", async () => {
      const mockResponse = {
        credential_id: "cred-123",
        service_id: "schwab",
        credential_type: "username_password",
        created_at: "1704067200",
      };

      vi.spyOn(client, "post").mockResolvedValue(mockResponse);

      const result = await service.createUsernamePassword(
        "schwab",
        "user@example.com",
        "secret",
      );

      expect(result.credential_id).toBe("cred-123");
      expect(client.post).toHaveBeenCalledWith(
        "/credentials",
        {
          service_id: "schwab",
          credential_type: CredentialType.UsernamePassword,
          plaintext_data: {
            username: "user@example.com",
            password: "secret",
          },
          provider: undefined,
          allowed_domains: undefined,
          custom_functions: undefined,
          requires_approval: undefined,
          expires_at: undefined,
        },
        undefined,
      );
    });

    it("createApiKey 应该正确创建凭证", async () => {
      const mockResponse = {
        credential_id: "cred-456",
        service_id: "stripe",
        credential_type: "api_key",
        created_at: "1704067200",
      };

      vi.spyOn(client, "post").mockResolvedValue(mockResponse);

      const result = await service.createApiKey(
        "stripe",
        "sk_live_...",
        "secret_key",
      );

      expect(result.credential_type).toBe("api_key");
      expect(client.post).toHaveBeenCalledWith(
        "/credentials",
        {
          service_id: "stripe",
          credential_type: CredentialType.ApiKey,
          plaintext_data: {
            api_key: "sk_live_...",
            secret_key: "secret_key",
            api_secret: "secret_key",
          },
          provider: undefined,
          allowed_domains: undefined,
          custom_functions: undefined,
          requires_approval: undefined,
          expires_at: undefined,
        },
        undefined,
      );
    });

    it("createApiKey 应该支持不带 secret", async () => {
      vi.spyOn(client, "post").mockResolvedValue({
        credential_id: "cred-789",
        service_id: "openai",
        credential_type: "api_key",
        created_at: "1704067200",
      });

      await service.createApiKey("openai", "sk-...");

      expect(client.post).toHaveBeenCalledWith(
        "/credentials",
        {
          service_id: "openai",
          credential_type: CredentialType.ApiKey,
          plaintext_data: {
            api_key: "sk-...",
          },
          provider: undefined,
          allowed_domains: undefined,
          custom_functions: undefined,
          requires_approval: undefined,
          expires_at: undefined,
        },
        undefined,
      );
    });

    it("createApiKey 应该支持 OKX transport 配置", async () => {
      vi.spyOn(client, "post").mockResolvedValue({
        credential_id: "cred-okx",
        service_id: "okx",
        credential_type: "api_key",
        created_at: "1704067200",
      });

      await service.createApiKey("okx", "ak_test", "sk_test", {
        provider: "okx",
        passphrase: "passphrase",
        allowedDomains: ["www.okx.com:443", "*.okx.com:443"],
        customFunctions: [
          {
            function_name: "build_auth_header",
            function_description: "build bearer header",
            function_body:
              "export default function func(input) { return `Bearer ${input}`; }",
          },
        ],
      });

      expect(client.post).toHaveBeenCalledWith(
        "/credentials",
        {
          service_id: "okx",
          credential_type: CredentialType.ApiKey,
          plaintext_data: {
            api_key: "ak_test",
            secret_key: "sk_test",
            api_secret: "sk_test",
            passphrase: "passphrase",
          },
          provider: "okx",
          allowed_domains: ["www.okx.com:443", "*.okx.com:443"],
          custom_functions: [
            {
              function_name: "build_auth_header",
              function_description: "build bearer header",
              function_body:
                "export default function func(input) { return `Bearer ${input}`; }",
            },
          ],
          requires_approval: undefined,
          expires_at: undefined,
        },
        undefined,
      );
    });

    it("createOAuthRefresh 应该正确创建凭证", async () => {
      const mockResponse = {
        credential_id: "cred-abc",
        service_id: "google",
        credential_type: "oauth_refresh",
        created_at: "1704067200",
      };

      vi.spyOn(client, "post").mockResolvedValue(mockResponse);

      const result = await service.createOAuthRefresh("google", "1//0d...");

      expect(result.credential_type).toBe("oauth_refresh");
      expect(client.post).toHaveBeenCalledWith(
        "/credentials",
        {
          service_id: "google",
          credential_type: CredentialType.OAuthRefresh,
          plaintext_data: {
            refreshToken: "1//0d...",
          },
          provider: undefined,
          allowed_domains: undefined,
          custom_functions: undefined,
          requires_approval: undefined,
          expires_at: undefined,
        },
        undefined,
      );
    });
  });

  describe("获取凭证列表", () => {
    it("应该获取凭证列表", async () => {
      const mockResponse = {
        items: [
          {
            credentialId: "cred-1",
            credentialType: "username_password",
            userIdHash: "hash123",
            serviceId: "schwab",
            tenantId: "tenant1",
            createdAt: "1704067200Z",
            isDeleted: false,
          },
          {
            credentialId: "cred-2",
            credentialType: "api_key",
            userIdHash: "hash123",
            serviceId: "stripe",
            tenantId: "tenant1",
            createdAt: "1704067200Z",
            isDeleted: false,
          },
        ],
        page: 1,
        pageSize: 20,
        total: 2,
        totalPages: 1,
      };

      vi.spyOn(client, "get").mockResolvedValue(mockResponse);

      const result = await service.list();

      expect(result.items).toHaveLength(2);
      expect(result.page).toBe(1);
      expect(result.total).toBe(2);
      expect(client.get).toHaveBeenCalledWith("/credentials", undefined);
    });

    it("应该支持过滤条件", async () => {
      vi.spyOn(client, "get").mockResolvedValue({
        items: [],
        page: 1,
        pageSize: 20,
        total: 0,
        totalPages: 0,
      });

      await service.list({
        serviceId: "schwab",
        credentialType: CredentialType.UsernamePassword,
        includeDeleted: false,
        onlyValid: true,
      });

      expect(client.get).toHaveBeenCalledWith(
        "/credentials?service_id=schwab&credential_type=username_password&include_deleted=false&only_valid=true",
        undefined,
      );
    });

    it("应该支持按服务过滤", async () => {
      vi.spyOn(client, "get").mockResolvedValue({
        items: [],
        page: 1,
        pageSize: 20,
        total: 0,
        totalPages: 0,
      });

      await service.getByService("schwab");

      expect(client.get).toHaveBeenCalledWith(
        "/credentials?service_id=schwab",
        undefined,
      );
    });

    it("应该支持按类型过滤", async () => {
      vi.spyOn(client, "get").mockResolvedValue({
        items: [],
        page: 1,
        pageSize: 20,
        total: 0,
        totalPages: 0,
      });

      await service.getByType(CredentialType.ApiKey);

      expect(client.get).toHaveBeenCalledWith(
        "/credentials?credential_type=api_key",
        undefined,
      );
    });
  });

  describe("获取凭证详情", () => {
    it("应该获取单个凭证", async () => {
      const mockResponse = {
        credentialId: "cred-123",
        serviceId: "schwab",
        credentialType: "username_password",
        createdAt: "1704067200",
        isDeleted: false,
      };

      vi.spyOn(client, "get").mockResolvedValue(mockResponse);

      const result = await service.get("cred-123");

      expect(result.credentialId).toBe("cred-123");
      expect(client.get).toHaveBeenCalledWith(
        "/credentials/cred-123",
        undefined,
      );
    });

    it("应该在凭证不存在时抛出错误", async () => {
      const error = new CredBridgeError(
        CredBridgeErrorCode.NotFound,
        "Credential not found",
        404,
      );

      vi.spyOn(client, "get").mockRejectedValue(error);

      await expect(service.get("nonexistent")).rejects.toThrow(CredBridgeError);
    });
  });

  describe("解密凭证", () => {
    it("应该解密凭证", async () => {
      const mockResponse = {
        credentialId: "cred-123",
        serviceId: "schwab",
        credentialType: "username_password",
        plaintextData: {
          username: "user@example.com",
          password: "secret",
        },
      };

      vi.spyOn(client, "post").mockResolvedValue(mockResponse);

      const result = await service.decrypt("cred-123", "用户登录操作");

      expect(result.plaintextData.username).toBe("user@example.com");
      expect(result.plaintextData.password).toBe("secret");
      expect(client.post).toHaveBeenCalledWith(
        "/credentials/cred-123/decrypt",
        { reason: "用户登录操作" },
        undefined,
      );
    });

    it("应该支持不带理由的解密", async () => {
      vi.spyOn(client, "post").mockResolvedValue({
        credentialId: "cred-123",
        serviceId: "schwab",
        credentialType: "api_key",
        plaintext_data: { apiKey: "sk-..." },
      });

      await service.decrypt("cred-123");

      expect(client.post).toHaveBeenCalledWith(
        "/credentials/cred-123/decrypt",
        { reason: undefined },
        undefined,
      );
    });
  });

  describe("删除凭证", () => {
    it("应该删除凭证", async () => {
      const mockResponse = {
        credentialId: "cred-123",
        deleted: true,
      };

      vi.spyOn(client, "delete").mockResolvedValue(mockResponse);

      const result = await service.delete("cred-123");

      expect(result.deleted).toBe(true);
      expect(client.delete).toHaveBeenCalledWith(
        "/credentials/cred-123",
        undefined,
      );
    });
  });

  describe("检查凭证存在性", () => {
    it("应该在凭证存在时返回 true", async () => {
      vi.spyOn(client, "get").mockResolvedValue({
        credentialId: "cred-123",
        serviceId: "schwab",
        credentialType: "username_password",
        createdAt: "1704067200",
        isDeleted: false,
      });

      const exists = await service.exists("cred-123");

      expect(exists).toBe(true);
    });

    it("应该在凭证不存在时返回 false", async () => {
      const error = new CredBridgeError(
        CredBridgeErrorCode.NotFound,
        "Credential not found",
        404,
      );

      vi.spyOn(client, "get").mockRejectedValue(error);

      const exists = await service.exists("nonexistent");

      expect(exists).toBe(false);
    });
  });
});
