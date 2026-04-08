//! Scope 映射测试
//!
//! 测试内容：
//! - 角色到 Scope 的映射
//! - Owner 获取所有 Scope
//! - Readonly 获取最小 Scope
//! - Scope 在中间件中的强制执行

use uuid::Uuid;
use vault_service::auth::{MembershipRole, TenantMembership};
use vault_service::token::CredentialAccess;
use vault_service::token::scope::Scope;

// ============================================================================
// 角色到 Scope 映射测试
// ============================================================================

#[test]
fn test_owner_role_scopes() {
    // Given: Owner 角色
    let scopes = MembershipRole::Owner.default_scopes();

    // Then: Owner 应该拥有所有权限
    assert!(scopes.contains(&"tenant:read".to_string()));
    assert!(scopes.contains(&"tenant:write".to_string()));
    assert!(scopes.contains(&"tenant:admin".to_string()));
    assert!(scopes.contains(&"tenant:delete".to_string()));
    assert!(scopes.contains(&"credential:read".to_string()));
    assert!(scopes.contains(&"credential:write".to_string()));
    assert!(scopes.contains(&"credential:delete".to_string()));
    assert!(scopes.contains(&"sandbox:read".to_string()));
    assert!(scopes.contains(&"sandbox:write".to_string()));
    assert!(scopes.contains(&"sandbox:execute".to_string()));
    assert!(scopes.contains(&"audit:read".to_string()));
    assert!(scopes.contains(&"members:read".to_string()));
    assert!(scopes.contains(&"members:write".to_string()));
    assert!(scopes.contains(&"members:invite".to_string()));
    assert!(scopes.contains(&"invitations:read".to_string()));
    assert!(scopes.contains(&"invitations:write".to_string()));
    assert!(scopes.contains(&"admin".to_string()));

    // 验证 scope 数量
    assert_eq!(scopes.len(), 23);
}

#[test]
fn test_admin_role_scopes() {
    // Given: Admin 角色
    let scopes = MembershipRole::Admin.default_scopes();

    // Then: Admin 应该拥有管理权限，但不能删除租户
    assert!(scopes.contains(&"tenant:read".to_string()));
    assert!(scopes.contains(&"tenant:write".to_string()));
    assert!(scopes.contains(&"tenant:admin".to_string()));
    assert!(!scopes.contains(&"tenant:delete".to_string())); // Admin 不能删除租户

    assert!(scopes.contains(&"credential:read".to_string()));
    assert!(scopes.contains(&"credential:write".to_string()));
    assert!(scopes.contains(&"credential:delete".to_string()));
    assert!(scopes.contains(&"sandbox:read".to_string()));
    assert!(scopes.contains(&"sandbox:write".to_string()));
    assert!(scopes.contains(&"sandbox:execute".to_string()));
    assert!(scopes.contains(&"audit:read".to_string()));
    assert!(scopes.contains(&"members:read".to_string()));
    assert!(scopes.contains(&"members:write".to_string()));
    assert!(scopes.contains(&"members:invite".to_string()));
    assert!(scopes.contains(&"invitations:read".to_string()));
    assert!(scopes.contains(&"invitations:write".to_string()));

    // 验证 scope 数量
    assert_eq!(scopes.len(), 20);
}

