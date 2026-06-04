use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;
use toani_vault_sdk::{
    client::CredBridgeClient,
    sandbox::SandboxService,
    types::{
        CredBridgeConfig, ExecuteSandboxOperationRequest, SandboxOperationType,
    },
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

fn create_test_token() -> String {
    let payload = serde_json::json!({
        "jti": "test_token_id",
        "sub": "tenant1:user1",
        "tenant_id": "tenant1",
        "exp": (chrono::Utc::now().timestamp() + 3600) as i64,
        "iat": chrono::Utc::now().timestamp() as i64,
        "scope": "sandbox:read sandbox:write sandbox:execute",
        "mfa_verified": true,
    });

    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload.to_string().as_bytes());
    format!("v4.local.{}.signature", payload_b64)
}

fn create_test_config(server_url: &str) -> CredBridgeConfig {
    CredBridgeConfig::new(server_url)
        .with_token(create_test_token())
        .with_timeout_ms(5000)
        .with_max_retries(0)
}

#[tokio::test]
async fn request_posts_broker_payload_to_http_requests_route() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/sandbox/http-requests"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "data": {
                "operation_id": "op-1",
                "success": true,
                "data": {
                    "status": 200
                },
                "error": null,
                "execution_time_ms": 12
            }
        })))
        .mount(&mock_server)
        .await;

    let config = create_test_config(&mock_server.uri());
    let client = Arc::new(CredBridgeClient::new(config).unwrap());
    let sandbox = SandboxService::new(client);

    let mut parameters = HashMap::new();
    parameters.insert("url".to_string(), json!("https://api.example.com/health"));
    parameters.insert("method".to_string(), json!("GET"));
    let request = ExecuteSandboxOperationRequest {
        operation_type: SandboxOperationType::HttpRequest,
        description: "health check".to_string(),
        parameters,
    };

    let result = sandbox.request(request, None).await.unwrap();
    assert!(result.success);
    assert_eq!(result.operation_id, "op-1");
}

#[tokio::test]
async fn get_request_reads_broker_operation_detail() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/sandbox/http-requests/op-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "data": {
                "operation_id": "op-1",
                "session_id": "",
                "operation_type": "http_request",
                "status": "success",
                "started_at": "2026-05-17T00:00:00Z",
                "completed_at": "2026-05-17T00:00:01Z",
                "execution_time_ms": 100
            }
        })))
        .mount(&mock_server)
        .await;

    let config = create_test_config(&mock_server.uri());
    let client = Arc::new(CredBridgeClient::new(config).unwrap());
    let sandbox = SandboxService::new(client);

    let result = sandbox.get_request("op-1", None).await.unwrap();
    assert_eq!(result.operation_id, "op-1");
    assert_eq!(result.operation_type, "http_request");
}
