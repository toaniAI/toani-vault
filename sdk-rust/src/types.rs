//! CredBridge SDK 核心类型定义
//!
//! 基于 CredBridge API 规范

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

// ============================================================================
// 基础枚举类型
// ============================================================================

/// 凭证类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialType {
    /// 用户名密码
    UsernamePassword,
    /// OAuth 刷新令牌
    OAuthRefresh,
    /// API 密钥
    ApiKey,
    /// 会话 Cookie
    SessionCookie,
    /// KYC 文档
    KycDocument,
    /// 证书
    Certificate,
    /// SSH 密钥
    SshKey,
    /// 数据库连接
    DatabaseConnection,
}

impl std::fmt::Display for CredentialType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CredentialType::UsernamePassword => "username_password",
            CredentialType::OAuthRefresh => "oauth_refresh",
            CredentialType::ApiKey => "api_key",
            CredentialType::SessionCookie => "session_cookie",
            CredentialType::KycDocument => "kyc_document",
            CredentialType::Certificate => "certificate",
            CredentialType::SshKey => "ssh_key",
            CredentialType::DatabaseConnection => "database_connection",
        };
        write!(f, "{}", s)
    }
}

/// Token Scope 权限
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenScope {
    /// 凭证读取权限
    #[serde(rename = "credential:read")]
    CredentialRead,
    /// 凭证解密权限
    #[serde(rename = "credential:decrypt")]
    CredentialDecrypt,
    /// 凭证写入权限
    #[serde(rename = "credential:write")]
    CredentialWrite,
    /// 凭证删除权限
    #[serde(rename = "credential:delete")]
    CredentialDelete,
    /// Token 读取权限
    #[serde(rename = "tokens:read")]
    TokensRead,
    /// Token 写入权限
    #[serde(rename = "tokens:write")]
    TokensWrite,
    /// Token 撤销权限
    #[serde(rename = "tokens:revoke")]
    TokensRevoke,
    /// 审计日志读取权限
    #[serde(rename = "audit:read")]
    AuditRead,
    /// 沙箱读取权限
    #[serde(rename = "sandbox:read")]
    SandboxRead,
    /// 沙箱写入权限
    #[serde(rename = "sandbox:write")]
    SandboxWrite,
    /// 沙箱执行权限
    #[serde(rename = "sandbox:execute")]
    SandboxExecute,
    /// 管理员权限
    #[serde(rename = "admin")]
    Admin,
}

impl std::fmt::Display for TokenScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            TokenScope::CredentialRead => "credential:read",
            TokenScope::CredentialDecrypt => "credential:decrypt",
            TokenScope::CredentialWrite => "credential:write",
            TokenScope::CredentialDelete => "credential:delete",
            TokenScope::TokensRead => "tokens:read",
            TokenScope::TokensWrite => "tokens:write",
            TokenScope::TokensRevoke => "tokens:revoke",
            TokenScope::AuditRead => "audit:read",
            TokenScope::SandboxRead => "sandbox:read",
            TokenScope::SandboxWrite => "sandbox:write",
            TokenScope::SandboxExecute => "sandbox:execute",
            TokenScope::Admin => "admin",
        };
        write!(f, "{}", s)
    }
}

/// 风险等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum RiskTier {
    Low,
    Medium,
    High,
    Critical,
}

/// 操作结果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Outcome {
    Success,
    Failure,
    Denied,
    Timeout,
    Aborted,
}

// ============================================================================
// SDK 配置类型
// ============================================================================

/// SDK 配置选项
#[derive(Debug, Clone)]
pub struct CredBridgeConfig {
    /// API 基础 URL
    pub base_url: String,
    /// API Token (PASETO v4.local)
    pub token: Option<String>,
    /// 租户 ID
    pub tenant_id: Option<String>,
    /// 用户 ID
    pub user_id: Option<String>,
    /// 请求超时时间（毫秒，默认 30000）
    pub timeout_ms: u64,
    /// 最大重试次数（默认 3）
    pub max_retries: u32,
    /// 是否自动刷新 Token（默认 true）
    pub auto_refresh_token: bool,
    /// Token 刷新缓冲时间（毫秒，默认 5分钟）
    pub token_refresh_buffer_ms: u64,
    /// 自定义请求头
    pub headers: HashMap<String, String>,
    /// 请求签名密钥（可选）
    pub signing_key: Option<String>,
}

