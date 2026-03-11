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
    /// 审计日志读取权限
    #[serde(rename = "audit:read")]
    AuditRead,
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
            TokenScope::AuditRead => "audit:read",
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
        matches!(self.code, CredBridgeErrorCode::NetworkError | CredBridgeErrorCode::Timeout)
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
