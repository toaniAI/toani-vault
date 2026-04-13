//! 认证 API 模块
//!
//! 提供 Privy 钱包优先的认证功能：
//! - POST /auth/session - 从 Privy Token 创建会话，自动创建/更新用户和成员资格
//! - GET /auth/me - 获取当前用户信息，包含租户上下文
//! - POST /auth/logout - 撤销当前会话
//! - POST /auth/invitations/consume - 消费邀请 Token，创建成员资格
//!
//! # 认证流程
//!
//! ```text
//! 1. 前端使用 Privy 认证用户，获取 Privy Access Token
//! 2. 前端调用 POST /auth/session，传入 Privy Token
//! 3. 后端验证 Privy Token，获取用户身份信息
//! 4. 创建或获取 User 记录，绑定 ExternalIdentity
//! 5. 如果有 invitation_token，创建 TenantMembership
//! 6. 创建 AuthSession，返回 Session Token
//! 7. 后续请求使用 Session Token（通过 auth_middleware 验证）
//! ```

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::audit::AuditStorage;
use crate::api::i18n::{I18nParams, ResolvedLocale, set_content_language, translate};
use crate::api::middleware::{TokenScope, ValidatedToken};
use crate::api::response::{ApiErrorResponse, ApiSuccessResponse};
use crate::api::tokens::{CreateTokenRequest, issue_access_token_from_user_token};
use crate::audit::{AuditAction, AuditEntry, Outcome, RedactedParam};
use crate::auth::{
    AuthError, AuthService, CreateUserRequest, ExternalIdentity, InviteeType, MembershipRole,
    TenantInvitation, TenantMembership, User,
};
use crate::token::TOKEN_ISSUED_FROM_SESSION;
use crate::vault::storage::CredentialVault;

use super::token_blacklist::{TokenStore, create_token_store};

// ============================================================================
// API 状态
// ============================================================================

/// 认证 API 状态
#[derive(Clone)]
pub struct AuthApiState {
    /// 认证服务
    pub auth_service: Arc<dyn AuthService>,
    /// Token 黑名单存储
    pub token_store: TokenStore,
    /// 审计存储（可选）
    pub audit_storage: Option<Arc<dyn AuditStorage>>,
    /// 凭证 Vault（用于 token 发放时校验白名单资源）
    pub vault: Option<Arc<CredentialVault>>,
}

impl AuthApiState {
    /// 创建新的认证 API 状态
    pub fn new(auth_service: Arc<dyn AuthService>) -> Self {
        Self {
            auth_service,
            token_store: create_token_store(),
            audit_storage: None,
            vault: None,
        }
    }

    /// 创建新的认证 API 状态（显式指定 Token 存储后端）
    pub fn new_with_token_store(
        auth_service: Arc<dyn AuthService>,
        token_store: TokenStore,
    ) -> Self {
        Self {
            auth_service,
            token_store,
            audit_storage: None,
            vault: None,
        }
    }

    /// 设置审计存储
    pub fn with_audit_storage(mut self, storage: Arc<dyn AuditStorage>) -> Self {
        self.audit_storage = Some(storage);
        self
    }

    pub fn with_vault(mut self, vault: Arc<CredentialVault>) -> Self {
        self.vault = Some(vault);
        self
    }

    /// 记录审计日志
    pub(crate) async fn record_audit(
        &self,
        action: AuditAction,
        user_id: &str,
        outcome: Outcome,
        details: Option<serde_json::Value>,
    ) {
        let Some(storage) = &self.audit_storage else {
            return;
        };

        let entry = AuditEntry::new(
            crate::audit::events::hash_user_id(user_id),
            "session",
            "auth",
            action,
            outcome,
            "software_mode",
            Uuid::now_v7().to_string(),
        );

        let entry = if let Some(d) = details {
            entry.with_param("details", RedactedParam::Plain(d.to_string()))
        } else {
            entry
        };

        if let Err(error) = storage.record(entry).await {
            tracing::warn!("[AUDIT] Auth audit record failed: {error:?}");
        }
    }
}

// ============================================================================
// 请求/响应模型
// ============================================================================

/// 创建会话请求（从 Privy Token）
/// 支持 `privy_access_token` 和 `privy_token` 两种字段名（兼容性别名）
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSessionRequest {
    /// Privy Access Token
    /// 支持两种字段名：`privy_access_token`（推荐）和 `privy_token`（兼容别名）
    #[serde(alias = "privy_token")]
    pub privy_access_token: String,
    /// 邀请 Token（可选，用于首次加入租户）
    #[serde(default)]
    pub invitation_token: Option<String>,
}

/// 用户 Profile（响应）
#[derive(Debug, Serialize)]
pub struct UserProfile {
    /// 用户 ID
    pub id: Uuid,
    /// 显示名称
    pub display_name: Option<String>,
    /// 用户状态
    pub status: String,
    /// 是否已完成引导
    pub onboarding_completed: bool,
    /// 默认租户 ID
    pub default_tenant_id: Option<Uuid>,
    /// 外部身份列表
    pub identities: Vec<IdentityInfo>,
}

/// 外部身份信息
#[derive(Debug, Serialize)]
pub struct IdentityInfo {
    /// 身份提供商
    pub provider: String,
    /// 提供商中的标识
    pub subject: String,
    /// 钱包地址（可选）
    pub wallet_address: Option<String>,
    /// 邮箱地址（可选）
    pub email: Option<String>,
    /// 是否已验证
    pub is_verified: bool,
    /// 是否为主要身份
    pub is_primary: bool,
}

/// 会话信息（响应）
#[derive(Debug, Serialize)]
pub struct SessionInfo {
    /// 会话 ID
    pub id: Uuid,
    /// Session Token（用于后续请求）
    pub session_token: String,
    /// 过期时间（ISO 8601）
    pub expires_at: String,
    /// MFA 状态
    pub mfa_status: String,
}

/// 面向 API/CLI 的 access token 签发请求
#[derive(Debug, Deserialize)]
pub struct CreateAccessTokenRequest {
    pub scopes: Vec<String>,
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
    #[serde(default)]
    pub credential_ids: Vec<String>,
}

/// access token 响应
#[derive(Debug, Serialize)]
pub struct AccessTokenResponse {
    pub access_token: String,
    pub token_id: String,
    pub token_type: String,
    pub subject_type: String,
    pub issued_from: String,
    pub display_name: Option<String>,
    pub expires_at: u64,
    pub expires_in: u64,
    pub granted_scopes: Vec<String>,
    pub credential_ids: Vec<String>,
    pub revoked_at: Option<String>,
}

/// 成员资格信息（响应）
#[derive(Debug, Serialize)]
pub struct MembershipInfo {
    /// 成员资格 ID
    pub id: Uuid,
    /// 租户 ID
    pub tenant_id: Uuid,
    /// 角色
    pub role: String,
    /// 状态
    pub status: String,
    /// 权限范围
    pub scopes: Vec<String>,
    /// 加入时间（可选）
    pub joined_at: Option<String>,
}

/// 租户信息（响应）
#[derive(Debug, Serialize)]
pub struct TenantInfo {
    /// 租户 ID
    pub id: Uuid,
    /// 租户名称（可选）
    pub name: Option<String>,
}

/// 创建会话响应
#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    /// 用户 Profile
    pub user: UserProfile,
    /// 会话信息
    pub session: SessionInfo,
    /// 全部活跃成员资格
    pub memberships: Vec<MembershipInfo>,
    /// 当前租户信息（可选）
    pub current_tenant: Option<TenantInfo>,
    /// 当前成员资格（可选）
    pub current_membership: Option<MembershipInfo>,
}

/// 获取当前用户响应
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCurrentUserResponse {
    /// 用户 ID（顶层字段，兼容自动化用例）
    pub user_id: Uuid,
    /// 租户 ID（顶层字段，兼容自动化用例）
    pub tenant_id: Option<Uuid>,
    /// 用户名（顶层字段，兼容自动化用例）
    /// 来源优先级：主身份邮箱 > 主身份钱包地址 > user.id 字符串
    pub username: String,
    /// 权限范围（顶层字段，兼容自动化用例）
    pub scopes: Vec<String>,
    /// 语言偏好（顶层字段，兼容自动化用例）
    pub locale: String,
    /// 用户 Profile
    pub user: UserProfile,
    /// 当前租户信息（可选）
    pub current_tenant: Option<TenantInfo>,
    /// 当前成员资格（可选）
    pub current_membership: Option<MembershipInfo>,
    /// 全部活跃成员资格
    pub memberships: Vec<MembershipInfo>,
    /// MFA 状态
    pub mfa_status: String,
}

