//! CredBridge MCP 工具处理器
//!
//! 实现 MCP 协议的工具调用处理器，将 MCP 请求转换为 CredBridge 操作。

use std::sync::Arc;

use rmcp::{
    model::*,
    handler::server::ServerHandler,
    service::{RequestContext, RoleServer},
    ErrorData as McpError,
};
use std::borrow::Cow;
use serde_json::json;
use tracing::{info, debug, error, warn};

use crate::tools::{
    CredBridgeTools, ToolError,
    tool_names,
};
use crate::tools::CredentialType;

/// MCP 工具处理器
#[derive(Clone)]
pub struct ToolHandler {
    tools: CredBridgeTools,
}

impl ToolHandler {
    /// 创建新的工具处理器
    pub fn new(tools: CredBridgeTools) -> Self {
        Self { tools }
    }

    /// 获取工具列表
    fn get_tool_definitions() -> Vec<Tool> {
        use serde_json::Map;

        // list_credentials 工具的 schema
        let list_creds_schema: Arc<Map<String, serde_json::Value>> = Arc::new(json!({
            "type": "object",
            "required": ["tenant_id", "user_id"],
            "properties": {
                "tenant_id": {
                    "type": "string",
                    "description": "Tenant ID"
                },
                "user_id": {
                    "type": "string",
                    "description": "User ID"
                },
                "service_id": {
                    "type": "string",
                    "description": "Optional service filter"
                },
                "credential_type": {
                    "type": "string",
                    "description": "Optional credential type filter"
                }
            }
        }).as_object().unwrap().clone());

        // get_credential 工具的 schema
        let get_cred_schema: Arc<Map<String, serde_json::Value>> = Arc::new(json!({
            "type": "object",
            "required": ["tenant_id", "user_id", "credential_id"],
            "properties": {
                "tenant_id": {
                    "type": "string",
                    "description": "Tenant ID"
                },
                "user_id": {
                    "type": "string",
                    "description": "User ID"
                },
                "credential_id": {
                    "type": "string",
                    "description": "Credential ID to retrieve"
                }
            }
        }).as_object().unwrap().clone());

        // decrypt_credential 工具的 schema
        let decrypt_schema: Arc<Map<String, serde_json::Value>> = Arc::new(json!({
            "type": "object",
            "required": ["tenant_id", "user_id", "credential_id", "scope"],
            "properties": {
                "tenant_id": {
                    "type": "string",
                    "description": "Tenant ID"
                },
                "user_id": {
                    "type": "string",
                    "description": "User ID"
                },
                "credential_id": {
                    "type": "string",
                    "description": "Credential ID to decrypt"
                },
                "scope": {
                    "type": "string",
                    "description": "Authorization scope for permission checking"
                }
            }
        }).as_object().unwrap().clone());

        // create_credential 工具的 schema
        let create_schema: Arc<Map<String, serde_json::Value>> = Arc::new(json!({
            "type": "object",
            "required": ["tenant_id", "user_id", "service_id", "credential_type", "plaintext_data", "scope"],
            "properties": {
                "tenant_id": {
                    "type": "string",
                    "description": "Tenant ID"
                },
                "user_id": {
                    "type": "string",
                    "description": "User ID"
                },
                "service_id": {
                    "type": "string",
                    "description": "Service identifier (e.g., 'github', 'aws', 'schwab')"
                },
                "credential_type": {
                    "type": "string",
                    "description": "Credential type: username_password, api_key, oauth_refresh, session_cookie, kyc_document",
                    "enum": ["username_password", "api_key", "oauth_refresh", "session_cookie", "kyc_document"]
                },
                "plaintext_data": {
                    "type": "object",
                    "description": "Credential plaintext data to be encrypted (e.g., {\"username\": \"user\", \"password\": \"pass\"})"
                },
                "scope": {
                    "type": "string",
                    "description": "Authorization scope for permission checking (requires 'credential:write')"
                },
                "expires_at": {
                    "type": "integer",
                    "description": "Optional expiration timestamp (Unix epoch seconds)"
                }
            }
        }).as_object().unwrap().clone());

        // update_credential 工具的 schema
        let update_schema: Arc<Map<String, serde_json::Value>> = Arc::new(json!({
            "type": "object",
            "required": ["tenant_id", "user_id", "credential_id", "scope"],
            "properties": {
                "tenant_id": {
                    "type": "string",
                    "description": "Tenant ID"
                },
                "user_id": {
                    "type": "string",
                    "description": "User ID"
                },
                "credential_id": {
                    "type": "string",
                    "description": "Credential ID to update"
                },
                "plaintext_data": {
                    "type": "object",
                    "description": "Optional new credential plaintext data (if not provided, existing data is kept)"
                },
                "scope": {
                    "type": "string",
                    "description": "Authorization scope for permission checking (requires 'credential:write')"
                },
                "expires_at": {
                    "type": ["integer", "null"],
                    "description": "Optional new expiration timestamp (Unix epoch seconds). Use null to clear expiration."
                }
            }
        }).as_object().unwrap().clone());

        // delete_credential 工具的 schema
        let delete_schema: Arc<Map<String, serde_json::Value>> = Arc::new(json!({
            "type": "object",
            "required": ["tenant_id", "user_id", "credential_id", "scope"],
            "properties": {
                "tenant_id": {
                    "type": "string",
                    "description": "Tenant ID"
                },
                "user_id": {
                    "type": "string",
                    "description": "User ID"
                },
                "credential_id": {
                    "type": "string",
                    "description": "Credential ID to delete"
                },
                "scope": {
                    "type": "string",
                    "description": "Authorization scope for permission checking (requires 'credential:write' or 'credential:delete')"
                }
            }
        }).as_object().unwrap().clone());

        // tee_status 工具的 schema
        let tee_status_schema: Arc<Map<String, serde_json::Value>> = Arc::new(json!({
            "type": "object",
            "properties": {}
        }).as_object().unwrap().clone());

        vec![
            Tool::new(
                tool_names::LIST_CREDENTIALS,
                "List user credentials metadata (sensitive data redacted). Returns a list of credentials with their IDs, service types, and creation dates.",
                list_creds_schema,
            ),
            Tool::new(
                tool_names::GET_CREDENTIAL,
                "Get credential metadata by ID. Returns non-sensitive information about a specific credential including service type, scopes, and timestamps.",
                get_cred_schema,
            ),
            Tool::new(
                tool_names::CREATE_CREDENTIAL,
                "Create a new credential with encrypted storage. Supports username_password, api_key, oauth_refresh types. Requires 'credential:write' scope. All operations are audited.",
                create_schema,
            ),
            Tool::new(
                tool_names::UPDATE_CREDENTIAL,
                "Update an existing credential. Can update the encrypted data and/or expiration time. Requires 'credential:write' scope. All operations are audited.",
                update_schema,
            ),
            Tool::new(
                tool_names::DELETE_CREDENTIAL,
                "Delete (soft-delete) a credential. The credential is marked as deleted but retained for audit purposes. Requires 'credential:write' or 'credential:delete' scope. All operations are audited.",
                delete_schema,
            ),
            Tool::new(
                tool_names::DECRYPT_CREDENTIAL,
                "Decrypt credential content inside TEE enclave. Requires 'credential:decrypt' scope. This is a high-risk operation that will be audited.",
                decrypt_schema,
            ),
            Tool::new(
                tool_names::TEE_STATUS,
                "Get TEE (Trusted Execution Environment) status and attestation information for security verification.",
                tee_status_schema,
            ),
        ]
    }
}

