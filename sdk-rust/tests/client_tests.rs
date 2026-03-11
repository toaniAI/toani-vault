//! CredBridge SDK 集成测试
//!
//! 测试 SDK 的核心功能，包括客户端、凭证管理和 Token 管理。

use std::collections::HashMap;

use credbridge_sdk::{
    client::CredBridgeClient,
    credentials::CredentialsService,
    token::TokenManager,
    types::{CredBridgeConfig, CredBridgeErrorCode, CredentialType, RequestOptions},
    CredBridgeSDK,
};
use serde_json::json;
use wiremock::{
    matchers::{header, method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

/// 创建测试用的 PASETO token
fn create_test_token() -> String {
    // 这是一个模拟的 PASETO v4.local token
    // 实际格式: version.purpose.payload.signature
    // 这里我们使用简化版本用于测试
    let payload = serde_json::json!({
        "jti": "test_token_id",
        "sub": "tenant1:user1",
        "tenant_id": "tenant1",
        "exp": (chrono::Utc::now().timestamp() + 3600) as i64,
        "iat": chrono::Utc::now().timestamp() as i64,
        "scope": "credential:read credential:write credential:decrypt audit:read",
        "mfa_verified": true,
    });

    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload.to_string().as_bytes());
    format!("v4.local.{}.signature", payload_b64)
}

/// 创建测试配置
fn create_test_config(server_url: &str) -> CredBridgeConfig {
    CredBridgeConfig::new(server_url)
        .with_token(create_test_token())
        .with_timeout_ms(5000)
        .with_max_retries(0) // 测试中禁用重试
}

#[tokio::test]
async fn test_client_creation() {
    let config = CredBridgeConfig::new("https://api.credbridge.io")
        .with_token(create_test_token());

    let client = CredBridgeClient::new(config);
    assert!(client.is_ok());

    let client = client.unwrap();
    assert!(client.get_token().is_some());
    assert!(client.get_token_info().is_some());
}

#[tokio::test]
async fn test_token_parsing() {
    let config = CredBridgeConfig::new("https://api.credbridge.io");
    let client = CredBridgeClient::new(config).unwrap();

    // 设置 token
    client.set_token(create_test_token());

    let token_info = client.get_token_info().unwrap();
    assert_eq!(token_info.token_id, "test_token_id");
    assert_eq!(token_info.tenant_id, "tenant1");
    assert_eq!(token_info.user_id, "user1");
    assert!(token_info.expires_at > chrono::Utc::now().timestamp());
}

#[tokio::test]
async fn test_token_validation() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/tokens/verify"))
        .and(header("authorization", "Bearer test_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "valid": true,
            "claims": {
                "jti": "test_jti",
                "sub": "tenant1:user1",
                "exp": (chrono::Utc::now().timestamp() + 3600) as i64,
                "iat": chrono::Utc::now().timestamp() as i64,
                "scope": "credential:read",
                "tenant_id": "tenant1",
            }
        })))
        .mount(&mock_server)
        .await;

    let config = CredBridgeConfig::new(&mock_server.uri()).with_token("test_token");
    let client = std::sync::Arc::new(CredBridgeClient::new(config).unwrap());
    let token_manager = TokenManager::new(client);

    let is_valid = token_manager.verify(None).await;
    assert!(is_valid.is_ok());
    assert!(is_valid.unwrap());
}

#[tokio::test]
async fn test_create_credential() {
    let mock_server = MockServer::start().await;

    let expected_response = serde_json::json!({
        "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
        "service_id": "schwab",
        "credential_type": "username_password",
        "created_at": "1709990400",
        "expires_at": "1893456000",
    });

    Mock::given(method("POST"))
        .and(path("/api/v1/credentials"))
        .respond_with(ResponseTemplate::new(201).set_body_json(expected_response))
        .mount(&mock_server)
        .await;

    let config = create_test_config(&mock_server.uri());
    let client = std::sync::Arc::new(CredBridgeClient::new(config).unwrap());
    let credentials = CredentialsService::new(client);

    let mut plaintext_data = HashMap::new();
    plaintext_data.insert("username".to_string(), json!("user@example.com"));
    plaintext_data.insert("password".to_string(), json!("secret_password"));

    let result = credentials
        .create(
            "schwab",
            CredentialType::UsernamePassword,
            plaintext_data,
            Some(1893456000),
            None,
        )
        .await;

    assert!(result.is_ok());
    let credential = result.unwrap();
    assert_eq!(credential.credential_id, "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c");
    assert_eq!(credential.service_id, "schwab");
}

