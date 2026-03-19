//! HashiCorp Vault 存储后端实现
//!
//! 实现 EP2-Story2.3: Vault 后端集成
//! - 实现 StorageBackend trait
//! - 使用 Vault KV v2 引擎存储凭证密文
//! - 启用 Vault 自带 AES-256 加密（双重加密）
//!
//! ## 存储路径格式
//!
//! ```text
//! secret/credbridge/{tenant_id}/{credential_id}
//! ```
//!
//! ## 安全特性
//!
//! - **双重加密**: TEE 内 AES-256-GCM + Vault AES-256
//! - **Token 安全**: 仅 TEE Enclave 持有 Vault Token
//! - **路径隔离**: 每个租户独立路径前缀
//! - **版本控制**: KV v2 自动版本管理

use super::client::{VaultClientError, VaultConfig, VaultCredentialData, VaultKvClient};
use super::models::*;
use super::version::CredentialVersion;
use crate::models::CredentialMetadata;
use crate::vault::storage::StorageBackend;
use std::sync::Arc;
use tokio::runtime::Handle;

/// Vault 存储后端错误类型
#[derive(Debug, thiserror::Error)]
pub enum VaultBackendError {
    #[error("Vault 客户端错误: {0}")]
    ClientError(#[from] VaultClientError),

    #[error("异步运行时错误: {0}")]
    RuntimeError(String),

    #[error("序列化错误: {0}")]
    SerializationError(String),

    #[error("反序列化错误: {0}")]
    DeserializationError(String),

    #[error("凭证格式错误: {0}")]
    InvalidCredentialFormat(String),

    #[error("配置错误: {0}")]
    ConfigError(String),
}

/// HashiCorp Vault 存储后端
///
/// 使用 Vault KV v2 引擎作为凭证密文的持久化存储
/// 实现 StorageBackend trait，可无缝集成到现有 CredentialVault
pub struct VaultStorageBackend {
    /// Vault 客户端
    client: Arc<VaultKvClient>,
    /// 运行时句柄（用于在同步上下文中执行异步操作）
    runtime_handle: Handle,
}

impl VaultStorageBackend {
    /// 创建新的 Vault 存储后端
    ///
    /// # Arguments
    /// * `config` - Vault 配置
    ///
    /// # Returns
    /// * `Ok(VaultStorageBackend)` - 初始化成功的后端
    pub async fn new(config: VaultConfig) -> Result<Self, VaultBackendError> {
        let client = VaultKvClient::new(config).await?;

        // 初始化 KV v2 引擎
        client.init_kv_engine().await?;

        Ok(Self {
            client: Arc::new(client),
            runtime_handle: Handle::current(),
        })
    }

    /// 从环境变量创建 Vault 存储后端
    pub async fn from_env() -> Result<Self, VaultBackendError> {
        let config = VaultConfig::from_env()?;
        Self::new(config).await
    }

    /// 获取 Vault 客户端引用
    pub fn client(&self) -> &VaultKvClient {
        &self.client
    }

    /// 在同步上下文中安全执行异步操作
    ///
    /// 使用 `block_in_place` 而非 `block_on`，避免在已有 tokio 运行时的线程上
    /// 调用 `block_on` 导致的死锁（nested block_on 会 panic）。
    /// `block_in_place` 将当前线程标记为 blocking，允许调度器在等待期间迁移其他任务。
    fn block_on<F, T>(&self, future: F) -> Result<T, VaultBackendError>
    where
        F: std::future::Future<Output = Result<T, VaultClientError>>,
    {
        tokio::task::block_in_place(|| {
            self.runtime_handle
                .block_on(future)
                .map_err(VaultBackendError::ClientError)
        })
    }

    /// 将 VaultEntry 转换为 VaultCredentialData
    fn entry_to_data(entry: &VaultEntry) -> Result<VaultCredentialData, VaultBackendError> {
        let encrypted_payload = format!(
            "{}.{}",
            entry.encrypted_payload.ciphertext, entry.encrypted_payload.auth_tag
        );

        let mut data = VaultCredentialData::new(
            entry.credential_id.as_str().to_string(),
            entry.tenant_id.as_str().to_string(),
            entry.user_id.hash().to_string(),
            entry.service_id.as_str().to_string(),
            entry.credential_type.as_str().to_string(),
            entry.created_at,
            entry.expires_at,
            encrypted_payload,
            entry.encrypted_payload.version,
            entry.encrypted_payload.algorithm.clone(),
            entry.encrypted_payload.kdf.clone(),
            entry.encrypted_payload.nonce.clone(),
            entry.encrypted_payload.auth_tag.clone(),
        );
        data.is_deleted = entry.is_deleted;
        // 修正：当 updated_at 为 0 时表示未更新，使用 None 而非 Some(0)
        // 避免审计记录显示为 1970 年
        data.updated_at = if entry.updated_at == 0 {
            None
        } else {
            Some(entry.updated_at)
        };
        Ok(data)
    }

