/**
 * Toani Vault SDK
 *
 * TypeScript client for Toani Vault API
 *
 * @example
 * ```typescript
 * import { ToaniVaultSDK, CredentialType } from '@toani/vault-sdk';
 *
 * const sdk = new ToaniVaultSDK({
 *   baseUrl: 'https://vault.toani.io',
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
 * // 提交无会话 sandbox broker 请求
 * const op = await sdk.sandbox.request({
 *   operationType: OperationType.HttpRequest,
 *   description: 'health check',
 *   parameters: { url: 'https://api.example.com/health', method: 'GET' },
 * });
 * const detail = await sdk.sandbox.getRequest(op.operationId);
 * console.log(detail.status);
 * ```
 */

// 导出核心类
export { CredBridgeClient } from "./client.js";
export { AuthService } from "./auth.js";
export { AuditService } from "./audit.js";
export { CredentialsService } from "./credentials.js";
export { TokenManager } from "./token.js";
export { ServiceAccountsService } from "./service-accounts.js";
export { SandboxService } from "./sandbox.js";
export { ApprovalsService } from "./approvals.js";

// 导出所有类型
export {
  // 枚举
  CredentialType,
  type CredentialProvider,
  type CredentialCustomFunction,
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
  type CreateApprovalRequest,
  type ApprovalInitiationResponse,
  type ApprovalDetailResponse,

  // Token 类型
  type TokenClaims,
  type CreateTokenRequest,
  type CreateTokenResponse,
  type TokenInfo,
  type TokenMetadata,
  type ListTokensResponse,
  type TokenRevokeByIdResponse,
  type TokenRefreshResult,
  type TokenStatsResponse,

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
  type ListAuditLogsRequest,
  type AuditLogItem,
  type AuditLogsListResponse,
  type AuditExportFormat,
  type ExportAuditLogsRequest,
  type AuditExportResult,
  type VerifyAuditLogRequest,
  type AuditVerificationDetail,
  type VerifyAuditLogResult,

  // Auth 类型
  type AuthCreateAccessTokenRequest,
  type AuthCreateAccessTokenResponse,
  type AuthIdentityInfo,
  type AuthUserProfile,
  type AuthMembershipInfo,
  type AuthTenantInfo,
  type AuthMeResponse,
  type AuthMembershipsResponse,
  type AuthLogoutResponse,

  // Service Account 类型
  type ServiceAccountStatus,
  type ServiceAccountInfo,
  type CreateServiceAccountRequest,
  type UpdateServiceAccountRequest,
  type ServiceAccountTokenCreateRequest,
  type ServiceAccountTokenCreateResponse,
  type ServiceAccountTokenMetadata,

  // Sandbox 类型
  type ExecuteOperationRequest,
  type ExecuteOperationResponse,
  type SandboxOperationInfo,
} from "./types.js";

import { AuthService } from "./auth.js";
import { AuditService } from "./audit.js";
import { CredBridgeClient } from "./client.js";
import { CredentialsService } from "./credentials.js";
import { TokenManager } from "./token.js";
import { ServiceAccountsService } from "./service-accounts.js";
import { SandboxService } from "./sandbox.js";
import { ApprovalsService } from "./approvals.js";
import type { CredBridgeConfig } from "./types.js";

/**
 * Toani Vault SDK 主类
 *
 * 提供凭证管理、Token 操作和 Sandbox 自动化功能的便捷接口
 */
export class ToaniVaultSDK {
  /** 核心 HTTP 客户端 */
  public readonly client: CredBridgeClient;
  /** 凭证管理服务 */
  public readonly credentials: CredentialsService;
  /** Auth 服务 */
  public readonly auth: AuthService;
  /** Audit 服务 */
  public readonly audit: AuditService;
  /** Token 管理 */
  public readonly token: TokenManager;
  /** Service Account 管理 */
  public readonly serviceAccounts: ServiceAccountsService;
  /** Sandbox 服务 */
  public readonly sandbox: SandboxService;
  /** Approvals 服务 */
  public readonly approvals: ApprovalsService;

  /**
   * 创建 Toani Vault SDK 实例
   *
   * @param config - SDK 配置
   *
   * @example
   * ```typescript
   * // 基础配置
   * const sdk = new ToaniVaultSDK({
   *   baseUrl: 'https://vault.toani.io',
   *   token: 'v4.local.your-token',
   * });
   *
   * // 高级配置
   * const sdk = new ToaniVaultSDK({
   *   baseUrl: 'https://vault.toani.io',
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
      const { CredBridgeClient } = require("./client.js");
      this.client = new CredBridgeClient(config);
    }

    this.credentials = new CredentialsService(this.client);
    this.auth = new AuthService(this.client);
    this.audit = new AuditService(this.client);
    this.token = new TokenManager(this.client);
    this.serviceAccounts = new ServiceAccountsService(this.client);
    this.sandbox = new SandboxService(this.client);
    this.approvals = new ApprovalsService(this.client);
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
   * const sdk = ToaniVaultSDK.create('https://vault.toani.io', 'v4.local.your-token');
   * ```
   */
  public static create(baseUrl: string, token: string): ToaniVaultSDK {
    return new ToaniVaultSDK({ baseUrl, token });
  }

  /**
   * 获取 SDK 版本
   */
  public static get version(): string {
    return "0.1.0";
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
      }>("/health");

      return {
        compatible: true,
        sdkVersion: ToaniVaultSDK.version,
        apiVersion: health.version,
        message: "SDK is compatible with the API",
      };
    } catch (error) {
      return {
        compatible: false,
        sdkVersion: ToaniVaultSDK.version,
        message: `Failed to check API compatibility: ${error instanceof Error ? error.message : "Unknown error"}`,
      };
    }
  }
}

/**
 * CredBridge SDK 主类（已弃用）
 *
 * @deprecated 请使用 {@link ToaniVaultSDK} 替代。CredBridgeSDK 将在 v1.0.0 版本中移除。
 */
export const CredBridgeSDK = ToaniVaultSDK;

// 默认导出
export default ToaniVaultSDK;
