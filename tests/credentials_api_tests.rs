#![allow(clippy::field_reassign_with_default)]
#![allow(dead_code)]
#![allow(clippy::uninlined_format_args)]
//! 凭证 CRUD API 集成测试
//!
//! 测试 EP2-Story2.2 凭证管理 API

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

use vault_service::api::credentials::{
    AppState, AuditLogger, DefaultAuditLogger, create_credential, decrypt_credential_endpoint,
    delete_credential, get_credential, list_credentials,
};
use vault_service::api::middleware::{TokenScope, ValidatedToken};
use vault_service::crypto::constants;
use vault_service::crypto::hkdf::KeyHierarchy;
use vault_service::vault::models::{
    CreateCredentialRequest, EncryptedPayload, ServiceId, TenantId, UserId,
};
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

fn create_test_payload() -> EncryptedPayload {
    EncryptedPayload::new(
        constants::PROTOCOL_VERSION,
        constants::ALGORITHM_AES_256_GCM,
        constants::KDF_HKDF_SHA256,
        vec![0u8; constants::NONCE_LENGTH],
        vec![0u8; constants::AUTH_TAG_LENGTH],
        vec![1, 2, 3, 4, 5],
    )
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
        .route(
            "/api/v1/credentials/:id/decrypt",
            post(decrypt_credential_endpoint),
        )
        .route("/api/v1/credentials/:id", delete(delete_credential))
        .layer(axum::Extension(token))
        .with_state(state)
}

/// 测试创建凭证 API 需要 write scope
#[tokio::test]
async fn test_create_credential_success_with_write_scope() {
    let state = setup_test_state().await;
    // 只要有 write scope，API 会尝试处理请求（即使后续加密可能失败）
    let token = create_test_token("tenant_123", "user_456", vec![TokenScope::CredentialWrite]);

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

/// 测试创建凭证时拒绝过去时间的 expires_at
#[tokio::test]
async fn test_create_credential_rejects_past_expires_at() {
    let state = setup_test_state().await;
    let token = create_test_token("tenant_123", "user_456", vec![TokenScope::CredentialWrite]);
    let past_expires_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .saturating_sub(60);

    let response = create_credential(
        axum::extract::State(state),
        axum::Extension(token),
        axum::Json(
            vault_service::api::credentials::CreateCredentialApiRequest {
                service_id: "test_service".to_string(),
                credential_type: vault_service::models::CredentialType::UsernamePassword,
                plaintext_data: serde_json::json!({
                    "username": "test_user",
                    "password": "secret123"
                }),
                expires_at: Some(past_expires_at),
            },
        ),
    )
    .await
    .unwrap_err()
    .into_response();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["error"].as_str(), Some("invalid_request"));
    assert_eq!(
        json["message"].as_str(),
        Some("expires_at must be in the future")
    );
}

