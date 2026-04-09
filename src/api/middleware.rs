//! API 认证中间件
//!
//! 实现 Session Token 和 Privy Token 验证中间件：
//! - 支持 Privy Access Token（用于创建会话）
//! - 支持 Session Token（用于 API 访问）
//! - Scope 权限控制
//! - Membership-based tenant isolation
//!
//! # Token 类型
//!
//! 1. **Privy Access Token**: 前端从 Privy 获取，用于创建会话
//!    - 格式: Privy JWT
//!    - 用途: POST /auth/session
//!
//! 2. **Session Token**: 后端生成，用于 API 访问
//!    - 格式: PASETO v4.local 或内部 Token
//!    - 用途: 所有需要认证的 API 请求
//!    - 包含: user_id, tenant_id, membership_id, scopes

use axum::{
    Json,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use super::context::RequestContext;
use super::i18n::{
    DEFAULT_LOCALE, I18nMetadata, I18nParams, resolve_accept_language, set_content_language,
    translate,
};
use serde_json::Value;
use std::sync::Arc;

use super::token_blacklist::TokenStore;
use crate::auth::AuthService;
use crate::token::{
    TOKEN_ISSUED_FROM_SESSION, TOKEN_SUBJECT_TYPE_SERVICE_ACCOUNT, TOKEN_SUBJECT_TYPE_USER,
};

const UNASSIGNED_TENANT_ID: &str = "00000000-0000-0000-0000-000000000000";

/// Token Scope 定义
///
/// 基于 MembershipRole 的权限范围
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenScope {
    /// 凭证读取权限
    CredentialRead,
    /// 凭证解密权限
    CredentialDecrypt,
    /// 凭证写入权限（创建/删除）
    CredentialWrite,
    /// 凭证删除权限
    CredentialDelete,
    /// 审计日志读取权限
    AuditRead,
    /// 沙箱执行权限
    SandboxExecute,
    /// 沙箱读取权限
    SandboxRead,
    /// 沙箱写入权限（创建/删除）
    SandboxWrite,
    /// 租户管理权限
    TenantAdmin,
    /// 租户读取权限
    TenantRead,
    /// 租户写入权限
    TenantWrite,
    /// 租户删除权限
    TenantDelete,
    /// 成员管理权限
    MembersRead,
    /// 成员写入权限
    MembersWrite,
    /// 成员邀请权限
    MembersInvite,
    /// 邀请管理权限
    InvitationsRead,
    /// 邀请写入权限
    InvitationsWrite,
    /// Token 读取权限
    TokensRead,
    /// Token 写入权限（创建）
    TokensWrite,
    /// Token 撤销权限
    TokensRevoke,
    /// 用户管理权限
    UsersManage,
    /// 角色管理权限
    RolesManage,
    /// 管理员权限（超级权限）
    Admin,
}

impl TokenScope {
    /// 获取 scope 字符串表示
    pub fn as_str(&self) -> &'static str {
        match self {
            TokenScope::CredentialRead => "credential:read",
            TokenScope::CredentialDecrypt => "credential:decrypt",
            TokenScope::CredentialWrite => "credential:write",
            TokenScope::CredentialDelete => "credential:delete",
            TokenScope::AuditRead => "audit:read",
            TokenScope::SandboxExecute => "sandbox:execute",
            TokenScope::SandboxRead => "sandbox:read",
            TokenScope::SandboxWrite => "sandbox:write",
            TokenScope::TenantAdmin => "tenant:admin",
            TokenScope::TenantRead => "tenant:read",
            TokenScope::TenantWrite => "tenant:write",
            TokenScope::TenantDelete => "tenant:delete",
            TokenScope::MembersRead => "members:read",
            TokenScope::MembersWrite => "members:write",
            TokenScope::MembersInvite => "members:invite",
            TokenScope::InvitationsRead => "invitations:read",
            TokenScope::InvitationsWrite => "invitations:write",
            TokenScope::TokensRead => "tokens:read",
            TokenScope::TokensWrite => "tokens:write",
            TokenScope::TokensRevoke => "tokens:revoke",
            TokenScope::UsersManage => "users:manage",
            TokenScope::RolesManage => "roles:manage",
            TokenScope::Admin => "admin",
        }
    }

    /// 从字符串解析 scope
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "credential:read" => Some(TokenScope::CredentialRead),
            "credential:decrypt" => Some(TokenScope::CredentialDecrypt),
            "credential:write" => Some(TokenScope::CredentialWrite),
            "credential:delete" => Some(TokenScope::CredentialDelete),
            "audit:read" => Some(TokenScope::AuditRead),
            "sandbox:execute" => Some(TokenScope::SandboxExecute),
            "sandbox:read" => Some(TokenScope::SandboxRead),
            "sandbox:write" => Some(TokenScope::SandboxWrite),
            "tenant:admin" => Some(TokenScope::TenantAdmin),
            "tenant:read" => Some(TokenScope::TenantRead),
            "tenant:write" => Some(TokenScope::TenantWrite),
            "tenant:delete" => Some(TokenScope::TenantDelete),
            "members:read" => Some(TokenScope::MembersRead),
            "members:write" => Some(TokenScope::MembersWrite),
            "members:invite" => Some(TokenScope::MembersInvite),
            "invitations:read" => Some(TokenScope::InvitationsRead),
            "invitations:write" => Some(TokenScope::InvitationsWrite),
            "tokens:read" => Some(TokenScope::TokensRead),
            "tokens:write" => Some(TokenScope::TokensWrite),
            "tokens:revoke" => Some(TokenScope::TokensRevoke),
            "users:manage" => Some(TokenScope::UsersManage),
            "roles:manage" => Some(TokenScope::RolesManage),
            "admin" => Some(TokenScope::Admin),
            // Legacy compatibility mappings
            "credentials:read" => Some(TokenScope::CredentialRead),
            "credentials:write" => Some(TokenScope::CredentialWrite),
            "credentials:delete" => Some(TokenScope::CredentialDelete),
            _ => None,
        }
    }

    /// 从 MembershipRole 的默认 scopes 转换
    pub fn from_membership_scopes(scope_str: &str) -> Vec<TokenScope> {
        scope_str
            .split_whitespace()
            .filter_map(Self::parse)
            .collect()
    }

    /// 从 MembershipRole 映射到 TokenScope 列表
    ///
    /// # 映射规则
    /// - **owner**: 所有权限（包含 Admin）
    /// - **admin**: credential:*, sandbox:*, tokens:*, audit:read, members:*, invitations:*
    /// - **member**: credential:read/write/decrypt, sandbox:*, tokens:read, tokens:write
    /// - **readonly**: credential:read, tokens:read
    pub fn from_role(role: crate::auth::models::MembershipRole) -> Vec<TokenScope> {
        use crate::auth::models::MembershipRole;
        match role {
            MembershipRole::Owner => vec![
                TokenScope::Admin,
                TokenScope::TenantRead,
                TokenScope::TenantWrite,
                TokenScope::TenantAdmin,
                TokenScope::TenantDelete,
                TokenScope::CredentialRead,
                TokenScope::CredentialDecrypt,
                TokenScope::CredentialWrite,
                TokenScope::CredentialDelete,
                TokenScope::SandboxRead,
                TokenScope::SandboxWrite,
                TokenScope::SandboxExecute,
                TokenScope::AuditRead,
                TokenScope::MembersRead,
                TokenScope::MembersWrite,
                TokenScope::MembersInvite,
                TokenScope::InvitationsRead,
                TokenScope::InvitationsWrite,
                TokenScope::TokensRead,
                TokenScope::TokensWrite,
                TokenScope::TokensRevoke,
                TokenScope::UsersManage,
                TokenScope::RolesManage,
            ],
            MembershipRole::Admin => vec![
                TokenScope::TenantRead,
                TokenScope::TenantWrite,
                TokenScope::TenantAdmin,
                TokenScope::CredentialRead,
                TokenScope::CredentialDecrypt,
                TokenScope::CredentialWrite,
                TokenScope::CredentialDelete,
                TokenScope::SandboxRead,
                TokenScope::SandboxWrite,
                TokenScope::SandboxExecute,
                TokenScope::AuditRead,
                TokenScope::MembersRead,
                TokenScope::MembersWrite,
                TokenScope::MembersInvite,
                TokenScope::InvitationsRead,
                TokenScope::InvitationsWrite,
                TokenScope::TokensRead,
                TokenScope::TokensWrite,
                TokenScope::TokensRevoke,
                TokenScope::UsersManage,
            ],
            MembershipRole::Member => vec![
                TokenScope::TenantRead,
                TokenScope::CredentialRead,
                TokenScope::CredentialDecrypt,
                TokenScope::CredentialWrite,
                TokenScope::SandboxRead,
                TokenScope::SandboxWrite,
                TokenScope::SandboxExecute,
                TokenScope::AuditRead,
                TokenScope::TokensRead,
                TokenScope::TokensWrite,
            ],
            MembershipRole::Readonly => vec![
                TokenScope::TenantRead,
                TokenScope::CredentialRead,
                TokenScope::TokensRead,
            ],
        }
    }
}

