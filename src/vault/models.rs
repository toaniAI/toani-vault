//! 凭证数据模型与存储结构
//!
//! 实现 EP2-Story2.1 凭证数据模型与存储
//! - UUID v7 作为凭证 ID（时间排序）
//! - 多租户隔离（Schema-per-Tenant + RLS）
//! - AES-256-GCM 加密载荷

use crate::crypto::constants;
use crate::models::{CredentialMetadata, CredentialType, StoredCredential};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use uuid::Uuid;

/// Vault 模块错误类型
#[derive(Error, Debug)]
pub enum VaultError {
    #[error("无效的凭证ID: {0}")]
    InvalidCredentialId(String),

    #[error("凭证未找到: {0}")]
    CredentialNotFound(String),

    #[error("租户隔离违规: 用户 {user_id} 无法访问租户 {tenant_id}")]
    TenantIsolationViolation { user_id: String, tenant_id: String },

    #[error("存储错误: {0}")]
    StorageError(String),

    #[error("序列化错误: {0}")]
    SerializationError(String),

    #[error("加密错误: {0}")]
    EncryptionError(String),

    #[error("无效的版本号: {message}")]
    InvalidVersion { message: String },

    #[error("凭证 {credential_id} 的版本 {version} 未找到")]
    VersionNotFound { credential_id: String, version: u32 },
}

/// 凭证 ID（基于 UUID v7）
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CredentialId(pub String);

impl CredentialId {
    /// 生成新的凭证 ID（UUID v7）
    ///
    /// UUID v7 特性：
    /// - 时间排序（前 48-bit 为 Unix 时间戳）
    /// - 更好的数据库索引性能
    /// - 无需全局协调即可生成唯一 ID
    pub fn new() -> Self {
        let uuid = Uuid::now_v7();
        Self(uuid.to_string())
    }

    /// 从字符串创建凭证 ID
    pub fn from_string(id: String) -> Result<Self, VaultError> {
        // 验证 UUID 格式
        match Uuid::parse_str(&id) {
            Ok(_) => Ok(Self(id)),
            Err(_) => Err(VaultError::InvalidCredentialId(id)),
        }
    }

    /// 获取 ID 字符串
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for CredentialId {
    fn default() -> Self {
        Self::new()
    }
}

/// 用户 ID（存储时哈希）
#[derive(Debug, Clone, Eq, Hash, Serialize, Deserialize)]
pub struct UserId {
    /// 原始用户 ID（仅在内存中，不序列化）
    #[serde(skip)]
    raw: String,
    /// 哈希后的用户 ID（用于存储和查询）
    hash: String,
}

impl PartialEq for UserId {
    fn eq(&self, other: &Self) -> bool {
        // 只比较哈希值，不比较原始值
        self.hash == other.hash
    }
}

impl UserId {
    /// 创建用户 ID（自动计算哈希）
    pub fn new(user_id: impl Into<String>) -> Self {
        let raw: String = user_id.into();
        let hash = Self::compute_hash(&raw);
        Self { raw, hash }
    }

    /// 从哈希值创建（用于从数据库恢复）
    pub fn from_hash(hash: impl Into<String>) -> Self {
        let hash = hash.into();
        Self {
            raw: String::new(), // 无法恢复原始值
            hash,
        }
    }

    /// 计算用户 ID 哈希（SHA-256）
    fn compute_hash(raw: &str) -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        let digest = ring::digest::digest(&ring::digest::SHA256, raw.as_bytes());
        URL_SAFE_NO_PAD.encode(digest.as_ref())
    }

    /// 获取哈希值（用于存储和查询）
    pub fn hash(&self) -> &str {
        &self.hash
    }

    /// 获取原始用户 ID（需要权限验证）
    pub fn raw(&self) -> Option<&str> {
        if self.raw.is_empty() {
            None
        } else {
            Some(&self.raw)
        }
    }

    /// 验证原始用户 ID 是否匹配哈希
    pub fn verify(&self, raw_user_id: &str) -> bool {
        Self::compute_hash(raw_user_id) == self.hash
    }
}

