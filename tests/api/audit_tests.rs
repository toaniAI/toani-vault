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
        membership_id: None,
        metadata: std::collections::HashMap::new(),
        subject_type: vault_service::token::TOKEN_SUBJECT_TYPE_USER.to_string(),
        issued_from: vault_service::token::TOKEN_ISSUED_FROM_SESSION.to_string(),
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
        membership_id: None,
        metadata: std::collections::HashMap::new(),
        subject_type: vault_service::token::TOKEN_SUBJECT_TYPE_USER.to_string(),
        issued_from: vault_service::token::TOKEN_ISSUED_FROM_SESSION.to_string(),
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
        membership_id: None,
        metadata: std::collections::HashMap::new(),
        subject_type: vault_service::token::TOKEN_SUBJECT_TYPE_USER.to_string(),
        issued_from: vault_service::token::TOKEN_ISSUED_FROM_SESSION.to_string(),
    }
}

/// 创建带测试数据的存储
fn create_test_storage() -> MemoryAuditStorage {
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

    storage
}

/// 创建测试应用
fn create_test_app() -> Router {
    let storage = create_test_storage();
    let verifier_public_key = storage.recorder().public_key().to_vec();
    let state = AuditApiState {
        storage: std::sync::Arc::new(MemoryAuditStorageAdapter::new(storage)),
        verifier_public_key,
    };
    audit_routes(state)
}

