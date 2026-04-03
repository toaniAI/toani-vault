//! 认证服务层
//!
//! 提供认证相关核心业务逻辑：
//! - 用户创建与管理
//! - 外部身份绑定与验证
//! - 租户邀请创建与消费
//! - 会话管理
//! - 审计日志记录

use async_trait::async_trait;
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use uuid::Uuid;

use super::error::AuthError;
use super::models::{
    AuthAuditLog, AuthEventType, AuthSession, CreateUserRequest, ExternalIdentity,
    IdentityProvider, InvitationStatus, InviteeType, MembershipRole, PrivyAuthResponse,
    TenantInvitation, TenantMembership, User,
};
use crate::audit::AuditRecorder;
use crate::auth::privy::JwksVerifier;
use crate::config::PrivyConfig;
use crate::crypto::constant_time::ct_compare;
use crate::tenant::TenantManager;

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

    /// 获取用户的所有外部身份
    ///
    /// 查询用户绑定的所有外部身份。
    async fn get_user_identities(&self, user_id: Uuid) -> Result<Vec<ExternalIdentity>, AuthError>;

    /// 获取用户的所有成员资格
    ///
    /// 查询用户的所有租户成员资格。
    async fn get_user_memberships(&self, user_id: Uuid)
    -> Result<Vec<TenantMembership>, AuthError>;

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

    /// 租户管理器
    tenant_manager: TenantManager<crate::tenant::config::MemoryTenantConfigStore>,

    /// 审计记录器（可选）
    audit_recorder: Option<AuditRecorder>,

    /// JWKS Token 验证器
    jwks_verifier: Option<JwksVerifier>,

    /// Privy 配置
    privy_config: Option<PrivyConfig>,
}

impl AuthServiceImpl {
    /// 创建新的认证服务实例
    pub fn new(
        db_pool: Option<PgPool>,
        tenant_manager: TenantManager<crate::tenant::config::MemoryTenantConfigStore>,
    ) -> Self {
        Self {
            db_pool,
            tenant_manager,
            audit_recorder: None,
            jwks_verifier: None,
            privy_config: None,
        }
    }