impl ServerHandler for ToolHandler {
    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        std::future::ready(Ok(ListToolsResult {
            tools: Self::get_tool_definitions(),
            next_cursor: None,
        }))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        let tool_name = request.name.to_string();
        let arguments = request.arguments.unwrap_or_default();

        debug!("Tool called: {}", tool_name);

        async move {
            match tool_name.as_str() {
                tool_names::LIST_CREDENTIALS => {
                    handle_list_credentials(&self.tools, arguments).await
                }
                tool_names::GET_CREDENTIAL => {
                    handle_get_credential(&self.tools, arguments).await
                }
                tool_names::CREATE_CREDENTIAL => {
                    handle_create_credential(&self.tools, arguments).await
                }
                tool_names::UPDATE_CREDENTIAL => {
                    handle_update_credential(&self.tools, arguments).await
                }
                tool_names::DELETE_CREDENTIAL => {
                    handle_delete_credential(&self.tools, arguments).await
                }
                tool_names::DECRYPT_CREDENTIAL => {
                    handle_decrypt_credential(&self.tools, arguments).await
                }
                tool_names::TEE_STATUS => {
                    handle_tee_status(&self.tools).await
                }
                _ => {
                    warn!("Unknown tool called: {}", tool_name);
                    Err(McpError::invalid_request(
                        format!("Unknown tool: {}", tool_name),
                        None
                    ))
                }
            }
        }
    }

    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability::default()),
                ..Default::default()
            },
            server_info: Implementation {
                name: "credbridge-mcp-server".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            instructions: Some(
                "CredBridge MCP Server - Secure credential management with TEE support. \
                Use list_credentials to browse available credentials, get_credential for metadata, \
                and decrypt_credential (requires special scope) to access sensitive data."
                .to_string()
            ),
        }
    }
}