impl Default for CredBridgeConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            token: None,
            tenant_id: None,
            user_id: None,
            timeout_ms: 30000,
            max_retries: 3,
            auto_refresh_token: true,
            token_refresh_buffer_ms: 5 * 60 * 1000, // 5 minutes
            headers: HashMap::new(),
            signing_key: None,
        }
    }
}

impl CredBridgeConfig {
    /// 创建新的配置
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            ..Default::default()
        }
    }

    /// 设置 Token
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// 设置租户 ID
    pub fn with_tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// 设置用户 ID
    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// 设置超时时间
    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// 设置最大重试次数
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// 设置是否自动刷新 Token
    pub fn with_auto_refresh_token(mut self, auto_refresh: bool) -> Self {
        self.auto_refresh_token = auto_refresh;
        self
    }
}

// ============================================================================
// API 请求/响应类型
// ============================================================================

/// 创建凭证请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCredentialRequest {
    /// 服务 ID
    #[serde(rename = "service_id")]
    pub service_id: String,
    /// 凭证类型
    #[serde(rename = "credential_type")]
    pub credential_type: CredentialType,
    /// 明文凭证内容（将被加密）
    #[serde(rename = "plaintext_data")]
    pub plaintext_data: HashMap<String, serde_json::Value>,
    /// 过期时间（Unix 时间戳，可选）
    #[serde(rename = "expires_at", skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

/// 创建凭证响应
#[derive(Debug, Clone, Deserialize)]
pub struct CreateCredentialResponse {
    /// 凭证 ID
    #[serde(rename = "credential_id")]
    pub credential_id: String,
    /// 服务 ID
    #[serde(rename = "service_id")]
    pub service_id: String,
    /// 凭证类型
    #[serde(rename = "credential_type")]
    pub credential_type: String,
    /// 创建时间
    #[serde(rename = "created_at")]
    pub created_at: String,
    /// 过期时间
    #[serde(rename = "expires_at", skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// 凭证元数据
#[derive(Debug, Clone, Deserialize)]
pub struct CredentialMetadata {
    /// 凭证 ID
    #[serde(rename = "credential_id")]
    pub credential_id: String,
    /// 凭证类型
    #[serde(rename = "credential_type")]
    pub credential_type: CredentialType,
    /// 用户 ID 哈希
    #[serde(rename = "user_id_hash")]
    pub user_id_hash: String,
    /// 服务 ID
    #[serde(rename = "service_id")]
    pub service_id: String,
    /// 租户 ID
    #[serde(rename = "tenant_id")]
    pub tenant_id: String,
    /// 创建时间
    #[serde(rename = "created_at")]
    pub created_at: String,
    /// 过期时间
    #[serde(rename = "expires_at", skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// 是否已删除
    #[serde(rename = "is_deleted")]
    pub is_deleted: bool,
}

/// 凭证列表响应
#[derive(Debug, Clone, Deserialize)]
pub struct ListCredentialsResponse {
    /// 凭证列表
    pub credentials: Vec<CredentialMetadata>,
    /// 总数
    pub total: u32,
}

/// 凭证详情响应
#[derive(Debug, Clone, Deserialize)]
pub struct GetCredentialResponse {
    /// 凭证 ID
    #[serde(rename = "credential_id")]
    pub credential_id: String,
    /// 服务 ID
    #[serde(rename = "service_id")]
    pub service_id: String,
    /// 凭证类型
    #[serde(rename = "credential_type")]
    pub credential_type: String,
    /// 创建时间
    #[serde(rename = "created_at")]
    pub created_at: String,
    /// 过期时间
    #[serde(rename = "expires_at", skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// 是否已删除
    #[serde(rename = "is_deleted")]
    pub is_deleted: bool,
    /// 加密载荷
    #[serde(rename = "encrypted_payload", skip_serializing_if = "Option::is_none")]
    pub encrypted_payload: Option<EncryptedPayload>,
}

/// 解密凭证请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecryptCredentialRequest {
    /// 请求解密的理由（用于审计）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// 解密凭证响应
#[derive(Debug, Clone, Deserialize)]
pub struct DecryptCredentialResponse {
    /// 凭证 ID
    #[serde(rename = "credential_id")]
    pub credential_id: String,
    /// 服务 ID
    #[serde(rename = "service_id")]
    pub service_id: String,
    /// 凭证类型
    #[serde(rename = "credential_type")]
    pub credential_type: String,
    /// 解密的明文数据
    #[serde(rename = "plaintext_data")]
    pub plaintext_data: HashMap<String, serde_json::Value>,
}

/// 删除凭证响应
#[derive(Debug, Clone, Deserialize)]
pub struct DeleteCredentialResponse {
    /// 凭证 ID
    #[serde(rename = "credential_id")]
    pub credential_id: String,
    /// 是否已删除
    pub deleted: bool,
}

/// 更新凭证请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCredentialRequest {
    /// 新的明文凭证内容
    #[serde(rename = "plaintext_data")]
    pub plaintext_data: serde_json::Value,
    /// 变更原因
    #[serde(rename = "change_reason", skip_serializing_if = "Option::is_none")]
    pub change_reason: Option<String>,
    /// 期望的版本号
    #[serde(rename = "expected_version", skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<u32>,
}

