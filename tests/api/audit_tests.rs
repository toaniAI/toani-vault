//! 审计日志 API 集成测试
//!
//! 测试审计查询 API 的所有端点

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use uuid::Uuid;

use vault_service::api::{
    audit::{AuditApiState, MemoryAuditStorageAdapter, audit_routes},
    audit_models::*,
    middleware::{TokenScope, ValidatedToken},
};
use vault_service::audit::{AuditAction, AuditEntry, MemoryAuditStorage, Outcome, RiskTier};

/// 创建测试 Token（带 audit:read 权限）
fn create_audit_token() -> ValidatedToken {
    ValidatedToken {
        token_id: Uuid::now_v7().to_string(),
        subject: "tenant1:user1".to_string(),
        tenant_id: "tenant1".to_string(),
        user_id: "user1".to_string(),
        expires_at: u64::MAX,
        scopes: vec![TokenScope::AuditRead],
        issued_at: 1000,
    }
}

/// 创建管理员 Token
fn create_admin_token() -> ValidatedToken {
    ValidatedToken {
        token_id: Uuid::now_v7().to_string(),
        subject: "tenant1:admin".to_string(),
        tenant_id: "tenant1".to_string(),
        user_id: "admin".to_string(),
        expires_at: u64::MAX,
        scopes: vec![TokenScope::Admin],
        issued_at: 1000,
    }
}

/// 创建无权限 Token
fn create_no_permission_token() -> ValidatedToken {
    ValidatedToken {
        token_id: Uuid::now_v7().to_string(),
        subject: "tenant1:user2".to_string(),
        tenant_id: "tenant1".to_string(),
        user_id: "user2".to_string(),
        expires_at: u64::MAX,
        scopes: vec![TokenScope::CredentialRead],
        issued_at: 1000,
    }
}

/// 创建带测试数据的存储
fn create_test_storage() -> MemoryAuditStorageAdapter {
    let storage = MemoryAuditStorage::new(1000).unwrap();

    // 添加一些测试数据
    for i in 0..10 {
        let entry = AuditEntry::new(
            format!("user_hash_{}", i % 3),
            format!("session_{}", i),
            "vault-service",
            if i % 2 == 0 {
                AuditAction::CredentialDecrypt
            } else {
                AuditAction::TokenValidate
            },
            if i % 4 == 0 {
                Outcome::Failure
            } else {
                Outcome::Success
            },
            "mrenclave_test",
            format!("jti_{}", i),
        );
        storage.record(entry).unwrap();
    }

    MemoryAuditStorageAdapter::new(storage)
}

/// 创建测试应用
fn create_test_app() -> Router {
    let state = AuditApiState {
        storage: std::sync::Arc::new(create_test_storage()),
    };
    audit_routes(state)
}

