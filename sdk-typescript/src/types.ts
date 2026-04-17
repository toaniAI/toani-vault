/**
 * CredBridge SDK TypeScript 类型定义
 *
 * 基于 CredBridge API 规范
 */

// ============================================================================
// 基础类型
// ============================================================================

/** 凭证类型 */
export enum CredentialType {
  /** 用户名密码 */
  UsernamePassword = "username_password",
  /** OAuth 刷新令牌 */
  OAuthRefresh = "oauth_refresh",
  /** API 密钥 */
  ApiKey = "api_key",
  /** 会话 Cookie */
  SessionCookie = "session_cookie",
  /** KYC 文档 */
  KycDocument = "kyc_document",
}

/** Token Scope 权限 */
export enum TokenScope {
  /** 凭证读取权限 */
  CredentialRead = "credential:read",
  /** 凭证写入权限 */
  CredentialWrite = "credential:write",
  /** 审计日志读取权限 */
  AuditRead = "audit:read",
  /** 管理员权限 */
  Admin = "admin",
}

// ============================================================================
// SDK 配置类型
// ============================================================================

/** SDK 配置选项 */
export interface CredBridgeConfig {
  /** API 基础 URL */
  baseUrl: string;
  /** API Token (PASETO v4.local) */
  token?: string;
  /** 租户 ID */
  tenantId?: string;
  /** 用户 ID */
  userId?: string;
  /** 请求超时时间（毫秒，默认 30000） */
  timeout?: number;
  /** 最大重试次数（默认 3） */
  maxRetries?: number;
  /** 是否自动刷新 Token（默认 true） */
  autoRefreshToken?: boolean;
  /** Token 刷新缓冲时间（毫秒，默认 5分钟） */
  tokenRefreshBuffer?: number;
  /** 自定义请求头 */
  headers?: Record<string, string>;
  /** 请求签名密钥（可选） */
  signingKey?: string;
}

/** SDK 初始化选项 */
export interface CredBridgeInitOptions extends CredBridgeConfig {
  /** 调试模式 */
  debug?: boolean;
}

// ============================================================================
// API 请求/响应类型
// ============================================================================

/** 创建凭证请求 */
export interface CreateCredentialRequest {
  /** 服务 ID */
  serviceId: string;
  /** 凭证类型 */
  credentialType: CredentialType;
  /** 明文凭证内容（将被加密） */
  plaintextData: Record<string, unknown>;
  /** 过期时间（Unix 时间戳，可选） */
  expiresAt?: number;
}

/** 创建凭证响应 */
export interface CreateCredentialResponse {
  /** 凭证 ID */
  credential_id: string;
  /** 服务 ID */
  service_id: string;
  /** 凭证类型 */
  credential_type: string;
  /** 创建时间 */
  created_at: string;
  /** 过期时间 */
  expires_at?: string;
}

/** 创建凭证响应 (camelCase 别名) */
export type CreateCredentialResponseCamel = {
  credentialId: string;
  serviceId: string;
  credentialType: string;
  createdAt: string;
  expiresAt?: string;
};

/** 凭证元数据 */
export interface CredentialMetadata {
  /** 凭证 ID */
  credentialId: string;
  /** 凭证类型 */
  credentialType: CredentialType;
  /** 用户 ID 哈希 */
  userIdHash: string;
  /** 服务 ID */
  serviceId: string;
  /** 租户 ID */
  tenantId: string;
  /** 创建时间 */
  createdAt: string;
  /** 过期时间 */
  expiresAt?: string;
  /** 是否已删除 */
  isDeleted: boolean;
}

/** 凭证列表响应 */
export interface ListCredentialsResponse {
  /** 凭证列表 */
  credentials: CredentialMetadata[];
  /** 总数 */
  total: number;
}

/** 凭证详情响应 */
export interface GetCredentialResponse {
  /** 凭证 ID */
  credentialId: string;
  /** 服务 ID */
  serviceId: string;
  /** 凭证类型 */
  credentialType: string;
  /** 创建时间 */
  createdAt: string;
  /** 过期时间 */
  expiresAt?: string;
  /** 是否已删除 */
  isDeleted: boolean;
  /** 加密载荷（可选） */
  encryptedPayload?: EncryptedPayload;
}

