//! Token Scope 权限系统测试
//!
//! EP3-Story3.3: Token Scope 权限系统测试套件
//!
//! # 测试覆盖
//!
//! - Scope 枚举和字符串转换
//! - Operation 枚举
//! - Scope.can_access() 方法
//! - 受限 Token (credential_ids 白名单)
//! - PermissionEngine 权限检查
//! - 批量权限检查

use vault_service::token::permission::{
    AccessContext, AccessDecision, AccessRequest, BatchPermissionChecker, CredentialAccess,
    ExtendedRestrictedContext, PermissionEngine, PermissionPolicy, ResourceType,
};
use vault_service::token::scope::{
    Operation, PermissionChecker, RestrictedTokenContext, Scope, ScopeError, ScopeSet,
};
use vault_service::token::scope_constants;

// ============================================================================
// 基础 Scope 测试
// ============================================================================

#[test]
fn test_scope_enum_values() {
    // Given: Scope 枚举
    // When: 转换为字符串
    // Then: 返回正确的 scope 字符串
    assert_eq!(Scope::CredentialRead.as_str(), "credential:read");
    assert_eq!(Scope::CredentialDecrypt.as_str(), "credential:decrypt");
    assert_eq!(Scope::CredentialWrite.as_str(), "credential:write");
    assert_eq!(Scope::CredentialDelete.as_str(), "credential:delete");
    assert_eq!(Scope::TokenManage.as_str(), "token:manage");
    assert_eq!(Scope::AuditRead.as_str(), "audit:read");
    assert_eq!(Scope::Admin.as_str(), "admin");
}

#[test]
fn test_scope_from_str_valid() {
    // Given: 有效的 scope 字符串
    // When: 解析为 Scope
    // Then: 返回正确的 Scope 枚举
    assert_eq!(Scope::from_str("credential:read"), Some(Scope::CredentialRead));
    assert_eq!(Scope::from_str("credential:decrypt"), Some(Scope::CredentialDecrypt));
    assert_eq!(Scope::from_str("credential:write"), Some(Scope::CredentialWrite));
    assert_eq!(Scope::from_str("credential:delete"), Some(Scope::CredentialDelete));
    assert_eq!(Scope::from_str("token:manage"), Some(Scope::TokenManage));
    assert_eq!(Scope::from_str("audit:read"), Some(Scope::AuditRead));
    assert_eq!(Scope::from_str("admin"), Some(Scope::Admin));
}

#[test]
fn test_scope_from_str_invalid() {
    // Given: 无效的 scope 字符串
    // When: 解析为 Scope
    // Then: 返回 None
    assert_eq!(Scope::from_str("invalid:scope"), None);
    assert_eq!(Scope::from_str(""), None);
    assert_eq!(Scope::from_str("credential"), None);
    assert_eq!(Scope::from_str("read"), None);
}

#[test]
fn test_scope_display() {
    // Given: Scope 枚举
    // When: 格式化为字符串
    // Then: 返回正确的格式
    assert_eq!(format!("{}", Scope::CredentialRead), "credential:read");
    assert_eq!(format!("{}", Scope::Admin), "admin");
}

// ============================================================================
// Scope.can_access() 方法测试
// ============================================================================

#[test]
fn test_credential_read_can_access_read_credential() {
    // Given: credential:read scope
    let scope = Scope::CredentialRead;

    // When: 检查是否可以访问 ReadCredential 操作
    let can_access = scope.can_access(&Operation::ReadCredential);

    // Then: 允许访问
    assert!(can_access, "credential:read 应该允许 ReadCredential 操作");
}

#[test]
fn test_credential_read_cannot_access_decrypt() {
    // Given: credential:read scope
    let scope = Scope::CredentialRead;

    // When: 检查是否可以访问 DecryptCredential 操作
    let can_access = scope.can_access(&Operation::DecryptCredential);

    // Then: 拒绝访问
    assert!(!can_access, "credential:read 不应该允许 DecryptCredential 操作");
}