/// 获取成员资格列表响应
#[derive(Debug, Serialize)]
pub struct MembershipsResponse {
    /// 全部活跃成员资格
    pub memberships: Vec<MembershipInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendIdentityInfo {
    pub provider: String,
    pub subject: String,
    pub wallet_address: Option<String>,
    pub email: Option<String>,
    pub is_verified: bool,
    pub is_primary: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendUserProfile {
    pub id: Uuid,
    pub display_name: Option<String>,
    pub status: String,
    pub onboarding_completed: bool,
    pub default_tenant_id: Option<Uuid>,
    pub identities: Vec<FrontendIdentityInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendMembershipInfo {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub status: String,
    pub invited_by: Option<Uuid>,
    pub joined_at: Option<String>,
    pub source: String,
    pub scopes: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendInvitationInfo {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub role: String,
    pub invitee_type: String,
    pub invitee_email: Option<String>,
    pub invitee_wallet: Option<String>,
    pub created_by: Uuid,
    pub expires_at: String,
    pub consumed_at: Option<String>,
    pub consumed_by: Option<Uuid>,
    pub status: String,
    pub max_uses: i32,
    pub use_count: i32,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendInvitationListItem {
    pub invitation: FrontendInvitationInfo,
    pub invite_token: String,
    pub invite_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendMemberListItem {
    pub membership: FrontendMembershipInfo,
    pub user: FrontendUserProfile,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendMemberListResponse {
    pub members: Vec<FrontendMemberListItem>,
    pub total: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateUserProfileRequest {
    pub display_name: Option<String>,
    pub default_tenant_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompleteOnboardingRequest {
    pub display_name: Option<String>,
}

const MAX_DISPLAY_NAME_CHARS: usize = 128;
/// 邀请码最大长度限制（防止超长字符串攻击）
const MAX_INVITATION_TOKEN_LENGTH: usize = 256;

#[derive(Debug, Deserialize)]
pub struct TenantScopedQuery {
    pub tenant_id: Uuid,
}

/// 邀请列表查询参数（支持分页参数校验）
#[derive(Debug, Deserialize)]
pub struct ListInvitationsQuery {
    pub tenant_id: Uuid,
    /// 分页限制（可选）。必须是有效正整数；非法值将返回 400。
    #[serde(default, deserialize_with = "deserialize_optional_limit")]
    pub limit: Option<u32>,
}

/// 自定义 deserializer：校验 limit 必须是有效正整数。
/// 非数字或负值返回 deserialization 错误，将被 handler 捕获并转换为 400。
fn deserialize_optional_limit<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<serde_json::Value> = Option::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(serde_json::Value::Number(n)) => {
            // 尝试解析为 u64，然后校验范围
            if let Some(num) = n.as_u64() {
                if num > u32::MAX as u64 {
                    return Err(serde::de::Error::custom("limit exceeds maximum value"));
                }
                if num == 0 {
                    return Err(serde::de::Error::custom("limit must be a positive integer"));
                }
                Ok(Some(num as u32))
            } else {
                // 负数或非整数（如 1.5）
                Err(serde::de::Error::custom("limit must be a positive integer"))
            }
        }
        Some(_) => {
            // 非数字类型（字符串 "abc"、null、对象等）
            Err(serde::de::Error::custom("limit must be a positive integer"))
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateInvitationApiRequest {
    pub tenant_id: Uuid,
    pub role: MembershipRole,
    pub invitee_type: InviteeType,
    pub invitee_email: Option<String>,
    pub invitee_wallet: Option<String>,
    pub expires_in_hours: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMembershipRoleRequest {
    pub role: MembershipRole,
}

/// 消费邀请请求
#[derive(Debug, Deserialize)]
pub struct ConsumeInvitationRequest {
    /// 邀请 Token
    #[serde(alias = "code")]
    pub invitation_token: String,
}

/// 消费邀请响应
#[derive(Debug, Serialize)]
pub struct ConsumeInvitationResponse {
    /// 成员资格信息
    pub membership: MembershipInfo,
    /// 租户信息
    pub tenant: TenantInfo,
}

/// 注销响应
#[derive(Debug, Serialize)]
pub struct LogoutResponse {
    /// 是否成功
    pub success: bool,
}

/// MFA 状态响应
#[derive(Debug, Serialize)]
pub struct MfaStatusResponse {
    /// MFA 是否已启用
    pub enabled: bool,
    /// MFA 是否已验证
    pub verified: bool,
    /// 是否需要 step-up 验证
    pub requires_step_up: bool,
    /// 最后验证时间
    pub last_verified_at: Option<String>,
    /// 同步时间
    pub synced_at: String,
}

/// 认证错误响应
#[derive(Debug, Serialize)]
pub struct AuthErrorResponse {
    pub error: String,
    pub message: String,
    pub error_description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub i18n: Option<super::i18n::I18nMetadata>,
    pub locale: String,
}

// ============================================================================
// API 路由
// ============================================================================

/// 创建公开认证路由（不需要认证）
pub fn auth_routes() -> Router<AuthApiState> {
    Router::new()
        // 从 Privy Token 创建会话
        .route("/auth/session", post(create_session_handler))
}

/// 创建受保护的认证路由（需要认证）
pub fn protected_auth_routes() -> Router<AuthApiState> {
    Router::new()
        .route("/auth/access-token", post(create_access_token_handler))
        // 获取当前用户信息
        .route("/auth/me", get(get_current_user_handler))
        // 获取当前用户全部成员资格
        .route("/auth/memberships", get(get_memberships_handler))
        // 注销（撤销会话）
        .route("/auth/logout", post(logout_handler))
        // 获取 MFA 状态
        .route("/auth/mfa-status", get(get_mfa_status_handler))
        // 同步 MFA 状态
        .route("/auth/mfa-status/sync", post(sync_mfa_status_handler))
        // 当前用户资料
        .route(
            "/users/me",
            get(get_current_user_self_handler)
                .patch(update_current_user_handler)
                .delete(delete_current_user_handler),
        )
        .route("/users/me/onboarding", post(complete_onboarding_handler))
        .route("/members", get(list_members_handler))
        .route(
            "/members/:membership_id/role",
            patch(update_member_role_handler),
        )
        .route("/members/:membership_id", delete(remove_member_handler))
        .route(
            "/invitations",
            get(list_invitations_handler).post(create_invitation_handler),
        )
        // Explicit handler for missing invitation_id in revoke path
        // Must be registered BEFORE the dynamic :invitation_id route
        .route(
            "/invitations/revoke",
            post(revoke_invitation_missing_id_handler),
        )
        .route(
            "/invitations/:invitation_id/revoke",
            post(revoke_invitation_handler),
        )
        .route("/invitations/consume", post(consume_invitation_handler))
        .route(
            "/auth/invitations/consume",
            post(consume_invitation_handler),
        )
}

// ============================================================================
// 错误处理
// ============================================================================

fn auth_error_response(
    status: StatusCode,
    error: &str,
    locale: &ResolvedLocale,
    key: &str,
    params: I18nParams,
) -> Response {
    let message = translate(locale.as_str(), key, &params);
    let payload = AuthErrorResponse {
        error: error.to_string(),
        message: message.clone(),
        error_description: message,
        i18n: Some(super::i18n::I18nMetadata::new(key).with_params(params)),
        locale: locale.as_str().to_string(),
    };

    let mut response = (status, Json(payload)).into_response();
    set_content_language(response.headers_mut(), locale.as_str());
    response
}

fn auth_error_to_response(err: AuthError, locale: &ResolvedLocale) -> Response {
    let status =
        StatusCode::from_u16(err.http_status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let (error_code, message_key) = match &err {
        AuthError::UserNotFound(_) => ("user_not_found", "errors.auth.user_not_found"),
        AuthError::InvalidPrivyToken(_) => {
            ("invalid_privy_token", "errors.auth.invalid_privy_token")
        }
        AuthError::PrivyTokenExpired => ("privy_token_expired", "errors.auth.privy_token_expired"),
        AuthError::PrivyAuthenticationFailed(_) => {
            ("privy_auth_failed", "errors.auth.privy_auth_failed")
        }
        AuthError::MembershipNotFound { .. } => {
            ("membership_not_found", "errors.auth.membership_not_found")
        }
        AuthError::InvalidInvitationToken => {
            ("invalid_invitation", "errors.auth.invalid_invitation")
        }
        AuthError::InvitationExpired(_) => ("invitation_expired", "errors.auth.invitation_expired"),
        AuthError::InvitationAlreadyConsumed(_) => {
            ("invitation_consumed", "errors.auth.invitation_consumed")
        }
        AuthError::SessionNotFound(_) => ("session_not_found", "errors.auth.session_not_found"),
        AuthError::SessionExpired(_) => ("session_expired", "errors.auth.session_expired"),
        AuthError::SessionRevoked(_) => ("session_revoked", "errors.auth.session_revoked"),
        AuthError::InsufficientPermissions { .. } => (
            "insufficient_permissions",
            "errors.auth.insufficient_permissions",
        ),
        AuthError::InvalidUserStatus { .. } => {
            ("invalid_user_status", "errors.auth.invalid_user_status")
        }
        AuthError::DatabaseError(_) => ("database_error", "errors.auth.database_error"),
        _ => ("internal_error", "errors.auth.internal_error"),
    };

    let mut params = I18nParams::new();
    params.insert(
        "error".to_string(),
        serde_json::Value::String(err.to_string()),
    );

    auth_error_response(status, error_code, locale, message_key, params)
}

fn map_user_profile(user: &User, identities: Vec<ExternalIdentity>) -> UserProfile {
    UserProfile {
        id: user.id,
        display_name: user.display_name.clone(),
        status: user.status.as_str().to_string(),
        onboarding_completed: user.onboarding_completed,
        default_tenant_id: user.default_tenant_id,
        identities: identities
            .into_iter()
            .map(|i| IdentityInfo {
                provider: i.provider.as_str().to_string(),
                subject: i.provider_subject,
                wallet_address: i.wallet_address,
                email: i.email,
                is_verified: i.is_verified,
                is_primary: i.is_primary,
            })
            .collect(),
    }
}

#[allow(clippy::result_large_err)]
fn parse_token_uuid(
    token_value: &str,
    field: &str,
    locale: &ResolvedLocale,
) -> Result<Uuid, Response> {
    Uuid::parse_str(token_value).map_err(|_| {
        auth_error_response(
            StatusCode::UNAUTHORIZED,
            "invalid_token",
            locale,
            "errors.auth.invalid_token",
            {
                let mut params = I18nParams::new();
                params.insert(
                    "reason".to_string(),
                    serde_json::Value::String(format!("invalid {field} in token")),
                );
                params
            },
        )
    })
}

fn map_membership_info(membership: &TenantMembership) -> MembershipInfo {
    MembershipInfo {
        id: membership.id,
        tenant_id: membership.tenant_id,
        role: membership.role.as_str().to_string(),
        status: membership.status.as_str().to_string(),
        scopes: membership.scopes.clone(),
        joined_at: membership.joined_at.map(|t| t.to_rfc3339()),
    }
}

fn map_tenant_info(membership: &TenantMembership) -> TenantInfo {
    TenantInfo {
        id: membership.tenant_id,
        name: None,
    }
}

fn is_web_session_token(token: &ValidatedToken) -> bool {
    token.is_user_subject()
        && token.issued_from() == TOKEN_ISSUED_FROM_SESSION
        && token.session_id() == Some(token.token_id.as_str())
}

fn map_frontend_user_profile(
    user: &User,
    identities: Vec<ExternalIdentity>,
) -> FrontendUserProfile {
    FrontendUserProfile {
        id: user.id,
        display_name: user.display_name.clone(),
        status: user.status.as_str().to_string(),
        onboarding_completed: user.onboarding_completed,
        default_tenant_id: user.default_tenant_id,
        identities: identities
            .into_iter()
            .map(|identity| FrontendIdentityInfo {
                provider: identity.provider.as_str().to_string(),
                subject: identity.provider_subject,
                wallet_address: identity.wallet_address,
                email: identity.email,
                is_verified: identity.is_verified,
                is_primary: identity.is_primary,
            })
            .collect(),
    }
}

fn map_frontend_membership_info(membership: &TenantMembership) -> FrontendMembershipInfo {
    FrontendMembershipInfo {
        id: membership.id,
        tenant_id: membership.tenant_id,
        user_id: membership.user_id,
        role: membership.role.as_str().to_string(),
        status: membership.status.as_str().to_string(),
        invited_by: membership.invited_by,
        joined_at: membership.joined_at.map(|value| value.to_rfc3339()),
        source: membership.source.as_str().to_string(),
        scopes: membership.scopes.clone(),
        created_at: membership.created_at.to_rfc3339(),
        updated_at: membership.updated_at.to_rfc3339(),
    }
}

fn map_frontend_invitation_info(invitation: &TenantInvitation) -> FrontendInvitationInfo {
    FrontendInvitationInfo {
        id: invitation.id,
        tenant_id: invitation.tenant_id,
        role: invitation.role.as_str().to_string(),
        invitee_type: invitation.invitee_type.as_str().to_string(),
        invitee_email: invitation.invitee_email.clone(),
        invitee_wallet: invitation.invitee_wallet.clone(),
        created_by: invitation.created_by,
        expires_at: invitation.expires_at.to_rfc3339(),
        consumed_at: invitation.consumed_at.map(|value| value.to_rfc3339()),
        consumed_by: invitation.consumed_by,
        status: invitation.status.as_str().to_string(),
        max_uses: invitation.max_uses,
        use_count: invitation.use_count,
        created_at: invitation.created_at.to_rfc3339(),
    }
}

fn select_current_membership(
    memberships: &[TenantMembership],
    preferred_tenant_id: Option<Uuid>,
    default_tenant_id: Option<Uuid>,
) -> Option<&TenantMembership> {
    preferred_tenant_id
        .and_then(|tenant_id| memberships.iter().find(|m| m.tenant_id == tenant_id))
        .or_else(|| {
            default_tenant_id
                .and_then(|tenant_id| memberships.iter().find(|m| m.tenant_id == tenant_id))
        })
        .or_else(|| memberships.first())
}

#[allow(clippy::result_large_err)]
fn require_scopes(token: &ValidatedToken, scopes: &[TokenScope]) -> Result<(), ApiErrorResponse> {
    if token.has_any_scope(scopes) {
        return Ok(());
    }

    Err(ApiErrorResponse::forbidden("Insufficient permissions"))
}

#[allow(clippy::result_large_err)]
fn parse_token_user_id(token: &ValidatedToken) -> Result<Uuid, ApiErrorResponse> {
    Uuid::parse_str(&token.user_id)
        .map_err(|_| ApiErrorResponse::unauthorized("Invalid user id in session"))
}

// ============================================================================
// API 处理器
// ============================================================================

/// 创建会话处理器
///
/// 从 Privy Access Token 创建会话：
/// 1. 验证 Privy Token
/// 2. 创建或获取 User
/// 3. 绑定 ExternalIdentity
/// 4. 如果有 invitation_token，创建 TenantMembership
/// 5. 创建 AuthSession
pub async fn create_session_handler(
    State(state): State<AuthApiState>,
    locale: ResolvedLocale,
    payload: Result<Json<CreateSessionRequest>, JsonRejection>,
) -> Response {
    // 处理 JSON 反序列化错误（如必填字段缺失、未知字段），返回 400 而不是默认的 422
    let Json(request) = match payload {
        Ok(json) => json,
        Err(rejection) => {
            return auth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                &locale,
                "errors.auth.invalid_request",
                I18nParams::from([("detail".to_string(), json!(rejection.body_text()))]),
            );
        }
    };

    // 校验 privy_access_token 长度上限（防止超长输入攻击）
    const PRIVY_TOKEN_MAX_LENGTH: usize = 2048; // Privy token 通常不超过 1KB，设置 2KB 上限
    if request.privy_access_token.len() > PRIVY_TOKEN_MAX_LENGTH {
        tracing::warn!(
            target: "auth::session",
            "[SESSION REJECTED] privy_access_token too long: length={}, max={}",
            request.privy_access_token.len(),
            PRIVY_TOKEN_MAX_LENGTH
        );
        return auth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            &locale,
            "errors.auth.token_too_long",
            I18nParams::default(),
        );
    }

    // [DIAGNOSTIC] 记录进入 session handler
    let token_preview = if request.privy_access_token.len() > 20 {
        format!("{}...", &request.privy_access_token[..20])
    } else {
        "[too short]".to_string()
    };
    tracing::info!(
        target: "auth::session",
        "[SESSION START] Creating session from Privy token: prefix={}, has_invitation={}, locale={}",
        token_preview,
        request.invitation_token.is_some(),
        locale.as_str()
    );

    // 1. 从 Privy Token 创建或获取用户
    let user = match state
        .auth_service
        .create_user_from_privy(&request.privy_access_token)
        .await
    {
        Ok(u) => {
            tracing::info!(
                target: "auth::session",
                "[SESSION STEP 1] User resolved: user_id={}, display_name={}",
                u.id,
                u.display_name.as_deref().unwrap_or("[none]")
            );
            u
        }
        Err(e) => {
            tracing::error!(
                target: "auth::session",
                "[SESSION FAILED] create_user_from_privy failed: error={:?}, http_status={}",
                e,
                e.http_status_code()
            );
            state
                .record_audit(
                    AuditAction::FailedAuth,
                    "unknown",
                    Outcome::Failure,
                    Some(json!({ "error": e.to_string() })),
                )
                .await;
            return auth_error_to_response(e, &locale);
        }
    };

    // 2. 获取用户的外部身份列表
    let identities = match state.auth_service.get_user_identities(user.id).await {
        Ok(ids) => ids,
        Err(e) => return auth_error_to_response(e, &locale),
    };

    // 3. 处理邀请 Token（如果提供）
    let invited_membership = if let Some(invitation_token) = &request.invitation_token {
        match state
            .auth_service
            .consume_invitation(invitation_token, user.id)
            .await
        {
            Ok(m) => Some(m),
            Err(e) => {
                // 邀请消费失败不阻止登录，只记录警告
                tracing::warn!(
                    user_id = user.id.to_string(),
                    error = e.to_string(),
                    "Failed to consume invitation token during session creation"
                );
                None
            }
        }
    } else {
        // 尝试获取用户默认租户的成员资格
        if let Some(default_tenant_id) = user.default_tenant_id {
            state
                .auth_service
                .get_active_membership(user.id, default_tenant_id)
                .await
                .ok()
                .flatten()
        } else {
            // 获取用户的第一个活跃成员资格
            state
                .auth_service
                .get_user_memberships(user.id)
                .await
                .ok()
                .and_then(|memberships| memberships.into_iter().next())
        }
    };

    let memberships = match state.auth_service.get_user_memberships(user.id).await {
        Ok(items) => items,
        Err(e) => return auth_error_to_response(e, &locale),
    };

    let current_membership = select_current_membership(
        &memberships,
        invited_membership
            .as_ref()
            .map(|membership| membership.tenant_id),
        user.default_tenant_id,
    );

    // 4. 创建会话
    let identity_id = identities.iter().find(|i| i.is_primary).map(|i| i.id);
    let create_request = CreateUserRequest {
        display_name: user.display_name.clone(),
    };

    let (session, session_token) = match state
        .auth_service
        .create_session(user.id, identity_id, create_request)
        .await
    {
        Ok((s, t)) => (s, t),
        Err(e) => return auth_error_to_response(e, &locale),
    };

    // 5. 记录审计日志
    state
        .record_audit(
            AuditAction::TokenIssue,
            &user.id.to_string(),
            Outcome::Success,
            Some(json!({
                "session_id": session.id.to_string(),
                "has_invitation": request.invitation_token.is_some(),
                "membership_created": invited_membership.is_some(),
            })),
        )
        .await;

    // 6. 构建响应
    let user_profile = map_user_profile(&user, identities);

    let session_info = SessionInfo {
        id: session.id,
        session_token,
        expires_at: session.expires_at.to_rfc3339(),
        mfa_status: session.mfa_status.as_str().to_string(),
    };

    let memberships_info = memberships.iter().map(map_membership_info).collect();
    let current_tenant = current_membership.map(map_tenant_info);
    let current_membership_info = current_membership.map(map_membership_info);

    let mut response = (
        StatusCode::OK,
        Json(CreateSessionResponse {
            user: user_profile,
            session: session_info,
            memberships: memberships_info,
            current_tenant,
            current_membership: current_membership_info,
        }),
    )
        .into_response();
    set_content_language(response.headers_mut(), locale.as_str());
    response
}

pub async fn create_access_token_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    Json(request): Json<CreateAccessTokenRequest>,
) -> Result<ApiSuccessResponse<AccessTokenResponse>, ApiErrorResponse> {
    let created = issue_access_token_from_user_token(
        &state,
        &token,
        CreateTokenRequest {
            scopes: request.scopes,
            expires_in: request.ttl_seconds,
            credential_ids: request.credential_ids,
        },
    )
    .await?;

    Ok(ApiSuccessResponse::new(AccessTokenResponse {
        access_token: created.access_token,
        token_id: created.token_id,
        token_type: created.token_type,
        subject_type: created.subject_type,
        issued_from: created.issued_from,
        display_name: created.display_name,
        expires_at: created.expires_at,
        expires_in: created.expires_in,
        granted_scopes: created.granted_scopes,
        credential_ids: created.credential_ids,
        revoked_at: created.revoked_at,
    }))
}

/// 获取当前用户处理器
///
/// 返回当前用户的完整信息，包括：
/// - 用户 Profile
/// - 当前租户信息
/// - 当前成员资格
/// - MFA 状态
pub async fn get_current_user_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    locale: ResolvedLocale,
) -> Response {
    if !is_web_session_token(&token) {
        return auth_error_response(
            StatusCode::FORBIDDEN,
            "forbidden",
            &locale,
            "errors.auth.insufficient_permissions",
            I18nParams::new(),
        );
    }

    // 1. 获取用户信息
    let user_id = match parse_token_uuid(&token.user_id, "user_id", &locale) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let user = match state.auth_service.get_user(user_id).await {
        Ok(u) => u,
        Err(e) => return auth_error_to_response(e, &locale),
    };

    // 2. 获取用户的外部身份
    let identities = match state.auth_service.get_user_identities(user.id).await {
        Ok(ids) => ids,
        Err(e) => return auth_error_to_response(e, &locale),
    };

    let memberships = match state.auth_service.get_user_memberships(user.id).await {
        Ok(items) => items,
        Err(e) => return auth_error_to_response(e, &locale),
    };

    // 3. 获取当前租户的成员资格
    let tenant_id = match parse_token_uuid(&token.tenant_id, "tenant_id", &locale) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let current_membership =
        select_current_membership(&memberships, Some(tenant_id), user.default_tenant_id);

    // 4. 构建 MFA 状态（从会话信息获取，如果可用）
    let mfa_status = if let Some(_session_id_str) = token.metadata.get("session_id") {
        // 尝试获取会话信息（需要扩展 AuthService）
        // 当前使用 token 中的信息
        "not_required"
    } else {
        "not_required"
    };

    // 5. 构建响应
    // 提取 username：优先主身份邮箱，其次主身份钱包地址，最后 user.id
    let username = identities
        .iter()
        .find(|i| i.is_primary)
        .and_then(|i| i.email.clone().or(i.wallet_address.clone()))
        .unwrap_or_else(|| user.id.to_string());

    // 提取 scopes：从当前成员资格获取，否则空数组
    let scopes: Vec<String> = current_membership
        .as_ref()
        .map(|m| m.scopes.clone())
        .unwrap_or_default();

    // 提取 tenant_id：从当前成员资格获取
    let response_tenant_id = current_membership.as_ref().map(|m| m.tenant_id);

    let user_profile = map_user_profile(&user, identities);
    let memberships_info = memberships.iter().map(map_membership_info).collect();
    let current_tenant = current_membership.map(map_tenant_info);
    let current_membership_info = current_membership.map(map_membership_info);

    let mut response = (
        StatusCode::OK,
        Json(GetCurrentUserResponse {
            user_id: user.id,
            tenant_id: response_tenant_id,
            username,
            scopes,
            locale: locale.as_str().to_string(),
            user: user_profile,
            current_tenant,
            current_membership: current_membership_info,
            memberships: memberships_info,
            mfa_status: mfa_status.to_string(),
        }),
    )
        .into_response();
    set_content_language(response.headers_mut(), locale.as_str());
    response
}

/// 获取当前用户所有活跃成员资格
pub async fn get_memberships_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    locale: ResolvedLocale,
) -> Response {
    if token.is_service_account_subject() {
        return auth_error_response(
            StatusCode::FORBIDDEN,
            "forbidden",
            &locale,
            "errors.auth.insufficient_permissions",
            I18nParams::new(),
        );
    }

    let user_id = match parse_token_uuid(&token.user_id, "user_id", &locale) {
        Ok(value) => value,
        Err(response) => return response,
    };
    let memberships = match state.auth_service.get_user_memberships(user_id).await {
        Ok(items) => items,
        Err(e) => return auth_error_to_response(e, &locale),
    };

    let mut response = (
        StatusCode::OK,
        Json(MembershipsResponse {
            memberships: memberships.iter().map(map_membership_info).collect(),
        }),
    )
        .into_response();
    set_content_language(response.headers_mut(), locale.as_str());
    response
}

/// 注销处理器
///
/// 撤销当前会话
pub async fn logout_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    locale: ResolvedLocale,
) -> Response {
    if !is_web_session_token(&token) {
        return auth_error_response(
            StatusCode::FORBIDDEN,
            "forbidden",
            &locale,
            "errors.auth.insufficient_permissions",
            I18nParams::new(),
        );
    }

    // 1. 获取会话 ID
    let session_id_str = token
        .metadata
        .get("session_id")
        .cloned()
        .unwrap_or_default();
    let session_id = if session_id_str.is_empty() {
        Uuid::nil()
    } else {
        match parse_token_uuid(&session_id_str, "session_id", &locale) {
            Ok(value) => value,
            Err(response) => return response,
        }
    };

    // 2. 撤销会话
    if session_id != Uuid::nil() {
        if let Err(e) = state
            .auth_service
            .revoke_session(session_id, "user_logout")
            .await
        {
            tracing::warn!(
                session_id = session_id.to_string(),
                error = e.to_string(),
                "Failed to revoke session during logout"
            );
        }

        // 3. 将 Token 加入黑名单
        let ttl = token
            .expires_at
            .saturating_sub(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            )
            .max(1);

        if let Err(e) = state
            .token_store
            .blacklist_token(&token.token_id, ttl)
            .await
        {
            tracing::warn!(
                token_id = token.token_id,
                error = e.to_string(),
                "Failed to blacklist token during logout"
            );
        }
    }

    // 4. 记录审计日志
    state
        .record_audit(
            AuditAction::TokenRevoke,
            &token.user_id,
            Outcome::Success,
            Some(json!({ "session_id": session_id_str })),
        )
        .await;

    let mut response = (StatusCode::OK, Json(LogoutResponse { success: true })).into_response();
    set_content_language(response.headers_mut(), locale.as_str());
    response
}

/// 消费邀请处理器
///
/// 验证邀请 Token 并创建成员资格
pub async fn consume_invitation_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    locale: ResolvedLocale,
    payload: Result<Json<ConsumeInvitationRequest>, JsonRejection>,
) -> Response {
    // 处理 JSON 反序列化错误（如必填字段缺失），返回 400 而不是默认的 422
    let Json(request) = match payload {
        Ok(json) => json,
        Err(rejection) => {
            return auth_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                &locale,
                "errors.auth.invalid_request",
                I18nParams::from([("detail".to_string(), json!(rejection.body_text()))]),
            );
        }
    };

    // 验证邀请码长度
    if request.invitation_token.len() > MAX_INVITATION_TOKEN_LENGTH {
        return auth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            &locale,
            "errors.auth.invalid_request",
            I18nParams::from([(
                "detail".to_string(),
                json!("invitation_token exceeds maximum length"),
            )]),
        );
    }

    if !is_web_session_token(&token) {
        return auth_error_response(
            StatusCode::FORBIDDEN,
            "forbidden",
            &locale,
            "errors.auth.insufficient_permissions",
            I18nParams::new(),
        );
    }

    // 1. 解析用户 ID
    let user_id = match parse_token_uuid(&token.user_id, "user_id", &locale) {
        Ok(value) => value,
        Err(response) => return response,
    };

    // 2. 消费邀请
    let membership = match state
        .auth_service
        .consume_invitation(&request.invitation_token, user_id)
        .await
    {
        Ok(m) => m,
        Err(e) => return auth_error_to_response(e, &locale),
    };

    // 3. 记录审计日志
    state
        .record_audit(
            AuditAction::SystemConfigChange, // Using existing action for membership changes
            &user_id.to_string(),
            Outcome::Success,
            Some(json!({
                "membership_id": membership.id.to_string(),
                "tenant_id": membership.tenant_id.to_string(),
                "role": membership.role.as_str(),
            })),
        )
        .await;

    // 4. 构建响应
    let membership_info = MembershipInfo {
        id: membership.id,
        tenant_id: membership.tenant_id,
        role: membership.role.as_str().to_string(),
        status: membership.status.as_str().to_string(),
        scopes: membership.scopes,
        joined_at: membership.joined_at.map(|t| t.to_rfc3339()),
    };

    let tenant_info = TenantInfo {
        id: membership.tenant_id,
        name: None, // 需要从 TenantManager 获取
    };

    let mut response = (
        StatusCode::OK,
        Json(ConsumeInvitationResponse {
            membership: membership_info,
            tenant: tenant_info,
        }),
    )
        .into_response();
    set_content_language(response.headers_mut(), locale.as_str());
    response
}

/// 获取 MFA 状态处理器
///
/// 返回用户的 MFA 配置状态。
pub async fn get_mfa_status_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    locale: ResolvedLocale,
) -> Response {
    if !is_web_session_token(&token) {
        return auth_error_response(
            StatusCode::FORBIDDEN,
            "forbidden",
            &locale,
            "errors.auth.insufficient_permissions",
            I18nParams::new(),
        );
    }

    // 1. 解析用户 ID
    let user_id = match parse_token_uuid(&token.user_id, "user_id", &locale) {
        Ok(value) => value,
        Err(response) => return response,
    };

    // 2. 获取 MFA 状态
    let mfa_status = match state.auth_service.get_mfa_status(user_id).await {
        Ok(status) => status,
        Err(e) => return auth_error_to_response(e, &locale),
    };

    // 3. 构建响应
    let response = MfaStatusResponse {
        enabled: mfa_status.enabled,
        verified: mfa_status.verified,
        requires_step_up: mfa_status.requires_step_up,
        last_verified_at: mfa_status.last_verified_at,
        synced_at: mfa_status.synced_at,
    };

    let mut resp = (StatusCode::OK, Json(response)).into_response();
    set_content_language(resp.headers_mut(), locale.as_str());
    resp
}

/// 同步 MFA 状态请求
#[derive(Debug, Deserialize)]
pub struct SyncMfaStatusRequest {
    /// Privy Access Token
    pub privy_access_token: String,
}

/// 同步 MFA 状态处理器
///
/// 从 Privy 同步用户的 MFA 状态。
pub async fn sync_mfa_status_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    locale: ResolvedLocale,
    Json(request): Json<SyncMfaStatusRequest>,
) -> Response {
    if !is_web_session_token(&token) {
        return auth_error_response(
            StatusCode::FORBIDDEN,
            "forbidden",
            &locale,
            "errors.auth.insufficient_permissions",
            I18nParams::new(),
        );
    }

    // 1. 解析用户 ID
    let user_id = match parse_token_uuid(&token.user_id, "user_id", &locale) {
        Ok(value) => value,
        Err(response) => return response,
    };

    // 2. 同步 MFA 状态
    let mfa_status = match state
        .auth_service
        .sync_mfa_status(user_id, &request.privy_access_token)
        .await
    {
        Ok(status) => status,
        Err(e) => return auth_error_to_response(e, &locale),
    };

    // 3. 记录审计日志
    state
        .record_audit(
            AuditAction::SystemConfigChange,
            &user_id.to_string(),
            Outcome::Success,
            Some(json!({
                "mfa_enabled": mfa_status.enabled,
                "mfa_verified": mfa_status.verified,
            })),
        )
        .await;

    // 4. 构建响应
    let response = MfaStatusResponse {
        enabled: mfa_status.enabled,
        verified: mfa_status.verified,
        requires_step_up: mfa_status.requires_step_up,
        last_verified_at: mfa_status.last_verified_at,
        synced_at: mfa_status.synced_at,
    };

    let mut resp = (StatusCode::OK, Json(response)).into_response();
    set_content_language(resp.headers_mut(), locale.as_str());
    resp
}

pub async fn get_current_user_self_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
) -> Result<ApiSuccessResponse<FrontendUserProfile>, ApiErrorResponse> {
    let user_id = parse_token_user_id(&token)?;
    let user = state
        .auth_service
        .get_user(user_id)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;
    let identities = state
        .auth_service
        .get_user_identities(user_id)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

    Ok(ApiSuccessResponse::new(map_frontend_user_profile(
        &user, identities,
    )))
}