impl std::str::FromStr for TokenScope {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or(())
    }
}

/// 验证后的 Token 信息
///
/// 包含从 Session Token 或 Privy Token 中提取的用户身份和权限信息。
/// 支持 membership-based 的租户隔离和权限控制。
#[derive(Debug, Clone)]
pub struct ValidatedToken {
    /// Token ID (jti)
    pub token_id: String,
    /// 主题（租户ID:用户ID）
    pub subject: String,
    /// 租户 ID（从 membership 中获取）
    pub tenant_id: String,
    /// 用户 ID
    pub user_id: String,
    /// Token 有效期（秒）
    pub expires_at: u64,
    /// 授权 Scope 列表（从 membership 中获取）
    pub scopes: Vec<TokenScope>,
    /// 签发时间
    pub issued_at: u64,
    /// 成员资格 ID（可选，用于 membership-based 隔离）
    pub membership_id: Option<String>,
    /// 额外元数据（如 session_id, identity_id 等）
    pub metadata: HashMap<String, String>,
    /// 主体类型
    pub subject_type: String,
    /// 令牌来源
    pub issued_from: String,
}

impl ValidatedToken {
    /// 检查是否包含指定 scope
    pub fn has_scope(&self, scope: &TokenScope) -> bool {
        self.scopes.contains(scope) || self.scopes.contains(&TokenScope::Admin)
    }