#[tokio::test]
async fn test_create_username_password_credential() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/credentials"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "credential_id": "test-cred-id",
            "service_id": "test-service",
            "credential_type": "username_password",
            "created_at": "1709990400",
        })))
        .mount(&mock_server)
        .await;

    let config = create_test_config(&mock_server.uri());
    let client = std::sync::Arc::new(CredBridgeClient::new(config).unwrap());
    let credentials = CredentialsService::new(client);

    let result = credentials
        .create_username_password("test-service", "testuser", "testpass", None, None)
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap().credential_id, "test-cred-id");
}

#[tokio::test]
async fn test_list_credentials() {
    let mock_server = MockServer::start().await;

    let expected_response = serde_json::json!({
        "credentials": [
            {
                "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
                "credential_type": "username_password",
                "user_id_hash": "abc123",
                "service_id": "schwab",
                "tenant_id": "tenant1",
                "created_at": "1709990400Z",
                "is_deleted": false,
            }
        ],
        "total": 1,
    });

    Mock::given(method("GET"))
        .and(path("/api/v1/credentials"))
        .respond_with(ResponseTemplate::new(200).set_body_json(expected_response))
        .mount(&mock_server)
        .await;

    let config = create_test_config(&mock_server.uri());
    let client = std::sync::Arc::new(CredBridgeClient::new(config).unwrap());
    let credentials = CredentialsService::new(client);

    let result = credentials.list(None, None).await;
    assert!(result.is_ok());

    let (cred_list, total) = result.unwrap();
    assert_eq!(total, 1);
    assert_eq!(cred_list.len(), 1);
    assert_eq!(cred_list[0].credential_id, "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c");
}

#[tokio::test]
async fn test_list_credentials_with_filter() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/credentials"))
        .and(query_param("service_id", "schwab"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "credentials": [],
            "total": 0,
        })))
        .mount(&mock_server)
        .await;

    let config = create_test_config(&mock_server.uri());
    let client = std::sync::Arc::new(CredBridgeClient::new(config).unwrap());
    let credentials = CredentialsService::new(client);

    let filter = credbridge_sdk::types::CredentialFilter {
        service_id: Some("schwab".to_string()),
        ..Default::default()
    };

    let result = credentials.list(Some(filter), None).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_get_credential() {
    let mock_server = MockServer::start().await;

    let credential_id = "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c";

    Mock::given(method("GET"))
        .and(path(format!("/api/v1/credentials/{}", credential_id)))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "credential_id": credential_id,
            "service_id": "schwab",
            "credential_type": "username_password",
            "created_at": "1709990400Z",
            "is_deleted": false,
        })))
        .mount(&mock_server)
        .await;

    let config = create_test_config(&mock_server.uri());
    let client = std::sync::Arc::new(CredBridgeClient::new(config).unwrap());
    let credentials = CredentialsService::new(client);

    let result = credentials.get(credential_id, None).await;
    assert!(result.is_ok());

    let credential = result.unwrap();
    assert_eq!(credential.credential_id, credential_id);
    assert_eq!(credential.service_id, "schwab");
}

#[tokio::test]
async fn test_get_credential_not_found() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/credentials/invalid-id"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "error": {
                    "code": "not_found",
                    "message": "Credential not found",
                },
                "meta": {
                    "request_id": "test-request-id",
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }
            })),
        )
        .mount(&mock_server)
        .await;

    let config = create_test_config(&mock_server.uri());
    let client = std::sync::Arc::new(CredBridgeClient::new(config).unwrap());
    let credentials = CredentialsService::new(client);

    let result = credentials.get("invalid-id", None).await;
    assert!(result.is_err());

    let error = result.unwrap_err();
    assert_eq!(error.code, CredBridgeErrorCode::NotFound);
}