/// 更新凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCredentialResponse {
    /// 凭证 ID
    #[serde(rename = "credential_id")]
    pub credential_id: String,
    /// 新版本号
    pub version: u32,
    /// 服务 ID
    #[serde(rename = "service_id")]
    pub service_id: String,
    /// 凭证类型
    #[serde(rename = "credential_type")]
    pub credential_type: String,
    /// 更新时间
    #[serde(rename = "updated_at")]
    pub updated_at: String,
    /// 上一版本号
    #[serde(rename = "previous_version")]
    pub previous_version: u32,
}

/// 版本摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionSummary {
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub changed_by: Option<String>,
    pub change_reason: Option<String>,
}

/// 凭证版本历史
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionHistory {
    pub credential_id: String,
    pub current_version: u32,
    pub versions: Vec<VersionSummary>,
    pub total: usize,
}

/// 版本元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionMetadata {
    pub service_id: String,
    pub credential_type: String,
    pub algorithm: String,
}

/// 版本详情
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDetail {
    pub credential_id: String,
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub changed_by: Option<String>,
    pub change_reason: Option<String>,
    pub metadata: VersionMetadata,
}

/// 回滚请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackCredentialRequest {
    pub target_version: u32,
    pub reason: String,
}

/// 回滚响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackCredentialResponse {
    pub credential_id: String,
    pub previous_version: u32,
    pub current_version: u32,
    pub rollback_to_version: u32,
    pub rollback_at: String,
    pub reason: String,
}

/// 加密载荷结构
#[derive(Debug, Clone, Deserialize)]
pub struct EncryptedPayload {
    /// 协议版本
    pub version: u32,
    /// 加密算法
    pub algorithm: String,
    /// KDF 算法
    pub kdf: String,
    /// Nonce（base64）
    pub nonce: String,
    /// Auth Tag（base64）
    #[serde(rename = "auth_tag")]
    pub auth_tag: String,
    /// 密文（base64）
    pub ciphertext: String,
}

/// 凭证列表过滤条件
#[derive(Debug, Clone, Default)]
pub struct CredentialFilter {
    /// 按服务 ID 过滤
    pub service_id: Option<String>,
    /// 按凭证类型过滤
    pub credential_type: Option<CredentialType>,
    /// 包含已删除的凭证
    pub include_deleted: Option<bool>,
    /// 仅返回未过期的凭证
    pub only_valid: Option<bool>,
}

// ============================================================================
// Token 类型
// ============================================================================

/// Token Claims
#[derive(Debug, Clone, Deserialize)]
pub struct TokenClaims {
    /// 颁发者
    pub iss: String,
    /// 主题（租户ID:用户ID）
    pub sub: String,
    /// 受众（租户ID）
    pub aud: String,
    /// 过期时间（Unix 时间戳）
    pub exp: i64,
    /// 生效时间（Unix 时间戳）
    pub nbf: i64,
    /// 颁发时间（Unix 时间戳）
    pub iat: i64,
    /// JWT ID
    pub jti: String,
    /// 权限范围
    pub scope: String,
    /// 租户 ID
    #[serde(rename = "tenant_id")]
    pub tenant_id: String,
    /// 允许的凭证 ID 列表（可选）
    #[serde(rename = "credential_ids", skip_serializing_if = "Option::is_none")]
    pub credential_ids: Option<Vec<String>>,
    /// 绑定 IP（可选）
    #[serde(rename = "ip_bound", skip_serializing_if = "Option::is_none")]
    pub ip_bound: Option<String>,
    /// MFA 验证状态
    #[serde(rename = "mfa_verified")]
    pub mfa_verified: bool,
}