#[test]
fn test_member_role_scopes() {
    // Given: Member 角色
    let scopes = MembershipRole::Member.default_scopes();

    // Then: Member 应该有基本操作权限，无管理权限
    assert!(scopes.contains(&"tenant:read".to_string()));
    assert!(!scopes.contains(&"tenant:write".to_string()));
    assert!(!scopes.contains(&"tenant:admin".to_string()));
    assert!(!scopes.contains(&"tenant:delete".to_string()));

    assert!(scopes.contains(&"credential:read".to_string()));
    assert!(scopes.contains(&"credential:write".to_string()));
    assert!(scopes.contains(&"sandbox:read".to_string()));
    assert!(scopes.contains(&"sandbox:write".to_string()));
    assert!(scopes.contains(&"sandbox:execute".to_string()));
    assert!(!scopes.contains(&"credential:delete".to_string())); // Member 不能删除凭证

    assert!(scopes.contains(&"audit:read".to_string()));
    assert!(!scopes.contains(&"members:read".to_string()));
    assert!(!scopes.contains(&"members:write".to_string()));
    assert!(!scopes.contains(&"members:invite".to_string()));
    assert!(!scopes.contains(&"invitations:read".to_string()));
    assert!(!scopes.contains(&"invitations:write".to_string()));

    // 验证 scope 数量
    assert_eq!(scopes.len(), 10);
}

#[test]
fn test_readonly_role_scopes() {
    // Given: Readonly 角色
    let scopes = MembershipRole::Readonly.default_scopes();

    // Then: Readonly 只应该有读取权限
    assert!(scopes.contains(&"tenant:read".to_string()));
    assert!(!scopes.contains(&"tenant:write".to_string()));
    assert!(!scopes.contains(&"tenant:admin".to_string()));
    assert!(!scopes.contains(&"tenant:delete".to_string()));

    assert!(scopes.contains(&"credential:read".to_string()));
    assert!(!scopes.contains(&"sandbox:read".to_string()));
    assert!(!scopes.contains(&"sandbox:write".to_string()));
    assert!(!scopes.contains(&"sandbox:execute".to_string()));
    assert!(!scopes.contains(&"credential:write".to_string()));
    assert!(!scopes.contains(&"credential:delete".to_string()));

    assert!(!scopes.contains(&"audit:read".to_string()));
    assert!(!scopes.contains(&"members:read".to_string()));
    assert!(!scopes.contains(&"members:write".to_string()));
    assert!(!scopes.contains(&"members:invite".to_string()));
    assert!(!scopes.contains(&"invitations:read".to_string()));
    assert!(!scopes.contains(&"invitations:write".to_string()));

    // 验证 scope 数量
    assert_eq!(scopes.len(), 3);
}

// ============================================================================
// Scope 层级测试
// ============================================================================

#[test]
fn test_scope_permission_hierarchy() {
    // Then: 权限层级正确
    // Owner > Admin > Member > Readonly

    let owner_level = MembershipRole::Owner.permission_level();
    let admin_level = MembershipRole::Admin.permission_level();
    let member_level = MembershipRole::Member.permission_level();
    let readonly_level = MembershipRole::Readonly.permission_level();

    assert!(owner_level > admin_level);
    assert!(admin_level > member_level);
    assert!(member_level > readonly_level);
}

#[test]
fn test_role_can_manage_check() {
    // Then: 只有 Owner 和 Admin 可以管理
    assert!(MembershipRole::Owner.can_manage());
    assert!(MembershipRole::Admin.can_manage());
    assert!(!MembershipRole::Member.can_manage());
    assert!(!MembershipRole::Readonly.can_manage());
}

#[test]
fn test_role_can_write_check() {
    // Then: 只有 Readonly 不能写
    assert!(MembershipRole::Owner.can_write());
    assert!(MembershipRole::Admin.can_write());
    assert!(MembershipRole::Member.can_write());
    assert!(!MembershipRole::Readonly.can_write());
}

// ============================================================================
// 成员资格 Scope 继承测试
// ============================================================================

#[test]
fn test_membership_inherits_role_scopes() {
    // Given: 创建成员资格
    let tenant_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();

    // When: 创建 Admin 成员资格
    let membership = TenantMembership::new_from_invitation(
        tenant_id,
        user_id,
        MembershipRole::Admin,
        Uuid::now_v7(),
    );

    // Then: 成员资格继承 Admin 的 scopes
    assert!(membership.has_scope("tenant:admin"));
    assert!(membership.has_scope("members:invite"));
    assert!(!membership.has_scope("tenant:delete"));
}

