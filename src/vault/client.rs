//! HashiCorp Vault 客户端封装
//!
//! 实现 EP2-Story2.3: Vault 后端集成
//! - Vault 连接管理
//! - KV v2 引擎操作
//! - Token 认证

use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use vaultrs::api::kv2::responses::SecretVersionMetadata;
use vaultrs::client::{VaultClient, VaultClientSettingsBuilder};

/// Vault 客户端错误类型
#[derive(Error, Debug)]
pub enum VaultClientError {
    #[error("Vault 连接失败: {0}")]
    ConnectionFailed(String),

    #[error("认证失败: {0}")]
    AuthenticationFailed(String),

    #[error("密钥不存在: {0}")]
    SecretNotFound(String),

    #[error("写入失败: {0}")]
    WriteFailed(String),

    #[error("读取失败: {0}")]
    ReadFailed(String),

    #[error("删除失败: {0}")]
    DeleteFailed(String),

    #[error("配置错误: {0}")]
    ConfigError(String),
}

/// Vault 配置
#[derive(Clone, Deserialize)]
pub struct VaultConfig {
    /// Vault 服务器地址 (如: http://127.0.0.1:8200)
    pub addr: String,

    /// Vault Token（仅 TEE Enclave 持有）
    pub token: String,

    /// KV v2 引擎挂载路径
    pub mount_path: String,

    /// 命名空间（企业版支持）
    pub namespace: Option<String>,

    /// CA 证书路径（用于 TLS 验证）
    pub ca_cert_path: Option<String>,

    /// 客户端证书路径（用于 mTLS）
    pub client_cert_path: Option<String>,

    /// 客户端密钥路径（用于 mTLS）
    pub client_key_path: Option<String>,

    /// 连接超时（秒）
    pub timeout_seconds: u64,

    /// 最大重试次数
    pub max_retries: u32,
}

impl std::fmt::Debug for VaultConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VaultConfig")
            .field("addr", &self.addr)
            .field("token", &"[REDACTED]")
            .field("mount_path", &self.mount_path)
            .field("namespace", &self.namespace)
            .field("ca_cert_path", &self.ca_cert_path)
            .field("client_cert_path", &self.client_cert_path)
            .field("client_key_path", &self.client_key_path)
            .field("timeout_seconds", &self.timeout_seconds)
            .field("max_retries", &self.max_retries)
            .finish()
    }
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            addr: "http://127.0.0.1:8200".to_string(),
            token: String::new(),
            mount_path: "secret".to_string(),
            namespace: None,
            ca_cert_path: None,
            client_cert_path: None,
            client_key_path: None,
            timeout_seconds: 30,
            max_retries: 3,
        }
    }
}