/// Token 信息
#[derive(Debug, Clone)]
pub struct TokenInfo {
    /// Token ID
    pub token_id: String,
    /// 主题
    pub subject: String,
    /// 租户 ID
    pub tenant_id: String,
    /// 用户 ID
    pub user_id: String,
    /// 过期时间（Unix 时间戳）
    pub expires_at: i64,
    /// 授权 Scope 列表
    pub scopes: Vec<TokenScope>,
    /// 颁发时间（Unix 时间戳）
    pub issued_at: i64,
}

/// 创建 token 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTokenRequest {
    pub user_id: Option<String>,
    pub scopes: Vec<String>,
    pub expires_in: Option<u64>,
    pub credential_ids: Option<Vec<String>>,
}

/// 创建 token 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTokenResponse {
    pub access_token: String,
    pub token_id: String,
    pub token_type: String,
    pub expires_in: u64,
    pub scope: String,
    pub issued_at: u64,
    pub expires_at: u64,
}

/// 创建 access token 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAccessTokenResponse {
    pub access_token: String,
    pub token_id: String,
    pub token_type: String,
    pub expires_at: u64,
    pub expires_in: u64,
    pub granted_scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAutomationTokenRequest {
    pub name: String,
    pub description: Option<String>,
    pub scopes: Vec<String>,
    pub ttl_seconds: Option<u64>,
    pub created_via: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAutomationTokenResponse {
    pub token_value: String,
    pub token_preview: String,
    #[serde(flatten)]
    pub metadata: ApiTokenMetadata,
}

/// Token 列表项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenListItem {
    pub token_id: String,
    pub user_id: String,
    pub tenant_id: String,
    pub scopes: Vec<String>,
    pub issued_at: u64,
    pub expires_at: u64,
    pub revoked: bool,
}

/// Token 列表响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTokensResponse {
    pub tokens: Vec<ApiTokenMetadata>,
}

/// API token 元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiTokenMetadata {
    pub token_id: String,
    pub token_kind: String,
    pub token_name: Option<String>,
    pub token_prefix: Option<String>,
    pub token_type: String,
    pub subject_type: String,
    pub subject_id: String,
    pub tenant_id: String,
    pub issued_from: String,
    pub session_id: Option<String>,
    pub membership_id: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub granted_scopes: Vec<String>,
    pub issued_membership_role_snapshot: Option<String>,
    pub permission_source: Option<String>,
    pub created_via: Option<String>,
    pub revoked_reason: Option<String>,
    pub expires_at: String,
    pub revoked_at: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

/// Service Account 信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceAccountInfo {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub role: String,
    pub scope_ceiling: Vec<String>,
    pub status: String,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

/// 创建 Service Account 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateServiceAccountRequest {
    pub name: String,
    pub description: Option<String>,
    pub scope_ceiling: Vec<String>,
}

/// 更新 Service Account 请求
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateServiceAccountRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub scope_ceiling: Option<Vec<String>>,
}

/// 创建 Service Account token 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateServiceAccountTokenRequest {
    pub scopes: Vec<String>,
    pub ttl_seconds: Option<u64>,
    pub display_name: Option<String>,
}

/// 创建 Service Account token 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateServiceAccountTokenResponse {
    pub access_token: String,
    pub token_id: String,
    pub token_type: String,
    pub subject_type: String,
    pub issued_from: String,
    pub display_name: Option<String>,
    pub expires_in: u64,
    pub scope: String,
    pub granted_scopes: Vec<String>,
    pub issued_at: u64,
    pub expires_at: u64,
    pub revoked_at: Option<String>,
}

/// Token 撤销响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokeTokenResponse {
    pub revoked: bool,
    pub token_id: String,
}