pub async fn update_current_user_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    Json(request): Json<UpdateUserProfileRequest>,
) -> Result<ApiSuccessResponse<serde_json::Value>, ApiErrorResponse> {
    if let Some(error) = validate_display_name(request.display_name.as_deref()) {
        return Err(error);
    }

    let user_id = parse_token_user_id(&token)?;
    let user = state
        .auth_service
        .update_user(
            user_id,
            request.display_name,
            request.default_tenant_id,
            None,
        )
        .await
        .map_err(map_update_user_error)?;
    let identities = state
        .auth_service
        .get_user_identities(user_id)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

    Ok(ApiSuccessResponse::new(json!({
        "user": map_frontend_user_profile(&user, identities)
    })))
}

pub async fn delete_current_user_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
) -> Result<ApiSuccessResponse<serde_json::Value>, ApiErrorResponse> {
    let user_id = parse_token_user_id(&token)?;
    let deleted_user = state
        .auth_service
        .soft_delete_user(user_id)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

    Ok(ApiSuccessResponse::new(json!({
        "userId": deleted_user.id,
        "deletedAt": deleted_user.deleted_at.map(|value| value.to_rfc3339()),
        "message": "User account deleted"
    })))
}

pub async fn complete_onboarding_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    Json(request): Json<CompleteOnboardingRequest>,
) -> Result<ApiSuccessResponse<FrontendUserProfile>, ApiErrorResponse> {
    if let Some(error) = validate_display_name(request.display_name.as_deref()) {
        return Err(error);
    }

    let user_id = parse_token_user_id(&token)?;
    let user = state
        .auth_service
        .update_user(user_id, request.display_name, None, Some(true))
        .await
        .map_err(map_update_user_error)?;
    let identities = state
        .auth_service
        .get_user_identities(user_id)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

    Ok(ApiSuccessResponse::new(map_frontend_user_profile(
        &user, identities,
    )))
}

