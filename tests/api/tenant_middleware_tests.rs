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