/** 解密凭证请求 */
export interface DecryptCredentialRequest {
  /** 请求解密的理由（用于审计） */
  reason?: string;
}

/** 解密凭证响应 */
export interface DecryptCredentialResponse {
  /** 凭证 ID */
  credential_id: string;
  /** 服务 ID */
  service_id: string;
  /** 凭证类型 */
  credential_type: string;
  /** 解密的明文数据 */
  plaintext_data: Record<string, unknown>;
}

/** 删除凭证响应 */
export interface DeleteCredentialResponse {
  /** 凭证 ID */
  credentialId: string;
  /** 是否已删除 */
  deleted: boolean;
}

/** 加密载荷结构 */
export interface EncryptedPayload {
  /** 协议版本 */
  version: number;
  /** 加密算法 */
  algorithm: string;
  /** KDF 算法 */
  kdf: string;
  /** Nonce（base64） */
  nonce: string;
  /** Auth Tag（base64） */
  authTag: string;
  /** 密文（base64） */
  ciphertext: string;
}

/** 凭证列表过滤条件 */
export interface CredentialFilter {
  /** 按服务 ID 过滤 */
  serviceId?: string;
  /** 按凭证类型过滤 */
  credentialType?: CredentialType;
  /** 包含已删除的凭证 */
  includeDeleted?: boolean;
  /** 仅返回未过期的凭证 */
  onlyValid?: boolean;
}

// ============================================================================
// Token 类型
// ============================================================================

/** Token Claims */
export interface TokenClaims {
  /** 颁发者 */
  iss: string;
  /** 主题（租户ID:用户ID） */
  sub: string;
  /** 受众（租户ID） */
  aud: string;
  /** 过期时间（Unix 时间戳） */
  exp: number;
  /** 生效时间（Unix 时间戳） */
  nbf: number;
  /** 颁发时间（Unix 时间戳） */
  iat: number;
  /** JWT ID */
  jti: string;
  /** 权限范围 */
  scope: string;
  /** 租户 ID */
  tenantId: string;
  /** 允许的凭证 ID 列表（可选） */
  credentialIds?: string[];
  /** 绑定 IP（可选） */
  ipBound?: string;
  /** MFA 验证状态 */
  mfaVerified: boolean;
}

/** Token 信息 */
export interface TokenInfo {
  /** Token ID */
  tokenId: string;
  /** 主题 */
  subject: string;
  /** 租户 ID */
  tenantId: string;
  /** 用户 ID */
  userId: string;
  /** 过期时间 */
  expiresAt: number;
  /** 授权 Scope 列表 */
  scopes: TokenScope[];
  /** 颁发时间 */
  issuedAt: number;
}

/** Token 刷新结果 */
export interface TokenRefreshResult {
  /** 新的 Token */
  token: string;
  /** Token 信息 */
  tokenInfo: TokenInfo;
  /** 过期时间 */
  expiresAt: number;
}

/** 创建 Token 请求 */
export interface CreateTokenRequest {
  /** 请求的权限范围 */
  scopes: string[];
  /** 允许访问的凭证白名单 */
  credentialIds: string[];
  /** 过期时间（秒） */
  expiresIn?: number;
}

/** 创建 Token 响应 */
export interface CreateTokenResponse {
  /** 访问令牌 */
  accessToken: string;
  /** Token ID */
  tokenId: string;
  /** Token 类型 */
  tokenType: string;
  /** 有效期（秒） */
  expiresIn: number;
  /** 空格分隔的 scope 字符串 */
  scope: string;
  /** 颁发时间 */
  issuedAt: number;
  /** 过期时间 */
  expiresAt: number;
}

/** 创建 access token 请求 */
export interface AuthCreateAccessTokenRequest {
  /** 请求的权限范围 */
  scopes: string[];
  /** 允许访问的凭证白名单 */
  credentialIds: string[];
  /** 过期时间（秒） */
  ttlSeconds?: number;
}