#[tokio::test]
async fn test_decrypt_credential() {
    let mock_server = MockServer::start().await;

    let credential_id = "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c";

    Mock::given(method("POST"))
        .and(path(format!("/api/v1/credentials/{}/decrypt", credential_id)))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "credential_id": credential_id,
            "service_id": "schwab",
            "credential_type": "username_password",
            "plaintext_data": {
                "username": "user@example.com",
                "password": "secret_password",
            }
        })))
        .mount(&mock_server)
        .await;

    let config = create_test_config(&mock_server.uri());
    let client = std::sync::Arc::new(CredBridgeClient::new(config).unwrap());
    let credentials = CredentialsService::new(client);

    let result = credentials.decrypt(credential_id, Some("用户登录"), None).await;
    assert!(result.is_ok());

    let decrypted = result.unwrap();
    assert_eq!(decrypted.credential_id, credential_id);
    assert_eq!(
        decrypted.plaintext_data.get("username").unwrap(),
        &json!("user@example.com")
    );
    assert_eq!(
        decrypted.plaintext_data.get("password").unwrap(),
        &json!("secret_password")
    );
}

#[tokio::test]
async fn test_delete_credential() {
    let mock_server = MockServer::start().await;

    let credential_id = "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c";

    Mock::given(method("DELETE"))
        .and(path(format!("/api/v1/credentials/{}", credential_id)))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "credential_id": credential_id,
            "deleted": true,
        })))
        .mount(&mock_server)
        .await;

    let config = create_test_config(&mock_server.uri());
    let client = std::sync::Arc::new(CredBridgeClient::new(config).unwrap());
    let credentials = CredentialsService::new(client);

    let result = credentials.delete(credential_id, None).await;
    assert!(result.is_ok());

    let response = result.unwrap();
    assert!(response.deleted);
    assert_eq!(response.credential_id, credential_id);
}

#[tokio::test]
async fn test_credential_exists() {
    let mock_server = MockServer::start().await;

    let credential_id = "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c";

    // Mock for exists (will call get)
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/credentials/{}", credential_id)))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "credential_id": credential_id,
            "service_id": "schwab",
            "credential_type": "username_password",
            "created_at": "1709990400Z",
            "is_deleted": false,
        })))
        .mount(&mock_server)
        .await;

    let config = create_test_config(&mock_server.uri());
    let client = std::sync::Arc::new(CredBridgeClient::new(config).unwrap());
    let credentials = CredentialsService::new(client);

    let result = credentials.exists(credential_id, None).await;
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[tokio::test]
async fn test_credential_not_exists() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/credentials/non-existent-id"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "error": {
                    "code": "not_found",
                    "message": "Credential not found",
                },
                "meta": {
                    "request_id": "test-request-id",
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }
            })),
        )
        .mount(&mock_server)
        .await;

    let config = create_test_config(&mock_server.uri());
    let client = std::sync::Arc::new(CredBridgeClient::new(config).unwrap());
    let credentials = CredentialsService::new(client);

    let result = credentials.exists("non-existent-id", None).await;
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[tokio::test]
async fn test_token_manager_scopes() {
    let mock_server = MockServer::start().await;

    let config = create_test_config(&mock_server.uri());
    let client = std::sync::Arc::new(CredBridgeClient::new(config).unwrap());
    let token_manager = TokenManager::new(client);

    // Check scopes from the test token
    assert!(token_manager.has_scope(credbridge_sdk::types::TokenScope::CredentialRead));
    assert!(token_manager.has_scope(credbridge_sdk::types::TokenScope::CredentialWrite));
    assert!(token_manager.has_scope(credbridge_sdk::types::TokenScope::CredentialDecrypt));

    // Check has_any_scope
    assert!(token_manager.has_any_scope(&[
        credbridge_sdk::types::TokenScope::CredentialRead,
        credbridge_sdk::types::TokenScope::Admin,
    ]));

    // Check has_all_scopes
    assert!(token_manager.has_all_scopes(&[
        credbridge_sdk::types::TokenScope::CredentialRead,
        credbridge_sdk::types::TokenScope::CredentialWrite,
    ]));
}

