/**
 * CredBridge SDK - Sandbox 服务
 *
 * 提供 TEE 安全沙箱中的浏览器自动化功能
 */

import type { CredBridgeClient } from "./client.js";
import {
  type BootstrapPageOptions,
  type BootstrapPageResponse,
  type CreateSessionRequest,
  type CreateSessionResponse,
  type SessionInfo,
  type ExecuteOperationRequest,
  type ExecuteOperationResponse,
  type SessionActionResponse,
  type DomExportRequest,
  type DomExportResponse,
  type ExportDataRequest,
  type ExportDataResponse,
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

function mapBootstrapPageResponse(
  response: ExecuteOperationResponse,
): BootstrapPageResponse {
  const payload =
    response.data && typeof response.data === "object"
      ? (response.data as Record<string, unknown>)
      : response.result && typeof response.result === "object"
        ? (response.result as Record<string, unknown>)
        : undefined;

  if (!payload) {
    return response as BootstrapPageResponse;
  }

  return {
    ...response,
    data: {
      injectedScripts: Array.isArray(payload.injected_scripts)
        ? payload.injected_scripts.filter(
            (item): item is string => typeof item === "string",
          )
        : [],
      finalUrl:
        typeof payload.final_url === "string" ? payload.final_url : undefined,
      title: typeof payload.title === "string" ? payload.title : undefined,
      waitSatisfied:
        typeof payload.wait_satisfied === "boolean"
          ? payload.wait_satisfied
          : undefined,
      diagnostics:
        payload.diagnostics && typeof payload.diagnostics === "object"
          ? (payload.diagnostics as Record<string, unknown>)
          : undefined,
    },
    result: {
      injectedScripts: Array.isArray(payload.injected_scripts)
        ? payload.injected_scripts.filter(
            (item): item is string => typeof item === "string",
          )
        : [],
      finalUrl:
        typeof payload.final_url === "string" ? payload.final_url : undefined,
      title: typeof payload.title === "string" ? payload.title : undefined,
      waitSatisfied:
        typeof payload.wait_satisfied === "boolean"
          ? payload.wait_satisfied
          : undefined,
      diagnostics:
        payload.diagnostics && typeof payload.diagnostics === "object"
          ? (payload.diagnostics as Record<string, unknown>)
          : undefined,
    },
  };
}

/**
 * Sandbox 服务
 *
 * 管理 TEE 安全沙箱中的浏览器会话和自动化操作
 */
export class SandboxService {
  private client: CredBridgeClient;

  constructor(client: CredBridgeClient) {
    this.client = client;
  }

  /**
   * 创建新的浏览器会话
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
   *   startUrl: 'https://www.schwab.com',
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
        start_url: request.startUrl,
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
   * console.log('Current URL:', session.currentUrl);
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
   *   operationType: OperationType.Click,
   *   selector: '#login-button',
   * });
   * ```
   */
  public async executeOperation(
    sessionId: string,
    request: ExecuteOperationRequest,
    options?: RequestOptions,
  ): Promise<ExecuteOperationResponse> {
    const parameters: Record<string, unknown> = { ...(request.parameters ?? {}) };
    if (request.selector !== undefined) parameters.selector = request.selector;
    if (request.value !== undefined) parameters.value = request.value;
    if (request.url !== undefined) parameters.url = request.url;
    if (request.method !== undefined) parameters.method = request.method;
    if (request.headers !== undefined) parameters.headers = request.headers;
    if (request.body !== undefined) parameters.body = request.body;
    if (request.script !== undefined) parameters.script = request.script;
    if (request.bindings !== undefined) parameters.bindings = request.bindings;
    if (request.attribute !== undefined)
      parameters.attribute = request.attribute;
    if (request.timeout !== undefined) parameters.timeout_ms = request.timeout;
    if (request.waitCondition !== undefined)
      parameters.wait_condition = request.waitCondition;

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
   * console.log('Session resumed:', session.status === SessionStatus.Running);
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

  public async exportDom(
    sessionId: string,
    request: DomExportRequest = {},
    options?: RequestOptions,
  ): Promise<DomExportResponse> {
    const response = await this.client.post<{
      operation_id: string;
      success: boolean;
      format: "html" | "text" | "json";
      data?: unknown;
      truncated: boolean;
      error?: string;
      execution_time_ms: number;
    }>(`/sandbox/sessions/${sessionId}/dom-export`, {
      root_selector: request.rootSelector,
      format: request.format,
      include_text: request.includeText,
      include_metadata: request.includeMetadata,
      extra_sensitive_selectors: request.extraSensitiveSelectors,
      max_bytes: request.maxBytes,
    }, options);

    return {
      operationId: response.operation_id,
      success: response.success,
      format: response.format,
      data: response.data,
      truncated: response.truncated,
      error: response.error,
      executionTimeMs: response.execution_time_ms,
    };
  }

  /**
   * 导出页面数据
   *
   * @param sessionId - 会话ID
   * @param request - 导出数据请求
   * @param options - 请求选项
   * @returns 导出结果
   *
   * @example
   * ```typescript
   * const exportResult = await sdk.sandbox.exportData('session-123', {
   *   format: 'json',
   *   selector: '.data-table',
   *   extractionRules: [
   *     { name: 'symbol', selector: '.symbol' },
   *     { name: 'price', selector: '.price' },
   *   ],
   * });
   * ```
   */
  public async exportData(
    sessionId: string,
    request: ExportDataRequest,
    options?: RequestOptions,
  ): Promise<ExportDataResponse> {
    const selectors =
      request.selectors ??
      (request.selector !== undefined ? [request.selector] : undefined) ??
      request.extractionRules?.map((rule) => rule.selector) ??
      [];
    const response = await this.client.post<{
      export_id: string;
      data_base64: string;
      format: "json" | "csv" | "pdf";
      filename: string;
      size_bytes: number;
    }>(
      `/sandbox/sessions/${sessionId}/export`,
      {
        format: request.format,
        selectors,
      },
      options,
    );
    return {
      exportId: response.export_id,
      dataBase64: response.data_base64,
      format: response.format,
      filename: response.filename,
      sizeBytes: response.size_bytes,
    };
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

  // ============================================================================
  // 快捷方法
  // ============================================================================

  /**
   * 导航到URL（快捷方法）
   *
   * @param sessionId - 会话ID
   * @param url - 目标URL
   * @param options - 请求选项
   * @returns 操作结果
   *
   * @example
   * ```typescript
   * await sdk.sandbox.navigate('session-123', 'https://example.com');
   * ```
   */
  public async navigate(
    sessionId: string,
    url: string,
    options?: RequestOptions,
  ): Promise<ExecuteOperationResponse> {
    return this.executeOperation(
      sessionId,
      { operationType: OperationType.Navigate, url },
      options,
    );
  }

  /**
   * 点击元素（快捷方法）
   *
   * @param sessionId - 会话ID
   * @param selector - CSS选择器
   * @param options - 请求选项
   * @returns 操作结果
   *
   * @example
   * ```typescript
   * await sdk.sandbox.click('session-123', '#submit-button');
   * ```
   */
  public async click(
    sessionId: string,
    selector: string,
    options?: RequestOptions,
  ): Promise<ExecuteOperationResponse> {
    return this.executeOperation(
      sessionId,
      { operationType: OperationType.Click, selector },
      options,
    );
  }

  /**
   * 填充表单字段（快捷方法）
   *
   * @param sessionId - 会话ID
   * @param selector - CSS选择器
   * @param value - 输入值
   * @param options - 请求选项
   * @returns 操作结果
   *
   * @example
   * ```typescript
   * await sdk.sandbox.fill('session-123', '#username', 'user@example.com');
   * await sdk.sandbox.fill('session-123', '#password', { $credential: 'password' });
   * ```
   */
  public async fill(
    sessionId: string,
    selector: string,
    value: ExecuteOperationRequest["value"],
    options?: RequestOptions,
  ): Promise<ExecuteOperationResponse> {
    return this.executeOperation(
      sessionId,
      { operationType: OperationType.Fill, selector, value },
      options,
    );
  }

  /**
   * 获取元素文本（快捷方法）
   *
   * @param sessionId - 会话ID
   * @param selector - CSS选择器
   * @param options - 请求选项
   * @returns 包含文本的操作结果
   *
   * @example
   * ```typescript
   * const result = await sdk.sandbox.getText('session-123', '.price-display');
   * console.log('Price:', result.data);
   * ```
   */
  public async getText(
    sessionId: string,
    selector: string,
    options?: RequestOptions,
  ): Promise<ExecuteOperationResponse> {
    return this.executeOperation(
      sessionId,
      { operationType: OperationType.GetText, selector },
      options,
    );
  }

  /**
   * 获取元素属性（快捷方法）
   *
   * @param sessionId - 会话ID
   * @param selector - CSS选择器
   * @param attribute - 属性名
   * @param options - 请求选项
   * @returns 包含属性值的操作结果
   *
   * @example
   * ```typescript
   * const result = await sdk.sandbox.getAttribute('session-123', 'a.link', 'href');
   * console.log('Link:', result.data);
   * ```
   */
  public async getAttribute(
    sessionId: string,
    selector: string,
    attribute: string,
    options?: RequestOptions,
  ): Promise<ExecuteOperationResponse> {
    return this.executeOperation(
      sessionId,
      { operationType: OperationType.GetAttribute, selector, attribute },
      options,
    );
  }

  /**
   * 执行JavaScript脚本（快捷方法）
   *
   * @param sessionId - 会话ID
   * @param script - JavaScript代码
   * @param options - 请求选项
   * @returns 包含执行结果的操作结果
   *
   * @example
   * ```typescript
   * const result = await sdk.sandbox.executeScript('session-123', `
   *   return {
   *     title: document.title,
   *     ready: document.readyState,
   *   };
   * `, {
   *   expectedState: 'complete',
   * });
   * console.log('Result:', result.data);
   * ```
   *
   * `bindings` 仅支持普通字符串。credential 引用必须通过 `fill` 等受控宿主操作使用。
   */
  public async executeScript(
    sessionId: string,
    script: string,
    bindings?: ExecuteOperationRequest["bindings"],
    options?: RequestOptions,
  ): Promise<ExecuteOperationResponse> {
    return this.executeOperation(
      sessionId,
      { operationType: OperationType.ExecuteScript, script, bindings },
      options,
    );
  }

  /**
   * Bootstrap a Rocket Loader-style page without exposing raw script execution.
   *
   * This operation only replays approved external bundles. It does not consume
   * credentials; keep secret usage in controlled host operations such as `fill`.
   *
   * @example
   * ```typescript
   * await sdk.sandbox.navigate('session-123', 'https://dashboard.zk.me/login');
   * await sdk.sandbox.bootstrapPage('session-123', {
   *   mode: 'rocket_loader',
   *   waitSelector: 'input[name=email]',
   *   waitTimeoutMs: 15000,
   * });
   * ```
   */
  public async bootstrapPage(
    sessionId: string,
    request: BootstrapPageOptions = {},
    options?: RequestOptions,
  ): Promise<BootstrapPageResponse> {
    const response = await this.executeOperation(
      sessionId,
      {
        operationType: OperationType.BootstrapPage,
        parameters: {
          mode: request.mode ?? "rocket_loader",
          script_selectors: request.scriptSelectors,
          include_plain_scripts: request.includePlainScripts,
          wait_selector: request.waitSelector,
          wait_timeout_ms: request.waitTimeoutMs,
        },
      },
      options,
    );

    return mapBootstrapPageResponse(response);
  }

  /**
   * 等待元素出现（快捷方法）
   *
   * @param sessionId - 会话ID
   * @param selector - CSS选择器
   * @param options - 请求选项（包含timeout）
   * @returns 操作结果
   *
   * @example
   * ```typescript
   * await sdk.sandbox.waitForSelector('session-123', '.loading-complete', {
   *   timeout: 10000,
   * });
   * ```
   */
  public async waitForSelector(
    sessionId: string,
    selector: string,
    options?: RequestOptions & { timeout?: number; visible?: boolean },
  ): Promise<ExecuteOperationResponse> {
    const { timeout, visible, ...requestOptions } = options || {};
    return this.executeOperation(
      sessionId,
      {
        operationType: OperationType.WaitForSelector,
        selector,
        timeout,
        waitCondition: visible !== undefined ? { visible } : undefined,
      },
      requestOptions,
    );
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
   * const session = await sdk.sandbox.waitForStatus('session-123', SessionStatus.Running, {
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
