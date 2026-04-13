//! Privy 认证服务测试
//!
//! 测试内容：
//! - 用户创建 from Privy token
//! - 外部身份绑定
//! - 会话创建和验证
//! - 邀请流程测试
//! - 成员资格创建
//! - 登出/会话撤销

use uuid::Uuid;
use vault_service::auth::{
    AuthError, AuthEventType, AuthServiceImpl, AuthSession, ExternalIdentity, IdentityProvider,
    InvitationStatus, InviteeType, MembershipRole, MembershipSource, MembershipStatus,
    PrivyAuthResponse, TenantInvitation, TenantMembership, User, UserStatus,
};
use vault_service::tenant::{TenantManager, config::MemoryTenantConfigStore};

// ============================================================================
// Mock 辅助函数
// ============================================================================

/// 创建内存存储的认证服务
#[allow(dead_code)]
fn create_auth_service() -> AuthServiceImpl {
    let tenant_manager = TenantManager::new_simple(MemoryTenantConfigStore::new());
    AuthServiceImpl::new_in_memory(tenant_manager)
}

/// 创建测试用的 Privy 响应
#[allow(dead_code)]
fn create_mock_privy_response(
    did: &str,
    wallet: Option<&str>,
    email: Option<&str>,
    is_new: bool,
) -> PrivyAuthResponse {
    PrivyAuthResponse {
        did: did.to_string(),
        wallet_address: wallet.map(|w| w.to_string()),
        email: email.map(|e| e.to_string()),
        name: Some("Test User".to_string()),
        profile: None,
        is_new_user: is_new,
    }
}

/// 创建测试用户
#[allow(dead_code)]
fn create_test_user() -> User {
    let mut user = User::new();
    user.display_name = Some("Test User".to_string());
    user
}

/// 创建测试邀请
#[allow(dead_code)]
fn create_test_invitation(tenant_id: Uuid, created_by: Uuid) -> TenantInvitation {
    TenantInvitation::new_wallet_invitation(
        tenant_id,
        MembershipRole::Member,
        "0x1234567890abcdef",
        created_by,
        24, // 24 hours
    )
}

// ============================================================================
// 用户创建测试
// ============================================================================

#[test]
fn test_user_creation_basic() {
    // Given: 新用户请求
    let user = User::new();

    // Then: 用户应该是活跃状态
    assert!(user.is_active());
    assert_eq!(user.status, UserStatus::Active);
    assert!(!user.onboarding_completed);
    assert!(user.display_name.is_none());
    assert!(user.deleted_at.is_none());
}

#[test]
fn test_user_with_display_name() {
    // Given: 用户带显示名称
    let user = User::new().with_display_name("Alice");

    // Then: 显示名称应该设置
    assert_eq!(user.display_name, Some("Alice".to_string()));
}

#[test]
fn test_user_with_default_tenant() {
    // Given: 用户带默认租户
    let tenant_id = Uuid::now_v7();
    let user = User::new().with_default_tenant(tenant_id);

    // Then: 默认租户应该设置
    assert_eq!(user.default_tenant_id, Some(tenant_id));
}

#[test]
fn test_user_status_transitions() {
    // Given: 活跃用户
    let mut user = User::new();
    assert!(user.is_active());

    // When: 暂停用户
    user.suspend();
    // Then: 用户被暂停
    assert_eq!(user.status, UserStatus::Suspended);
    assert!(!user.is_active());

    // When: 激活用户
    user.activate();
    // Then: 用户恢复活跃
    assert_eq!(user.status, UserStatus::Active);
    assert!(user.is_active());

    // When: 软删除用户
    user.soft_delete();
    // Then: 用户标记为待删除
    assert_eq!(user.status, UserStatus::PendingDeletion);
    assert!(user.deleted_at.is_some());
    assert!(!user.is_active());
}

#[test]
fn test_user_onboarding_completion() {
    // Given: 新用户
    let mut user = User::new();
    assert!(!user.onboarding_completed);

    // When: 完成引导
    user.complete_onboarding();
    // Then: 标记为已完成
    assert!(user.onboarding_completed);
}

// ============================================================================
// 外部身份测试
// ============================================================================

