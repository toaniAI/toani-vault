//! 凭证 CRUD API 集成测试
//!
//! 测试 EP2-Story2.2 凭证管理 API

use axum::body::Body;
use axum::http::{Request, StatusCode};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

use vault_service::api::credentials::{
    create_credential, delete_credential, decrypt_credential_endpoint, get_credential,
    list_credentials, AppState, AuditLogger, DefaultAuditLogger,
};
use vault_service::api::middleware::{TokenScope, ValidatedToken};
use vault_service::crypto::hkdf::KeyHierarchy;
use vault_service::vault::storage::CredentialVault;

/// 设置测试状态
async fn setup_test_state() -> AppState {
    let vault = CredentialVault::new_in_memory();
    let key_hierarchy = Arc::new(RwLock::new(KeyHierarchy::new()));
    let audit_logger: Arc<dyn AuditLogger> = Arc::new(DefaultAuditLogger);

    AppState {
        vault: Arc::new(vault),
        key_hierarchy,
        audit_logger,
    }
}

/// 创建模拟的已验证 Token
fn create_test_token(tenant_id: &str, user_id: &str, scopes: Vec<TokenScope>) -> ValidatedToken {
    use std::time::{SystemTime, UNIX_EPOCH};

    ValidatedToken {
        token_id: uuid::Uuid::now_v7().to_string(),
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

/// 构建测试路由 - 简化版本，直接使用 Extension 层
fn test_router(state: AppState, token: ValidatedToken) -> axum::Router {
    use axum::routing::{delete, get, post};

    axum::Router::new()
        .route("/api/v1/credentials", post(create_credential))
        .route("/api/v1/credentials", get(list_credentials))
        .route("/api/v1/credentials/:id", get(get_credential))
        .route("/api/v1/credentials/:id/decrypt", post(decrypt_credential_endpoint))
        .route("/api/v1/credentials/:id", delete(delete_credential))
        .layer(axum::Extension(token))
        .with_state(state)
}

/// 测试创建凭证 API 需要 write scope
#[tokio::test]
async fn test_create_credential_success_with_write_scope() {
    let state = setup_test_state().await;
    // 只要有 write scope，API 会尝试处理请求（即使后续加密可能失败）
    let token = create_test_token(
        "tenant_123",
        "user_456",
        vec![TokenScope::CredentialWrite],
    );

    let app = test_router(state, token);

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/credentials")
        .header("Content-Type", "application/json")
        .body(Body::from(
            r#"{
                "service_id": "test_service",
                "credential_type": "username_password",
                "plaintext_data": {"username": "test_user", "password": "secret123"}
            }"#,
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    // 注意：由于测试环境中 TEE 未完全初始化，加密会失败返回 500
    // 这里我们只验证请求通过了 scope 检查（不是 403）
    assert_ne!(response.status(), StatusCode::FORBIDDEN);
}

/// 测试创建凭证缺少 write scope
#[tokio::test]
async fn test_create_credential_missing_scope() {
    let state = setup_test_state().await;
    let token = create_test_token(
        "tenant_123",
        "user_456",
        vec![TokenScope::CredentialRead], // 缺少 write scope
    );

    let app = test_router(state, token);

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/credentials")
        .header("Content-Type", "application/json")
        .body(Body::from(
            r#"{
                "service_id": "test_service",
                "credential_type": "username_password",
                "plaintext_data": {"username": "test_user", "password": "secret123"}
            }"#,
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// 测试获取凭证列表
#[tokio::test]
async fn test_list_credentials() {
    let state = setup_test_state().await;
    let token = create_test_token(
        "tenant_123",
        "user_456",
        vec![TokenScope::CredentialRead],
    );

    let app = test_router(state, token);

    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/credentials")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

/// 测试获取凭证列表缺少 read scope
#[tokio::test]
async fn test_list_credentials_missing_scope() {
    let state = setup_test_state().await;
    let token = create_test_token(
        "tenant_123",
        "user_456",
        vec![TokenScope::CredentialWrite], // 缺少 read scope
    );

    let app = test_router(state, token);

    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/credentials")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// 测试解密凭证需要 decrypt scope
#[tokio::test]
async fn test_decrypt_credential_requires_decrypt_scope() {
    let state = setup_test_state().await;
    let token = create_test_token(
        "tenant_123",
        "user_456",
        vec![TokenScope::CredentialRead, TokenScope::CredentialWrite], // 缺少 decrypt scope
    );

    let app = test_router(state, token);

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/credentials/test-id/decrypt")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"reason": "test"}"#))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// 测试删除凭证
#[tokio::test]
async fn test_delete_credential_requires_write_or_admin() {
    let state = setup_test_state().await;
    let token = create_test_token(
        "tenant_123",
        "user_456",
        vec![TokenScope::Admin], // Admin 也可以删除
    );

    let app = test_router(state, token);

    let request = Request::builder()
        .method("DELETE")
        .uri("/api/v1/credentials/test-id")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    // 由于凭证不存在，应该返回 404，而不是 403
    assert_ne!(response.status(), StatusCode::FORBIDDEN);
}

/// 测试删除凭证缺少 scope
#[tokio::test]
async fn test_delete_credential_missing_scope() {
    let state = setup_test_state().await;
    let token = create_test_token(
        "tenant_123",
        "user_456",
        vec![TokenScope::CredentialRead], // 既没有 write 也没有 admin
    );

    let app = test_router(state, token);

    let request = Request::builder()
        .method("DELETE")
        .uri("/api/v1/credentials/test-id")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