#[test]
fn test_credential_decrypt_can_access_decrypt_and_read() {
    // Given: credential:decrypt scope
    let scope = Scope::CredentialDecrypt;

    // When: 检查可以访问的操作
    let can_decrypt = scope.can_access(&Operation::DecryptCredential);
    let can_read = scope.can_access(&Operation::ReadCredential);

    // Then: 允许访问 decrypt 和 read 操作
    assert!(can_decrypt, "credential:decrypt 应该允许 DecryptCredential 操作");
    assert!(can_read, "credential:decrypt 应该隐式允许 ReadCredential 操作");
}

#[test]
fn test_credential_write_can_access_write_operations() {
    // Given: credential:write scope
    let scope = Scope::CredentialWrite;

    // When: 检查可以访问的操作
    let can_create = scope.can_access(&Operation::CreateCredential);
    let can_update = scope.can_access(&Operation::UpdateCredential);

    // Then: 允许创建和更新
    assert!(can_create, "credential:write 应该允许 CreateCredential 操作");
    assert!(can_update, "credential:write 应该允许 UpdateCredential 操作");
}

#[test]
fn test_credential_delete_can_access_delete() {
    // Given: credential:delete scope
    let scope = Scope::CredentialDelete;

    // When: 检查是否可以删除凭证
    let can_delete = scope.can_access(&Operation::DeleteCredential);

    // Then: 允许删除
    assert!(can_delete, "credential:delete 应该允许 DeleteCredential 操作");
}

#[test]
fn test_admin_can_access_all_operations() {
    // Given: admin scope
    let scope = Scope::Admin;

    // When/Then: 检查所有操作都允许
    assert!(scope.can_access(&Operation::ReadCredential), "admin 应该允许 ReadCredential");
    assert!(scope.can_access(&Operation::DecryptCredential), "admin 应该允许 DecryptCredential");
    assert!(scope.can_access(&Operation::CreateCredential), "admin 应该允许 CreateCredential");
    assert!(scope.can_access(&Operation::UpdateCredential), "admin 应该允许 UpdateCredential");
    assert!(scope.can_access(&Operation::DeleteCredential), "admin 应该允许 DeleteCredential");
    assert!(scope.can_access(&Operation::ManageToken), "admin 应该允许 ManageToken");
    assert!(scope.can_access(&Operation::RevokeToken), "admin 应该允许 RevokeToken");
    assert!(scope.can_access(&Operation::RefreshToken), "admin 应该允许 RefreshToken");
    assert!(scope.can_access(&Operation::ReadAudit), "admin 应该允许 ReadAudit");
}

#[test]
fn test_token_manage_can_access_token_operations() {
    // Given: token:manage scope
    let scope = Scope::TokenManage;

    // When: 检查 Token 相关操作
    let can_manage = scope.can_access(&Operation::ManageToken);
    let can_revoke = scope.can_access(&Operation::RevokeToken);
    let can_refresh = scope.can_access(&Operation::RefreshToken);

    // Then: 允许所有 Token 操作
    assert!(can_manage, "token:manage 应该允许 ManageToken 操作");
    assert!(can_revoke, "token:manage 应该允许 RevokeToken 操作");
    assert!(can_refresh, "token:manage 应该允许 RefreshToken 操作");
}

#[test]
fn test_audit_read_can_access_read_audit() {
    // Given: audit:read scope
    let scope = Scope::AuditRead;

    // When: 检查是否可以读取审计日志
    let can_read = scope.can_access(&Operation::ReadAudit);

    // Then: 允许读取审计日志
    assert!(can_read, "audit:read 应该允许 ReadAudit 操作");
}

// ============================================================================
// ScopeSet 测试
// ============================================================================

#[test]
fn test_scope_set_from_string_valid() {
    // Given: 有效的 scope 字符串
    let scope_str = "credential:read credential:write";

    // When: 解析为 ScopeSet
    let scope_set = ScopeSet::from_string(scope_str).unwrap();

    // Then: 正确解析所有 scope
    assert!(scope_set.contains(&Scope::CredentialRead));
    assert!(scope_set.contains(&Scope::CredentialWrite));
    assert!(!scope_set.contains(&Scope::CredentialDelete));
}