#[test]
fn test_external_identity_creation() {
    // Given: 用户 ID 和 Privy did
    let user_id = Uuid::now_v7();
    let identity = ExternalIdentity::new(user_id, IdentityProvider::Privy, "did:privy:123");

    // Then: 身份应该正确创建
    assert_eq!(identity.user_id, user_id);
    assert_eq!(identity.provider, IdentityProvider::Privy);
    assert_eq!(identity.provider_subject, "did:privy:123");
    assert!(!identity.is_verified);
    assert!(!identity.is_primary);
}

#[test]
fn test_external_identity_with_wallet() {
    // Given: 用户 ID 和钱包地址
    let user_id = Uuid::now_v7();
    let identity = ExternalIdentity::new(user_id, IdentityProvider::Privy, "did:privy:123")
        .with_wallet("0xabcdef123456");

    // Then: 钱包地址应该设置
    assert_eq!(identity.wallet_address, Some("0xabcdef123456".to_string()));
    assert!(identity.is_wallet_identity());
}

#[test]
fn test_external_identity_with_email() {
    // Given: 用户 ID 和邮箱
    let user_id = Uuid::now_v7();
    let identity = ExternalIdentity::new(user_id, IdentityProvider::Email, "user@example.com")
        .with_email("user@example.com");

    // Then: 邮箱应该设置
    assert_eq!(identity.email, Some("user@example.com".to_string()));
    assert!(!identity.is_wallet_identity());
}

#[test]
fn test_external_identity_verification() {
    // Given: 未验证的身份
    let user_id = Uuid::now_v7();
    let mut identity = ExternalIdentity::new(user_id, IdentityProvider::Privy, "did:privy:123");
    assert!(!identity.is_verified);

    // When: 验证身份
    identity.verify();
    // Then: 标记为已验证
    assert!(identity.is_verified);
}

#[test]
fn test_external_identity_set_primary() {
    // Given: 非主要身份
    let user_id = Uuid::now_v7();
    let mut identity = ExternalIdentity::new(user_id, IdentityProvider::Privy, "did:privy:123");
    assert!(!identity.is_primary);

    // When: 设置为主要身份
    identity.set_primary();
    // Then: 标记为主要
    assert!(identity.is_primary);
}

#[test]
fn test_identity_provider_types() {
    // Then: 各种提供商类型正确识别
    assert!(IdentityProvider::Privy.is_wallet_provider());
    assert!(!IdentityProvider::Privy.is_oauth());

    assert!(IdentityProvider::Google.is_oauth());
    assert!(!IdentityProvider::Google.is_wallet_provider());

    assert!(IdentityProvider::GitHub.is_oauth());
    assert!(IdentityProvider::Apple.is_oauth());
    assert!(!IdentityProvider::Email.is_oauth());
    assert!(!IdentityProvider::Email.is_wallet_provider());
}

// ============================================================================
// 邀请流程测试
// ============================================================================

#[test]
fn test_invitation_creation_email() {
    // Given: 租户 ID 和创建者
    let tenant_id = Uuid::now_v7();
    let created_by = Uuid::now_v7();

    // When: 创建邮箱邀请
    let invitation = TenantInvitation::new_email_invitation(
        tenant_id,
        MembershipRole::Admin,
        "invite@example.com",
        created_by,
        48, // 48 hours
    );

    // Then: 邀请应该正确创建
    assert_eq!(invitation.tenant_id, tenant_id);
    assert_eq!(invitation.role, MembershipRole::Admin);
    assert_eq!(invitation.invitee_type, InviteeType::Email);
    assert_eq!(
        invitation.invitee_email,
        Some("invite@example.com".to_string())
    );
    assert!(invitation.invitee_wallet.is_none());
    assert_eq!(invitation.status, InvitationStatus::Pending);
    assert_eq!(invitation.max_uses, 1);
    assert!(invitation.is_valid());
}

#[test]
fn test_invitation_creation_wallet() {
    // Given: 租户 ID 和钱包地址
    let tenant_id = Uuid::now_v7();
    let created_by = Uuid::now_v7();

    // When: 创建钱包邀请
    let invitation = TenantInvitation::new_wallet_invitation(
        tenant_id,
        MembershipRole::Member,
        "0x1234567890abcdef",
        created_by,
        24,
    );

    // Then: 邀请应该正确创建
    assert_eq!(invitation.invitee_type, InviteeType::Wallet);
    assert_eq!(
        invitation.invitee_wallet,
        Some("0x1234567890abcdef".to_string())
    );
    assert!(invitation.invitee_email.is_none());
}

