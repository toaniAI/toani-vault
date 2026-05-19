/**
 * CredBridge SDK - Sandbox 服务
 *
 * 提供 TEE 安全沙箱中的受控执行功能
 */

import type { CredBridgeClient } from "./client.js";
import {
  type CreateSessionRequest,
  type CreateSessionResponse,
  type SessionInfo,
  type ExecuteOperationRequest,
  type ExecuteOperationResponse,
  type SessionActionResponse,
  type ListSessionsResponse,
  type RequestOptions,
  type SandboxOperationInfo,
  type SandboxStats,
  SessionStatus,
  OperationType,
  OperationStatus,
} from "./types.js";

interface SandboxOperationInfoApi {
  operation_id: string;
  session_id: string;
  operation_type: string;
  status: string;
  started_at: string;
  completed_at?: string;
  execution_time_ms?: number;
}

interface SandboxStatsApi {
  pool_status: string;
  active_sessions: number;
  warm_instances: number;
  healthy: boolean;
  error?: string;
}

interface ExecuteOperationResponseApi {
  operation_id?: string;
  operationId?: string;
  success?: boolean;
  status?: OperationStatus;
  data?: unknown;
  result?: unknown;
  error?: string;
  execution_time_ms?: number;
  executionTimeMs?: number;
}

interface SessionActionResponseApi {
  session_id?: string;
  sessionId?: string;
  success?: boolean;
  closed?: boolean;
  status: string;
  message?: string;
}

function mapSessionActionResponse(
  response: SessionActionResponseApi,
): SessionActionResponse {
  return {
    sessionId: response.session_id ?? response.sessionId ?? "",
    success: response.success ?? response.closed ?? false,
    closed: response.closed ?? response.success,
    status: response.status,
    message: response.message ?? "",
  };
}

/**
 * Sandbox 服务
 *
 * 管理 TEE 安全沙箱中的会话和受控操作
 */
export class SandboxService {
  private client: CredBridgeClient;

  constructor(client: CredBridgeClient) {
    this.client = client;
  }

  /**
   * 创建新的沙箱会话
   *
   * @param request - 创建会话请求
   * @param options - 请求选项
   * @returns 创建的会话信息
   *
   * @example
   * ```typescript
   * const session = await sdk.sandbox.createSession({
   *   serviceId: 'schwab',
   *   credentialId: 'cred-123',
   *   viewportWidth: 1920,
   *   viewportHeight: 1080,
   * });
   * console.log('Session created:', session.sessionId);
   * ```
   */
  public async createSession(
    request: CreateSessionRequest,
    options?: RequestOptions,
  ): Promise<CreateSessionResponse> {
    return this.client.post<CreateSessionResponse>(
      "/sandbox/sessions",
      {
        service_id: request.serviceId,
        original_intent: request.originalIntent,
        credential_id: request.credentialId,
        viewport_width: request.viewportWidth,
        viewport_height: request.viewportHeight,
        user_agent: request.userAgent,
        timeout: request.timeout,
      },
      options,
    );
  }

  /**
   * 获取所有会话列表
   *
   * @param options - 请求选项
   * @returns 会话列表
   *
   * @example
   * ```typescript
   * const { sessions, total } = await sdk.sandbox.listSessions();
   * console.log(`Total sessions: ${total}`);
   * ```
   */
  public async listSessions(
    options?: RequestOptions,
  ): Promise<{ sessions: SessionInfo[]; total: number }> {
    const response = await this.client.get<ListSessionsResponse>(
      "/sandbox/sessions",
      options,
    );
    return { sessions: response.sessions, total: response.total };
  }

  /**
   * 获取单个会话详情
   *
   * @param sessionId - 会话ID
   * @param options - 请求选项
   * @returns 会话详情
   *
   * @example
   * ```typescript
   * const session = await sdk.sandbox.getSession('session-123');
   * console.log('Status:', session.status);
   * ```
   */
  public async getSession(
    sessionId: string,
    options?: RequestOptions,
  ): Promise<SessionInfo> {
    return this.client.get<SessionInfo>(
      `/sandbox/sessions/${sessionId}`,
      options,
    );
  }

  /**
   * 在会话中执行操作
   *
   * @param sessionId - 会话ID
   * @param request - 执行操作请求
   * @param options - 请求选项
   * @returns 操作结果
   *
   * @example
   * ```typescript
   * const result = await sdk.sandbox.executeOperation('session-123', {
   *   operationType: OperationType.HttpRequest,
   *   method: 'GET',
   *   parameters: { url: 'https://api.example.com/health' },
   * });
   * ```
   */
  public async executeOperation(
    sessionId: string,
    request: ExecuteOperationRequest,
    options?: RequestOptions,
  ): Promise<ExecuteOperationResponse> {
    const parameters: Record<string, unknown> = { ...(request.parameters ?? {}) };
    if (request.method !== undefined) parameters.method = request.method;
    if (request.headers !== undefined) parameters.headers = request.headers;
    if (request.body !== undefined) parameters.body = request.body;
    if (request.timeout !== undefined) parameters.timeout_ms = request.timeout;

    const response = await this.client.post<ExecuteOperationResponseApi>(
      `/sandbox/sessions/${sessionId}/execute`,
      {
        operation_type: request.operationType,
        description: request.description ?? request.operationType,
        parameters,
      },
      options,
    );
    const data = response.data ?? response.result;
    const success =
      response.success ??
      (response.status !== undefined
        ? response.status === OperationStatus.Success
        : response.error === undefined);
    return {
      operationId: response.operation_id ?? response.operationId ?? "",
      success,
      status: response.status,
      data,
      result: data,
      error: response.error,
      executionTimeMs:
        response.execution_time_ms ?? response.executionTimeMs ?? 0,
    };
  }