impl From<String> for UserId {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for UserId {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

/// 租户 ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantId(pub String);

impl TenantId {
    /// 创建租户 ID
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// 获取 ID 字符串
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for TenantId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for TenantId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// 服务 ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ServiceId(pub String);

impl ServiceId {
    /// 创建服务 ID
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// 获取 ID 字符串
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for ServiceId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ServiceId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// 加密载荷结构（符合 EP2-Story2.1 规范）
///
/// encrypted_payload 包含：
/// - version = 2
/// - algorithm = 'AES-256-GCM'
/// - kdf = 'HKDF-SHA-256'
/// - nonce（12 bytes）
/// - auth_tag（16 bytes）
/// - ciphertext
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedPayload {
    /// 协议版本（固定为 2）
    pub version: u8,

    /// 加密算法（固定为 'AES-256-GCM'）
    pub algorithm: String,

    /// KDF 算法（固定为 'HKDF-SHA-256'）
    pub kdf: String,

    /// Nonce（96-bit = 12 bytes, base64 encoded）
    pub nonce: String,

    /// Auth Tag（128-bit = 16 bytes, base64 encoded）
    pub auth_tag: String,

    /// 密文（base64 encoded）
    pub ciphertext: String,
}

impl EncryptedPayload {
    /// 创建新的加密载荷
    pub fn new(
        version: u8,
        algorithm: impl Into<String>,
        kdf: impl Into<String>,
        nonce: Vec<u8>,
        auth_tag: Vec<u8>,
        ciphertext: Vec<u8>,
    ) -> Self {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

        Self {
            version,
            algorithm: algorithm.into(),
            kdf: kdf.into(),
            nonce: URL_SAFE_NO_PAD.encode(&nonce),
            auth_tag: URL_SAFE_NO_PAD.encode(&auth_tag),
            ciphertext: URL_SAFE_NO_PAD.encode(&ciphertext),
        }
    }

    /// 从 crypto 模块的 EncryptedBlob 转换
    pub fn from_blob(blob: &crate::crypto::EncryptedBlob) -> Self {
        Self {
            version: blob.version,
            algorithm: blob.algorithm.clone(),
            kdf: blob.kdf.clone(),
            nonce: blob.nonce.clone(),
            auth_tag: blob.auth_tag.clone(),
            ciphertext: blob.ciphertext.clone(),
        }
    }

    /// 获取 nonce 字节
    pub fn nonce_bytes(&self) -> Result<Vec<u8>, VaultError> {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        URL_SAFE_NO_PAD
            .decode(&self.nonce)
            .map_err(|e| VaultError::SerializationError(format!("nonce decode failed: {}", e)))
    }

    /// 获取 auth_tag 字节
    pub fn auth_tag_bytes(&self) -> Result<Vec<u8>, VaultError> {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        URL_SAFE_NO_PAD
            .decode(&self.auth_tag)
            .map_err(|e| VaultError::SerializationError(format!("auth_tag decode failed: {}", e)))
    }

    /// 获取密文字节
    pub fn ciphertext_bytes(&self) -> Result<Vec<u8>, VaultError> {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        URL_SAFE_NO_PAD
            .decode(&self.ciphertext)
            .map_err(|e| VaultError::SerializationError(format!("ciphertext decode failed: {}", e)))
    }

    /// 验证格式是否符合规范
    pub fn validate(&self) -> Result<(), VaultError> {
        // 验证版本
        if self.version != constants::PROTOCOL_VERSION {
            return Err(VaultError::EncryptionError(format!(
                "unsupported version: {}, expected {}",
                self.version,
                constants::PROTOCOL_VERSION
            )));
        }

        // 验证算法
        if self.algorithm != constants::ALGORITHM_AES_256_GCM {
            return Err(VaultError::EncryptionError(format!(
                "unsupported algorithm: {}",
                self.algorithm
            )));
        }

        // 验证 KDF
        if self.kdf != constants::KDF_HKDF_SHA256 {
            return Err(VaultError::EncryptionError(format!(
                "unsupported kdf: {}",
                self.kdf
            )));
        }

        // 验证 nonce 长度
        let nonce = self.nonce_bytes()?;
        if nonce.len() != constants::NONCE_LENGTH {
            return Err(VaultError::EncryptionError(format!(
                "invalid nonce length: {}, expected {}",
                nonce.len(),
                constants::NONCE_LENGTH
            )));
        }

        // 验证 auth_tag 长度
        let auth_tag = self.auth_tag_bytes()?;
        if auth_tag.len() != constants::AUTH_TAG_LENGTH {
            return Err(VaultError::EncryptionError(format!(
                "invalid auth_tag length: {}, expected {}",
                auth_tag.len(),
                constants::AUTH_TAG_LENGTH
            )));
        }

        Ok(())
    }
}

impl Default for EncryptedPayload {
    fn default() -> Self {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

        Self {
            version: constants::PROTOCOL_VERSION,
            algorithm: constants::ALGORITHM_AES_256_GCM.to_string(),
            kdf: constants::KDF_HKDF_SHA256.to_string(),
            nonce: URL_SAFE_NO_PAD.encode(&[0u8; constants::NONCE_LENGTH]),
            auth_tag: URL_SAFE_NO_PAD.encode(&[0u8; constants::AUTH_TAG_LENGTH]),
            ciphertext: String::new(),
        }
    }
}

/// Vault 凭证记录（完整存储格式）
///
/// 符合多租户隔离架构（Schema-per-Tenant + RLS）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultEntry {
    /// 凭证唯一 ID（UUID v7）
    pub credential_id: CredentialId,