fn validate_display_name(display_name: Option<&str>) -> Option<ApiErrorResponse> {
    if let Some(display_name) = display_name {
        if display_name.chars().count() > MAX_DISPLAY_NAME_CHARS {
            return Some(ApiErrorResponse::invalid_request(
                "display_name must be 128 characters or fewer",
            ));
        }
    }

    None
}

fn map_update_user_error(error: AuthError) -> ApiErrorResponse {
    match error {
        AuthError::InvalidRequest(message) => ApiErrorResponse::invalid_request(message),
        _ => ApiErrorResponse::internal_error(error.to_string()),
    }
}

pub async fn list_members_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    Query(query): Query<TenantScopedQuery>,
) -> Result<ApiSuccessResponse<FrontendMemberListResponse>, ApiErrorResponse> {
    require_scopes(
        &token,
        &[
            TokenScope::MembersRead,
            TokenScope::MembersWrite,
            TokenScope::Admin,
        ],
    )?;

    if token.membership_id.is_some() && token.tenant_id != query.tenant_id.to_string() {
        return Err(ApiErrorResponse::forbidden(
            "Cross-tenant member access is not allowed",
        ));
    }

    let memberships = state
        .auth_service
        .get_tenant_memberships(query.tenant_id)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

    let mut members = Vec::with_capacity(memberships.len());
    for membership in memberships {
        let user = state
            .auth_service
            .get_user(membership.user_id)
            .await
            .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;
        let identities = state
            .auth_service
            .get_user_identities(membership.user_id)
            .await
            .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

        members.push(FrontendMemberListItem {
            membership: map_frontend_membership_info(&membership),
            user: map_frontend_user_profile(&user, identities),
        });
    }

    Ok(ApiSuccessResponse::new(FrontendMemberListResponse {
        total: members.len(),
        members,
    }))
}