#[test]
fn test_invitation_creation_open() {
    // Given: 租户 ID
    let tenant_id = Uuid::now_v7();
    let created_by = Uuid::now_v7();

    // When: 创建开放邀请链接
    let invitation = TenantInvitation::new_open_invitation(
        tenant_id,
        MembershipRole::Readonly,
        created_by,
        72,
        5, // 可使用 5 次
    );

    // Then: 开放邀请应该正确创建
    assert_eq!(invitation.invitee_type, InviteeType::Any);
    assert!(invitation.invitee_email.is_none());
    assert!(invitation.invitee_wallet.is_none());
    assert_eq!(invitation.max_uses, 5);
}

#[test]
fn test_invitation_validity_check() {
    // Given: 新创建的邀请
    let tenant_id = Uuid::now_v7();
    let created_by = Uuid::now_v7();
    let invitation = TenantInvitation::new_email_invitation(
        tenant_id,
        MembershipRole::Member,
        "test@example.com",
        created_by,
        24,
    );

    // Then: 邀请应该有效
    assert!(invitation.is_valid());
    assert!(!invitation.is_expired());
    assert_eq!(invitation.status, InvitationStatus::Pending);
    assert_eq!(invitation.use_count, 0);
}

#[test]
fn test_invitation_expiry() {
    // Given: 已过期的邀请（过期时间为负数）
    use chrono::{Duration, Utc};
    let tenant_id = Uuid::now_v7();
    let created_by = Uuid::now_v7();
    let mut invitation = TenantInvitation::new_email_invitation(
        tenant_id,
        MembershipRole::Member,
        "test@example.com",
        created_by,
        -1, // 已过期
    );

    // 手动设置过期时间为过去
    invitation.expires_at = Utc::now() - Duration::hours(1);

    // Then: 邀请应该无效
    assert!(!invitation.is_valid());
    assert!(invitation.is_expired());
}

#[test]
fn test_invitation_consume() {
    // Given: 有效邀请
    let tenant_id = Uuid::now_v7();
    let created_by = Uuid::now_v7();
    let mut invitation = TenantInvitation::new_email_invitation(
        tenant_id,
        MembershipRole::Member,
        "test@example.com",
        created_by,
        24,
    );
    let consumer_id = Uuid::now_v7();

    // When: 消费邀请
    invitation.consume(consumer_id);

    // Then: 邀请状态更新
    assert_eq!(invitation.use_count, 1);
    assert!(invitation.consumed_at.is_some());
    assert_eq!(invitation.consumed_by, Some(consumer_id));
    assert_eq!(invitation.status, InvitationStatus::Consumed);
    assert!(!invitation.is_valid());
}

#[test]
fn test_invitation_revoke() {
    // Given: 有效邀请
    let tenant_id = Uuid::now_v7();
    let created_by = Uuid::now_v7();
    let mut invitation = TenantInvitation::new_email_invitation(
        tenant_id,
        MembershipRole::Member,
        "test@example.com",
        created_by,
        24,
    );
    assert!(invitation.is_valid());

    // When: 撤销邀请
    invitation.revoke();

    // Then: 邀请应该无效
    assert_eq!(invitation.status, InvitationStatus::Revoked);
    assert!(!invitation.is_valid());
}

#[test]
fn test_invitation_multiple_uses() {
    // Given: 可多次使用的邀请
    let tenant_id = Uuid::now_v7();
    let created_by = Uuid::now_v7();
    let mut invitation = TenantInvitation::new_open_invitation(
        tenant_id,
        MembershipRole::Readonly,
        created_by,
        24,
        3, // 可使用 3 次
    );

    // When: 使用两次
    invitation.consume(Uuid::now_v7());
    assert_eq!(invitation.use_count, 1);
    assert!(invitation.is_valid()); // 还可以继续使用

    invitation.consume(Uuid::now_v7());
    assert_eq!(invitation.use_count, 2);
    assert!(invitation.is_valid()); // 还可以继续使用

    // When: 使用第三次
    invitation.consume(Uuid::now_v7());
    assert_eq!(invitation.use_count, 3);
    assert!(!invitation.is_valid()); // 已用完
    assert_eq!(invitation.status, InvitationStatus::Consumed);
}

