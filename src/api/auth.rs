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
    extract::{Path, Query, State},
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
use crate::audit::{AuditAction, AuditEntry, Outcome, RedactedParam};
use crate::auth::{
    AuthError, AuthService, CreateUserRequest, ExternalIdentity, InviteeType, MembershipRole,
    TenantInvitation, TenantMembership, User,
};

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
}

impl AuthApiState {
    /// 创建新的认证 API 状态
    pub fn new(auth_service: Arc<dyn AuthService>) -> Self {
        Self {
            auth_service,
            token_store: create_token_store(),
            audit_storage: None,
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
        }
    }

    /// 设置审计存储
    pub fn with_audit_storage(mut self, storage: Arc<dyn AuditStorage>) -> Self {
        self.audit_storage = Some(storage);
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
#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    /// Privy Access Token
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
pub struct GetCurrentUserResponse {
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
#[serde(rename_all = "camelCase")]
pub struct UpdateUserProfileRequest {
    pub display_name: Option<String>,
    pub default_tenant_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteOnboardingRequest {
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TenantScopedQuery {
    pub tenant_id: Uuid,
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
    Json(request): Json<CreateSessionRequest>,
) -> Response {
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
    // 1. 获取用户信息
    let user_id = Uuid::parse_str(&token.user_id).unwrap_or(Uuid::nil());
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
    let tenant_id = Uuid::parse_str(&token.tenant_id).unwrap_or(Uuid::nil());
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
    let user_profile = map_user_profile(&user, identities);
    let memberships_info = memberships.iter().map(map_membership_info).collect();
    let current_tenant = current_membership.map(map_tenant_info);
    let current_membership_info = current_membership.map(map_membership_info);

    let mut response = (
        StatusCode::OK,
        Json(GetCurrentUserResponse {
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
    let user_id = Uuid::parse_str(&token.user_id).unwrap_or(Uuid::nil());
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
    // 1. 获取会话 ID
    let session_id_str = token
        .metadata
        .get("session_id")
        .cloned()
        .unwrap_or_default();
    let session_id = Uuid::parse_str(&session_id_str).unwrap_or(Uuid::nil());

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
    Json(request): Json<ConsumeInvitationRequest>,
) -> Response {
    // 1. 解析用户 ID
    let user_id = Uuid::parse_str(&token.user_id).unwrap_or(Uuid::nil());

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
    // 1. 解析用户 ID
    let user_id = Uuid::parse_str(&token.user_id).unwrap_or(Uuid::nil());

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
    // 1. 解析用户 ID
    let user_id = Uuid::parse_str(&token.user_id).unwrap_or(Uuid::nil());

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
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;
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
    let user_id = parse_token_user_id(&token)?;
    let user = state
        .auth_service
        .update_user(user_id, request.display_name, None, Some(true))
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
    Json(request): Json<UpdateMembershipRoleRequest>,
) -> Result<ApiSuccessResponse<FrontendMembershipInfo>, ApiErrorResponse> {
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
    Query(query): Query<TenantScopedQuery>,
) -> Result<ApiSuccessResponse<Vec<FrontendInvitationListItem>>, ApiErrorResponse> {
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
    Json(request): Json<CreateInvitationApiRequest>,
) -> Result<ApiSuccessResponse<FrontendInvitationListItem>, ApiErrorResponse> {
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
            request.expires_in_hours.unwrap_or(24),
        )
        .await
        .map_err(|error| match error {
            AuthError::DuplicatePendingInvitation { .. } => {
                ApiErrorResponse::conflict(error.to_string())
            }
            _ => ApiErrorResponse::internal_error(error.to_string()),
        })?;

    Ok(ApiSuccessResponse::new(FrontendInvitationListItem {
        invite_url: format!("/invitation/accept?token={invite_token}"),
        invite_token,
        invitation: map_frontend_invitation_info(&invitation),
    }))
}

pub async fn revoke_invitation_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(invitation_id): Path<Uuid>,
) -> Result<ApiSuccessResponse<serde_json::Value>, ApiErrorResponse> {
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
}