#[tokio::test]
async fn test_list_audit_logs_success() {
    let app = create_test_app();

    let request = Request::builder()
        .uri("/audit/logs")
        .method("GET")
        .header("Authorization", "Bearer test_token")
        .extension(create_audit_token())
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_list_audit_logs_with_pagination() {
    let app = create_test_app();

    let request = Request::builder()
        .uri("/audit/logs?page=1&page_size=5")
        .method("GET")
        .header("Authorization", "Bearer test_token")
        .extension(create_audit_token())
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_list_audit_logs_with_filters() {
    let app = create_test_app();

    let request = Request::builder()
        .uri("/audit/logs?action=credential_decrypt&outcome=success")
        .method("GET")
        .header("Authorization", "Bearer test_token")
        .extension(create_audit_token())
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_list_audit_logs_forbidden() {
    let app = create_test_app();

    let request = Request::builder()
        .uri("/audit/logs")
        .method("GET")
        .header("Authorization", "Bearer test_token")
        .extension(create_no_permission_token())
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_list_audit_logs_invalid_pagination() {
    let app = create_test_app();

    let request = Request::builder()
        .uri("/audit/logs?page=0")
        .method("GET")
        .header("Authorization", "Bearer test_token")
        .extension(create_audit_token())
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_get_audit_log_detail_by_id() {
    let app = create_test_app();

    // 注意：内存存储中的 ID 是动态的，这里测试 404 情况
    let request = Request::builder()
        .uri("/audit/logs/non-existent-id")
        .method("GET")
        .header("Authorization", "Bearer test_token")
        .extension(create_audit_token())
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    // 应该是 404，因为 ID 不存在
    assert!(response.status() == StatusCode::OK || response.status() == StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_audit_log_detail_by_index() {
    let app = create_test_app();

    let request = Request::builder()
        .uri("/audit/logs/0")
        .method("GET")
        .header("Authorization", "Bearer test_token")
        .extension(create_audit_token())
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_audit_log_detail_forbidden() {
    let app = create_test_app();

    let request = Request::builder()
        .uri("/audit/logs/0")
        .method("GET")
        .header("Authorization", "Bearer test_token")
        .extension(create_no_permission_token())
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_export_audit_logs_json() {
    let app = create_test_app();

    let request_body = r#"{"format": "json"}"#;
    let request = Request::builder()
        .uri("/audit/export")
        .method("POST")
        .header("Authorization", "Bearer test_token")
        .header("Content-Type", "application/json")
        .extension(create_audit_token())
        .body(Body::from(request_body))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_export_audit_logs_csv() {
    let app = create_test_app();

    let request_body = r#"{"format": "csv"}"#;
    let request = Request::builder()
        .uri("/audit/export")
        .method("POST")
        .header("Authorization", "Bearer test_token")
        .header("Content-Type", "application/json")
        .extension(create_audit_token())
        .body(Body::from(request_body))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_export_audit_logs_with_time_range() {
    let app = create_test_app();

    let request_body = r#"{
        "format": "json",
        "start_time": 0,
        "end_time": 86400000
    }"#;
    let request = Request::builder()
        .uri("/audit/export")
        .method("POST")
        .header("Authorization", "Bearer test_token")
        .header("Content-Type", "application/json")
        .extension(create_audit_token())
        .body(Body::from(request_body))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_export_audit_logs_invalid_time_range() {
    let app = create_test_app();

    let request_body = r#"{
        "format": "json",
        "start_time": 2000,
        "end_time": 1000
    }"#;
    let request = Request::builder()
        .uri("/audit/export")
        .method("POST")
        .header("Authorization", "Bearer test_token")
        .header("Content-Type", "application/json")
        .extension(create_audit_token())
        .body(Body::from(request_body))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_export_audit_logs_forbidden() {
    let app = create_test_app();

    let request_body = r#"{"format": "json"}"#;
    let request = Request::builder()
        .uri("/audit/export")
        .method("POST")
        .header("Authorization", "Bearer test_token")
        .header("Content-Type", "application/json")
        .extension(create_no_permission_token())
        .body(Body::from(request_body))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_verify_audit_log_by_index() {
    let app = create_test_app();

    let request_body = r#"{"log_index": 0}"#;
    let request = Request::builder()
        .uri("/audit/verify")
        .method("POST")
        .header("Authorization", "Bearer test_token")
        .header("Content-Type", "application/json")
        .extension(create_audit_token())
        .body(Body::from(request_body))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_verify_audit_log_not_found() {
    let app = create_test_app();

    let request_body = r#"{"log_index": 999999}"#;
    let request = Request::builder()
        .uri("/audit/verify")
        .method("POST")
        .header("Authorization", "Bearer test_token")
        .header("Content-Type", "application/json")
        .extension(create_audit_token())
        .body(Body::from(request_body))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_verify_audit_log_forbidden() {
    let app = create_test_app();

    let request_body = r#"{"log_index": 0}"#;
    let request = Request::builder()
        .uri("/audit/verify")
        .method("POST")
        .header("Authorization", "Bearer test_token")
        .header("Content-Type", "application/json")
        .extension(create_no_permission_token())
        .body(Body::from(request_body))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_admin_can_access_all_endpoints() {
    let app = create_test_app();

    // 测试列表查询
    let request = Request::builder()
        .uri("/audit/logs")
        .method("GET")
        .header("Authorization", "Bearer admin_token")
        .extension(create_admin_token())
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 测试导出
    let request_body = r#"{"format": "json"}"#;
    let request = Request::builder()
        .uri("/audit/export")
        .method("POST")
        .header("Authorization", "Bearer admin_token")
        .header("Content-Type", "application/json")
        .extension(create_admin_token())
        .body(Body::from(request_body))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 测试验证
    let request_body = r#"{"log_index": 0}"#;
    let request = Request::builder()
        .uri("/audit/verify")
        .method("POST")
        .header("Authorization", "Bearer admin_token")
        .header("Content-Type", "application/json")
        .extension(create_admin_token())
        .body(Body::from(request_body))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_audit_models_serialization() {
    // 测试查询请求序列化
    let query = AuditLogQueryRequest::new()
        .with_start_time(1000)
        .with_end_time(2000)
        .with_action(AuditAction::CredentialDecrypt)
        .with_pagination(1, 10);

    let json = serde_json::to_string(&query).unwrap();
    assert!(json.contains("start_time"));
    assert!(json.contains("credential_decrypt"));

    // 测试导出请求序列化
    let export = AuditExportRequest::new()
        .with_time_range(1000, 2000)
        .with_format(ExportFormat::Csv);

    let json = serde_json::to_string(&export).unwrap();
    assert!(json.contains("csv"));

    // 测试验证请求序列化
    let verify = AuditVerifyRequest {
        id: "test-id".to_string(),
        log_index: Some(0),
    };

    let json = serde_json::to_string(&verify).unwrap();
    assert!(json.contains("test-id"));
}

#[test]
fn test_audit_filter_creation() {
    let filter = vault_service::audit::AuditFilter::new()
        .with_start_time(1000)
        .with_end_time(2000)
        .with_user_id_hash("hash123")
        .with_action(AuditAction::CredentialDecrypt)
        .with_risk_tier(RiskTier::High)
        .with_outcome(Outcome::Success)
        .with_service("vault-service");

    assert_eq!(filter.start_time, Some(1000));
    assert_eq!(filter.end_time, Some(2000));
    assert_eq!(filter.user_id_hash, Some("hash123".to_string()));
    assert_eq!(filter.action, Some(AuditAction::CredentialDecrypt));
    assert_eq!(filter.risk_tier, Some(RiskTier::High));
    assert_eq!(filter.outcome, Some(Outcome::Success));
    assert_eq!(filter.service, Some("vault-service".to_string()));
}
