//! 密钥管理与缓存模块
//!
//! 实现安全的密钥缓存机制，支持：
//! - TTL 自动过期清理
//! - Zeroize 安全清理
//! - Enclave 重启后的密钥恢复
//!
//! # 安全特性
//!
//! - 所有敏感密钥材料使用 `ZeroizeOnDrop` 自动清零
//! - 缓存项超过 TTL 后自动移除并清零
//! - Enclave 重启时从密封存储恢复 L1 主密钥
//! - 验证 MRSIGNER 兼容性防止未授权 Enclave 访问

use crate::crypto::constants::KEY_LENGTH;
use crate::crypto::{CryptoError, KeyHandle};
use crate::tee::sealing::{SealPolicy, SealedStorage, SealingService};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// 默认用户密钥缓存 TTL
pub const DEFAULT_KEY_TTL_SECONDS: u64 = 300; // 5 分钟

/// 主密钥存储标识
pub const MASTER_KEY_STORAGE_ID: &str = "enclave_master_key";

/// L1 主密钥密封元数据
#[derive(Debug, Clone)]
pub struct MasterKeyMetadata {
    /// 密封策略
    pub seal_policy: SealPolicy,

    /// 创建时的 MRSIGNER
    pub sealed_mrsigner: [u8; 32],

    /// 创建时的 MRENCLAVE
    pub sealed_mrenclave: [u8; 32],

    /// 创建时间戳
    pub created_at: u64,

    /// 密钥版本
    pub version: u32,
}

impl MasterKeyMetadata {
    /// 创建新的元数据
    pub fn new(
        seal_policy: SealPolicy,
        mrsigner: [u8; 32],
        mrenclave: [u8; 32],
        version: u32,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            seal_policy,
            sealed_mrsigner: mrsigner,
            sealed_mrenclave: mrenclave,
            created_at: now,
            version,
        }
    }

    /// 序列化为字节
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut result = Vec::with_capacity(128);

        // 版本 (4 bytes)
        result.extend_from_slice(&self.version.to_le_bytes());

        // 策略 (1 byte)
        result.push(match self.seal_policy {
            SealPolicy::Mrenclave => 0u8,
            SealPolicy::Mrsigner => 1u8,
        });

        // MRSIGNER (32 bytes)
        result.extend_from_slice(&self.sealed_mrsigner);

        // MRENCLAVE (32 bytes)
        result.extend_from_slice(&self.sealed_mrenclave);

        // 创建时间 (8 bytes)
        result.extend_from_slice(&self.created_at.to_le_bytes());

        result
    }

    /// 从字节反序列化
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, KeyManagerError> {
        if bytes.len() < 77 {
            return Err(KeyManagerError::InvalidMetadata);
        }

        let mut offset = 0;

        // 版本
        let version = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]);
        offset += 4;

        // 策略
        let seal_policy = match bytes[offset] {
            0 => SealPolicy::Mrenclave,
            1 => SealPolicy::Mrsigner,
            _ => return Err(KeyManagerError::InvalidMetadata),
        };
        offset += 1;

        // MRSIGNER
        let mut sealed_mrsigner = [0u8; 32];
        sealed_mrsigner.copy_from_slice(&bytes[offset..offset + 32]);
        offset += 32;

        // MRENCLAVE
        let mut sealed_mrenclave = [0u8; 32];
        sealed_mrenclave.copy_from_slice(&bytes[offset..offset + 32]);
        offset += 32;

        // 创建时间
        let created_at = u64::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]);

        Ok(Self {
            seal_policy,
            sealed_mrsigner,
            sealed_mrenclave,
            created_at,
            version,
        })
    }

    /// 验证 MRSIGNER 兼容性
    ///
    /// # 安全说明
    ///
    /// - MRSIGNER 模式：验证签名者是否相同（允许同一签名者的不同版本）
    /// - MRENCLAVE 模式：验证 Enclave 身份完全相同（更严格）
    pub fn verify_compatibility(
        &self,
        current_mrsigner: &[u8; 32],
        current_mrenclave: &[u8; 32],
    ) -> Result<(), KeyManagerError> {
        match self.seal_policy {
            SealPolicy::Mrsigner => {
                // MRSIGNER 模式：只需验证签名者相同
                if &self.sealed_mrsigner != current_mrsigner {
                    return Err(KeyManagerError::MrsignerMismatch);
                }
            }
            SealPolicy::Mrenclave => {
                // MRENCLAVE 模式：需要完全相同的测量值
                if &self.sealed_mrenclave != current_mrenclave {
                    return Err(KeyManagerError::MrenclaveMismatch);
                }
            }
        }
        Ok(())
    }
}

