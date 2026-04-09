/**
 * CredBridge SDK - Sandbox 服务
 *
 * 提供 TEE 安全沙箱中的浏览器自动化功能
 */

import type { CredBridgeClient } from './client.js';
import {
  type CreateSessionRequest,
  type CreateSessionResponse,
  type SessionInfo,
  type ExecuteOperationRequest,
  type ExecuteOperationResponse,
  type ScreenshotOptions,
  type ScreenshotResponse,
  type ExportDataRequest,
  type ExportDataResponse,
  type ListSessionsResponse,
  type RequestOptions,
  type SandboxOperationInfo,
  type SandboxStats,
  SessionStatus,
  OperationType,
  OperationStatus,
} from './types.js';

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
    options?: RequestOptions
  ): Promise<CreateSessionResponse> {
    return this.client.post<CreateSessionResponse>('/sandbox/sessions', {
      service_id: request.serviceId,
      original_intent: request.originalIntent,
      credential_id: request.credentialId,
      start_url: request.startUrl,
      viewport_width: request.viewportWidth,
      viewport_height: request.viewportHeight,
      user_agent: request.userAgent,
      timeout: request.timeout,
    }, options);
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
    options?: RequestOptions
  ): Promise<{ sessions: SessionInfo[]; total: number }> {
    const response = await this.client.get<ListSessionsResponse>('/sandbox/sessions', options);
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
    options?: RequestOptions
  ): Promise<SessionInfo> {
    return this.client.get<SessionInfo>(`/sandbox/sessions/${sessionId}`, options);
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
    options?: RequestOptions
  ): Promise<ExecuteOperationResponse> {
    const parameters: Record<string, unknown> = {};
    if (request.selector !== undefined) parameters.selector = request.selector;
    if (request.value !== undefined) parameters.value = request.value;
    if (request.url !== undefined) parameters.url = request.url;
    if (request.script !== undefined) parameters.script = request.script;
    if (request.bindings !== undefined) parameters.bindings = request.bindings;
    if (request.attribute !== undefined) parameters.attribute = request.attribute;
    if (request.timeout !== undefined) parameters.timeout_ms = request.timeout;
    if (request.waitCondition !== undefined) parameters.wait_condition = request.waitCondition;

    return this.client.post<ExecuteOperationResponse>(
      `/sandbox/sessions/${sessionId}/execute`,
      {
        operation_type: request.operationType,
        description: request.description ?? request.operationType,
        parameters,
      },
      options
    );
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
    options?: RequestOptions
  ): Promise<SessionInfo> {
    return this.client.post<SessionInfo>(`/sandbox/sessions/${sessionId}/pause`, {}, options);
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
    options?: RequestOptions
  ): Promise<SessionInfo> {
    return this.client.post<SessionInfo>(`/sandbox/sessions/${sessionId}/resume`, {}, options);
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
    options?: RequestOptions
  ): Promise<{ sessionId: string; closed: boolean }> {
    return this.client.delete<{ sessionId: string; closed: boolean }>(
      `/sandbox/sessions/${sessionId}`,
      options
    );
  }

  /**
   * 截取页面截图
   *
   * @param sessionId - 会话ID
   * @param options - 截图选项
   * @param requestOptions - 请求选项
   * @returns 截图结果
   *
   * @example
   * ```typescript
   * // 截取完整页面
   * const screenshot = await sdk.sandbox.takeScreenshot('session-123', {
   *   fullPage: true,
   *   type: 'png',
   * });
   *
   * // 截取特定元素
   * const elementScreenshot = await sdk.sandbox.takeScreenshot('session-123', {
   *   selector: '#chart-container',
   * });
   * ```
   */
  public async takeScreenshot(
    sessionId: string,
    options?: ScreenshotOptions,
    requestOptions?: RequestOptions
  ): Promise<ScreenshotResponse> {
    return this.client.post<ScreenshotResponse>(
      `/sandbox/sessions/${sessionId}/screenshot`,
      {
        selector: options?.selector,
        full_page: options?.fullPage,
        type: options?.type,
        quality: options?.quality,
        clip: options?.clip,
      },
      requestOptions
    );
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
    options?: RequestOptions
  ): Promise<ExportDataResponse> {
    return this.client.post<ExportDataResponse>(
      `/sandbox/sessions/${sessionId}/export`,
      {
        format: request.format,
        selector: request.selector,
        extraction_rules: request.extractionRules,
      },
      options
    );
  }

  /**
   * 获取操作详情
   */
  public async getOperation(
    operationId: string,
    options?: RequestOptions
  ): Promise<SandboxOperationInfo> {
    const response = await this.client.get<SandboxOperationInfoApi>(
      `/sandbox/operations/${operationId}`,
      options
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
    const response = await this.client.get<SandboxStatsApi>('/sandbox/stats', options);
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
    options?: RequestOptions
  ): Promise<ExecuteOperationResponse> {
    return this.executeOperation(
      sessionId,
      { operationType: OperationType.Navigate, url },
      options
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
    options?: RequestOptions
  ): Promise<ExecuteOperationResponse> {
    return this.executeOperation(
      sessionId,
      { operationType: OperationType.Click, selector },
      options
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
    value: ExecuteOperationRequest['value'],
    options?: RequestOptions
  ): Promise<ExecuteOperationResponse> {
    return this.executeOperation(
      sessionId,
      { operationType: OperationType.Fill, selector, value },
      options
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
   * console.log('Price:', result.result);
   * ```
   */
  public async getText(
    sessionId: string,
    selector: string,
    options?: RequestOptions
  ): Promise<ExecuteOperationResponse> {
    return this.executeOperation(
      sessionId,
      { operationType: OperationType.GetText, selector },
      options
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
   * console.log('Link:', result.result);
   * ```
   */
  public async getAttribute(
    sessionId: string,
    selector: string,
    attribute: string,
    options?: RequestOptions
  ): Promise<ExecuteOperationResponse> {
    return this.executeOperation(
      sessionId,
      { operationType: OperationType.GetAttribute, selector, attribute },
      options
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
   *   await credbridge.fill('#api-key', 'apiKey');
   *   return await credbridge.getText('#status');
   * `, {
   *   apiKey: { $credential: 'api_key' },
   * });
   * console.log('Title:', result.result);
   * ```
   */
  public async executeScript(
    sessionId: string,
    script: string,
    bindings?: ExecuteOperationRequest['bindings'],
    options?: RequestOptions
  ): Promise<ExecuteOperationResponse> {
    return this.executeOperation(
      sessionId,
      { operationType: OperationType.ExecuteScript, script, bindings },
      options
    );
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
    options?: RequestOptions & { timeout?: number; visible?: boolean }
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
      requestOptions
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
    options?: RequestOptions
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
    }
  ): Promise<SessionInfo> {
    const timeout = options?.timeout ?? 30000;
    const interval = options?.interval ?? 1000;
    const startTime = Date.now();

    while (Date.now() - startTime < timeout) {
      const session = await this.getSession(sessionId, options?.requestOptions);
      if (session.status === status) {
        return session;
      }
      await new Promise(resolve => setTimeout(resolve, interval));
    }

    throw new Error(`Timeout waiting for session status: ${status}`);
  }
}

export { SessionStatus, OperationType, OperationStatus };