/// Token 统计响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenStatsResponse {
    pub active_tokens: u64,
}

/// Token 刷新结果
#[derive(Debug, Clone)]
pub struct TokenRefreshResult {
    /// 新的 Token
    pub token: String,
    /// Token 信息
    pub token_info: TokenInfo,
    /// 过期时间
    pub expires_at: i64,
}

// ============================================================================
// 错误类型
// ============================================================================

/// SDK 错误码
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredBridgeErrorCode {
    /// 未知错误
    Unknown,
    /// 网络错误
    NetworkError,
    /// 请求超时
    Timeout,
    /// 未授权
    Unauthorized,
    /// 禁止访问
    Forbidden,
    /// 凭证未找到
    NotFound,
    /// 无效的请求
    InvalidRequest,
    /// 服务器内部错误
    InternalError,
    /// Token 过期
    TokenExpired,
    /// Token 无效
    InvalidToken,
    /// Token 已被撤销
    TokenRevoked,
    /// 权限不足
    InsufficientScope,
    /// 租户隔离违规
    TenantIsolationViolation,
    /// 凭证已过期
    CredentialExpired,
    /// 解密失败
    DecryptionFailed,
    /// 加密失败
    EncryptionFailed,
}

impl std::fmt::Display for CredBridgeErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CredBridgeErrorCode::Unknown => "unknown",
            CredBridgeErrorCode::NetworkError => "network_error",
            CredBridgeErrorCode::Timeout => "timeout",
            CredBridgeErrorCode::Unauthorized => "unauthorized",
            CredBridgeErrorCode::Forbidden => "forbidden",
            CredBridgeErrorCode::NotFound => "not_found",
            CredBridgeErrorCode::InvalidRequest => "invalid_request",
            CredBridgeErrorCode::InternalError => "internal_error",
            CredBridgeErrorCode::TokenExpired => "token_expired",
            CredBridgeErrorCode::InvalidToken => "invalid_token",
            CredBridgeErrorCode::TokenRevoked => "token_revoked",
            CredBridgeErrorCode::InsufficientScope => "insufficient_scope",
            CredBridgeErrorCode::TenantIsolationViolation => "tenant_isolation_violation",
            CredBridgeErrorCode::CredentialExpired => "credential_expired",
            CredBridgeErrorCode::DecryptionFailed => "decryption_failed",
            CredBridgeErrorCode::EncryptionFailed => "encryption_failed",
        };
        write!(f, "{}", s)
    }
}

/// SDK 错误
#[derive(Debug, Error, Clone)]
#[error("CredBridge error [{code}]: {message}")]
pub struct CredBridgeError {
    /// 错误码
    pub code: CredBridgeErrorCode,
    /// 错误消息
    pub message: String,
    /// HTTP 状态码
    pub status_code: Option<u16>,
    /// 错误详情
    pub details: Option<HashMap<String, serde_json::Value>>,
    /// 请求 ID
    pub request_id: Option<String>,
}

impl CredBridgeError {
    /// 创建新的错误
    pub fn new(code: CredBridgeErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            status_code: None,
            details: None,
            request_id: None,
        }
    }

    /// 设置 HTTP 状态码
    pub fn with_status_code(mut self, status_code: u16) -> Self {
        self.status_code = Some(status_code);
        self
    }

    /// 设置错误详情
    pub fn with_details(mut self, details: HashMap<String, serde_json::Value>) -> Self {
        self.details = Some(details);
        self
    }

    /// 设置请求 ID
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    /// 是否为网络错误
    pub fn is_network_error(&self) -> bool {
        matches!(
            self.code,
            CredBridgeErrorCode::NetworkError | CredBridgeErrorCode::Timeout
        )
    }

    /// 是否为认证错误
    pub fn is_auth_error(&self) -> bool {
        matches!(
            self.code,
            CredBridgeErrorCode::Unauthorized
                | CredBridgeErrorCode::InvalidToken
                | CredBridgeErrorCode::TokenExpired
                | CredBridgeErrorCode::TokenRevoked
        )
    }

    /// 是否可重试
    pub fn is_retryable(&self) -> bool {
        self.is_network_error()
            || matches!(self.code, CredBridgeErrorCode::InternalError)
            || self.status_code == Some(429)
    }
}