/// 受保护的密钥材料
///
/// 使用 `ZeroizeOnDrop` 确保密钥材料在 drop 时被安全清零
#[derive(ZeroizeOnDrop)]
pub struct ProtectedKeyMaterial {
    /// 密钥材料（32字节）
    #[zeroize]
    pub material: [u8; KEY_LENGTH],

    /// 密钥类型
    #[zeroize(skip)]
    pub key_type: KeyType,

    /// 创建时间
    #[zeroize(skip)]
    pub created_at: Instant,
}

impl ProtectedKeyMaterial {
    /// 创建新的受保护密钥材料
    pub fn new(material: [u8; KEY_LENGTH], key_type: KeyType) -> Self {
        Self {
            material,
            key_type,
            created_at: Instant::now(),
        }
    }

    /// 获取密钥材料引用
    pub fn as_bytes(&self) -> &[u8; KEY_LENGTH] {
        &self.material
    }

    /// 获取密钥类型
    pub fn key_type(&self) -> KeyType {
        self.key_type
    }
}

/// 密钥类型枚举
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KeyType {
    /// L0: 硬件根密钥
    HardwareRoot,
    /// L1: Enclave 主密钥
    EnclaveMaster,
    /// L2: 用户保险库密钥
    UserVault,
    /// L3: 凭证加密密钥
    Credential,
}

/// 缓存的用户密钥条目
///
/// 包含密钥句柄和元数据，实际密钥材料存储在单独的受保护区域
pub struct CachedKeyEntry {
    /// 密钥句柄
    pub handle: KeyHandle,

    /// 租户ID
    pub tenant_id: String,

    /// 用户ID哈希
    pub user_id_hash: String,

    /// 创建时间
    pub created_at: Instant,

    /// 最后访问时间
    pub last_accessed_at: Instant,

    /// 访问计数
    pub access_count: u64,

    /// 密钥材料（受保护，自动清零）
    pub key_material: Arc<Mutex<ProtectedKeyMaterial>>,
}

impl CachedKeyEntry {
    /// 创建新的缓存条目
    pub fn new(
        handle: KeyHandle,
        tenant_id: String,
        user_id_hash: String,
        key_material: [u8; KEY_LENGTH],
        key_type: KeyType,
    ) -> Self {
        let now = Instant::now();
        let protected = ProtectedKeyMaterial::new(key_material, key_type);

        Self {
            handle,
            tenant_id,
            user_id_hash,
            created_at: now,
            last_accessed_at: now,
            access_count: 0,
            key_material: Arc::new(Mutex::new(protected)),
        }
    }

    /// 更新访问时间
    pub fn touch(&mut self) {
        self.last_accessed_at = Instant::now();
        self.access_count += 1;
    }

    /// 检查是否过期
    pub fn is_expired(&self, ttl: Duration) -> bool {
        self.last_accessed_at.elapsed() > ttl
    }

    /// 安全清理密钥材料
    ///
    /// 手动触发密钥材料的 zeroize 清理
    pub fn secure_clear(&self) {
        if let Ok(mut material) = self.key_material.lock() {
            material.material.zeroize();
        }
    }
}