/** 创建 access token 响应 */
export interface AuthCreateAccessTokenResponse {
  /** 访问令牌 */
  accessToken: string;
  /** Token ID */
  tokenId: string;
  /** Token 类型 */
  tokenType: string;
  /** 主体类型 */
  subjectType?: string;
  /** 签发来源 */
  issuedFrom?: string;
  /** 展示名称 */
  displayName?: string;
  /** 过期时间（Unix 时间戳） */
  expiresAt: number;
  /** 有效期（秒） */
  expiresIn: number;
  /** 已授予 scopes */
  grantedScopes: string[];
  /** 撤销时间 */
  revokedAt?: string;
}

/** Token 统计响应 */
export interface TokenStatsResponse {
  /** Token 总数 */
  totalTokens?: number;
  /** 活跃 Token 数量 */
  activeTokens: number;
  /** 已撤销 Token 数量 */
  revokedTokens?: number;
}

/** Token 元数据 */
export interface TokenMetadata {
  tokenId: string;
  tokenType: string;
  subjectType: string;
  subjectId: string;
  tenantId: string;
  issuedFrom: string;
  sessionId?: string;
  membershipId?: string;
  displayName?: string;
  grantedScopes: string[];
  expiresAt: string;
  revokedAt?: string;
  createdAt: string;
  lastUsedAt?: string;
}

/** Service Account 状态 */
export type ServiceAccountStatus = "active" | "disabled" | "deleted";

/** Service Account 信息 */
export interface ServiceAccountInfo {
  id: string;
  tenantId: string;
  name: string;
  description?: string;
  role: string;
  scopeCeiling: string[];
  status: string;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
  deletedAt?: string;
}

/** 创建 Service Account 请求 */
export interface CreateServiceAccountRequest {
  name: string;
  description?: string;
  scopeCeiling: string[];
}

/** 更新 Service Account 请求 */
export interface UpdateServiceAccountRequest {
  name?: string;
  description?: string;
  status?: ServiceAccountStatus;
  scopeCeiling?: string[];
}

/** Service Account token 创建请求 */
export interface ServiceAccountTokenCreateRequest {
  scopes: string[];
  ttlSeconds?: number;
  displayName?: string;
}

/** Service Account token 创建响应 */
export interface ServiceAccountTokenCreateResponse {
  accessToken: string;
  tokenId: string;
  tokenType: string;
  subjectType: string;
  issuedFrom: string;
  displayName?: string;
  expiresIn: number;
  scope: string;
  grantedScopes: string[];
  issuedAt: number;
  expiresAt: number;
  revokedAt?: string;
}

/** Service Account token 元数据 */
export type ServiceAccountTokenMetadata = TokenMetadata;

/** 按 ID 撤销 token 响应 */
export interface TokenRevokeByIdResponse {
  revoked: boolean;
  tokenId: string;
}

// ============================================================================
// 错误类型
// ============================================================================

/** SDK 错误码 */
export enum CredBridgeErrorCode {
  /** 未知错误 */
  Unknown = "unknown",
  /** 网络错误 */
  NetworkError = "network_error",
  /** 请求超时 */
  Timeout = "timeout",
  /** 未授权 */
  Unauthorized = "unauthorized",
  /** 禁止访问 */
  Forbidden = "forbidden",
  /** 凭证未找到 */
  NotFound = "not_found",
  /** 无效的请求 */
  InvalidRequest = "invalid_request",
  /** 服务器内部错误 */
  InternalError = "internal_error",
  /** Token 过期 */
  TokenExpired = "token_expired",
  /** Token 无效 */
  InvalidToken = "invalid_token",
  /** Token 已被撤销 */
  TokenRevoked = "token_revoked",
  /** 权限不足 */
  InsufficientScope = "insufficient_scope",
  /** 租户隔离违规 */
  TenantIsolationViolation = "tenant_isolation_violation",
  /** 凭证已过期 */
  CredentialExpired = "credential_expired",
  /** 解密失败 */
  DecryptionFailed = "decryption_failed",
  /** 加密失败 */
  EncryptionFailed = "encryption_failed",
}