#[test]
fn test_scope_set_from_string_invalid() {
    // Given: 包含无效 scope 的字符串
    let scope_str = "credential:read invalid:scope";

    // When: 解析为 ScopeSet
    let result = ScopeSet::from_string(scope_str);

    // Then: 返回错误
    assert!(matches!(result, Err(ScopeError::ParseError(_))));
}

#[test]
fn test_scope_set_can_access_with_multiple_scopes() {
    // Given: 包含多个 scope 的 ScopeSet
    let scope_set = ScopeSet::from_string("credential:read credential:write").unwrap();

    // When/Then: 检查各种操作的访问权限
    assert!(scope_set.can_access(&Operation::ReadCredential));
    assert!(scope_set.can_access(&Operation::CreateCredential));
    assert!(!scope_set.can_access(&Operation::DeleteCredential));
}

#[test]
fn test_scope_set_contains_any() {
    // Given: ScopeSet
    let scope_set = ScopeSet::from_string("credential:read token:manage").unwrap();

    // When/Then: 检查是否包含任意指定 scope
    assert!(scope_set.contains_any(&[Scope::CredentialRead, Scope::Admin]));
    assert!(scope_set.contains_any(&[Scope::Admin, Scope::TokenManage]));
    assert!(!scope_set.contains_any(&[Scope::Admin, Scope::CredentialDelete]));
}

#[test]
fn test_scope_set_contains_all() {
    // Given: ScopeSet
    let scope_set = ScopeSet::from_string("credential:read credential:write").unwrap();

    // When/Then: 检查是否包含所有指定 scope
    assert!(scope_set.contains_all(&[Scope::CredentialRead]));
    assert!(scope_set.contains_all(&[Scope::CredentialRead, Scope::CredentialWrite]));
    assert!(!scope_set.contains_all(&[Scope::CredentialRead, Scope::Admin]));
}

// ============================================================================
// 受限 Token 测试 (credential_ids 白名单)
// ============================================================================

#[test]
fn test_restricted_token_unrestricted() {
    // Given: 无限制的 Token 上下文
    let context = RestrictedTokenContext::unrestricted();

    // When: 检查任何凭证的访问权限
    let result1 = context.can_access_credential("cred_123");
    let result2 = context.can_access_credential("any_credential_id");

    // Then: 允许访问所有凭证
    assert!(result1.is_ok(), "无限制 Token 应该允许访问任何凭证");
    assert!(result2.is_ok(), "无限制 Token 应该允许访问任何凭证");
    assert!(!context.is_restricted());
}

#[test]
fn test_restricted_token_with_whitelist() {
    // Given: 受限 Token（只能访问 cred_123 和 cred_456）
    let allowed_ids = vec!["cred_123".to_string(), "cred_456".to_string()];
    let context = RestrictedTokenContext::restricted(allowed_ids);

    // When: 检查白名单内凭证
    let result_allowed = context.can_access_credential("cred_123");

    // Then: 允许访问
    assert!(result_allowed.is_ok(), "应该允许访问白名单内的凭证");

    // When: 检查白名单外凭证
    let result_denied = context.can_access_credential("cred_999");

    // Then: 拒绝访问并返回 AccessDenied 错误
    assert!(result_denied.is_err(), "应该拒绝访问白名单外的凭证");
    match result_denied {
        Err(ScopeError::AccessDenied { credential_id }) => {
            assert_eq!(credential_id, "cred_999");
        }
        _ => panic!("期望 AccessDenied 错误"),
    }
    assert!(context.is_restricted());
}