    /// 将 VaultCredentialData 转换为 VaultEntry（简化版）
    fn data_to_entry(
        data: &VaultCredentialData,
        encrypted_payload: EncryptedPayload,
    ) -> VaultEntry {
        VaultEntry {
            credential_id: CredentialId::from_string(data.credential_id.clone())
                .unwrap_or_else(|_| CredentialId::new()),
            version: 1, // 新创建的凭证版本为 1
            tenant_id: TenantId::new(data.tenant_id.clone()),
            user_id: UserId::from_hash(data.user_id_hash.clone()),
            service_id: ServiceId::new(data.service_id.clone()),
            credential_type: match data.credential_type.as_str() {
                "username_password" => crate::models::CredentialType::UsernamePassword,
                "oauth_refresh" => crate::models::CredentialType::OAuthRefresh,
                "api_key" => crate::models::CredentialType::ApiKey,
                "session_cookie" => crate::models::CredentialType::SessionCookie,
                "kyc_document" => crate::models::CredentialType::KycDocument,
                _ => crate::models::CredentialType::ApiKey,
            },
            created_at: data.created_at,
            updated_at: data.updated_at.unwrap_or(data.created_at),
            expires_at: data.expires_at,
            encrypted_payload,
            is_deleted: data.is_deleted,
        }
    }

    /// 从 Vault 数据中解析 EncryptedPayload
    fn parse_encrypted_payload(
        data: &VaultCredentialData,
    ) -> Result<EncryptedPayload, VaultBackendError> {
        Ok(EncryptedPayload {
            version: data.version,
            algorithm: data.algorithm.clone(),
            kdf: data.kdf.clone(),
            nonce: data.nonce.clone(),
            auth_tag: data.auth_tag.clone(),
            ciphertext: data
                .encrypted_payload
                .split('.')
                .next()
                .unwrap_or("")
                .to_string(),
        })
    }
}

impl StorageBackend for VaultStorageBackend {
    fn store(&self, entry: &VaultEntry) -> Result<(), VaultError> {
        // 转换为 Vault 数据格式
        let data =
            Self::entry_to_data(entry).map_err(|e| VaultError::StorageError(e.to_string()))?;

        let json_data = data
            .to_json()
            .map_err(|e| VaultError::SerializationError(e.to_string()))?;

        // 写入 Vault
        self.block_on(self.client.write_secret(
            entry.tenant_id.as_str(),
            entry.credential_id.as_str(),
            &json_data,
        ))
        .map_err(|e| VaultError::StorageError(e.to_string()))?;

        Ok(())
    }

    fn get(&self, credential_id: &CredentialId) -> Result<Option<VaultEntry>, VaultError> {
        // 从 credential_id 中提取 tenant_id
        // 由于 Vault 路径格式为 credbridge/{tenant_id}/{credential_id}
        // 我们需要先获取该凭证的完整路径

        // 首先尝试从所有可能的租户中查找
        // 实际实现中应该使用索引或缓存来优化
        let tenant_ids = self
            .list_all_tenants()
            .map_err(|e| VaultError::StorageError(format!("Failed to list tenants: {}", e)))?;

        for tenant_id in tenant_ids {
            match self.block_on(self.client.read_secret(&tenant_id, credential_id.as_str())) {
                Ok(json_data) => {
                    let data: VaultCredentialData = VaultCredentialData::from_json(json_data)
                        .map_err(|e| {
                            VaultError::SerializationError(format!(
                                "Failed to parse credential data: {}",
                                e
                            ))
                        })?;

                    let encrypted_payload = Self::parse_encrypted_payload(&data)
                        .map_err(|e| VaultError::StorageError(e.to_string()))?;

                    return Ok(Some(Self::data_to_entry(&data, encrypted_payload)));
                }
                Err(VaultBackendError::ClientError(VaultClientError::SecretNotFound(_))) => {
                    continue;
                }
                Err(e) => return Err(VaultError::StorageError(e.to_string())),
            }
        }

        Ok(None)
    }

    fn get_metadata(
        &self,
        credential_id: &CredentialId,
    ) -> Result<Option<CredentialMetadata>, VaultError> {
        // 获取完整条目并提取元数据
        match self.get(credential_id)? {
            Some(entry) => Ok(Some(entry.metadata())),
            None => Ok(None),
        }
    }

