//! CredBridge MCP 工具定义
//!
//! 定义 MCP Server 暴露的工具和相关的请求/响应类型。
//!
//! ## 工具列表
//!
//! - `list_credentials`: 列出用户凭证列表
//! - `get_credential`: 获取单个凭证元数据
//! - `create_credential`: 创建新凭证
//! - `update_credential`: 更新现有凭证
//! - `delete_credential`: 删除凭证（软删除）
//! - `decrypt_credential`: 在 TEE 内解密凭证
//! - `tee_status`: 获取 TEE 状态信息

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use rmcp::schemars;
use tracing::{info, debug, warn};
use zeroize::Zeroize;

use vault_service::vault::{CredentialId, TenantId, UserId, CredentialFilter, ServiceId, EncryptedPayload};
use vault_service::audit::{AuditEntry, AuditAction, Outcome, PiiRedactor};
use vault_service::crypto::{EncryptedBlob, KeyPurpose, decrypt_credential as crypto_decrypt};

// 公开 CredentialType 供其他模块使用
pub use vault_service::models::CredentialType;

use crate::McpServerState;

/// MCP 工具集合
#[derive(Clone)]
pub struct CredBridgeTools {
    state: Arc<McpServerState>,
}

impl CredBridgeTools {
    /// 创建新的工具集合
    pub fn new(state: Arc<McpServerState>) -> Self {
        Self { state }
    }

    /// 列出凭证列表
    pub async fn list_credentials(
        &self,
        tenant_id: &str,
        user_id: &str,
    ) -> Result<ListCredentialsResponse, ToolError> {
        debug!("Listing credentials for tenant: {}, user: {}", tenant_id, user_id);

        // 记录审计日志
        let audit_entry = AuditEntry::new(
            user_id,
            &format!("mcp_session_{}", uuid::Uuid::new_v4()),
            "credbridge-mcp-server",
            AuditAction::CredentialAccess,
            Outcome::Success,
            "mrenclave_placeholder",
            &format!("jti_{}", uuid::Uuid::new_v4()),
        );

        let _ = self.state.audit.record(audit_entry);

        // 获取凭证列表
        let tenant = TenantId::new(tenant_id);
        let user = UserId::new(user_id);
        let filter = CredentialFilter::default();

        let results = self.state.vault
            .list_credentials(&tenant, &user, filter)
            .map_err(|e| ToolError::VaultError(e.to_string()))?;

        let credentials: Vec<CredentialSummary> = results
            .credentials
            .into_iter()
            .map(|metadata| CredentialSummary {
                id: metadata.credential_id,
                service_id: metadata.service_id,
                credential_type: metadata.credential_type.as_str().to_string(),
                created_at: metadata.created_at,
                expires_at: metadata.expires_at,
            })
            .collect();

        info!("Found {} credentials for tenant {}", credentials.len(), tenant_id);

        Ok(ListCredentialsResponse {
            credentials,
            total: results.total,
        })
    }

    /// 获取单个凭证元数据
    pub async fn get_credential(
        &self,
        tenant_id: &str,
        user_id: &str,
        credential_id: &str,
    ) -> Result<GetCredentialResponse, ToolError> {
        debug!("Getting credential {} for tenant: {}, user: {}",
               credential_id, tenant_id, user_id);

        let tenant = TenantId::new(tenant_id);
        let cred_id = CredentialId::from_string(credential_id.to_string())
            .map_err(|e| ToolError::InvalidInput(format!("Invalid credential ID: {}", e)))?;

        // 获取凭证元数据（不包含加密内容）
        let metadata = self.state.vault
            .get_credential_metadata(&cred_id, &tenant, &UserId::new(user_id))
            .map_err(|e| ToolError::VaultError(e.to_string()))?
            .ok_or_else(|| ToolError::NotFound(format!("Credential {} not found", credential_id)))?;

        // 记录审计日志
        let audit_entry = AuditEntry::new(
            user_id,
            &format!("mcp_session_{}", uuid::Uuid::new_v4()),
            "credbridge-mcp-server",
            AuditAction::CredentialAccess,
            Outcome::Success,
            "mrenclave_placeholder",
            &format!("jti_{}", uuid::Uuid::new_v4()),
        );

        let _ = self.state.audit.record(audit_entry);

        Ok(GetCredentialResponse {
            id: metadata.credential_id,
            service_id: metadata.service_id,
            credential_type: metadata.credential_type.as_str().to_string(),
            created_at: metadata.created_at,
            updated_at: None,  // CredentialMetadata 中没有 updated_at
            expires_at: metadata.expires_at,
            granted_scopes: vec![],  // CredentialMetadata 中没有 granted_scopes
            mfa_hint: None,  // CredentialMetadata 中没有 mfa_hint
        })
    }