/// 用户密钥缓存
///
/// 管理 L2 用户密钥的缓存，支持 TTL 自动过期
pub struct UserKeyCache {
    /// 缓存条目
    entries: HashMap<String, CachedKeyEntry>,

    /// TTL 配置
    ttl: Duration,

    /// 缓存统计
    stats: CacheStatistics,
}

impl UserKeyCache {
    /// 创建新的缓存
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            entries: HashMap::new(),
            ttl: Duration::from_secs(ttl_seconds),
            stats: CacheStatistics::default(),
        }
    }

    /// 获取缓存条目
    pub fn get(&mut self, tenant_id: &str, user_id_hash: &str) -> Option<&CachedKeyEntry> {
        let key = format!("{}:{}", tenant_id, user_id_hash);

        if let Some(entry) = self.entries.get(&key) {
            if entry.is_expired(self.ttl) {
                // 条目已过期，移除并清理
                self.remove(&key);
                return None;
            }

            // 更新访问时间
            let entry = self.entries.get_mut(&key).unwrap();
            entry.touch();
            self.stats.hits += 1;

            return Some(entry);
        }

        self.stats.misses += 1;
        None
    }

    /// 插入缓存条目
    pub fn insert(&mut self, tenant_id: &str, user_id_hash: &str, entry: CachedKeyEntry) {
        let key = format!("{}:{}", tenant_id, user_id_hash);

        // 如果已存在，先清理旧的
        if self.entries.contains_key(&key) {
            self.remove(&key);
        }

        self.stats.insertions += 1;
        self.entries.insert(key, entry);
    }

    /// 移除缓存条目并清理密钥材料
    pub fn remove(&mut self, cache_key: &str) -> Option<CachedKeyEntry> {
        if let Some(entry) = self.entries.remove(cache_key) {
            // 安全清理密钥材料
            entry.secure_clear();
            self.stats.removals += 1;
            return Some(entry);
        }
        None
    }

    /// 清理所有过期条目
    ///
    /// 返回清理的条目数量
    pub fn cleanup_expired(&mut self) -> usize {
        let expired_keys: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.is_expired(self.ttl))
            .map(|(k, _)| k.clone())
            .collect();

        let count = expired_keys.len();

        for key in expired_keys {
            self.remove(&key);
        }

        self.stats.cleanups += count as u64;
        count
    }

    /// 清理所有条目
    ///
    /// 用于 Enclave 关闭时彻底清理
    pub fn clear(&mut self) {
        for entry in self.entries.values() {
            entry.secure_clear();
        }
        self.entries.clear();
        self.stats.clears += 1;
    }

    /// 获取缓存大小
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 检查缓存是否为空
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 获取统计信息
    pub fn stats(&self) -> &CacheStatistics {
        &self.stats
    }

    /// 获取 TTL
    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    /// 设置 TTL
    pub fn set_ttl(&mut self, ttl_seconds: u64) {
        self.ttl = Duration::from_secs(ttl_seconds);
    }
}

impl Drop for UserKeyCache {
    fn drop(&mut self) {
        // 确保在 drop 时清理所有密钥材料
        self.clear();
    }
}

/// 缓存统计信息
#[derive(Debug, Clone, Default)]
pub struct CacheStatistics {
    /// 缓存命中次数
    pub hits: u64,

    /// 缓存未命中次数
    pub misses: u64,

    /// 插入次数
    pub insertions: u64,

    /// 移除次数
    pub removals: u64,

    /// 清理次数
    pub cleanups: u64,

    /// 清空次数
    pub clears: u64,
}

/// 密钥管理器错误
#[derive(Debug)]
pub enum KeyManagerError {
    /// 密钥未找到
    KeyNotFound,

    /// 缓存已满
    CacheFull,

    /// 密封/解封失败
    SealingFailed(String),

    /// MRSIGNER 不匹配
    MrsignerMismatch,