/** SDK 错误 */
export class CredBridgeError extends Error {
  /** 错误码 */
  public readonly code: CredBridgeErrorCode;
  /** HTTP 状态码 */
  public readonly statusCode?: number;
  /** 错误详情 */
  public readonly details?: Record<string, unknown>;
  /** 请求 ID */
  public readonly requestId?: string;

  constructor(
    code: CredBridgeErrorCode,
    message: string,
    statusCode?: number,
    details?: Record<string, unknown>,
    requestId?: string,
  ) {
    super(message);
    this.name = "CredBridgeError";
    this.code = code;
    this.statusCode = statusCode;
    this.details = details;
    this.requestId = requestId;

    // 修复原型链
    Object.setPrototypeOf(this, CredBridgeError.prototype);
  }

  /** 是否为网络错误 */
  public isNetworkError(): boolean {
    return (
      this.code === CredBridgeErrorCode.NetworkError ||
      this.code === CredBridgeErrorCode.Timeout
    );
  }

  /** 是否为认证错误 */
  public isAuthError(): boolean {
    return (
      this.code === CredBridgeErrorCode.Unauthorized ||
      this.code === CredBridgeErrorCode.InvalidToken ||
      this.code === CredBridgeErrorCode.TokenExpired ||
      this.code === CredBridgeErrorCode.TokenRevoked
    );
  }

  /** 是否可重试 */
  public isRetryable(): boolean {
    return (
      this.isNetworkError() ||
      this.code === CredBridgeErrorCode.InternalError ||
      this.statusCode === 429
    ); // Rate limited
  }
}

// ============================================================================
// API 通用响应类型
// ============================================================================

/** API 响应元数据 */
export interface ApiMeta {
  /** 请求 ID */
  requestId: string;
  /** 时间戳 */
  timestamp: string;
}

/** API 成功响应 */
export interface ApiSuccessResponse<T> {
  success: true;
  data: T;
  meta: ApiMeta;
}

/** API 错误详情 */
export interface ApiErrorDetails {
  code: string;
  message: string;
  details?: Record<string, unknown>;
}

/** API 错误响应 */
export interface ApiErrorResponse {
  success: false;
  error: ApiErrorDetails;
  meta: ApiMeta;
}

/** API 响应 */
export type ApiResponse<T> = ApiSuccessResponse<T> | ApiErrorResponse;

// ============================================================================
// 请求选项类型
// ============================================================================

/** 请求选项 */
export interface RequestOptions {
  /** 请求超时时间（毫秒） */
  timeout?: number;
  /** 是否跳过重试 */
  skipRetry?: boolean;
  /** 重试次数 */
  retries?: number;
  /** 自定义请求头 */
  headers?: Record<string, string>;
  /** 请求 ID */
  requestId?: string;
}

/** 分页参数 */
export interface PaginationParams {
  /** 页码（从 1 开始） */
  page?: number;
  /** 每页数量 */
  pageSize?: number;
  /** 排序字段 */
  sortBy?: string;
  /** 排序方向 */
  sortOrder?: "asc" | "desc";
}

// ============================================================================
// 事件类型
// ============================================================================

/** SDK 事件类型 */
export enum SdkEventType {
  /** Token 即将过期 */
  TokenExpiring = "token_expiring",
  /** Token 已刷新 */
  TokenRefreshed = "token_refreshed",
  /** 请求开始 */
  RequestStart = "request_start",
  /** 请求成功 */
  RequestSuccess = "request_success",
  /** 请求失败 */
  RequestError = "request_error",
  /** 重试 */
  Retry = "retry",
}

/** SDK 事件 */
export interface SdkEvent<T = unknown> {
  type: SdkEventType;
  timestamp: number;
  data: T;
}

/** 事件监听器 */
export type EventListener<T = unknown> = (event: SdkEvent<T>) => void;

// ============================================================================
// 审计日志类型
// ============================================================================