    /// 检查是否包含任一指定 scope
    pub fn has_any_scope(&self, scopes: &[TokenScope]) -> bool {
        scopes.iter().any(|s| self.has_scope(s))
    }

    /// 检查是否已过期
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now > self.expires_at
    }

    /// 获取 membership_id（如果存在）
    pub fn membership_id(&self) -> Option<&str> {
        self.metadata.get("membership_id").map(|s| s.as_str())
    }

    /// 获取 session_id（如果存在）
    pub fn session_id(&self) -> Option<&str> {
        self.metadata.get("session_id").map(|s| s.as_str())
    }

    pub fn subject_type(&self) -> &str {
        &self.subject_type
    }

    /// 获取当前主体 ID（user_id 或 service_account_id）
    pub fn principal_id(&self) -> &str {
        &self.user_id
    }

    pub fn issued_from(&self) -> &str {
        &self.issued_from
    }

    pub fn is_user_subject(&self) -> bool {
        self.subject_type == TOKEN_SUBJECT_TYPE_USER
    }

    pub fn is_service_account_subject(&self) -> bool {
        self.subject_type == TOKEN_SUBJECT_TYPE_SERVICE_ACCOUNT
    }

    /// 创建用于测试的模拟 Token
    #[cfg(test)]
    pub fn mock(tenant_id: &str, user_id: &str, scopes: Vec<TokenScope>) -> Self {
        Self {
            token_id: uuid::Uuid::now_v7().to_string(),
            subject: format!("{tenant_id}:{user_id}"),
            tenant_id: tenant_id.to_string(),
            user_id: user_id.to_string(),
            expires_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 3600,
            scopes,
            issued_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            membership_id: None,
            metadata: HashMap::new(),
            subject_type: TOKEN_SUBJECT_TYPE_USER.to_string(),
            issued_from: TOKEN_ISSUED_FROM_SESSION.to_string(),
        }
    }
}