    /// 解密凭证
    pub async fn decrypt_credential(
        &self,
        tenant_id: &str,
        user_id: &str,
        credential_id: &str,
        scope: &str,
    ) -> Result<DecryptCredentialResponse, ToolError> {
        info!("Decrypting credential {} for tenant: {}, user: {}",
              credential_id, tenant_id, user_id);

        // 验证 Scope 权限
        if !self.validate_scope(scope, "credential:decrypt") {
            // 记录权限拒绝审计日志
            let audit_entry = AuditEntry::new(
                user_id,
                &format!("mcp_session_{}", uuid::Uuid::new_v4()),
                "credbridge-mcp-server",
                AuditAction::CredentialDecrypt,
                Outcome::Denied,
                "mrenclave_placeholder",
                &format!("jti_{}", uuid::Uuid::new_v4()),
            );
            let _ = self.state.audit.record(audit_entry);

            return Err(ToolError::PermissionDenied(
                "Missing 'credential:decrypt' scope".to_string()
            ));
        }

        // 获取含加密载荷的完整凭证条目
        let tenant = TenantId::new(tenant_id);
        let user = UserId::new(user_id);
        let cred_id = CredentialId::from_string(credential_id.to_string())
            .map_err(|e| ToolError::InvalidInput(format!("Invalid credential ID: {}", e)))?;

        let entry = self.state.vault
            .get_credential(&cred_id, &tenant, &user)
            .map_err(|e| ToolError::VaultError(e.to_string()))?
            .ok_or_else(|| ToolError::NotFound(format!("Credential {} not found", credential_id)))?;

        // 构建 EncryptedBlob 供 crypto 模块解密
        let blob = EncryptedBlob {
            version: entry.encrypted_payload.version,
            algorithm: entry.encrypted_payload.algorithm.clone(),
            kdf: entry.encrypted_payload.kdf.clone(),
            nonce: entry.encrypted_payload.nonce.clone(),
            auth_tag: entry.encrypted_payload.auth_tag.clone(),
            ciphertext: entry.encrypted_payload.ciphertext.clone(),
            aad_hash: None,
        };

        // 派生 L3 凭证密钥（分两步以最小化锁持有时间）
        let user_hash = entry.user_id.hash();
        let credential_id_str = entry.credential_id.as_str().to_string();

        // 步骤1：派生 L2 密钥
        let l2_key = {
            let hierarchy = self.state.key_hierarchy.read().await;
            hierarchy
                .derive_user_vault_key(entry.tenant_id.as_str(), user_hash)
                .map_err(|e| ToolError::VaultError(format!("L2 key derivation failed: {}", e)))?
        };

        // 步骤2：派生 L3 密钥（使用新的读锁）
        let l3_key = {
            let hierarchy = self.state.key_hierarchy.read().await;
            hierarchy
                .derive_credential_key(&l2_key, &credential_id_str, KeyPurpose::CredentialEncryption)
                .map_err(|e| ToolError::VaultError(format!("L3 key derivation failed: {}", e)))?
        };

        // 构建 AAD（与加密时保持一致）
        let aad = format!("{}:{}", entry.tenant_id.as_str(), entry.user_id.hash());

        // 执行 AES-256-GCM 解密
        let mut plaintext_bytes = crypto_decrypt(&l3_key, &blob, Some(aad.as_bytes()))
            .map_err(|e| ToolError::VaultError(format!("Decryption failed: {:?}", e)))?;

        // 解析明文为 JSON（先转换，再 zeroize 原始字节）
        let decrypted_data: serde_json::Value = serde_json::from_slice(&plaintext_bytes)
            .unwrap_or_else(|_| {
                String::from_utf8_lossy(&plaintext_bytes)
                    .to_string()
                    .into()
            });

        // 立即清除明文内存，防止在内存中残留敏感数据
        plaintext_bytes.zeroize();

        // 记录成功审计日志（脱敏）
        let audit_entry = AuditEntry::new(
            user_id,
            &format!("mcp_session_{}", uuid::Uuid::new_v4()),
            "credbridge-mcp-server",
            AuditAction::CredentialDecrypt,
            Outcome::Success,
            "mrenclave_placeholder",
            &format!("jti_{}", uuid::Uuid::new_v4()),
        )
        .with_param("credential_id", PiiRedactor::plain(credential_id));

        let _ = self.state.audit.record(audit_entry);

        Ok(DecryptCredentialResponse {
            credential_id: credential_id.to_string(),
            decrypted_data: decrypted_data.to_string(),
            decrypted_at: chrono::Utc::now().to_rfc3339(),
            tee_verified: true,
        })
    }