// ============================================================================
// 成员资格测试
// ============================================================================

#[test]
fn test_membership_creation_from_invitation() {
    // Given: 租户 ID、用户 ID 和邀请人
    let tenant_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let invited_by = Uuid::now_v7();

    // When: 从邀请创建成员资格
    let membership = TenantMembership::new_from_invitation(
        tenant_id,
        user_id,
        MembershipRole::Member,
        invited_by,
    );

    // Then: 成员资格应该正确创建
    assert_eq!(membership.tenant_id, tenant_id);
    assert_eq!(membership.user_id, user_id);
    assert_eq!(membership.role, MembershipRole::Member);
    assert_eq!(membership.status, MembershipStatus::Pending);
    assert_eq!(membership.source, MembershipSource::Invitation);
    assert_eq!(membership.invited_by, Some(invited_by));
    assert!(membership.joined_at.is_none());
}

#[test]
fn test_membership_creation_owner() {
    // Given: 租户 ID 和创建者
    let tenant_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();

    // When: 创建所有者成员资格
    let membership = TenantMembership::new_owner(tenant_id, user_id);

    // Then: 所有者成员资格应该正确创建
    assert_eq!(membership.role, MembershipRole::Owner);
    assert_eq!(membership.status, MembershipStatus::Active);
    assert_eq!(membership.source, MembershipSource::OwnerCreation);
    assert!(membership.invited_by.is_none());
    assert!(membership.joined_at.is_some());
    assert!(membership.is_owner());
    assert!(membership.can_manage());
}

#[test]
fn test_membership_accept_invitation() {
    // Given: 待确认的成员资格
    let tenant_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let invited_by = Uuid::now_v7();
    let mut membership = TenantMembership::new_from_invitation(
        tenant_id,
        user_id,
        MembershipRole::Member,
        invited_by,
    );
    assert_eq!(membership.status, MembershipStatus::Pending);
    assert!(!membership.status.allows_access());

    // When: 接受邀请
    membership.accept_invitation();

    // Then: 成员资格激活
    assert_eq!(membership.status, MembershipStatus::Active);
    assert!(membership.status.allows_access());
    assert!(membership.joined_at.is_some());
}

#[test]
fn test_membership_role_update() {
    // Given: 成员角色
    let tenant_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let mut membership = TenantMembership::new_from_invitation(
        tenant_id,
        user_id,
        MembershipRole::Member,
        Uuid::now_v7(),
    );
    membership.accept_invitation();

    // Then: 初始 scopes (使用单数 credential:)
    assert!(membership.has_scope("credential:read"));
    assert!(membership.has_scope("credential:write"));
    assert!(!membership.has_scope("tenant:admin"));

    // When: 升级为管理员
    membership.update_role(MembershipRole::Admin);

    // Then: scopes 更新
    assert_eq!(membership.role, MembershipRole::Admin);
    assert!(membership.has_scope("tenant:admin"));
    assert!(membership.has_scope("members:invite"));
}

#[test]
fn test_membership_scope_management() {
    // Given: 成员资格
    let tenant_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let mut membership = TenantMembership::new_owner(tenant_id, user_id);

    // When: 添加自定义 scope
    membership.add_scope("custom:feature");

    // Then: scope 应该添加
    assert!(membership.has_scope("custom:feature"));

    // When: 移除 scope
    membership.remove_scope("custom:feature");

    // Then: scope 应该移除
    assert!(!membership.has_scope("custom:feature"));
}

#[test]
fn test_membership_status_transitions() {
    // Given: 活跃成员
    let tenant_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let mut membership = TenantMembership::new_owner(tenant_id, user_id);
    assert!(membership.status.allows_access());

    // When: 暂停成员
    membership.suspend();
    assert_eq!(membership.status, MembershipStatus::Suspended);
    assert!(!membership.status.allows_access());

    // When: 激活成员
    membership.activate();
    assert_eq!(membership.status, MembershipStatus::Active);
    assert!(membership.status.allows_access());

    // When: 成员离开
    membership.leave();
    assert_eq!(membership.status, MembershipStatus::Inactive);
    assert!(!membership.status.allows_access());
}