#[test]
fn test_restricted_token_allowed_credentials() {
    // Given: 受限 Token
    let allowed_ids = vec!["cred_123".to_string(), "cred_456".to_string()];
    let context = RestrictedTokenContext::restricted(allowed_ids.clone());

    // When: 获取允许的凭证列表
    let allowed = context.allowed_credentials();

    // Then: 返回正确的列表
    assert_eq!(allowed, Some(allowed_ids.as_slice()));
}

// ============================================================================
// PermissionChecker 测试
// ============================================================================

#[test]
fn test_permission_checker_can_execute() {
    // Given: PermissionChecker 带有 credential:read scope
    let scope_set = ScopeSet::from_string("credential:read").unwrap();
    let checker = PermissionChecker::new(scope_set);

    // When/Then: 检查操作权限
    assert!(checker.can_execute(&Operation::ReadCredential));
    assert!(!checker.can_execute(&Operation::CreateCredential));
}

#[test]
fn test_permission_checker_check_permission() {
    // Given: PermissionChecker 带有 credential:read scope 且无限制
    let scope_set = ScopeSet::from_string("credential:read").unwrap();
    let checker = PermissionChecker::new(scope_set);

    // When: 检查读权限
    let result = checker.check_permission(&Operation::ReadCredential, Some("any_cred"));

    // Then: 允许
    assert!(result.is_ok());
}

#[test]
fn test_permission_checker_insufficient_scope() {
    // Given: PermissionChecker 只有 read 权限
    let scope_set = ScopeSet::from_string("credential:read").unwrap();
    let checker = PermissionChecker::new(scope_set);

    // When: 尝试执行写操作
    let result = checker.check_permission(&Operation::CreateCredential, None);

    // Then: 返回权限不足错误
    assert!(result.is_err());
    match result {
        Err(ScopeError::InsufficientScope { required, actual }) => {
            assert_eq!(required, "credential:write");
            assert!(actual.contains("credential:read"));
        }
        _ => panic!("期望 InsufficientScope 错误"),
    }
}

#[test]
fn test_permission_checker_restricted_token() {
    // Given: PermissionChecker 带有 credential:read scope 且受限
    let scope_set = ScopeSet::from_string("credential:read").unwrap();
    let restricted = RestrictedTokenContext::restricted(vec!["cred_123".to_string()]);
    let checker = PermissionChecker::with_restriction(scope_set, restricted);

    // When: 访问允许的凭证
    let result_allowed = checker.check_permission(&Operation::ReadCredential, Some("cred_123"));

    // Then: 允许
    assert!(result_allowed.is_ok());

    // When: 访问不允许的凭证
    let result_denied = checker.check_permission(&Operation::ReadCredential, Some("cred_999"));

    // Then: 拒绝
    assert!(result_denied.is_err());
    match result_denied {
        Err(ScopeError::AccessDenied { credential_id }) => {
            assert_eq!(credential_id, "cred_999");
        }
        _ => panic!("期望 AccessDenied 错误"),
    }
}

// ============================================================================
// PermissionEngine 测试
// ============================================================================

#[test]
fn test_permission_engine_new() {
    // Given/When: 创建 PermissionEngine
    let engine = PermissionEngine::new("credential:read credential:write");

    // Then: 创建成功
    assert!(engine.is_ok());

    let engine = PermissionEngine::new("invalid:scope");
    assert!(engine.is_err());
}

#[test]
fn test_permission_engine_is_restricted() {
    // Given: 受限的 PermissionEngine
    let engine = PermissionEngine::with_restricted_credentials(
        "credential:read",
        vec!["cred_123".to_string()],
    )
    .unwrap();

    // Then: 识别为受限
    assert!(engine.is_restricted());

    // Given: 无限制的 PermissionEngine
    let engine = PermissionEngine::new("credential:read").unwrap();

    // Then: 识别为非受限
    assert!(!engine.is_restricted());
}

#[test]
fn test_permission_engine_check_credential_access() {
    // Given: 受限的 PermissionEngine
    let engine = PermissionEngine::with_restricted_credentials(
        "credential:read",
        vec!["cred_123".to_string()],
    )
    .unwrap();

    // When: 访问白名单内凭证
    let decision = engine.check_credential_access("cred_123");

    // Then: 允许
    assert!(matches!(decision, AccessDecision::Allow));

    // When: 访问白名单外凭证
    let decision = engine.check_credential_access("cred_999");

    // Then: 拒绝
    assert!(decision.is_denied());
}

