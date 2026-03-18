//! 凭证 CRUD API
//!
//! 实现 EP2-Story2.2 凭证管理 API
//! - POST /api/v1/credentials - 创建凭证
//! - GET /api/v1/credentials - 获取凭证列表
//! - GET /api/v1/credentials/:id - 获取凭证详情
//! - POST /api/v1/credentials/:id/decrypt - 解密凭证
//! - DELETE /api/v1/credentials/:id - 删除凭证

use crate::api::middleware::{TokenScope, ValidatedToken, require_any_scope, require_scope};
use crate::crypto::cipher::{EncryptedBlob, decrypt_credential, encrypt_credential};
use crate::crypto::hkdf::KeyHierarchy;
use crate::crypto::keys::KeyPurpose;
use crate::models::{CredentialMetadata, CredentialType};
use crate::vault::models::{
    CreateCredentialRequest, CredentialFilter, CredentialId, EncryptedPayload, ServiceId, TenantId,
    UserId, VaultEntry,
};
use crate::vault::storage::CredentialVault;
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 应用状态
#[derive(Clone)]
pub struct AppState {
    /// 凭证 Vault
    pub vault: Arc<CredentialVault>,
    /// 密钥层次结构
    pub key_hierarchy: Arc<RwLock<KeyHierarchy>>,
    /// 审计日志记录器
    pub audit_logger: Arc<dyn AuditLogger>,
}

/// 审计日志记录器 trait
pub trait AuditLogger: Send + Sync {
    fn log_credential_created(&self, tenant_id: &str, user_id: &str, credential_id: &str);
    fn log_credential_accessed(&self, tenant_id: &str, user_id: &str, credential_id: &str);
    fn log_credential_deleted(&self, tenant_id: &str, user_id: &str, credential_id: &str);
    fn log_decryption_attempt(
        &self,
        tenant_id: &str,
        user_id: &str,
        credential_id: &str,
        success: bool,
    );
}

/// 默认审计日志记录器（打印到控制台）
pub struct DefaultAuditLogger;

impl AuditLogger for DefaultAuditLogger {
    fn log_credential_created(&self, tenant_id: &str, user_id: &str, credential_id: &str) {
        log::info!(
            "[AUDIT] Credential created - tenant: {}, user: {}, credential: {}",
            tenant_id,
            user_id,
            credential_id
        );
    }

    fn log_credential_accessed(&self, tenant_id: &str, user_id: &str, credential_id: &str) {
        log::info!(
            "[AUDIT] Credential accessed - tenant: {}, user: {}, credential: {}",
            tenant_id,
            user_id,
            credential_id
        );
    }

    fn log_credential_deleted(&self, tenant_id: &str, user_id: &str, credential_id: &str) {
        log::info!(
            "[AUDIT] Credential deleted - tenant: {}, user: {}, credential: {}",
            tenant_id,
            user_id,
            credential_id
        );
    }

    fn log_decryption_attempt(
        &self,
        tenant_id: &str,
        user_id: &str,
        credential_id: &str,
        success: bool,
    ) {
        log::info!(
            "[AUDIT] Decryption attempt - tenant: {}, user: {}, credential: {}, success: {}",
            tenant_id,
            user_id,
            credential_id,
            success
        );
    }
}

/// API 错误响应
#[derive(Debug, Clone, Serialize)]
pub struct ApiError {
    pub error: String,
    pub message: String,
}

