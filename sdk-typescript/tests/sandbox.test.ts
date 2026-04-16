/**
 * CredBridge SDK Sandbox 服务测试
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { SandboxService } from "../src/sandbox.js";
import { CredBridgeClient } from "../src/client.js";
import {
  SessionStatus,
  OperationType,
  OperationStatus,
  CredBridgeError,
  CredBridgeErrorCode,
} from "../src/types.js";

describe("SandboxService", () => {
  const mockBaseUrl = "https://vault.credbridge.io";
  const mockToken =
    "v4.local.eyJzdWIiOiJ0ZW5hbnQxOnVzZXIxIiwiZXhwIjoxNzA0MDY3MjAwLCJpYXQiOjE3MDQwNjM2MDAsImp0aSI6InRva2VuMTIzIiwic2NvcGUiOiJjcmVkZW50aWFsOnJlYWQgY3JlZGVudGlhbDp3cml0ZSJ9.signature";

  let client: CredBridgeClient;
  let service: SandboxService;

  beforeEach(() => {
    client = new CredBridgeClient({
      baseUrl: mockBaseUrl,
      token: mockToken,
    });
    service = new SandboxService(client);
    vi.resetAllMocks();
  });

  describe("创建会话", () => {
    it("应该成功创建会话", async () => {
      const mockResponse = {
        sessionId: "session-123",
        status: SessionStatus.Creating,
        createdAt: "1704067200",
      };

      vi.spyOn(client, "post").mockResolvedValue(mockResponse);

      const result = await service.createSession({
        serviceId: "schwab",
        originalIntent: "login flow",
        credentialId: "cred-123",
        startUrl: "https://www.schwab.com",
        viewportWidth: 1920,
        viewportHeight: 1080,
      });

      expect(result.sessionId).toBe("session-123");
      expect(result.status).toBe(SessionStatus.Creating);
      expect(client.post).toHaveBeenCalledWith(
        "/sandbox/sessions",
        {
          service_id: "schwab",
          original_intent: "login flow",
          credential_id: "cred-123",
          start_url: "https://www.schwab.com",
          viewport_width: 1920,
          viewport_height: 1080,
          user_agent: undefined,
          timeout: undefined,
        },
        undefined,
      );
    });

    it("应该支持自定义用户代理和超时", async () => {
      const mockResponse = {
        sessionId: "session-456",
        status: SessionStatus.Running,
        wsUrl: "wss://vault.credbridge.io/sandbox/session-456",
        createdAt: "1704067200",
      };

      vi.spyOn(client, "post").mockResolvedValue(mockResponse);

      await service.createSession({
        serviceId: "stripe",
        originalIntent: "open dashboard",
        startUrl: "https://dashboard.stripe.com",
        userAgent: "CustomBot/1.0",
        timeout: 60000,
      });

      expect(client.post).toHaveBeenCalledWith(
        "/sandbox/sessions",
        {
          service_id: "stripe",
          original_intent: "open dashboard",
          credential_id: undefined,
          start_url: "https://dashboard.stripe.com",
          viewport_width: undefined,
          viewport_height: undefined,
          user_agent: "CustomBot/1.0",
          timeout: 60000,
        },
        undefined,
      );
    });
  });

  describe("获取会话列表", () => {
    it("应该获取所有会话", async () => {
      const mockResponse = {
        sessions: [
          {
            sessionId: "session-1",
            status: SessionStatus.Running,
            serviceId: "schwab",
            currentUrl: "https://www.schwab.com",
            createdAt: "1704067200",
          },
          {
            sessionId: "session-2",
            status: SessionStatus.Paused,
            serviceId: "stripe",
            createdAt: "1704067100",
          },
        ],
        total: 2,
      };

      vi.spyOn(client, "get").mockResolvedValue(mockResponse);

      const result = await service.listSessions();

      expect(result.sessions).toHaveLength(2);
      expect(result.total).toBe(2);
      expect(client.get).toHaveBeenCalledWith("/sandbox/sessions", undefined);
    });

    it("应该返回空列表当没有会话时", async () => {
      vi.spyOn(client, "get").mockResolvedValue({
        sessions: [],
        total: 0,
      });

      const result = await service.listSessions();

      expect(result.sessions).toHaveLength(0);
      expect(result.total).toBe(0);
    });
  });

  describe("获取单个会话", () => {
    it("应该获取会话详情", async () => {
      const mockResponse = {
        sessionId: "session-123",
        status: SessionStatus.Running,
        serviceId: "schwab",
        credentialId: "cred-123",
        currentUrl: "https://www.schwab.com/accounts",
        pageTitle: "Accounts Summary",
        createdAt: "1704067200",
        lastActivityAt: "1704067500",
      };

      vi.spyOn(client, "get").mockResolvedValue(mockResponse);

      const result = await service.getSession("session-123");

      expect(result.sessionId).toBe("session-123");
      expect(result.status).toBe(SessionStatus.Running);
      expect(result.currentUrl).toBe("https://www.schwab.com/accounts");
      expect(client.get).toHaveBeenCalledWith(
        "/sandbox/sessions/session-123",
        undefined,
      );
    });

    it("应该在会话不存在时抛出错误", async () => {
      const error = new CredBridgeError(
        CredBridgeErrorCode.NotFound,
        "Session not found",
        404,
      );

      vi.spyOn(client, "get").mockRejectedValue(error);

      await expect(service.getSession("nonexistent")).rejects.toThrow(
        CredBridgeError,
      );
    });
  });

  describe("执行操作", () => {
    it("应该执行点击操作", async () => {
      const mockResponse = {
        operationId: "op-123",
        status: OperationStatus.Success,
        result: null,
        executionTimeMs: 150,
      };

      vi.spyOn(client, "post").mockResolvedValue(mockResponse);

      const result = await service.executeOperation("session-123", {
        operationType: OperationType.Click,
        selector: "#login-button",
      });

      expect(result.operationId).toBe("op-123");
      expect(result.status).toBe(OperationStatus.Success);
      expect(client.post).toHaveBeenCalledWith(
        "/sandbox/sessions/session-123/execute",
        {
          operation_type: OperationType.Click,
          description: OperationType.Click,
          parameters: {
            selector: "#login-button",
          },
        },
        undefined,
      );
    });

    it("应该执行填充操作", async () => {
      const mockResponse = {
        operationId: "op-456",
        status: OperationStatus.Success,
        result: null,
        executionTimeMs: 100,
      };

      vi.spyOn(client, "post").mockResolvedValue(mockResponse);

      const result = await service.executeOperation("session-123", {
        operationType: OperationType.Fill,
        selector: "#username",
        value: "user@example.com",
      });

      expect(result.status).toBe(OperationStatus.Success);
      expect(client.post).toHaveBeenCalledWith(
        "/sandbox/sessions/session-123/execute",
        expect.objectContaining({
          operation_type: OperationType.Fill,
          parameters: {
            selector: "#username",
            value: "user@example.com",
          },
        }),
        undefined,
      );
    });

    it("应该支持凭证字段引用填充", async () => {
      vi.spyOn(client, "post").mockResolvedValue({
        operationId: "op-cred",
        status: OperationStatus.Success,
        result: null,
        executionTimeMs: 80,
      });

      await service.executeOperation("session-123", {
        operationType: OperationType.Fill,
        selector: "#api-key",
        value: { $credential: "api_key" },
      });

      expect(client.post).toHaveBeenCalledWith(
        "/sandbox/sessions/session-123/execute",
        expect.objectContaining({
          parameters: {
            selector: "#api-key",
            value: { $credential: "api_key" },
          },
        }),
        undefined,
      );
    });

    it("应该传递纯字符串脚本绑定", async () => {
      vi.spyOn(client, "post").mockResolvedValue({
        operationId: "op-script",
        status: OperationStatus.Success,
        result: null,
        executionTimeMs: 120,
      });

      await service.executeScript(
        "session-123",
        "return bindings.expectedState === document.readyState;",
        {
          expectedState: "complete",
        },
      );

      expect(client.post).toHaveBeenCalledWith(
        "/sandbox/sessions/session-123/execute",
        expect.objectContaining({
          operation_type: OperationType.ExecuteScript,
          parameters: expect.objectContaining({
            script: "return bindings.expectedState === document.readyState;",
            bindings: {
              expectedState: "complete",
            },
          }),
        }),
        undefined,
      );
    });

    it("应该透传后端对脚本凭证绑定的拒绝", async () => {
      const error = new CredBridgeError(
        CredBridgeErrorCode.InvalidRequest,
        "execute_script bindings only support plain strings",
        400,
      );

      vi.spyOn(client, "post").mockRejectedValue(error);

      await expect(
        service.executeOperation("session-123", {
          operationType: OperationType.ExecuteScript,
          script: "return bindings.password;",
          bindings: {
            password: { $credential: "password" },
          } as never,
        }),
      ).rejects.toThrow(CredBridgeError);
    });

    it("应该执行导航操作", async () => {
      const mockResponse = {
        operationId: "op-789",
        status: OperationStatus.Success,
        result: "https://example.com",
        executionTimeMs: 500,
      };

      vi.spyOn(client, "post").mockResolvedValue(mockResponse);

      const result = await service.executeOperation("session-123", {
        operationType: OperationType.Navigate,
        url: "https://example.com",
      });

      expect(result.result).toBe("https://example.com");
    });
  });

  describe("会话控制", () => {
    it("应该暂停会话", async () => {
      const mockResponse = {
        sessionId: "session-123",
        status: SessionStatus.Paused,
        serviceId: "schwab",
        createdAt: "1704067200",
      };

      vi.spyOn(client, "post").mockResolvedValue(mockResponse);

      const result = await service.pauseSession("session-123");

      expect(result.status).toBe(SessionStatus.Paused);
      expect(client.post).toHaveBeenCalledWith(
        "/sandbox/sessions/session-123/pause",
        {},
        undefined,
      );
    });

    it("应该恢复会话", async () => {
      const mockResponse = {
        sessionId: "session-123",
        status: SessionStatus.Running,
        serviceId: "schwab",
        createdAt: "1704067200",
      };

      vi.spyOn(client, "post").mockResolvedValue(mockResponse);

      const result = await service.resumeSession("session-123");

      expect(result.status).toBe(SessionStatus.Running);
      expect(client.post).toHaveBeenCalledWith(
        "/sandbox/sessions/session-123/resume",
        {},
        undefined,
      );
    });

    it("应该关闭会话", async () => {
      const mockResponse = {
        session_id: "session-123",
        success: true,
        status: "closed",
        message: "Session closed successfully",
      };

      vi.spyOn(client, "delete").mockResolvedValue(mockResponse);

      const result = await service.closeSession("session-123");

      expect(result.success).toBe(true);
      expect(result.status).toBe("closed");
      expect(client.delete).toHaveBeenCalledWith(
        "/sandbox/sessions/session-123",
        undefined,
      );
    });
  });

  describe("截图功能", () => {
    it("不暴露后端未支持的截图接口", () => {
      expect("takeScreenshot" in service).toBe(false);
    });
  });

  describe("导出数据", () => {
    it("应该导出JSON数据", async () => {
      const mockResponse = {
        export_id: "export-123",
        data_base64: "e30=",
        format: "json" as const,
        filename: "export_export-123.json",
        size_bytes: 2,
      };

      vi.spyOn(client, "post").mockResolvedValue(mockResponse);

      const result = await service.exportData("session-123", {
        format: "json",
        selectors: [".symbol", ".price"],
      });

      expect(result.format).toBe("json");
      expect(result.exportId).toBe("export-123");
      expect(result.sizeBytes).toBe(2);
      expect(client.post).toHaveBeenCalledWith(
        "/sandbox/sessions/session-123/export",
        {
          format: "json",
          selectors: [".symbol", ".price"],
        },
        undefined,
      );
    });
  });

  describe("后端新增接口", () => {
    it("应该获取操作详情", async () => {
      vi.spyOn(client, "get").mockResolvedValue({
        operation_id: "op-123",
        session_id: "session-123",
        operation_type: "click",
        status: "success",
        started_at: "2026-04-09T00:00:00Z",
        completed_at: "2026-04-09T00:00:01Z",
        execution_time_ms: 1000,
      });

      const result = await service.getOperation("op-123");

      expect(result.operationId).toBe("op-123");
      expect(result.sessionId).toBe("session-123");
      expect(result.executionTimeMs).toBe(1000);
      expect(client.get).toHaveBeenCalledWith(
        "/sandbox/operations/op-123",
        undefined,
      );
    });

    it("应该获取沙箱统计", async () => {
      vi.spyOn(client, "get").mockResolvedValue({
        pool_status: "ready",
        active_sessions: 3,
        warm_instances: 2,
        healthy: true,
        error: undefined,
      });

      const result = await service.getStats();

      expect(result.poolStatus).toBe("ready");
      expect(result.activeSessions).toBe(3);
      expect(result.warmInstances).toBe(2);
      expect(result.healthy).toBe(true);
      expect(client.get).toHaveBeenCalledWith("/sandbox/stats", undefined);
    });
  });

  describe("快捷方法", () => {
    it("navigate 应该调用 executeOperation", async () => {
      const executeSpy = vi
        .spyOn(service, "executeOperation")
        .mockResolvedValue({
          operationId: "op-123",
          status: OperationStatus.Success,
          executionTimeMs: 500,
        });

      await service.navigate("session-123", "https://example.com");

      expect(executeSpy).toHaveBeenCalledWith(
        "session-123",
        { operationType: OperationType.Navigate, url: "https://example.com" },
        undefined,
      );
    });

    it("click 应该调用 executeOperation", async () => {
      const executeSpy = vi
        .spyOn(service, "executeOperation")
        .mockResolvedValue({
          operationId: "op-456",
          status: OperationStatus.Success,
          executionTimeMs: 100,
        });

      await service.click("session-123", "#submit-button");

      expect(executeSpy).toHaveBeenCalledWith(
        "session-123",
        { operationType: OperationType.Click, selector: "#submit-button" },
        undefined,
      );
    });

    it("fill 应该调用 executeOperation", async () => {
      const executeSpy = vi
        .spyOn(service, "executeOperation")
        .mockResolvedValue({
          operationId: "op-789",
          status: OperationStatus.Success,
          executionTimeMs: 50,
        });

      await service.fill("session-123", "#username", "user@example.com");

      expect(executeSpy).toHaveBeenCalledWith(
        "session-123",
        {
          operationType: OperationType.Fill,
          selector: "#username",
          value: "user@example.com",
        },
        undefined,
      );
    });

    it("getText 应该调用 executeOperation", async () => {
      const executeSpy = vi
        .spyOn(service, "executeOperation")
        .mockResolvedValue({
          operationId: "op-abc",
          status: OperationStatus.Success,
          result: "Hello World",
          executionTimeMs: 30,
        });

      const result = await service.getText("session-123", ".greeting");

      expect(executeSpy).toHaveBeenCalledWith(
        "session-123",
        { operationType: OperationType.GetText, selector: ".greeting" },
        undefined,
      );
      expect(result.result).toBe("Hello World");
    });

    it("waitForSelector 应该调用 executeOperation", async () => {
      const executeSpy = vi
        .spyOn(service, "executeOperation")
        .mockResolvedValue({
          operationId: "op-def",
          status: OperationStatus.Success,
          executionTimeMs: 200,
        });

      await service.waitForSelector("session-123", ".loading-complete", {
        timeout: 10000,
        visible: true,
      });

      expect(executeSpy).toHaveBeenCalledWith(
        "session-123",
        {
          operationType: OperationType.WaitForSelector,
          selector: ".loading-complete",
          timeout: 10000,
          waitCondition: { visible: true },
        },
        {},
      );
    });

    it("executeScript 应该调用 executeOperation", async () => {
      const executeSpy = vi
        .spyOn(service, "executeOperation")
        .mockResolvedValue({
          operationId: "op-ghi",
          status: OperationStatus.Success,
          result: "Page Title",
          executionTimeMs: 25,
        });

      const result = await service.executeScript(
        "session-123",
        "return document.title;",
      );

      expect(executeSpy).toHaveBeenCalledWith(
        "session-123",
        {
          operationType: OperationType.ExecuteScript,
          script: "return document.title;",
        },
        undefined,
      );
      expect(result.result).toBe("Page Title");
    });

    it("getAttribute 应该调用 executeOperation", async () => {
      const executeSpy = vi
        .spyOn(service, "executeOperation")
        .mockResolvedValue({
          operationId: "op-jkl",
          status: OperationStatus.Success,
          result: "https://example.com",
          executionTimeMs: 20,
        });

      const result = await service.getAttribute(
        "session-123",
        "a.link",
        "href",
      );

      expect(executeSpy).toHaveBeenCalledWith(
        "session-123",
        {
          operationType: OperationType.GetAttribute,
          selector: "a.link",
          attribute: "href",
        },
        undefined,
      );
      expect(result.result).toBe("https://example.com");
    });
  });

  describe("exists", () => {
    it("应该在会话存在时返回 true", async () => {
      vi.spyOn(client, "get").mockResolvedValue({
        sessionId: "session-123",
        status: SessionStatus.Running,
        serviceId: "schwab",
        createdAt: "1704067200",
      });

      const exists = await service.exists("session-123");

      expect(exists).toBe(true);
    });

    it("应该在会话不存在时返回 false", async () => {
      const error = new CredBridgeError(
        CredBridgeErrorCode.NotFound,
        "Session not found",
        404,
      );

      vi.spyOn(client, "get").mockRejectedValue(error);

      const exists = await service.exists("nonexistent");

      expect(exists).toBe(false);
    });
  });

  describe("waitForStatus", () => {
    it("应该在状态匹配时立即返回", async () => {
      vi.spyOn(client, "get").mockResolvedValue({
        sessionId: "session-123",
        status: SessionStatus.Running,
        serviceId: "schwab",
        createdAt: "1704067200",
      });

      const result = await service.waitForStatus(
        "session-123",
        SessionStatus.Running,
      );

      expect(result.status).toBe(SessionStatus.Running);
    });

    it("应该在超时前等待状态变化", async () => {
      vi.spyOn(client, "get")
        .mockResolvedValueOnce({
          sessionId: "session-123",
          status: SessionStatus.Creating,
          serviceId: "schwab",
          createdAt: "1704067200",
        })
        .mockResolvedValueOnce({
          sessionId: "session-123",
          status: SessionStatus.Creating,
          serviceId: "schwab",
          createdAt: "1704067200",
        })
        .mockResolvedValue({
          sessionId: "session-123",
          status: SessionStatus.Running,
          serviceId: "schwab",
          createdAt: "1704067200",
        });

      const result = await service.waitForStatus(
        "session-123",
        SessionStatus.Running,
        {
          interval: 100,
        },
      );

      expect(result.status).toBe(SessionStatus.Running);
      expect(client.get).toHaveBeenCalledTimes(3);
    });

    it("应该在超时时抛出错误", async () => {
      vi.spyOn(client, "get").mockResolvedValue({
        sessionId: "session-123",
        status: SessionStatus.Creating,
        serviceId: "schwab",
        createdAt: "1704067200",
      });

      await expect(
        service.waitForStatus("session-123", SessionStatus.Running, {
          timeout: 200,
          interval: 100,
        }),
      ).rejects.toThrow("Timeout waiting for session status: running");
    });
  });
});