    /// 当前版本号（从 1 开始递增）
    pub version: u32,

    /// 租户 ID（多租户隔离）
    pub tenant_id: TenantId,

    /// 用户 ID（哈希后存储）
    pub user_id: UserId,

    /// 服务 ID
    pub service_id: ServiceId,

    /// 凭证类型
    pub credential_type: CredentialType,

    /// 创建时间戳（Unix 秒）
    pub created_at: u64,

    /// 更新时间戳（Unix 秒）
    pub updated_at: u64,

    /// 过期时间戳（可选，Unix 秒）
    pub expires_at: Option<u64>,

    /// 加密载荷
    pub encrypted_payload: EncryptedPayload,

    /// 是否已删除（软删除）
    pub is_deleted: bool,
}

impl VaultEntry {
    /// 创建新的凭证条目
    pub fn new(
        tenant_id: TenantId,
        user_id: UserId,
        service_id: ServiceId,
        credential_type: CredentialType,
        encrypted_payload: EncryptedPayload,
        expires_at: Option<u64>,
    ) -> Self {
        let now = current_timestamp();

        Self {
            credential_id: CredentialId::new(),
            version: 1, // 新凭证从版本 1 开始
            tenant_id,
            user_id,
            service_id,
            credential_type,
            created_at: now,
            updated_at: now,
            expires_at,
            encrypted_payload,
            is_deleted: false,
        }
    }

    /// 创建新的凭证条目（使用预生成的 credential_id）
    ///
    /// 用于修复加密流程中 credential_id 不一致的问题：
    /// 加密时需要知道将要使用的 credential_id，以确保密钥派生参数一致
    pub fn with_credential_id(
        credential_id: CredentialId,
        tenant_id: TenantId,
        user_id: UserId,
        service_id: ServiceId,
        credential_type: CredentialType,
        encrypted_payload: EncryptedPayload,
        expires_at: Option<u64>,
    ) -> Self {
        let now = current_timestamp();

        Self {
            credential_id,
            version: 1, // 新凭证从版本 1 开始
            tenant_id,
            user_id,
            service_id,
            credential_type,
            created_at: now,
            updated_at: now,
            expires_at,
            encrypted_payload,
            is_deleted: false,
        }
    }

    /// 增加版本号并更新时间戳
    pub fn increment_version(&mut self) {
        self.version += 1;
        self.updated_at = current_timestamp();
    }

    /// 检查凭证是否过期
    pub fn is_expired(&self) -> bool {
        match self.expires_at {
            Some(exp) => current_timestamp() > exp,
            None => false,
        }
    }

    /// 获取元数据（不含加密载荷）
    pub fn metadata(&self) -> CredentialMetadata {
        CredentialMetadata {
            credential_id: self.credential_id.as_str().to_string(),
            credential_type: self.credential_type.clone(),
            user_id_hash: self.user_id.hash().to_string(),
            service_id: self.service_id.as_str().to_string(),
            tenant_id: self.tenant_id.as_str().to_string(),
            created_at: timestamp_to_iso8601(self.created_at),
            expires_at: self.expires_at.map(timestamp_to_iso8601),
            is_deleted: self.is_deleted,
            version: self.version,
        }
    }

