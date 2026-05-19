#![allow(clippy::field_reassign_with_default)]
#![allow(dead_code)]
#![allow(clippy::uninlined_format_args)]
//! 租户隔离中间件集成测试
//!
//! 测试内容：
//! - 从 Token 中提取 tenant_id
//! - 验证租户隔离
//! - 跨租户访问拦截
//! - 请求上下文注入

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use std::time::{SystemTime, UNIX_EPOCH};
use tower::ServiceExt;
use uuid::Uuid;

use vault_service::api::{
    context::{RequestContext, TenantId},
    middleware::{TokenScope, ValidatedToken},
    tenant_middleware::{
        RequestContextExt, TenantIsolationConfig, TenantIsolationState,
        tenant_isolation_middleware, validate_path_tenant_id,
    },
};

/// 创建测试用的 ValidatedToken
fn create_test_token(tenant_id: &str, user_id: &str, scopes: Vec<TokenScope>) -> ValidatedToken {
    ValidatedToken {
        token_id: format!("token_{}", uuid::Uuid::now_v7()),
        subject: format!("{}:{}", tenant_id, user_id),
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
        metadata: std::collections::HashMap::new(),
        subject_type: vault_service::token::TOKEN_SUBJECT_TYPE_USER.to_string(),
        issued_from: vault_service::token::TOKEN_ISSUED_FROM_SESSION.to_string(),
        token_plane: "management".to_string(),
        allowed_credential_ids: None,
        allowed_binding_handles: None,
    }
}

/// 创建测试路由
fn create_test_router() -> Router {
    let config = TenantIsolationConfig::testing();
    let state = TenantIsolationState::new(config);

    Router::new()
        .route("/api/v1/credentials", get(test_handler))
        .route(
            "/api/v1/tenants/:tenant_id/credentials",
            get(tenant_path_handler),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            state,
            tenant_isolation_middleware,
        ))
}

/// 测试处理器 - 返回请求上下文信息
async fn test_handler(request: Request<Body>) -> axum::Json<serde_json::Value> {
    let context = request.context();
    axum::Json(serde_json::json!({
        "has_context": context.is_some(),
        "tenant_id": context.map(|c| c.tenant_id().to_string()),
        "user_id": context.map(|c| c.user_id().to_string()),
    }))
}

/// 测试处理器 - 带租户路径参数
async fn tenant_path_handler(
    axum::extract::Path(path_tenant_id): axum::extract::Path<String>,
    axum::extract::Extension(context): axum::extract::Extension<RequestContext>,
) -> Result<axum::Json<serde_json::Value>, StatusCode> {
    // 验证路径参数中的租户ID
    match validate_path_tenant_id(&context, &path_tenant_id) {
        Ok(_) => Ok(axum::Json(serde_json::json!({
            "success": true,
            "tenant_id": context.tenant_id(),
            "path_tenant_id": path_tenant_id,
        }))),
        Err(_) => Err(StatusCode::FORBIDDEN),
    }
}