impl ApiError {
    pub fn new(error: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.error.as_str() {
            "not_found" => StatusCode::NOT_FOUND,
            "invalid_request" => StatusCode::BAD_REQUEST,
            "unauthorized" => StatusCode::UNAUTHORIZED,
            "forbidden" => StatusCode::FORBIDDEN,
            "internal_error" => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        (status, Json(json!(self))).into_response()
    }
}

/// 创建凭证请求
#[derive(Debug, Deserialize)]
pub struct CreateCredentialApiRequest {
    /// 服务 ID
    pub service_id: String,
    /// 凭证类型
    pub credential_type: CredentialType,
    /// 明文凭证内容（将被加密）
    pub plaintext_data: serde_json::Value,
    /// 过期时间（Unix 时间戳，可选）
    pub expires_at: Option<u64>,
}

/// 创建凭证响应
#[derive(Debug, Serialize)]
pub struct CreateCredentialResponse {
    pub credential_id: String,
    pub service_id: String,
    pub credential_type: String,
    pub created_at: String,
    pub expires_at: Option<String>,
}

/// POST /api/v1/credentials - 创建凭证
pub async fn create_credential(
    State(state): State<AppState>,
    Extension(token): Extension<ValidatedToken>,
    Json(request): Json<CreateCredentialApiRequest>,
) -> Result<(StatusCode, Json<CreateCredentialResponse>), ApiError> {
    // 验证 Scope: credential:write
    require_scope(TokenScope::CredentialWrite)(&token)
        .map_err(|e| ApiError::new("forbidden", e.message))?;

    // 先创建 UserId 对象，用于加密和存储
    let user_id = UserId::new(&token.user_id);
    let tenant_id = TenantId::new(&token.tenant_id);

    // 先生成 credential_id，确保加密时使用的 ID 与存储时一致
    let credential_id = CredentialId::new();

    // 加密凭证内容（在 TEE 内完成）
    let encrypted_payload = encrypt_credential_in_tee(
        &state,
        tenant_id.as_str(),
        &user_id,
        &credential_id,
        &request.plaintext_data,
    )
    .await
    .map_err(|e| ApiError::new("internal_error", e))?;

    // 创建凭证请求
    let create_request = CreateCredentialRequest {
        tenant_id,
        user_id,
        service_id: ServiceId::new(&request.service_id),
        credential_type: request.credential_type,
        expires_at: request.expires_at,
    };

    // 存储凭证（使用预生成的 credential_id）
    let entry = state
        .vault
        .create_credential_with_id(create_request, encrypted_payload, credential_id)
        .map_err(|e| ApiError::new("internal_error", e.to_string()))?;

    // 记录审计日志
    state.audit_logger.log_credential_created(
        &token.tenant_id,
        &token.user_id,
        entry.credential_id.as_str(),
    );

    let response = CreateCredentialResponse {
        credential_id: entry.credential_id.as_str().to_string(),
        service_id: entry.service_id.as_str().to_string(),
        credential_type: entry.credential_type.as_str().to_string(),
        created_at: entry.created_at.to_string(),
        expires_at: entry.expires_at.map(|t| t.to_string()),
    };

    Ok((StatusCode::CREATED, Json(response)))
}

/// 在 TEE 内加密凭证
///
/// 修复说明：
/// - 接收预生成的 credential_id，确保加密和存储使用相同的 ID
/// - 使用 user_id.hash() 派生 L2 密钥，与解密流程一致
/// - 使用 user_id.hash() 构建 AAD，与解密流程一致
async fn encrypt_credential_in_tee(
    state: &AppState,
    tenant_id: &str,
    user_id: &UserId,
    credential_id: &CredentialId,
    plaintext: &serde_json::Value,
) -> Result<EncryptedPayload, String> {
    // 序列化明文
    let plaintext_bytes =
        serde_json::to_vec(plaintext).map_err(|e| format!("明文序列化失败: {}", e))?;

    // 派生 L3 密钥
    // 使用 user_id.hash() 保持与解密流程一致
    let l3_key = {
        let hierarchy = state.key_hierarchy.write().await;
        let l2_key = hierarchy
            .derive_user_vault_key(tenant_id, user_id.hash())
            .map_err(|e| format!("L2 密钥派生失败: {}", e))?;

        // 使用预生成的 credential_id 派生密钥，确保与存储的 ID 一致
        hierarchy
            .derive_credential_key(
                &l2_key,
                credential_id.as_str(),
                KeyPurpose::CredentialEncryption,
            )
            .map_err(|e| format!("L3 密钥派生失败: {}", e))?
    };

    // 执行加密
    // 使用 user_id.hash() 构建 AAD，与解密流程一致
    let aad = format!("{}:{}", tenant_id, user_id.hash());
    let blob = encrypt_credential(&l3_key, &plaintext_bytes, Some(aad.as_bytes()))
        .map_err(|e| format!("加密失败: {}", e))?;

    Ok(EncryptedPayload::from_blob(&blob))
}

/// 凭证列表响应
#[derive(Debug, Serialize)]
pub struct ListCredentialsResponse {
    pub credentials: Vec<CredentialMetadata>,
    pub total: usize,
}

/// GET /api/v1/credentials - 获取凭证列表
pub async fn list_credentials(
    State(state): State<AppState>,
    Extension(token): Extension<ValidatedToken>,
) -> Result<Json<ListCredentialsResponse>, ApiError> {
    // 验证 Scope: credential:read
    require_scope(TokenScope::CredentialRead)(&token)
        .map_err(|e| ApiError::new("forbidden", e.message))?;

    let tenant_id = TenantId::new(&token.tenant_id);
    let user_id = UserId::new(&token.user_id);

    let filter = CredentialFilter {
        include_deleted: false,
        only_valid: true,
        ..Default::default()
    };

    let result = state
        .vault
        .list_credentials(&tenant_id, &user_id, filter)
        .map_err(|e| ApiError::new("internal_error", e.to_string()))?;

    Ok(Json(ListCredentialsResponse {
        credentials: result.credentials,
        total: result.total,
    }))
}

/// 凭证详情响应
#[derive(Debug, Serialize)]
pub struct GetCredentialResponse {
    pub credential_id: String,
    pub service_id: String,
    pub credential_type: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub is_deleted: bool,
}

/// GET /api/v1/credentials/:id - 获取凭证详情
pub async fn get_credential(
    State(state): State<AppState>,
    Extension(token): Extension<ValidatedToken>,
    Path(id): Path<String>,
) -> Result<Json<GetCredentialResponse>, ApiError> {
    // 验证 Scope: credential:read
    require_scope(TokenScope::CredentialRead)(&token)
        .map_err(|e| ApiError::new("forbidden", e.message))?;

    let credential_id = CredentialId::from_string(id.clone())
        .map_err(|e| ApiError::new("invalid_request", e.to_string()))?;

    let tenant_id = TenantId::new(&token.tenant_id);
    let user_id = UserId::new(&token.user_id);

    let metadata = state
        .vault
        .get_credential_metadata(&credential_id, &tenant_id, &user_id)
        .map_err(|e| ApiError::new("internal_error", e.to_string()))?
        .ok_or_else(|| ApiError::new("not_found", "凭证不存在"))?;

    // 记录访问审计日志
    state.audit_logger.log_credential_accessed(
        &token.tenant_id,
        &token.user_id,
        &metadata.credential_id,
    );

    Ok(Json(GetCredentialResponse {
        credential_id: metadata.credential_id,
        service_id: metadata.service_id,
        credential_type: metadata.credential_type.as_str().to_string(),
        created_at: metadata.created_at,
        expires_at: metadata.expires_at,
        is_deleted: metadata.is_deleted,
    }))
}

/// 解密凭证请求
#[derive(Debug, Deserialize)]
pub struct DecryptCredentialRequest {
    /// 请求解密的理由（用于审计）
    pub reason: Option<String>,
}

/// 解密凭证响应
#[derive(Debug, Serialize)]
pub struct DecryptCredentialResponse {
    pub credential_id: String,
    pub service_id: String,
    pub credential_type: String,
    /// 解密的明文数据
    pub plaintext_data: serde_json::Value,
}

/// POST /api/v1/credentials/:id/decrypt - 解密凭证
pub async fn decrypt_credential_endpoint(
    State(state): State<AppState>,
    Extension(token): Extension<ValidatedToken>,
    Path(id): Path<String>,
    Json(_request): Json<DecryptCredentialRequest>,
) -> Result<Json<DecryptCredentialResponse>, ApiError> {
    // 验证 Scope: credential:decrypt
    require_scope(TokenScope::CredentialDecrypt)(&token)
        .map_err(|e| ApiError::new("forbidden", e.message))?;

    let credential_id = CredentialId::from_string(id.clone())
        .map_err(|e| ApiError::new("invalid_request", e.to_string()))?;

    let tenant_id = TenantId::new(&token.tenant_id);
    let user_id = UserId::new(&token.user_id);

    // 获取完整凭证（含加密载荷）
    let entry = state
        .vault
        .get_credential(&credential_id, &tenant_id, &user_id)
        .map_err(|e| ApiError::new("internal_error", e.to_string()))?
        .ok_or_else(|| ApiError::new("not_found", "凭证不存在"))?;

    // 在 TEE 内解密密文
    let plaintext_bytes = match decrypt_credential_in_tee(&state, &entry).await {
        Ok(data) => {
            state
                .audit_logger
                .log_decryption_attempt(&token.tenant_id, &token.user_id, &id, true);
            data
        }
        Err(e) => {
            state
                .audit_logger
                .log_decryption_attempt(&token.tenant_id, &token.user_id, &id, false);
            return Err(ApiError::new("internal_error", e));
        }
    };

    // 解析明文为 JSON
    let plaintext_data: serde_json::Value = match serde_json::from_slice(&plaintext_bytes) {
        Ok(v) => v,
        Err(_) => match String::from_utf8(plaintext_bytes) {
            Ok(s) => serde_json::Value::String(s),
            Err(e) => {
                return Err(ApiError::new(
                    "invalid_request",
                    format!("Failed to decode plaintext as UTF-8: {}", e),
                ));
            }
        },
    };

    Ok(Json(DecryptCredentialResponse {
        credential_id: entry.credential_id.as_str().to_string(),
        service_id: entry.service_id.as_str().to_string(),
        credential_type: entry.credential_type.as_str().to_string(),
        plaintext_data,
    }))
}

/// 在 TEE 内解密凭证
async fn decrypt_credential_in_tee(
    state: &AppState,
    entry: &VaultEntry,
) -> Result<Vec<u8>, String> {
    // 构建 EncryptedBlob
    let blob = EncryptedBlob {
        version: entry.encrypted_payload.version,
        algorithm: entry.encrypted_payload.algorithm.clone(),
        kdf: entry.encrypted_payload.kdf.clone(),
        nonce: entry.encrypted_payload.nonce.clone(),
        auth_tag: entry.encrypted_payload.auth_tag.clone(),
        ciphertext: entry.encrypted_payload.ciphertext.clone(),
        aad_hash: None,
    };

    // 派生 L3 密钥
    let l3_key = {
        let hierarchy = state.key_hierarchy.write().await;
        let l2_key = hierarchy
            .derive_user_vault_key(entry.tenant_id.as_str(), entry.user_id.hash())
            .map_err(|e| format!("L2 密钥派生失败: {}", e))?;

        // 注意：AES-GCM 是对称加密，解密时使用与加密相同的 KeyPurpose
        hierarchy
            .derive_credential_key(
                &l2_key,
                entry.credential_id.as_str(),
                KeyPurpose::CredentialEncryption,
            )
            .map_err(|e| format!("L3 密钥派生失败: {}", e))?
    };

    // 执行解密
    let aad = format!("{}:{}", entry.tenant_id.as_str(), entry.user_id.hash());
    let plaintext = decrypt_credential(&l3_key, &blob, Some(aad.as_bytes()))
        .map_err(|e| format!("解密失败: {:?}", e))?;

    Ok(plaintext)
}

/// 删除凭证响应
#[derive(Debug, Serialize)]
pub struct DeleteCredentialResponse {
    pub credential_id: String,
    pub deleted: bool,
}

/// DELETE /api/v1/credentials/:id - 删除凭证（软删除）
pub async fn delete_credential(
    State(state): State<AppState>,
    Extension(token): Extension<ValidatedToken>,
    Path(id): Path<String>,
) -> Result<Json<DeleteCredentialResponse>, ApiError> {
    // 验证 Scope: credential:write 或 admin
    require_any_scope(vec![TokenScope::CredentialWrite, TokenScope::Admin])(&token)
        .map_err(|e| ApiError::new("forbidden", e.message))?;

    let credential_id = CredentialId::from_string(id.clone())
        .map_err(|e| ApiError::new("invalid_request", e.to_string()))?;

    let tenant_id = TenantId::new(&token.tenant_id);
    let user_id = UserId::new(&token.user_id);

    let deleted = state
        .vault
        .delete_credential(&credential_id, &tenant_id, &user_id)
        .map_err(|e| ApiError::new("internal_error", e.to_string()))?;

    if !deleted {
        return Err(ApiError::new("not_found", "凭证不存在"));
    }

    // 记录审计日志
    state.audit_logger.log_credential_deleted(
        &token.tenant_id,
        &token.user_id,
        &credential_id.as_str(),
    );

    Ok(Json(DeleteCredentialResponse {
        credential_id: credential_id.as_str().to_string(),
        deleted: true,
    }))
}

/// 更新凭证请求
#[derive(Debug, Deserialize)]
pub struct UpdateCredentialApiRequest {
    /// 新的明文凭证内容
    pub plaintext_data: serde_json::Value,
    /// 变更原因（用于审计）
    pub change_reason: Option<String>,
}

/// 更新凭证响应
#[derive(Debug, Serialize)]
pub struct UpdateCredentialApiResponse {
    pub credential_id: String,
    pub version: u32,
    pub service_id: String,
    pub credential_type: String,
    pub updated_at: String,
    pub previous_version: u32,
}

/// PUT /api/v1/credentials/:id - 更新凭证（创建新版本）
pub async fn update_credential(
    State(state): State<AppState>,
    Extension(token): Extension<ValidatedToken>,
    Path(id): Path<String>,
    Json(request): Json<UpdateCredentialApiRequest>,
) -> Result<Json<UpdateCredentialApiResponse>, ApiError> {
    // 验证 Scope: credential:write
    require_scope(TokenScope::CredentialWrite)(&token)
        .map_err(|e| ApiError::new("forbidden", e.message))?;

    let credential_id = CredentialId::from_string(id.clone())
        .map_err(|e| ApiError::new("invalid_request", e.to_string()))?;

    let tenant_id = TenantId::new(&token.tenant_id);
    let user_id = UserId::new(&token.user_id);

    // 加密新的凭证内容
    let encrypted_payload = encrypt_credential_update(
        &state,
        tenant_id.as_str(),
        &user_id,
        &credential_id,
        &request.plaintext_data,
    )
    .await
    .map_err(|e| ApiError::new("internal_error", e))?;

    // 更新凭证
    let update_result = state
        .vault
        .update_credential_with_version(
            &credential_id,
            &tenant_id,
            &user_id,
            encrypted_payload,
            request.change_reason,
        )
        .map_err(|e| ApiError::new("internal_error", e.to_string()))?;

    Ok(Json(UpdateCredentialApiResponse {
        credential_id: id,
        version: update_result.new_version,
        service_id: update_result.service_id,
        credential_type: update_result.credential_type,
        updated_at: chrono::Utc::now().to_rfc3339(),
        previous_version: update_result.previous_version,
    }))
}

/// 加密更新后的凭证内容
///
/// 修复说明：
/// - 使用 user_id.hash() 派生 L2 密钥，与解密流程一致
/// - 使用 user_id.hash() 构建 AAD，与解密流程一致
async fn encrypt_credential_update(
    state: &AppState,
    tenant_id: &str,
    user_id: &UserId,
    credential_id: &CredentialId,
    plaintext: &serde_json::Value,
) -> Result<EncryptedPayload, String> {
    // 序列化明文
    let plaintext_bytes =
        serde_json::to_vec(plaintext).map_err(|e| format!("明文序列化失败: {}", e))?;

    // 派生 L3 密钥
    // 使用 user_id.hash() 保持与解密流程一致
    let l3_key = {
        let hierarchy = state.key_hierarchy.write().await;
        let l2_key = hierarchy
            .derive_user_vault_key(tenant_id, user_id.hash())
            .map_err(|e| format!("L2 密钥派生失败: {}", e))?;

        hierarchy
            .derive_credential_key(
                &l2_key,
                credential_id.as_str(),
                KeyPurpose::CredentialEncryption,
            )
            .map_err(|e| format!("L3 密钥派生失败: {}", e))?
    };

    // 执行加密
    // 使用 user_id.hash() 构建 AAD，与解密流程一致
    let aad = format!("{}:{}", tenant_id, user_id.hash());
    let blob =
        crate::crypto::cipher::encrypt_credential(&l3_key, &plaintext_bytes, Some(aad.as_bytes()))
            .map_err(|e| format!("加密失败: {}", e))?;

    Ok(EncryptedPayload::from_blob(&blob))
}

/// 构建凭证 API 路由
pub fn routes() -> axum::Router<AppState> {
    use axum::routing::{delete, get, post, put};

    axum::Router::new()
        .route("/credentials", post(create_credential))
        .route("/credentials", get(list_credentials))
        .route("/credentials/:id", get(get_credential))
        .route("/credentials/:id", put(update_credential))
        .route(
            "/credentials/:id/decrypt",
            post(decrypt_credential_endpoint),
        )
        .route("/credentials/:id", delete(delete_credential))
        .route(
            "/credentials/:id/versions",
            get(crate::api::versions::get_version_history),
        )
        .route(
            "/credentials/:id/versions/:version",
            get(crate::api::versions::get_version_detail),
        )
        .route(
            "/credentials/:id/rollback",
            post(crate::api::versions::rollback_credential),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::middleware::TokenScope;
    use crate::api::middleware::tests::create_mock_token;

    #[test]
    fn test_scope_checking() {
        // 测试 Scope 检查逻辑
        let token = create_mock_token(
            "tenant_123",
            "user_456",
            vec![TokenScope::CredentialRead, TokenScope::CredentialWrite],
        );

        assert!(token.has_scope(&TokenScope::CredentialRead));
        assert!(token.has_scope(&TokenScope::CredentialWrite));
        assert!(!token.has_scope(&TokenScope::CredentialDecrypt));
        assert!(!token.has_scope(&TokenScope::Admin));
    }

    #[test]
    fn test_admin_scope_has_all_permissions() {
        // Admin scope 应该拥有所有权限
        let token = create_mock_token("tenant_123", "user_456", vec![TokenScope::Admin]);

        assert!(token.has_scope(&TokenScope::CredentialRead));
        assert!(token.has_scope(&TokenScope::CredentialWrite));
        assert!(token.has_scope(&TokenScope::CredentialDecrypt));
    }

    #[test]
    fn test_any_scope_checking() {
        let token = create_mock_token("tenant_123", "user_456", vec![TokenScope::CredentialWrite]);

        assert!(token.has_any_scope(&[TokenScope::CredentialRead, TokenScope::CredentialWrite]));
        assert!(!token.has_any_scope(&[TokenScope::CredentialRead, TokenScope::CredentialDecrypt]));
    }
}