    /// 验证租户访问权限
    pub fn verify_tenant_access(&self, tenant_id: &TenantId, user_id: &UserId) -> Result<(), VaultError> {
        // 验证租户匹配
        if self.tenant_id != *tenant_id {
            return Err(VaultError::TenantIsolationViolation {
                user_id: user_id.hash().to_string(),
                tenant_id: tenant_id.as_str().to_string(),
            });
        }

        // 验证用户匹配
        if self.user_id != *user_id {
            return Err(VaultError::TenantIsolationViolation {
                user_id: user_id.hash().to_string(),
                tenant_id: tenant_id.as_str().to_string(),
            });
        }

        Ok(())
    }

    /// 标记为已删除
    pub fn mark_deleted(&mut self) {
        self.is_deleted = true;
        self.updated_at = current_timestamp();
    }

    /// 转换为存储格式（用于旧版兼容）
    pub fn to_stored_credential(&self) -> StoredCredential {
        StoredCredential {
            metadata: self.metadata(),
            encrypted_payload: serde_json::to_string(&self.encrypted_payload).unwrap_or_default(),
            version: self.encrypted_payload.version,
            algorithm: self.encrypted_payload.algorithm.clone(),
            kdf: self.encrypted_payload.kdf.clone(),
        }
    }
}

/// 凭证查询过滤器
#[derive(Debug, Clone, Default)]
pub struct CredentialFilter {
    /// 按服务 ID 过滤
    pub service_id: Option<ServiceId>,

    /// 按凭证类型过滤
    pub credential_type: Option<CredentialType>,

    /// 包含已删除的凭证
    pub include_deleted: bool,

    /// 仅返回未过期的
    pub only_valid: bool,
}

/// 凭证查询结果
#[derive(Debug, Clone)]
pub struct CredentialQueryResult {
    /// 凭证元数据列表（不含加密载荷）
    pub credentials: Vec<CredentialMetadata>,

    /// 总数（用于分页）
    pub total: usize,
}

/// 凭证创建请求
#[derive(Debug, Clone)]
pub struct CreateCredentialRequest {
    /// 租户 ID
    pub tenant_id: TenantId,

    /// 用户 ID
    pub user_id: UserId,

    /// 服务 ID
    pub service_id: ServiceId,

    /// 凭证类型
    pub credential_type: CredentialType,