/// SDK 结果类型
pub type Result<T> = std::result::Result<T, CredBridgeError>;

// ============================================================================
// API 通用响应类型
// ============================================================================

/// API 响应元数据
#[derive(Debug, Clone, Deserialize)]
pub struct ApiMeta {
    /// 请求 ID
    #[serde(rename = "request_id")]
    pub request_id: String,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
}

/// API 错误详情
#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorDetails {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<HashMap<String, serde_json::Value>>,
}

/// API 错误响应
#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorResponse {
    pub error: ApiErrorDetails,
    pub meta: ApiMeta,
}

// ============================================================================
// 请求选项类型
// ============================================================================

/// 请求选项
#[derive(Debug, Clone, Default)]
pub struct RequestOptions {
    /// 请求超时时间（毫秒）
    pub timeout_ms: Option<u64>,
    /// 是否跳过重试
    pub skip_retry: bool,
    /// 重试次数
    pub retries: Option<u32>,
    /// 自定义请求头
    pub headers: HashMap<String, String>,
    /// 请求 ID
    pub request_id: Option<String>,
}

impl RequestOptions {
    /// 创建默认请求选项
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置超时时间
    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    /// 设置是否跳过重试
    pub fn with_skip_retry(mut self, skip_retry: bool) -> Self {
        self.skip_retry = skip_retry;
        self
    }

    /// 设置重试次数
    pub fn with_retries(mut self, retries: u32) -> Self {
        self.retries = Some(retries);
        self
    }

    /// 添加自定义请求头
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// 设置请求 ID
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }
}

// ============================================================================
// 审计日志类型
// ============================================================================

/// 审计日志条目
#[derive(Debug, Clone, Deserialize)]
pub struct AuditLogEntry {
    /// 日志 ID
    pub id: String,
    /// 用户 ID 哈希
    #[serde(rename = "user_id_hash")]
    pub user_id_hash: String,
    /// 时间戳
    pub timestamp: i64,
    /// 会话 ID
    #[serde(rename = "session_id")]
    pub session_id: String,
    /// 服务名称
    pub service: String,
    /// 操作类型
    pub action: String,
    /// 风险等级
    #[serde(rename = "risk_tier")]
    pub risk_tier: RiskTier,
    /// 操作结果
    pub outcome: Outcome,
    /// TEE MRENCLAVE 测量值
    #[serde(rename = "tee_mrenclave")]
    pub tee_mrenclave: String,
    /// Action Token JTI
    #[serde(rename = "action_token_jti")]
    pub action_token_jti: String,
    /// 链索引
    #[serde(rename = "chain_index")]
    pub chain_index: u64,
    /// Merkle Tree 根哈希
    #[serde(rename = "merkle_root")]
    pub merkle_root: String,
}

/// 审计日志列表响应
#[derive(Debug, Clone, Deserialize)]
pub struct ListAuditLogsResponse {
    pub entries: Vec<AuditLogEntry>,
    pub total: u32,
    pub limit: u32,
    pub offset: u32,
    #[serde(rename = "has_more")]
    pub has_more: bool,
}

/// 审计日志查询过滤
#[derive(Debug, Clone, Default)]
pub struct AuditLogFilter {
    pub start_time: Option<u64>,
    pub end_time: Option<u64>,
    pub action: Option<String>,
    pub risk_tier: Option<RiskTier>,
    pub user_id_hash: Option<String>,
    pub outcome: Option<Outcome>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// 审计日志列表项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogListItem {
    pub id: String,
    pub timestamp: u64,
    pub user_id_hash: String,
    pub session_id: String,
    pub service: String,
    pub action: String,
    pub risk_tier: RiskTier,
    pub outcome: Outcome,
    pub log_index: u64,
}

/// 审计日志列表数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogListData {
    pub items: Vec<AuditLogListItem>,
    pub total: u64,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

/// 审计日志查询响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogsResponse {
    pub success: bool,
    pub data: AuditLogListData,
    pub error: Option<String>,
}

/// 审计导出格式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuditExportFormat {
    Json,
    Csv,
}

/// 审计导出请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditExportRequest {
    pub start_time: Option<u64>,
    pub end_time: Option<u64>,
    pub format: AuditExportFormat,
    pub user_id_hash: Option<String>,
    pub action: Option<String>,
}