/// Token 验证错误
#[derive(Debug, Clone, Serialize)]
pub struct AuthError {
    pub error: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub i18n: Option<I18nMetadata>,
    pub locale: String,
}

impl AuthError {
    pub fn new(error: impl Into<String>, locale: &str, key: &str, params: I18nParams) -> Self {
        let error = error.into();
        Self {
            message: translate(locale, key, &params),
            i18n: Some(I18nMetadata::new(key).with_params(params)),
            error,
            locale: locale.to_string(),
        }
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let status = match self.error.as_str() {
            "missing_token" => StatusCode::UNAUTHORIZED,
            "invalid_token" => StatusCode::UNAUTHORIZED,
            "expired_token" => StatusCode::UNAUTHORIZED,
            "revoked_token" => StatusCode::UNAUTHORIZED,
            "insufficient_scope" => StatusCode::FORBIDDEN,
            _ => StatusCode::UNAUTHORIZED,
        };

        tracing::warn!(error_code = %self.error, message = %self.message, status = status.as_u16(), "auth error");

        let locale = self.locale.clone();
        let mut response = (status, Json(json!(self))).into_response();
        set_content_language(response.headers_mut(), &locale);
        response
    }
}

// Re-export from token_blacklist module
pub use super::token_blacklist::create_token_store;

/// 从请求头提取 Token
fn extract_token_from_header(request: &Request, locale: &str) -> Result<String, AuthError> {
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .ok_or_else(|| {
            AuthError::new(
                "missing_token",
                locale,
                "errors.auth.missing_token",
                I18nParams::new(),
            )
        })?
        .to_str()
        .map_err(|_| {
            AuthError::new(
                "invalid_token",
                locale,
                "errors.auth.invalid_authorization_header",
                I18nParams::new(),
            )
        })?
        .to_string();

    // 支持 "Bearer <token>" 格式
    Ok(auth_header
        .strip_prefix("Bearer ")
        .map(|s| s.to_string())
        .unwrap_or(auth_header))
}

/// Token 默认黑名单 TTL（秒）
/// 设置为 Token 最大有效期 + 缓冲时间，确保过期 Token 不会永远留在黑名单
#[allow(dead_code)]
const TOKEN_BLACKLIST_TTL_SECONDS: u64 = 900; // 15 分钟

/// 验证 Token（使用 pasetors）
///
/// 注意：Access Token 在有效期内可重复使用，不启用单次使用限制。
/// 如需单次使用 Token，请使用专门的 Action Token 机制。
async fn validate_token(
    token: &str,
    token_store: &TokenStore,
    secret_key: &[u8],
    auth_service: &Arc<dyn AuthService>,
    locale: &str,
) -> Result<ValidatedToken, AuthError> {
    let validation_result = match validate_paseto_token(token, secret_key, locale) {
        Ok(token) => token,
        Err(_) => validate_session_token(token, auth_service, locale).await?,
    };

    // 检查是否过期
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    if now > validation_result.expires_at {
        return Err(AuthError::new(
            "expired_token",
            locale,
            "errors.auth.expired_token",
            I18nParams::new(),
        ));
    }

    if token_store
        .is_blacklisted(&validation_result.token_id)
        .await
        .map_err(|e| {
            let mut params = I18nParams::new();
            params.insert("reason".to_string(), Value::String(e.to_string()));
            AuthError::new("invalid_token", locale, "errors.auth.invalid_token", params)
        })?
    {
        return Err(AuthError::new(
            "revoked_token",
            locale,
            "errors.auth.invalid_token",
            I18nParams::new(),
        ));
    }

    if let Ok(Some(metadata)) = auth_service
        .get_api_token_metadata(&validation_result.token_id)
        .await
    {
        if metadata.revoked_at.is_some() {
            return Err(AuthError::new(
                "revoked_token",
                locale,
                "errors.auth.invalid_token",
                I18nParams::new(),
            ));
        }

        if metadata.expires_at.timestamp() as u64 <= now {
            return Err(AuthError::new(
                "expired_token",
                locale,
                "errors.auth.expired_token",
                I18nParams::new(),
            ));
        }

        if metadata.tenant_id.to_string() != validation_result.tenant_id {
            return Err(AuthError::new(
                "invalid_token",
                locale,
                "errors.auth.invalid_token",
                I18nParams::new(),
            ));
        }

        let _ = auth_service
            .mark_api_token_used(&validation_result.token_id, chrono::Utc::now())
            .await;
    }

    Ok(validation_result)
}