    fn query(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        filter: &CredentialFilter,
    ) -> Result<CredentialQueryResult, VaultError> {
        // 列出租户的所有凭证
        let credential_ids = self
            .block_on(self.client.list_secrets(tenant_id.as_str()))
            .map_err(|e| VaultError::StorageError(e.to_string()))?;

        let mut credentials = Vec::new();

        for cred_id in credential_ids {
            if let Ok(Some(entry)) = self.get(&CredentialId::from_string(cred_id)?) {
                // 验证租户隔离
                if entry.tenant_id != *tenant_id || entry.user_id != *user_id {
                    continue;
                }

                // 应用过滤器
                if entry.is_deleted && !filter.include_deleted {
                    continue;
                }

                if filter.only_valid && entry.is_expired() {
                    continue;
                }

                if let Some(ref service_id) = filter.service_id
                    && entry.service_id != *service_id
                {
                    continue;
                }

                if let Some(ref cred_type) = filter.credential_type
                    && entry.credential_type != *cred_type
                {
                    continue;
                }

                credentials.push(entry.metadata());
            }
        }

        let total = credentials.len();

        Ok(CredentialQueryResult { credentials, total })
    }

    fn delete(&self, credential_id: &CredentialId) -> Result<bool, VaultError> {
        // 软删除：标记为已删除但不从 Vault 中移除
        if let Some(mut entry) = self.get(credential_id)? {
            entry.mark_deleted();

            // 更新 Vault 中的记录
            let data =
                Self::entry_to_data(&entry).map_err(|e| VaultError::StorageError(e.to_string()))?;
            let json_data = data
                .to_json()
                .map_err(|e| VaultError::SerializationError(e.to_string()))?;

            self.block_on(self.client.write_secret(
                entry.tenant_id.as_str(),
                credential_id.as_str(),
                &json_data,
            ))
            .map_err(|e| VaultError::StorageError(e.to_string()))?;

            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn purge(&self, credential_id: &CredentialId) -> Result<bool, VaultError> {
        // 物理删除：从 Vault 中永久移除
        let tenant_ids = self
            .list_all_tenants()
            .map_err(|e| VaultError::StorageError(format!("Failed to list tenants: {}", e)))?;

        for tenant_id in tenant_ids {
            match self.block_on(
                self.client
                    .delete_secret(&tenant_id, credential_id.as_str()),
            ) {
                Ok(_) => return Ok(true),
                Err(VaultBackendError::ClientError(VaultClientError::SecretNotFound(_))) => {
                    continue;
                }
                Err(e) => return Err(VaultError::StorageError(e.to_string())),
            }
        }

        Ok(false)
    }

    fn exists(&self, credential_id: &CredentialId) -> Result<bool, VaultError> {
        match self.get(credential_id) {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn update(&self, entry: &VaultEntry) -> Result<(), VaultError> {
        // 检查条目是否存在
        if !self.exists(&entry.credential_id)? {
            return Err(VaultError::CredentialNotFound(
                entry.credential_id.as_str().to_string(),
            ));
        }

        // 更新 Vault 中的记录
        let data =
            Self::entry_to_data(entry).map_err(|e| VaultError::StorageError(e.to_string()))?;
        let json_data = data
            .to_json()
            .map_err(|e| VaultError::SerializationError(e.to_string()))?;

        self.block_on(self.client.write_secret(
            entry.tenant_id.as_str(),
            entry.credential_id.as_str(),
            &json_data,
        ))
        .map_err(|e| VaultError::StorageError(e.to_string()))?;

        Ok(())
    }

    fn create_version_record(&self, _version: &CredentialVersion) -> Result<(), VaultError> {
        // Vault 后端不支持版本历史记录
        // 版本历史由 Vault 的内置版本控制处理
        Ok(())
    }

    fn get_version_history(
        &self,
        credential_id: &CredentialId,
    ) -> Result<Vec<CredentialVersion>, VaultError> {
        // Vault 后端不支持版本历史记录
        // 返回空列表
        let _ = credential_id;
        Ok(Vec::new())
    }

    fn get_version(
        &self,
        credential_id: &CredentialId,
        version: u32,
    ) -> Result<Option<CredentialVersion>, VaultError> {
        // Vault 后端不支持版本历史记录
        let _ = (credential_id, version);
        Ok(None)
    }
}

impl VaultStorageBackend {
    /// 列出所有租户 ID（内部方法）
    ///
    /// 通过列出 Vault KV2 中 `credbridge/` 路径下的子目录获取租户列表。
    /// Vault list 操作返回条目可能带有尾部斜杠（目录），需要去除。
    fn list_all_tenants(&self) -> Result<Vec<String>, VaultBackendError> {
        // list_secrets("") 内部构建路径 "credbridge/"，列出所有租户子目录
        let entries = self.block_on(self.client.list_secrets(""))?;
        // Vault 对目录条目返回时附带尾部斜杠，去除后即为租户 ID
        let tenant_ids = entries
            .into_iter()
            .map(|e| e.trim_end_matches('/').to_string())
            .filter(|e| !e.is_empty())
            .collect();
        Ok(tenant_ids)
    }
}

/// Vault 存储后端构建器
pub struct VaultStorageBackendBuilder {
    config: Option<VaultConfig>,
}

impl VaultStorageBackendBuilder {
    /// 创建新的构建器
    pub fn new() -> Self {
        Self { config: None }
    }

    /// 设置 Vault 配置
    pub fn config(mut self, config: VaultConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// 从环境变量配置
    pub fn from_env(mut self) -> Result<Self, VaultBackendError> {
        self.config = Some(VaultConfig::from_env()?);
        Ok(self)
    }

    /// 构建 Vault 存储后端
    pub async fn build(self) -> Result<VaultStorageBackend, VaultBackendError> {
        let config = self
            .config
            .ok_or_else(|| VaultBackendError::ConfigError("Vault config not set".to_string()))?;

        VaultStorageBackend::new(config).await
    }
}

impl Default for VaultStorageBackendBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Vault 健康检查
pub async fn check_vault_health(
    config: &VaultConfig,
) -> Result<VaultHealthStatus, VaultClientError> {
    use std::time::Duration;
    use tokio::time::timeout;

    let check = async {
        let client = VaultKvClient::new(config.clone()).await?;

        // 尝试列出根目录来验证权限
        match client.list_secrets("").await {
            Ok(_) => Ok(VaultHealthStatus::Healthy),
            Err(VaultClientError::AuthenticationFailed(_)) => {
                Ok(VaultHealthStatus::Unauthenticated)
            }
            Err(e) => Ok(VaultHealthStatus::Unhealthy(e.to_string())),
        }
    };

    match timeout(Duration::from_secs(5), check).await {
        Ok(result) => result,
        Err(_) => Ok(VaultHealthStatus::Timeout),
    }
}

/// Vault 健康状态
#[derive(Debug, Clone)]
pub enum VaultHealthStatus {
    /// 健康
    Healthy,
    /// 未认证
    Unauthenticated,
    /// 不健康
    Unhealthy(String),
    /// 超时
    Timeout,
}

impl VaultHealthStatus {
    /// 是否健康
    pub fn is_healthy(&self) -> bool {
        matches!(self, VaultHealthStatus::Healthy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::constants;
    use crate::models::CredentialType;

    fn create_test_entry() -> VaultEntry {
        VaultEntry::new(
            TenantId::new("tenant_123"),
            UserId::new("user_456"),
            ServiceId::new("schwab"),
            CredentialType::UsernamePassword,
            EncryptedPayload::new(
                constants::PROTOCOL_VERSION,
                constants::ALGORITHM_AES_256_GCM,
                constants::KDF_HKDF_SHA256,
                vec![0u8; constants::NONCE_LENGTH],
                vec![0u8; constants::AUTH_TAG_LENGTH],
                vec![1, 2, 3, 4, 5],
            ),
            None,
        )
    }

    #[test]
    fn test_entry_to_data_conversion() {
        let entry = create_test_entry();
        let data = VaultStorageBackend::entry_to_data(&entry).unwrap();

        assert_eq!(data.credential_id, entry.credential_id.as_str());
        assert_eq!(data.tenant_id, entry.tenant_id.as_str());
        assert_eq!(data.service_id, entry.service_id.as_str());
        assert_eq!(data.version, constants::PROTOCOL_VERSION);
        assert_eq!(data.algorithm, constants::ALGORITHM_AES_256_GCM);
    }

    #[test]
    fn test_vault_credential_data_roundtrip() {
        let entry = create_test_entry();
        let data = VaultStorageBackend::entry_to_data(&entry).unwrap();

        // 序列化
        let json = data.to_json().unwrap();

        // 反序列化
        let parsed: VaultCredentialData = VaultCredentialData::from_json(json).unwrap();

        // 验证
        assert_eq!(parsed.credential_id, data.credential_id);
        assert_eq!(parsed.tenant_id, data.tenant_id);
        assert_eq!(parsed.encrypted_payload, data.encrypted_payload);
    }

    #[test]
    fn test_vault_health_status() {
        assert!(VaultHealthStatus::Healthy.is_healthy());
        assert!(!VaultHealthStatus::Unauthenticated.is_healthy());
        assert!(!VaultHealthStatus::Unhealthy("error".to_string()).is_healthy());
        assert!(!VaultHealthStatus::Timeout.is_healthy());
    }
}