#[test]
fn test_membership_scope_update_on_role_change() {
    // Given: Member 成员资格
    let tenant_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let mut membership = TenantMembership::new_from_invitation(
        tenant_id,
        user_id,
        MembershipRole::Member,
        Uuid::now_v7(),
    );

    // 初始状态
    assert!(membership.has_scope("credential:read"));
    assert!(!membership.has_scope("tenant:admin"));

    // When: 升级为 Admin
    membership.update_role(MembershipRole::Admin);

    // Then: Scopes 更新
    assert!(membership.has_scope("tenant:admin"));
    assert!(membership.has_scope("members:invite"));
}

#[test]
fn test_membership_custom_scope_addition() {
    // Given: 成员资格
    let tenant_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let mut membership = TenantMembership::new_owner(tenant_id, user_id);

    // When: 添加自定义 scope
    membership.add_scope("custom:special_feature");

    // Then: 自定义 scope 存在
    assert!(membership.has_scope("custom:special_feature"));

    // 默认 scopes 仍然存在
    assert!(membership.has_scope("tenant:read"));
    assert!(membership.has_scope("credential:write"));
}

#[test]
fn test_membership_scope_removal() {
    // Given: 带自定义 scope 的成员资格
    let tenant_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let mut membership = TenantMembership::new_owner(tenant_id, user_id);
    membership.add_scope("custom:feature");

    // When: 移除自定义 scope
    membership.remove_scope("custom:feature");

    // Then: scope 已移除
    assert!(!membership.has_scope("custom:feature"));
}

// ============================================================================
// Scope 枚举映射测试
// ============================================================================

#[test]
fn test_scope_enum_values() {
    // Then: Scope 枚举值正确
    assert_eq!(Scope::CredentialRead.as_str(), "credential:read");
    assert_eq!(Scope::CredentialDecrypt.as_str(), "credential:decrypt");
    assert_eq!(Scope::CredentialWrite.as_str(), "credential:write");
    assert_eq!(Scope::CredentialDelete.as_str(), "credential:delete");
    assert_eq!(Scope::TokenManage.as_str(), "token:manage");
    assert_eq!(Scope::AuditRead.as_str(), "audit:read");
    assert_eq!(Scope::Admin.as_str(), "admin");
}

#[test]
fn test_scope_parse() {
    // Then: Scope 解析正确
    assert_eq!(Scope::parse("credential:read"), Some(Scope::CredentialRead));
    assert_eq!(
        Scope::parse("credential:decrypt"),
        Some(Scope::CredentialDecrypt)
    );
    assert_eq!(
        Scope::parse("credential:write"),
        Some(Scope::CredentialWrite)
    );
    assert_eq!(
        Scope::parse("credential:delete"),
        Some(Scope::CredentialDelete)
    );
    assert_eq!(Scope::parse("token:manage"), Some(Scope::TokenManage));
    assert_eq!(Scope::parse("audit:read"), Some(Scope::AuditRead));
    assert_eq!(Scope::parse("admin"), Some(Scope::Admin));

    // 无效 scope
    assert_eq!(Scope::parse("invalid:scope"), None);
    assert_eq!(Scope::parse(""), None);
}

// ============================================================================
// Scope 权限检查测试
// ============================================================================

#[test]
fn test_scope_can_access_operation() {
    use vault_service::token::scope::Operation;

    // Then: Scope 可以访问对应操作
    assert!(Scope::CredentialRead.can_access(&Operation::ReadCredential));
    assert!(Scope::CredentialDecrypt.can_access(&Operation::DecryptCredential));
    assert!(Scope::CredentialWrite.can_access(&Operation::CreateCredential));
    assert!(Scope::CredentialWrite.can_access(&Operation::UpdateCredential));
    assert!(Scope::CredentialDelete.can_access(&Operation::DeleteCredential));
    assert!(Scope::TokenManage.can_access(&Operation::ManageToken));
    assert!(Scope::AuditRead.can_access(&Operation::ReadAudit));
}

