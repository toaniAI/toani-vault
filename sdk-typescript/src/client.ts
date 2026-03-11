/**
 * CredBridge SDK 核心客户端
 *
 * 实现 HTTP 请求、错误重试、Token 管理等功能
 */

import {
  type CredBridgeConfig,
  type RequestOptions,
  type ApiResponse,
  CredBridgeError,
  CredBridgeErrorCode,
  type SdkEvent,
  type SdkEventType,
  type EventListener,
  type TokenInfo,
} from './types.js';

/** 默认配置 */
const DEFAULT_CONFIG: Partial<CredBridgeConfig> = {
  timeout: 30000,
  maxRetries: 3,
  autoRefreshToken: true,
  tokenRefreshBuffer: 5 * 60 * 1000, // 5 minutes
};

/** 指数退避延迟计算 */
function calculateBackoffDelay(attempt: number, baseDelay = 1000): number {
  return baseDelay * Math.pow(2, attempt - 1);
}

/** 延迟函数 */
function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** 生成请求 ID */
function generateRequestId(): string {
  return `req_${Date.now()}_${Math.random().toString(36).substring(2, 11)}`;
}

/** 解析 API 错误码 */
function parseErrorCode(statusCode: number, errorCode?: string): CredBridgeErrorCode {
  if (errorCode) {
    const codeMap: Record<string, CredBridgeErrorCode> = {
      'not_found': CredBridgeErrorCode.NotFound,
      'invalid_request': CredBridgeErrorCode.InvalidRequest,
      'unauthorized': CredBridgeErrorCode.Unauthorized,
      'forbidden': CredBridgeErrorCode.Forbidden,
      'internal_error': CredBridgeErrorCode.InternalError,
      'token_expired': CredBridgeErrorCode.TokenExpired,
      'invalid_token': CredBridgeErrorCode.InvalidToken,
      'token_revoked': CredBridgeErrorCode.TokenRevoked,
      'insufficient_scope': CredBridgeErrorCode.InsufficientScope,
      'tenant_isolation_violation': CredBridgeErrorCode.TenantIsolationViolation,
      'credential_expired': CredBridgeErrorCode.CredentialExpired,
      'decryption_failed': CredBridgeErrorCode.DecryptionFailed,
    };
    return codeMap[errorCode] ?? CredBridgeErrorCode.Unknown;
  }

  // 基于 HTTP 状态码的错误码映射
  switch (statusCode) {
    case 400:
      return CredBridgeErrorCode.InvalidRequest;
    case 401:
      return CredBridgeErrorCode.Unauthorized;
    case 403:
      return CredBridgeErrorCode.Forbidden;
    case 404:
      return CredBridgeErrorCode.NotFound;
    case 408:
      return CredBridgeErrorCode.Timeout;
    case 429:
      return CredBridgeErrorCode.InternalError; // Rate limited, should retry
    case 500:
    case 502:
    case 503:
    case 504:
      return CredBridgeErrorCode.InternalError;
    default:
      return CredBridgeErrorCode.Unknown;
  }
}

/**
 * CredBridge HTTP 客户端
 */
export class CredBridgeClient {
  private config: Required<CredBridgeConfig>;
  private tokenInfo?: TokenInfo;
  private eventListeners: Map<SdkEventType, Set<EventListener>> = new Map();
  private refreshPromise?: Promise<string>;

  constructor(config: CredBridgeConfig) {
    this.config = {
      ...DEFAULT_CONFIG,
      ...config,
    } as Required<CredBridgeConfig>;

    // 如果提供了 token，解析它
    if (config.token) {
      this.parseAndStoreToken(config.token);
    }
  }

  /**
   * 获取当前配置
   */
  public getConfig(): Required<CredBridgeConfig> {
    return { ...this.config };
  }

  /**
   * 更新 Token
   */
  public setToken(token: string): void {
    this.config.token = token;
    this.parseAndStoreToken(token);
    this.emit('token_refreshed' as SdkEventType, { token, tokenInfo: this.tokenInfo });
  }

  /**
   * 获取当前 Token
   */
  public getToken(): string | undefined {
    return this.config.token;
  }

  /**
   * 获取 Token 信息
   */
  public getTokenInfo(): TokenInfo | undefined {
    return this.tokenInfo;
  }

  /**
   * 检查 Token 是否即将过期
   */
  public isTokenExpiringSoon(): boolean {
    if (!this.tokenInfo) return true;

    const now = Date.now();
    const expiresAt = this.tokenInfo.expiresAt * 1000;
    const buffer = this.config.tokenRefreshBuffer;

    return now >= expiresAt - buffer;
  }

  /**
   * 检查 Token 是否已过期
   */
  public isTokenExpired(): boolean {
    if (!this.tokenInfo) return true;

    const now = Math.floor(Date.now() / 1000);
    return now >= this.tokenInfo.expiresAt;
  }