    /// 获取 TEE 状态
    pub async fn get_tee_status(&self) -> Result<TeeStatusResponse, ToolError> {
        Ok(TeeStatusResponse {
            enabled: true,
            mrenclave: "mrenclave_placeholder".to_string(),
            mrsigner: "mrsigner_placeholder".to_string(),
            isv_svn: 1,
            quote_valid: true,
        })
    }

    /// 验证 Scope 权限
    fn validate_scope(&self, provided_scope: &str, required_scope: &str) -> bool {
        // 检查是否为 admin
        if provided_scope.contains("admin") {
            return true;
        }

        // 检查具体权限
        provided_scope.contains(required_scope)
    }

    /// 创建凭证
    pub async fn create_credential(
        &self,
        tenant_id: &str,
        user_id: &str,
        service_id: &str,
        credential_type: CredentialType,
        plaintext_data: &serde_json::Value,
        scope: &str,
        expires_at: Option<u64>,
    ) -> Result<CreateCredentialResponse, ToolError> {
        info!("Creating credential for tenant: {}, user: {}, service: {}",
              tenant_id, user_id, service_id);

        // 验证 Scope 权限
        if !self.validate_scope(scope, "credential:write") {
            let audit_entry = AuditEntry::new(
                user_id,
                &format!("mcp_session_{}", uuid::Uuid::new_v4()),
                "credbridge-mcp-server",
                AuditAction::CredentialCreate,
                Outcome::Denied,
                "mrenclave_placeholder",
                &format!("jti_{}", uuid::Uuid::new_v4()),
            );
            let _ = self.state.audit.record(audit_entry);

            return Err(ToolError::PermissionDenied(
                "Missing 'credential:write' scope".to_string()
            ));
        }

        // 序列化明文数据
        let plaintext_bytes = serde_json::to_vec(plaintext_data)
            .map_err(|e| ToolError::InvalidInput(format!("Invalid plaintext data: {}", e)))?;

        // 预生成 CredentialId，以便在加密时使用相同的 ID 派生 L3 密钥
        let credential_id_obj = CredentialId::new();

        // 创建用于派生密钥的 user_id
        let user_for_key = UserId::new(user_id);

        // 通过 key_hierarchy 派生 L3 凭证密钥，保证加密/解密使用同一密钥（分两步以最小化锁持有时间）
        let user_hash = user_for_key.hash();
        let credential_id_str = credential_id_obj.as_str().to_string();

        // 步骤1：派生 L2 密钥
        let l2_key = {
            let hierarchy = self.state.key_hierarchy.read().await;
            hierarchy
                .derive_user_vault_key(tenant_id, user_hash)
                .map_err(|e| ToolError::VaultError(format!("L2 key derivation failed: {}", e)))?
        };

        // 步骤2：派生 L3 密钥（使用新的读锁）
        let l3_key = {
            let hierarchy = self.state.key_hierarchy.read().await;
            hierarchy
                .derive_credential_key(&l2_key, &credential_id_str, KeyPurpose::CredentialEncryption)
                .map_err(|e| ToolError::VaultError(format!("L3 key derivation failed: {}", e)))?
        };

        // 使用 AAD（与 decrypt_credential 一致）
        let aad = format!("{}:{}", tenant_id, user_for_key.hash());

        // 用 AES-256-GCM 加密明文
        let blob = vault_service::crypto::encrypt_credential(&l3_key, &plaintext_bytes, Some(aad.as_bytes()))
            .map_err(|e| ToolError::VaultError(format!("Encryption failed: {:?}", e)))?;
        let encrypted_payload = EncryptedPayload::from_blob(&blob);

        // 创建凭证
        let tenant = TenantId::new(tenant_id);
        let user = UserId::new(user_id);
        let service = ServiceId::new(service_id);

        let request = vault_service::vault::CreateCredentialRequest {
            tenant_id: tenant,
            user_id: user,
            service_id: service,
            credential_type,
            expires_at,
        };

        let entry = self.state.vault
            .create_credential_with_id(request, encrypted_payload, credential_id_obj)
            .map_err(|e| ToolError::VaultError(e.to_string()))?;

        // 记录审计日志
        let audit_entry = AuditEntry::new(
            user_id,
            &format!("mcp_session_{}", uuid::Uuid::new_v4()),
            "credbridge-mcp-server",
            AuditAction::CredentialCreate,
            Outcome::Success,
            "mrenclave_placeholder",
            &format!("jti_{}", uuid::Uuid::new_v4()),
        )
        .with_param("credential_id", PiiRedactor::plain(entry.credential_id.as_str()))
        .with_param("service_id", PiiRedactor::plain(service_id));

        let _ = self.state.audit.record(audit_entry);

        info!("Credential created: {} for tenant: {}", entry.credential_id.as_str(), tenant_id);

        Ok(CreateCredentialResponse {
            credential_id: entry.credential_id.as_str().to_string(),
            service_id: entry.service_id.as_str().to_string(),
            credential_type: entry.credential_type.as_str().to_string(),
            created_at: entry.created_at.to_string(),
            expires_at: entry.expires_at.map(|t| t.to_string()),
        })
    }