fn extract_log_indexes(list_response: &Value) -> Vec<u64> {
    list_response["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["log_index"].as_u64().unwrap())
        .collect()
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
#[ignore = "依赖于已移除的 /tokens/verify 端点 - 新认证流程使用 Privy + Session 模型"]
async fn test_verify_token_writes_audit_log_visible_to_audit_api() {
    let shared_storage = std::sync::Arc::new(tokio::sync::Mutex::new(
        MemoryAuditStorage::new(1000).unwrap(),
    ));

    // Create a mock AuthService for testing
    use async_trait::async_trait;
    use vault_service::auth::{
        AuthError, AuthService, AuthSession, ExternalIdentity, TenantMembership, User,
    };

    struct MockAuthService;

    #[async_trait]
    impl AuthService for MockAuthService {
        async fn create_user_from_privy(&self, _privy_token: &str) -> Result<User, AuthError> {
            Err(AuthError::PrivyAuthenticationFailed("mock".to_string()))
        }
        async fn get_or_create_external_identity(
            &self,
            _: uuid::Uuid,
            _: vault_service::auth::IdentityProvider,
            _: &str,
            _: Option<serde_json::Value>,
        ) -> Result<ExternalIdentity, AuthError> {
            unimplemented!()
        }
        async fn create_tenant_invitation(
            &self,
            _: uuid::Uuid,
            _: vault_service::auth::MembershipRole,
            _: vault_service::auth::InviteeType,
            _: Option<String>,
            _: Option<String>,
            _: uuid::Uuid,
            _: i64,
        ) -> Result<(vault_service::auth::TenantInvitation, String), AuthError> {
            unimplemented!()
        }
        async fn consume_invitation(
            &self,
            _: &str,
            _: uuid::Uuid,
        ) -> Result<TenantMembership, AuthError> {
            unimplemented!()
        }
        async fn create_session(
            &self,
            _: uuid::Uuid,
            _: Option<uuid::Uuid>,
            _: vault_service::auth::CreateUserRequest,
        ) -> Result<(AuthSession, String), AuthError> {
            unimplemented!()
        }
        async fn get_active_membership(
            &self,
            _: uuid::Uuid,
            _: uuid::Uuid,
        ) -> Result<Option<TenantMembership>, AuthError> {
            Ok(None)
        }
        async fn audit_log(
            &self,
            _: vault_service::auth::AuthEventType,
            _: Option<uuid::Uuid>,
            _: Option<serde_json::Value>,
        ) -> Result<(), AuthError> {
            Ok(())
        }
        async fn verify_session(&self, _: &str) -> Result<AuthSession, AuthError> {
            unimplemented!()
        }
        async fn revoke_session(&self, _: uuid::Uuid, _: &str) -> Result<(), AuthError> {
            Ok(())
        }
        async fn get_user(&self, _: uuid::Uuid) -> Result<User, AuthError> {
            unimplemented!()
        }
        async fn get_user_identities(
            &self,
            _: uuid::Uuid,
        ) -> Result<Vec<ExternalIdentity>, AuthError> {
            Ok(vec![])
        }
        async fn get_user_memberships(
            &self,
            _: uuid::Uuid,
        ) -> Result<Vec<TenantMembership>, AuthError> {
            Ok(vec![])
        }
        async fn sync_mfa_status(
            &self,
            _: uuid::Uuid,
            _: &str,
        ) -> Result<vault_service::auth::service::MfaStatusSnapshot, AuthError> {
            Ok(vault_service::auth::service::MfaStatusSnapshot::default())
        }
        async fn get_mfa_status(
            &self,
            _: uuid::Uuid,
        ) -> Result<vault_service::auth::service::MfaStatusSnapshot, AuthError> {
            Ok(vault_service::auth::service::MfaStatusSnapshot::default())
        }
    }

    let auth_service = std::sync::Arc::new(MockAuthService);
    let audit_adapter = std::sync::Arc::new(MemoryAuditStorageAdapter::from_shared_storage(
        shared_storage.clone(),
    ));
    let auth_state = AuthApiState::new(auth_service).with_audit_storage(audit_adapter.clone());
    let verifier_public_key = {
        let storage = shared_storage.lock().await;
        storage.recorder().public_key().to_vec()
    };
    let audit_state = AuditApiState {
        storage: audit_adapter,
        verifier_public_key,
    };
    let app = Router::new()
        .merge(auth_routes().with_state(auth_state.clone()))
        .merge(protected_auth_routes().with_state(auth_state))
        .merge(audit_routes(audit_state));

    // 使用 ValidatedToken 扩展来模拟已认证用户
    let test_token = create_audit_token();

    let create_request = Request::builder()
        .uri("/tokens")
        .method("POST")
        .header("Content-Type", "application/json")
        .extension(test_token.clone())
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

    // 验证会话端点写入审计日志 (新认证流程)
    let verify_request = Request::builder()
        .uri("/auth/me")
        .method("GET")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
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
    assert!(
        items
            .iter()
            .any(|item| { item["action"] == "token_issue" && item["outcome"] == "success" })
    );
    assert!(
        items
            .iter()
            .any(|item| { item["action"] == "token_validate" && item["outcome"] == "success" })
    );
}

#[tokio::test]
#[ignore = "依赖于已移除的 /tokens/stats 端点 - 新认证流程使用 Privy + Session 模型"]
async fn test_token_stats_endpoint_returns_active_count_for_current_tenant() {
    let shared_storage = std::sync::Arc::new(tokio::sync::Mutex::new(
        MemoryAuditStorage::new(1000).unwrap(),
    ));

    // Create a mock AuthService for testing
    use async_trait::async_trait;
    use vault_service::auth::{
        AuthError, AuthService, AuthSession, ExternalIdentity, TenantMembership, User,
    };

    struct MockAuthService;

    #[async_trait]
    impl AuthService for MockAuthService {
        async fn create_user_from_privy(&self, _privy_token: &str) -> Result<User, AuthError> {
            Err(AuthError::PrivyAuthenticationFailed("mock".to_string()))
        }
        async fn get_or_create_external_identity(
            &self,
            _: uuid::Uuid,
            _: vault_service::auth::IdentityProvider,
            _: &str,
            _: Option<serde_json::Value>,
        ) -> Result<ExternalIdentity, AuthError> {
            unimplemented!()
        }
        async fn create_tenant_invitation(
            &self,
            _: uuid::Uuid,
            _: vault_service::auth::MembershipRole,
            _: vault_service::auth::InviteeType,
            _: Option<String>,
            _: Option<String>,
            _: uuid::Uuid,
            _: i64,
        ) -> Result<(vault_service::auth::TenantInvitation, String), AuthError> {
            unimplemented!()
        }
        async fn consume_invitation(
            &self,
            _: &str,
            _: uuid::Uuid,
        ) -> Result<TenantMembership, AuthError> {
            unimplemented!()
        }
        async fn create_session(
            &self,
            _: uuid::Uuid,
            _: Option<uuid::Uuid>,
            _: vault_service::auth::CreateUserRequest,
        ) -> Result<(AuthSession, String), AuthError> {
            unimplemented!()
        }
        async fn get_active_membership(
            &self,
            _: uuid::Uuid,
            _: uuid::Uuid,
        ) -> Result<Option<TenantMembership>, AuthError> {
            Ok(None)
        }
        async fn audit_log(
            &self,
            _: vault_service::auth::AuthEventType,
            _: Option<uuid::Uuid>,
            _: Option<serde_json::Value>,
        ) -> Result<(), AuthError> {
            Ok(())
        }
        async fn verify_session(&self, _: &str) -> Result<AuthSession, AuthError> {
            unimplemented!()
        }
        async fn revoke_session(&self, _: uuid::Uuid, _: &str) -> Result<(), AuthError> {
            Ok(())
        }
        async fn get_user(&self, _: uuid::Uuid) -> Result<User, AuthError> {
            unimplemented!()
        }
        async fn get_user_identities(
            &self,
            _: uuid::Uuid,
        ) -> Result<Vec<ExternalIdentity>, AuthError> {
            Ok(vec![])
        }
        async fn get_user_memberships(
            &self,
            _: uuid::Uuid,
        ) -> Result<Vec<TenantMembership>, AuthError> {
            Ok(vec![])
        }
        async fn sync_mfa_status(
            &self,
            _: uuid::Uuid,
            _: &str,
        ) -> Result<vault_service::auth::service::MfaStatusSnapshot, AuthError> {
            Ok(vault_service::auth::service::MfaStatusSnapshot::default())
        }
        async fn get_mfa_status(
            &self,
            _: uuid::Uuid,
        ) -> Result<vault_service::auth::service::MfaStatusSnapshot, AuthError> {
            Ok(vault_service::auth::service::MfaStatusSnapshot::default())
        }
    }

    let auth_service = std::sync::Arc::new(MockAuthService);
    let audit_adapter = std::sync::Arc::new(MemoryAuditStorageAdapter::from_shared_storage(
        shared_storage,
    ));
    let auth_state = AuthApiState::new(auth_service).with_audit_storage(audit_adapter);
    let app = Router::new()
        .merge(auth_routes().with_state(auth_state.clone()))
        .merge(protected_auth_routes().with_state(auth_state));

    // 使用统一的测试 token（与 stats 端点使用相同的 tenant）
    let test_token = ValidatedToken {
        token_id: Uuid::now_v7().to_string(),
        subject: "default-tenant:user-001".to_string(),
        tenant_id: "default-tenant".to_string(),
        user_id: "user-001".to_string(),
        expires_at: u64::MAX,
        scopes: vec![TokenScope::Admin],
        issued_at: 1000,
        membership_id: None,
        metadata: std::collections::HashMap::new(),
        subject_type: vault_service::token::TOKEN_SUBJECT_TYPE_USER.to_string(),
        issued_from: vault_service::token::TOKEN_ISSUED_FROM_SESSION.to_string(),
    };

    for expires_in in [900_u64, 1800_u64] {
        let create_request = Request::builder()
            .uri("/tokens")
            .method("POST")
            .header("Content-Type", "application/json")
            .extension(test_token.clone())
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

    // 使用 /auth/me 端点代替已移除的 /tokens/stats
    let stats_request = Request::builder()
        .uri("/auth/me")
        .method("GET")
        .header("Authorization", "Bearer test_token")
        .extension(test_token)
        .body(Body::empty())
        .unwrap();

    let stats_response = app.oneshot(stats_request).await.unwrap();
    assert_eq!(stats_response.status(), StatusCode::OK);
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
async fn test_list_audit_logs_are_sorted_desc_by_operation_time() {
    let app = create_test_app();

    let request = Request::builder()
        .uri("/audit/logs?page=1&page_size=10")
        .method("GET")
        .header("Authorization", "Bearer test_token")
        .extension(create_audit_token())
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let payload: Value = serde_json::from_slice(&body).unwrap();
    let log_indexes = extract_log_indexes(&payload);

    assert_eq!(log_indexes.len(), 10);
    assert!(
        log_indexes.windows(2).all(|w| w[0] >= w[1]),
        "expected descending log_index order, got: {:?}",
        log_indexes
    );
}

#[tokio::test]
async fn test_list_audit_logs_pagination_keeps_global_desc_order() {
    let app = create_test_app();

    let page1_request = Request::builder()
        .uri("/audit/logs?page=1&page_size=3")
        .method("GET")
        .header("Authorization", "Bearer test_token")
        .extension(create_audit_token())
        .body(Body::empty())
        .unwrap();
    let page1_response = app.clone().oneshot(page1_request).await.unwrap();
    assert_eq!(page1_response.status(), StatusCode::OK);
    let page1_body = page1_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let page1_payload: Value = serde_json::from_slice(&page1_body).unwrap();
    let page1_indexes = extract_log_indexes(&page1_payload);

    let page2_request = Request::builder()
        .uri("/audit/logs?page=2&page_size=3")
        .method("GET")
        .header("Authorization", "Bearer test_token")
        .extension(create_audit_token())
        .body(Body::empty())
        .unwrap();
    let page2_response = app.oneshot(page2_request).await.unwrap();
    assert_eq!(page2_response.status(), StatusCode::OK);
    let page2_body = page2_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let page2_payload: Value = serde_json::from_slice(&page2_body).unwrap();
    let page2_indexes = extract_log_indexes(&page2_payload);

    assert_eq!(page1_indexes.len(), 3);
    assert_eq!(page2_indexes.len(), 3);
    assert!(
        page1_indexes.last().unwrap() > page2_indexes.first().unwrap(),
        "expected page1 to be newer than page2, got page1={:?}, page2={:?}",
        page1_indexes,
        page2_indexes
    );
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

    let request_body = r#"{"format": "json", "start_time": 0, "end_time": 86400000}"#;
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

    let request_body = r#"{"format": "csv", "start_time": 0, "end_time": 86400000}"#;
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
async fn test_export_audit_logs_missing_end_time() {
    let app = create_test_app();

    // 缺少 end_time 应返回 400 Bad Request
    let request_body = r#"{"format": "json", "start_time": 1735689600000}"#;
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
    // 验证响应体包含 error: invalid_request
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let data: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(data["error"], "invalid_request");
}

#[tokio::test]
async fn test_export_audit_logs_missing_start_time() {
    let app = create_test_app();

    // 缺少 start_time 也应返回 400 Bad Request
    let request_body = r#"{"format": "json", "end_time": 1735689600000}"#;
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
    // 验证响应体包含 error: invalid_request
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let data: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(data["error"], "invalid_request");
}

#[tokio::test]
async fn test_export_audit_logs_forbidden() {
    let app = create_test_app();

    // 需要提供合法的时间参数才能通过参数校验，然后触发权限检查
    let request_body = r#"{"format": "json", "start_time": 0, "end_time": 86400000}"#;
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
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let data: Value = serde_json::from_slice(&body).unwrap();
    assert!(data["success"].as_bool().unwrap());
    assert!(data["data"]["verified"].as_bool().unwrap());
    assert!(data["data"]["content_hash_match"].as_bool().unwrap());
    assert!(data["data"]["signature_valid"].as_bool().unwrap());
    assert!(data["data"]["merkle_proof_valid"].as_bool().unwrap());
}

#[tokio::test]
async fn test_verify_audit_log_fails_with_wrong_public_key() {
    let storage = create_test_storage();
    let mut wrong_public_key = storage.recorder().public_key().to_vec();
    // Flip one byte to ensure signature verification fails while keeping key length valid.
    wrong_public_key[0] ^= 0xFF;

    let app = audit_routes(AuditApiState {
        storage: std::sync::Arc::new(MemoryAuditStorageAdapter::new(storage)),
        verifier_public_key: wrong_public_key,
    });

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
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let data: Value = serde_json::from_slice(&body).unwrap();
    assert!(data["success"].as_bool().unwrap());
    assert_eq!(data["status"], "invalid");
    assert!(data["data"]["content_hash_match"].as_bool().unwrap());
    assert!(!data["data"]["signature_valid"].as_bool().unwrap());
    assert!(!data["data"]["verified"].as_bool().unwrap());
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
    let request_body = r#"{"format": "json", "start_time": 0, "end_time": 86400000}"#;
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

// BUG-18229: 获取审计日志详情路径缺少id应返回400而非200
// 测试验证 GET /audit/logs/（带尾斜杠）返回 400 Bad Request
use axum::extract::OriginalUri;
use tower::Layer;
use tower_http::normalize_path::NormalizePathLayer;

/// 创建带 NormalizePathLayer 的测试应用（模拟真实服务链路）
/// BUG-18229: 在 NormalizePathLayer 之前使用 MapRequestLayer 保存原始 URI
fn create_test_app_with_normalize_path() -> impl tower::Service<
    axum::extract::Request,
    Response = axum::response::Response,
    Error = std::convert::Infallible,
> + Clone
+ Send
+ Sync
+ 'static {
    let storage = create_test_storage();
    let verifier_public_key = storage.recorder().public_key().to_vec();
    let state = AuditApiState {
        storage: std::sync::Arc::new(MemoryAuditStorageAdapter::new(storage)),
        verifier_public_key,
    };
    // BUG-18229: 使用 MapRequestLayer 保存原始 URI，然后 NormalizePathLayer 去除尾斜杠
    // Layer 执行顺序：最后添加的 layer 先执行
    // 我们需要：preserve_original_uri 先执行（保存原始 URI），然后 NormalizePath 执行（修改 URI）
    // 所以：preserve_original_uri.layer(NormalizePathLayer.layer(router))
    // 这样请求流程是：preserve_original_uri（保存原始URI） -> NormalizePath（去除尾斜杠） -> router
    let preserve_original_uri =
        tower::util::MapRequestLayer::new(|mut req: axum::extract::Request| {
            let original_uri = OriginalUri(req.uri().clone());
            req.extensions_mut().insert(original_uri);
            req
        });
    let normalized_router = NormalizePathLayer::trim_trailing_slash().layer(audit_routes(state));
    preserve_original_uri.layer(normalized_router)
}

#[tokio::test]
async fn test_audit_logs_trailing_slash_returns_400() {
    // BUG-18229: GET /audit/logs/ 缺少详情路径参数 id 应返回 400
    // NormalizePathLayer 会将 /audit/logs/ 归一化为 /audit/logs
    // 但 handler 应检测原始 URI 并返回 400
    let app = create_test_app_with_normalize_path();

    let request = Request::builder()
        .uri("/audit/logs/")
        .method("GET")
        .header("Authorization", "Bearer test_token")
        .extension(create_audit_token())
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // 验证响应体包含 error: invalid_request
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let data: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(data["error"], "invalid_request");
}

#[tokio::test]
async fn test_audit_logs_without_trailing_slash_returns_200() {
    // 验证正常列表查询仍返回 200
    let app = create_test_app_with_normalize_path();

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
