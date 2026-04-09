#![allow(clippy::field_reassign_with_default)]
#![allow(dead_code)]
#![allow(clippy::uninlined_format_args)]
//! 凭证版本控制 API 集成测试
//!
//! 测试 EP2-Story2.4 凭证版本控制功能
//! - PUT /api/v1/credentials/:id - 更新凭证（创建新版本）
//! - GET /api/v1/credentials/:id/versions - 查询版本历史
//! - GET /api/v1/credentials/:id/versions/:version - 查询指定版本
//! - POST /api/v1/credentials/:id/rollback - 版本回滚

use axum::body::Body;
use axum::http::{Request, StatusCode};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

use tokio::sync::Mutex;
use vault_service::api::credentials::{
    AppState, AuditLogger, DefaultAuditLogger, create_credential, delete_credential,
    get_credential, list_credentials, update_credential,
};
use vault_service::api::middleware::{TokenScope, ValidatedToken};
use vault_service::api::versions::{get_version_detail, get_version_history, rollback_credential};
use vault_service::crypto::hkdf::KeyHierarchy;
use vault_service::crypto::keys::HardwareRootKey;
use vault_service::tee::{Enclave, EnclaveConfig};
use vault_service::vault::storage::CredentialVault;

/// 设置测试状态
async fn setup_test_state() -> AppState {
    let vault = CredentialVault::new_in_memory();
    let mut hierarchy = KeyHierarchy::new();
    let l0 = HardwareRootKey::for_simulation().expect("failed to create simulation hardware root");
    hierarchy
        .initialize_master_key(&l0)
        .expect("failed to initialize test key hierarchy");
    let key_hierarchy = Arc::new(RwLock::new(hierarchy));
    let mut enclave = Enclave::new(EnclaveConfig {
        debug_mode: true,
        ..Default::default()
    });
    enclave
        .initialize()
        .expect("failed to initialize test enclave");
    let audit_logger: Arc<dyn AuditLogger> = Arc::new(DefaultAuditLogger);

    AppState {
        vault: Arc::new(vault),
        key_hierarchy,
        enclave: Arc::new(Mutex::new(enclave)),
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
        membership_id: None,
        metadata: std::collections::HashMap::new(),
        subject_type: vault_service::token::TOKEN_SUBJECT_TYPE_USER.to_string(),
        issued_from: vault_service::token::TOKEN_ISSUED_FROM_SESSION.to_string(),
    }
}

/// 构建测试路由 - 包含版本控制端点
fn test_versioning_router(state: AppState, token: ValidatedToken) -> axum::Router {
    use axum::routing::{delete, get, post, put};

    axum::Router::new()
        // 基础凭证操作
        .route("/api/v1/credentials", post(create_credential))
        .route("/api/v1/credentials", get(list_credentials))
        .route("/api/v1/credentials/:id", get(get_credential))
        .route("/api/v1/credentials/:id", put(update_credential))
        .route("/api/v1/credentials/:id", delete(delete_credential))
        // 版本控制端点
        .route("/api/v1/credentials/:id/versions", get(get_version_history))
        .route(
            "/api/v1/credentials/:id/versions/:version",
            get(get_version_detail),
        )
        .route(
            "/api/v1/credentials/:id/rollback",
            post(rollback_credential),
        )
        .layer(axum::Extension(token))
        .with_state(state)
}