  /**
   * 添加事件监听器
   */
  public on<T>(event: SdkEventType, listener: EventListener<T>): () => void {
    if (!this.eventListeners.has(event)) {
      this.eventListeners.set(event, new Set());
    }
    this.eventListeners.get(event)!.add(listener as EventListener);

    // 返回取消订阅函数
    return () => {
      this.eventListeners.get(event)?.delete(listener as EventListener);
    };
  }

  /**
   * 移除事件监听器
   */
  public off<T>(event: SdkEventType, listener: EventListener<T>): void {
    this.eventListeners.get(event)?.delete(listener as EventListener);
  }

  /**
   * 触发事件
   */
  private emit<T>(event: SdkEventType, data: T): void {
    const listeners = this.eventListeners.get(event);
    if (listeners) {
      const eventObj: SdkEvent<T> = {
        type: event,
        timestamp: Date.now(),
        data,
      };
      listeners.forEach((listener) => {
        try {
          listener(eventObj as unknown as Parameters<typeof listener>[0]);
        } catch (error) {
          // 监听器错误不应影响主流程
          console.error('Event listener error:', error);
        }
      });
    }
  }

  /**
   * 发送 HTTP 请求
   */
  public async request<T>(
    method: string,
    path: string,
    body?: unknown,
    options: RequestOptions = {}
  ): Promise<T> {
    const requestId = options.requestId ?? generateRequestId();
    const url = `${this.config.baseUrl.replace(/\/$/, '')}/api/v1${path}`;
    const timeout = options.timeout ?? this.config.timeout;
    const maxRetries = options.skipRetry ? 0 : (options.retries ?? this.config.maxRetries);

    // 检查是否需要刷新 Token
    if (this.config.autoRefreshToken && this.isTokenExpiringSoon() && !path.includes('/tokens')) {
      this.emit('token_expiring' as SdkEventType, { tokenInfo: this.tokenInfo });
    }

    this.emit('request_start' as SdkEventType, { method, path, requestId });

    let lastError: CredBridgeError | undefined;

    for (let attempt = 0; attempt <= maxRetries; attempt++) {
      try {
        const result = await this.executeRequest<T>(method, url, body, timeout, requestId, options.headers);
        this.emit('request_success' as SdkEventType, { method, path, requestId });
        return result;
      } catch (error) {
        lastError = error as CredBridgeError;

        // 如果是认证错误，尝试刷新 Token 后重试
        if (lastError.isAuthError() && this.config.autoRefreshToken && attempt === 0) {
          try {
            await this.refreshTokenIfNeeded();
            continue; // 使用新 Token 重试
          } catch {
            // Token 刷新失败，继续正常重试流程
          }
        }

        // 如果不是可重试的错误，或者已经是最后一次尝试，抛出错误
        if (!lastError.isRetryable() || attempt === maxRetries) {
          this.emit('request_error' as SdkEventType, {
            method,
            path,
            requestId,
            error: lastError,
          });
          throw lastError;
        }

        // 计算退避延迟
        const delay = calculateBackoffDelay(attempt);
        this.emit('retry' as SdkEventType, {
          method,
          path,
          requestId,
          attempt: attempt + 1,
          maxRetries,
          delay,
        });
        await sleep(delay);
      }
    }

    // 所有重试都失败了
    throw lastError ?? new CredBridgeError(
      CredBridgeErrorCode.Unknown,
      'Request failed after retries'
    );
  }

