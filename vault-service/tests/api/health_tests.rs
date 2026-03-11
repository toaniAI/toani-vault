//! 健康检查 API 测试

use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::get,
    Router,
};
use std::sync::Arc;
use tower::ServiceExt;
use vault_service::api::{
    health, health_check, health_check_detail, HealthConfig, HealthState, HealthStatus,
};

/// 创建测试用的 Router
fn create_test_app() -> Router {
    let config = HealthConfig::default();
    let state = Arc::new(HealthState::new(config));

    Router::new()
        .route("/health", get(health_check))
        .route("/health/detail", get(health_check_detail))
        .with_state(state)
}

/// 创建带健康检查函数的测试应用
fn create_test_app_with_checks() -> Router {
    let config = HealthConfig::default();
    let state = Arc::new(
        HealthState::new(config)
            .with_db_check(health::mock_db_health_check)
            .with_redis_check(health::mock_redis_health_check)
            .with_tee_check(health::mock_tee_health_check),
    );

    Router::new()
        .route("/health", get(health_check))
        .route("/health/detail", get(health_check_detail))
        .with_state(state)
}

#[tokio::test]
async fn test_health_check_basic() {
    let app = create_test_app();

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_health_check_returns_json() {
    let app = create_test_app();

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let content_type = response
        .headers()
        .get("content-type")
        .expect("Content-Type header should exist");
    assert!(content_type.to_str().unwrap().contains("application/json"));
}

#[tokio::test]
async fn test_health_check_with_components() {
    let app = create_test_app_with_checks();

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();

    // 验证响应包含预期字段
    assert!(body_str.contains("\"status\""));
    assert!(body_str.contains("\"service\""));
    assert!(body_str.contains("\"version\""));
    assert!(body_str.contains("\"timestamp\""));
    assert!(body_str.contains("\"uptime_seconds\""));
    assert!(body_str.contains("\"components\""));

    // 验证组件信息
    assert!(body_str.contains("database"));
    assert!(body_str.contains("redis"));
    assert!(body_str.contains("tee"));
}

#[tokio::test]
async fn test_health_check_detail() {
    let app = create_test_app_with_checks();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health/detail")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();

    // 验证详细响应包含系统信息
    assert!(body_str.contains("\"system\""));
    assert!(body_str.contains("\"tee_details\""));
}

#[tokio::test]
async fn test_health_check_service_name() {
    let app = create_test_app();

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["service"], "credbridge-vault");
    assert!(json["version"].as_str().unwrap().len() > 0);
}

#[tokio::test]
async fn test_health_status_values() {
    let app = create_test_app_with_checks();

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // 验证状态值为 "healthy" | "degraded" | "unhealthy"
    let status = json["status"].as_str().unwrap();
    assert!(
        status == "healthy" || status == "degraded" || status == "unhealthy",
        "Status should be one of healthy, degraded, or unhealthy"
    );
}

#[tokio::test]
async fn test_health_check_components_structure() {
    let app = create_test_app_with_checks();

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let components = json["components"].as_array().expect("Components should be an array");
    assert!(!components.is_empty(), "Should have at least one component");

    // 验证组件结构
    for component in components {
        assert!(component["name"].is_string());
        assert!(component["status"].is_string());

        let status = component["status"].as_str().unwrap();
        assert!(
            status == "healthy" || status == "degraded" || status == "unhealthy"
        );

        // latency_ms 可能为 null 或 number
        if !component["latency_ms"].is_null() {
            assert!(component["latency_ms"].is_u64());
        }
    }
}

#[tokio::test]
async fn test_health_check_timestamp_and_uptime() {
    let app = create_test_app();

    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // 验证时间戳是正整数
    let timestamp = json["timestamp"].as_u64().expect("Timestamp should be a positive integer");
    assert!(timestamp > 0);

    // 验证运行时间是整数
    let uptime = json["uptime_seconds"].as_u64().expect("Uptime should be an integer");
    // 由于测试很快，运行时间应该是 0 或很小的数
    assert!(uptime < 10);
}