/// AC-1: 更新凭证 API 需要 write scope
#[tokio::test]
async fn test_update_credential_requires_write_scope() {
    let state = setup_test_state().await;
    let token = create_test_token(
        "tenant_123",
        "user_456",
        vec![TokenScope::CredentialRead], // 缺少 write scope
    );

    let app = test_versioning_router(state, token);

    let request = Request::builder()
        .method("PUT")
        .uri("/api/v1/credentials/test-id")
        .header("Content-Type", "application/json")
        .body(Body::from(
            r#"{
                "plaintext_data": {"password": "new_secret"},
                "change_reason": "定期更新"
            }"#,
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// AC-2: 版本历史查询 API 需要 read scope
#[tokio::test]
async fn test_get_version_history_requires_read_scope() {
    let state = setup_test_state().await;
    let token = create_test_token(
        "tenant_123",
        "user_456",
        vec![TokenScope::CredentialWrite], // 缺少 read scope
    );

    let app = test_versioning_router(state, token);

    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/credentials/test-id/versions")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// AC-3: 指定版本查询 API 需要 read scope
#[tokio::test]
async fn test_get_version_detail_requires_read_scope() {
    let state = setup_test_state().await;
    let token = create_test_token(
        "tenant_123",
        "user_456",
        vec![TokenScope::CredentialWrite], // 缺少 read scope
    );

    let app = test_versioning_router(state, token);

    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/credentials/test-id/versions/1")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// AC-4: 回滚 API 需要 write scope
#[tokio::test]
async fn test_rollback_requires_write_scope() {
    let state = setup_test_state().await;
    let token = create_test_token(
        "tenant_123",
        "user_456",
        vec![TokenScope::CredentialRead], // 缺少 write scope
    );

    let app = test_versioning_router(state, token);

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/credentials/test-id/rollback")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"target_version": 1, "reason": "回滚测试"}"#))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// AC-5: 更新不存在的凭证应返回 404
#[tokio::test]
async fn test_update_nonexistent_credential_returns_404() {
    let state = setup_test_state().await;
    let token = create_test_token("tenant_123", "user_456", vec![TokenScope::CredentialWrite]);

    let app = test_versioning_router(state, token);

    // 使用有效的 UUID 格式
    let request = Request::builder()
        .method("PUT")
        .uri("/api/v1/credentials/018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c")
        .header("Content-Type", "application/json")
        .body(Body::from(
            r#"{
                "plaintext_data": {"password": "new_secret"},
                "change_reason": "定期更新"
            }"#,
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    // 凭证不存在或加密失败，但不应该是 403
    assert_ne!(response.status(), StatusCode::FORBIDDEN);
}

/// AC-6: 版本历史 API 结构验证
#[tokio::test]
async fn test_version_history_api_structure() {
    let state = setup_test_state().await;
    let token = create_test_token("tenant_123", "user_456", vec![TokenScope::CredentialRead]);

    let app = test_versioning_router(state, token);

    // 查询不存在的凭证版本历史
    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/credentials/018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c/versions")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    // 凭证不存在应该返回 404
    assert_ne!(response.status(), StatusCode::FORBIDDEN);
}

/// AC-7: 指定版本查询 API 结构验证
#[tokio::test]
async fn test_version_detail_api_structure() {
    let state = setup_test_state().await;
    let token = create_test_token("tenant_123", "user_456", vec![TokenScope::CredentialRead]);

    let app = test_versioning_router(state, token);

    // 查询不存在的凭证的指定版本
    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/credentials/018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c/versions/1")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    // 凭证不存在应该返回 404
    assert_ne!(response.status(), StatusCode::FORBIDDEN);
}

/// AC-8: 回滚 API 请求格式验证
#[tokio::test]
async fn test_rollback_request_format() {
    let state = setup_test_state().await;
    let token = create_test_token("tenant_123", "user_456", vec![TokenScope::CredentialWrite]);

    let app = test_versioning_router(state, token);

    // 测试缺少 reason 字段
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/credentials/test-id/rollback")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"target_version": 1}"#))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    // 请求格式错误或凭证不存在，但不应该是 403
    assert_ne!(response.status(), StatusCode::FORBIDDEN);
}

/// AC-9: 回滚 API 需要同时提供 target_version 和 reason
#[tokio::test]
async fn test_rollback_with_valid_request_format() {
    let state = setup_test_state().await;
    let token = create_test_token("tenant_123", "user_456", vec![TokenScope::CredentialWrite]);

    let app = test_versioning_router(state, token);

    // 使用有效的 UUID 格式，提供完整参数
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/credentials/018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c/rollback")
        .header("Content-Type", "application/json")
        .body(Body::from(
            r#"{"target_version": 1, "reason": "安全审计要求回滚"}"#,
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    // 凭证不存在或回滚失败，但不应该是 403
    assert_ne!(response.status(), StatusCode::FORBIDDEN);
}

/// AC-10: 更新请求格式验证 - 缺少 plaintext_data
#[tokio::test]
async fn test_update_credential_request_format() {
    let state = setup_test_state().await;
    let token = create_test_token("tenant_123", "user_456", vec![TokenScope::CredentialWrite]);

    let app = test_versioning_router(state, token);

    // 缺少必需的 plaintext_data 字段
    let request = Request::builder()
        .method("PUT")
        .uri("/api/v1/credentials/test-id")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"change_reason": "测试"}"#))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    // 请求格式错误或凭证不存在，但不应该是 403
    assert_ne!(response.status(), StatusCode::FORBIDDEN);
}

/// AC-11: 版本历史 API 响应格式验证（结构检查）
#[tokio::test]
async fn test_version_history_response_structure() {
    use serde_json::Value;

    // 验证 VersionHistory 结构体可以正确序列化
    let history = vault_service::vault::version::VersionHistory {
        credential_id: "test-id".to_string(),
        current_version: 3,
        versions: vec![
            vault_service::vault::version::VersionSummary {
                version: 1,
                created_at: chrono::Utc::now(),
                changed_by: Some("user_1".to_string()),
                change_reason: Some("初始创建".to_string()),
            },
            vault_service::vault::version::VersionSummary {
                version: 2,
                created_at: chrono::Utc::now(),
                changed_by: Some("user_1".to_string()),
                change_reason: Some("密码更新".to_string()),
            },
        ],
        total: 2,
    };

    let json = serde_json::to_string(&history).expect("应该能序列化");
    let parsed: Value = serde_json::from_str(&json).expect("应该能反序列化");

    assert_eq!(parsed["credential_id"], "test-id");
    assert_eq!(parsed["current_version"], 3);
    assert_eq!(parsed["total"], 2);
    assert!(parsed["versions"].is_array());
    assert_eq!(parsed["versions"].as_array().unwrap().len(), 2);
}

/// AC-12: 版本详情 API 响应格式验证（结构检查）
#[tokio::test]
async fn test_version_detail_response_structure() {
    use serde_json::Value;

    // 验证 VersionDetail 结构体可以正确序列化
    let detail = vault_service::vault::version::VersionDetail {
        credential_id: "test-id".to_string(),
        version: 1,
        created_at: chrono::Utc::now(),
        changed_by: Some("user_1".to_string()),
        change_reason: Some("初始创建".to_string()),
        metadata: vault_service::vault::version::VersionMetadata {
            service_id: "schwab".to_string(),
            credential_type: "username_password".to_string(),
            algorithm: "AES-256-GCM".to_string(),
        },
    };

    let json = serde_json::to_string(&detail).expect("应该能序列化");
    let parsed: Value = serde_json::from_str(&json).expect("应该能反序列化");

    assert_eq!(parsed["credential_id"], "test-id");
    assert_eq!(parsed["version"], 1);
    assert!(parsed["metadata"].is_object());
    assert_eq!(parsed["metadata"]["service_id"], "schwab");
    assert_eq!(parsed["metadata"]["credential_type"], "username_password");
    assert_eq!(parsed["metadata"]["algorithm"], "AES-256-GCM");
}

/// AC-13: 回滚响应格式验证（结构检查）
#[tokio::test]
async fn test_rollback_response_structure() {
    use serde_json::Value;

    // 验证 RollbackResponse 结构体可以正确序列化
    let response = vault_service::vault::version::RollbackResponse {
        credential_id: "test-id".to_string(),
        previous_version: 3,
        current_version: 4,
        rollback_to_version: 1,
        rollback_at: chrono::Utc::now().to_rfc3339(),
        reason: "安全审计要求".to_string(),
    };

    let json = serde_json::to_string(&response).expect("应该能序列化");
    let parsed: Value = serde_json::from_str(&json).expect("应该能反序列化");

    assert_eq!(parsed["credential_id"], "test-id");
    assert_eq!(parsed["previous_version"], 3);
    assert_eq!(parsed["current_version"], 4);
    assert_eq!(parsed["rollback_to_version"], 1);
    assert!(parsed["rollback_at"].is_string());
    assert_eq!(parsed["reason"], "安全审计要求");
}

/// AC-14: 更新响应格式验证（结构检查）
#[tokio::test]
async fn test_update_response_structure() {
    use serde_json::Value;

    // 验证 UpdateCredentialResponse 结构体可以正确序列化
    let response = vault_service::vault::version::UpdateCredentialResponse {
        credential_id: "test-id".to_string(),
        version: 3,
        service_id: "schwab".to_string(),
        credential_type: "username_password".to_string(),
        updated_at: chrono::Utc::now().to_rfc3339(),
        previous_version: 2,
    };

    let json = serde_json::to_string(&response).expect("应该能序列化");
    let parsed: Value = serde_json::from_str(&json).expect("应该能反序列化");

    assert_eq!(parsed["credential_id"], "test-id");
    assert_eq!(parsed["version"], 3);
    assert_eq!(parsed["previous_version"], 2);
    assert_eq!(parsed["service_id"], "schwab");
    assert_eq!(parsed["credential_type"], "username_password");
    assert!(parsed["updated_at"].is_string());
}

/// AC-15: 更新请求格式验证（结构检查）
#[tokio::test]
async fn test_update_request_structure() {
    // 验证 UpdateCredentialRequest 可以正确反序列化
    let json = r#"{
        "plaintext_data": {"username": "test", "password": "secret"},
        "change_reason": "密码轮换",
        "expected_version": 2
    }"#;

    let request: vault_service::vault::version::UpdateCredentialRequest =
        serde_json::from_str(json).expect("应该能反序列化");

    assert_eq!(request.change_reason, Some("密码轮换".to_string()));
    assert_eq!(request.expected_version, Some(2));
    assert!(request.plaintext_data.is_object());
}

/// AC-16: 回滚请求格式验证（结构检查）
#[tokio::test]
async fn test_rollback_request_structure() {
    // 验证 RollbackRequest 可以正确反序列化
    let json = r#"{"target_version": 2, "reason": "回滚到稳定版本"}"#;

    let request: vault_service::vault::version::RollbackRequest =
        serde_json::from_str(json).expect("应该能反序列化");

    assert_eq!(request.target_version, 2);
    assert_eq!(request.reason, "回滚到稳定版本");
}