    /// 更新凭证
    pub async fn update_credential(
        &self,
        tenant_id: &str,
        user_id: &str,
        credential_id: &str,
        plaintext_data: Option<&serde_json::Value>,
        scope: &str,
        expires_at: Option<Option<u64>>,
    ) -> Result<UpdateCredentialResponse, ToolError> {
        info!("Updating credential {} for tenant: {}, user: {}",
              credential_id, tenant_id, user_id);

        // 验证 Scope 权限
        if !self.validate_scope(scope, "credential:write") {
            let audit_entry = AuditEntry::new(
                user_id,
                &format!("mcp_session_{}", uuid::Uuid::new_v4()),
                "credbridge-mcp-server",
                AuditAction::CredentialUpdate,
                Outcome::Denied,
                "mrenclave_placeholder",
                &format!("jti_{}", uuid::Uuid::new_v4()),
            );
            let _ = self.state.audit.record(audit_entry);

            return Err(ToolError::PermissionDenied(
                "Missing 'credential:write' scope".to_string()
            ));
        }

        let tenant = TenantId::new(tenant_id);
        let user = UserId::new(user_id);
        let cred_id = CredentialId::from_string(credential_id.to_string())
            .map_err(|e| ToolError::InvalidInput(format!("Invalid credential ID: {}", e)))?;

        // 如果提供了新的明文数据，使用 key_hierarchy 派生密钥后加密
        let encrypted_payload = if let Some(data) = plaintext_data {
            let plaintext_bytes = serde_json::to_vec(data)
                .map_err(|e| ToolError::InvalidInput(format!("Invalid plaintext data: {}", e)))?;

            let user_for_key = UserId::new(user_id);
            let user_hash = user_for_key.hash();

            // 步骤1：派生 L2 密钥
            let l2_key = {
                let hierarchy = self.state.key_hierarchy.read().await;
                hierarchy
                    .derive_user_vault_key(tenant_id, user_hash)
                    .map_err(|e| ToolError::VaultError(format!("L2 key derivation failed: {}", e)))?
            };

            // 步骤2：派生 L3 密钥（使用新的读锁）
            let l3_key = {
                let hierarchy = self.state.key_hierarchy.read().await;
                hierarchy
                    .derive_credential_key(&l2_key, credential_id, KeyPurpose::CredentialEncryption)
                    .map_err(|e| ToolError::VaultError(format!("L3 key derivation failed: {}", e)))?
            };
            let aad = format!("{}:{}", tenant_id, user_hash);
            let blob = vault_service::crypto::encrypt_credential(&l3_key, &plaintext_bytes, Some(aad.as_bytes()))
                .map_err(|e| ToolError::VaultError(format!("Encryption failed: {:?}", e)))?;
            Some(EncryptedPayload::from_blob(&blob))
        } else {
            None
        };

        // 更新凭证
        let entry = self.state.vault
            .update_credential(&cred_id, &tenant, &user, encrypted_payload, expires_at)
            .map_err(|e| ToolError::VaultError(e.to_string()))?;

        // 记录审计日志
        let audit_entry = AuditEntry::new(
            user_id,
            &format!("mcp_session_{}", uuid::Uuid::new_v4()),
            "credbridge-mcp-server",
            AuditAction::CredentialUpdate,
            Outcome::Success,
            "mrenclave_placeholder",
            &format!("jti_{}", uuid::Uuid::new_v4()),
        )
        .with_param("credential_id", PiiRedactor::plain(credential_id));

        let _ = self.state.audit.record(audit_entry);

        info!("Credential updated: {} for tenant: {}", credential_id, tenant_id);

        Ok(UpdateCredentialResponse {
            credential_id: entry.credential_id.as_str().to_string(),
            service_id: entry.service_id.as_str().to_string(),
            credential_type: entry.credential_type.as_str().to_string(),
            updated_at: entry.updated_at.to_string(),
            expires_at: entry.expires_at.map(|t| t.to_string()),
        })
    }