#[test]
fn test_permission_engine_check_access_with_high_risk() {
    // Given: PermissionEngine
    let engine = PermissionEngine::new("credential:read").unwrap();

    // When: 创建高风险请求
    let request = AccessRequest::credential(Operation::ReadCredential, "cred_123")
        .with_context(AccessContext::new().with_risk_level(2));

    // Then: 需要审批
    let decision = engine.check_access(&request);
    assert!(decision.requires_approval());
}

#[test]
fn test_permission_engine_is_admin() {
    // Given: admin scope
    let engine = PermissionEngine::new("admin").unwrap();
    assert!(engine.is_admin());

    // Given: 非 admin scope
    let engine = PermissionEngine::new("credential:read").unwrap();
    assert!(!engine.is_admin());
}

#[test]
fn test_permission_engine_credential_access_trait() {
    // Given: 受限 PermissionEngine
    let engine = PermissionEngine::with_restricted_credentials(
        "credential:read",
        vec!["cred_123".to_string()],
    )
    .unwrap();

    // When/Then: 使用 trait 方法检查
    assert!(engine.can_access_credential("cred_123").is_ok());
    assert!(engine.can_access_credential("cred_999").is_err());

    assert!(engine
        .can_execute_on_credential(&Operation::ReadCredential, "cred_123")
        .is_ok());

    // 没有写权限
    assert!(engine
        .can_execute_on_credential(&Operation::CreateCredential, "cred_123")
        .is_err());
}

// ============================================================================
// BatchPermissionChecker 测试
// ============================================================================

#[test]
fn test_batch_permission_checker() {
    // Given: 受限 PermissionEngine
    let engine = PermissionEngine::with_restricted_credentials(
        "credential:read",
        vec!["cred_1".to_string(), "cred_2".to_string()],
    )
    .unwrap();

    let mut checker = BatchPermissionChecker::new(engine);

    // When: 批量检查多个凭证
    let credential_ids = vec!["cred_1".to_string(), "cred_2".to_string(), "cred_3".to_string()];
    let results = checker.check_all_credentials(Operation::ReadCredential, &credential_ids);

    // Then: cred_1 和 cred_2 允许，cred_3 拒绝
    assert_eq!(results.len(), 3);
    assert!(results[0].1.is_allowed()); // cred_1
    assert!(results[1].1.is_allowed()); // cred_2
    assert!(results[2].1.is_denied()); // cred_3

    assert_eq!(checker.allowed_count(), 2);
    assert_eq!(checker.denied_count(), 1);
    assert!(!checker.all_allowed());
}

#[test]
fn test_batch_permission_checker_all_allowed() {
    // Given: 无限制 PermissionEngine
    let engine = PermissionEngine::new("credential:read").unwrap();
    let mut checker = BatchPermissionChecker::new(engine);

    // When: 检查凭证
    let credential_ids = vec!["cred_1".to_string(), "cred_2".to_string()];
    checker.check_all_credentials(Operation::ReadCredential, &credential_ids);

    // Then: 全部允许
    assert!(checker.all_allowed());
    assert_eq!(checker.denied_count(), 0);
}

// ============================================================================
// ExtendedRestrictedContext 测试
// ============================================================================

#[test]
fn test_extended_restricted_context_credentials() {
    // Given: 带凭证限制的上下文
    let context = ExtendedRestrictedContext::new()
        .with_credentials(vec!["cred_123".to_string(), "cred_456".to_string()]);

    // When/Then: 检查凭证访问
    assert!(context.can_access_credential("cred_123").is_ok());
    assert!(context.can_access_credential("cred_999").is_err());
}