/** 审计日志条目 */
export interface AuditLogEntry {
  /** 日志 ID */
  logId: string;
  /** 事件类型 */
  eventType: string;
  /** 租户 ID */
  tenantId: string;
  /** 用户 ID */
  userId: string;
  /** 凭证 ID（可选） */
  credentialId?: string;
  /** 操作结果 */
  success: boolean;
  /** 时间戳 */
  timestamp: string;
  /** 客户端 IP */
  clientIp?: string;
  /** 请求 ID */
  requestId: string;
  /** 附加数据 */
  metadata?: Record<string, unknown>;
}

/** 审计日志列表响应 */
export interface ListAuditLogsResponse {
  logs: AuditLogEntry[];
  total: number;
  hasMore: boolean;
}

/** 审计日志查询过滤 */
export interface AuditLogFilter {
  eventType?: string;
  credentialId?: string;
  startTime?: string;
  endTime?: string;
  success?: boolean;
}

/** 审计日志查询请求 */
export interface ListAuditLogsRequest {
  startTime?: number;
  endTime?: number;
  userIdHash?: string;
  action?: string;
  riskTier?: string;
  outcome?: string;
  service?: string;
  page?: number;
  pageSize?: number;
}

/** 审计日志列表项 */
export interface AuditLogItem {
  id: string;
  timestamp: number;
  userIdHash: string;
  sessionId: string;
  service: string;
  action: string;
  riskTier: string;
  outcome: string;
  logIndex: number;
}

/** 审计日志列表响应 */
export interface AuditLogsListResponse {
  items: AuditLogItem[];
  total: number;
  page: number;
  pageSize: number;
  totalPages: number;
}

/** 审计导出格式 */
export type AuditExportFormat = "json" | "csv";

/** 导出审计日志请求 */
export interface ExportAuditLogsRequest {
  startTime?: number;
  endTime?: number;
  format?: AuditExportFormat;
  userIdHash?: string;
  action?: string;
}

/** 导出审计日志响应 */
export interface AuditExportResult {
  exportId: string;
  format: AuditExportFormat;
  content: string;
  integrityHash: string;
  count: number;
  generatedAt: number;
}

/** 验证审计日志请求 */
export interface VerifyAuditLogRequest {
  id?: string;
  logIndex?: number;
}

/** 审计验证详情 */
export interface AuditVerificationDetail {
  step: string;
  passed: boolean;
  message?: string;
}

/** 验证审计日志结果 */
export interface VerifyAuditLogResult {
  id: string;
  logIndex: number;
  verified: boolean;
  contentHashMatch: boolean;
  signatureValid: boolean;
  merkleProofValid: boolean;
  details: AuditVerificationDetail[];
  verifiedAt: number;
}

// ============================================================================
// Auth 类型
// ============================================================================

/** Auth 身份信息 */
export interface AuthIdentityInfo {
  provider: string;
  subject: string;
  walletAddress?: string;
  email?: string;
  isVerified: boolean;
  isPrimary: boolean;
}

/** Auth 用户信息 */
export interface AuthUserProfile {
  id: string;
  displayName?: string;
  status: string;
  onboardingCompleted: boolean;
  defaultTenantId?: string;
  identities: AuthIdentityInfo[];
}

/** Auth 成员资格信息 */
export interface AuthMembershipInfo {
  id: string;
  tenantId: string;
  role: string;
  status: string;
  scopes: string[];
  joinedAt?: string;
}

/** Auth 租户信息 */
export interface AuthTenantInfo {
  id: string;
  name?: string;
}

/** 当前用户响应 */
export interface AuthMeResponse {
  user: AuthUserProfile;
  currentTenant?: AuthTenantInfo;
  currentMembership?: AuthMembershipInfo;
  memberships: AuthMembershipInfo[];
  mfaStatus: string;
}

/** 成员资格列表响应 */
export interface AuthMembershipsResponse {
  memberships: AuthMembershipInfo[];
}

/** 注销响应 */
export interface AuthLogoutResponse {
  success: boolean;
}

// ============================================================================
// Sandbox 类型
// ============================================================================

/** Session 状态 */
export enum SessionStatus {
  /** 正在创建 */
  Creating = "creating",
  /** 运行中 */
  Running = "running",
  /** 已暂停 */
  Paused = "paused",
  /** 已关闭 */
  Closed = "closed",
  /** 错误状态 */
  Error = "error",
}