/// 审计导出数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditExportData {
    pub export_id: String,
    pub format: AuditExportFormat,
    pub content: String,
    pub integrity_hash: String,
    pub count: u64,
    pub generated_at: u64,
}

/// 审计导出响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditExportResponse {
    pub success: bool,
    pub data: Option<AuditExportData>,
    pub error: Option<String>,
}

/// 审计校验请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditVerifyRequest {
    pub id: Option<String>,
    pub log_index: Option<u64>,
}

/// 审计校验详情
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditVerifyDetail {
    pub step: String,
    pub passed: bool,
    pub message: Option<String>,
}

/// 审计校验数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditVerifyData {
    pub id: String,
    pub log_index: u64,
    pub verified: bool,
    pub content_hash_match: bool,
    pub signature_valid: bool,
    pub merkle_proof_valid: bool,
    pub details: Vec<AuditVerifyDetail>,
    pub verified_at: u64,
}

/// 审计校验响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditVerifyResponse {
    pub success: bool,
    pub data: Option<AuditVerifyData>,
    pub error: Option<String>,
}

/// 沙箱创建会话请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSandboxSessionRequest {
    pub credential_id: String,
    pub original_intent: String,
    pub metadata: Option<HashMap<String, String>>,
}

/// 沙箱创建会话响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSandboxSessionResponse {
    pub session_id: String,
    pub sandbox_id: String,
    pub status: String,
    pub created_at: String,
    pub expires_at: String,
}

/// 沙箱会话摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxSessionSummary {
    pub session_id: String,
    pub sandbox_id: String,
    pub credential_id: String,
    pub status: String,
    pub original_intent: String,
    pub created_at: String,
    pub expires_at: String,
    pub is_expired: bool,
}

/// 沙箱会话列表响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxSessionsResponse {
    pub sessions: Vec<SandboxSessionSummary>,
    pub total: usize,
}

/// 沙箱会话详情
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxSessionDetail {
    pub session_id: String,
    pub sandbox_id: String,
    pub tenant_id: String,
    pub user_id: String,
    pub credential_id: String,
    pub original_intent: String,
    pub status: String,
    pub created_at: String,
    pub expires_at: String,
    pub last_activity_at: String,
    pub is_expired: bool,
}

/// 沙箱操作类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxOperationType {
    Navigate,
    Click,
    Fill,
    GetText,
    Screenshot,
    Export,
    ExecuteScript,
    Wait,
    HttpRequest,
    Custom,
}

impl std::fmt::Display for SandboxOperationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            SandboxOperationType::Navigate => "navigate",
            SandboxOperationType::Click => "click",
            SandboxOperationType::Fill => "fill",
            SandboxOperationType::GetText => "get_text",
            SandboxOperationType::Screenshot => "screenshot",
            SandboxOperationType::Export => "export",
            SandboxOperationType::ExecuteScript => "execute_script",
            SandboxOperationType::Wait => "wait",
            SandboxOperationType::HttpRequest => "http_request",
            SandboxOperationType::Custom => "custom",
        };
        write!(f, "{value}")
    }
}

/// 沙箱执行请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteSandboxOperationRequest {
    pub operation_type: SandboxOperationType,
    pub description: String,
    pub parameters: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxCredentialReference {
    #[serde(rename = "$credential")]
    pub field: String,
}

/// 沙箱执行响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteSandboxOperationResponse {
    pub operation_id: String,
    pub success: bool,
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
    pub execution_time_ms: u64,
}

/// 沙箱操作详情
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxOperationDetail {
    pub operation_id: String,
    pub session_id: String,
    pub operation_type: String,
    pub status: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub execution_time_ms: Option<u64>,
}

/// 沙箱会话操作响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxSessionActionResponse {
    pub session_id: String,
    pub success: bool,
    pub status: String,
    pub message: String,
}

/// 沙箱健康/统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxStatsResponse {
    pub pool_status: String,
    pub active_sessions: usize,
    pub warm_instances: usize,
    pub healthy: bool,
    pub error: Option<String>,
}

/// 通用 API 成功包装
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSuccess<T> {
    pub success: bool,
    pub data: T,
}