/// 测试获取凭证列表
#[tokio::test]
async fn test_list_credentials() {
    let state = setup_test_state().await;
    let token = create_test_token("tenant_123", "user_456", vec![TokenScope::CredentialRead]);

    let app = test_router(state, token);

    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/credentials")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

/// 测试获取凭证列表时保留已过期但未删除的凭证
#[tokio::test]
async fn test_list_credentials_includes_expired_entries() {
    let state = setup_test_state().await;
    let expired_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .saturating_sub(60);

    let expired_entry = state
        .vault
        .create_credential(
            CreateCredentialRequest {
                tenant_id: TenantId::new("tenant_123"),
                user_id: UserId::new("user_456"),
                service_id: ServiceId::new("expired_service"),
                credential_type: vault_service::models::CredentialType::ApiKey,
                expires_at: Some(expired_at),
            },
            create_test_payload(),
        )
        .unwrap();

    let token = create_test_token("tenant_123", "user_456", vec![TokenScope::CredentialRead]);
    let app = test_router(state, token);

    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/credentials")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let credentials = json["credentials"].as_array().unwrap();

    assert_eq!(json["total"].as_u64(), Some(1));
    assert_eq!(credentials.len(), 1);
    assert_eq!(
        credentials[0]["credential_id"].as_str(),
        Some(expired_entry.credential_id.as_str())
    );
    assert_eq!(
        credentials[0]["service_id"].as_str(),
        Some("expired_service")
    );
    assert_eq!(credentials[0]["is_deleted"].as_bool(), Some(false));
    assert_eq!(
        credentials[0]["expires_at"].as_str(),
        expired_entry.metadata().expires_at.as_deref()
    );
}

/// 测试按服务标识过滤凭证列表
#[tokio::test]
async fn test_list_credentials_filters_by_service_id() {
    let state = setup_test_state().await;

    state
        .vault
        .create_credential(
            CreateCredentialRequest {
                tenant_id: TenantId::new("tenant_123"),
                user_id: UserId::new("user_456"),
                service_id: ServiceId::new("github-prod"),
                credential_type: vault_service::models::CredentialType::ApiKey,
                expires_at: None,
            },
            create_test_payload(),
        )
        .unwrap();

    state
        .vault
        .create_credential(
            CreateCredentialRequest {
                tenant_id: TenantId::new("tenant_123"),
                user_id: UserId::new("user_456"),
                service_id: ServiceId::new("aws-dev"),
                credential_type: vault_service::models::CredentialType::UsernamePassword,
                expires_at: None,
            },
            create_test_payload(),
        )
        .unwrap();

    let token = create_test_token("tenant_123", "user_456", vec![TokenScope::CredentialRead]);
    let app = test_router(state, token);

    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/credentials?service_id=github-prod")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let credentials = json["credentials"].as_array().unwrap();

    assert_eq!(json["total"].as_u64(), Some(1));
    assert_eq!(credentials.len(), 1);
    assert_eq!(credentials[0]["service_id"].as_str(), Some("github-prod"));
}

/// 测试按凭证类型过滤凭证列表
#[tokio::test]
async fn test_list_credentials_filters_by_credential_type() {
    let state = setup_test_state().await;

    state
        .vault
        .create_credential(
            CreateCredentialRequest {
                tenant_id: TenantId::new("tenant_123"),
                user_id: UserId::new("user_456"),
                service_id: ServiceId::new("github-prod"),
                credential_type: vault_service::models::CredentialType::ApiKey,
                expires_at: None,
            },
            create_test_payload(),
        )
        .unwrap();

    state
        .vault
        .create_credential(
            CreateCredentialRequest {
                tenant_id: TenantId::new("tenant_123"),
                user_id: UserId::new("user_456"),
                service_id: ServiceId::new("aws-dev"),
                credential_type: vault_service::models::CredentialType::UsernamePassword,
                expires_at: None,
            },
            create_test_payload(),
        )
        .unwrap();

    let token = create_test_token("tenant_123", "user_456", vec![TokenScope::CredentialRead]);
    let app = test_router(state, token);

    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/credentials?credential_type=api_key")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let credentials = json["credentials"].as_array().unwrap();

    assert_eq!(json["total"].as_u64(), Some(1));
    assert_eq!(credentials.len(), 1);
    assert_eq!(credentials[0]["credential_type"].as_str(), Some("api_key"));
}

/// 测试 only_valid=true 时过滤掉已过期凭证
#[tokio::test]
async fn test_list_credentials_only_valid_filters_expired_entries() {
    let state = setup_test_state().await;
    let expired_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .saturating_sub(60);

    state
        .vault
        .create_credential(
            CreateCredentialRequest {
                tenant_id: TenantId::new("tenant_123"),
                user_id: UserId::new("user_456"),
                service_id: ServiceId::new("expired-service"),
                credential_type: vault_service::models::CredentialType::ApiKey,
                expires_at: Some(expired_at),
            },
            create_test_payload(),
        )
        .unwrap();

    state
        .vault
        .create_credential(
            CreateCredentialRequest {
                tenant_id: TenantId::new("tenant_123"),
                user_id: UserId::new("user_456"),
                service_id: ServiceId::new("valid-service"),
                credential_type: vault_service::models::CredentialType::ApiKey,
                expires_at: None,
            },
            create_test_payload(),
        )
        .unwrap();

    let token = create_test_token("tenant_123", "user_456", vec![TokenScope::CredentialRead]);
    let app = test_router(state, token);

    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/credentials?only_valid=true")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let credentials = json["credentials"].as_array().unwrap();

    assert_eq!(json["total"].as_u64(), Some(1));
    assert_eq!(credentials.len(), 1);
    assert_eq!(credentials[0]["service_id"].as_str(), Some("valid-service"));
}

/// 测试未知凭证类型返回 400，避免无声忽略无效筛选
#[tokio::test]
async fn test_list_credentials_rejects_unknown_credential_type() {
    let state = setup_test_state().await;
    let token = create_test_token("tenant_123", "user_456", vec![TokenScope::CredentialRead]);
    let app = test_router(state, token);

    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/credentials?credential_type=database_connection")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
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