/** 操作类型 */
export enum OperationType {
  /** 导航到URL */
  Navigate = "navigate",
  /** 点击元素 */
  Click = "click",
  /** 填充表单 */
  Fill = "fill",
  /** 获取文本 */
  GetText = "get_text",
  /** 获取元素属性 */
  GetAttribute = "get_attribute",
  /** 执行脚本 */
  ExecuteScript = "execute_script",
  /** 受控页面引导注入 */
  BootstrapPage = "bootstrap_page",
  /** 等待元素 */
  WaitForSelector = "wait",
  /** 直接发起 HTTP 请求 */
  HttpRequest = "http_request",
  /** 导出数据 */
  ExportData = "export",
  /** 导出 DOM */
  DomExport = "dom_export",
}

/** 操作状态 */
export enum OperationStatus {
  /** 待执行 */
  Pending = "pending",
  /** 执行中 */
  Running = "running",
  /** 成功 */
  Success = "success",
  /** 失败 */
  Failed = "failed",
  /** 已取消 */
  Cancelled = "cancelled",
}

/** 创建 Session 请求 */
export interface CreateSessionRequest {
  /** 服务ID */
  serviceId: string;
  /** 原始意图（后端必填） */
  originalIntent: string;
  /** 凭证ID（可选） */
  credentialId?: string;
  /** 启动URL */
  startUrl?: string;
  /** 视口宽度 */
  viewportWidth?: number;
  /** 视口高度 */
  viewportHeight?: number;
  /** 用户代理 */
  userAgent?: string;
  /** 超时时间（毫秒） */
  timeout?: number;
}

/** 创建 Session 响应 */
export interface CreateSessionResponse {
  /** Session ID */
  sessionId: string;
  /** Session 状态 */
  status: SessionStatus;
  /** WebSocket URL */
  wsUrl?: string;
  /** 创建时间 */
  createdAt: string;
}

/** Session 信息 */
export interface SessionInfo {
  /** Session ID */
  sessionId: string;
  /** Session 状态 */
  status: SessionStatus;
  /** 服务ID */
  serviceId: string;
  /** 凭证ID */
  credentialId?: string;
  /** 当前URL */
  currentUrl?: string;
  /** 页面标题 */
  pageTitle?: string;
  /** 创建时间 */
  createdAt: string;
  /** 最后活动时间 */
  lastActivityAt?: string;
  /** 过期时间 */
  expiresAt?: string;
}

/** Sandbox 操作详情 */
export interface SandboxOperationInfo {
  operationId: string;
  sessionId: string;
  operationType: string;
  status: string;
  startedAt: string;
  completedAt?: string;
  executionTimeMs?: number;
}

/** Sandbox 统计 */
export interface SandboxStats {
  poolStatus: string;
  activeSessions: number;
  warmInstances: number;
  healthy: boolean;
  error?: string;
}

/** 执行操作请求 */
export type SandboxCredentialField = string;

export interface SandboxCredentialReference {
  /** Credential field name resolved by controlled host operations such as `fill` */
  $credential: SandboxCredentialField;
}

/** Plain string bindings passed into `execute_script` */
export type SandboxScriptBindings = Record<string, string>;

export type BootstrapPageMode = "rocket_loader";

export interface BootstrapPageOptions {
  /** Controlled bootstrap mode. Initial support is limited to Rocket Loader pages. */
  mode?: BootstrapPageMode;
  /** Optional selector overrides for external scripts to re-inject. */
  scriptSelectors?: string[];
  /** Whether to include plain text/javascript external scripts in addition to rewritten ones. */
  includePlainScripts?: boolean;
  /** Whether to replay DOM lifecycle events after bundle reinjection for late-mounted apps. */
  replayLifecycleEvents?: boolean;
  /** Optional selector to wait for after injection completes. */
  waitSelector?: string;
  /** Wait timeout in milliseconds. Defaults to 30000 on the backend. */
  waitTimeoutMs?: number;
}

export interface BootstrapPageResult {
  injectedScripts: string[];
  finalUrl?: string;
  title?: string;
  waitSatisfied?: boolean;
  diagnostics?: Record<string, unknown>;
}