async fn handle_list_credentials(
    tools: &CredBridgeTools,
    arguments: serde_json::Map<String, serde_json::Value>,
) -> Result<CallToolResult, McpError> {
    let tenant_id = arguments
        .get("tenant_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing required parameter: tenant_id", None))?;

    let user_id = arguments
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing required parameter: user_id", None))?;

    debug!("list_credentials called for tenant: {}, user: {}", tenant_id, user_id);

    match tools.list_credentials(tenant_id, user_id).await {
        Ok(response) => {
            let content = json!({
                "credentials": response.credentials,
                "total": response.total
            });

            Ok(CallToolResult::success(vec![
                Content::text(content.to_string())
            ]))
        }
        Err(ToolError::NotFound(msg)) => {
            Ok(CallToolResult::success(vec![
                Content::text(json!({"error": msg, "credentials": [], "total": 0}).to_string())
            ]))
        }
        Err(e) => {
            error!("list_credentials failed: {}", e);
            Err(McpError::internal_error(
                format!("Failed to list credentials: {}", e),
                None
            ))
        }
    }
}

async fn handle_get_credential(
    tools: &CredBridgeTools,
    arguments: serde_json::Map<String, serde_json::Value>,
) -> Result<CallToolResult, McpError> {
    let tenant_id = arguments
        .get("tenant_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing required parameter: tenant_id", None))?;

    let user_id = arguments
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing required parameter: user_id", None))?;

    let credential_id = arguments
        .get("credential_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing required parameter: credential_id", None))?;

    debug!("get_credential called: {} for tenant: {}", credential_id, tenant_id);

    match tools.get_credential(tenant_id, user_id, credential_id).await {
        Ok(response) => {
            let content = json!({
                "id": response.id,
                "service_id": response.service_id,
                "credential_type": response.credential_type,
                "created_at": response.created_at,
                "updated_at": response.updated_at,
                "expires_at": response.expires_at,
                "granted_scopes": response.granted_scopes,
                "mfa_hint": response.mfa_hint,
            });

            Ok(CallToolResult::success(vec![
                Content::text(content.to_string())
            ]))
        }
        Err(ToolError::NotFound(msg)) => {
            Ok(CallToolResult::success(vec![
                Content::text(json!({"error": msg}).to_string())
            ]))
        }
        Err(e) => {
            error!("get_credential failed: {}", e);
            Err(McpError::internal_error(
                format!("Failed to get credential: {}", e),
                None
            ))
        }
    }
}

async fn handle_decrypt_credential(
    tools: &CredBridgeTools,
    arguments: serde_json::Map<String, serde_json::Value>,
) -> Result<CallToolResult, McpError> {
    let tenant_id = arguments
        .get("tenant_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing required parameter: tenant_id", None))?;

    let user_id = arguments
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing required parameter: user_id", None))?;

    let credential_id = arguments
        .get("credential_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing required parameter: credential_id", None))?;

    let scope = arguments
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    info!("decrypt_credential called: {} for tenant: {} (scope: {})",
          credential_id, tenant_id, scope);

    match tools.decrypt_credential(tenant_id, user_id, credential_id, scope).await {
        Ok(response) => {
            let content = json!({
                "credential_id": response.credential_id,
                "decrypted_data": response.decrypted_data,
                "decrypted_at": response.decrypted_at,
                "tee_verified": response.tee_verified,
                "warning": "This data is sensitive and should be handled securely."
            });

            Ok(CallToolResult::success(vec![
                Content::text(content.to_string())
            ]))
        }
        Err(ToolError::PermissionDenied(msg)) => {
            warn!("Permission denied for decrypt_credential: {}", msg);
            Ok(CallToolResult::success(vec![
                Content::text(json!({
                    "error": "Permission denied",
                    "message": msg,
                    "required_scope": "credential:decrypt"
                }).to_string())
            ]))
        }
        Err(e) => {
            error!("decrypt_credential failed: {}", e);
            Err(McpError::internal_error(
                format!("Failed to decrypt credential: {}", e),
                None
            ))
        }
    }
}

