#![allow(clippy::field_reassign_with_default)]
#![allow(dead_code)]
#![allow(clippy::uninlined_format_args)]

//! 认证服务 API 集成测试
//!
//! 测试 EP8-Story8.2 实现的认证服务 API：
//! - POST /api/v1/attestation/challenge - 创建认证挑战
//! - POST /api/v1/attestation/verify-response - 验证挑战响应
//! - GET /api/v1/attestation/status - 获取认证状态

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use vault_service::api::{AttestationApiConfig, attestation_routes, init_attestation_api};

/// 创建测试用的认证 API 状态
fn create_test_state() -> std::sync::Arc<vault_service::api::AttestationState> {
    let config = AttestationApiConfig {
        simulation_mode: true,
        require_api_key: false,
        quote_max_age: 3600,
        enable_pcs_registration: false,
    };

    init_attestation_api(config).expect("Failed to initialize attestation API")
}

/// 测试创建认证挑战端点
#[tokio::test]
async fn test_create_challenge_endpoint() {
    let state = create_test_state();
    let app = attestation_routes(state);

    let request = Request::builder()
        .method("POST")
        .uri("/challenge")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{}"#))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["success"].as_bool().unwrap());
    assert!(!json["challenge_id"].as_str().unwrap().is_empty());
    assert!(!json["nonce"].as_str().unwrap().is_empty());
    assert!(!json["quote_b64"].as_str().unwrap().is_empty());
    assert!(!json["mrenclave"].as_str().unwrap().is_empty());
    assert!(!json["mrsigner"].as_str().unwrap().is_empty());
    assert!(json["expires_at"].as_u64().unwrap() > 0);
}

/// 测试创建认证挑战带 Enclave ID
#[tokio::test]
async fn test_create_challenge_with_enclave_id() {
    let state = create_test_state();
    let app = attestation_routes(state);

    let request = Request::builder()
        .method("POST")
        .uri("/challenge")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"enclave_id": "test_enclave_123"}"#))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["success"].as_bool().unwrap());
    assert!(!json["challenge_id"].as_str().unwrap().is_empty());
}

/// 测试完整的挑战-响应流程
#[tokio::test]
async fn test_challenge_response_full_flow() {
    let state = create_test_state();
    let app = attestation_routes(state.clone());

    // 步骤 1: 创建挑战
    let challenge_request = Request::builder()
        .method("POST")
        .uri("/challenge")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{}"#))
        .unwrap();

    let challenge_response = app.clone().oneshot(challenge_request).await.unwrap();
    assert_eq!(challenge_response.status(), StatusCode::OK);

    let challenge_body = axum::body::to_bytes(challenge_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let challenge_json: serde_json::Value = serde_json::from_slice(&challenge_body).unwrap();

    let challenge_id = challenge_json["challenge_id"].as_str().unwrap().to_string();
    let quote_b64 = challenge_json["quote_b64"].as_str().unwrap().to_string();

    // 步骤 2: 验证挑战响应
    let verify_request_body = format!(
        r#"{{"challenge_id": "{}", "quote_b64": "{}"}}"#,
        challenge_id, quote_b64
    );

    let verify_request = Request::builder()
        .method("POST")
        .uri("/verify-response")
        .header("Content-Type", "application/json")
        .body(Body::from(verify_request_body))
        .unwrap();

    let verify_response = app.oneshot(verify_request).await.unwrap();
    assert_eq!(verify_response.status(), StatusCode::OK);

    let verify_body = axum::body::to_bytes(verify_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let verify_json: serde_json::Value = serde_json::from_slice(&verify_body).unwrap();

    assert!(verify_json["success"].as_bool().unwrap());
    // 由于 Quote 是由同一个 Enclave 生成的，验证应该成功
    assert!(verify_json["verified"].as_bool().unwrap());
    assert!(!verify_json["mrenclave"].as_str().unwrap().is_empty());
    assert!(!verify_json["mrsigner"].as_str().unwrap().is_empty());
}

/// 测试验证无效的挑战响应
#[tokio::test]
async fn test_verify_invalid_challenge_response() {
    let state = create_test_state();
    let app = attestation_routes(state);

    // 使用无效的 quote 验证
    let verify_request_body = r#"{
        "challenge_id": "non_existent_challenge",
        "quote_b64": "invalid_base64!!!"
    }"#;

    let verify_request = Request::builder()
        .method("POST")
        .uri("/verify-response")
        .header("Content-Type", "application/json")
        .body(Body::from(verify_request_body))
        .unwrap();

    let verify_response = app.oneshot(verify_request).await.unwrap();
    // 应该返回 400 Bad Request
    assert_eq!(verify_response.status(), StatusCode::BAD_REQUEST);
}

/// 测试获取认证状态端点
#[tokio::test]
async fn test_get_attestation_status_endpoint() {
    let state = create_test_state();
    let app = attestation_routes(state);

    let request = Request::builder()
        .method("GET")
        .uri("/status")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["success"].as_bool().unwrap());
    // 验证状态字段
    let status = json["status"].as_str().unwrap();
    assert!(
        status == "authenticated" || status == "pending_verification" || status == "expired",
        "Unexpected status: {}",
        status
    );

    assert!(!json["enclave_state"].as_str().unwrap().is_empty());
    assert!(!json["mrenclave"].as_str().unwrap().is_empty());
    assert!(!json["mrsigner"].as_str().unwrap().is_empty());
    assert!(json["quote_valid"].is_boolean());
}