async fn validate_session_token(
    token: &str,
    auth_service: &Arc<dyn AuthService>,
    locale: &str,
) -> Result<ValidatedToken, AuthError> {
    let session = auth_service.verify_session(token).await.map_err(|err| {
        let mut params = I18nParams::new();
        params.insert("reason".to_string(), Value::String(err.to_string()));
        AuthError::new("invalid_token", locale, "errors.auth.invalid_token", params)
    })?;

    let memberships = auth_service
        .get_user_memberships(session.user_id)
        .await
        .map_err(|err| {
            let mut params = I18nParams::new();
            params.insert("reason".to_string(), Value::String(err.to_string()));
            AuthError::new("invalid_token", locale, "errors.auth.invalid_token", params)
        })?;

    let fallback_membership = memberships.first().cloned();
    let membership = memberships
        .into_iter()
        .find(|membership| Some(membership.id) == session.active_membership_id)
        .or(fallback_membership);

    let mut metadata = HashMap::new();
    metadata.insert("session_id".to_string(), session.id.to_string());

    if let Some(membership) = membership {
        metadata.insert("membership_id".to_string(), membership.id.to_string());

        return Ok(ValidatedToken {
            token_id: session.id.to_string(),
            subject: format!("{}:{}", membership.tenant_id, session.user_id),
            tenant_id: membership.tenant_id.to_string(),
            user_id: session.user_id.to_string(),
            expires_at: session.expires_at.timestamp() as u64,
            scopes: membership
                .scopes
                .iter()
                .filter_map(|scope| TokenScope::parse(scope))
                .collect(),
            issued_at: session.created_at.timestamp() as u64,
            membership_id: Some(membership.id.to_string()),
            metadata,
            subject_type: TOKEN_SUBJECT_TYPE_USER.to_string(),
            issued_from: TOKEN_ISSUED_FROM_SESSION.to_string(),
        });
    }

    Ok(ValidatedToken {
        token_id: session.id.to_string(),
        subject: format!("{UNASSIGNED_TENANT_ID}:{}", session.user_id),
        tenant_id: UNASSIGNED_TENANT_ID.to_string(),
        user_id: session.user_id.to_string(),
        expires_at: session.expires_at.timestamp() as u64,
        scopes: Vec::new(),
        issued_at: session.created_at.timestamp() as u64,
        membership_id: None,
        metadata,
        subject_type: TOKEN_SUBJECT_TYPE_USER.to_string(),
        issued_from: TOKEN_ISSUED_FROM_SESSION.to_string(),
    })
}