async fn handle_tee_status(
    tools: &CredBridgeTools,
) -> Result<CallToolResult, McpError> {
    debug!("tee_status called");

    match tools.get_tee_status().await {
        Ok(response) => {
            let content = json!({
                "enabled": response.enabled,
                "mrenclave": response.mrenclave,
                "mrsigner": response.mrsigner,
                "isv_svn": response.isv_svn,
                "quote_valid": response.quote_valid,
            });

            Ok(CallToolResult::success(vec![
                Content::text(content.to_string())
            ]))
        }
        Err(e) => {
            error!("tee_status failed: {}", e);
            Err(McpError::internal_error(
                format!("Failed to get TEE status: {}", e),
                None
            ))
        }
    }
}

async fn handle_create_credential(
    tools: &CredBridgeTools,
    arguments: serde_json::Map<String, serde_json::Value>,
) -> Result<CallToolResult, McpError> {
    let tenant_id = arguments
        .get("tenant_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing required parameter: tenant_id", None))?;

    let user_id = arguments
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing required parameter: user_id", None))?;

    let service_id = arguments
        .get("service_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing required parameter: service_id", None))?;

    let credential_type_str = arguments
        .get("credential_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing required parameter: credential_type", None))?;

    let plaintext_data = arguments
        .get("plaintext_data")
        .cloned()
        .ok_or_else(|| McpError::invalid_params("Missing required parameter: plaintext_data", None))?;

    let scope = arguments
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let expires_at = arguments
        .get("expires_at")
        .and_then(|v| v.as_u64());

    // 解析凭证类型
    let credential_type = parse_credential_type(credential_type_str)?;

    info!("create_credential called for tenant: {}, user: {}, service: {}, type: {}",
          tenant_id, user_id, service_id, credential_type_str);

    match tools.create_credential(
        tenant_id,
        user_id,
        service_id,
        credential_type,
        &plaintext_data,
        scope,
        expires_at,
    ).await {
        Ok(response) => {
            let content = json!({
                "credential_id": response.credential_id,
                "service_id": response.service_id,
                "credential_type": response.credential_type,
                "created_at": response.created_at,
                "expires_at": response.expires_at,
                "message": "Credential created successfully"
            });

            Ok(CallToolResult::success(vec![
                Content::text(content.to_string())
            ]))
        }
        Err(ToolError::PermissionDenied(msg)) => {
            warn!("Permission denied for create_credential: {}", msg);
            Ok(CallToolResult::success(vec![
                Content::text(json!({
                    "error": "Permission denied",
                    "message": msg,
                    "required_scope": "credential:write"
                }).to_string())
            ]))
        }
        Err(e) => {
            error!("create_credential failed: {}", e);
            Err(McpError::internal_error(
                format!("Failed to create credential: {}", e),
                None
            ))
        }
    }
}

async fn handle_update_credential(
    tools: &CredBridgeTools,
    arguments: serde_json::Map<String, serde_json::Value>,
) -> Result<CallToolResult, McpError> {
    let tenant_id = arguments
        .get("tenant_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing required parameter: tenant_id", None))?;

    let user_id = arguments
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing required parameter: user_id", None))?;

    let credential_id = arguments
        .get("credential_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing required parameter: credential_id", None))?;

    let plaintext_data = arguments
        .get("plaintext_data")
        .cloned();

    let scope = arguments
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let expires_at: Option<Option<u64>> = arguments
        .get("expires_at")
        .map(|v| {
            if v.is_null() {
                None  // 明确设置为 null 表示清除过期时间
            } else {
                v.as_u64()  // 有值则设置为该值
            }
        });

    info!("update_credential called: {} for tenant: {}", credential_id, tenant_id);

    match tools.update_credential(
        tenant_id,
        user_id,
        credential_id,
        plaintext_data.as_ref(),
        scope,
        expires_at,
    ).await {
        Ok(response) => {
            let content = json!({
                "credential_id": response.credential_id,
                "service_id": response.service_id,
                "credential_type": response.credential_type,
                "updated_at": response.updated_at,
                "expires_at": response.expires_at,
                "message": "Credential updated successfully"
            });

            Ok(CallToolResult::success(vec![
                Content::text(content.to_string())
            ]))
        }
        Err(ToolError::NotFound(msg)) => {
            Ok(CallToolResult::success(vec![
                Content::text(json!({
                    "error": "Not found",
                    "message": msg
                }).to_string())
            ]))
        }
        Err(ToolError::PermissionDenied(msg)) => {
            warn!("Permission denied for update_credential: {}", msg);
            Ok(CallToolResult::success(vec![
                Content::text(json!({
                    "error": "Permission denied",
                    "message": msg,
                    "required_scope": "credential:write"
                }).to_string())
            ]))
        }
        Err(e) => {
            error!("update_credential failed: {}", e);
            Err(McpError::internal_error(
                format!("Failed to update credential: {}", e),
                None
            ))
        }
    }
}