impl VaultConfig {
    /// 从环境变量创建配置
    pub fn from_env() -> Result<Self, VaultClientError> {
        use std::env;

        let addr = env::var("VAULT_ADDR")
            .map_err(|_| VaultClientError::ConfigError("VAULT_ADDR not set".to_string()))?;

        let token = env::var("VAULT_TOKEN")
            .map_err(|_| VaultClientError::ConfigError("VAULT_TOKEN not set".to_string()))?;

        let mount_path = env::var("VAULT_MOUNT_PATH").unwrap_or_else(|_| "secret".to_string());

        let namespace = env::var("VAULT_NAMESPACE").ok();

        let ca_cert_path = env::var("VAULT_CA_CERT").ok();
        let client_cert_path = env::var("VAULT_CLIENT_CERT").ok();
        let client_key_path = env::var("VAULT_CLIENT_KEY").ok();

        let timeout_seconds = env::var("VAULT_TIMEOUT_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);

        let max_retries = env::var("VAULT_MAX_RETRIES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3);

        Ok(Self {
            addr,
            token,
            mount_path,
            namespace,
            ca_cert_path,
            client_cert_path,
            client_key_path,
            timeout_seconds,
            max_retries,
        })
    }

    /// 验证配置有效性
    pub fn validate(&self) -> Result<(), VaultClientError> {
        if self.addr.is_empty() {
            return Err(VaultClientError::ConfigError(
                "Vault address is empty".to_string(),
            ));
        }

        if self.token.is_empty() {
            return Err(VaultClientError::ConfigError(
                "Vault token is empty".to_string(),
            ));
        }

        if self.mount_path.is_empty() {
            return Err(VaultClientError::ConfigError(
                "Mount path is empty".to_string(),
            ));
        }

        Ok(())
    }

    /// 构建路径前缀（用于 CredBridge 凭证存储）
    ///
    /// 路径格式: secret/credbridge/{tenant_id}/{credential_id}
    pub fn build_path(&self, tenant_id: &str, credential_id: &str) -> String {
        format!("credbridge/{tenant_id}/{credential_id}")
    }
}

/// Vault 客户端封装
///
/// 提供对 HashiCorp Vault KV v2 引擎的安全访问
/// 仅 TEE Enclave 持有 Vault Token
pub struct VaultKvClient {
    /// Vault 客户端
    client: VaultClient,
    /// 配置
    config: VaultConfig,
}

impl VaultKvClient {
    /// 创建新的 Vault 客户端
    pub async fn new(config: VaultConfig) -> Result<Self, VaultClientError> {
        config.validate()?;

        let client_settings = VaultClientSettingsBuilder::default()
            .address(&config.addr)
            .token(&config.token)
            .timeout(Some(Duration::from_secs(config.timeout_seconds)))
            .build()
            .map_err(|e| VaultClientError::ConfigError(e.to_string()))?;

        let client = VaultClient::new(client_settings)
            .map_err(|e| VaultClientError::ConnectionFailed(e.to_string()))?;

        // 验证连接
        Self::verify_connection(&client).await?;

        Ok(Self { client, config })
    }

    /// 从环境变量创建客户端
    pub async fn from_env() -> Result<Self, VaultClientError> {
        let config = VaultConfig::from_env()?;
        Self::new(config).await
    }

    /// 验证 Vault 连接
    async fn verify_connection(client: &VaultClient) -> Result<(), VaultClientError> {
        // 尝试一个简单的操作来验证连接
        // 这里使用 vaultrs::sys::health 或类似的轻量级 API
        match vaultrs::sys::health(client).await {
            Ok(_) => Ok(()),
            Err(e) => Err(VaultClientError::ConnectionFailed(e.to_string())),
        }
    }

    /// 初始化 KV v2 引擎
    ///
    /// 如果引擎不存在，则创建它
    pub async fn init_kv_engine(&self) -> Result<(), VaultClientError> {
        use vaultrs::sys::mount;

        // 检查引擎是否已挂载
        let mounts = mount::list(&self.client)
            .await
            .map_err(|e| VaultClientError::ConfigError(format!("Failed to list mounts: {e}")))?;

        let mount_path = &self.config.mount_path;

        if !mounts.contains_key(&format!("{mount_path}/")) {
            // 创建 KV v2 引擎
            mount::enable(&self.client, mount_path, "kv-v2", None)
                .await
                .map_err(|e| {
                    VaultClientError::ConfigError(format!("Failed to enable KV v2: {e}"))
                })?;
        }

        Ok(())
    }

    /// 写入凭证密文到 Vault KV v2
    ///
    /// # Arguments
    /// * `tenant_id` - 租户 ID
    /// * `credential_id` - 凭证 ID
    /// * `data` - 要存储的数据（已加密的密文）
    ///
    /// # Returns
    /// * `Ok(SecretVersionMetadata)` - 版本元数据
    pub async fn write_secret(
        &self,
        tenant_id: &str,
        credential_id: &str,
        data: &serde_json::Value,
    ) -> Result<SecretVersionMetadata, VaultClientError> {
        use vaultrs::kv2;

        let path = self.config.build_path(tenant_id, credential_id);

        kv2::set(&self.client, &self.config.mount_path, &path, data)
            .await
            .map_err(|e| VaultClientError::WriteFailed(e.to_string()))
    }

    /// 从 Vault KV v2 读取凭证密文
    ///
    /// # Arguments
    /// * `tenant_id` - 租户 ID
    /// * `credential_id` - 凭证 ID
    ///
    /// # Returns
    /// * `Ok(serde_json::Value)` - 存储的数据（加密的密文）
    /// * `Err(VaultClientError::SecretNotFound)` - 密钥不存在
    pub async fn read_secret(
        &self,
        tenant_id: &str,
        credential_id: &str,
    ) -> Result<serde_json::Value, VaultClientError> {
        use vaultrs::kv2;

        let path = self.config.build_path(tenant_id, credential_id);

        let secret: Result<serde_json::Value, _> =
            kv2::read(&self.client, &self.config.mount_path, &path).await;

        match secret {
            Ok(data) => Ok(data),
            Err(vaultrs::error::ClientError::APIError { code: 404, .. }) => {
                Err(VaultClientError::SecretNotFound(format!(
                    "Credential {credential_id} not found for tenant {tenant_id}"
                )))
            }
            Err(e) => Err(VaultClientError::ReadFailed(e.to_string())),
        }
    }

    /// 删除凭证密文（软删除）
    ///
    /// KV v2 默认是软删除，可以恢复
    pub async fn delete_secret(
        &self,
        tenant_id: &str,
        credential_id: &str,
    ) -> Result<(), VaultClientError> {
        use vaultrs::kv2;

        let path = self.config.build_path(tenant_id, credential_id);

        // 使用 delete_latest 删除最新版本
        kv2::delete_latest(&self.client, &self.config.mount_path, &path)
            .await
            .map_err(|e| VaultClientError::DeleteFailed(format!("{e:?}")))
    }

    /// 物理删除凭证密文
    ///
    /// 永久删除，不可恢复
    pub async fn destroy_secret(
        &self,
        tenant_id: &str,
        credential_id: &str,
        _versions: &[u32],
    ) -> Result<(), VaultClientError> {
        use vaultrs::kv2;

        let path = self.config.build_path(tenant_id, credential_id);

        // 删除元数据（永久删除）
        kv2::delete_metadata(&self.client, &self.config.mount_path, &path)
            .await
            .map_err(|e| VaultClientError::DeleteFailed(format!("{e:?}")))
    }

    /// 恢复软删除的凭证
    pub async fn undelete_secret(
        &self,
        _tenant_id: &str,
        _credential_id: &str,
        _versions: &[u32],
    ) -> Result<(), VaultClientError> {
        // 注意：vaultrs 0.7 版本的 undelete API 可能不同
        // 这里暂时返回未实现错误
        Err(VaultClientError::WriteFailed(
            "Undelete not implemented in current vaultrs version".to_string(),
        ))
    }

    /// 列出租户的所有凭证
    ///
    /// # Arguments
    /// * `tenant_id` - 租户 ID
    ///
    /// # Returns
    /// * `Ok(Vec<String>)` - 凭证 ID 列表
    pub async fn list_secrets(&self, tenant_id: &str) -> Result<Vec<String>, VaultClientError> {
        use vaultrs::kv2;

        let prefix = format!("credbridge/{tenant_id}");

        let keys = kv2::list(&self.client, &self.config.mount_path, &prefix)
            .await
            .map_err(|e| VaultClientError::ReadFailed(e.to_string()))?;

        Ok(keys)
    }

    /// 获取客户端配置
    pub fn config(&self) -> &VaultConfig {
        &self.config
    }

    /// 获取 Vault 地址
    pub fn addr(&self) -> &str {
        &self.config.addr
    }

    /// 获取挂载路径
    pub fn mount_path(&self) -> &str {
        &self.config.mount_path
    }
}

/// 存储在 Vault 中的凭证数据格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultCredentialData {
    /// 凭证 ID
    pub credential_id: String,

    /// 租户 ID
    pub tenant_id: String,

    /// 用户 ID（哈希）
    pub user_id_hash: String,

    /// 服务 ID
    pub service_id: String,

    /// 凭证类型
    pub credential_type: String,

    /// 创建时间戳
    pub created_at: u64,

    /// 过期时间戳（可选）
    pub expires_at: Option<u64>,

    /// 加密载荷（Base64 编码）
    /// 这是 TEE 内 AES-256-GCM 加密后的密文
    /// Vault 再进行一次 AES-256 加密（双重加密）
    pub encrypted_payload: String,

    /// 协议版本
    pub version: u8,

    /// 加密算法
    pub algorithm: String,

    /// KDF 算法
    pub kdf: String,

    /// Nonce（Base64 编码）
    pub nonce: String,

    /// Auth Tag（Base64 编码）
    pub auth_tag: String,

    /// 是否已删除
    pub is_deleted: bool,

    /// 最后更新时间戳（可选，Unix 秒）
    #[serde(default)]
    pub updated_at: Option<u64>,
}

/// Vault 凭证数据构建器
///
/// 用于构建 VaultCredentialData 的 Builder 模式实现
pub struct VaultCredentialDataBuilder {
    credential_id: String,
    tenant_id: String,
    user_id_hash: String,
    service_id: String,
    credential_type: String,
    created_at: u64,
    expires_at: Option<u64>,
    encrypted_payload: String,
    version: u8,
    algorithm: String,
    kdf: String,
    nonce: String,
    auth_tag: String,
}

impl VaultCredentialDataBuilder {
    /// 创建新的构建器（必需字段）
    pub fn new(
        credential_id: String,
        tenant_id: String,
        user_id_hash: String,
        service_id: String,
        credential_type: String,
    ) -> Self {
        Self {
            credential_id,
            tenant_id,
            user_id_hash,
            service_id,
            credential_type,
            created_at: 0,
            expires_at: None,
            encrypted_payload: String::new(),
            version: 1,
            algorithm: String::new(),
            kdf: String::new(),
            nonce: String::new(),
            auth_tag: String::new(),
        }
    }