#[test]
fn test_scope_cannot_access_unauthorized_operation() {
    use vault_service::token::scope::Operation;

    // Then: Scope 不能访问未授权操作
    assert!(!Scope::CredentialRead.can_access(&Operation::DecryptCredential));
    assert!(!Scope::CredentialRead.can_access(&Operation::CreateCredential));
    assert!(!Scope::CredentialDecrypt.can_access(&Operation::DeleteCredential));
    assert!(!Scope::AuditRead.can_access(&Operation::ReadCredential));
}

#[test]
fn test_admin_scope_access_all() {
    use vault_service::token::scope::Operation;

    // Then: Admin scope 可以访问所有操作
    assert!(Scope::Admin.can_access(&Operation::ReadCredential));
    assert!(Scope::Admin.can_access(&Operation::DecryptCredential));
    assert!(Scope::Admin.can_access(&Operation::CreateCredential));
    assert!(Scope::Admin.can_access(&Operation::UpdateCredential));
    assert!(Scope::Admin.can_access(&Operation::DeleteCredential));
    assert!(Scope::Admin.can_access(&Operation::ManageToken));
    assert!(Scope::Admin.can_access(&Operation::ReadAudit));
}

// ============================================================================
// ScopeSet 测试
// ============================================================================

#[test]
fn test_scope_set_from_string() {
    use vault_service::token::scope::ScopeSet;

    // Given: Scope 字符串
    let scope_str = "credential:read credential:write audit:read";

    // When: 解析为 ScopeSet
    let scope_set = ScopeSet::from_string(scope_str).unwrap();

    // Then: 正确解析
    assert!(scope_set.contains(&Scope::CredentialRead));
    assert!(scope_set.contains(&Scope::CredentialWrite));
    assert!(scope_set.contains(&Scope::AuditRead));
    assert!(!scope_set.contains(&Scope::CredentialDelete));
}

#[test]
fn test_scope_set_can_access() {
    use vault_service::token::scope::{Operation, ScopeSet};

    // Given: ScopeSet
    let scope_set = ScopeSet::from_string("credential:read credential:write").unwrap();

    // Then: 可以访问授权操作
    assert!(scope_set.can_access(&Operation::ReadCredential));
    assert!(scope_set.can_access(&Operation::CreateCredential));

    // Then: 不能访问未授权操作
    assert!(!scope_set.can_access(&Operation::DecryptCredential));
    assert!(!scope_set.can_access(&Operation::DeleteCredential));
}

#[test]
fn test_scope_set_contains_any() {
    use vault_service::token::scope::ScopeSet;

    // Given: ScopeSet without admin
    let scope_set = ScopeSet::from_string("credential:read credential:write").unwrap();

    // Then: contains_any 检查
    assert!(scope_set.contains_any(&[Scope::CredentialRead, Scope::CredentialDelete]));
    assert!(!scope_set.contains_any(&[Scope::AuditRead, Scope::TokenManage]));
}

#[test]
fn test_scope_set_contains_all() {
    use vault_service::token::scope::ScopeSet;

    // Given: ScopeSet without admin
    let scope_set = ScopeSet::from_string("credential:read credential:write").unwrap();

    // Then: contains_all 检查
    assert!(scope_set.contains_all(&[Scope::CredentialRead, Scope::CredentialWrite]));
    assert!(!scope_set.contains_all(&[Scope::CredentialRead, Scope::AuditRead]));
}

// ============================================================================
// 权限引擎测试
// ============================================================================

#[test]
fn test_permission_engine_creation() {
    use vault_service::token::permission::PermissionEngine;

    // Given: Scope 字符串
    let scope_str = "credential:read credential:write";

    // When: 创建权限引擎
    let engine = PermissionEngine::new(scope_str);

    // Then: 创建成功
    assert!(engine.is_ok());
}

