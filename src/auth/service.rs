//! 认证服务层
//!
//! 提供认证相关核心业务逻辑：
//! - 用户创建与管理
//! - 外部身份绑定与验证
//! - 租户邀请创建与消费
//! - 会话管理
//! - 审计日志记录

#![allow(clippy::needless_borrows_for_generic_args)]

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use uuid::Uuid;

use super::error::AuthError;
use super::models::{
    ApiTokenMetadata, AuthAuditLog, AuthEventType, AuthSession, CreateUserRequest,
    ExternalIdentity, IdentityProvider, InvitationStatus, InviteeType, MembershipRole,
    PrivyAuthResponse, ServiceAccount, TenantInvitation, TenantMembership, User,
};
use crate::audit::AuditRecorder;
use crate::auth::privy::JwksVerifier;
use crate::config::PrivyConfig;
use crate::crypto::constant_time::ct_compare;
use crate::tenant::{TenantConfigStore, TenantManager, TenantService};

const MAX_DISPLAY_NAME_CHARS: usize = 128;

/// 认证服务 Trait
///
/// 定义认证服务的核心接口。
#[async_trait]
pub trait AuthService: Send + Sync {
    /// 从 Privy Token 创建或获取用户
    ///
    /// 解析 Privy Token，获取用户信息，如果用户不存在则创建新用户。
    /// 同时绑定外部身份。
    async fn create_user_from_privy(&self, privy_token: &str) -> Result<User, AuthError>;

    /// 获取或创建外部身份
    ///
    /// 根据提供商和 subject 查找外部身份，不存在则创建。
    async fn get_or_create_external_identity(
        &self,
        user_id: Uuid,
        provider: IdentityProvider,
        provider_subject: &str,
        profile: Option<JsonValue>,
    ) -> Result<ExternalIdentity, AuthError>;

    /// 创建租户邀请
    ///
    /// 创建新的租户邀请，返回邀请实体（包含 Token 哈希）。
    #[allow(clippy::too_many_arguments)]
    async fn create_tenant_invitation(
        &self,
        tenant_id: Uuid,
        role: MembershipRole,
        invitee_type: InviteeType,
        invitee_email: Option<String>,
        invitee_wallet: Option<String>,
        created_by: Uuid,
        expires_hours: i64,
    ) -> Result<(TenantInvitation, String), AuthError>;

    /// 消费邀请
    ///
    /// 验证邀请 Token，创建成员资格并激活。
    async fn consume_invitation(
        &self,
        invitation_token: &str,
        user_id: Uuid,
    ) -> Result<TenantMembership, AuthError>;

    /// 创建会话
    ///
    /// 为用户创建新的认证会话。
    async fn create_session(
        &self,
        user_id: Uuid,
        identity_id: Option<Uuid>,
        request: CreateUserRequest,
    ) -> Result<(AuthSession, String), AuthError>;