async fn handle_delete_credential(
    tools: &CredBridgeTools,
    arguments: serde_json::Map<String, serde_json::Value>,
) -> Result<CallToolResult, McpError> {
    let tenant_id = arguments
        .get("tenant_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing required parameter: tenant_id", None))?;

    let user_id = arguments
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing required parameter: user_id", None))?;

    let credential_id = arguments
        .get("credential_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::invalid_params("Missing required parameter: credential_id", None))?;

    let scope = arguments
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    info!("delete_credential called: {} for tenant: {}", credential_id, tenant_id);

    match tools.delete_credential(tenant_id, user_id, credential_id, scope).await {
        Ok(response) => {
            let content = json!({
                "credential_id": response.credential_id,
                "deleted": response.deleted,
                "deleted_at": response.deleted_at,
                "message": "Credential deleted successfully (soft delete)"
            });

            Ok(CallToolResult::success(vec![
                Content::text(content.to_string())
            ]))
        }
        Err(ToolError::NotFound(msg)) => {
            Ok(CallToolResult::success(vec![
                Content::text(json!({
                    "error": "Not found",
                    "message": msg
                }).to_string())
            ]))
        }
        Err(ToolError::PermissionDenied(msg)) => {
            warn!("Permission denied for delete_credential: {}", msg);
            Ok(CallToolResult::success(vec![
                Content::text(json!({
                    "error": "Permission denied",
                    "message": msg,
                    "required_scope": "credential:write or credential:delete"
                }).to_string())
            ]))
        }
        Err(e) => {
            error!("delete_credential failed: {}", e);
            Err(McpError::internal_error(
                format!("Failed to delete credential: {}", e),
                None
            ))
        }
    }
}

/// 解析凭证类型字符串
fn parse_credential_type(type_str: &str) -> Result<CredentialType, McpError> {
    match type_str.to_lowercase().as_str() {
        "username_password" => Ok(CredentialType::UsernamePassword),
        "api_key" => Ok(CredentialType::ApiKey),
        "oauth_refresh" => Ok(CredentialType::OAuthRefresh),
        "session_cookie" => Ok(CredentialType::SessionCookie),
        "kyc_document" => Ok(CredentialType::KycDocument),
        _ => Err(McpError::invalid_params(
            format!("Invalid credential_type: {}. Must be one of: username_password, api_key, oauth_refresh, session_cookie, kyc_document", type_str),
            None
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::McpServerState;

    fn create_test_handler() -> ToolHandler {
        let state = Arc::new(McpServerState::new_in_memory().unwrap());
        let tools = CredBridgeTools::new(state);
        ToolHandler::new(tools)
    }

    #[test]
    fn test_tool_definitions() {
        let tools = ToolHandler::get_tool_definitions();
        assert_eq!(tools.len(), 7);

        let tool_names: Vec<&str> = tools.iter()
            .map(|t| t.name.as_ref())
            .collect();

        assert!(tool_names.contains(&"list_credentials"));
        assert!(tool_names.contains(&"get_credential"));
        assert!(tool_names.contains(&"create_credential"));
        assert!(tool_names.contains(&"update_credential"));
        assert!(tool_names.contains(&"delete_credential"));
        assert!(tool_names.contains(&"decrypt_credential"));
        assert!(tool_names.contains(&"tee_status"));
    }

    #[test]
    fn test_server_info() {
        let handler = create_test_handler();
        let info = handler.get_info();

        assert_eq!(info.protocol_version, ProtocolVersion::V_2024_11_05);
        assert_eq!(info.server_info.name, "credbridge-mcp-server");
        assert!(info.capabilities.tools.is_some());
    }

    #[test]
    fn test_parse_credential_type() {
        assert!(matches!(parse_credential_type("username_password").unwrap(), CredentialType::UsernamePassword));
        assert!(matches!(parse_credential_type("api_key").unwrap(), CredentialType::ApiKey));
        assert!(matches!(parse_credential_type("oauth_refresh").unwrap(), CredentialType::OAuthRefresh));
        assert!(matches!(parse_credential_type("session_cookie").unwrap(), CredentialType::SessionCookie));
        assert!(matches!(parse_credential_type("kyc_document").unwrap(), CredentialType::KycDocument));

        // Test case insensitivity
        assert!(matches!(parse_credential_type("USERNAME_PASSWORD").unwrap(), CredentialType::UsernamePassword));
        assert!(matches!(parse_credential_type("Api_Key").unwrap(), CredentialType::ApiKey));

        // Test invalid type
        assert!(parse_credential_type("invalid_type").is_err());
    }
}