    /// MRENCLAVE 不匹配
    MrenclaveMismatch,

    /// 无效的元数据
    InvalidMetadata,

    /// 存储错误
    StorageError(String),

    /// 内部错误
    InternalError(String),
}

impl std::fmt::Display for KeyManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyManagerError::KeyNotFound => write!(f, "Key not found in cache"),
            KeyManagerError::CacheFull => write!(f, "Key cache is full"),
            KeyManagerError::SealingFailed(msg) => write!(f, "Sealing failed: {}", msg),
            KeyManagerError::MrsignerMismatch => {
                write!(f, "MRSIGNER mismatch - incompatible enclave")
            }
            KeyManagerError::MrenclaveMismatch => {
                write!(f, "MRENCLAVE mismatch - enclave identity changed")
            }
            KeyManagerError::InvalidMetadata => write!(f, "Invalid key metadata"),
            KeyManagerError::StorageError(msg) => write!(f, "Storage error: {}", msg),
            KeyManagerError::InternalError(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for KeyManagerError {}

impl From<CryptoError> for KeyManagerError {
    fn from(e: CryptoError) -> Self {
        KeyManagerError::InternalError(e.to_string())
    }
}

/// 密钥管理器
///
/// 管理 L1 Enclave 主密钥的持久化和恢复
pub struct KeyManager {
    /// 密封存储
    storage: SealedStorage,

    /// 密封服务
    sealing_service: SealingService,

    /// 当前 MRSIGNER
    current_mrsigner: [u8; 32],

    /// 当前 MRENCLAVE
    current_mrenclave: [u8; 32],
}

impl KeyManager {
    /// 创建新的密钥管理器
    pub fn new(storage_path: String, mrsigner: [u8; 32], mrenclave: [u8; 32]) -> Self {
        let storage = SealedStorage::new(storage_path);
        let sealing_service = SealingService::new();

        Self {
            storage,
            sealing_service,
            current_mrsigner: mrsigner,
            current_mrenclave: mrenclave,
        }
    }

    /// 密封并存储 L1 主密钥
    ///
    /// # 流程
    /// 1. 创建元数据（包含 MRSIGNER/MRENCLAVE）
    /// 2. 将元数据和密钥材料一起密封
    /// 3. 存储到持久化存储
    pub fn seal_master_key(
        &self,
        master_key_material: &[u8; KEY_LENGTH],
        seal_policy: SealPolicy,
    ) -> Result<(), KeyManagerError> {
        // 创建元数据
        let metadata = MasterKeyMetadata::new(
            seal_policy,
            self.current_mrsigner,
            self.current_mrenclave,
            1, // 版本 1
        );

        // 组合元数据和密钥材料
        let mut plaintext = metadata.to_bytes();
        plaintext.extend_from_slice(master_key_material);

        // 密封数据
        let sealed = self
            .sealing_service
            .seal_data(&plaintext, MASTER_KEY_STORAGE_ID.as_bytes(), seal_policy)
            .map_err(|e| KeyManagerError::SealingFailed(e.to_string()))?;

        // 存储
        self.storage
            .store(MASTER_KEY_STORAGE_ID, &sealed)
            .map_err(|e| KeyManagerError::StorageError(e.to_string()))?;

        Ok(())
    }

    /// 从密封存储恢复 L1 主密钥
    ///
    /// # 流程
    /// 1. 检查密封存储中是否存在主密钥
    /// 2. 解封数据
    /// 3. 验证 MRSIGNER/MRENCLAVE 兼容性
    /// 4. 提取密钥材料
    ///
    /// # 安全说明
    ///
    /// 如果 MRSIGNER 不匹配，返回错误（防止未授权 Enclave 访问）
    pub fn restore_master_key(
        &self,
    ) -> Result<([u8; KEY_LENGTH], MasterKeyMetadata), KeyManagerError> {
        // 检查是否存在
        if !self.storage.exists(MASTER_KEY_STORAGE_ID) {
            return Err(KeyManagerError::KeyNotFound);
        }

        // 加载密封数据
        let sealed = self
            .storage
            .load(MASTER_KEY_STORAGE_ID)
            .map_err(|e| KeyManagerError::StorageError(e.to_string()))?;

        // 解封
        let plaintext = self
            .sealing_service
            .unseal_data(&sealed)
            .map_err(|e| KeyManagerError::SealingFailed(e.to_string()))?;

        if plaintext.len() < 77 + KEY_LENGTH {
            return Err(KeyManagerError::InvalidMetadata);
        }

        // 分离元数据和密钥材料
        let metadata_bytes = &plaintext[..77];
        let key_bytes = &plaintext[77..77 + KEY_LENGTH];

        // 解析元数据
        let metadata = MasterKeyMetadata::from_bytes(metadata_bytes)?;

        // 验证兼容性
        metadata.verify_compatibility(&self.current_mrsigner, &self.current_mrenclave)?;

        // 提取密钥材料
        let mut master_key = [0u8; KEY_LENGTH];
        master_key.copy_from_slice(key_bytes);

        Ok((master_key, metadata))
    }

    /// 检查是否存在密封的主密钥
    pub fn has_sealed_master_key(&self) -> bool {
        self.storage.exists(MASTER_KEY_STORAGE_ID)
    }

    /// 删除密封的主密钥
    pub fn delete_sealed_master_key(&self) -> Result<(), KeyManagerError> {
        self.storage
            .delete(MASTER_KEY_STORAGE_ID)
            .map_err(|e| KeyManagerError::StorageError(e.to_string()))
    }

    /// 获取当前 MRSIGNER
    pub fn current_mrsigner(&self) -> &[u8; 32] {
        &self.current_mrsigner
    }

    /// 获取当前 MRENCLAVE
    pub fn current_mrenclave(&self) -> &[u8; 32] {
        &self.current_mrenclave
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_protected_key_material_zeroize() {
        let material = [0x42u8; KEY_LENGTH];
        let key_type = KeyType::UserVault;

        {
            let protected = ProtectedKeyMaterial::new(material, key_type);
            assert_eq!(protected.as_bytes(), &material);
            assert_eq!(protected.key_type(), key_type);
        }

        // 离开作用域后，material 应该被 zeroize
        // 注意：由于 ZeroizeOnDrop 是自动的，这里无法直接测试
        // 但可以通过内存调试工具验证
    }

    #[test]
    fn test_master_key_metadata_serialization() {
        let mrsigner = [0x01u8; 32];
        let mrenclave = [0x02u8; 32];
        let metadata = MasterKeyMetadata::new(SealPolicy::Mrsigner, mrsigner, mrenclave, 1);

        let bytes = metadata.to_bytes();
        let restored = MasterKeyMetadata::from_bytes(&bytes).unwrap();

        assert_eq!(restored.seal_policy, metadata.seal_policy);
        assert_eq!(restored.sealed_mrsigner, metadata.sealed_mrsigner);
        assert_eq!(restored.sealed_mrenclave, metadata.sealed_mrenclave);
        assert_eq!(restored.version, metadata.version);
    }

    #[test]
    fn test_mrsigner_verification() {
        let mrsigner1 = [0x01u8; 32];
        let mrsigner2 = [0x02u8; 32];
        let mrenclave = [0x03u8; 32];

        // MRSIGNER 模式：签名者相同即可
        let metadata = MasterKeyMetadata::new(SealPolicy::Mrsigner, mrsigner1, mrenclave, 1);

        // 相同 MRSIGNER - 应该通过
        assert!(
            metadata
                .verify_compatibility(&mrsigner1, &[0xFFu8; 32])
                .is_ok()
        );

        // 不同 MRSIGNER - 应该失败
        assert!(matches!(
            metadata
                .verify_compatibility(&mrsigner2, &[0xFFu8; 32])
                .unwrap_err(),
            KeyManagerError::MrsignerMismatch
        ));
    }

    #[test]
    fn test_mrenclave_verification() {
        let mrsigner = [0x01u8; 32];
        let mrenclave1 = [0x02u8; 32];
        let mrenclave2 = [0x03u8; 32];

        // MRENCLAVE 模式：需要完全相同的测量值
        let metadata = MasterKeyMetadata::new(SealPolicy::Mrenclave, mrsigner, mrenclave1, 1);

        // 相同 MRENCLAVE - 应该通过
        assert!(
            metadata
                .verify_compatibility(&mrsigner, &mrenclave1)
                .is_ok()
        );

        // 不同 MRENCLAVE - 应该失败
        assert!(matches!(
            metadata
                .verify_compatibility(&mrsigner, &mrenclave2)
                .unwrap_err(),
            KeyManagerError::MrenclaveMismatch
        ));
    }

    #[test]
    fn test_user_key_cache_operations() {
        let mut cache = UserKeyCache::new(60); // 60秒 TTL

        let handle = [0u8; 32];
        let key_material = [0x42u8; KEY_LENGTH];
        let entry = CachedKeyEntry::new(
            handle,
            "tenant_1".to_string(),
            "user_hash_1".to_string(),
            key_material,
            KeyType::UserVault,
        );

        // 插入
        cache.insert("tenant_1", "user_hash_1", entry);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.stats().insertions, 1);

        // 获取
        let retrieved = cache.get("tenant_1", "user_hash_1");
        assert!(retrieved.is_some());
        assert_eq!(cache.stats().hits, 1);

        // 获取不存在的
        let not_found = cache.get("tenant_1", "user_hash_2");
        assert!(not_found.is_none());
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn test_user_key_cache_expiration() {
        let mut cache = UserKeyCache::new(1); // 1秒 TTL

        let handle = [0u8; 32];
        let key_material = [0x42u8; KEY_LENGTH];
        let entry = CachedKeyEntry::new(
            handle,
            "tenant_1".to_string(),
            "user_hash_1".to_string(),
            key_material,
            KeyType::UserVault,
        );

        cache.insert("tenant_1", "user_hash_1", entry);
        assert_eq!(cache.len(), 1);

        // 立即获取 - 应该存在
        assert!(cache.get("tenant_1", "user_hash_1").is_some());

        // 等待过期
        thread::sleep(Duration::from_secs(2));

        // 现在获取应该失败（会被自动清理）
        assert!(cache.get("tenant_1", "user_hash_1").is_none());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_user_key_cache_cleanup() {
        let mut cache = UserKeyCache::new(1); // 1秒 TTL

        // 添加多个条目
        for i in 0..3 {
            let entry = CachedKeyEntry::new(
                [i as u8; 32],
                format!("tenant_{}", i),
                format!("user_{}", i),
                [i as u8; KEY_LENGTH],
                KeyType::UserVault,
            );
            cache.insert(&format!("tenant_{}", i), &format!("user_{}", i), entry);
        }

        assert_eq!(cache.len(), 3);

        // 等待过期
        thread::sleep(Duration::from_secs(2));

        // 手动清理
        let cleaned = cache.cleanup_expired();
        assert_eq!(cleaned, 3);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_cache_clear() {
        let mut cache = UserKeyCache::new(60);

        // 添加条目
        for i in 0..3 {
            let entry = CachedKeyEntry::new(
                [i as u8; 32],
                format!("tenant_{}", i),
                format!("user_{}", i),
                [i as u8; KEY_LENGTH],
                KeyType::UserVault,
            );
            cache.insert(&format!("tenant_{}", i), &format!("user_{}", i), entry);
        }

        assert_eq!(cache.len(), 3);

        // 清空
        cache.clear();
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.stats().clears, 1);
    }
}
