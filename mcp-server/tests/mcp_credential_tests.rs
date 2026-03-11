//! MCP Credential Tools 集成测试
//!
//! 测试 EP6-Story6.2: Credential 工具
//! - create_credential
//! - update_credential
//! - delete_credential

use std::sync::Arc;
use serde_json::json;

use credbridge_mcp_server::McpServerState;
use credbridge_mcp_server::tools::{CredBridgeTools, ToolError, CreateCredentialResponse, UpdateCredentialResponse, DeleteCredentialResponse, CredentialType};

/// 创建测试用的 Server State
fn create_test_state() -> Arc<McpServerState> {
    Arc::new(McpServerState::new_in_memory().unwrap())
}

#[tokio::test]
async fn test_create_credential_success() {
    let state = create_test_state();
    let tools = CredBridgeTools::new(state);

    let plaintext_data = json!({
        "username": "test_user",
        "password": "test_password"
    });

    let result: Result<CreateCredentialResponse, ToolError> = tools.create_credential(
        "tenant_123",
        "user_456",
        "github",
        CredentialType::UsernamePassword,
        &plaintext_data,
        "credential:write",
        None,
    ).await;

    assert!(result.is_ok(), "Failed to create credential: {:?}", result.err());

    let response = result.unwrap();
    assert!(!response.credential_id.is_empty());
    assert_eq!(response.service_id, "github");
    assert_eq!(response.credential_type, "username_password");
}