  /**
   * 执行单次 HTTP 请求
   */
  private async executeRequest<T>(
    method: string,
    url: string,
    body: unknown,
    timeout: number,
    requestId: string,
    customHeaders?: Record<string, string>
  ): Promise<T> {
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), timeout);

    try {
      const headers: Record<string, string> = {
        'Content-Type': 'application/json',
        'X-Request-ID': requestId,
        ...this.config.headers,
        ...customHeaders,
      };

      // 添加 Authorization 头
      if (this.config.token) {
        headers['Authorization'] = `Bearer ${this.config.token}`;
      }

      // 添加请求签名（如果配置了签名密钥）
      if (this.config.signingKey) {
        const signature = await this.signRequest(method, url, body);
        headers['X-CredBridge-Signature'] = signature;
      }

      const response = await fetch(url, {
        method,
        headers,
        body: body ? JSON.stringify(body) : undefined,
        signal: controller.signal,
      });

      clearTimeout(timeoutId);

      // 解析响应
      let data: ApiResponse<T>;
      const contentType = response.headers.get('content-type');

      if (contentType?.includes('application/json')) {
        data = await response.json() as ApiResponse<T>;
      } else {
        const text = await response.text();
        data = {
          success: false,
          error: {
            code: 'unknown',
            message: text || 'Unknown error',
          },
          meta: {
            requestId,
            timestamp: new Date().toISOString(),
          },
        };
      }

      // 处理错误响应
      if (!response.ok || !data.success) {
        const errorData = data.success === false ? data.error : { code: 'unknown', message: 'Unknown error' };
        const errorCode = parseErrorCode(response.status, errorData.code);

        throw new CredBridgeError(
          errorCode,
          errorData.message,
          response.status,
          errorData.details,
          requestId
        );
      }

      return data.data;
    } catch (error) {
      clearTimeout(timeoutId);

      // 处理 AbortController 超时
      if (error instanceof Error && error.name === 'AbortError') {
        throw new CredBridgeError(
          CredBridgeErrorCode.Timeout,
          `Request timeout after ${timeout}ms`,
          undefined,
          undefined,
          requestId
        );
      }

      // 处理网络错误
      if (error instanceof TypeError && error.message.includes('fetch')) {
        throw new CredBridgeError(
          CredBridgeErrorCode.NetworkError,
          `Network error: ${error.message}`,
          undefined,
          undefined,
          requestId
        );
      }

      // 重新抛出 CredBridgeError
      if (error instanceof CredBridgeError) {
        throw error;
      }

      // 未知错误
      throw new CredBridgeError(
        CredBridgeErrorCode.Unknown,
        error instanceof Error ? error.message : 'Unknown error',
        undefined,
        undefined,
        requestId
      );
    }
  }

  /**
   * 解析并存储 Token 信息
   */
  private parseAndStoreToken(token: string): void {
    try {
      // 解析 PASETO token 的 payload 部分
      const parts = token.split('.');
      if (parts.length >= 3) {
        const payloadBase64 = parts[2];
        const payloadJson = Buffer.from(payloadBase64, 'base64url').toString('utf-8');
        const payload = JSON.parse(payloadJson);

        // 解析 scope 字符串为数组
        const scopeStr = payload.scope || '';
        const scopes = scopeStr.split(/\s+/).filter(Boolean);

        // 解析 subject (tenant_id:user_id)
        const subject = payload.sub || '';
        const [tenantId, userId] = subject.split(':');

        this.tokenInfo = {
          tokenId: payload.jti || '',
          subject,
          tenantId: tenantId || payload.tenant_id || '',
          userId: userId || '',
          expiresAt: parseInt(payload.exp || '0', 10),
          scopes: scopes as TokenInfo['scopes'],
          issuedAt: parseInt(payload.iat || '0', 10),
        };
      }
    } catch (error) {
      // Token 解析失败，但不影响使用
      console.warn('Failed to parse token:', error);
    }
  }

  /**
   * 刷新 Token（如果需要）
   */
  private async refreshTokenIfNeeded(): Promise<string> {
    // 如果已经在刷新中，返回现有的 Promise
    if (this.refreshPromise) {
      return this.refreshPromise;
    }

    // 创建新的刷新 Promise
    this.refreshPromise = this.doRefreshToken();

    try {
      const newToken = await this.refreshPromise;
      return newToken;
    } finally {
      this.refreshPromise = undefined;
    }
  }

  /**
   * 执行 Token 刷新
   */
  private async doRefreshToken(): Promise<string> {
    // 注意：这里应该调用实际的 Token 刷新端点
    // 由于 Token 刷新逻辑可能因实现而异，这里提供一个基础框架
    // 实际使用时需要替换为真正的刷新逻辑
    throw new CredBridgeError(
      CredBridgeErrorCode.InvalidToken,
      'Token refresh not implemented. Please provide a new token manually.'
    );
  }

  /**
   * 签名请求
   */
  private async signRequest(
    _method: string,
    _url: string,
    _body: unknown
  ): Promise<string> {
    // 请求签名实现
    // 这里应该使用配置的 signingKey 对请求进行签名
    // 实际实现取决于服务端验证签名的方式
    const timestamp = Date.now().toString();
    return `t=${timestamp},v1=placeholder`;
  }

  // ============================================================================
  // HTTP 方法快捷方式
  // ============================================================================

  /**
   * GET 请求
   */
  public async get<T>(path: string, options?: RequestOptions): Promise<T> {
    return this.request<T>('GET', path, undefined, options);
  }

  /**
   * POST 请求
   */
  public async post<T>(path: string, body: unknown, options?: RequestOptions): Promise<T> {
    return this.request<T>('POST', path, body, options);
  }

  /**
   * PUT 请求
   */
  public async put<T>(path: string, body: unknown, options?: RequestOptions): Promise<T> {
    return this.request<T>('PUT', path, body, options);
  }

  /**
   * DELETE 请求
   */
  public async delete<T>(path: string, options?: RequestOptions): Promise<T> {
    return this.request<T>('DELETE', path, undefined, options);
  }

  /**
   * PATCH 请求
   */
  public async patch<T>(path: string, body: unknown, options?: RequestOptions): Promise<T> {
    return this.request<T>('PATCH', path, body, options);
  }
}