#[test]
fn test_extended_restricted_context_operations() {
    // Given: 带操作限制的上下文
    let context =
        ExtendedRestrictedContext::new().with_operations(vec![Operation::ReadCredential]);

    // When/Then: 检查操作执行
    assert!(context.can_execute(&Operation::ReadCredential).is_ok());
    assert!(context.can_execute(&Operation::CreateCredential).is_err());
}

#[test]
fn test_extended_restricted_context_rate_limit() {
    // Given: 带请求限制的上下文
    let mut context = ExtendedRestrictedContext::new().with_max_requests(2);

    // When/Then: 检查请求限制
    assert!(context.check_rate_limit().is_ok()); // 第1次
    assert!(context.check_rate_limit().is_ok()); // 第2次
    assert!(context.check_rate_limit().is_err()); // 第3次 - 超出限制

    assert_eq!(context.remaining_requests(), Some(0));
}

// ============================================================================
// PermissionPolicy 测试
// ============================================================================

#[test]
fn test_permission_policy_default() {
    let policy = PermissionPolicy::default();
    assert!(policy.enable_resource_restriction);
    assert!(policy.enable_risk_check);
    assert_eq!(policy.high_risk_threshold, 2);
    assert!(policy.audit_logging);
}

#[test]
fn test_permission_policy_permissive() {
    let policy = PermissionPolicy::permissive();
    assert!(!policy.enable_resource_restriction);
    assert!(!policy.enable_risk_check);
    assert_eq!(policy.high_risk_threshold, 3);
    assert!(!policy.audit_logging);
}

#[test]
fn test_permission_policy_strict() {
    let policy = PermissionPolicy::strict();
    assert!(policy.enable_resource_restriction);
    assert!(policy.enable_risk_check);
    assert_eq!(policy.high_risk_threshold, 1);
    assert!(policy.audit_logging);
}

// ============================================================================
// Scope 常量测试 (向后兼容)
// ============================================================================

#[test]
fn test_scope_constants() {
    // 验证常量与枚举值一致
    assert_eq!(scope_constants::CREDENTIAL_READ, Scope::CredentialRead);
    assert_eq!(scope_constants::CREDENTIAL_DECRYPT, Scope::CredentialDecrypt);
    assert_eq!(scope_constants::CREDENTIAL_WRITE, Scope::CredentialWrite);
    assert_eq!(scope_constants::CREDENTIAL_DELETE, Scope::CredentialDelete);
    assert_eq!(scope_constants::TOKEN_MANAGE, Scope::TokenManage);
    assert_eq!(scope_constants::AUDIT_READ, Scope::AuditRead);
    assert_eq!(scope_constants::ADMIN, Scope::Admin);
}

// ============================================================================
// 集成测试
// ============================================================================

#[test]
fn test_full_permission_flow() {
    // 场景：创建受限 Token，验证权限检查流程

    // Step 1: 创建 ScopeSet
    let scope_set = ScopeSet::from_string("credential:read credential:decrypt").unwrap();

    // Step 2: 创建受限上下文
    let restricted_context =
        RestrictedTokenContext::restricted(vec!["cred_123".to_string(), "cred_456".to_string()]);

    // Step 3: 创建 PermissionChecker
    let checker = PermissionChecker::with_restriction(scope_set, restricted_context);

    // Step 4: 验证读权限
    assert!(checker.can_execute(&Operation::ReadCredential));

    // Step 5: 验证解密权限
    assert!(checker.can_execute(&Operation::DecryptCredential));

    // Step 6: 验证不允许写
    assert!(!checker.can_execute(&Operation::CreateCredential));

    // Step 7: 验证受限凭证访问
    assert!(checker.can_access_credential("cred_123").is_ok());
    assert!(checker.can_access_credential("cred_999").is_err());

    // Step 8: 综合权限检查
    assert!(checker
        .check_permission(&Operation::ReadCredential, Some("cred_123"))
        .is_ok());
    assert!(checker
        .check_permission(&Operation::DecryptCredential, Some("cred_456"))
        .is_ok());

    // 尝试访问不允许的凭证
    let result = checker.check_permission(&Operation::ReadCredential, Some("cred_999"));
    assert!(matches!(result, Err(ScopeError::AccessDenied { .. })));
}