    /// 删除凭证（软删除）
    pub async fn delete_credential(
        &self,
        tenant_id: &str,
        user_id: &str,
        credential_id: &str,
        scope: &str,
    ) -> Result<DeleteCredentialResponse, ToolError> {
        info!("Deleting credential {} for tenant: {}, user: {}",
              credential_id, tenant_id, user_id);

        // 验证 Scope 权限
        if !self.validate_scope(scope, "credential:write") && !self.validate_scope(scope, "credential:delete") {
            let audit_entry = AuditEntry::new(
                user_id,
                &format!("mcp_session_{}", uuid::Uuid::new_v4()),
                "credbridge-mcp-server",
                AuditAction::CredentialDelete,
                Outcome::Denied,
                "mrenclave_placeholder",
                &format!("jti_{}", uuid::Uuid::new_v4()),
            );
            let _ = self.state.audit.record(audit_entry);

            return Err(ToolError::PermissionDenied(
                "Missing 'credential:write' or 'credential:delete' scope".to_string()
            ));
        }

        let tenant = TenantId::new(tenant_id);
        let user = UserId::new(user_id);
        let cred_id = CredentialId::from_string(credential_id.to_string())
            .map_err(|e| ToolError::InvalidInput(format!("Invalid credential ID: {}", e)))?;

        // 删除凭证
        let deleted = self.state.vault
            .delete_credential(&cred_id, &tenant, &user)
            .map_err(|e| ToolError::VaultError(e.to_string()))?;

        if !deleted {
            return Err(ToolError::NotFound(format!("Credential {} not found", credential_id)));
        }

        // 记录审计日志
        let audit_entry = AuditEntry::new(
            user_id,
            &format!("mcp_session_{}", uuid::Uuid::new_v4()),
            "credbridge-mcp-server",
            AuditAction::CredentialDelete,
            Outcome::Success,
            "mrenclave_placeholder",
            &format!("jti_{}", uuid::Uuid::new_v4()),
        )
        .with_param("credential_id", PiiRedactor::plain(credential_id));

        let _ = self.state.audit.record(audit_entry);

        info!("Credential deleted: {} for tenant: {}", credential_id, tenant_id);

        Ok(DeleteCredentialResponse {
            credential_id: credential_id.to_string(),
            deleted: true,
            deleted_at: chrono::Utc::now().to_rfc3339(),
        })
    }
}