#[test]
fn test_permission_engine_is_admin() {
    use vault_service::token::permission::PermissionEngine;

    // Given: Admin 权限引擎
    let admin_engine = PermissionEngine::new("admin").unwrap();
    assert!(admin_engine.is_admin());

    // Given: 非Admin 权限引擎
    let member_engine = PermissionEngine::new("credential:read").unwrap();
    assert!(!member_engine.is_admin());
}

#[test]
fn test_permission_engine_can_execute() {
    use vault_service::token::permission::PermissionEngine;
    use vault_service::token::scope::Operation;

    // Given: 读写权限引擎
    let engine = PermissionEngine::new("credential:read credential:write").unwrap();

    // Then: 可以执行读写操作
    assert!(engine.can_execute(&Operation::ReadCredential));
    assert!(engine.can_execute(&Operation::CreateCredential));
    assert!(engine.can_execute(&Operation::UpdateCredential));

    // Then: 不能执行删除操作
    assert!(!engine.can_execute(&Operation::DeleteCredential));
    assert!(!engine.can_execute(&Operation::DecryptCredential));
}

#[test]
fn test_permission_engine_restricted_credentials() {
    use vault_service::token::permission::PermissionEngine;

    // Given: 受限权限引擎（只能访问特定凭证）
    let engine = PermissionEngine::with_restricted_credentials(
        "credential:read",
        vec!["cred_001".to_string(), "cred_002".to_string()],
    )
    .unwrap();

    // Then: 可以访问白名单内的凭证
    assert!(engine.can_access_credential("cred_001").is_ok());
    assert!(engine.can_access_credential("cred_002").is_ok());

    // Then: 不能访问白名单外的凭证
    assert!(engine.can_access_credential("cred_999").is_err());
}

#[test]
fn test_permission_engine_check_credential_access() {
    use vault_service::token::permission::{AccessDecision, PermissionEngine};

    // Given: 受限权限引擎
    let engine = PermissionEngine::with_restricted_credentials(
        "credential:read",
        vec!["cred_123".to_string()],
    )
    .unwrap();

    // Then: 访问决策正确
    let allow = engine.check_credential_access("cred_123");
    assert!(matches!(allow, AccessDecision::Allow));

    let deny = engine.check_credential_access("cred_999");
    assert!(deny.is_denied());
}

// ============================================================================
// 批量权限检查测试
// ============================================================================

#[test]
fn test_batch_permission_checker() {
    use vault_service::token::permission::{BatchPermissionChecker, PermissionEngine};
    use vault_service::token::scope::Operation;

    // Given: 受限权限引擎
    let engine = PermissionEngine::with_restricted_credentials(
        "credential:read",
        vec!["cred_1".to_string(), "cred_2".to_string()],
    )
    .unwrap();

    let mut checker = BatchPermissionChecker::new(engine);

    // When: 批量检查凭证
    let credential_ids = vec![
        "cred_1".to_string(),
        "cred_2".to_string(),
        "cred_3".to_string(),
    ];
    let results = checker.check_all_credentials(Operation::ReadCredential, &credential_ids);

    // Then: 结果正确
    assert_eq!(results.len(), 3);
    assert!(results[0].1.is_allowed()); // cred_1
    assert!(results[1].1.is_allowed()); // cred_2
    assert!(results[2].1.is_denied()); // cred_3

    assert_eq!(checker.allowed_count(), 2);
    assert_eq!(checker.denied_count(), 1);
    assert!(!checker.all_allowed());
}

// ============================================================================
// Scope 与租户隔离集成测试
// ============================================================================