    /// 设置创建时间
    pub fn created_at(mut self, created_at: u64) -> Self {
        self.created_at = created_at;
        self
    }

    /// 设置过期时间
    pub fn expires_at(mut self, expires_at: Option<u64>) -> Self {
        self.expires_at = expires_at;
        self
    }

    /// 设置加密载荷
    pub fn encrypted_payload(mut self, encrypted_payload: String) -> Self {
        self.encrypted_payload = encrypted_payload;
        self
    }

    /// 设置版本
    pub fn version(mut self, version: u8) -> Self {
        self.version = version;
        self
    }

    /// 设置算法
    pub fn algorithm(mut self, algorithm: String) -> Self {
        self.algorithm = algorithm;
        self
    }

    /// 设置 KDF
    pub fn kdf(mut self, kdf: String) -> Self {
        self.kdf = kdf;
        self
    }

    /// 设置 Nonce
    pub fn nonce(mut self, nonce: String) -> Self {
        self.nonce = nonce;
        self
    }

    /// 设置 Auth Tag
    pub fn auth_tag(mut self, auth_tag: String) -> Self {
        self.auth_tag = auth_tag;
        self
    }

    /// 构建 VaultCredentialData
    pub fn build(self) -> VaultCredentialData {
        VaultCredentialData {
            credential_id: self.credential_id,
            tenant_id: self.tenant_id,
            user_id_hash: self.user_id_hash,
            service_id: self.service_id,
            credential_type: self.credential_type,
            created_at: self.created_at,
            expires_at: self.expires_at,
            encrypted_payload: self.encrypted_payload,
            version: self.version,
            algorithm: self.algorithm,
            kdf: self.kdf,
            nonce: self.nonce,
            auth_tag: self.auth_tag,
            is_deleted: false,
            updated_at: None,
        }
    }
}

impl VaultCredentialData {
    /// 创建新的 Vault 凭证数据
    ///
    /// 推荐使用 VaultCredentialDataBuilder 来构建复杂实例
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        credential_id: String,
        tenant_id: String,
        user_id_hash: String,
        service_id: String,
        credential_type: String,
        created_at: u64,
        expires_at: Option<u64>,
        encrypted_payload: String,
        version: u8,
        algorithm: String,
        kdf: String,
        nonce: String,
        auth_tag: String,
    ) -> Self {
        Self {
            credential_id,
            tenant_id,
            user_id_hash,
            service_id,
            credential_type,
            created_at,
            expires_at,
            encrypted_payload,
            version,
            algorithm,
            kdf,
            nonce,
            auth_tag,
            is_deleted: false,
            updated_at: None,
        }
    }