#[tokio::test]
async fn test_token_manager_info() {
    let mock_server = MockServer::start().await;

    let config = create_test_config(&mock_server.uri());
    let client = std::sync::Arc::new(CredBridgeClient::new(config).unwrap());
    let token_manager = TokenManager::new(client);

    assert_eq!(token_manager.get_tenant_id(), Some("tenant1".to_string()));
    assert_eq!(token_manager.get_user_id(), Some("user1".to_string()));
    assert_eq!(token_manager.get_token_id(), Some("test_token_id".to_string()));
    assert!(token_manager.is_valid());
    // Token expires in 1 hour (3600 seconds)
    // Should NOT be expiring within 5 minutes (300 seconds)
    assert!(!token_manager.is_expiring_soon(300));
    // Should be expiring within 2 hours (7200 seconds) because the entire lifetime is only 1 hour
    assert!(token_manager.is_expiring_soon(7200));
}

#[tokio::test]
async fn test_sdk_creation() {
    let mock_server = MockServer::start().await;

    let config = create_test_config(&mock_server.uri());
    let sdk = CredBridgeSDK::new(config);
    assert!(sdk.is_ok());

    let sdk = sdk.unwrap();
    assert_eq!(credbridge_sdk::version(), CredBridgeSDK::version());
}

#[tokio::test]
async fn test_sdk_services() {
    let mock_server = MockServer::start().await;

    // Mock for credential listing
    Mock::given(method("GET"))
        .and(path("/api/v1/credentials"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "credentials": [],
            "total": 0,
        })))
        .mount(&mock_server)
        .await;

    let config = create_test_config(&mock_server.uri());
    let sdk = CredBridgeSDK::new(config).unwrap();

    // Test credentials service
    let (credentials, total) = sdk.credentials().list(None, None).await.unwrap();
    assert_eq!(total, 0);
    assert!(credentials.is_empty());

    // Test token manager
    assert!(sdk.token().is_valid());
}

#[tokio::test]
async fn test_request_options() {
    let options = RequestOptions::new()
        .with_timeout_ms(10000)
        .with_retries(5)
        .with_skip_retry(false)
        .with_request_id("custom-request-id")
        .with_header("X-Custom-Header", "custom-value");

    assert_eq!(options.timeout_ms, Some(10000));
    assert_eq!(options.retries, Some(5));
    assert!(!options.skip_retry);
    assert_eq!(options.request_id, Some("custom-request-id".to_string()));
    assert_eq!(
        options.headers.get("X-Custom-Header"),
        Some(&"custom-value".to_string())
    );
}

#[tokio::test]
async fn test_error_handling() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/credentials/error-test"))
        .respond_with(
            ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "error": {
                    "code": "internal_error",
                    "message": "Internal server error",
                    "details": {
                        "error_id": "err-123",
                    }
                },
                "meta": {
                    "request_id": "test-request-id",
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }
            })),
        )
        .mount(&mock_server)
        .await;

    let config = create_test_config(&mock_server.uri());
    let client = CredBridgeClient::new(config).unwrap();

    let result = client.get::<serde_json::Value>("/credentials/error-test").await;
    assert!(result.is_err());

    let error = result.unwrap_err();
    assert_eq!(error.code, CredBridgeErrorCode::InternalError);
    assert!(error.is_retryable());
    assert_eq!(error.status_code, Some(500));
}

#[tokio::test]
async fn test_network_error() {
    // Use an invalid URL to trigger network error
    let config = CredBridgeConfig::new("http://localhost:59999")
        .with_token("test_token")
        .with_timeout_ms(100)
        .with_max_retries(0);

    let client = CredBridgeClient::new(config).unwrap();
    let result = client.get::<serde_json::Value>("/credentials").await;

    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.is_network_error());
    assert!(error.is_retryable());
}

#[tokio::test]
async fn test_credential_type_display() {
    use std::fmt::Write;

    let mut output = String::new();
    write!(&mut output,
        "{}",
        CredentialType::UsernamePassword
    )
    .unwrap();
    assert_eq!(output, "username_password");

    output.clear();
    write!(&mut output, "{}", CredentialType::ApiKey).unwrap();
    assert_eq!(output, "api_key");
}

#[tokio::test]
async fn test_token_scope_display() {
    use std::fmt::Write;

    let mut output = String::new();
    write!(&mut output,
        "{}",
        credbridge_sdk::types::TokenScope::CredentialRead
    )
    .unwrap();
    assert_eq!(output, "credential:read");

    output.clear();
    write!(
        &mut output,
        "{}",
        credbridge_sdk::types::TokenScope::Admin
    )
    .unwrap();
    assert_eq!(output, "admin");
}
