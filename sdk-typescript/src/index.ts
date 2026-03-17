/**
 * CredBridge SDK
 *
 * TypeScript client for CredBridge Vault API
 *
 * @example
 * ```typescript
 * import { CredBridgeSDK, CredentialType } from '@credbridge/sdk';
 *
 * const sdk = new CredBridgeSDK({
 *   baseUrl: 'https://vault.credbridge.io',
 *   token: 'v4.local.your-token-here',
 * });
 *
 * // 创建凭证
 * const credential = await sdk.credentials.create({
 *   serviceId: 'schwab',
 *   credentialType: CredentialType.UsernamePassword,
 *   plaintextData: {
 *     username: 'user@example.com',
 *     password: 'secret',
 *   },
 * });
 *
 * // 解密凭证
 * const decrypted = await sdk.credentials.decrypt(credential.credentialId);
 * console.log(decrypted.plaintextData);
 * ```
 */

// 导出核心类
export { CredBridgeClient } from './client.js';
export { CredentialsService } from './credentials.js';
export { TokenManager } from './token.js';
export { SandboxService } from './sandbox.js';
export { SandboxWebSocketClient } from './websocket.js';

// 导出所有类型
export {
  // 枚举
  CredentialType,
  TokenScope,
  CredBridgeErrorCode,
  SdkEventType,
  SessionStatus,
  OperationType,
  OperationStatus,

  // 错误类
  CredBridgeError,

  // 配置类型
  type CredBridgeConfig,

  // 请求/响应类型
  type CreateCredentialRequest,
  type CreateCredentialResponse,
  type CredentialMetadata,
  type GetCredentialResponse,
  type ListCredentialsResponse,
  type DecryptCredentialRequest,
  type DecryptCredentialResponse,
  type DeleteCredentialResponse,
  type EncryptedPayload,
  type CredentialFilter,

  // Token 类型
  type TokenClaims,
  type TokenInfo,
  type TokenRefreshResult,

  // API 类型
  type ApiMeta,
  type ApiSuccessResponse,
  type ApiErrorDetails,
  type ApiErrorResponse,
  type ApiResponse,

  // 请求选项
  type RequestOptions,
  type PaginationParams,

  // 事件类型
  type SdkEvent,
  type EventListener,

  // 审计日志类型
  type AuditLogEntry,
  type ListAuditLogsResponse,
  type AuditLogFilter,

  // Sandbox 类型
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
  type SandboxConfig,
} from './types.js';

// 重新导出 WebSocket 类型
export type {
  WebSocketConfig,
  ClientMessage,
  ServerMessage,
  ExecuteMessage,
  ScreenshotMessage,
  HeartbeatMessage,
  CloseMessage,
  ConnectedMessage,
  OperationProgressMessage,
  OperationCompletedMessage,
  ScreenshotResultMessage,
  HeartbeatAckMessage,
  SessionStatusUpdateMessage,
  ErrorMessage,
  ExecuteOperationOptions,
  ExecuteOperationResult,
  ScreenshotResult,
} from './websocket.js';

// 导出 WebSocket 状态枚举
export { WebSocketState } from './websocket.js';

import { CredBridgeClient } from './client.js';
import { CredentialsService } from './credentials.js';
import { TokenManager } from './token.js';
import { SandboxService } from './sandbox.js';
import type { CredBridgeConfig } from './types.js';

/**
 * CredBridge SDK 主类
 *
 * 提供凭证管理、Token 操作和 Sandbox 自动化功能的便捷接口
 */
export class CredBridgeSDK {
  /** 核心 HTTP 客户端 */
  public readonly client: CredBridgeClient;
  /** 凭证管理服务 */
  public readonly credentials: CredentialsService;
  /** Token 管理 */
  public readonly token: TokenManager;
  /** Sandbox 服务 */
  public readonly sandbox: SandboxService;

  /**
   * 创建 CredBridge SDK 实例
   *
   * @param config - SDK 配置
   *
   * @example
   * ```typescript
   * // 基础配置
   * const sdk = new CredBridgeSDK({
   *   baseUrl: 'https://vault.credbridge.io',
   *   token: 'v4.local.your-token',
   * });
   *
   * // 高级配置
   * const sdk = new CredBridgeSDK({
   *   baseUrl: 'https://vault.credbridge.io',
   *   token: 'v4.local.your-token',
   *   timeout: 60000,
   *   maxRetries: 5,
   *   autoRefreshToken: true,
   *   tokenRefreshBuffer: 5 * 60 * 1000, // 5分钟
   * });
   * ```
   */
  constructor(config: CredBridgeConfig | CredBridgeClient) {
    // 如果传入的是 CredBridgeClient 实例，直接使用
    if (config instanceof CredBridgeClient) {
      this.client = config;
    } else {
      // 动态导入 client.ts 以避免循环依赖
      const { CredBridgeClient } = require('./client.js');
      this.client = new CredBridgeClient(config);
    }

    this.credentials = new CredentialsService(this.client);
    this.token = new TokenManager(this.client);
    this.sandbox = new SandboxService(this.client);
  }

  /**
   * 快速创建 SDK 实例的工厂方法
   *
   * @param baseUrl - API 基础 URL
   * @param token - API Token
   * @returns SDK 实例
   *
   * @example
   * ```typescript
   * const sdk = CredBridgeSDK.create('https://vault.credbridge.io', 'v4.local.your-token');
   * ```
   */
  public static create(baseUrl: string, token: string): CredBridgeSDK {
    return new CredBridgeSDK({ baseUrl, token });
  }

  /**
   * 获取 SDK 版本
   */
  public static get version(): string {
    return '0.1.0';
  }

  /**
   * 检查 SDK 是否与服务器 API 版本兼容
   *
   * @returns 兼容性检查结果
   */
  public async checkCompatibility(): Promise<{
    compatible: boolean;
    sdkVersion: string;
    apiVersion?: string;
    message: string;
  }> {
    try {
      const health = await this.client.get<{
        status: string;
        version?: string;
      }>('/health');

      return {
        compatible: true,
        sdkVersion: CredBridgeSDK.version,
        apiVersion: health.version,
        message: 'SDK is compatible with the API',
      };
    } catch (error) {
      return {
        compatible: false,
        sdkVersion: CredBridgeSDK.version,
        message: `Failed to check API compatibility: ${error instanceof Error ? error.message : 'Unknown error'}`,
      };
    }
  }
}

// 默认导出
export default CredBridgeSDK;