export interface ExecuteOperationRequest {
  /** 操作类型 */
  operationType: OperationType;
  /** 操作描述 */
  description?: string;
  /** 原始操作参数；用于后端新增的浏览器操作字段 */
  parameters?: Record<string, unknown>;
  /** 选择器（CSS选择器或XPath） */
  selector?: string;
  /** 输入值。`fill` 等受控宿主操作支持 credential 引用。 */
  value?: string | SandboxCredentialReference;
  /** URL（用于导航操作） */
  url?: string;
  /** HTTP 方法 */
  method?: string;
  /** HTTP 请求头 */
  headers?: Record<string, string | SandboxCredentialReference>;
  /** HTTP 请求体 */
  body?: unknown;
  /** 脚本（用于执行脚本操作） */
  script?: string;
  /** 脚本绑定。仅支持普通字符串，不支持 credential 引用。 */
  bindings?: SandboxScriptBindings;
  /** 属性名（用于获取属性操作） */
  attribute?: string;
  /** 超时时间（毫秒） */
  timeout?: number;
  /** 等待条件 */
  waitCondition?: {
    /** 可见性 */
    visible?: boolean;
    /** 存在性 */
    attached?: boolean;
  };
}

/** 执行操作响应 */
export interface ExecuteOperationResponse {
  /** 操作ID */
  operationId: string;
  /** 是否成功 */
  success: boolean;
  /** @deprecated Use success. */
  status?: OperationStatus;
  /** 操作结果 */
  data?: unknown;
  /** @deprecated Use data. */
  result?: unknown;
  /** 错误信息 */
  error?: string;
  /** 执行时间（毫秒） */
  executionTimeMs: number;
}

export interface BootstrapPageResponse extends ExecuteOperationResponse {
  data?: BootstrapPageResult;
  result?: BootstrapPageResult;
}

/** Session 操作响应 */
export interface SessionActionResponse {
  /** Session ID */
  sessionId: string;
  /** 操作结果 */
  success: boolean;
  /** @deprecated Use success. */
  closed?: boolean;
  /** 当前状态 */
  status: string;
  /** 消息 */
  message: string;
}

export type DomExportFormat = "html" | "text" | "json";

/** DOM 导出请求 */
export interface DomExportRequest {
  rootSelector?: string;
  format?: DomExportFormat;
  includeText?: boolean;
  includeMetadata?: boolean;
  extraSensitiveSelectors?: string[];
  maxBytes?: number;
}

/** DOM 导出响应 */
export interface DomExportResponse {
  operationId: string;
  success: boolean;
  format: DomExportFormat;
  data?: unknown;
  truncated: boolean;
  error?: string;
  executionTimeMs: number;
}

/** 导出数据请求 */
export interface ExportDataRequest {
  /** 导出格式 */
  format: "json" | "csv" | "pdf";
  /** 数据选择器 */
  selectors?: string[];
  /** @deprecated Use selectors. */
  selector?: string;
  /** @deprecated The backend export API accepts selectors only. */
  extractionRules?: Array<{
    /** 字段名 */
    name: string;
    /** 选择器 */
    selector: string;
    /** 属性（默认为textContent） */
    attribute?: string;
  }>;
}

/** 导出数据响应 */
export interface ExportDataResponse {
  /** 导出 ID */
  exportId: string;
  /** Base64 编码导出数据 */
  dataBase64: string;
  /** 数据格式 */
  format: "json" | "csv" | "pdf";
  /** 文件名 */
  filename: string;
  /** 大小（字节） */
  sizeBytes: number;
}

/** Session 列表响应 */
export interface ListSessionsResponse {
  /** Session 列表 */
  sessions: SessionInfo[];
  /** 总数 */
  total: number;
}

/** Sandbox 配置 */
export interface SandboxConfig {
  /** 默认视口宽度 */
  defaultViewportWidth?: number;
  /** 默认视口高度 */
  defaultViewportHeight?: number;
  /** 默认超时时间（毫秒） */
  defaultTimeout?: number;
  /** 默认用户代理 */
  defaultUserAgent?: string;
}