    /// 过期时间（可选）
    pub expires_at: Option<u64>,
}

/// 获取当前 Unix 时间戳（秒）
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time before Unix epoch")
        .as_secs()
}

/// 将 Unix 时间戳转换为 ISO 8601 格式
fn timestamp_to_iso8601(timestamp: u64) -> String {
    use chrono::{DateTime, Utc};
    let datetime = DateTime::from_timestamp(timestamp as i64, 0)
        .unwrap_or_else(|| Utc::now());
    datetime.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credential_id_generation() {
        let id1 = CredentialId::new();
        let id2 = CredentialId::new();

        // 两个 ID 应该不同
        assert_ne!(id1.as_str(), id2.as_str());

        // 验证 UUID 格式
        assert!(Uuid::parse_str(id1.as_str()).is_ok());
        assert!(Uuid::parse_str(id2.as_str()).is_ok());
    }

    #[test]
    fn test_credential_id_from_string() {
        let valid_uuid = "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c";
        let id = CredentialId::from_string(valid_uuid.to_string());
        assert!(id.is_ok());
        assert_eq!(id.unwrap().as_str(), valid_uuid);

        let invalid = "not-a-uuid";
        let id = CredentialId::from_string(invalid.to_string());
        assert!(id.is_err());
    }

    #[test]
    fn test_user_id_hashing() {
        let user_id = UserId::new("user_123");

        // 哈希值应该存在
        assert!(!user_id.hash().is_empty());

        // 原始值应该可访问
        assert_eq!(user_id.raw(), Some("user_123"));

        // 验证应该成功
        assert!(user_id.verify("user_123"));
        assert!(!user_id.verify("wrong_user"));
    }

    #[test]
    fn test_user_id_from_hash() {
        let user_id = UserId::new("user_123");
        let hash = user_id.hash().to_string();

        let restored = UserId::from_hash(&hash);
        assert_eq!(restored.hash(), hash);
        assert!(restored.raw().is_none()); // 无法恢复原始值
    }

    #[test]
    fn test_encrypted_payload_validation() {
        // 有效的载荷
        let valid = EncryptedPayload::new(
            constants::PROTOCOL_VERSION,
            constants::ALGORITHM_AES_256_GCM,
            constants::KDF_HKDF_SHA256,
            vec![0u8; constants::NONCE_LENGTH],
            vec![0u8; constants::AUTH_TAG_LENGTH],
            vec![1, 2, 3],
        );
        assert!(valid.validate().is_ok());

        // 无效版本
        let mut invalid = valid.clone();
        invalid.version = 99;
        assert!(invalid.validate().is_err());

        // 无效算法
        let mut invalid = valid.clone();
        invalid.algorithm = "INVALID".to_string();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_vault_entry_creation() {
        let entry = VaultEntry::new(
            TenantId::new("tenant_123"),
            UserId::new("user_456"),
            ServiceId::new("service_789"),
            CredentialType::UsernamePassword,
            EncryptedPayload::default(),
            None,
        );

        // 验证 ID 已生成
        assert!(!entry.credential_id.as_str().is_empty());

        // 验证时间戳
        assert!(entry.created_at > 0);
        assert_eq!(entry.created_at, entry.updated_at);

        // 未删除
        assert!(!entry.is_deleted);

        // 未过期（无过期时间）
        assert!(!entry.is_expired());
    }

    #[test]
    fn test_vault_entry_expiration() {
        let now = current_timestamp();

        // 已过期
        let expired = VaultEntry::new(
            TenantId::new("tenant_123"),
            UserId::new("user_456"),
            ServiceId::new("service_789"),
            CredentialType::ApiKey,
            EncryptedPayload::default(),
            Some(now - 3600), // 1小时前过期
        );
        assert!(expired.is_expired());

        // 未过期
        let valid = VaultEntry::new(
            TenantId::new("tenant_123"),
            UserId::new("user_456"),
            ServiceId::new("service_789"),
            CredentialType::ApiKey,
            EncryptedPayload::default(),
            Some(now + 3600), // 1小时后过期
        );
        assert!(!valid.is_expired());
    }

    #[test]
    fn test_tenant_isolation() {
        let entry = VaultEntry::new(
            TenantId::new("tenant_123"),
            UserId::new("user_456"),
            ServiceId::new("service_789"),
            CredentialType::OAuthRefresh,
            EncryptedPayload::default(),
            None,
        );

        // 正确的租户和用户
        assert!(entry
            .verify_tenant_access(&TenantId::new("tenant_123"), &UserId::from_hash(entry.user_id.hash()))
            .is_ok());

        // 错误的租户
        assert!(entry
            .verify_tenant_access(&TenantId::new("wrong_tenant"), &UserId::from_hash(entry.user_id.hash()))
            .is_err());
    }

    #[test]
    fn test_metadata_extraction() {
        let entry = VaultEntry::new(
            TenantId::new("tenant_123"),
            UserId::new("user_456"),
            ServiceId::new("schwab"),
            CredentialType::UsernamePassword,
            EncryptedPayload::default(),
            Some(1893456000),
        );

        let metadata = entry.metadata();

        assert_eq!(metadata.credential_id, entry.credential_id.as_str());
        assert_eq!(metadata.service_id, "schwab");
        assert_eq!(metadata.tenant_id, "tenant_123");
        assert!(metadata.user_id_hash.contains("user") || !metadata.user_id_hash.is_empty());
        assert_eq!(metadata.credential_type, CredentialType::UsernamePassword);
        assert!(!metadata.is_deleted);
    }

    #[test]
    fn test_uuid_v7_sorting() {
        // 生成多个 ID 并验证它们可以按时间排序
        let mut ids: Vec<CredentialId> = (0..10).map(|_| CredentialId::new()).collect();

        // 所有 ID 应该唯一
        let unique_count: std::collections::HashSet<_> = ids.iter().map(|id| id.as_str()).collect();
        assert_eq!(unique_count.len(), ids.len());

        // 按字符串排序（UUID v7 的前 48-bit 是时间戳，字符串排序即时间排序）
        ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));

        // 验证时间顺序（第一个的时间戳应该小于等于最后一个）
        // UUID v7: time_low(32bit)-time_mid(16bit)-ver(4bit)-rand(76bit)
        let first = ids.first().unwrap().as_str();
        let last = ids.last().unwrap().as_str();
        assert!(first <= last);
    }
}