/// 使用 pasetors 验证 Token
pub(crate) fn validate_paseto_token(
    token: &str,
    secret_key: &[u8],
    locale: &str,
) -> Result<ValidatedToken, String> {
    use pasetors::claims::ClaimsValidationRules;
    use pasetors::keys::SymmetricKey;
    use pasetors::local;
    use pasetors::token::UntrustedToken;
    use time::OffsetDateTime;

    // 创建对称密钥
    let sk: SymmetricKey<_> =
        SymmetricKey::from(secret_key).map_err(|_| "无效的密钥长度".to_string())?;

    // 解析未受信任的 Token
    let untrusted =
        UntrustedToken::try_from(token).map_err(|e| format!("Token 解析失败: {e:?}"))?;

    // 验证 Token（禁用自动 exp 验证，我们自己检查过期时间以提供更清晰的错误消息）
    let mut validation_rules = ClaimsValidationRules::new();
    validation_rules.allow_non_expiring();
    let trusted_token = local::decrypt(&sk, &untrusted, &validation_rules, None, None)
        .map_err(|e| format!("解密失败: {e:?}"))?;

    // 获取 Claims
    let claims = trusted_token
        .payload_claims()
        .ok_or("Token 不包含 payload claims")?;

    // 提取声明
    let token_id = claims
        .get_claim("jti")
        .and_then(|v| v.as_str())
        .ok_or("Token 缺少 jti 声明")?
        .to_string();

    let subject = claims
        .get_claim("sub")
        .and_then(|v| v.as_str())
        .ok_or("Token 缺少 sub 声明")?
        .to_string();

    // 解析 exp（ISO 8601 格式）
    let expires_at = match claims.get_claim("exp").and_then(|v| v.as_str()) {
        Some(s) => OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
            .map(|dt| dt.unix_timestamp() as u64)
            .map_err(|_| "无法解析 exp 时间".to_string()),
        None => Err("Token 缺少 exp 声明".to_string()),
    }?;

    // 解析 iat（ISO 8601 格式）
    let issued_at = match claims.get_claim("iat").and_then(|v| v.as_str()) {
        Some(s) => OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
            .map(|dt| dt.unix_timestamp() as u64)
            .unwrap_or_else(|_| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            }),
        None => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    let scope_str = claims
        .get_claim("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let scopes: Vec<TokenScope> = scope_str
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();

    if scopes.is_empty() {
        return Err("Token 缺少 scope 声明".to_string());
    }

    let subject_type = claims
        .get_claim("subject_type")
        .and_then(|v| v.as_str())
        .unwrap_or(TOKEN_SUBJECT_TYPE_USER)
        .to_string();
    let issued_from = claims
        .get_claim("issued_from")
        .and_then(|v| v.as_str())
        .unwrap_or(TOKEN_ISSUED_FROM_SESSION)
        .to_string();

    // 解析租户 ID 和用户 ID
    let (tenant_id, user_id) = parse_subject(&subject, locale)
        .map_err(|e| format!("parse subject failed: {}", e.message))?;

    // 提取可选的 membership_id 和 session_id
    let membership_id = claims
        .get_claim("membership_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let session_id = claims
        .get_claim("session_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // 构建元数据
    let mut metadata = HashMap::new();
    if let Some(ref sid) = session_id {
        metadata.insert("session_id".to_string(), sid.clone());
    }
    if let Some(ref mid) = membership_id {
        metadata.insert("membership_id".to_string(), mid.clone());
    }

    Ok(ValidatedToken {
        token_id,
        subject,
        tenant_id,
        user_id,
        expires_at,
        scopes,
        issued_at,
        membership_id,
        metadata,
        subject_type,
        issued_from,
    })
}

/// 解析 subject 格式 "tenant_id:user_id"
fn parse_subject(subject: &str, locale: &str) -> Result<(String, String), AuthError> {
    let parts: Vec<&str> = subject.split(':').collect();
    if parts.len() != 2 {
        return Err(AuthError::new(
            "invalid_token",
            locale,
            "errors.auth.invalid_subject",
            I18nParams::new(),
        ));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Token 验证中间件
pub async fn auth_middleware(
    State((token_store, secret_key, auth_service)): State<(
        TokenStore,
        Vec<u8>,
        Arc<dyn AuthService>,
    )>,
    mut request: Request,
    next: Next,
) -> Response {
    let request_locale = resolve_accept_language(request.headers()).to_string();
    // 提取 Token
    let token = match extract_token_from_header(&request, &request_locale) {
        Ok(t) => t,
        Err(e) => return e.into_response(),
    };

    // 验证 Token
    let validated_token = match validate_token(
        &token,
        &token_store,
        &secret_key,
        &auth_service,
        &request_locale,
    )
    .await
    {
        Ok(t) => t,
        Err(e) => return e.into_response(),
    };

    // 将验证后的 Token 添加到请求扩展
    let context = RequestContext::from_validated_token(&validated_token)
        .with_resolved_locale(DEFAULT_LOCALE.to_string());
    request.extensions_mut().insert(validated_token);
    request.extensions_mut().insert(context);

    // 继续处理请求
    next.run(request).await
}

/// 从请求扩展获取 Token（处理器中使用）
pub fn get_token_from_request(request: &Request) -> Option<&ValidatedToken> {
    request.extensions().get::<ValidatedToken>()
}

/// Scope 验证中间件（检查特定 scope）
pub fn require_scope(
    required_scope: TokenScope,
) -> impl Fn(&ValidatedToken) -> Result<(), AuthError> + Clone {
    move |token: &ValidatedToken| {
        if token.has_scope(&required_scope) {
            Ok(())
        } else {
            let mut params = I18nParams::new();
            params.insert(
                "required_scope".to_string(),
                Value::String(required_scope.as_str().to_string()),
            );
            params.insert(
                "current_scopes".to_string(),
                Value::String(
                    token
                        .scopes
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
            );
            Err(AuthError::new(
                "insufficient_scope",
                DEFAULT_LOCALE,
                "errors.auth.insufficient_scope",
                params,
            ))
        }
    }
}

/// Scope 验证中间件（检查多个 scope 中的任意一个）
pub fn require_any_scope(
    required_scopes: Vec<TokenScope>,
) -> impl Fn(&ValidatedToken) -> Result<(), AuthError> + Clone {
    move |token: &ValidatedToken| {
        if token.has_any_scope(&required_scopes) {
            Ok(())
        } else {
            let mut params = I18nParams::new();
            params.insert(
                "required_scope".to_string(),
                Value::String(
                    required_scopes
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
            );
            params.insert(
                "current_scopes".to_string(),
                Value::String(
                    token
                        .scopes
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
            );
            Err(AuthError::new(
                "insufficient_scope",
                DEFAULT_LOCALE,
                "errors.auth.insufficient_scope",
                params,
            ))
        }
    }
}

/// 测试辅助模块
#[cfg(test)]
pub mod tests {
    use super::*;
    use async_trait::async_trait;
    use uuid::Uuid;

    use crate::auth::{
        AuthError as ServiceAuthError, AuthService, AuthSession, CreateUserRequest,
        ExternalIdentity, MembershipRole, MfaStatus, TenantInvitation, TenantMembership, User,
    };

    /// 获取测试密钥
    pub fn get_test_key() -> Vec<u8> {
        vec![0u8; 32]
    }

    /// 创建模拟的 ValidatedToken（用于测试）
    pub fn create_mock_token(
        tenant_id: &str,
        user_id: &str,
        scopes: Vec<TokenScope>,
    ) -> ValidatedToken {
        ValidatedToken {
            token_id: uuid::Uuid::now_v7().to_string(),
            subject: format!("{tenant_id}:{user_id}"),
            tenant_id: tenant_id.to_string(),
            user_id: user_id.to_string(),
            expires_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 3600,
            scopes,
            issued_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            membership_id: None,
            metadata: HashMap::new(),
            subject_type: TOKEN_SUBJECT_TYPE_USER.to_string(),
            issued_from: TOKEN_ISSUED_FROM_SESSION.to_string(),
        }
    }

    struct NoMembershipAuthService {
        session: AuthSession,
    }

    #[async_trait]
    impl AuthService for NoMembershipAuthService {
        async fn create_user_from_privy(
            &self,
            _privy_token: &str,
        ) -> Result<User, ServiceAuthError> {
            unreachable!()
        }

        async fn get_or_create_external_identity(
            &self,
            _user_id: Uuid,
            _provider: crate::auth::IdentityProvider,
            _provider_subject: &str,
            _profile: Option<serde_json::Value>,
        ) -> Result<ExternalIdentity, ServiceAuthError> {
            unreachable!()
        }

        async fn create_tenant_invitation(
            &self,
            _tenant_id: Uuid,
            _role: MembershipRole,
            _invitee_type: crate::auth::InviteeType,
            _invitee_email: Option<String>,
            _invitee_wallet: Option<String>,
            _created_by: Uuid,
            _expires_hours: i64,
        ) -> Result<(TenantInvitation, String), ServiceAuthError> {
            unreachable!()
        }

        async fn consume_invitation(
            &self,
            _invitation_token: &str,
            _user_id: Uuid,
        ) -> Result<TenantMembership, ServiceAuthError> {
            unreachable!()
        }

        async fn create_session(
            &self,
            _user_id: Uuid,
            _identity_id: Option<Uuid>,
            _request: CreateUserRequest,
        ) -> Result<(AuthSession, String), ServiceAuthError> {
            unreachable!()
        }

        async fn get_active_membership(
            &self,
            _user_id: Uuid,
            _tenant_id: Uuid,
        ) -> Result<Option<TenantMembership>, ServiceAuthError> {
            Ok(None)
        }

        async fn audit_log(
            &self,
            _event_type: crate::auth::AuthEventType,
            _user_id: Option<Uuid>,
            _data: Option<serde_json::Value>,
        ) -> Result<(), ServiceAuthError> {
            Ok(())
        }

        async fn verify_session(
            &self,
            _session_token: &str,
        ) -> Result<AuthSession, ServiceAuthError> {
            Ok(self.session.clone())
        }

        async fn revoke_session(
            &self,
            _session_id: Uuid,
            _reason: &str,
        ) -> Result<(), ServiceAuthError> {
            Ok(())
        }

        async fn get_user(&self, _user_id: Uuid) -> Result<User, ServiceAuthError> {
            Ok(User::new())
        }

        async fn get_user_identities(
            &self,
            _user_id: Uuid,
        ) -> Result<Vec<ExternalIdentity>, ServiceAuthError> {
            Ok(vec![])
        }

        async fn get_user_memberships(
            &self,
            _user_id: Uuid,
        ) -> Result<Vec<TenantMembership>, ServiceAuthError> {
            Ok(vec![])
        }

        async fn sync_mfa_status(
            &self,
            _user_id: Uuid,
            _privy_token: &str,
        ) -> Result<crate::auth::service::MfaStatusSnapshot, ServiceAuthError> {
            Ok(crate::auth::service::MfaStatusSnapshot::default())
        }

        async fn get_mfa_status(
            &self,
            _user_id: Uuid,
        ) -> Result<crate::auth::service::MfaStatusSnapshot, ServiceAuthError> {
            Ok(crate::auth::service::MfaStatusSnapshot::default())
        }
    }

    #[tokio::test]
    async fn test_validate_session_token_without_membership_uses_unassigned_tenant() {
        let user_id = Uuid::now_v7();
        let session = AuthSession {
            id: Uuid::now_v7(),
            user_id,
            session_token_hash: "ignored".to_string(),
            identity_id: None,
            active_membership_id: None,
            mfa_status: MfaStatus::NotRequired,
            mfa_verified_at: None,
            user_agent: None,
            ip_address: None,
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            last_active_at: chrono::Utc::now(),
            revoked_at: None,
            revoked_reason: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let auth_service: Arc<dyn AuthService> = Arc::new(NoMembershipAuthService { session });

        let validated = validate_session_token("session-token", &auth_service, "en")
            .await
            .expect("session without membership should still validate");

        assert_eq!(validated.user_id, user_id.to_string());
        assert_eq!(validated.tenant_id, UNASSIGNED_TENANT_ID);
        assert!(validated.membership_id.is_none());
        assert!(validated.scopes.is_empty());
        assert_eq!(
            validated.metadata.get("session_id").map(String::as_str),
            Some(validated.token_id.as_str())
        );
    }
}