#[test]
fn test_membership_role_permissions() {
    // Then: 各角色权限正确
    let owner = MembershipRole::Owner;
    assert!(owner.is_owner());
    assert!(owner.can_manage());
    assert!(owner.can_write());
    assert_eq!(owner.permission_level(), 100);

    let admin = MembershipRole::Admin;
    assert!(!admin.is_owner());
    assert!(admin.can_manage());
    assert!(admin.can_write());
    assert_eq!(admin.permission_level(), 80);

    let member = MembershipRole::Member;
    assert!(!member.is_owner());
    assert!(!member.can_manage());
    assert!(member.can_write());
    assert_eq!(member.permission_level(), 50);

    let readonly = MembershipRole::Readonly;
    assert!(!readonly.is_owner());
    assert!(!readonly.can_manage());
    assert!(!readonly.can_write());
    assert_eq!(readonly.permission_level(), 10);
}

#[test]
fn test_membership_default_scopes() {
    // Then: 各角色默认 scopes (使用单数 credential:)
    let owner_scopes = MembershipRole::Owner.default_scopes();
    assert!(owner_scopes.contains(&"tenant:delete".to_string()));
    assert!(owner_scopes.contains(&"members:invite".to_string()));
    assert!(owner_scopes.contains(&"sandbox:execute".to_string()));

    let admin_scopes = MembershipRole::Admin.default_scopes();
    assert!(admin_scopes.contains(&"tenant:admin".to_string()));
    assert!(!admin_scopes.contains(&"tenant:delete".to_string()));
    assert!(admin_scopes.contains(&"sandbox:write".to_string()));

    let member_scopes = MembershipRole::Member.default_scopes();
    assert!(member_scopes.contains(&"credential:read".to_string()));
    assert!(member_scopes.contains(&"credential:write".to_string()));
    assert!(member_scopes.contains(&"sandbox:read".to_string()));
    assert!(!member_scopes.contains(&"members:invite".to_string()));

    let readonly_scopes = MembershipRole::Readonly.default_scopes();
    assert!(readonly_scopes.contains(&"credential:read".to_string()));
    assert!(!readonly_scopes.contains(&"sandbox:read".to_string()));
    assert!(!readonly_scopes.contains(&"credential:write".to_string()));
}

// ============================================================================
// 会话测试
// ============================================================================

#[test]
fn test_session_creation() {
    // Given: 用户 ID 和 token hash
    let user_id = Uuid::now_v7();
    let token_hash = "test_hash_value".to_string();
    let session = AuthSession::new(user_id, token_hash.clone(), 3600);

    // Then: 会话应该正确创建
    assert_eq!(session.user_id, user_id);
    assert_eq!(session.session_token_hash, token_hash);
    assert!(session.is_valid());
    assert!(!session.is_expired());
    assert!(!session.is_revoked());
}

#[test]
fn test_session_with_identity() {
    // Given: 用户 ID 和身份 ID
    let user_id = Uuid::now_v7();
    let identity_id = Uuid::now_v7();
    let session = AuthSession::new(user_id, "hash".to_string(), 3600).with_identity(identity_id);

    // Then: 身份 ID 应该设置
    assert_eq!(session.identity_id, Some(identity_id));
}

#[test]
fn test_session_with_active_membership() {
    // Given: 用户 ID 和成员资格 ID
    let user_id = Uuid::now_v7();
    let membership_id = Uuid::now_v7();
    let session =
        AuthSession::new(user_id, "hash".to_string(), 3600).with_active_membership(membership_id);

    // Then: 成员资格 ID 应该设置
    assert_eq!(session.active_membership_id, Some(membership_id));
}

#[test]
fn test_session_expiry() {
    // Given: 即将过期的会话（TTL 为 0）
    use chrono::{Duration, Utc};
    let user_id = Uuid::now_v7();
    let mut session = AuthSession::new(user_id, "hash".to_string(), 0);

    // 手动设置过期时间为过去
    session.expires_at = Utc::now() - Duration::seconds(1);

    // Then: 会话应该已过期
    assert!(session.is_expired());
    assert!(!session.is_valid());
}

#[test]
fn test_session_revocation() {
    // Given: 有效会话
    let user_id = Uuid::now_v7();
    let mut session = AuthSession::new(user_id, "hash".to_string(), 3600);
    assert!(session.is_valid());

    // When: 撤销会话
    session.revoke("user logout");

    // Then: 会话应该已撤销
    assert!(session.is_revoked());
    assert!(!session.is_valid());
    assert!(session.revoked_at.is_some());
    assert_eq!(session.revoked_reason, Some("user logout".to_string()));
}