/// 工具错误类型
#[derive(Debug, Clone)]
pub enum ToolError {
    NotFound(String),
    PermissionDenied(String),
    VaultError(String),
    AuditError(String),
    InvalidInput(String),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::NotFound(msg) => write!(f, "Not found: {}", msg),
            ToolError::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            ToolError::VaultError(msg) => write!(f, "Vault error: {}", msg),
            ToolError::AuditError(msg) => write!(f, "Audit error: {}", msg),
            ToolError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
        }
    }
}

impl std::error::Error for ToolError {}

// ============ 请求/响应类型 ============

/// 列出凭证请求
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ListCredentialsRequest {
    /// 租户 ID
    pub tenant_id: String,
    /// 用户 ID
    pub user_id: String,
    /// 服务筛选（可选）
    pub service_id: Option<String>,
    /// 凭证类型筛选（可选）
    pub credential_type: Option<String>,
}

/// 列出凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListCredentialsResponse {
    /// 凭证列表（脱敏）
    pub credentials: Vec<CredentialSummary>,
    /// 总数
    pub total: usize,
}

/// 凭证摘要（脱敏）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialSummary {
    /// 凭证 ID
    pub id: String,
    /// 服务标识
    pub service_id: String,
    /// 凭证类型
    pub credential_type: String,
    /// 创建时间
    pub created_at: String,
    /// 过期时间
    pub expires_at: Option<String>,
}

/// 获取凭证请求
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct GetCredentialRequest {
    /// 租户 ID
    pub tenant_id: String,
    /// 用户 ID
    pub user_id: String,
    /// 凭证 ID
    pub credential_id: String,
}

/// 获取凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCredentialResponse {
    /// 凭证 ID
    pub id: String,
    /// 服务标识
    pub service_id: String,
    /// 凭证类型
    pub credential_type: String,
    /// 创建时间
    pub created_at: String,
    /// 更新时间
    pub updated_at: Option<String>,
    /// 过期时间
    pub expires_at: Option<String>,
    /// 已授权的 Scope 列表
    pub granted_scopes: Vec<String>,
    /// MFA 提示信息
    pub mfa_hint: Option<String>,
}

/// 解密凭证请求
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct DecryptCredentialRequest {
    /// 租户 ID
    pub tenant_id: String,
    /// 用户 ID
    pub user_id: String,
    /// 凭证 ID
    pub credential_id: String,
    /// 请求的 Scope（用于权限验证）
    pub scope: String,
}

/// 解密凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecryptCredentialResponse {
    /// 凭证 ID
    pub credential_id: String,
    /// 解密后的数据（JSON 格式）
    pub decrypted_data: String,
    /// 解密时间
    pub decrypted_at: String,
    /// TEE 验证状态
    pub tee_verified: bool,
}

/// TEE 状态响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeeStatusResponse {
    /// TEE 是否启用
    pub enabled: bool,
    /// MRENCLAVE 测量值
    pub mrenclave: String,
    /// MRSIGNER 测量值
    pub mrsigner: String,
    /// ISV SVN 版本
    pub isv_svn: u16,
    /// Quote 是否有效
    pub quote_valid: bool,
}