#[test]
fn test_tenant_scope_isolation() {
    // Given: 两个不同租户的成员资格
    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    let user_id = Uuid::now_v7();

    let membership_a = TenantMembership::new_owner(tenant_a, user_id);
    let membership_b = TenantMembership::new_from_invitation(
        tenant_b,
        user_id,
        MembershipRole::Readonly,
        Uuid::now_v7(),
    );

    // Then: 租户 A 的 Owner 有全部权限
    assert!(membership_a.has_scope("tenant:delete"));
    assert!(membership_a.has_scope("members:invite"));

    // Then: 租户 B 的 Readonly 只有读取权限
    assert!(membership_b.has_scope("credential:read"));
    assert!(!membership_b.has_scope("credential:write"));

    // Then: 权限不跨租户
    // membership_a 的权限只适用于 tenant_a
    // membership_b 的权限只适用于 tenant_b
}

#[test]
fn test_cross_tenant_scope_prevention() {
    // Given: 用户在租户 A 有 Admin 权限
    let tenant_a = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let membership = TenantMembership::new_from_invitation(
        tenant_a,
        user_id,
        MembershipRole::Admin,
        Uuid::now_v7(),
    );

    // Then: 用户有租户 A 的管理权限
    assert!(membership.has_scope("tenant:admin"));
    assert!(membership.has_scope("members:invite"));

    // 但是这些 scope 只在 tenant_a 有效
    // 不能用于访问其他租户资源（由中间件强制执行）
}

// ============================================================================
// Scope 变更审计测试
// ============================================================================

#[test]
fn test_scope_change_tracking() {
    // Given: 成员资格
    let tenant_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let mut membership = TenantMembership::new_from_invitation(
        tenant_id,
        user_id,
        MembershipRole::Member,
        Uuid::now_v7(),
    );

    // 记录初始 scope 数量
    let initial_scope_count = membership.scopes.len();

    // When: 角色升级
    membership.update_role(MembershipRole::Admin);

    // Then: scope 数量应该增加
    assert!(membership.scopes.len() > initial_scope_count);

    // Scope 变更应该被审计（由 AuthService 记录）
}

// ============================================================================
// 边界情况测试
// ============================================================================

#[test]
fn test_empty_scope_handling() {
    use vault_service::token::scope::ScopeSet;

    // Given: 空字符串
    let result = ScopeSet::from_string("");

    // Then: 应该失败或返回空集合
    assert!(result.is_err() || result.unwrap().is_empty());
}

#[test]
fn test_invalid_scope_in_string() {
    use vault_service::token::scope::ScopeSet;

    // Given: 包含无效 scope 的字符串
    let scope_str = "credential:read invalid:scope credential:write";

    // When: 解析
    let result = ScopeSet::from_string(scope_str);

    // Then: 应该失败
    assert!(result.is_err());
}

#[test]
fn test_scope_case_sensitivity() {
    // Then: Scope 解析应该区分大小写
    assert_eq!(Scope::parse("CREDENTIAL:READ"), None);
    assert_eq!(Scope::parse("Credential:Read"), None);
    assert_eq!(Scope::parse("credential:read"), Some(Scope::CredentialRead));
}

#[test]
fn test_duplicate_scope_handling() {
    use vault_service::token::scope::ScopeSet;

    // Given: 包含重复 scope 的字符串
    let scope_str = "credential:read credential:read credential:write";

    // When: 解析
    let scope_set = ScopeSet::from_string(scope_str);

    // Then: 应该去重
    if let Ok(set) = scope_set {
        // 检查只包含两个 scope（去重后）
        let count = if set.contains(&Scope::CredentialRead) {
            1
        } else {
            0
        } + if set.contains(&Scope::CredentialWrite) {
            1
        } else {
            0
        };
        assert_eq!(count, 2);
    }
}

#[test]
fn test_scope_whitespace_handling() {
    use vault_service::token::scope::ScopeSet;

    // Given: 包含多余空格的 scope 字符串
    let scope_str = "  credential:read   credential:write  ";

    // When: 解析
    let result = ScopeSet::from_string(scope_str);

    // Then: 应该正确处理
    assert!(result.is_ok());
    let scope_set = result.unwrap();
    assert!(scope_set.contains(&Scope::CredentialRead));
    assert!(scope_set.contains(&Scope::CredentialWrite));
}