#[tokio::test]
async fn test_tenant_context_extraction_from_token() {
    let app = create_test_router();

    // 创建包含 ValidatedToken 的请求
    let token = create_test_token(
        "tenant_abc123",
        "user_xyz789",
        vec![TokenScope::CredentialRead],
    );
    let (mut parts, body) = Request::builder()
        .uri("/api/v1/credentials")
        .body(Body::empty())
        .unwrap()
        .into_parts();

    // 将 Token 添加到扩展
    parts.extensions.insert(token);
    let request = Request::from_parts(parts, body);

    // 发送请求
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_cross_tenant_access_blocked() {
    let app = create_test_router();

    // 创建 Token，租户ID 为 tenant_abc123
    let token = create_test_token(
        "tenant_abc123",
        "user_xyz789",
        vec![TokenScope::CredentialRead],
    );

    // 但请求路径中尝试访问 tenant_different 的资源
    let request = Request::builder()
        .uri("/api/v1/tenants/tenant_different/credentials")
        .body(Body::empty())
        .unwrap();

    let (mut parts, body) = request.into_parts();
    parts.extensions.insert(token);
    let request = Request::from_parts(parts, body);

    // 发送请求
    let response = app.oneshot(request).await.unwrap();

    // 应该返回 403 Forbidden
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_same_tenant_access_allowed() {
    let app = create_test_router();

    // 创建 Token，租户ID 为 tenant_abc123
    let token = create_test_token(
        "tenant_abc123",
        "user_xyz789",
        vec![TokenScope::CredentialRead],
    );

    // 请求相同租户的资源
    let request = Request::builder()
        .uri("/api/v1/tenants/tenant_abc123/credentials")
        .body(Body::empty())
        .unwrap();

    let (mut parts, body) = request.into_parts();
    parts.extensions.insert(token);
    let request = Request::from_parts(parts, body);

    // 发送请求
    let response = app.oneshot(request).await.unwrap();

    // 应该返回 200 OK
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_request_context_extensions() {
    let token = create_test_token(
        "tenant_test",
        "user_test",
        vec![TokenScope::CredentialRead, TokenScope::Admin],
    );
    let context = RequestContext::from_validated_token(&token);

    // 验证上下文内容
    assert_eq!(context.tenant_id(), "tenant_test");
    assert_eq!(context.user_id(), "user_test");
    assert!(context.has_scope("credential:read"));
    assert!(context.has_scope("admin"));
}

#[tokio::test]
async fn test_tenant_id_extractor() {
    use axum::extract::FromRequestParts;

    // 创建带上下文的 Parts
    let token = create_test_token(
        "tenant_extractor_test",
        "user_test",
        vec![TokenScope::CredentialRead],
    );
    let context = RequestContext::from_validated_token(&token);

    let request = Request::builder().uri("/test").body(Body::empty()).unwrap();

    let (mut parts, _body) = request.into_parts();
    parts.extensions.insert(context);

    // 提取 TenantId
    let tenant_id = TenantId::from_request_parts(&mut parts, &()).await.unwrap();

    assert_eq!(tenant_id.as_str(), "tenant_extractor_test");
}

#[tokio::test]
async fn test_validate_path_tenant_id_success() {
    let token = create_test_token(
        "tenant_match",
        "user_test",
        vec![TokenScope::CredentialRead],
    );
    let context = RequestContext::from_validated_token(&token);

    // 匹配的租户ID应该通过
    let result = validate_path_tenant_id(&context, "tenant_match");
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_validate_path_tenant_id_failure() {
    let token = create_test_token("tenant_a", "user_test", vec![TokenScope::CredentialRead]);
    let context = RequestContext::from_validated_token(&token);

    // 不匹配的租户ID应该失败
    let result = validate_path_tenant_id(&context, "tenant_b");
    assert!(result.is_err());

    if let Err(e) = result {
        let msg = e.to_string();
        assert!(msg.contains("tenant_a"));
        assert!(msg.contains("tenant_b"));
    }
}

#[tokio::test]
async fn test_public_paths_bypass_tenant_check() {
    // 公开路径应该跳过租户检查
    let config = TenantIsolationConfig::testing();
    assert!(config.is_public_path("/health"));
    assert!(config.is_public_path("/api/v1/health"));
    assert!(!config.is_public_path("/api/v1/credentials"));
}

#[tokio::test]
async fn test_request_context_scope_check() {
    let token = create_test_token(
        "tenant_test",
        "user_test",
        vec![TokenScope::CredentialRead, TokenScope::CredentialWrite],
    );
    let context = RequestContext::from_validated_token(&token);

    // 检查存在的 scope
    assert!(context.has_scope("credential:read"));
    assert!(context.has_scope("credential:write"));

    // 检查不存在的 scope
    assert!(!context.has_scope("admin"));
    assert!(!context.has_scope("audit:read"));
}

#[tokio::test]
async fn test_admin_scope_grants_all() {
    // 只有 admin scope
    let token = create_test_token("tenant_admin", "user_admin", vec![TokenScope::Admin]);
    let context = RequestContext::from_validated_token(&token);

    // Admin 应该通过所有 scope 检查
    assert!(context.has_scope("credential:read"));
    assert!(context.has_scope("credential:write"));
    assert!(context.has_scope("admin"));
}

#[tokio::test]
async fn test_rls_context_sql_generation() {
    use vault_service::api::context::RlsContext;

    let ctx = RlsContext {
        tenant_id: "tenant_123".to_string(),
        user_id: "user_456".to_string(),
        membership_id: None,
        scopes: vec!["read".to_string(), "write".to_string()],
        is_admin: false,
    };

    let statements = ctx.to_sql_statements();

    assert_eq!(statements.len(), 4);
    assert!(statements[0].contains("SET app.current_tenant_id = 'tenant_123'"));
    assert!(statements[1].contains("SET app.current_user_id = 'user_456'"));
    assert!(statements[2].contains("SET app.current_scopes = 'read,write'"));
    assert!(statements[3].contains("SET app.is_admin = 'false'"));
}

#[tokio::test]
async fn test_sql_injection_protection() {
    use vault_service::api::context::TenantQueryBuilder;

    // 尝试 SQL 注入
    let malicious_tenant_id = "test' OR '1'='1";
    let builder = TenantQueryBuilder::new(malicious_tenant_id, "SELECT * FROM credentials");
    let (query, _params) = builder.build();

    // 验证恶意字符被正确转义
    assert!(!query.contains("OR '1'='1"));
    assert!(query.contains("\\'"));
}

#[tokio::test]
async fn test_tenant_middleware_builder() {
    use vault_service::api::tenant_middleware::TenantMiddlewareBuilder;

    let state = TenantMiddlewareBuilder::new()
        .enable_cross_tenant_check()
        .enable_tenant_active_check()
        .add_public_path("/custom/public/path")
        .build();

    assert!(state.config.enable_cross_tenant_check);
    assert!(state.config.enable_tenant_active_check);
    assert!(state.config.is_public_path("/custom/public/path"));
}

#[tokio::test]
async fn test_request_id_generation() {
    let token = create_test_token("tenant_test", "user_test", vec![TokenScope::CredentialRead]);
    let context1 = RequestContext::from_validated_token(&token);
    let context2 = RequestContext::from_validated_token(&token);

    // 每个上下文应该有唯一的请求ID
    assert_ne!(context1.request_id(), context2.request_id());
    assert!(context1.request_id().starts_with("req_"));
}

#[tokio::test]
async fn test_cross_tenant_error_response() {
    use vault_service::api::context::{CrossTenantErrorResponse, TenantIsolationError};

    let error = TenantIsolationError::CrossTenantAccessDenied {
        requested: "tenant_a".to_string(),
        actual: "tenant_b".to_string(),
    };

    let response = CrossTenantErrorResponse::new("req_test123", error.to_string());

    assert!(!response.success);
    assert_eq!(response.error.code, "CROSS_TENANT_ACCESS_DENIED");
    assert!(response.error.message.contains("tenant_a"));
    assert!(response.error.message.contains("tenant_b"));
    assert_eq!(response.meta.request_id, "req_test123");
}

// ============================================================================
// 成员资格基础的租户隔离测试
// ============================================================================

#[tokio::test]
async fn test_membership_based_tenant_isolation() {
    // 场景：基于成员资格的租户隔离
    // 用户必须拥有活跃的成员资格才能访问租户资源

    use vault_service::auth::{MembershipStatus, TenantMembership};

    // Given: 用户在租户 A 有成员资格
    let tenant_a = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let membership = TenantMembership::new_owner(tenant_a, user_id);

    // Then: 成员资格允许访问
    assert_eq!(membership.tenant_id, tenant_a);
    assert_eq!(membership.status, MembershipStatus::Active);
    assert!(membership.status.allows_access());
}

#[tokio::test]
async fn test_suspended_membership_denies_access() {
    // 场景：暂停的成员资格拒绝访问

    use vault_service::auth::{MembershipStatus, TenantMembership};

    // Given: 暂停的成员资格
    let tenant_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let mut membership = TenantMembership::new_owner(tenant_id, user_id);
    membership.suspend();

    // Then: 成员资格不允许访问
    assert_eq!(membership.status, MembershipStatus::Suspended);
    assert!(!membership.status.allows_access());
}

#[tokio::test]
async fn test_pending_membership_denies_access() {
    // 场景：待确认的成员资格拒绝访问

    use vault_service::auth::{MembershipRole, MembershipStatus, TenantMembership};

    // Given: 待确认的成员资格（用户还未接受邀请）
    let tenant_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let membership = TenantMembership::new_from_invitation(
        tenant_id,
        user_id,
        MembershipRole::Member,
        Uuid::now_v7(),
    );

    // Then: 待确认成员资格不允许访问
    assert_eq!(membership.status, MembershipStatus::Pending);
    assert!(!membership.status.allows_access());
}

#[tokio::test]
async fn test_cross_tenant_access_with_membership_check() {
    // 场景：跨租户访问检查（带成员资格验证）

    let app = create_test_router();

    // Given: 用户 A 在 tenant_a 有成员资格
    let token_a = create_test_token("tenant_a", "user_a", vec![TokenScope::CredentialRead]);

    // When: 尝试访问 tenant_b 的资源
    let request = Request::builder()
        .uri("/api/v1/tenants/tenant_b/credentials")
        .body(Body::empty())
        .unwrap();

    let (mut parts, body) = request.into_parts();
    parts.extensions.insert(token_a);
    let request = Request::from_parts(parts, body);

    let response = app.oneshot(request).await.unwrap();

    // Then: 应该被拒绝（跨租户访问）
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_membership_role_scope_enforcement() {
    // 场景：成员资格角色的 Scope 强制执行

    use vault_service::auth::{MembershipRole, TenantMembership};

    // Given: 不同角色的成员资格
    let tenant_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();

    // Owner 角色
    let owner_membership = TenantMembership::new_owner(tenant_id, user_id);
    assert!(owner_membership.has_scope("tenant:delete"));
    assert!(owner_membership.has_scope("members:invite"));

    // Member 角色
    let member_membership = TenantMembership::new_from_invitation(
        tenant_id,
        user_id,
        MembershipRole::Member,
        Uuid::now_v7(),
    );
    assert!(!member_membership.has_scope("tenant:delete"));
    assert!(!member_membership.has_scope("members:invite"));
    assert!(member_membership.has_scope("credential:read"));
    assert!(member_membership.has_scope("credential:write"));

    // Readonly 角色
    let readonly_membership = TenantMembership::new_from_invitation(
        tenant_id,
        user_id,
        MembershipRole::Readonly,
        Uuid::now_v7(),
    );
    assert!(readonly_membership.has_scope("credential:read"));
    assert!(!readonly_membership.has_scope("credential:write"));
}

#[tokio::test]
async fn test_invitation_based_membership_access() {
    // 场景：基于邀请的成员资格访问

    use vault_service::auth::{
        InvitationStatus, MembershipRole, MembershipStatus, TenantInvitation, TenantMembership,
    };

    // Given: 管理员创建邀请
    let tenant_id = Uuid::now_v7();
    let admin_id = Uuid::now_v7();
    let invitation = TenantInvitation::new_wallet_invitation(
        tenant_id,
        MembershipRole::Member,
        "0xwallet123",
        admin_id,
        24,
    );

    // Then: 邀请有效
    assert!(invitation.is_valid());
    assert_eq!(invitation.status, InvitationStatus::Pending);

    // When: 用户接受邀请
    let user_id = Uuid::now_v7();
    let mut membership =
        TenantMembership::new_from_invitation(tenant_id, user_id, invitation.role, admin_id);
    membership.accept_invitation();

    // Then: 成员资格激活，可以访问
    assert_eq!(membership.status, MembershipStatus::Active);
    assert!(membership.status.allows_access());
}

#[tokio::test]
async fn test_expired_invitation_membership_denied() {
    // 场景：过期邀请无法获得成员资格

    use chrono::{Duration, Utc};
    use vault_service::auth::{MembershipRole, TenantInvitation};

    // Given: 过期的邀请
    let tenant_id = Uuid::now_v7();
    let admin_id = Uuid::now_v7();
    let mut invitation = TenantInvitation::new_wallet_invitation(
        tenant_id,
        MembershipRole::Member,
        "0xwallet456",
        admin_id,
        -1, // 已过期
    );
    invitation.expires_at = Utc::now() - Duration::hours(1);

    // Then: 邀请无效
    assert!(!invitation.is_valid());
    assert!(invitation.is_expired());

    // 用户无法通过过期邀请获得成员资格
    // 这在 AuthService.consume_invitation 中会返回 InvitationExpired 错误
}

#[tokio::test]
async fn test_membership_scope_in_request_context() {
    // 场景：请求上下文中的成员资格 Scope

    // Given: 带有特定 scope 的 Token
    let token = create_test_token(
        "tenant_test",
        "user_test",
        vec![TokenScope::CredentialRead, TokenScope::CredentialWrite],
    );

    let context = RequestContext::from_validated_token(&token);

    // Then: 上下文中包含正确的 scope
    assert!(context.has_scope("credential:read"));
    assert!(context.has_scope("credential:write"));
    assert!(!context.has_scope("credential:delete"));
    assert!(!context.has_scope("admin"));
}

#[tokio::test]
async fn test_membership_management_permission() {
    // 场景：成员管理权限检查

    use vault_service::auth::{MembershipRole, TenantMembership};

    // Given: Admin 角色的成员资格
    let tenant_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let mut admin_membership = TenantMembership::new_from_invitation(
        tenant_id,
        user_id,
        MembershipRole::Admin,
        Uuid::now_v7(),
    );
    admin_membership.accept_invitation();

    // Then: Admin 可以管理成员
    assert!(admin_membership.can_manage());
    assert!(admin_membership.has_scope("members:read"));
    assert!(admin_membership.has_scope("members:write"));
    assert!(admin_membership.has_scope("members:invite"));

    // Given: Member 角色的成员资格
    let mut member_membership = TenantMembership::new_from_invitation(
        tenant_id,
        user_id,
        MembershipRole::Member,
        Uuid::now_v7(),
    );
    member_membership.accept_invitation();

    // Then: Member 不能管理成员
    assert!(!member_membership.can_manage());
    assert!(!member_membership.has_scope("members:read"));
    assert!(!member_membership.has_scope("members:invite"));
}

#[tokio::test]
async fn test_inactive_membership_access_denied() {
    // 场景：非活跃成员资格拒绝访问

    use vault_service::auth::{MembershipStatus, TenantMembership};

    // Given: 用户离开租户后的成员资格
    let tenant_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let mut membership = TenantMembership::new_owner(tenant_id, user_id);
    membership.leave();

    // Then: 成员资格不允许访问
    assert_eq!(membership.status, MembershipStatus::Inactive);
    assert!(!membership.status.allows_access());
    assert!(!membership.can_manage());
}

#[tokio::test]
async fn test_membership_role_upgrade() {
    // 场景：成员资格角色升级

    use vault_service::auth::{MembershipRole, TenantMembership};

    // Given: Member 角色的成员资格
    let tenant_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let mut membership = TenantMembership::new_from_invitation(
        tenant_id,
        user_id,
        MembershipRole::Member,
        Uuid::now_v7(),
    );
    membership.accept_invitation();

    // 初始状态：Member 权限
    assert!(!membership.can_manage());
    assert!(!membership.has_scope("tenant:admin"));

    // When: 升级为 Admin
    membership.update_role(MembershipRole::Admin);

    // Then: 权限更新
    assert!(membership.can_manage());
    assert!(membership.has_scope("tenant:admin"));
    assert!(membership.has_scope("members:invite"));
}

#[tokio::test]
async fn test_multi_tenant_user_isolation() {
    // 场景：多租户用户隔离

    use vault_service::auth::{MembershipRole, TenantMembership};

    // Given: 用户在两个租户有不同角色的成员资格
    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    let user_id = Uuid::now_v7();

    let membership_a = TenantMembership::new_owner(tenant_a, user_id);
    let mut membership_b = TenantMembership::new_from_invitation(
        tenant_b,
        user_id,
        MembershipRole::Readonly,
        Uuid::now_v7(),
    );
    membership_b.accept_invitation();

    // Then: 租户 A 有完全权限
    assert!(membership_a.is_owner());
    assert!(membership_a.has_scope("tenant:delete"));

    // Then: 租户 B 只有读取权限
    assert!(!membership_b.is_owner());
    assert!(!membership_b.can_manage());
    assert!(membership_b.has_scope("credential:read"));
    assert!(!membership_b.has_scope("credential:write"));

    // 权限不跨租户
}