/// 创建凭证请求
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct CreateCredentialRequest {
    /// 租户 ID
    pub tenant_id: String,
    /// 用户 ID
    pub user_id: String,
    /// 服务 ID
    pub service_id: String,
    /// 凭证类型 (username_password, api_key, oauth_refresh)
    pub credential_type: String,
    /// 凭证明文数据（将被加密）
    pub plaintext_data: serde_json::Value,
    /// 授权的 Scope
    pub scope: String,
    /// 过期时间（Unix 时间戳，可选）
    pub expires_at: Option<u64>,
}

/// 创建凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCredentialResponse {
    /// 凭证 ID
    pub credential_id: String,
    /// 服务标识
    pub service_id: String,
    /// 凭证类型
    pub credential_type: String,
    /// 创建时间
    pub created_at: String,
    /// 过期时间
    pub expires_at: Option<String>,
}

/// 更新凭证请求
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct UpdateCredentialRequest {
    /// 租户 ID
    pub tenant_id: String,
    /// 用户 ID
    pub user_id: String,
    /// 凭证 ID
    pub credential_id: String,
    /// 新的凭证明文数据（可选，如不提供则保持原数据）
    pub plaintext_data: Option<serde_json::Value>,
    /// 授权的 Scope
    pub scope: String,
    /// 新的过期时间（Unix 时间戳，null 表示清除过期时间，不设置表示保持原值）
    pub expires_at: Option<Option<u64>>,
}

/// 更新凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCredentialResponse {
    /// 凭证 ID
    pub credential_id: String,
    /// 服务标识
    pub service_id: String,
    /// 凭证类型
    pub credential_type: String,
    /// 更新时间
    pub updated_at: String,
    /// 过期时间
    pub expires_at: Option<String>,
}

/// 删除凭证请求
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct DeleteCredentialRequest {
    /// 租户 ID
    pub tenant_id: String,
    /// 用户 ID
    pub user_id: String,
    /// 凭证 ID
    pub credential_id: String,
    /// 授权的 Scope
    pub scope: String,
}

/// 删除凭证响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteCredentialResponse {
    /// 凭证 ID
    pub credential_id: String,
    /// 是否删除成功
    pub deleted: bool,
    /// 删除时间
    pub deleted_at: String,
}

/// MCP 工具名称常量
pub mod tool_names {
    pub const LIST_CREDENTIALS: &str = "list_credentials";
    pub const GET_CREDENTIAL: &str = "get_credential";
    pub const CREATE_CREDENTIAL: &str = "create_credential";
    pub const UPDATE_CREDENTIAL: &str = "update_credential";
    pub const DELETE_CREDENTIAL: &str = "delete_credential";
    pub const DECRYPT_CREDENTIAL: &str = "decrypt_credential";
    pub const TEE_STATUS: &str = "tee_status";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tool_error_display() {
        let err = ToolError::NotFound("test credential".to_string());
        assert_eq!(err.to_string(), "Not found: test credential");

        let err = ToolError::PermissionDenied("missing scope".to_string());
        assert_eq!(err.to_string(), "Permission denied: missing scope");
    }

    #[test]
    fn test_credential_summary_serialization() {
        let summary = CredentialSummary {
            id: "cred_123".to_string(),
            service_id: "schwab".to_string(),
            credential_type: "oauth_token".to_string(),
            created_at: "2026-03-11T10:00:00Z".to_string(),
            expires_at: None,
        };

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("cred_123"));
        assert!(json.contains("schwab"));
    }

    #[tokio::test]
    async fn test_validate_scope() {
        let state = Arc::new(crate::McpServerState::new_in_memory().unwrap());
        let tools = CredBridgeTools::new(state);

        assert!(tools.validate_scope("admin", "credential:read"));
        assert!(tools.validate_scope("credential:decrypt", "credential:decrypt"));
        assert!(!tools.validate_scope("credential:read", "credential:decrypt"));
    }
}