    /// 创建无数据库的认证服务实例（使用内存存储）
    pub fn new_in_memory(
        tenant_manager: TenantManager<crate::tenant::config::MemoryTenantConfigStore>,
    ) -> Self {
        Self {
            db_pool: None,
            tenant_manager,
            audit_recorder: None,
            jwks_verifier: None,
            privy_config: None,
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
        // 检查是否启用 Mock 模式
        if let Some(ref config) = self.privy_config {
            if config.mock_enabled {
                return self.mock_verify_privy_token(token);
            }
        }

        // 真实 JWKS 验证
        let verifier = self
            .jwks_verifier
            .as_ref()
            .ok_or_else(|| AuthError::ConfigError("JWKS verifier not initialized".to_string()))?;

        let claims = verifier.verify(token).await?;

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
        // TODO: 实现数据库插入
        // 当前返回 Mock 数据
        Ok(user.clone())
    }

    /// 创建外部身份记录
    async fn create_external_identity_record(
        &self,
        identity: &ExternalIdentity,
    ) -> Result<ExternalIdentity, AuthError> {
        // TODO: 实现数据库插入
        Ok(identity.clone())
    }

    /// 创建成员资格记录
    async fn create_membership_record(
        &self,
        membership: &TenantMembership,
    ) -> Result<TenantMembership, AuthError> {
        // TODO: 实现数据库插入
        Ok(membership.clone())
    }

    /// 创建邀请记录
    async fn create_invitation_record(
        &self,
        invitation: &TenantInvitation,
    ) -> Result<TenantInvitation, AuthError> {
        // TODO: 实现数据库插入
        Ok(invitation.clone())
    }

    /// 创建会话记录
    async fn create_session_record(&self, session: &AuthSession) -> Result<AuthSession, AuthError> {
        // TODO: 实现数据库插入
        Ok(session.clone())
    }

    /// 创建审计日志记录
    async fn create_audit_log_record(&self, log: &AuthAuditLog) -> Result<(), AuthError> {
        // TODO: 实现数据库插入
        // 当前 AuthAuditLog 与 AuditEntry 结构不同，需要映射
        // 暂时只记录到日志
        tracing::info!(
            event_type = log.event_type.as_str(),
            user_id = log.user_id.map(|id| id.to_string()).unwrap_or_default(),
            success = log.success,
            "Auth audit event recorded"
        );
        Ok(())
    }

    /// 查询用户
    async fn query_user(&self, _user_id: Uuid) -> Result<Option<User>, AuthError> {
        // TODO: 实现数据库查询
        Ok(None)
    }

    /// 查询外部身份
    async fn query_external_identity(
        &self,
        _provider: IdentityProvider,
        _subject: &str,
    ) -> Result<Option<ExternalIdentity>, AuthError> {
        // TODO: 实现数据库查询
        Ok(None)
    }

    /// 查询邀请
    async fn query_invitation(
        &self,
        _invitation_id: Uuid,
    ) -> Result<Option<TenantInvitation>, AuthError> {
        // TODO: 实现数据库查询
        Ok(None)
    }

    /// 查询会话
    async fn query_session(&self, _session_id: Uuid) -> Result<Option<AuthSession>, AuthError> {
        // TODO: 实现数据库查询
        Ok(None)
    }

    /// 查询用户的所有外部身份
    async fn query_user_identities(
        &self,
        _user_id: Uuid,
    ) -> Result<Vec<ExternalIdentity>, AuthError> {
        // TODO: 实现数据库查询
        Ok(Vec::new())
    }

    /// 查询用户的所有成员资格
    async fn query_user_memberships(
        &self,
        _user_id: Uuid,
    ) -> Result<Vec<TenantMembership>, AuthError> {
        // TODO: 实现数据库查询
        Ok(Vec::new())
    }

    /// 查询活跃成员资格
    async fn query_active_membership(
        &self,
        _user_id: Uuid,
        _tenant_id: Uuid,
    ) -> Result<Option<TenantMembership>, AuthError> {
        // TODO: 实现数据库查询
        Ok(None)
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

        // 3. 如果外部身份已存在，返回关联用户
        if let Some(identity) = existing_identity {
            let user = self
                .query_user(identity.user_id)
                .await?
                .ok_or(AuthError::UserNotFound(identity.user_id))?;
            return Ok(user);
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

        // 7. 记录审计日志
        self.audit_log(
            AuthEventType::UserCreated,
            Some(user.id),
            Some(serde_json::json!({
                "provider": "privy",
                "did": privy_response.did,
                "is_new_user": privy_response.is_new_user,
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

        // 2. 生成邀请 Token
        let invitation_token = Self::generate_secure_token()?;
        let token_hash = Self::hash_token(&invitation_token);

        // 3. 创建邀请实体
        let invitation = match invitee_type {
            InviteeType::Email => TenantInvitation::new_email_invitation(
                tenant_id,
                role,
                invitee_email.unwrap_or_default(),
                created_by,
                expires_hours,
            ),
            InviteeType::Wallet => TenantInvitation::new_wallet_invitation(
                tenant_id,
                role,
                invitee_wallet.unwrap_or_default(),
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
        let _token_hash = Self::hash_token(invitation_token);

        // TODO: 实现根据 token_hash 查询邀请
        // 当前使用 Mock 实现
        let invitation = self
            .query_invitation(Uuid::nil())
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
        // TODO: 实现邀请消费记录更新

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
        // 1. 计算 Token 哈希
        let _token_hash = Self::hash_token(session_token);

        // TODO: 实现根据 token_hash 查询会话
        // 当前使用 Mock 实现
        let session = self
            .query_session(Uuid::nil())
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
        // TODO: 实现数据库更新

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

    async fn sync_mfa_status(
        &self,
        user_id: Uuid,
        privy_token: &str,
    ) -> Result<MfaStatusSnapshot, AuthError> {
        // 1. 验证用户存在
        self.get_user(user_id).await?;

        // 2. 调用 Privy API 获取 MFA 状态
        // TODO: 实现实际的 Privy API 调用
        // 当前返回 Mock 数据
        let mfa_status = self.fetch_privy_mfa_status(privy_token).await?;

        // 3. 更新外部身份的 MFA 状态
        // TODO: 实现数据库更新

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
    // 创建所有者成员资格
    let membership = TenantMembership::new_owner(tenant_id, user_id);

    // TODO: 实现数据库插入
    // 当前返回实体

    // 记录审计日志
    service
        .audit_log(
            AuthEventType::MemberJoined,
            Some(user_id),
            Some(serde_json::json!({
                "tenant_id": tenant_id,
                "role": "owner",
                "source": "owner_creation",
            })),
        )
        .await?;

    Ok(membership)
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
}