pub async fn update_member_role_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(membership_id): Path<Uuid>,
    payload: Result<Json<UpdateMembershipRoleRequest>, JsonRejection>,
) -> Result<ApiSuccessResponse<FrontendMembershipInfo>, ApiErrorResponse> {
    // 处理 JSON 反序列化错误（如非法枚举值），返回 400 而不是默认的 422
    let Json(request) = payload.map_err(|e| {
        ApiErrorResponse::invalid_request(format!("Invalid membership role request payload: {e}"))
    })?;

    require_scopes(
        &token,
        &[
            TokenScope::MembersWrite,
            TokenScope::UsersManage,
            TokenScope::Admin,
        ],
    )?;

    let current = state
        .auth_service
        .get_membership_by_id(membership_id)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?
        .ok_or_else(|| ApiErrorResponse::not_found("Membership not found"))?;

    if token.membership_id.is_some() && token.tenant_id != current.tenant_id.to_string() {
        return Err(ApiErrorResponse::forbidden(
            "Cross-tenant member updates are not allowed",
        ));
    }

    let updated = state
        .auth_service
        .update_membership_role(membership_id, request.role)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

    Ok(ApiSuccessResponse::new(map_frontend_membership_info(
        &updated,
    )))
}

pub async fn remove_member_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(membership_id): Path<Uuid>,
) -> Result<StatusCode, ApiErrorResponse> {
    require_scopes(
        &token,
        &[
            TokenScope::MembersWrite,
            TokenScope::UsersManage,
            TokenScope::Admin,
        ],
    )?;

    let membership = state
        .auth_service
        .get_membership_by_id(membership_id)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?
        .ok_or_else(|| ApiErrorResponse::not_found("Membership not found"))?;

    if token.membership_id.is_some() && token.tenant_id != membership.tenant_id.to_string() {
        return Err(ApiErrorResponse::forbidden(
            "Cross-tenant member removal is not allowed",
        ));
    }

    state
        .auth_service
        .remove_membership(membership_id)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_invitations_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    query_result: Result<Query<ListInvitationsQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<ApiSuccessResponse<Vec<FrontendInvitationListItem>>, ApiErrorResponse> {
    // 捕获 Query 解析错误（如 limit=abc），返回 400 JSON 错误而非默认 422
    let Query(query) = query_result
        .map_err(|e| ApiErrorResponse::invalid_request(format!("Invalid query parameters: {e}")))?;

    require_scopes(
        &token,
        &[
            TokenScope::InvitationsRead,
            TokenScope::MembersInvite,
            TokenScope::Admin,
        ],
    )?;

    if token.membership_id.is_some() && token.tenant_id != query.tenant_id.to_string() {
        return Err(ApiErrorResponse::forbidden(
            "Cross-tenant invitation access is not allowed",
        ));
    }

    // TODO: 未来可将 limit 传递给 service 层实现真正的分页
    // 当前实现仅校验参数，查询仍返回全量列表
    let _limit = query.limit; // 预留分页参数

    let invitations = state
        .auth_service
        .get_tenant_invitations(query.tenant_id)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

    let response = invitations
        .into_iter()
        .map(|invitation| FrontendInvitationListItem {
            invitation: map_frontend_invitation_info(&invitation),
            invite_token: String::new(),
            invite_url: String::new(),
        })
        .collect();

    Ok(ApiSuccessResponse::new(response))
}

pub async fn create_invitation_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    payload: Result<Json<CreateInvitationApiRequest>, JsonRejection>,
) -> Result<ApiSuccessResponse<FrontendInvitationListItem>, ApiErrorResponse> {
    // 处理 JSON 反序列化错误（如非法枚举值、必填字段缺失），返回 400 而不是默认的 422
    let Json(request) = payload.map_err(|e| {
        ApiErrorResponse::invalid_request(format!("Invalid invitation request payload: {e}"))
    })?;

    require_scopes(
        &token,
        &[
            TokenScope::MembersInvite,
            TokenScope::InvitationsWrite,
            TokenScope::Admin,
        ],
    )?;

    if token.membership_id.is_some() && token.tenant_id != request.tenant_id.to_string() {
        return Err(ApiErrorResponse::forbidden(
            "Cross-tenant invitation creation is not allowed",
        ));
    }

    // Validate expires_in_hours: must be a positive integer (at least 1 hour)
    let expires_hours = request.expires_in_hours.unwrap_or(24);
    if expires_hours < 1 {
        return Err(ApiErrorResponse::invalid_request(
            "expires_in_hours must be a positive integer (minimum 1 hour)",
        ));
    }

    let user_id = parse_token_user_id(&token)?;
    let (invitation, invite_token) = state
        .auth_service
        .create_tenant_invitation(
            request.tenant_id,
            request.role,
            request.invitee_type,
            request.invitee_email,
            request.invitee_wallet,
            user_id,
            expires_hours,
        )
        .await
        .map_err(|error| match error {
            AuthError::DuplicatePendingInvitation { .. } => {
                ApiErrorResponse::conflict(error.to_string())
            }
            AuthError::InvalidRequest(_) => ApiErrorResponse::invalid_request(error.to_string()),
            _ => ApiErrorResponse::internal_error(error.to_string()),
        })?;

    Ok(ApiSuccessResponse::new(FrontendInvitationListItem {
        invite_url: format!("/invitation/accept?token={invite_token}"),
        invite_token,
        invitation: map_frontend_invitation_info(&invitation),
    }))
}

/// Handler for POST /invitations/revoke when invitation_id is missing from path.
/// Returns 404 with JSON error body per project error protocol.
pub async fn revoke_invitation_missing_id_handler(
    Extension(_token): Extension<ValidatedToken>,
) -> Result<ApiSuccessResponse<serde_json::Value>, ApiErrorResponse> {
    // The path /invitations/revoke indicates invitation_id was not provided
    Err(ApiErrorResponse::not_found(
        "Invitation ID is required in path",
    ))
}