#[tokio::test]
async fn test_create_credential_permission_denied() {
    let state = create_test_state();
    let tools = CredBridgeTools::new(state);

    let plaintext_data = json!({
        "api_key": "secret_key_123"
    });

    let result: Result<CreateCredentialResponse, ToolError> = tools.create_credential(
        "tenant_123",
        "user_456",
        "aws",
        CredentialType::ApiKey,
        &plaintext_data,
        "credential:read", // Wrong scope
        None,
    ).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        ToolError::PermissionDenied(msg) => {
            assert!(msg.contains("credential:write"));
        }
        other => panic!("Expected PermissionDenied error, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_create_and_delete_credential() {
    let state = create_test_state();
    let tools = CredBridgeTools::new(state);

    // Create credential
    let plaintext_data = json!({
        "username": "test_user",
        "password": "test_password"
    });

    let create_result: Result<CreateCredentialResponse, ToolError> = tools.create_credential(
        "tenant_123",
        "user_456",
        "github",
        CredentialType::UsernamePassword,
        &plaintext_data,
        "credential:write",
        None,
    ).await;

    assert!(create_result.is_ok());
    let credential_id = create_result.unwrap().credential_id;

    // Delete credential
    let delete_result: Result<DeleteCredentialResponse, ToolError> = tools.delete_credential(
        "tenant_123",
        "user_456",
        &credential_id,
        "credential:write",
    ).await;

    assert!(delete_result.is_ok());
    let response = delete_result.unwrap();
    assert!(response.deleted);
    assert!(!response.deleted_at.is_empty());
}

#[tokio::test]
async fn test_delete_credential_permission_denied() {
    let state = create_test_state();
    let tools = CredBridgeTools::new(state);

    // Create credential first
    let create_result: Result<CreateCredentialResponse, ToolError> = tools.create_credential(
        "tenant_123",
        "user_456",
        "aws",
        CredentialType::ApiKey,
        &json!({"api_key": "key123"}),
        "credential:write",
        None,
    ).await;
    assert!(create_result.is_ok());

    let credential_id = create_result.unwrap().credential_id;

    // Try to delete with wrong scope
    let result: Result<DeleteCredentialResponse, ToolError> = tools.delete_credential(
        "tenant_123",
        "user_456",
        &credential_id,
        "credential:read",
    ).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        ToolError::PermissionDenied(msg) => {
            assert!(msg.contains("credential:write") || msg.contains("credential:delete"));
        }
        other => panic!("Expected PermissionDenied error, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_credential_types_support() {
    let state = create_test_state();
    let tools = CredBridgeTools::new(state);

    // Test UsernamePassword
    let result: Result<CreateCredentialResponse, ToolError> = tools.create_credential(
        "tenant_123",
        "user_456",
        "github",
        CredentialType::UsernamePassword,
        &json!({"username": "user", "password": "pass"}),
        "credential:write",
        None,
    ).await;
    assert!(result.is_ok());

    // Test ApiKey
    let result: Result<CreateCredentialResponse, ToolError> = tools.create_credential(
        "tenant_123",
        "user_456",
        "aws",
        CredentialType::ApiKey,
        &json!({"api_key": "key123", "secret": "secret456"}),
        "credential:write",
        None,
    ).await;
    assert!(result.is_ok());

    // Test OAuthRefresh
    let result: Result<CreateCredentialResponse, ToolError> = tools.create_credential(
        "tenant_123",
        "user_456",
        "google",
        CredentialType::OAuthRefresh,
        &json!({"refresh_token": "token123"}),
        "credential:write",
        None,
    ).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_full_credential_lifecycle() {
    let state = create_test_state();
    let tools = CredBridgeTools::new(state);

    // 1. Create
    let plaintext_data = json!({
        "username": "lifecycle_user",
        "password": "lifecycle_pass"
    });

    let create_result: Result<CreateCredentialResponse, ToolError> = tools.create_credential(
        "tenant_123",
        "user_456",
        "github",
        CredentialType::UsernamePassword,
        &plaintext_data,
        "credential:write",
        None,
    ).await;
    assert!(create_result.is_ok());
    let credential_id = create_result.unwrap().credential_id;

    // 2. Update
    let new_plaintext_data = json!({
        "username": "updated_user",
        "password": "updated_pass"
    });

    let update_result: Result<UpdateCredentialResponse, ToolError> = tools.update_credential(
        "tenant_123",
        "user_456",
        &credential_id,
        Some(&new_plaintext_data),
        "credential:write",
        None,
    ).await;
    assert!(update_result.is_ok());
    let update_response = update_result.unwrap();
    assert_eq!(update_response.credential_id, credential_id);
    assert!(!update_response.updated_at.is_empty());

    // 3. Delete
    let delete_result: Result<DeleteCredentialResponse, ToolError> = tools.delete_credential(
        "tenant_123",
        "user_456",
        &credential_id,
        "credential:write",
    ).await;
    assert!(delete_result.is_ok());
    assert!(delete_result.unwrap().deleted);
}

#[tokio::test]
async fn test_admin_scope_bypass() {
    let state = create_test_state();
    let tools = CredBridgeTools::new(state);

    // Create with credential:write
    let create_result: Result<CreateCredentialResponse, ToolError> = tools.create_credential(
        "tenant_123",
        "user_456",
        "github",
        CredentialType::UsernamePassword,
        &json!({"username": "user", "password": "pass"}),
        "credential:write",
        None,
    ).await;
    assert!(create_result.is_ok());

    let credential_id = create_result.unwrap().credential_id;

    // Update with admin scope (should work)
    let update_result: Result<UpdateCredentialResponse, ToolError> = tools.update_credential(
        "tenant_123",
        "user_456",
        &credential_id,
        Some(&json!({"username": "updated", "password": "updated"})),
        "admin",
        None,
    ).await;
    assert!(update_result.is_ok());

    // Delete with admin scope (should work)
    let delete_result: Result<DeleteCredentialResponse, ToolError> = tools.delete_credential(
        "tenant_123",
        "user_456",
        &credential_id,
        "admin",
    ).await;
    assert!(delete_result.is_ok());
}

#[tokio::test]
async fn test_create_credential_with_expiration() {
    let state = create_test_state();
    let tools = CredBridgeTools::new(state);

    let plaintext_data = json!({
        "refresh_token": "oauth_refresh_token_123"
    });

    let expires_at = Some(chrono::Utc::now().timestamp() as u64 + 3600); // 1 hour from now

    let result: Result<CreateCredentialResponse, ToolError> = tools.create_credential(
        "tenant_123",
        "user_456",
        "google",
        CredentialType::OAuthRefresh,
        &plaintext_data,
        "credential:write",
        expires_at,
    ).await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.expires_at.is_some());
}