  /**
   * 暂停会话
   *
   * @param sessionId - 会话ID
   * @param options - 请求选项
   * @returns 更新后的会话信息
   *
   * @example
   * ```typescript
   * const session = await sdk.sandbox.pauseSession('session-123');
   * console.log('Session paused:', session.status === SessionStatus.Paused);
   * ```
   */
  public async pauseSession(
    sessionId: string,
    options?: RequestOptions,
  ): Promise<SessionActionResponse> {
    const response = await this.client.post<SessionActionResponseApi>(
      `/sandbox/sessions/${sessionId}/pause`,
      {},
      options,
    );
    return mapSessionActionResponse(response);
  }

  /**
   * 恢复会话
   *
   * @param sessionId - 会话ID
   * @param options - 请求选项
   * @returns 更新后的会话信息
   *
   * @example
   * ```typescript
   * const session = await sdk.sandbox.resumeSession('session-123');
   * console.log('Session resumed:', session.status === SessionStatus.Ready);
   * ```
   */
  public async resumeSession(
    sessionId: string,
    options?: RequestOptions,
  ): Promise<SessionActionResponse> {
    const response = await this.client.post<SessionActionResponseApi>(
      `/sandbox/sessions/${sessionId}/resume`,
      {},
      options,
    );
    return mapSessionActionResponse(response);
  }

  /**
   * 关闭会话
   *
   * @param sessionId - 会话ID
   * @param options - 请求选项
   * @returns 关闭结果
   *
   * @example
   * ```typescript
   * await sdk.sandbox.closeSession('session-123');
   * console.log('Session closed');
   * ```
   */
  public async closeSession(
    sessionId: string,
    options?: RequestOptions,
  ): Promise<SessionActionResponse> {
    const response = await this.client.delete<SessionActionResponseApi>(
      `/sandbox/sessions/${sessionId}`,
      options,
    );
    return mapSessionActionResponse(response);
  }

  /**
   * 获取操作详情
   */
  public async getOperation(
    operationId: string,
    options?: RequestOptions,
  ): Promise<SandboxOperationInfo> {
    const response = await this.client.get<SandboxOperationInfoApi>(
      `/sandbox/operations/${operationId}`,
      options,
    );

    return {
      operationId: response.operation_id,
      sessionId: response.session_id,
      operationType: response.operation_type,
      status: response.status,
      startedAt: response.started_at,
      completedAt: response.completed_at,
      executionTimeMs: response.execution_time_ms,
    };
  }

  /**
   * 获取 Sandbox 统计
   */
  public async getStats(options?: RequestOptions): Promise<SandboxStats> {
    const response = await this.client.get<SandboxStatsApi>(
      "/sandbox/stats",
      options,
    );
    return {
      poolStatus: response.pool_status,
      activeSessions: response.active_sessions,
      warmInstances: response.warm_instances,
      healthy: response.healthy,
      error: response.error,
    };
  }

  /**
   * 检查会话是否存在
   *
   * @param sessionId - 会话ID
   * @param options - 请求选项
   * @returns 是否存在
   *
   * @example
   * ```typescript
   * const exists = await sdk.sandbox.exists('session-123');
   * console.log('Session exists:', exists);
   * ```
   */
  public async exists(
    sessionId: string,
    options?: RequestOptions,
  ): Promise<boolean> {
    try {
      await this.getSession(sessionId, { ...options, skipRetry: true });
      return true;
    } catch (error) {
      return false;
    }
  }

  /**
   * 等待会话达到指定状态
   *
   * @param sessionId - 会话ID
   * @param status - 目标状态
   * @param options - 选项（包含超时时间和轮询间隔）
   * @returns 会话信息
   *
   * @example
   * ```typescript
   * const session = await sdk.sandbox.waitForStatus('session-123', SessionStatus.Ready, {
   *   timeout: 30000,
   *   interval: 1000,
   * });
   * ```
   */
  public async waitForStatus(
    sessionId: string,
    status: SessionStatus,
    options?: {
      timeout?: number;
      interval?: number;
      requestOptions?: RequestOptions;
    },
  ): Promise<SessionInfo> {
    const timeout = options?.timeout ?? 30000;
    const interval = options?.interval ?? 1000;
    const startTime = Date.now();

    while (Date.now() - startTime < timeout) {
      const session = await this.getSession(sessionId, options?.requestOptions);
      if (session.status === status) {
        return session;
      }
      await new Promise((resolve) => setTimeout(resolve, interval));
    }

    throw new Error(`Timeout waiting for session status: ${status}`);
  }
}

export { SessionStatus, OperationType, OperationStatus };