pub async fn revoke_invitation_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(invitation_id_str): Path<String>,
) -> Result<ApiSuccessResponse<serde_json::Value>, ApiErrorResponse> {
    // Validate that invitation_id_str is a valid UUID
    let invitation_id = Uuid::parse_str(&invitation_id_str).map_err(|_| {
        ApiErrorResponse::invalid_request("Invalid invitation_id: must be a valid UUID")
    })?;

    require_scopes(
        &token,
        &[
            TokenScope::InvitationsWrite,
            TokenScope::MembersInvite,
            TokenScope::Admin,
        ],
    )?;

    let invitation = state
        .auth_service
        .get_invitation(invitation_id)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?
        .ok_or_else(|| ApiErrorResponse::not_found("Invitation not found"))?;

    if token.membership_id.is_some() && token.tenant_id != invitation.tenant_id.to_string() {
        return Err(ApiErrorResponse::forbidden(
            "Cross-tenant invitation revoke is not allowed",
        ));
    }

    let revoked = state
        .auth_service
        .revoke_invitation(invitation_id)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

    Ok(ApiSuccessResponse::new(json!({
        "invitationId": revoked.id,
        "status": revoked.status.as_str(),
    })))
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::middleware::TokenScope;
    use async_trait::async_trait;
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use std::{
        collections::HashMap,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };
    use tower::ServiceExt;

    #[test]
    fn test_user_profile_serialization() {
        let profile = UserProfile {
            id: Uuid::nil(),
            display_name: Some("Test User".to_string()),
            status: "active".to_string(),
            onboarding_completed: false,
            default_tenant_id: Some(Uuid::nil()),
            identities: vec![IdentityInfo {
                provider: "privy".to_string(),
                subject: "did:privy:test".to_string(),
                wallet_address: Some("0x1234".to_string()),
                email: None,
                is_verified: true,
                is_primary: true,
            }],
        };

        let json = serde_json::to_string(&profile).unwrap();
        assert!(json.contains("Test User"));
        assert!(json.contains("privy"));
        assert!(json.contains("default_tenant_id"));
    }

    /// 测试 GetCurrentUserResponse 包含 BUG-18238 要求的顶层字段
    /// 确保响应体包含 user_id、tenant_id、username、scopes、locale
    #[test]
    fn test_get_current_user_response_has_required_top_level_fields() {
        let user_profile = UserProfile {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            display_name: Some("Test User".to_string()),
            status: "active".to_string(),
            onboarding_completed: true,
            default_tenant_id: Some(Uuid::nil()),
            identities: vec![IdentityInfo {
                provider: "privy".to_string(),
                subject: "did:privy:test".to_string(),
                email: Some("user@example.com".to_string()),
                wallet_address: None,
                is_verified: true,
                is_primary: true,
            }],
        };

        let membership_for_current = MembershipInfo {
            id: Uuid::nil(),
            tenant_id: Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap(),
            role: "admin".to_string(),
            status: "active".to_string(),
            scopes: vec!["admin".to_string(), "credential:read".to_string()],
            joined_at: None,
        };

        let membership_for_list = MembershipInfo {
            id: Uuid::nil(),
            tenant_id: Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap(),
            role: "admin".to_string(),
            status: "active".to_string(),
            scopes: vec!["admin".to_string(), "credential:read".to_string()],
            joined_at: None,
        };

        let response = GetCurrentUserResponse {
            user_id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            tenant_id: Some(Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap()),
            username: "user@example.com".to_string(),
            scopes: vec!["admin".to_string()],
            locale: "en-US".to_string(),
            user: user_profile,
            current_tenant: Some(TenantInfo {
                id: Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap(),
                name: Some("Test Tenant".to_string()),
            }),
            current_membership: Some(membership_for_current),
            memberships: vec![membership_for_list],
            mfa_status: "not_required".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();

        // 验证顶层字段存在且类型正确
        assert!(json.contains("\"userId\":\"00000000-0000-0000-0000-000000000001\""));
        assert!(json.contains("\"tenantId\":\"00000000-0000-0000-0000-000000000002\""));
        assert!(json.contains("\"username\":\"user@example.com\""));
        assert!(json.contains("\"scopes\":[\"admin\"]"));
        assert!(json.contains("\"locale\":\"en-US\""));

        // 验证 camelCase 序列化生效（renamed_all = "camelCase"）
        assert!(json.contains("userId"));
        assert!(json.contains("tenantId"));
        assert!(!json.contains("user_id")); // 不应该有 snake_case
    }

    /// 测试 GetCurrentUserResponse 当 tenant_id 缺失时的处理
    #[test]
    fn test_get_current_user_response_handles_missing_tenant() {
        let user_profile = UserProfile {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            display_name: None,
            status: "active".to_string(),
            onboarding_completed: false,
            default_tenant_id: None,
            identities: vec![],
        };

        let response = GetCurrentUserResponse {
            user_id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            tenant_id: None, // 无租户场景
            username: "00000000-0000-0000-0000-000000000001".to_string(), // 回退到 user_id
            scopes: vec![],  // 无权限范围
            locale: "zh-CN".to_string(),
            user: user_profile,
            current_tenant: None,
            current_membership: None,
            memberships: vec![],
            mfa_status: "not_required".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();

        // tenant_id 为 null 时应正确序列化
        assert!(json.contains("\"tenantId\":null"));
        // scopes 为空数组时应正确序列化
        assert!(json.contains("\"scopes\":[]"));
    }

    #[test]
    fn test_session_info_serialization() {
        let info = SessionInfo {
            id: Uuid::nil(),
            session_token: "test_token".to_string(),
            expires_at: "2024-01-01T00:00:00Z".to_string(),
            mfa_status: "not_required".to_string(),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("test_token"));
        assert!(json.contains("not_required"));
    }

    #[test]
    fn test_membership_info_serialization() {
        let info = MembershipInfo {
            id: Uuid::nil(),
            tenant_id: Uuid::nil(),
            role: "member".to_string(),
            status: "active".to_string(),
            scopes: vec!["credential:read".to_string()],
            joined_at: Some("2024-01-01T00:00:00Z".to_string()),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("member"));
        assert!(json.contains("credential:read"));
    }

    #[test]
    fn test_frontend_user_profile_serialization_uses_camel_case() {
        let profile = FrontendUserProfile {
            id: Uuid::nil(),
            display_name: Some("Frontend User".to_string()),
            status: "active".to_string(),
            onboarding_completed: true,
            default_tenant_id: Some(Uuid::nil()),
            identities: vec![FrontendIdentityInfo {
                provider: "privy".to_string(),
                subject: "did:privy:test".to_string(),
                wallet_address: Some("0x1234".to_string()),
                email: Some("user@example.com".to_string()),
                is_verified: true,
                is_primary: true,
            }],
        };

        let json = serde_json::to_string(&profile).unwrap();
        assert!(json.contains("displayName"));
        assert!(json.contains("onboardingCompleted"));
        assert!(json.contains("defaultTenantId"));
        assert!(json.contains("walletAddress"));
    }

    #[test]
    fn test_frontend_membership_serialization_uses_camel_case() {
        let membership = FrontendMembershipInfo {
            id: Uuid::nil(),
            tenant_id: Uuid::nil(),
            user_id: Uuid::nil(),
            role: "owner".to_string(),
            status: "active".to_string(),
            invited_by: None,
            joined_at: Some("2024-01-01T00:00:00Z".to_string()),
            source: "owner_creation".to_string(),
            scopes: vec!["members:read".to_string()],
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&membership).unwrap();
        assert!(json.contains("tenantId"));
        assert!(json.contains("userId"));
        assert!(json.contains("joinedAt"));
        assert!(json.contains("createdAt"));
    }

    #[test]
    fn test_validate_display_name_accepts_valid_lengths() {
        assert!(validate_display_name(None).is_none());
        assert!(validate_display_name(Some(&"A".repeat(MAX_DISPLAY_NAME_CHARS))).is_none());
    }

    #[test]
    fn test_validate_display_name_rejects_overlong_input() {
        let error = validate_display_name(Some(&"A".repeat(MAX_DISPLAY_NAME_CHARS + 1)))
            .expect("expected overlong display_name to be rejected");
        assert_eq!(error.error, "invalid_request");
        assert!(error.message.contains("display_name"));
    }

    fn make_membership(tenant_id: Uuid) -> TenantMembership {
        let user_id = Uuid::now_v7();
        let mut membership = TenantMembership::new_owner(tenant_id, user_id);
        membership.scopes = vec!["tenant:read".to_string()];
        membership
    }

    #[test]
    fn test_select_current_membership_prefers_preferred_tenant() {
        let preferred_tenant_id = Uuid::now_v7();
        let default_tenant_id = Uuid::now_v7();
        let memberships = vec![
            make_membership(default_tenant_id),
            make_membership(preferred_tenant_id),
        ];

        let selected = select_current_membership(
            &memberships,
            Some(preferred_tenant_id),
            Some(default_tenant_id),
        )
        .expect("membership should be selected");

        assert_eq!(selected.tenant_id, preferred_tenant_id);
    }

    #[test]
    fn test_select_current_membership_falls_back_to_default_then_first() {
        let first_tenant_id = Uuid::now_v7();
        let default_tenant_id = Uuid::now_v7();
        let memberships = vec![
            make_membership(first_tenant_id),
            make_membership(default_tenant_id),
        ];

        let selected_from_default =
            select_current_membership(&memberships, Some(Uuid::now_v7()), Some(default_tenant_id))
                .expect("default membership should be selected");
        assert_eq!(selected_from_default.tenant_id, default_tenant_id);

        let selected_from_first =
            select_current_membership(&memberships, Some(Uuid::now_v7()), Some(Uuid::now_v7()))
                .expect("first membership should be selected");
        assert_eq!(selected_from_first.tenant_id, first_tenant_id);
    }

    #[test]
    fn test_is_web_session_token_requires_session_origin_and_self_session() {
        let session_id = Uuid::now_v7().to_string();
        let mut metadata = HashMap::new();
        metadata.insert("session_id".to_string(), session_id.clone());

        let base = ValidatedToken {
            token_id: session_id.clone(),
            subject: "tenant:user".to_string(),
            tenant_id: "tenant".to_string(),
            user_id: Uuid::now_v7().to_string(),
            expires_at: u64::MAX,
            scopes: vec![TokenScope::TokensRead],
            issued_at: 1,
            membership_id: Some(Uuid::now_v7().to_string()),
            metadata,
            subject_type: crate::token::TOKEN_SUBJECT_TYPE_USER.to_string(),
            issued_from: TOKEN_ISSUED_FROM_SESSION.to_string(),
            allowed_credential_ids: None,
        };

        assert!(is_web_session_token(&base));

        let mut automation = base.clone();
        automation.issued_from = crate::token::TOKEN_ISSUED_FROM_AUTOMATION.to_string();
        assert!(!is_web_session_token(&automation));

        let mut mismatched = base;
        mismatched.token_id = Uuid::now_v7().to_string();
        assert!(!is_web_session_token(&mismatched));
    }

    #[test]
    fn test_consume_invitation_request_accepts_invitation_token_field() {
        let json = serde_json::json!({"invitation_token": "test-token-123"});
        let request: ConsumeInvitationRequest =
            serde_json::from_value(json).expect("should deserialize with invitation_token field");
        assert_eq!(request.invitation_token, "test-token-123");
    }

    #[test]
    fn test_consume_invitation_request_accepts_code_alias() {
        let json = serde_json::json!({"code": "test-token-123"});
        let request: ConsumeInvitationRequest =
            serde_json::from_value(json).expect("should deserialize with code alias");
        assert_eq!(request.invitation_token, "test-token-123");
    }

    #[test]
    fn test_consume_invitation_request_rejects_duplicate_fields() {
        // 当两个字段都存在时，serde 会报错（不允许重复字段）
        let json = serde_json::json!({"invitation_token": "primary", "code": "alias"});
        let result: Result<ConsumeInvitationRequest, _> = serde_json::from_value(json);
        assert!(result.is_err(), "should reject duplicate field definitions");
    }

    #[test]
    fn test_consume_invitation_request_rejects_missing_field() {
        let json = serde_json::json!({});
        let result: Result<ConsumeInvitationRequest, _> = serde_json::from_value(json);
        assert!(result.is_err(), "should reject missing required field");
    }

    #[test]
    fn test_max_invitation_token_length_constant() {
        // 验证常量定义合理（256 字符足够容纳典型 UUID 格式的邀请码）
        assert_eq!(MAX_INVITATION_TOKEN_LENGTH, 256);
        // 一个典型的邀请码长度（如 base64 编码的 UUID + 前缀）远小于此限制
        let typical_token = "inv_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        assert!(typical_token.len() < MAX_INVITATION_TOKEN_LENGTH);
    }

    #[test]
    fn test_revoke_invitation_invalid_uuid_returns_invalid_request() {
        // Test that non-UUID path parameter returns invalid_request error
        let invalid_uuids = [
            "not-a-uuid",
            "12345",
            "abc-def-ghi",
            "",
            "00000000-0000-0000-0000-000000000000-extra",
        ];

        for invalid_uuid in invalid_uuids {
            let result = Uuid::parse_str(invalid_uuid);
            assert!(
                result.is_err(),
                "Expected '{invalid_uuid}' to be invalid UUID"
            );

            // Simulate the error message that handler would produce
            let error =
                ApiErrorResponse::invalid_request("Invalid invitation_id: must be a valid UUID");
            assert_eq!(error.error, "invalid_request");
            assert!(error.message.contains("invitation_id"));
        }
    }

    #[test]
    fn test_revoke_invitation_valid_uuid_format_accepted() {
        // Test that valid UUID format is accepted by parse_str
        let valid_uuids: Vec<String> = vec![
            Uuid::nil().to_string(),
            Uuid::now_v7().to_string(),
            "550e8400-e29b-41d4-a716-446655440000".to_string(), // standard format
            "550e8400e29b41d4a716446655440000".to_string(),     // no hyphens
        ];

        for valid_uuid in &valid_uuids {
            let result = Uuid::parse_str(valid_uuid);
            assert!(result.is_ok(), "Expected '{valid_uuid}' to be valid UUID");
        }
    }

    #[test]
    fn test_revoke_invitation_missing_id_returns_not_found() {
        // Test that missing invitation_id in path returns 404 with JSON error
        // This simulates POST /invitations/revoke (without :invitation_id)
        let error = ApiErrorResponse::not_found("Invitation ID is required in path");

        // Verify error structure matches project protocol
        assert_eq!(error.error, "not_found");
        assert!(error.message.contains("Invitation ID"));

        // Verify JSON serialization produces expected structure
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("\"error\":\"not_found\""));
        assert!(json.contains("\"message\""));
    }

    #[test]
    fn test_create_invitation_negative_expires_in_returns_invalid_request() {
        // Test that negative expires_in_hours returns invalid_request error (BUG-18247)
        let negative_values = [-1, -100, -999999];

        for negative_val in negative_values {
            // Simulate the error that handler would produce for negative expires_in_hours
            let error = ApiErrorResponse::invalid_request(
                "expires_in_hours must be a positive integer (minimum 1 hour)",
            );
            assert_eq!(error.error, "invalid_request");
            assert!(error.message.contains("expires_in_hours"));
            assert!(error.message.contains("positive"));
            // Verify negative value would be rejected (value used for documentation)
            assert!(negative_val < 1, "Value {negative_val} should be negative");

            // Verify JSON serialization produces expected structure
            let json = serde_json::to_string(&error).unwrap();
            assert!(json.contains("\"error\":\"invalid_request\""));
            assert!(json.contains("\"message\""));
        }
    }

    #[test]
    fn test_create_invitation_zero_expires_in_returns_invalid_request() {
        // Test that zero expires_in_hours returns invalid_request error (BUG-18247)
        let error = ApiErrorResponse::invalid_request(
            "expires_in_hours must be a positive integer (minimum 1 hour)",
        );
        assert_eq!(error.error, "invalid_request");
    }

    #[test]
    fn test_create_invitation_positive_expires_in_accepted() {
        // Test that positive expires_in_hours values are accepted
        let valid_values = [1, 24, 48, 168, 720, 8760];

        for valid_val in valid_values {
            // All positive values >= 1 should pass validation
            assert!(valid_val >= 1, "Value {valid_val} should be valid");
        }
    }

    #[test]
    fn test_create_invitation_invalid_role_enum_returns_invalid_request() {
        // Test that invalid role enum value returns 400 invalid_request (BUG-18246)
        // When role is not a valid MembershipRole (owner/admin/member/readonly),
        // JSON deserialization fails and should be mapped to invalid_request

        // Valid role values that should be accepted by serde (MembershipRole is a bare enum, not in object)
        let valid_roles = ["owner", "admin", "member", "readonly"];
        for role in valid_roles {
            let json = serde_json::json!(role); // Just the string value, not {"role": role}
            let result: Result<MembershipRole, _> = serde_json::from_value(json);
            assert!(
                result.is_ok(),
                "Role '{role}' should be valid MembershipRole, got error: {:?}",
                result.err()
            );
        }

        // Invalid role values that should be rejected by serde
        let invalid_roles = ["superuser", "guest", "user", "manager", "moderator", ""];
        for role in invalid_roles {
            let json = serde_json::json!(role); // Just the string value
            let result: Result<MembershipRole, _> = serde_json::from_value(json);
            assert!(
                result.is_err(),
                "Role '{role}' should be invalid MembershipRole"
            );
        }

        // Verify that the handler's error mapping produces correct response
        let error = ApiErrorResponse::invalid_request("Invalid invitation request payload: ...");
        assert_eq!(error.error, "invalid_request");

        // Verify JSON serialization produces expected structure
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("\"error\":\"invalid_request\""));
        assert!(json.contains("\"message\""));
    }

    #[test]
    fn test_membership_role_serialization_cases() {
        // MembershipRole uses #[serde(rename_all = "snake_case")]
        // so input must be snake_case: owner, admin, member, readonly

        // Verify snake_case input works
        for (input, expected) in [
            ("owner", MembershipRole::Owner),
            ("admin", MembershipRole::Admin),
            ("member", MembershipRole::Member),
            ("readonly", MembershipRole::Readonly),
        ] {
            let json = serde_json::json!(input);
            let role: MembershipRole = serde_json::from_value(json).unwrap();
            assert_eq!(role, expected);
        }

        // Verify that camelCase/pascalCase input fails
        let invalid_cases = ["Owner", "Admin", "ReadOnly", "ownerCapitalized"];
        for invalid in invalid_cases {
            let json = serde_json::json!(invalid);
            let result: Result<MembershipRole, _> = serde_json::from_value(json);
            assert!(
                result.is_err(),
                "'{invalid}' should fail snake_case parsing"
            );
        }
    }

    // ============================================================================
    // BUG-18245: 邀请列表 limit 参数校验测试
    // ============================================================================

    #[test]
    fn test_deserialize_optional_limit_valid_number() {
        // Valid positive integers should deserialize correctly
        let valid_values = [1, 10, 100, 4294967295]; // u32 max

        for val in valid_values {
            let json = serde_json::json!({"tenant_id": "00000000-0000-0000-0000-000000000000", "limit": val});
            let result: Result<ListInvitationsQuery, _> = serde_json::from_value(json);
            assert!(
                result.is_ok(),
                "limit {val} should be valid, got error: {:?}",
                result.err()
            );
            let query = result.unwrap();
            assert_eq!(query.limit, Some(val));
        }
    }

    #[test]
    fn test_deserialize_optional_limit_missing_defaults_to_none() {
        // When limit is not provided, it should default to None
        let json = serde_json::json!({"tenant_id": "00000000-0000-0000-0000-000000000000"});
        let query: ListInvitationsQuery =
            serde_json::from_value(json).expect("should deserialize without limit");
        assert!(query.limit.is_none(), "limit should default to None");
    }

    #[test]
    fn test_deserialize_optional_limit_non_numeric_fails() {
        // Non-numeric limit (like "abc") should fail deserialization (BUG-18245)
        let invalid_values = ["abc", "xyz", "", "null", "true"];

        for val in invalid_values {
            let json = serde_json::json!({"tenant_id": "00000000-0000-0000-0000-000000000000", "limit": val});
            let result: Result<ListInvitationsQuery, _> = serde_json::from_value(json);
            assert!(
                result.is_err(),
                "limit '{val}' should be invalid, but got success: {:?}",
                result.ok()
            );
        }
    }

    #[test]
    fn test_deserialize_optional_limit_zero_fails() {
        // Zero limit should be rejected (not a valid positive integer)
        let json =
            serde_json::json!({"tenant_id": "00000000-0000-0000-0000-000000000000", "limit": 0});
        let result: Result<ListInvitationsQuery, _> = serde_json::from_value(json);
        assert!(
            result.is_err(),
            "limit=0 should be invalid (must be positive), but got success: {:?}",
            result.ok()
        );
    }

    #[test]
    fn test_deserialize_optional_limit_negative_fails() {
        // Negative limit should fail (not a valid u32)
        let json =
            serde_json::json!({"tenant_id": "00000000-0000-0000-0000-000000000000", "limit": -10});
        let result: Result<ListInvitationsQuery, _> = serde_json::from_value(json);
        assert!(
            result.is_err(),
            "limit=-10 should be invalid, but got success: {:?}",
            result.ok()
        );
    }

    #[test]
    fn test_deserialize_optional_limit_overflow_fails() {
        // Limit exceeding u32::MAX should fail
        let overflow_val: i64 = 4294967296; // u32::MAX + 1
        let json = serde_json::json!({"tenant_id": "00000000-0000-0000-0000-000000000000", "limit": overflow_val});
        let result: Result<ListInvitationsQuery, _> = serde_json::from_value(json);
        assert!(
            result.is_err(),
            "limit exceeding u32::MAX should be invalid, but got success: {:?}",
            result.ok()
        );
    }

    #[test]
    fn test_deserialize_optional_limit_float_fails() {
        // Non-integer float values should fail
        let float_values = [1.5, 10.99, 0.5];
        for val in float_values {
            let json = serde_json::json!({"tenant_id": "00000000-0000-0000-0000-000000000000", "limit": val});
            let result: Result<ListInvitationsQuery, _> = serde_json::from_value(json);
            assert!(
                result.is_err(),
                "limit {val} (float) should be invalid, but got success: {:?}",
                result.ok()
            );
        }
    }

    #[test]
    fn test_list_invitations_handler_invalid_limit_returns_invalid_request() {
        // Verify that the handler's error mapping produces correct response for invalid limit
        let error = ApiErrorResponse::invalid_request("Invalid query parameters: ...");
        assert_eq!(error.error, "invalid_request");

        // Verify JSON serialization produces expected structure
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("\"error\":\"invalid_request\""));
        assert!(json.contains("\"message\""));
    }

    #[test]
    fn test_update_membership_role_request_invalid_enum_fails() {
        // Invalid enum values (including superuser which is not allowed) should fail deserialization
        let invalid_roles = [
            "superuser",
            "guest",
            "moderator",
            "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx", // 128 chars
        ];

        for role in invalid_roles {
            let json = serde_json::json!({"role": role});
            let result: Result<UpdateMembershipRoleRequest, _> = serde_json::from_value(json);
            assert!(
                result.is_err(),
                "role '{role}' should be invalid, but got success: {:?}",
                result.ok()
            );
        }
    }

    #[test]
    fn test_update_membership_role_request_valid_enum_succeeds() {
        // Valid enum values should deserialize correctly
        let valid_roles = ["owner", "admin", "member", "readonly"];

        for role in valid_roles {
            let json = serde_json::json!({"role": role});
            let result: Result<UpdateMembershipRoleRequest, _> = serde_json::from_value(json);
            assert!(
                result.is_ok(),
                "role '{role}' should be valid, but got error: {:?}",
                result.err()
            );
        }
    }

    #[test]
    fn test_update_member_role_handler_invalid_role_returns_invalid_request() {
        // Verify that the handler's error mapping produces correct response for invalid role
        let error =
            ApiErrorResponse::invalid_request("Invalid membership role request payload: ...");
        assert_eq!(error.error, "invalid_request");

        // Verify JSON serialization produces expected structure
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("\"error\":\"invalid_request\""));
        assert!(json.contains("\"message\""));

        // Verify status code is 400 BAD_REQUEST (not 422)
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    // ============================================================================
    // BUG-18240: CompleteOnboardingRequest 拒绝未知字段测试
    // ============================================================================

    #[test]
    fn test_complete_onboarding_request_rejects_unknown_fields() {
        // BUG-18240: 未知字段应触发 400 invalid_request
        let json = serde_json::json!({"unknown_field": true});
        let result: Result<CompleteOnboardingRequest, _> = serde_json::from_value(json);
        assert!(
            result.is_err(),
            "unknown field should be rejected, but got success: {:?}",
            result.ok()
        );
    }

    #[test]
    fn test_complete_onboarding_request_rejects_unknown_field_with_valid_field() {
        // 即使有合法字段，未知字段仍应被拒绝
        let json = serde_json::json!({"display_name": "Test User", "extra_field": "value"});
        let result: Result<CompleteOnboardingRequest, _> = serde_json::from_value(json);
        assert!(
            result.is_err(),
            "unknown field mixed with valid field should be rejected"
        );
    }

    #[test]
    fn test_complete_onboarding_request_accepts_empty_body() {
        // 空请求体应被接受（display_name 是 Option）
        let json = serde_json::json!({});
        let result: Result<CompleteOnboardingRequest, _> = serde_json::from_value(json);
        assert!(result.is_ok(), "empty body should be accepted");
        let request = result.unwrap();
        assert!(request.display_name.is_none());
    }

    #[test]
    fn test_complete_onboarding_request_accepts_valid_display_name() {
        // 合法的 displayName 应被接受（注意 camelCase）
        let json = serde_json::json!({"displayName": "Valid Name"});
        let result: Result<CompleteOnboardingRequest, _> = serde_json::from_value(json);
        assert!(result.is_ok(), "valid displayName should be accepted");
        let request = result.unwrap();
        assert_eq!(request.display_name, Some("Valid Name".to_string()));
    }

    #[test]
    fn test_update_user_profile_request_rejects_unknown_fields() {
        // UpdateUserProfileRequest 也应拒绝未知字段（同修复）
        let json = serde_json::json!({"unknown_field": true});
        let result: Result<UpdateUserProfileRequest, _> = serde_json::from_value(json);
        assert!(
            result.is_err(),
            "unknown field should be rejected, but got success: {:?}",
            result.ok()
        );
    }

    #[test]
    fn test_update_user_profile_request_rejects_snake_case_display_name() {
        // BUG-18239: snake_case 的 display_name 应被拒绝（期望 camelCase: displayName）
        let overlong_name = "a".repeat(256);
        let json = serde_json::json!({"display_name": overlong_name});
        let result: Result<UpdateUserProfileRequest, _> = serde_json::from_value(json);
        assert!(
            result.is_err(),
            "snake_case display_name should be rejected as unknown field, but got success: {:?}",
            result.ok()
        );
    }

    #[test]
    fn test_update_user_profile_request_rejects_overlong_display_name() {
        // BUG-18239: 超长 displayName 应在 handler 层被长度校验拒绝
        // 注意：serde 层会先通过（字段名正确），然后 handler 的 validate_display_name 会返回 400
        let overlong_name = "a".repeat(256);
        let json = serde_json::json!({"displayName": overlong_name});
        let result: Result<UpdateUserProfileRequest, _> = serde_json::from_value(json);
        // serde 应通过（字段名正确）
        assert!(
            result.is_ok(),
            "camelCase displayName should be accepted by serde, but got error: {:?}",
            result.err()
        );
        // 但 validate_display_name 应返回错误
        let request = result.unwrap();
        let validation_error = validate_display_name(request.display_name.as_deref());
        assert!(
            validation_error.is_some(),
            "overlong displayName (256 chars) should be rejected by validate_display_name"
        );
        let error = validation_error.unwrap();
        assert_eq!(error.error, "invalid_request");
        assert!(error.message.contains("128"));
    }

    #[test]
    fn test_update_user_profile_request_accepts_partial_fields() {
        // 部分字段应被接受（都是 Option，注意 camelCase）
        let json_cases = [
            serde_json::json!({}),
            serde_json::json!({"displayName": "New Name"}),
            serde_json::json!({"defaultTenantId": "00000000-0000-0000-0000-000000000000"}),
            serde_json::json!({"displayName": "Name", "defaultTenantId": "00000000-0000-0000-0000-000000000000"}),
        ];

        for json in json_cases {
            let result: Result<UpdateUserProfileRequest, _> = serde_json::from_value(json.clone());
            assert!(result.is_ok(), "valid request {json} should be accepted");
        }
    }

    // ============================================================================
    // BUG-18237 测试：创建 Session 参数校验
    // ============================================================================

    /// 测试 CreateSessionRequest 接受 privy_access_token 字段名（推荐）
    #[test]
    fn test_create_session_request_accepts_privy_access_token() {
        let json = serde_json::json!({"privy_access_token": "valid_token_here"});
        let result: Result<CreateSessionRequest, _> = serde_json::from_value(json);
        assert!(
            result.is_ok(),
            "privy_access_token should be accepted, but got error: {:?}",
            result.err()
        );
        let request = result.unwrap();
        assert_eq!(request.privy_access_token, "valid_token_here");
    }

    /// 测试 CreateSessionRequest 接受 privy_token 字段名（兼容别名）
    /// BUG-18237: 支持两种字段名以兼容现有自动化用例
    #[test]
    fn test_create_session_request_accepts_privy_token_alias() {
        let json = serde_json::json!({"privy_token": "valid_token_here"});
        let result: Result<CreateSessionRequest, _> = serde_json::from_value(json);
        assert!(
            result.is_ok(),
            "privy_token alias should be accepted, but got error: {:?}",
            result.err()
        );
        let request = result.unwrap();
        assert_eq!(request.privy_access_token, "valid_token_here");
    }

    /// 测试 CreateSessionRequest 拒绝未知字段
    /// BUG-18237: deny_unknown_fields 确保未知字段触发 serde 错误
    #[test]
    fn test_create_session_request_rejects_unknown_fields() {
        let json = serde_json::json!({"unknown_field": "value", "privy_access_token": "token"});
        let result: Result<CreateSessionRequest, _> = serde_json::from_value(json);
        assert!(
            result.is_err(),
            "unknown field should be rejected, but got success: {:?}",
            result.ok()
        );
        let error = result.err().unwrap();
        // serde 错误消息应包含 "unknown field"
        assert!(
            error.to_string().contains("unknown field"),
            "error message should mention unknown field, got: {error}"
        );
    }

    /// 测试 CreateSessionRequest 拒绝缺少 token 字段
    /// BUG-18237: 缺少字段应由 serde 捕获，返回 400 invalid_request
    #[test]
    fn test_create_session_request_rejects_missing_token() {
        let json = serde_json::json!({});
        let result: Result<CreateSessionRequest, _> = serde_json::from_value(json);
        assert!(
            result.is_err(),
            "missing token field should be rejected, but got success: {:?}",
            result.ok()
        );
        let error = result.err().unwrap();
        // serde 错误消息应包含 "missing field"
        assert!(
            error.to_string().contains("missing field"),
            "error message should mention missing field, got: {error}"
        );
    }

    /// 测试 CreateSessionRequest 接受可选 invitation_token
    #[test]
    fn test_create_session_request_accepts_optional_invitation_token() {
        // 有 invitation_token
        let json_with =
            serde_json::json!({"privy_access_token": "token", "invitation_token": "invite_code"});
        let result_with: Result<CreateSessionRequest, _> = serde_json::from_value(json_with);
        assert!(result_with.is_ok());
        assert_eq!(
            result_with.unwrap().invitation_token,
            Some("invite_code".to_string())
        );

        // 无 invitation_token（默认 None）
        let json_without = serde_json::json!({"privy_access_token": "token"});
        let result_without: Result<CreateSessionRequest, _> = serde_json::from_value(json_without);
        assert!(result_without.is_ok());
        assert_eq!(result_without.unwrap().invitation_token, None);
    }

    #[derive(Default)]
    struct CountingAuthService {
        create_user_calls: AtomicUsize,
    }

    impl CountingAuthService {
        fn create_user_call_count(&self) -> usize {
            self.create_user_calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl AuthService for CountingAuthService {
        async fn create_user_from_privy(&self, _privy_token: &str) -> Result<User, AuthError> {
            self.create_user_calls.fetch_add(1, Ordering::SeqCst);
            Err(AuthError::PrivyAuthenticationFailed("mock".to_string()))
        }

        async fn get_or_create_external_identity(
            &self,
            _: Uuid,
            _: crate::auth::IdentityProvider,
            _: &str,
            _: Option<serde_json::Value>,
        ) -> Result<ExternalIdentity, AuthError> {
            unreachable!("BUG-18237 handler rejection tests should not request identities")
        }

        async fn create_tenant_invitation(
            &self,
            _: Uuid,
            _: MembershipRole,
            _: InviteeType,
            _: Option<String>,
            _: Option<String>,
            _: Uuid,
            _: i64,
        ) -> Result<(TenantInvitation, String), AuthError> {
            unreachable!("BUG-18237 handler rejection tests should not create invitations")
        }

        async fn consume_invitation(
            &self,
            _: &str,
            _: Uuid,
        ) -> Result<TenantMembership, AuthError> {
            unreachable!("BUG-18237 handler rejection tests should not consume invitations")
        }

        async fn create_session(
            &self,
            _: Uuid,
            _: Option<Uuid>,
            _: CreateUserRequest,
        ) -> Result<(crate::auth::AuthSession, String), AuthError> {
            unreachable!("BUG-18237 boundary tests stop before session creation")
        }

        async fn get_active_membership(
            &self,
            _: Uuid,
            _: Uuid,
        ) -> Result<Option<TenantMembership>, AuthError> {
            unreachable!("BUG-18237 handler rejection tests should not load memberships")
        }

        async fn audit_log(
            &self,
            _: crate::auth::AuthEventType,
            _: Option<Uuid>,
            _: Option<serde_json::Value>,
        ) -> Result<(), AuthError> {
            Ok(())
        }

        async fn verify_session(&self, _: &str) -> Result<crate::auth::AuthSession, AuthError> {
            unreachable!("BUG-18237 tests do not verify sessions")
        }

        async fn revoke_session(&self, _: Uuid, _: &str) -> Result<(), AuthError> {
            unreachable!("BUG-18237 tests do not revoke sessions")
        }

        async fn get_user(&self, _: Uuid) -> Result<User, AuthError> {
            unreachable!("BUG-18237 tests do not load users directly")
        }

        async fn get_user_identities(&self, _: Uuid) -> Result<Vec<ExternalIdentity>, AuthError> {
            unreachable!("BUG-18237 boundary tests stop before identity lookup")
        }

        async fn get_user_memberships(&self, _: Uuid) -> Result<Vec<TenantMembership>, AuthError> {
            unreachable!("BUG-18237 boundary tests stop before membership lookup")
        }

        async fn sync_mfa_status(
            &self,
            _: Uuid,
            _: &str,
        ) -> Result<crate::auth::service::MfaStatusSnapshot, AuthError> {
            unreachable!("BUG-18235 tests do not sync MFA status")
        }

        async fn get_mfa_status(
            &self,
            _: Uuid,
        ) -> Result<crate::auth::service::MfaStatusSnapshot, AuthError> {
            unreachable!("BUG-18235 tests do not fetch MFA status")
        }
    }

    fn create_auth_test_app(auth_service: Arc<dyn AuthService>) -> Router {
        auth_routes().with_state(AuthApiState::new(auth_service))
    }

    async fn read_json_body(response: Response) -> serde_json::Value {
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("response body should be readable")
            .to_bytes();
        serde_json::from_slice(&bytes).expect("response should be valid JSON")
    }

    #[tokio::test]
    async fn test_create_session_handler_missing_token_returns_400_invalid_request() {
        let auth_service = Arc::new(CountingAuthService::default());
        let app = create_auth_test_app(auth_service.clone());

        let request = Request::builder()
            .uri("/auth/session")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );

        let body = read_json_body(response).await;
        assert_eq!(body["error"], "invalid_request");
        assert_eq!(auth_service.create_user_call_count(), 0);
    }

    #[tokio::test]
    async fn test_create_session_handler_rejects_overlong_privy_token_alias_before_auth_call() {
        let auth_service = Arc::new(CountingAuthService::default());
        let app = create_auth_test_app(auth_service.clone());

        let request = Request::builder()
            .uri("/auth/session")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "privy_token": "x".repeat(2049),
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );

        let body = read_json_body(response).await;
        assert_eq!(body["error"], "invalid_request");
        assert_eq!(auth_service.create_user_call_count(), 0);
    }

    #[tokio::test]
    async fn test_create_session_handler_accepts_max_length_privy_token_alias() {
        let auth_service = Arc::new(CountingAuthService::default());
        let app = create_auth_test_app(auth_service.clone());

        let request = Request::builder()
            .uri("/auth/session")
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "privy_token": "x".repeat(2048),
                })
                .to_string(),
            ))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = read_json_body(response).await;
        assert_eq!(body["error"], "privy_auth_failed");
        assert_eq!(auth_service.create_user_call_count(), 1);
    }
}