    /// 获取活跃成员资格
    ///
    /// 查询用户在指定租户的活跃成员资格。
    async fn get_active_membership(
        &self,
        user_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Option<TenantMembership>, AuthError>;

    /// 记录审计日志
    ///
    /// 记录认证相关事件到审计系统。
    async fn audit_log(
        &self,
        event_type: AuthEventType,
        user_id: Option<Uuid>,
        data: Option<JsonValue>,
    ) -> Result<(), AuthError>;

    /// 验证会话
    ///
    /// 验证会话 Token 并返回会话信息。
    async fn verify_session(&self, session_token: &str) -> Result<AuthSession, AuthError>;

    /// 撤销会话
    ///
    /// 撤销指定会话。
    async fn revoke_session(&self, session_id: Uuid, reason: &str) -> Result<(), AuthError>;

    /// 获取用户
    ///
    /// 根据 ID 获取用户信息。
    async fn get_user(&self, user_id: Uuid) -> Result<User, AuthError>;

    /// 更新用户资料。
    async fn update_user(
        &self,
        user_id: Uuid,
        display_name: Option<String>,
        default_tenant_id: Option<Uuid>,
        onboarding_completed: Option<bool>,
    ) -> Result<User, AuthError> {
        let _ = (
            user_id,
            display_name,
            default_tenant_id,
            onboarding_completed,
        );
        Err(AuthError::InternalError(
            "update_user is not implemented".to_string(),
        ))
    }

    /// 软删除用户。
    async fn soft_delete_user(&self, user_id: Uuid) -> Result<User, AuthError> {
        let _ = user_id;
        Err(AuthError::InternalError(
            "soft_delete_user is not implemented".to_string(),
        ))
    }

    /// 获取用户的所有外部身份
    ///
    /// 查询用户绑定的所有外部身份。
    async fn get_user_identities(&self, user_id: Uuid) -> Result<Vec<ExternalIdentity>, AuthError>;

    /// 获取用户的所有成员资格
    ///
    /// 查询用户的所有租户成员资格。
    async fn get_user_memberships(&self, user_id: Uuid)
    -> Result<Vec<TenantMembership>, AuthError>;

    /// 获取租户成员列表。
    async fn get_tenant_memberships(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<TenantMembership>, AuthError> {
        let _ = tenant_id;
        Err(AuthError::InternalError(
            "get_tenant_memberships is not implemented".to_string(),
        ))
    }

    /// 获取租户邀请列表。
    async fn get_tenant_invitations(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<TenantInvitation>, AuthError> {
        let _ = tenant_id;
        Err(AuthError::InternalError(
            "get_tenant_invitations is not implemented".to_string(),
        ))
    }

    /// 根据成员资格 ID 查询成员资格。
    async fn get_membership_by_id(
        &self,
        membership_id: Uuid,
    ) -> Result<Option<TenantMembership>, AuthError> {
        let _ = membership_id;
        Err(AuthError::InternalError(
            "get_membership_by_id is not implemented".to_string(),
        ))
    }

    /// 更新成员角色。
    async fn update_membership_role(
        &self,
        membership_id: Uuid,
        role: MembershipRole,
    ) -> Result<TenantMembership, AuthError> {
        let _ = (membership_id, role);
        Err(AuthError::InternalError(
            "update_membership_role is not implemented".to_string(),
        ))
    }

    /// 移除成员资格。
    async fn remove_membership(&self, membership_id: Uuid) -> Result<(), AuthError> {
        let _ = membership_id;
        Err(AuthError::InternalError(
            "remove_membership is not implemented".to_string(),
        ))
    }

    /// 根据邀请 ID 查询邀请。
    async fn get_invitation(
        &self,
        invitation_id: Uuid,
    ) -> Result<Option<TenantInvitation>, AuthError> {
        let _ = invitation_id;
        Err(AuthError::InternalError(
            "get_invitation is not implemented".to_string(),
        ))
    }

    /// 根据原始邀请 token 查询邀请。
    async fn get_invitation_by_token(
        &self,
        invitation_token: &str,
    ) -> Result<Option<TenantInvitation>, AuthError> {
        let _ = invitation_token;
        Err(AuthError::InternalError(
            "get_invitation_by_token is not implemented".to_string(),
        ))
    }

    /// 撤销邀请。
    async fn revoke_invitation(&self, invitation_id: Uuid) -> Result<TenantInvitation, AuthError> {
        let _ = invitation_id;
        Err(AuthError::InternalError(
            "revoke_invitation is not implemented".to_string(),
        ))
    }

    /// 为租户创建 owner membership。
    async fn create_owner_membership(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<TenantMembership, AuthError> {
        let _ = (tenant_id, user_id);
        Err(AuthError::InternalError(
            "create_owner_membership is not implemented".to_string(),
        ))
    }

    /// 创建 service account。
    async fn create_service_account(
        &self,
        service_account: &ServiceAccount,
    ) -> Result<ServiceAccount, AuthError> {
        Ok(service_account.clone())
    }

    /// 列出租户下的全部 service accounts。
    async fn list_service_accounts(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ServiceAccount>, AuthError> {
        let _ = tenant_id;
        Ok(Vec::new())
    }

    /// 按 ID 获取 service account。
    async fn get_service_account(
        &self,
        service_account_id: Uuid,
    ) -> Result<Option<ServiceAccount>, AuthError> {
        let _ = service_account_id;
        Ok(None)
    }

    /// 更新 service account。
    async fn update_service_account(
        &self,
        service_account: &ServiceAccount,
    ) -> Result<ServiceAccount, AuthError> {
        Ok(service_account.clone())
    }

    /// 创建 API token 元数据记录。
    async fn create_api_token_metadata(
        &self,
        metadata: &ApiTokenMetadata,
    ) -> Result<ApiTokenMetadata, AuthError> {
        Ok(metadata.clone())
    }

    /// 列出租户内 API token 元数据。
    async fn list_api_tokens(&self, tenant_id: Uuid) -> Result<Vec<ApiTokenMetadata>, AuthError> {
        let _ = tenant_id;
        Ok(Vec::new())
    }

    /// 列出某个 service account 的 API token 元数据。
    async fn list_service_account_api_tokens(
        &self,
        tenant_id: Uuid,
        service_account_id: Uuid,
    ) -> Result<Vec<ApiTokenMetadata>, AuthError> {
        let _ = (tenant_id, service_account_id);
        Ok(Vec::new())
    }

    /// 按 token ID 查询 API token 元数据。
    async fn get_api_token_metadata(
        &self,
        token_id: &str,
    ) -> Result<Option<ApiTokenMetadata>, AuthError> {
        let _ = token_id;
        Ok(None)
    }

    /// 标记 API token 已撤销。
    async fn revoke_api_token_metadata(
        &self,
        token_id: &str,
        revoked_at: DateTime<Utc>,
    ) -> Result<Option<ApiTokenMetadata>, AuthError> {
        let _ = (token_id, revoked_at);
        Ok(None)
    }

    /// 更新 API token 最近使用时间。
    async fn mark_api_token_used(
        &self,
        token_id: &str,
        last_used_at: DateTime<Utc>,
    ) -> Result<Option<ApiTokenMetadata>, AuthError> {
        let _ = (token_id, last_used_at);
        Ok(None)
    }

    /// 同步 MFA 状态
    ///
    /// 从 Privy 获取用户的 MFA 状态并更新会话快照。
    async fn sync_mfa_status(
        &self,
        user_id: Uuid,
        privy_token: &str,
    ) -> Result<MfaStatusSnapshot, AuthError>;

    /// 获取用户 MFA 状态
    ///
    /// 获取用户的 MFA 状态快照。
    async fn get_mfa_status(&self, user_id: Uuid) -> Result<MfaStatusSnapshot, AuthError>;
}

/// MFA 状态快照
///
/// 存储用户的 MFA 状态信息，从 Privy 同步。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MfaStatusSnapshot {
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

impl Default for MfaStatusSnapshot {
    fn default() -> Self {
        Self {
            enabled: false,
            verified: false,
            requires_step_up: false,
            last_verified_at: None,
            synced_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// 默认会话 TTL（秒）
const DEFAULT_SESSION_TTL: i64 = 3600; // 1 hour

/// 认证服务实现
///
/// 使用 PostgreSQL 作为持久化存储。
#[allow(dead_code)]
pub struct AuthServiceImpl {
    /// 数据库连接池（可选，用于持久化存储）
    db_pool: Option<PgPool>,

    /// 审计记录器（可选）
    audit_recorder: Option<AuditRecorder>,

    /// JWKS Token 验证器
    jwks_verifier: Option<JwksVerifier>,

    /// Privy 配置
    privy_config: Option<PrivyConfig>,

    /// 租户服务（用于创建默认租户）
    tenant_service: Option<std::sync::Arc<dyn TenantService>>,
}

impl AuthServiceImpl {
    /// 创建新的认证服务实例
    pub fn new<S: TenantConfigStore>(
        db_pool: Option<PgPool>,
        tenant_manager: TenantManager<S>,
    ) -> Self {
        Self {
            db_pool,
            audit_recorder: None,
            jwks_verifier: None,
            privy_config: None,
            tenant_service: Some(tenant_manager.tenant_storage()),
        }
    }

    /// 创建无数据库的认证服务实例（使用内存存储）
    pub fn new_in_memory<S: TenantConfigStore>(tenant_manager: TenantManager<S>) -> Self {
        Self {
            db_pool: None,
            audit_recorder: None,
            jwks_verifier: None,
            privy_config: None,
            tenant_service: Some(tenant_manager.tenant_storage()),
        }
    }

    /// 设置审计记录器
    pub fn with_audit_recorder(mut self, recorder: AuditRecorder) -> Self {
        self.audit_recorder = Some(recorder);
        self
    }

    /// 设置 Privy 配置
    pub fn with_privy_config(mut self, config: PrivyConfig) -> Self {
        self.privy_config = Some(config.clone());
        if !config.mock_enabled {
            self.jwks_verifier = Some(JwksVerifier::new(config));
        }
        self
    }

    /// 设置租户服务
    pub fn with_tenant_service(
        mut self,
        tenant_service: std::sync::Arc<dyn TenantService>,
    ) -> Self {
        self.tenant_service = Some(tenant_service);
        self
    }

    /// 生成安全随机 Token
    fn generate_secure_token() -> Result<String, AuthError> {
        use rand::RngCore;
        let mut bytes = [0u8; 32];
        rand::thread_rng()
            .try_fill_bytes(&mut bytes)
            .map_err(|e| AuthError::TokenGenerationError(e.to_string()))?;
        Ok(base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            bytes,
        ))
    }

    /// 计算 Token 哈希（SHA-256）
    fn hash_token(token: &str) -> String {
        use base64::Engine;
        let digest = ring::digest::digest(&ring::digest::SHA256, token.as_bytes());
        Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            digest.as_ref(),
        )
    }

    /// 验证 Token（常量时间比较）
    fn verify_token_hash(token: &str, expected_hash: &str) -> bool {
        let computed_hash = Self::hash_token(token);
        ct_compare(computed_hash.as_bytes(), expected_hash.as_bytes())
    }

    /// 根据本地外部身份映射判断是否需要首次登录初始化。
    fn should_initialize_default_tenant(existing_identity: Option<&ExternalIdentity>) -> bool {
        existing_identity.is_none()
    }

    /// 生成首次登录默认租户名。
    fn build_default_tenant_name(privy_response: &PrivyAuthResponse, user: &User) -> String {
        privy_response
            .email
            .clone()
            .or(user.display_name.clone())
            .unwrap_or_else(|| format!("Default Tenant for {}", user.id))
    }

    fn validate_display_name(display_name: Option<&str>) -> Result<(), AuthError> {
        if let Some(display_name) = display_name {
            if display_name.chars().count() > MAX_DISPLAY_NAME_CHARS {
                return Err(AuthError::InvalidRequest(
                    "display_name must be 128 characters or fewer".to_string(),
                ));
            }
        }

        Ok(())
    }

    fn require_pool(&self) -> Result<&PgPool, AuthError> {
        self.db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))
    }

    /// 将 MembershipRole 映射到 TokenScope 列表
    ///
    /// 提供本地授权映射，确保基于角色的权限控制继续生效。
    ///
    /// # 映射规则
    /// | Role | Scopes |
    /// |------|--------|
    /// | owner | admin, tenant:*, credential:*, audit:read, members:*, invitations:*, tokens:*, users:manage, roles:manage |
    /// | admin | tenant:read/write/admin, credential:*, audit:read, members:*, invitations:*, tokens:*, users:manage |
    /// | member | tenant:read, credential:read/write/decrypt, audit:read, tokens:read/write |
    /// | readonly | tenant:read, credential:read, tokens:read |
    pub fn role_to_scopes(role: MembershipRole) -> Vec<String> {
        use crate::api::middleware::TokenScope;
        TokenScope::from_role(role)
            .into_iter()
            .map(|s| s.as_str().to_string())
            .collect()
    }

    /// 验证 Privy Token 并获取用户信息
    ///
    /// 支持 Mock 模式和真实 JWKS 验证。
    async fn verify_privy_token(&self, token: &str) -> Result<PrivyAuthResponse, AuthError> {
        // [DIAGNOSTIC] 记录验证开始
        tracing::info!(
            target: "auth::privy",
            "[PRIVY VERIFY START] Starting token verification"
        );

        // 检查是否启用 Mock 模式
        if let Some(ref config) = self.privy_config {
            tracing::info!(
                target: "auth::privy",
                "[PRIVY CONFIG] mock_enabled={}, jwks_url={}",
                config.mock_enabled,
                config.jwks_url
            );
            if config.mock_enabled {
                tracing::info!(target: "auth::privy", "[PRIVY VERIFY] Using mock verification");
                return self.mock_verify_privy_token(token);
            }
        } else {
            tracing::warn!(target: "auth::privy", "[PRIVY CONFIG MISSING] privy_config is None");
        }

        // 真实 JWKS 验证
        tracing::info!(target: "auth::privy", "[PRIVY VERIFY] Using real JWKS verification");

        let verifier = self
            .jwks_verifier
            .as_ref()
            .ok_or_else(|| {
                tracing::error!(target: "auth::privy", "[PRIVY VERIFY FAILED] JWKS verifier not initialized");
                AuthError::ConfigError("JWKS verifier not initialized".to_string())
            })?;

        tracing::info!(target: "auth::privy", "[PRIVY VERIFY] Calling JWKS verifier...");

        let claims = match verifier.verify(token).await {
            Ok(c) => {
                tracing::info!(
                    target: "auth::privy",
                    "[PRIVY VERIFY SUCCESS] Token verified, did={}",
                    c.sub
                );
                c
            }
            Err(e) => {
                tracing::error!(
                    target: "auth::privy",
                    "[PRIVY VERIFY FAILED] JWKS verification error: {:?}",
                    e
                );
                return Err(e);
            }
        };

        Ok(PrivyAuthResponse {
            did: claims.sub,
            wallet_address: claims.custom.wallet_address,
            email: claims.custom.email,
            name: claims.custom.name,
            is_new_user: false, // 通过数据库查询判断
            profile: None,
        })
    }

    /// Mock 验证 Privy Token（用于开发和测试）
    fn mock_verify_privy_token(&self, _token: &str) -> Result<PrivyAuthResponse, AuthError> {
        // Mock 实现：返回测试用户数据
        Ok(PrivyAuthResponse {
            did: "did:privy:mock".to_string(),
            wallet_address: Some("0x1234567890abcdef".to_string()),
            email: Some("mock@example.com".to_string()),
            name: Some("Mock User".to_string()),
            is_new_user: false,
            profile: None,
        })
    }

    /// 创建用户记录
    async fn create_user_record(&self, user: &User) -> Result<User, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let row = sqlx::query_as::<_, User>(
            r#"
            INSERT INTO users (id, status, display_name, default_tenant_id, onboarding_completed, deleted_at, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, status, display_name, default_tenant_id, onboarding_completed, deleted_at, created_at, updated_at
            "#,
        )
        .bind(user.id)
        .bind(user.status.as_str())
        .bind(&user.display_name)
        .bind(user.default_tenant_id)
        .bind(user.onboarding_completed)
        .bind(user.deleted_at)
        .bind(user.created_at)
        .bind(user.updated_at)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db_err) = &e {
                if db_err.constraint() == Some("users_pkey") {
                    return AuthError::UserAlreadyExists { user_id: user.id };
                }
            }
            AuthError::DatabaseError(e)
        })?;

        Ok(row)
    }

    /// 创建外部身份记录
    async fn create_external_identity_record(
        &self,
        identity: &ExternalIdentity,
    ) -> Result<ExternalIdentity, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let row = sqlx::query_as::<_, ExternalIdentity>(
            r#"
            INSERT INTO external_identities (
                id, user_id, provider, provider_subject, wallet_address, email,
                provider_profile, is_verified, is_primary, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING id, user_id, provider, provider_subject, wallet_address, email,
                      provider_profile, is_verified, is_primary, mfa_verified, mfa_verified_at,
                      created_at, updated_at
            "#,
        )
        .bind(identity.id)
        .bind(identity.user_id)
        .bind(identity.provider.as_str())
        .bind(&identity.provider_subject)
        .bind(&identity.wallet_address)
        .bind(&identity.email)
        .bind(&identity.provider_profile)
        .bind(identity.is_verified)
        .bind(identity.is_primary)
        .bind(identity.created_at)
        .bind(identity.updated_at)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db_err) = &e {
                if db_err.constraint() == Some("external_identities_provider_provider_subject_key")
                {
                    return AuthError::ExternalIdentityAlreadyExists {
                        provider: identity.provider,
                        subject: identity.provider_subject.clone(),
                    };
                }
            }
            AuthError::DatabaseError(e)
        })?;

        Ok(row)
    }

    /// 将现有外部身份重新绑定到新的用户记录
    async fn rebind_external_identity_user(
        &self,
        identity_id: Uuid,
        user_id: Uuid,
    ) -> Result<ExternalIdentity, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let row = sqlx::query_as::<_, ExternalIdentity>(
            r#"
            UPDATE external_identities
            SET user_id = $2,
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, user_id, provider, provider_subject, wallet_address, email,
                      provider_profile, is_verified, is_primary, mfa_verified, mfa_verified_at,
                      created_at, updated_at
            "#,
        )
        .bind(identity_id)
        .bind(user_id)
        .fetch_one(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(row)
    }

    /// 创建成员资格记录
    async fn create_membership_record(
        &self,
        membership: &TenantMembership,
    ) -> Result<TenantMembership, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let scopes_json =
            serde_json::to_value(&membership.scopes).map_err(AuthError::SerializationError)?;

        let row = sqlx::query_as::<_, TenantMembership>(
            r#"
            INSERT INTO tenant_memberships (
                id, tenant_id, user_id, role, status, invited_by, joined_at,
                source, scopes, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING id, tenant_id, user_id, role, status, invited_by, joined_at,
                      source,
                      ARRAY(SELECT jsonb_array_elements_text(scopes)) AS scopes,
                      created_at, updated_at
            "#,
        )
        .bind(membership.id)
        .bind(membership.tenant_id)
        .bind(membership.user_id)
        .bind(membership.role.as_str())
        .bind(membership.status.as_str())
        .bind(membership.invited_by)
        .bind(membership.joined_at)
        .bind(membership.source.as_str())
        .bind(&scopes_json)
        .bind(membership.created_at)
        .bind(membership.updated_at)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db_err) = &e {
                if db_err.constraint() == Some("tenant_memberships_tenant_id_user_id_key") {
                    return AuthError::MembershipAlreadyExists {
                        user_id: membership.user_id,
                        tenant_id: membership.tenant_id,
                    };
                }
            }
            AuthError::DatabaseError(e)
        })?;

        Ok(row)
    }

    /// 创建邀请记录
    async fn create_invitation_record(
        &self,
        invitation: &TenantInvitation,
    ) -> Result<TenantInvitation, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let row = sqlx::query_as::<_, TenantInvitation>(
            r#"
            INSERT INTO tenant_invitations (
                id, tenant_id, role, invitee_type, invitee_email, invitee_wallet,
                token_hash, created_by, expires_at, consumed_at, consumed_by,
                status, max_uses, use_count, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
            RETURNING id, tenant_id, role, invitee_type, invitee_email, invitee_wallet,
                      token_hash, created_by, expires_at, consumed_at, consumed_by,
                      status, max_uses, use_count, created_at, updated_at
            "#,
        )
        .bind(invitation.id)
        .bind(invitation.tenant_id)
        .bind(invitation.role.as_str())
        .bind(invitation.invitee_type.as_str())
        .bind(&invitation.invitee_email)
        .bind(&invitation.invitee_wallet)
        .bind(&invitation.token_hash)
        .bind(invitation.created_by)
        .bind(invitation.expires_at)
        .bind(invitation.consumed_at)
        .bind(invitation.consumed_by)
        .bind(invitation.status.as_str())
        .bind(invitation.max_uses)
        .bind(invitation.use_count)
        .bind(invitation.created_at)
        .bind(invitation.updated_at)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db_err) = &e {
                if db_err.constraint() == Some("tenant_invitations_token_hash_key") {
                    return AuthError::InternalError("Invitation token hash collision".to_string());
                }
            }
            AuthError::DatabaseError(e)
        })?;

        Ok(row)
    }

    /// 创建会话记录
    async fn create_session_record(&self, session: &AuthSession) -> Result<AuthSession, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let row = sqlx::query_as::<_, AuthSession>(
            r#"
            INSERT INTO auth_sessions (
                id, user_id, session_token_hash, identity_id, active_membership_id,
                mfa_status, mfa_verified_at, user_agent, ip_address, expires_at,
                last_active_at, revoked_at, revoked_reason, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            RETURNING id, user_id, session_token_hash, identity_id, active_membership_id,
                      mfa_status, mfa_verified_at, user_agent, ip_address, expires_at,
                      last_active_at, revoked_at, revoked_reason, created_at, updated_at
            "#,
        )
        .bind(session.id)
        .bind(session.user_id)
        .bind(&session.session_token_hash)
        .bind(session.identity_id)
        .bind(session.active_membership_id)
        .bind(session.mfa_status.as_str())
        .bind(session.mfa_verified_at)
        .bind(&session.user_agent)
        .bind(&session.ip_address)
        .bind(session.expires_at)
        .bind(session.last_active_at)
        .bind(session.revoked_at)
        .bind(&session.revoked_reason)
        .bind(session.created_at)
        .bind(session.updated_at)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db_err) = &e {
                if db_err.constraint() == Some("auth_sessions_session_token_hash_key") {
                    return AuthError::InternalError("Session token hash collision".to_string());
                }
            }
            AuthError::DatabaseError(e)
        })?;

        Ok(row)
    }

    async fn create_service_account_record(
        &self,
        service_account: &ServiceAccount,
    ) -> Result<ServiceAccount, AuthError> {
        let pool = self.require_pool()?;
        let scope_ceiling = serde_json::to_value(&service_account.scope_ceiling)
            .map_err(AuthError::SerializationError)?;

        let row = sqlx::query_as::<_, ServiceAccount>(
            r#"
            INSERT INTO service_accounts (
                id, tenant_id, name, description, role, scope_ceiling, status,
                created_by, created_at, updated_at, deleted_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING id, tenant_id, name, description, role,
                      ARRAY(SELECT jsonb_array_elements_text(scope_ceiling)) AS scope_ceiling,
                      status, created_by, created_at, updated_at, deleted_at
            "#,
        )
        .bind(service_account.id)
        .bind(service_account.tenant_id)
        .bind(&service_account.name)
        .bind(&service_account.description)
        .bind(&service_account.role)
        .bind(&scope_ceiling)
        .bind(service_account.status.as_str())
        .bind(service_account.created_by)
        .bind(service_account.created_at)
        .bind(service_account.updated_at)
        .bind(service_account.deleted_at)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db_err) = &e {
                if db_err.constraint() == Some("uq_service_accounts_tenant_name") {
                    return AuthError::ServiceAccountAlreadyExists {
                        tenant_id: service_account.tenant_id,
                        name: service_account.name.clone(),
                    };
                }
            }
            AuthError::DatabaseError(e)
        })?;

        Ok(row)
    }

    async fn list_service_account_records(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ServiceAccount>, AuthError> {
        let pool = self.require_pool()?;
        let rows = sqlx::query_as::<_, ServiceAccount>(
            r#"
            SELECT id, tenant_id, name, description, role,
                   ARRAY(SELECT jsonb_array_elements_text(scope_ceiling)) AS scope_ceiling,
                   status, created_by, created_at, updated_at, deleted_at
            FROM service_accounts
            WHERE tenant_id = $1 AND deleted_at IS NULL
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(rows)
    }

    async fn query_service_account(
        &self,
        service_account_id: Uuid,
    ) -> Result<Option<ServiceAccount>, AuthError> {
        let pool = self.require_pool()?;
        let row = sqlx::query_as::<_, ServiceAccount>(
            r#"
            SELECT id, tenant_id, name, description, role,
                   ARRAY(SELECT jsonb_array_elements_text(scope_ceiling)) AS scope_ceiling,
                   status, created_by, created_at, updated_at, deleted_at
            FROM service_accounts
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(service_account_id)
        .fetch_optional(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(row)
    }

    async fn update_service_account_record(
        &self,
        service_account: &ServiceAccount,
    ) -> Result<ServiceAccount, AuthError> {
        let pool = self.require_pool()?;
        let scope_ceiling = serde_json::to_value(&service_account.scope_ceiling)
            .map_err(AuthError::SerializationError)?;

        let row = sqlx::query_as::<_, ServiceAccount>(
            r#"
            UPDATE service_accounts
            SET name = $2,
                description = $3,
                role = $4,
                scope_ceiling = $5,
                status = $6,
                updated_at = $7,
                deleted_at = $8
            WHERE id = $1
            RETURNING id, tenant_id, name, description, role,
                      ARRAY(SELECT jsonb_array_elements_text(scope_ceiling)) AS scope_ceiling,
                      status, created_by, created_at, updated_at, deleted_at
            "#,
        )
        .bind(service_account.id)
        .bind(&service_account.name)
        .bind(&service_account.description)
        .bind(&service_account.role)
        .bind(&scope_ceiling)
        .bind(service_account.status.as_str())
        .bind(service_account.updated_at)
        .bind(service_account.deleted_at)
        .fetch_optional(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        row.ok_or(AuthError::ServiceAccountNotFound(service_account.id))
    }

    async fn create_api_token_metadata_record(
        &self,
        metadata: &ApiTokenMetadata,
    ) -> Result<ApiTokenMetadata, AuthError> {
        let pool = self.require_pool()?;
        let scopes =
            serde_json::to_value(&metadata.scopes).map_err(AuthError::SerializationError)?;

        let row = sqlx::query_as::<_, ApiTokenMetadata>(
            r#"
            INSERT INTO api_tokens (
                id, token_kind, token_type, subject_type, subject_id, tenant_id, issued_from,
                session_id, membership_id, token_name, token_prefix, display_name, description,
                scopes, issued_membership_role_snapshot, permission_source, created_via,
                revoked_reason, oauth_client_id, oauth_grant_type, oauth_subject_mode,
                expires_at, revoked_at, created_at, last_used_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17,
                    $18, $19, $20, $21, $22, $23, $24, $25)
            RETURNING id, token_kind, token_type, subject_type, subject_id, tenant_id, issued_from,
                      session_id, membership_id, token_name, token_prefix, display_name, description,
                      ARRAY(SELECT jsonb_array_elements_text(scopes)) AS scopes,
                      issued_membership_role_snapshot, permission_source, created_via,
                      revoked_reason, oauth_client_id, oauth_grant_type, oauth_subject_mode,
                      expires_at, revoked_at, created_at, last_used_at
            "#,
        )
        .bind(&metadata.id)
        .bind(&metadata.token_kind)
        .bind(metadata.token_type.as_str())
        .bind(metadata.subject_type.as_str())
        .bind(metadata.subject_id)
        .bind(metadata.tenant_id)
        .bind(&metadata.issued_from)
        .bind(metadata.session_id)
        .bind(metadata.membership_id)
        .bind(&metadata.token_name)
        .bind(&metadata.token_prefix)
        .bind(&metadata.display_name)
        .bind(&metadata.description)
        .bind(&scopes)
        .bind(&metadata.issued_membership_role_snapshot)
        .bind(&metadata.permission_source)
        .bind(&metadata.created_via)
        .bind(&metadata.revoked_reason)
        .bind(&metadata.oauth_client_id)
        .bind(&metadata.oauth_grant_type)
        .bind(&metadata.oauth_subject_mode)
        .bind(metadata.expires_at)
        .bind(metadata.revoked_at)
        .bind(metadata.created_at)
        .bind(metadata.last_used_at)
        .fetch_one(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(row)
    }

    async fn list_api_token_metadata_records(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ApiTokenMetadata>, AuthError> {
        let pool = self.require_pool()?;
        let rows = sqlx::query_as::<_, ApiTokenMetadata>(
            r#"
            SELECT id, token_kind, token_type, subject_type, subject_id, tenant_id, issued_from,
                   session_id, membership_id, token_name, token_prefix, display_name, description,
                   COALESCE(ARRAY(SELECT jsonb_array_elements_text(scopes)), ARRAY[]::text[]) AS scopes,
                   issued_membership_role_snapshot, permission_source, created_via,
                   revoked_reason, oauth_client_id, oauth_grant_type, oauth_subject_mode,
                   expires_at, revoked_at, created_at, last_used_at
            FROM api_tokens
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(rows)
    }

    async fn list_api_token_metadata_for_service_account(
        &self,
        tenant_id: Uuid,
        service_account_id: Uuid,
    ) -> Result<Vec<ApiTokenMetadata>, AuthError> {
        let pool = self.require_pool()?;
        let rows = sqlx::query_as::<_, ApiTokenMetadata>(
            r#"
            SELECT id, token_kind, token_type, subject_type, subject_id, tenant_id, issued_from,
                   session_id, membership_id, token_name, token_prefix, display_name, description,
                   COALESCE(ARRAY(SELECT jsonb_array_elements_text(scopes)), ARRAY[]::text[]) AS scopes,
                   issued_membership_role_snapshot, permission_source, created_via,
                   revoked_reason, oauth_client_id, oauth_grant_type, oauth_subject_mode,
                   expires_at, revoked_at, created_at, last_used_at
            FROM api_tokens
            WHERE tenant_id = $1
              AND subject_id = $2
              AND subject_type = 'service_account'
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .bind(service_account_id)
        .fetch_all(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(rows)
    }

    async fn query_api_token_metadata(
        &self,
        token_id: &str,
    ) -> Result<Option<ApiTokenMetadata>, AuthError> {
        let pool = self.require_pool()?;
        let row = sqlx::query_as::<_, ApiTokenMetadata>(
            r#"
            SELECT id, token_kind, token_type, subject_type, subject_id, tenant_id, issued_from,
                   session_id, membership_id, token_name, token_prefix, display_name, description,
                   COALESCE(ARRAY(SELECT jsonb_array_elements_text(scopes)), ARRAY[]::text[]) AS scopes,
                   issued_membership_role_snapshot, permission_source, created_via,
                   revoked_reason, oauth_client_id, oauth_grant_type, oauth_subject_mode,
                   expires_at, revoked_at, created_at, last_used_at
            FROM api_tokens
            WHERE id = $1
            "#,
        )
        .bind(token_id)
        .fetch_optional(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(row)
    }

    async fn revoke_api_token_metadata_record(
        &self,
        token_id: &str,
        revoked_at: DateTime<Utc>,
    ) -> Result<Option<ApiTokenMetadata>, AuthError> {
        let pool = self.require_pool()?;
        let row = sqlx::query_as::<_, ApiTokenMetadata>(
            r#"
            UPDATE api_tokens
            SET revoked_at = $2
            WHERE id = $1
            RETURNING id, token_kind, token_type, subject_type, subject_id, tenant_id, issued_from,
                      session_id, membership_id, token_name, token_prefix, display_name, description,
                      ARRAY(SELECT jsonb_array_elements_text(scopes)) AS scopes,
                      issued_membership_role_snapshot, permission_source, created_via,
                      revoked_reason, oauth_client_id, oauth_grant_type, oauth_subject_mode,
                      expires_at, revoked_at, created_at, last_used_at
            "#,
        )
        .bind(token_id)
        .bind(revoked_at)
        .fetch_optional(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(row)
    }

    async fn mark_api_token_used_record(
        &self,
        token_id: &str,
        last_used_at: DateTime<Utc>,
    ) -> Result<Option<ApiTokenMetadata>, AuthError> {
        let pool = self.require_pool()?;
        let row = sqlx::query_as::<_, ApiTokenMetadata>(
            r#"
            UPDATE api_tokens
            SET last_used_at = $2
            WHERE id = $1
            RETURNING id, token_kind, token_type, subject_type, subject_id, tenant_id, issued_from,
                      session_id, membership_id, token_name, token_prefix, display_name, description,
                      ARRAY(SELECT jsonb_array_elements_text(scopes)) AS scopes,
                      issued_membership_role_snapshot, permission_source, created_via,
                      revoked_reason, oauth_client_id, oauth_grant_type, oauth_subject_mode,
                      expires_at, revoked_at, created_at, last_used_at
            "#,
        )
        .bind(token_id)
        .bind(last_used_at)
        .fetch_optional(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(row)
    }

    /// 创建审计日志记录
    async fn create_audit_log_record(&self, log: &AuthAuditLog) -> Result<(), AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let event_data =
            serde_json::to_value(&log.details).map_err(AuthError::SerializationError)?;

        let severity = if log.success { "info" } else { "warning" };

        sqlx::query(
            r#"
            INSERT INTO auth_audit_logs (
                id, event_type, severity, user_id, identity_id, tenant_id,
                membership_id, invitation_id, session_id, event_data,
                ip_address, user_agent, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(&log.id)
        .bind(&log.event_type.as_str())
        .bind(&severity)
        .bind(&log.user_id)
        .bind(&log.identity_id)
        .bind(&log.tenant_id)
        .bind(&log.membership_id)
        .bind(&log.invitation_id)
        .bind(&log.session_id)
        .bind(&event_data)
        .bind(&log.ip_address)
        .bind(&log.user_agent)
        .bind(&log.created_at)
        .execute(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(())
    }

    /// 查询用户
    async fn query_user(&self, user_id: Uuid) -> Result<Option<User>, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let row = sqlx::query_as::<_, User>(
            r#"
            SELECT id, status, display_name, default_tenant_id, onboarding_completed,
                   deleted_at, created_at, updated_at
            FROM users
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(&user_id)
        .fetch_optional(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(row)
    }

    /// 查询外部身份
    async fn query_external_identity(
        &self,
        provider: IdentityProvider,
        subject: &str,
    ) -> Result<Option<ExternalIdentity>, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let row = sqlx::query_as::<_, ExternalIdentity>(
            r#"
            SELECT id, user_id, provider, provider_subject, wallet_address, email,
                   provider_profile, is_verified, is_primary, mfa_verified, mfa_verified_at,
                   created_at, updated_at
            FROM external_identities
            WHERE provider = $1 AND provider_subject = $2
            "#,
        )
        .bind(&provider.as_str())
        .bind(&subject)
        .fetch_optional(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(row)
    }

    /// 查询邀请
    #[allow(dead_code)]
    async fn query_invitation(
        &self,
        invitation_id: Uuid,
    ) -> Result<Option<TenantInvitation>, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let row = sqlx::query_as::<_, TenantInvitation>(
            r#"
            SELECT id, tenant_id, role, invitee_type, invitee_email, invitee_wallet,
                   token_hash, created_by, expires_at, consumed_at, consumed_by,
                   status, max_uses, use_count, created_at, updated_at
            FROM tenant_invitations
            WHERE id = $1
            "#,
        )
        .bind(&invitation_id)
        .fetch_optional(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(row)
    }

    /// 查询会话
    async fn query_session(&self, session_id: Uuid) -> Result<Option<AuthSession>, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let row = sqlx::query_as::<_, AuthSession>(
            r#"
            SELECT id, user_id, session_token_hash, identity_id, active_membership_id,
                   mfa_status, mfa_verified_at, user_agent, ip_address, expires_at,
                   last_active_at, revoked_at, revoked_reason, created_at, updated_at
            FROM auth_sessions
            WHERE id = $1
            "#,
        )
        .bind(&session_id)
        .fetch_optional(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(row)
    }

    async fn query_user_identities(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<ExternalIdentity>, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let rows = sqlx::query_as::<_, ExternalIdentity>(
            r#"
            SELECT id, user_id, provider, provider_subject, wallet_address, email,
                   provider_profile, is_verified, is_primary, mfa_verified, mfa_verified_at,
                   created_at, updated_at
            FROM external_identities
            WHERE user_id = $1
            ORDER BY is_primary DESC, created_at ASC
            "#,
        )
        .bind(&user_id)
        .fetch_all(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(rows)
    }

    /// 查询用户的所有成员资格
    async fn query_user_memberships(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<TenantMembership>, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let rows = sqlx::query_as::<_, TenantMembership>(
            r#"
            SELECT id, tenant_id, user_id, role, status, invited_by, joined_at,
                   source,
                   ARRAY(SELECT jsonb_array_elements_text(scopes)) AS scopes,
                   created_at, updated_at
            FROM tenant_memberships
            WHERE user_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(&user_id)
        .fetch_all(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(rows)
    }

    async fn query_tenant_memberships(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<TenantMembership>, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let rows = sqlx::query_as::<_, TenantMembership>(
            r#"
            SELECT id, tenant_id, user_id, role, status, invited_by, joined_at,
                   source,
                   ARRAY(SELECT jsonb_array_elements_text(scopes)) AS scopes,
                   created_at, updated_at
            FROM tenant_memberships
            WHERE tenant_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(&tenant_id)
        .fetch_all(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(rows)
    }

    /// 查询活跃成员资格
    async fn query_active_membership(
        &self,
        user_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Option<TenantMembership>, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let row = sqlx::query_as::<_, TenantMembership>(
            r#"
            SELECT id, tenant_id, user_id, role, status, invited_by, joined_at,
                   source,
                   ARRAY(SELECT jsonb_array_elements_text(scopes)) AS scopes,
                   created_at, updated_at
            FROM tenant_memberships
            WHERE user_id = $1 AND tenant_id = $2 AND status = 'active'
            "#,
        )
        .bind(&user_id)
        .bind(&tenant_id)
        .fetch_optional(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(row)
    }

    async fn query_membership_by_id(
        &self,
        membership_id: Uuid,
    ) -> Result<Option<TenantMembership>, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let row = sqlx::query_as::<_, TenantMembership>(
            r#"
            SELECT id, tenant_id, user_id, role, status, invited_by, joined_at,
                   source,
                   ARRAY(SELECT jsonb_array_elements_text(scopes)) AS scopes,
                   created_at, updated_at
            FROM tenant_memberships
            WHERE id = $1
            "#,
        )
        .bind(&membership_id)
        .fetch_optional(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(row)
    }

    /// 根据 Token 哈希查询邀请
    async fn query_invitation_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<TenantInvitation>, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let row = sqlx::query_as::<_, TenantInvitation>(
            r#"
            SELECT id, tenant_id, role, invitee_type, invitee_email, invitee_wallet,
                   token_hash, created_by, expires_at, consumed_at, consumed_by,
                   status, max_uses, use_count, created_at, updated_at
            FROM tenant_invitations
            WHERE token_hash = $1
            "#,
        )
        .bind(token_hash)
        .fetch_optional(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(row)
    }

    async fn query_tenant_invitations(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<TenantInvitation>, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let rows = sqlx::query_as::<_, TenantInvitation>(
            r#"
            SELECT id, tenant_id, role, invitee_type, invitee_email, invitee_wallet,
                   token_hash, created_by, expires_at, consumed_at, consumed_by,
                   status, max_uses, use_count, created_at, updated_at
            FROM tenant_invitations
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(&tenant_id)
        .fetch_all(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(rows)
    }

    async fn find_active_invitation_for_invitee(
        &self,
        tenant_id: Uuid,
        invitee_type: InviteeType,
        invitee_email: Option<&str>,
        invitee_wallet: Option<&str>,
    ) -> Result<Option<TenantInvitation>, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let row = sqlx::query_as::<_, TenantInvitation>(
            r#"
            SELECT id, tenant_id, role, invitee_type, invitee_email, invitee_wallet,
                   token_hash, created_by, expires_at, consumed_at, consumed_by,
                   status, max_uses, use_count, created_at, updated_at
            FROM tenant_invitations
            WHERE tenant_id = $1
              AND invitee_type = $2
              AND status = 'pending'
              AND expires_at > NOW()
              AND (
                    ($2 = 'email' AND invitee_email = $3)
                 OR ($2 = 'wallet' AND invitee_wallet = $4)
                 OR ($2 = 'any')
              )
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(invitee_type.as_str())
        .bind(invitee_email)
        .bind(invitee_wallet)
        .fetch_optional(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(row)
    }

    /// 根据 Token 哈希查询会话
    async fn query_session_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<AuthSession>, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let row = sqlx::query_as::<_, AuthSession>(
            r#"
            SELECT id, user_id, session_token_hash, identity_id, active_membership_id,
                   mfa_status, mfa_verified_at, user_agent, ip_address, expires_at,
                   last_active_at, revoked_at, revoked_reason, created_at, updated_at
            FROM auth_sessions
            WHERE session_token_hash = $1
            "#,
        )
        .bind(token_hash)
        .fetch_optional(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(row)
    }

    /// 更新邀请消费状态
    async fn update_invitation_consumed(
        &self,
        invitation_id: Uuid,
        consumed_by: Uuid,
    ) -> Result<(), AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        sqlx::query(
            r#"
            UPDATE tenant_invitations
            SET use_count = use_count + 1,
                consumed_at = NOW(),
                consumed_by = $2,
                status = CASE WHEN use_count + 1 >= max_uses THEN 'consumed' ELSE status END,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(invitation_id)
        .bind(consumed_by)
        .execute(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(())
    }

    async fn update_user_record(
        &self,
        user_id: Uuid,
        display_name: Option<String>,
        default_tenant_id: Option<Option<Uuid>>,
        onboarding_completed: Option<bool>,
        deleted_at: Option<Option<chrono::DateTime<chrono::Utc>>>,
    ) -> Result<User, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let current_user = self
            .query_user(user_id)
            .await?
            .ok_or(AuthError::UserNotFound(user_id))?;

        let next_display_name = display_name.or(current_user.display_name);
        let next_default_tenant_id = default_tenant_id.unwrap_or(current_user.default_tenant_id);
        let next_onboarding_completed =
            onboarding_completed.unwrap_or(current_user.onboarding_completed);
        let next_deleted_at = deleted_at.unwrap_or(current_user.deleted_at);
        let next_status = if next_deleted_at.is_some() {
            super::models::UserStatus::PendingDeletion
        } else {
            current_user.status
        };
        let now = chrono::Utc::now();

        let row = sqlx::query_as::<_, User>(
            r#"
            UPDATE users
            SET status = $2,
                display_name = $3,
                default_tenant_id = $4,
                onboarding_completed = $5,
                deleted_at = $6,
                updated_at = $7
            WHERE id = $1
            RETURNING id, status, display_name, default_tenant_id, onboarding_completed, deleted_at, created_at, updated_at
            "#,
        )
        .bind(user_id)
        .bind(next_status.as_str())
        .bind(&next_display_name)
        .bind(next_default_tenant_id)
        .bind(next_onboarding_completed)
        .bind(next_deleted_at)
        .bind(now)
        .fetch_one(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(row)
    }

    async fn update_membership_record(
        &self,
        membership: &TenantMembership,
    ) -> Result<TenantMembership, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let row = sqlx::query_as::<_, TenantMembership>(
            r#"
            UPDATE tenant_memberships
            SET role = $2,
                status = $3,
                joined_at = $4,
                scopes = $5,
                updated_at = $6
            WHERE id = $1
            RETURNING id, tenant_id, user_id, role, status, invited_by, joined_at,
                      source,
                      ARRAY(SELECT jsonb_array_elements_text(scopes)) AS scopes,
                      created_at, updated_at
            "#,
        )
        .bind(membership.id)
        .bind(membership.role.as_str())
        .bind(membership.status.as_str())
        .bind(membership.joined_at)
        .bind(&membership.scopes)
        .bind(membership.updated_at)
        .fetch_one(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(row)
    }

    async fn mark_membership_inactive(&self, membership_id: Uuid) -> Result<(), AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        sqlx::query(
            r#"
            UPDATE tenant_memberships
            SET status = 'inactive',
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(membership_id)
        .execute(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(())
    }

    async fn revoke_invitation_record(
        &self,
        invitation_id: Uuid,
    ) -> Result<TenantInvitation, AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        let row = sqlx::query_as::<_, TenantInvitation>(
            r#"
            UPDATE tenant_invitations
            SET status = 'revoked',
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, tenant_id, role, invitee_type, invitee_email, invitee_wallet,
                      token_hash, created_by, expires_at, consumed_at, consumed_by,
                      status, max_uses, use_count, created_at, updated_at
            "#,
        )
        .bind(invitation_id)
        .fetch_one(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(row)
    }

    /// 更新会话撤销状态
    async fn update_session_revoked(
        &self,
        session_id: Uuid,
        reason: &str,
    ) -> Result<(), AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        sqlx::query(
            r#"
            UPDATE auth_sessions
            SET revoked_at = NOW(),
                revoked_reason = $2,
                updated_at = NOW()
            WHERE id = $1 AND revoked_at IS NULL
            "#,
        )
        .bind(session_id)
        .bind(reason)
        .execute(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(())
    }

    /// 更新身份 MFA 状态
    async fn update_identity_mfa_status(
        &self,
        identity_id: Uuid,
        mfa_verified: bool,
    ) -> Result<(), AuthError> {
        let pool = self
            .db_pool
            .as_ref()
            .ok_or_else(|| AuthError::InternalError("Database pool not initialized".to_string()))?;

        sqlx::query(
            r#"
            UPDATE external_identities
            SET mfa_verified = $2,
                mfa_verified_at = CASE WHEN $2 THEN NOW() ELSE mfa_verified_at END,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(identity_id)
        .bind(mfa_verified)
        .execute(pool)
        .await
        .map_err(AuthError::DatabaseError)?;

        Ok(())
    }

    /// 从 Privy API 获取 MFA 状态
    ///
    /// 调用 Privy API 获取用户的 MFA 配置状态。
    async fn fetch_privy_mfa_status(
        &self,
        privy_token: &str,
    ) -> Result<MfaStatusSnapshot, AuthError> {
        // 检查是否启用 Mock 模式
        if let Some(ref config) = self.privy_config {
            if config.mock_enabled {
                return Ok(MfaStatusSnapshot::default());
            }
        }

        // 获取 Privy 配置
        let config = self
            .privy_config
            .as_ref()
            .ok_or_else(|| AuthError::ConfigError("Privy config not initialized".to_string()))?;

        // 调用 Privy API 获取 MFA 状态
        let client = reqwest::Client::new();
        let response = client
            .get(format!("{}/users/me/mfa", config.api_url))
            .header("Authorization", format!("Bearer {privy_token}"))
            .header("privy-app-id", &config.app_id)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| AuthError::PrivyApiError {
                status: 0,
                message: format!("Failed to fetch MFA status: {e}"),
            })?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AuthError::PrivyApiError {
                status,
                message: text,
            });
        }

        // 解析 MFA 状态响应
        let mfa_info: serde_json::Value =
            response
                .json()
                .await
                .map_err(|e| AuthError::PrivyApiError {
                    status: 0,
                    message: format!("Failed to parse MFA response: {e}"),
                })?;

        let enabled = mfa_info
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let verified = mfa_info
            .get("verified")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        Ok(MfaStatusSnapshot {
            enabled,
            verified,
            requires_step_up: false,
            last_verified_at: None,
            synced_at: chrono::Utc::now().to_rfc3339(),
        })
    }
}

#[async_trait]
impl AuthService for AuthServiceImpl {
    async fn create_user_from_privy(&self, privy_token: &str) -> Result<User, AuthError> {
        // 1. 验证 Privy Token
        let privy_response = self.verify_privy_token(privy_token).await?;

        // 2. 查找或创建外部身份
        let existing_identity = self
            .query_external_identity(IdentityProvider::Privy, &privy_response.did)
            .await?;
        let is_first_login =
            AuthServiceImpl::should_initialize_default_tenant(existing_identity.as_ref());

        // 3. 如果外部身份已存在，返回关联用户
        if let Some(identity) = existing_identity {
            if let Some(user) = self.query_user(identity.user_id).await? {
                return Ok(user);
            }

            tracing::warn!(
                target: "auth::session",
                identity_id = %identity.id,
                missing_user_id = %identity.user_id,
                provider = %identity.provider.as_str(),
                subject = %identity.provider_subject,
                "External identity points to a missing user; recreating user and rebinding identity"
            );

            let mut recreated_user = User::new();
            if let Some(name) = &privy_response.name {
                recreated_user.display_name = Some(name.clone());
            }

            let recreated_user = self.create_user_record(&recreated_user).await?;
            self.rebind_external_identity_user(identity.id, recreated_user.id)
                .await?;

            self.audit_log(
                AuthEventType::UserCreated,
                Some(recreated_user.id),
                Some(serde_json::json!({
                    "provider": "privy",
                    "did": privy_response.did,
                    "is_new_user": false,
                    "recovered_identity_id": identity.id,
                    "recovered_missing_user_id": identity.user_id,
                })),
            )
            .await?;

            return Ok(recreated_user);
        }

        // 4. 创建新用户
        let mut user = User::new();
        if let Some(name) = &privy_response.name {
            user.display_name = Some(name.clone());
        }

        // 5. 保存用户记录
        self.create_user_record(&user).await?;

        // 6. 创建外部身份绑定
        let mut identity =
            ExternalIdentity::new(user.id, IdentityProvider::Privy, &privy_response.did);

        if let Some(wallet) = &privy_response.wallet_address {
            identity.wallet_address = Some(wallet.clone());
        }
        if let Some(email) = &privy_response.email {
            identity.email = Some(email.clone());
        }
        if let Some(profile) = &privy_response.profile {
            identity.provider_profile = Some(profile.clone());
        }

        identity.verify();
        identity.set_primary();

        self.create_external_identity_record(&identity).await?;

        // 7. 为首次登录用户创建默认租户和 Owner membership
        let mut default_tenant_created = false;
        if is_first_login {
            let tenant_service = self.tenant_service.as_ref().ok_or_else(|| {
                AuthError::InternalError(
                    "Tenant service unavailable for first-login initialization".to_string(),
                )
            })?;

            let tenant_name = AuthServiceImpl::build_default_tenant_name(&privy_response, &user);

            // 创建租户请求（不设置 owner_user_id，稍后手动创建 membership）
            let create_request = crate::tenant::service::CreateTenantRequest::new(&tenant_name)
                .without_owner_binding()
                .with_tier("free");

            // 创建租户
            let tenant_result = tenant_service
                .create_tenant(create_request, Some(user.id.to_string()))
                .await
                .map_err(|e| {
                    AuthError::InternalError(format!("Failed to create default tenant: {e}"))
                })?;

            // 更新用户的 default_tenant_id
            let tenant_id_uuid = uuid::Uuid::parse_str(tenant_result.tenant.id.as_str())
                .map_err(|e| AuthError::InternalError(format!("Invalid tenant ID: {e}")))?;

            self.create_membership_record(&TenantMembership::new_owner(tenant_id_uuid, user.id))
                .await?;

            user = self
                .update_user_record(user.id, None, Some(Some(tenant_id_uuid)), None, None)
                .await?;

            default_tenant_created = true;
            tracing::info!(
                target: "auth::session",
                user_id = %user.id,
                tenant_id = %tenant_result.tenant.id,
                "Created default tenant and Owner membership for first-login user"
            );
        }

        // 8. 记录审计日志
        self.audit_log(
            AuthEventType::UserCreated,
            Some(user.id),
            Some(serde_json::json!({
                "provider": "privy",
                "did": privy_response.did,
                "is_new_user": is_first_login,
                "default_tenant_created": default_tenant_created,
            })),
        )
        .await?;

        Ok(user)
    }

    async fn get_or_create_external_identity(
        &self,
        user_id: Uuid,
        provider: IdentityProvider,
        provider_subject: &str,
        profile: Option<JsonValue>,
    ) -> Result<ExternalIdentity, AuthError> {
        // 1. 查找现有身份
        let existing = self
            .query_external_identity(provider, provider_subject)
            .await?;

        if let Some(identity) = existing {
            // 2. 如果存在，更新 Profile（如果提供）
            if profile.is_some() {
                // TODO: 实现 Profile 更新
            }
            return Ok(identity);
        }

        // 3. 验证用户存在
        let user = self
            .query_user(user_id)
            .await?
            .ok_or(AuthError::UserNotFound(user_id))?;

        // 4. 创建新身份
        let mut identity = ExternalIdentity::new(user.id, provider, provider_subject);
        if let Some(profile) = profile {
            identity = identity.with_profile(profile);
        }

        // 5. 保存身份记录
        self.create_external_identity_record(&identity).await?;

        // 6. 记录审计日志
        self.audit_log(
            AuthEventType::IdentityLinked,
            Some(user.id),
            Some(serde_json::json!({
                "provider": provider.as_str(),
                "subject": provider_subject,
            })),
        )
        .await?;

        Ok(identity)
    }

    async fn create_tenant_invitation(
        &self,
        tenant_id: Uuid,
        role: MembershipRole,
        invitee_type: InviteeType,
        invitee_email: Option<String>,
        invitee_wallet: Option<String>,
        created_by: Uuid,
        expires_hours: i64,
    ) -> Result<(TenantInvitation, String), AuthError> {
        // 1. 验证创建者权限
        let creator_membership = self
            .get_active_membership(created_by, tenant_id)
            .await?
            .ok_or(AuthError::MembershipNotFound {
                user_id: created_by,
                tenant_id,
            })?;

        if !creator_membership.can_manage() {
            return Err(AuthError::InsufficientPermissions {
                required: "admin".to_string(),
                current: creator_membership.role.to_string(),
            });
        }

        let normalized_email = invitee_email
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase());
        let normalized_wallet = invitee_wallet
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase());

        if let Some(existing) = self
            .find_active_invitation_for_invitee(
                tenant_id,
                invitee_type,
                normalized_email.as_deref(),
                normalized_wallet.as_deref(),
            )
            .await?
        {
            let invitee = normalized_email
                .clone()
                .or_else(|| normalized_wallet.clone())
                .unwrap_or_else(|| existing.id.to_string());
            return Err(AuthError::DuplicatePendingInvitation { tenant_id, invitee });
        }

        // 2. 生成邀请 Token
        let invitation_token = Self::generate_secure_token()?;
        let token_hash = Self::hash_token(&invitation_token);

        // 3. 创建邀请实体
        let invitation = match invitee_type {
            InviteeType::Email => TenantInvitation::new_email_invitation(
                tenant_id,
                role,
                normalized_email.unwrap_or_default(),
                created_by,
                expires_hours,
            ),
            InviteeType::Wallet => TenantInvitation::new_wallet_invitation(
                tenant_id,
                role,
                normalized_wallet.unwrap_or_default(),
                created_by,
                expires_hours,
            ),
            InviteeType::Any => {
                TenantInvitation::new_open_invitation(tenant_id, role, created_by, expires_hours, 1)
            }
        }
        .with_token_hash(token_hash);

        // 4. 保存邀请记录
        self.create_invitation_record(&invitation).await?;

        // 5. 记录审计日志
        self.audit_log(
            AuthEventType::InvitationCreated,
            Some(created_by),
            Some(serde_json::json!({
                "invitation_id": invitation.id,
                "tenant_id": tenant_id,
                "role": role.as_str(),
                "invitee_type": invitee_type.as_str(),
            })),
        )
        .await?;

        Ok((invitation, invitation_token))
    }

    async fn consume_invitation(
        &self,
        invitation_token: &str,
        user_id: Uuid,
    ) -> Result<TenantMembership, AuthError> {
        // 1. 计算 Token 哈希并查找邀请
        let token_hash = Self::hash_token(invitation_token);

        let invitation = self
            .query_invitation_by_token_hash(&token_hash)
            .await?
            .ok_or(AuthError::InvalidInvitationToken)?;

        // 2. 验证邀请有效性
        if !invitation.is_valid() {
            if invitation.status == InvitationStatus::Expired {
                return Err(AuthError::InvitationExpired(invitation.id));
            }
            if invitation.status == InvitationStatus::Consumed {
                return Err(AuthError::InvitationAlreadyConsumed(invitation.id));
            }
            if invitation.status == InvitationStatus::Revoked {
                return Err(AuthError::InvitationRevoked(invitation.id));
            }
            return Err(AuthError::InvalidInvitationToken);
        }

        // 3. 验证 Token 哈希匹配
        if !Self::verify_token_hash(invitation_token, &invitation.token_hash) {
            return Err(AuthError::InvalidInvitationToken);
        }

        // 4. 创建成员资格
        let mut membership = TenantMembership::new_from_invitation(
            invitation.tenant_id,
            user_id,
            invitation.role,
            invitation.created_by,
        );

        // 5. 接受邀请（激活成员资格）
        membership.accept_invitation();

        // 6. 保存成员资格记录
        self.create_membership_record(&membership).await?;

        // 7. 更新邀请状态
        self.update_invitation_consumed(invitation.id, user_id)
            .await?;

        // 8. 记录审计日志
        self.audit_log(
            AuthEventType::InvitationAccepted,
            Some(user_id),
            Some(serde_json::json!({
                "invitation_id": invitation.id,
                "tenant_id": invitation.tenant_id,
                "role": invitation.role.as_str(),
            })),
        )
        .await?;

        Ok(membership)
    }

    async fn create_session(
        &self,
        user_id: Uuid,
        identity_id: Option<Uuid>,
        request: CreateUserRequest,
    ) -> Result<(AuthSession, String), AuthError> {
        // 1. 验证用户存在且活跃
        let user = self
            .query_user(user_id)
            .await?
            .ok_or(AuthError::UserNotFound(user_id))?;

        if !user.is_active() {
            return Err(AuthError::InvalidUserStatus {
                status: user.status.to_string(),
            });
        }

        // 2. 生成会话 Token
        let session_token = Self::generate_secure_token()?;
        let session_token_hash = Self::hash_token(&session_token);

        // 3. 计算过期时间
        let ttl = request
            .display_name
            .map(|_| DEFAULT_SESSION_TTL)
            .unwrap_or(DEFAULT_SESSION_TTL);

        // 4. 创建会话实体
        let mut session = AuthSession::new(user_id, session_token_hash, ttl);

        if let Some(identity_id) = identity_id {
            session = session.with_identity(identity_id);
        }

        // 5. 保存会话记录
        self.create_session_record(&session).await?;

        // 6. 记录审计日志
        self.audit_log(
            AuthEventType::SessionCreated,
            Some(user_id),
            Some(serde_json::json!({
                "session_id": session.id,
                "identity_id": identity_id,
                "ttl_seconds": ttl,
            })),
        )
        .await?;

        Ok((session, session_token))
    }

    async fn get_active_membership(
        &self,
        user_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Option<TenantMembership>, AuthError> {
        let membership = self.query_active_membership(user_id, tenant_id).await?;

        if let Some(m) = &membership {
            // 验证成员资格状态
            if !m.status.allows_access() {
                return Ok(None);
            }
        }

        Ok(membership)
    }

    async fn audit_log(
        &self,
        event_type: AuthEventType,
        user_id: Option<Uuid>,
        data: Option<JsonValue>,
    ) -> Result<(), AuthError> {
        let log = AuthAuditLog::new(event_type)
            .with_user(user_id.unwrap_or(Uuid::nil()))
            .with_details(data.unwrap_or(JsonValue::Null));

        self.create_audit_log_record(&log).await?;
        Ok(())
    }

    async fn verify_session(&self, session_token: &str) -> Result<AuthSession, AuthError> {
        // 1. 计算 Token 哈希并查询会话
        let token_hash = Self::hash_token(session_token);

        let session = self
            .query_session_by_token_hash(&token_hash)
            .await?
            .ok_or(AuthError::SessionNotFound(Uuid::nil()))?;

        // 2. 验证 Token 哈希匹配
        if !Self::verify_token_hash(session_token, &session.session_token_hash) {
            return Err(AuthError::SessionNotFound(session.id));
        }

        // 3. 验证会话有效性
        if session.is_revoked() {
            return Err(AuthError::SessionRevoked(session.id));
        }

        if session.is_expired() {
            return Err(AuthError::SessionExpired(session.id));
        }

        // 4. 验证用户状态
        let user = self
            .query_user(session.user_id)
            .await?
            .ok_or(AuthError::UserNotFound(session.user_id))?;

        if !user.is_active() {
            return Err(AuthError::InvalidUserStatus {
                status: user.status.to_string(),
            });
        }

        Ok(session)
    }

    async fn revoke_session(&self, session_id: Uuid, reason: &str) -> Result<(), AuthError> {
        // 1. 查询会话
        let session = self
            .query_session(session_id)
            .await?
            .ok_or(AuthError::SessionNotFound(session_id))?;

        // 2. 检查是否已撤销
        if session.is_revoked() {
            return Ok(()); // 已撤销，无需操作
        }

        // 3. 撤销会话
        self.update_session_revoked(session_id, reason).await?;

        // 4. 记录审计日志
        self.audit_log(
            AuthEventType::SessionRevoked,
            Some(session.user_id),
            Some(serde_json::json!({
                "session_id": session_id,
                "reason": reason,
            })),
        )
        .await?;

        Ok(())
    }

    async fn get_user(&self, user_id: Uuid) -> Result<User, AuthError> {
        let user = self
            .query_user(user_id)
            .await?
            .ok_or(AuthError::UserNotFound(user_id))?;

        if user.deleted_at.is_some() {
            return Err(AuthError::UserNotFound(user_id));
        }

        Ok(user)
    }

    async fn update_user(
        &self,
        user_id: Uuid,
        display_name: Option<String>,
        default_tenant_id: Option<Uuid>,
        onboarding_completed: Option<bool>,
    ) -> Result<User, AuthError> {
        self.get_user(user_id).await?;
        Self::validate_display_name(display_name.as_deref())?;

        self.update_user_record(
            user_id,
            display_name,
            default_tenant_id.map(Some),
            onboarding_completed,
            None,
        )
        .await
    }

    async fn soft_delete_user(&self, user_id: Uuid) -> Result<User, AuthError> {
        let deleted_at = chrono::Utc::now();
        self.get_user(user_id).await?;

        self.update_user_record(user_id, None, None, None, Some(Some(deleted_at)))
            .await
    }

    async fn get_user_identities(&self, user_id: Uuid) -> Result<Vec<ExternalIdentity>, AuthError> {
        // 验证用户存在
        self.get_user(user_id).await?;

        let identities = self.query_user_identities(user_id).await?;
        Ok(identities)
    }

    async fn get_user_memberships(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<TenantMembership>, AuthError> {
        // 验证用户存在
        self.get_user(user_id).await?;

        let memberships = self.query_user_memberships(user_id).await?;

        // 过滤活跃成员资格
        let active_memberships = memberships
            .into_iter()
            .filter(|m| m.status.allows_access())
            .collect();

        Ok(active_memberships)
    }

    async fn get_tenant_memberships(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<TenantMembership>, AuthError> {
        let memberships = self.query_tenant_memberships(tenant_id).await?;
        Ok(memberships
            .into_iter()
            .filter(|membership| membership.status.allows_access())
            .collect())
    }

    async fn get_tenant_invitations(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<TenantInvitation>, AuthError> {
        self.query_tenant_invitations(tenant_id).await
    }

    async fn get_membership_by_id(
        &self,
        membership_id: Uuid,
    ) -> Result<Option<TenantMembership>, AuthError> {
        self.query_membership_by_id(membership_id).await
    }

    async fn update_membership_role(
        &self,
        membership_id: Uuid,
        role: MembershipRole,
    ) -> Result<TenantMembership, AuthError> {
        let mut membership = self.query_membership_by_id(membership_id).await?.ok_or(
            AuthError::MembershipNotFound {
                user_id: Uuid::nil(),
                tenant_id: Uuid::nil(),
            },
        )?;

        membership.update_role(role);
        self.update_membership_record(&membership).await
    }

    async fn remove_membership(&self, membership_id: Uuid) -> Result<(), AuthError> {
        let membership = self.query_membership_by_id(membership_id).await?.ok_or(
            AuthError::MembershipNotFound {
                user_id: Uuid::nil(),
                tenant_id: Uuid::nil(),
            },
        )?;

        self.mark_membership_inactive(membership.id).await?;
        Ok(())
    }

    async fn get_invitation(
        &self,
        invitation_id: Uuid,
    ) -> Result<Option<TenantInvitation>, AuthError> {
        self.query_invitation(invitation_id).await
    }

    async fn get_invitation_by_token(
        &self,
        invitation_token: &str,
    ) -> Result<Option<TenantInvitation>, AuthError> {
        let token_hash = Self::hash_token(invitation_token);
        self.query_invitation_by_token_hash(&token_hash).await
    }

    async fn revoke_invitation(&self, invitation_id: Uuid) -> Result<TenantInvitation, AuthError> {
        let invitation = self
            .query_invitation(invitation_id)
            .await?
            .ok_or(AuthError::InvitationNotFound(invitation_id))?;

        if invitation.status == InvitationStatus::Consumed {
            return Err(AuthError::InvitationAlreadyConsumed(invitation_id));
        }

        if invitation.status == InvitationStatus::Revoked {
            return Ok(invitation);
        }

        let revoked = self.revoke_invitation_record(invitation_id).await?;

        self.audit_log(
            AuthEventType::InvitationRevoked,
            Some(revoked.created_by),
            Some(serde_json::json!({
                "invitation_id": revoked.id,
                "tenant_id": revoked.tenant_id,
            })),
        )
        .await?;

        Ok(revoked)
    }

    async fn create_owner_membership(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<TenantMembership, AuthError> {
        if let Some(existing) = self.get_active_membership(user_id, tenant_id).await? {
            return Ok(existing);
        }

        let membership = TenantMembership::new_owner(tenant_id, user_id);
        let created = self.create_membership_record(&membership).await?;

        self.audit_log(
            AuthEventType::MemberJoined,
            Some(user_id),
            Some(serde_json::json!({
                "tenant_id": tenant_id,
                "role": "owner",
                "source": "owner_creation",
            })),
        )
        .await?;

        Ok(created)
    }

    async fn create_service_account(
        &self,
        service_account: &ServiceAccount,
    ) -> Result<ServiceAccount, AuthError> {
        self.create_service_account_record(service_account).await
    }

    async fn list_service_accounts(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ServiceAccount>, AuthError> {
        self.list_service_account_records(tenant_id).await
    }

    async fn get_service_account(
        &self,
        service_account_id: Uuid,
    ) -> Result<Option<ServiceAccount>, AuthError> {
        self.query_service_account(service_account_id).await
    }

    async fn update_service_account(
        &self,
        service_account: &ServiceAccount,
    ) -> Result<ServiceAccount, AuthError> {
        self.update_service_account_record(service_account).await
    }

    async fn create_api_token_metadata(
        &self,
        metadata: &ApiTokenMetadata,
    ) -> Result<ApiTokenMetadata, AuthError> {
        self.create_api_token_metadata_record(metadata).await
    }

    async fn list_api_tokens(&self, tenant_id: Uuid) -> Result<Vec<ApiTokenMetadata>, AuthError> {
        self.list_api_token_metadata_records(tenant_id).await
    }

    async fn list_service_account_api_tokens(
        &self,
        tenant_id: Uuid,
        service_account_id: Uuid,
    ) -> Result<Vec<ApiTokenMetadata>, AuthError> {
        self.list_api_token_metadata_for_service_account(tenant_id, service_account_id)
            .await
    }

    async fn get_api_token_metadata(
        &self,
        token_id: &str,
    ) -> Result<Option<ApiTokenMetadata>, AuthError> {
        self.query_api_token_metadata(token_id).await
    }

    async fn revoke_api_token_metadata(
        &self,
        token_id: &str,
        revoked_at: DateTime<Utc>,
    ) -> Result<Option<ApiTokenMetadata>, AuthError> {
        self.revoke_api_token_metadata_record(token_id, revoked_at)
            .await
    }

    async fn mark_api_token_used(
        &self,
        token_id: &str,
        last_used_at: DateTime<Utc>,
    ) -> Result<Option<ApiTokenMetadata>, AuthError> {
        self.mark_api_token_used_record(token_id, last_used_at)
            .await
    }

    async fn sync_mfa_status(
        &self,
        user_id: Uuid,
        privy_token: &str,
    ) -> Result<MfaStatusSnapshot, AuthError> {
        // 1. 验证用户存在
        self.get_user(user_id).await?;

        // 2. 调用 Privy API 获取 MFA 状态
        let mfa_status = self.fetch_privy_mfa_status(privy_token).await?;

        // 3. 更新外部身份的 MFA 状态
        let identities = self.query_user_identities(user_id).await?;
        if let Some(privy_identity) = identities
            .iter()
            .find(|i| i.provider == IdentityProvider::Privy)
        {
            self.update_identity_mfa_status(privy_identity.id, mfa_status.verified)
                .await?;
        }

        // 4. 记录审计日志
        self.audit_log(
            AuthEventType::MfaVerified,
            Some(user_id),
            Some(serde_json::json!({
                "enabled": mfa_status.enabled,
                "verified": mfa_status.verified,
            })),
        )
        .await?;

        Ok(mfa_status)
    }

    async fn get_mfa_status(&self, user_id: Uuid) -> Result<MfaStatusSnapshot, AuthError> {
        // 1. 验证用户存在
        self.get_user(user_id).await?;

        // 2. 获取用户的外部身份
        let identities = self.query_user_identities(user_id).await?;

        // 3. 查找 Privy 身份
        let privy_identity = identities
            .iter()
            .find(|i| i.provider == IdentityProvider::Privy);

        // 4. 构建 MFA 状态快照
        let mfa_status = if let Some(identity) = privy_identity {
            MfaStatusSnapshot {
                enabled: identity.mfa_verified,
                verified: identity.mfa_verified,
                requires_step_up: false,
                last_verified_at: identity.mfa_verified_at.map(|t| t.to_rfc3339()),
                synced_at: chrono::Utc::now().to_rfc3339(),
            }
        } else {
            MfaStatusSnapshot::default()
        };

        Ok(mfa_status)
    }
}

/// 创建所有者成员资格
///
/// 在创建新租户时，自动为创建者绑定所有者角色。
pub async fn create_owner_membership(
    service: &impl AuthService,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<TenantMembership, AuthError> {
    service.create_owner_membership(tenant_id, user_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_generation() {
        let token1 = AuthServiceImpl::generate_secure_token().unwrap();
        let token2 = AuthServiceImpl::generate_secure_token().unwrap();

        // Token 应唯一
        assert_ne!(token1, token2);

        // Token 长度应为 32 bytes base64 encoded = 43 chars (URL_SAFE_NO_PAD)
        assert_eq!(token1.len(), 43);
    }

    #[test]
    fn test_token_hashing() {
        let token = "test_token_123";
        let hash = AuthServiceImpl::hash_token(token);

        // Hash 应为 SHA-256 = 32 bytes base64 encoded = 43 chars
        assert_eq!(hash.len(), 43);

        // 相同 Token 应产生相同 Hash
        let hash2 = AuthServiceImpl::hash_token(token);
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_token_verification() {
        let token = "test_token_123";
        let hash = AuthServiceImpl::hash_token(token);

        // 正确 Token 应验证成功
        assert!(AuthServiceImpl::verify_token_hash(token, &hash));

        // 错误 Token 应验证失败
        assert!(!AuthServiceImpl::verify_token_hash("wrong_token", &hash));
    }

    #[test]
    fn test_user_creation_flow() {
        let user = User::new();
        assert!(user.is_active());
        assert!(user.id.to_string().starts_with("0")); // UUID v7 starts with timestamp
    }

    #[test]
    fn test_membership_scopes() {
        let membership = TenantMembership::new_owner(Uuid::nil(), Uuid::nil());
        assert!(membership.has_scope("tenant:read"));
        assert!(membership.has_scope("credential:write"));
    }

    #[test]
    fn test_should_initialize_default_tenant_for_first_login() {
        assert!(AuthServiceImpl::should_initialize_default_tenant(None));

        let existing_identity = ExternalIdentity::new(
            Uuid::now_v7(),
            IdentityProvider::Privy,
            "did:privy:existing",
        );
        assert!(!AuthServiceImpl::should_initialize_default_tenant(Some(
            &existing_identity
        )));
    }

    #[test]
    fn test_build_default_tenant_name_prefers_email_then_display_name() {
        let user = User::new().with_display_name("Display Name");
        let from_email = PrivyAuthResponse {
            did: "did:privy:1".to_string(),
            wallet_address: None,
            email: Some("owner@example.com".to_string()),
            name: Some("Alice".to_string()),
            profile: None,
            is_new_user: false,
        };
        assert_eq!(
            AuthServiceImpl::build_default_tenant_name(&from_email, &user),
            "owner@example.com"
        );

        let from_display_name = PrivyAuthResponse {
            did: "did:privy:2".to_string(),
            wallet_address: None,
            email: None,
            name: Some("Alice".to_string()),
            profile: None,
            is_new_user: false,
        };
        assert_eq!(
            AuthServiceImpl::build_default_tenant_name(&from_display_name, &user),
            "Display Name"
        );
    }

    #[test]
    fn test_build_default_tenant_name_falls_back_to_user_id() {
        let user = User::new();
        let response = PrivyAuthResponse {
            did: "did:privy:3".to_string(),
            wallet_address: None,
            email: None,
            name: None,
            profile: None,
            is_new_user: false,
        };

        let name = AuthServiceImpl::build_default_tenant_name(&response, &user);
        assert_eq!(name, format!("Default Tenant for {}", user.id));
    }

    #[test]
    fn test_validate_display_name_accepts_valid_lengths() {
        assert!(AuthServiceImpl::validate_display_name(None).is_ok());
        assert!(
            AuthServiceImpl::validate_display_name(Some(&"A".repeat(MAX_DISPLAY_NAME_CHARS)))
                .is_ok()
        );
    }

    #[test]
    fn test_validate_display_name_rejects_overlong_input() {
        let error =
            AuthServiceImpl::validate_display_name(Some(&"A".repeat(MAX_DISPLAY_NAME_CHARS + 1)))
                .expect_err("expected overlong display_name to fail");

        match error {
            AuthError::InvalidRequest(message) => {
                assert!(message.contains("display_name"));
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }
}