    /// 创建构建器（必需字段）
    pub fn builder(
        credential_id: String,
        tenant_id: String,
        user_id_hash: String,
        service_id: String,
        credential_type: String,
    ) -> VaultCredentialDataBuilder {
        VaultCredentialDataBuilder::new(
            credential_id,
            tenant_id,
            user_id_hash,
            service_id,
            credential_type,
        )
    }

    /// 转换为 JSON Value
    pub fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    /// 从 JSON Value 解析
    pub fn from_json(value: serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_config_build_path() {
        let config = VaultConfig {
            addr: "http://localhost:8200".to_string(),
            token: "test-token".to_string(),
            mount_path: "secret".to_string(),
            ..Default::default()
        };

        let path = config.build_path("tenant_123", "cred_456");
        assert_eq!(path, "credbridge/tenant_123/cred_456");
    }

    #[test]
    fn test_vault_config_validation() {
        let valid = VaultConfig {
            addr: "http://localhost:8200".to_string(),
            token: "test-token".to_string(),
            mount_path: "secret".to_string(),
            ..Default::default()
        };
        assert!(valid.validate().is_ok());

        let invalid_addr = VaultConfig {
            addr: "".to_string(),
            token: "test".to_string(),
            mount_path: "secret".to_string(),
            ..Default::default()
        };
        assert!(invalid_addr.validate().is_err());

        let invalid_token = VaultConfig {
            addr: "http://localhost:8200".to_string(),
            token: "".to_string(),
            mount_path: "secret".to_string(),
            ..Default::default()
        };
        assert!(invalid_token.validate().is_err());
    }

    #[test]
    fn test_vault_credential_data_serialization() {
        let data = VaultCredentialData::new(
            "cred_123".to_string(),
            "tenant_456".to_string(),
            "user_hash".to_string(),
            "schwab".to_string(),
            "username_password".to_string(),
            1234567890,
            None,
            "base64_ciphertext".to_string(),
            2,
            "AES-256-GCM".to_string(),
            "HKDF-SHA-256".to_string(),
            "base64_nonce".to_string(),
            "base64_auth_tag".to_string(),
        );

        let json = data.to_json().unwrap();
        let parsed = VaultCredentialData::from_json(json).unwrap();

        assert_eq!(parsed.credential_id, data.credential_id);
        assert_eq!(parsed.tenant_id, data.tenant_id);
        assert_eq!(parsed.encrypted_payload, data.encrypted_payload);
    }
}