#[test]
fn test_session_extend() {
    // Given: 有效会话
    let user_id = Uuid::now_v7();
    let mut session = AuthSession::new(user_id, "hash".to_string(), 3600);
    let original_expires_at = session.expires_at;

    // When: 延长会话
    session.extend(1800); // 延长 30 分钟

    // Then: 过期时间应该更新
    assert!(session.expires_at > original_expires_at);
}

#[test]
fn test_session_touch() {
    // Given: 会话
    let user_id = Uuid::now_v7();
    let mut session = AuthSession::new(user_id, "hash".to_string(), 3600);
    let original_last_active = session.last_active_at;

    // When: 更新活跃时间
    session.touch();

    // Then: 最后活跃时间应该更新
    assert!(session.last_active_at > original_last_active);
}

#[test]
fn test_session_mfa_status() {
    // Given: 无需 MFA 的会话
    let user_id = Uuid::now_v7();
    let mut session = AuthSession::new(user_id, "hash".to_string(), 3600);
    assert!(session.is_mfa_passed());

    // When: 设置 MFA 待验证
    session.set_mfa_status(vault_service::auth::MfaStatus::Pending);
    assert!(!session.is_mfa_passed());

    // When: MFA 验证通过
    session.set_mfa_status(vault_service::auth::MfaStatus::Verified);
    assert!(session.is_mfa_passed());
    assert!(session.mfa_verified_at.is_some());
}

// ============================================================================
// 错误类型测试
// ============================================================================

#[test]
fn test_auth_error_retryable() {
    // Then: 可重试错误
    let db_error = AuthError::DatabaseError(sqlx::Error::RowNotFound);
    assert!(db_error.is_retryable());

    let crypto_error = AuthError::CryptoError("encryption failed".to_string());
    assert!(crypto_error.is_retryable());

    // Then: 不可重试错误
    let user_not_found = AuthError::UserNotFound(Uuid::nil());
    assert!(!user_not_found.is_retryable());

    let invalid_token = AuthError::InvalidPrivyToken("bad token".to_string());
    assert!(!invalid_token.is_retryable());
}

#[test]
fn test_auth_error_http_status() {
    // Then: HTTP 状态码映射
    assert_eq!(AuthError::UserNotFound(Uuid::nil()).http_status_code(), 404);
    assert_eq!(
        AuthError::MembershipNotFound {
            user_id: Uuid::nil(),
            tenant_id: Uuid::nil()
        }
        .http_status_code(),
        404
    );
    assert_eq!(
        AuthError::InvitationNotFound(Uuid::nil()).http_status_code(),
        404
    );

    assert_eq!(
        AuthError::UserAlreadyExists {
            user_id: Uuid::nil()
        }
        .http_status_code(),
        409
    );
    assert_eq!(
        AuthError::MembershipAlreadyExists {
            user_id: Uuid::nil(),
            tenant_id: Uuid::nil()
        }
        .http_status_code(),
        409
    );

    assert_eq!(
        AuthError::InvalidPrivyToken("test".to_string()).http_status_code(),
        401
    );
    assert_eq!(AuthError::PrivyTokenExpired.http_status_code(), 401);
    assert_eq!(
        AuthError::SessionExpired(Uuid::nil()).http_status_code(),
        401
    );
    assert_eq!(
        AuthError::InsufficientPermissions {
            required: "admin".to_string(),
            current: "member".to_string()
        }
        .http_status_code(),
        403
    );

    assert_eq!(
        AuthError::InvitationExpired(Uuid::nil()).http_status_code(),
        410
    );

    assert_eq!(
        AuthError::DatabaseError(sqlx::Error::RowNotFound).http_status_code(),
        500
    );
}

#[test]
fn test_auth_error_client_error() {
    // Then: 客户端错误识别
    assert!(AuthError::UserNotFound(Uuid::nil()).is_client_error());
    assert!(AuthError::InvalidPrivyToken("test".to_string()).is_client_error());
    assert!(AuthError::PrivyTokenExpired.is_client_error());
    assert!(AuthError::InvitationExpired(Uuid::nil()).is_client_error());
    assert!(
        AuthError::InsufficientPermissions {
            required: "admin".to_string(),
            current: "member".to_string()
        }
        .is_client_error()
    );

    // Then: 服务端错误不是客户端错误
    assert!(!AuthError::DatabaseError(sqlx::Error::RowNotFound).is_client_error());
}

