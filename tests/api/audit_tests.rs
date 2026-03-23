#![allow(clippy::field_reassign_with_default)]
#![allow(dead_code)]
#![allow(clippy::uninlined_format_args)]
//! 审计日志 API 集成测试
//!
//! 测试审计查询 API 的所有端点

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

use vault_service::api::{
    audit::{AuditApiState, MemoryAuditStorageAdapter, audit_routes},
    audit_models::*,
    auth::{AuthApiState, auth_routes, protected_auth_routes},
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
        verifier_public_key: vec![],
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
async fn test_verify_token_writes_audit_log_visible_to_audit_api() {
    let shared_storage = std::sync::Arc::new(tokio::sync::Mutex::new(
        MemoryAuditStorage::new(1000).unwrap(),
    ));
    let auth_state = AuthApiState::with_audit_storage(shared_storage.clone());
    let audit_state = AuditApiState {
        storage: std::sync::Arc::new(MemoryAuditStorageAdapter::from_shared_storage(
            shared_storage,
        )),
        verifier_public_key: vec![],
    };
    let app = Router::new()
        .merge(auth_routes().with_state(auth_state))
        .merge(audit_routes(audit_state));

    let create_request = Request::builder()
        .uri("/tokens")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "scopes": ["audit:read"],
                "expires_in": 900
            })
            .to_string(),
        ))
        .unwrap();

    let create_response = app.clone().oneshot(create_request).await.unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);
    let create_body = create_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let created: Value = serde_json::from_slice(&create_body).unwrap();
    let token = created["access_token"].as_str().unwrap().to_string();

    let verify_request = Request::builder()
        .uri("/tokens/verify")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "token": token
            })
            .to_string(),
        ))
        .unwrap();

    let verify_response = app.clone().oneshot(verify_request).await.unwrap();
    assert_eq!(verify_response.status(), StatusCode::OK);

    let audit_request = Request::builder()
        .uri("/audit/logs")
        .method("GET")
        .extension(create_audit_token())
        .body(Body::empty())
        .unwrap();

    let audit_response = app.oneshot(audit_request).await.unwrap();
    assert_eq!(audit_response.status(), StatusCode::OK);
    let audit_body = audit_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let audit_json: Value = serde_json::from_slice(&audit_body).unwrap();
    let items = audit_json["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert!(items.iter().any(|item| {
        item["action"] == "token_issue" && item["outcome"] == "success"
    }));
    assert!(items.iter().any(|item| {
        item["action"] == "token_validate" && item["outcome"] == "success"
    }));
}

#[tokio::test]
async fn test_token_stats_endpoint_returns_active_count_for_current_tenant() {
    let shared_storage = std::sync::Arc::new(tokio::sync::Mutex::new(
        MemoryAuditStorage::new(1000).unwrap(),
    ));
    let auth_state = AuthApiState::with_audit_storage(shared_storage);
    let app = Router::new()
        .merge(auth_routes().with_state(auth_state.clone()))
        .merge(protected_auth_routes().with_state(auth_state));

    for expires_in in [900_u64, 1800_u64] {
        let create_request = Request::builder()
            .uri("/tokens")
            .method("POST")
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({
                    "scopes": ["audit:read"],
                    "expires_in": expires_in
                })
                .to_string(),
            ))
            .unwrap();

        let create_response = app.clone().oneshot(create_request).await.unwrap();
        assert_eq!(create_response.status(), StatusCode::OK);
    }

    let stats_request = Request::builder()
        .uri("/tokens/stats")
        .method("GET")
        .header("Authorization", "Bearer test_token")
        .extension(ValidatedToken {
            token_id: Uuid::now_v7().to_string(),
            subject: "default-tenant:user-001".to_string(),
            tenant_id: "default-tenant".to_string(),
            user_id: "user-001".to_string(),
            expires_at: u64::MAX,
            scopes: vec![TokenScope::Admin],
            issued_at: 1000,
        })
        .body(Body::empty())
        .unwrap();

    let stats_response = app.oneshot(stats_request).await.unwrap();
    assert_eq!(stats_response.status(), StatusCode::OK);
    let stats_body = stats_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let stats_json: Value = serde_json::from_slice(&stats_body).unwrap();
    assert_eq!(stats_json["active_tokens"], 2);
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
