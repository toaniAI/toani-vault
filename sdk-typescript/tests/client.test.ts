/**
 * CredBridge SDK 客户端测试
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { CredBridgeClient } from "../src/client.js";
import {
  CredBridgeError,
  CredBridgeErrorCode,
  SdkEventType,
} from "../src/types.js";

describe("CredBridgeClient", () => {
  const mockBaseUrl = "https://vault.credbridge.io";
  const mockToken =
    "v4.local.eyJzdWIiOiJ0ZW5hbnQxOnVzZXIxIiwiZXhwIjoxNzA0MDY3MjAwLCJpYXQiOjE3MDQwNjM2MDAsImp0aSI6InRva2VuMTIzIiwic2NvcGUiOiJjcmVkZW50aWFsOnJlYWQgY3JlZGVudGlhbDp3cml0ZSJ9.signature";

  beforeEach(() => {
    vi.resetAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe("初始化", () => {
    it("应该使用默认值正确初始化", () => {
      const client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token: mockToken,
      });

      const config = client.getConfig();
      expect(config.baseUrl).toBe(mockBaseUrl);
      expect(config.token).toBe(mockToken);
      expect(config.timeout).toBe(30000);
      expect(config.maxRetries).toBe(3);
      expect(config.autoRefreshToken).toBe(true);
    });

    it("应该允许自定义配置", () => {
      const client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token: mockToken,
        timeout: 60000,
        maxRetries: 5,
        autoRefreshToken: false,
      });

      const config = client.getConfig();
      expect(config.timeout).toBe(60000);
      expect(config.maxRetries).toBe(5);
      expect(config.autoRefreshToken).toBe(false);
    });

    it("应该正确解析 Token 信息", () => {
      const client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token: mockToken,
      });

      const tokenInfo = client.getTokenInfo();
      expect(tokenInfo).toBeDefined();
      expect(tokenInfo?.tokenId).toBe("token123");
      expect(tokenInfo?.tenantId).toBe("tenant1");
      expect(tokenInfo?.userId).toBe("user1");
      expect(tokenInfo?.scopes).toContain("credential:read");
      expect(tokenInfo?.scopes).toContain("credential:write");
    });
  });

  describe("Token 管理", () => {
    it("应该能够更新 Token", () => {
      const client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
      });

      expect(client.getToken()).toBeUndefined();

      client.setToken(mockToken);
      expect(client.getToken()).toBe(mockToken);
    });

    it("应该检测 Token 是否过期", () => {
      // 使用已过期 Token（exp 在过去）
      const expiredToken =
        "v4.local.eyJzdWIiOiJ0ZW5hbnQxOnVzZXIxIiwiZXhwIjoxNzAwMDAwMDAwLCJpYXQiOjE3MDAwMDAwMDAsImp0aSI6InRva2VuMTIzIiwic2NvcGUiOiJjcmVkZW50aWFsOnJlYWQifQ.signature";

      const client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token: expiredToken,
      });

      expect(client.isTokenExpired()).toBe(true);
    });

    it("应该检测 Token 是否即将过期", () => {
      // Token 将在 3 分钟后过期
      const expiringSoon = Math.floor(Date.now() / 1000) + 180;
      const tokenPayload = Buffer.from(
        JSON.stringify({
          sub: "tenant1:user1",
          exp: expiringSoon,
          iat: Math.floor(Date.now() / 1000),
          jti: "token123",
          scope: "credential:read",
        }),
      ).toString("base64url");
      const token = `v4.local.${tokenPayload}.signature`;

      const client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token,
        tokenRefreshBuffer: 5 * 60 * 1000, // 5分钟缓冲
      });

      expect(client.isTokenExpiringSoon()).toBe(true);
    });
  });

  describe("事件系统", () => {
    it("应该支持事件监听和触发", () => {
      const client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token: mockToken,
      });

      const listener = vi.fn();
      const unsubscribe = client.on(SdkEventType.RequestStart, listener);

      // 触发事件（通过发送请求）
      client.get("/test").catch(() => {}); // 忽略错误

      // 取消监听
      unsubscribe();
    });

    it("应该能够取消事件监听", () => {
      const client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
      });

      const listener = vi.fn();
      const unsubscribe = client.on(SdkEventType.RequestStart, listener);

      // 取消监听
      unsubscribe();

      // 验证监听器已被移除
      expect(
        client["eventListeners"].get(SdkEventType.RequestStart)?.has(listener),
      ).toBe(false);
    });
  });

  describe("HTTP 请求", () => {
    it("应该发送带有正确头的 GET 请求", async () => {
      const mockFetch = vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        headers: new Map([["content-type", "application/json"]]),
        json: async () => ({
          success: true,
          data: { id: "123" },
          meta: { requestId: "req_123", timestamp: new Date().toISOString() },
        }),
      } as unknown as Response);

      global.fetch = mockFetch;

      const client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token: mockToken,
      });

      await client.get("/test");

      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining("/api/v1/test"),
        expect.objectContaining({
          method: "GET",
          headers: expect.objectContaining({
            Authorization: `Bearer ${mockToken}`,
            "Content-Type": "application/json",
          }),
        }),
      );
    });

    it("应该发送带有正确体的 POST 请求", async () => {
      const mockFetch = vi.fn().mockResolvedValue({
        ok: true,
        status: 201,
        headers: new Map([["content-type", "application/json"]]),
        json: async () => ({
          success: true,
          data: { id: "123" },
          meta: { requestId: "req_123", timestamp: new Date().toISOString() },
        }),
      } as unknown as Response);

      global.fetch = mockFetch;

      const client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token: mockToken,
      });

      const body = { name: "test" };
      await client.post("/test", body);

      expect(mockFetch).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          method: "POST",
          body: JSON.stringify(body),
        }),
      );
    });

    it("应该在 404 时抛出 NotFound 错误", async () => {
      const mockFetch = vi.fn().mockResolvedValue({
        ok: false,
        status: 404,
        headers: new Map([["content-type", "application/json"]]),
        json: async () => ({
          success: false,
          error: {
            code: "not_found",
            message: "Credential not found",
          },
          meta: { requestId: "req_123", timestamp: new Date().toISOString() },
        }),
      } as unknown as Response);

      global.fetch = mockFetch;

      const client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token: mockToken,
      });

      await expect(client.get("/credentials/nonexistent")).rejects.toThrow(
        CredBridgeError,
      );
      await expect(
        client.get("/credentials/nonexistent"),
      ).rejects.toMatchObject({
        code: CredBridgeErrorCode.NotFound,
        statusCode: 404,
      });
    });

    it("应该在 401 时抛出 Unauthorized 错误", async () => {
      const mockFetch = vi.fn().mockResolvedValue({
        ok: false,
        status: 401,
        headers: new Map([["content-type", "application/json"]]),
        json: async () => ({
          success: false,
          error: {
            code: "unauthorized",
            message: "Invalid token",
          },
          meta: { requestId: "req_123", timestamp: new Date().toISOString() },
        }),
      } as unknown as Response);

      global.fetch = mockFetch;

      const client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token: mockToken,
      });

      await expect(client.get("/test")).rejects.toMatchObject({
        code: CredBridgeErrorCode.Unauthorized,
        statusCode: 401,
      });
    });

    it("应该解析后端顶层 error/message 格式的错误响应", async () => {
      const mockFetch = vi.fn().mockResolvedValue({
        ok: false,
        status: 404,
        headers: new Map([["content-type", "application/json"]]),
        json: async () => ({
          success: false,
          error: "not_found",
          message: "Session not found",
        }),
      } as unknown as Response);

      global.fetch = mockFetch;

      const client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token: mockToken,
      });

      await expect(client.get("/sandbox/sessions/missing")).rejects.toMatchObject({
        code: CredBridgeErrorCode.NotFound,
        statusCode: 404,
        message: "Session not found",
      });
    });

    it("应该在超时时抛出 Timeout 错误", async () => {
      const abortError = new Error("The operation was aborted");
      abortError.name = "AbortError";

      const mockFetch = vi.fn().mockImplementation(() => {
        return new Promise((_, reject) => {
          setTimeout(() => reject(abortError), 100);
        });
      });

      global.fetch = mockFetch;

      const client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token: mockToken,
        timeout: 50,
      });

      await expect(client.get("/test")).rejects.toMatchObject({
        code: CredBridgeErrorCode.Timeout,
      });
    });

    it("应该在网络错误时抛出 NetworkError", async () => {
      const mockFetch = vi
        .fn()
        .mockRejectedValue(new TypeError("fetch failed"));

      global.fetch = mockFetch;

      const client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token: mockToken,
      });

      await expect(client.get("/test")).rejects.toMatchObject({
        code: CredBridgeErrorCode.NetworkError,
      });
    });
  });

  describe("重试逻辑", () => {
    it("应该在可重试错误时进行重试", async () => {
      const mockFetch = vi
        .fn()
        .mockRejectedValueOnce(new TypeError("fetch failed"))
        .mockResolvedValueOnce({
          ok: true,
          status: 200,
          headers: new Map([["content-type", "application/json"]]),
          json: async () => ({
            success: true,
            data: { id: "123" },
            meta: { requestId: "req_123", timestamp: new Date().toISOString() },
          }),
        } as unknown as Response);

      global.fetch = mockFetch;

      const client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token: mockToken,
        maxRetries: 3,
      });

      const result = await client.get("/test");
      expect(result).toEqual({ id: "123" });
      expect(mockFetch).toHaveBeenCalledTimes(2);
    });

    it("应该在 skipRetry 时跳过重试", async () => {
      const mockFetch = vi
        .fn()
        .mockRejectedValue(new TypeError("fetch failed"));

      global.fetch = mockFetch;

      const client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token: mockToken,
        maxRetries: 3,
      });

      await expect(client.get("/test", { skipRetry: true })).rejects.toThrow();

      expect(mockFetch).toHaveBeenCalledTimes(1);
    });

    it("应该在非可重试错误时不重试", async () => {
      const mockFetch = vi.fn().mockResolvedValue({
        ok: false,
        status: 400,
        headers: new Map([["content-type", "application/json"]]),
        json: async () => ({
          success: false,
          error: {
            code: "invalid_request",
            message: "Invalid parameters",
          },
          meta: { requestId: "req_123", timestamp: new Date().toISOString() },
        }),
      } as unknown as Response);

      global.fetch = mockFetch;

      const client = new CredBridgeClient({
        baseUrl: mockBaseUrl,
        token: mockToken,
        maxRetries: 3,
      });

      await expect(client.get("/test")).rejects.toThrow();
      expect(mockFetch).toHaveBeenCalledTimes(1);
    });
  });

  describe("错误处理", () => {
    it("isRetryable 应该正确判断", () => {
      const networkError = new CredBridgeError(
        CredBridgeErrorCode.NetworkError,
        "Network error",
      );
      expect(networkError.isRetryable()).toBe(true);

      const timeoutError = new CredBridgeError(
        CredBridgeErrorCode.Timeout,
        "Timeout",
      );
      expect(timeoutError.isRetryable()).toBe(true);

      const authError = new CredBridgeError(
        CredBridgeErrorCode.Unauthorized,
        "Unauthorized",
      );
      expect(authError.isRetryable()).toBe(false);

      const notFoundError = new CredBridgeError(
        CredBridgeErrorCode.NotFound,
        "Not found",
      );
      expect(notFoundError.isRetryable()).toBe(false);
    });

    it("isAuthError 应该正确判断", () => {
      const authError = new CredBridgeError(
        CredBridgeErrorCode.Unauthorized,
        "Unauthorized",
      );
      expect(authError.isAuthError()).toBe(true);

      const tokenExpiredError = new CredBridgeError(
        CredBridgeErrorCode.TokenExpired,
        "Token expired",
      );
      expect(tokenExpiredError.isAuthError()).toBe(true);

      const networkError = new CredBridgeError(
        CredBridgeErrorCode.NetworkError,
        "Network error",
      );
      expect(networkError.isAuthError()).toBe(false);
    });
  });
});