#[test]
fn test_token_with_multiple_scopes_and_restrictions() {
    // 场景：Token 拥有多个 Scope，且受限
    let engine = PermissionEngine::with_restricted_credentials(
        "credential:read credential:write token:manage",
        vec!["cred_admin".to_string()],
    )
    .unwrap();

    // 验证多种操作权限
    assert!(engine.can_execute(&Operation::ReadCredential));
    assert!(engine.can_execute(&Operation::CreateCredential));
    assert!(engine.can_execute(&Operation::ManageToken));

    // 但凭证访问受限
    assert!(engine.check_credential_access("cred_admin").is_allowed());
    assert!(engine.check_credential_access("cred_user").is_denied());
}

#[test]
fn test_access_decision_variants() {
    // 测试 AccessDecision 的各种变体
    let allow = AccessDecision::Allow;
    assert!(allow.is_allowed());
    assert!(!allow.is_denied());
    assert!(!allow.requires_approval());

    let deny = AccessDecision::Deny("test reason".to_string());
    assert!(!deny.is_allowed());
    assert!(deny.is_denied());
    assert_eq!(deny.denial_reason(), Some("test reason"));

    let approval = AccessDecision::RequireApproval("需要审批".to_string());
    assert!(!approval.is_allowed());
    assert!(approval.requires_approval());
}

#[test]
fn test_scope_helper_methods() {
    // 测试 Scope 辅助方法
    assert!(Scope::CredentialRead.is_credential_scope());
    assert!(Scope::CredentialDecrypt.is_credential_scope());
    assert!(Scope::CredentialWrite.is_credential_scope());
    assert!(Scope::CredentialDelete.is_credential_scope());
    assert!(Scope::Admin.is_credential_scope());

    assert!(!Scope::TokenManage.is_credential_scope());
    assert!(!Scope::AuditRead.is_credential_scope());

    assert!(Scope::Admin.is_admin());
    assert!(!Scope::CredentialRead.is_admin());
}

#[test]
fn test_operation_helper_methods() {
    // 测试 Operation 辅助方法
    assert!(Operation::ReadCredential.is_credential_operation());
    assert!(Operation::DecryptCredential.is_credential_operation());
    assert!(Operation::CreateCredential.is_credential_operation());
    assert!(Operation::UpdateCredential.is_credential_operation());
    assert!(Operation::DeleteCredential.is_credential_operation());

    assert!(!Operation::ManageToken.is_credential_operation());
    assert!(!Operation::ReadAudit.is_credential_operation());

    // 测试 required_scope
    assert_eq!(
        Operation::ReadCredential.required_scope(),
        Scope::CredentialRead
    );
    assert_eq!(
        Operation::DecryptCredential.required_scope(),
        Scope::CredentialDecrypt
    );
    assert_eq!(
        Operation::CreateCredential.required_scope(),
        Scope::CredentialWrite
    );
}

#[test]
fn test_access_request_builder() {
    // 测试 AccessRequest 构建器
    let request = AccessRequest::credential(Operation::ReadCredential, "cred_123")
        .with_context(AccessContext::new().with_ip("192.168.1.1").with_risk_level(1));

    assert_eq!(request.resource_type, ResourceType::Credential);
    assert_eq!(request.resource_id, "cred_123");
    assert_eq!(request.operation, Operation::ReadCredential);
    assert_eq!(request.context.client_ip, Some("192.168.1.1".to_string()));
    assert_eq!(request.context.risk_level, Some(1));
}

#[test]
fn test_token_access_request() {
    let request = AccessRequest::token(Operation::RevokeToken, "token_abc");

    assert_eq!(request.resource_type, ResourceType::Token);
    assert_eq!(request.resource_id, "token_abc");
    assert_eq!(request.operation, Operation::RevokeToken);
}