/// 测试 Quote 获取端点
#[tokio::test]
async fn test_get_quote_endpoint() {
    let state = create_test_state();
    let app = attestation_routes(state);

    let request = Request::builder()
        .method("GET")
        .uri("/quote")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["success"].as_bool().unwrap());
    assert!(json["data"].is_object());
    assert!(!json["data"]["quote_b64"].as_str().unwrap().is_empty());
    assert!(!json["data"]["mrenclave"].as_str().unwrap().is_empty());
    assert!(!json["data"]["mrsigner"].as_str().unwrap().is_empty());
}

/// 测试健康检查端点
#[tokio::test]
async fn test_health_check_endpoint() {
    let state = create_test_state();
    let app = attestation_routes(state);

    let request = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"].as_str().unwrap(), "healthy");
    assert!(json["quote_valid"].is_boolean());
}

/// 测试重复验证（重放攻击防护）
#[tokio::test]
async fn test_replay_protection() {
    let state = create_test_state();
    let app = attestation_routes(state.clone());

    // 创建挑战
    let challenge_request = Request::builder()
        .method("POST")
        .uri("/challenge")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{}"#))
        .unwrap();

    let challenge_response = app.clone().oneshot(challenge_request).await.unwrap();
    let challenge_body = axum::body::to_bytes(challenge_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let challenge_json: serde_json::Value = serde_json::from_slice(&challenge_body).unwrap();

    let challenge_id = challenge_json["challenge_id"].as_str().unwrap().to_string();
    let quote_b64 = challenge_json["quote_b64"].as_str().unwrap().to_string();

    // 第一次验证
    let verify_request_body = format!(
        r#"{{"challenge_id": "{}", "quote_b64": "{}"}}"#,
        challenge_id, quote_b64
    );

    let verify_request = Request::builder()
        .method("POST")
        .uri("/verify-response")
        .header("Content-Type", "application/json")
        .body(Body::from(verify_request_body.clone()))
        .unwrap();

    let first_verify = app.clone().oneshot(verify_request).await.unwrap();
    assert_eq!(first_verify.status(), StatusCode::OK);

    let first_body = axum::body::to_bytes(first_verify.into_body(), usize::MAX)
        .await
        .unwrap();
    let first_json: serde_json::Value = serde_json::from_slice(&first_body).unwrap();
    assert!(first_json["verified"].as_bool().unwrap());

    // 第二次使用相同的挑战验证（应该失败）
    let verify_request2 = Request::builder()
        .method("POST")
        .uri("/verify-response")
        .header("Content-Type", "application/json")
        .body(Body::from(verify_request_body))
        .unwrap();

    let second_verify = app.oneshot(verify_request2).await.unwrap();
    assert_eq!(second_verify.status(), StatusCode::OK);

    let second_body = axum::body::to_bytes(second_verify.into_body(), usize::MAX)
        .await
        .unwrap();
    let second_json: serde_json::Value = serde_json::from_slice(&second_body).unwrap();

    // 第二次验证应该失败，因为挑战已经被使用
    assert!(!second_json["verified"].as_bool().unwrap());
    assert!(
        second_json["error"]
            .as_str()
            .unwrap()
            .contains("Challenge not found")
    );
}