// ============================================================================
// 审计日志测试
// ============================================================================

#[test]
fn test_audit_log_creation() {
    // Given: 事件类型
    let log = vault_service::auth::AuthAuditLog::new(AuthEventType::UserLogin);

    // Then: 审计日志应该正确创建
    assert!(log.success);
    assert_eq!(log.event_type, AuthEventType::UserLogin);
    assert!(log.user_id.is_none());
    assert!(log.details.is_none());
}

#[test]
fn test_audit_log_with_user() {
    // Given: 用户 ID
    let user_id = Uuid::now_v7();
    let log = vault_service::auth::AuthAuditLog::new(AuthEventType::UserCreated).with_user(user_id);

    // Then: 用户 ID 应该设置
    assert_eq!(log.user_id, Some(user_id));
}

#[test]
fn test_audit_log_with_details() {
    // Given: 详情数据
    let details = serde_json::json!({
        "provider": "privy",
        "did": "did:privy:123"
    });
    let log = vault_service::auth::AuthAuditLog::new(AuthEventType::IdentityLinked)
        .with_details(details.clone());

    // Then: 详情应该设置
    assert_eq!(log.details, Some(details));
}

#[test]
fn test_audit_log_mark_failed() {
    // Given: 成功的日志
    let log = vault_service::auth::AuthAuditLog::new(AuthEventType::AuthenticationFailed)
        .mark_failed("Invalid credentials");

    // Then: 标记为失败
    assert!(!log.success);
    assert_eq!(log.error_message, Some("Invalid credentials".to_string()));
}

#[test]
fn test_auth_event_type_str() {
    // Then: 事件类型字符串
    assert_eq!(AuthEventType::UserCreated.as_str(), "user_created");
    assert_eq!(AuthEventType::UserLogin.as_str(), "user_login");
    assert_eq!(AuthEventType::UserLogout.as_str(), "user_logout");
    assert_eq!(AuthEventType::SessionCreated.as_str(), "session_created");
    assert_eq!(AuthEventType::SessionRevoked.as_str(), "session_revoked");
    assert_eq!(
        AuthEventType::InvitationCreated.as_str(),
        "invitation_created"
    );
    assert_eq!(
        AuthEventType::InvitationAccepted.as_str(),
        "invitation_accepted"
    );
    assert_eq!(AuthEventType::MemberJoined.as_str(), "member_joined");
}

// ============================================================================
// AuthService 内部方法测试
// ============================================================================

// ============================================================================
// 边界情况测试
// ============================================================================

#[test]
fn test_duplicate_identity_handling() {
    // Given: 相同的身份信息
    let user_id = Uuid::now_v7();
    let identity1 = ExternalIdentity::new(user_id, IdentityProvider::Privy, "did:privy:123");
    let identity2 = ExternalIdentity::new(user_id, IdentityProvider::Privy, "did:privy:123");

    // Then: 身份应该相同（仅关键属性）
    assert_eq!(identity1.provider, identity2.provider);
    assert_eq!(identity1.provider_subject, identity2.provider_subject);
}

#[test]
fn test_empty_scopes_handling() {
    // Given: Readonly 角色（最小权限）
    let scopes = MembershipRole::Readonly.default_scopes();

    // Then: 只读角色有最小 scopes
    assert!(!scopes.is_empty());
    assert!(scopes.contains(&"tenant:read".to_string()));
    assert!(scopes.contains(&"credential:read".to_string()));
}

#[test]
fn test_membership_can_manage_check() {
    // Given: 不同角色的成员资格
    let owner_membership = TenantMembership::new_owner(Uuid::nil(), Uuid::nil());
    assert!(owner_membership.can_manage());

    let mut member_membership = TenantMembership::new_from_invitation(
        Uuid::nil(),
        Uuid::nil(),
        MembershipRole::Member,
        Uuid::nil(),
    );
    member_membership.accept_invitation();
    assert!(!member_membership.can_manage());

    // When: 成员资格暂停
    member_membership.suspend();
    // Then: 即使是管理员，暂停后也不能管理
    assert!(!member_membership.can_manage());
}
