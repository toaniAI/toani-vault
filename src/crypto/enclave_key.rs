//! Enclave 密钥管理器
//!
//! 实现 Enclave 级别的密钥生命周期管理（生成、轮换、签名、验证）
//! 密钥仅存储在 TEE 密封存储中

use crate::crypto::CryptoError;
use crate::tee::sealing::{SealPolicy, SealedStorage};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use zeroize::ZeroizeOnDrop;

/// 密钥状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyState {
    /// 活跃状态（可用于签名）
    Active,
    /// 仅解密状态（新签名使用新密钥）
    DecryptOnly,
    /// 已归档（保留用于历史数据验证）
    Archived,
    /// 已撤销
    Revoked,
}

impl std::fmt::Display for KeyState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyState::Active => write!(f, "active"),
            KeyState::DecryptOnly => write!(f, "decrypt_only"),
            KeyState::Archived => write!(f, "archived"),
            KeyState::Revoked => write!(f, "revoked"),
        }
    }
}

/// Enclave 密钥
#[derive(Debug, Clone, ZeroizeOnDrop, Serialize, Deserialize)]
pub struct EnclaveKey {
    /// 密钥 ID
    #[zeroize(skip)]
    pub key_id: String,
    /// 密钥对（签名密钥）
    pub key_pair: SigningKeyPair,
    /// 创建时间
    #[zeroize(skip)]
    pub created_at: OffsetDateTime,
    /// 过期时间
    #[zeroize(skip)]
    pub expires_at: OffsetDateTime,
    /// 密钥状态
    #[zeroize(skip)]
    pub state: KeyState,
    /// 密钥版本
    #[zeroize(skip)]
    pub version: u32,
}

/// 签名密钥对
#[derive(Debug, Clone, ZeroizeOnDrop, Serialize, Deserialize)]
pub struct SigningKeyPair {
    /// 私钥（仅用于签名）
    #[zeroize]
    pub private_key: Vec<u8>,
    /// 公钥（用于验证）
    pub public_key: Vec<u8>,
    /// 算法标识
    pub algorithm: String,
}

/// 数字签名
#[derive(Debug, Clone)]
pub struct Signature {
    /// 签名数据
    pub data: Vec<u8>,
    /// 签名算法
    pub algorithm: String,
    /// 使用的密钥 ID
    pub key_id: String,
    /// 签名时间
    pub timestamp: OffsetDateTime,
    /// 公钥（用于验证）
    pub public_key: Vec<u8>,
}

/// 密钥配置
#[derive(Debug, Clone)]
pub struct KeyConfig {
    /// 密钥算法
    pub algorithm: KeyAlgorithm,
    /// 密钥有效期（天）
    pub key_validity_days: u32,
    /// 保留的历史密钥数量
    pub max_historical_keys: usize,
    /// 自动轮换间隔（天）
    pub rotation_interval_days: Option<u32>,
    /// 密封策略
    pub seal_policy: SealPolicy,
}

impl Default for KeyConfig {
    fn default() -> Self {
        Self {
            algorithm: KeyAlgorithm::Ed25519,
            key_validity_days: 90,
            max_historical_keys: 5,
            rotation_interval_days: Some(90),
            seal_policy: SealPolicy::Mrsigner,
        }
    }
}

/// 密钥算法
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAlgorithm {
    /// Ed25519
    Ed25519,
    /// ECDSA P-256
    EcdsaP256,
}

impl std::fmt::Display for KeyAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyAlgorithm::Ed25519 => write!(f, "Ed25519"),
            KeyAlgorithm::EcdsaP256 => write!(f, "ECDSA-P256"),
        }
    }
}

/// 密钥错误
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("密钥未找到: {0}")]
    KeyNotFound(String),
    #[error("密钥已过期: {0}")]
    KeyExpired(String),
    #[error("密钥状态错误: {key_id}, 当前状态: {state}")]
    InvalidState { key_id: String, state: KeyState },
    #[error("密钥生成失败: {0}")]
    GenerationFailed(String),
    #[error("签名失败: {0}")]
    SignFailed(String),
    #[error("验证失败: {0}")]
    VerifyFailed(String),
    #[error("密封存储错误: {0}")]
    StorageError(String),
    #[error("已达到最大历史密钥数")]
    MaxHistoricalKeysReached,
}

impl From<CryptoError> for KeyError {
    fn from(err: CryptoError) -> Self {
        KeyError::GenerationFailed(err.to_string())
    }
}

/// Enclave 密钥管理器
///
/// 管理 Enclave 级别的签名密钥，支持密钥轮换和历史密钥验证。
/// 所有密钥仅存储在 TEE 密封存储中。
pub struct EnclaveKeyManager {
    /// 密封存储
    storage: Arc<SealedStorage>,
    /// 当前活跃密钥
    current_key: Arc<RwLock<Option<EnclaveKey>>>,
    /// 历史密钥（用于验证旧签名）
    historical_keys: Arc<RwLock<VecDeque<EnclaveKey>>>,
    /// 配置
    config: KeyConfig,
}

impl EnclaveKeyManager {
    /// 创建新的密钥管理器
    pub fn new(storage: Arc<SealedStorage>, config: KeyConfig) -> Self {
        Self {
            storage,
            current_key: Arc::new(RwLock::new(None)),
            historical_keys: Arc::new(RwLock::new(VecDeque::new())),
            config,
        }
    }

    /// 初始化密钥管理器
    ///
    /// 从密封存储恢复密钥，如果没有则生成新密钥
    pub async fn initialize(&mut self) -> Result<(), KeyError> {
        info!("初始化 Enclave 密钥管理器");

        // 尝试从密封存储加载当前密钥
        match self.load_current_key().await {
            Ok(key) => {
                info!("已从密封存储恢复密钥: {}", key.key_id);
                *self.current_key.write().await = Some(key);
            }
            Err(_) => {
                info!("未找到现有密钥，生成新密钥");
                self.generate_key().await?;
            }
        }

        // 加载历史密钥
        self.load_historical_keys().await?;

        Ok(())
    }

    /// 生成新密钥
    ///
    /// 如果存在当前密钥，将其移至历史密钥列表
    pub async fn generate_key(&mut self) -> Result<EnclaveKey, KeyError> {
        info!("生成新的 Enclave 签名密钥");

        // 先生成新密钥（不持有锁）
        let key_id = format!("enclave-key-{}", uuid::Uuid::new_v4());
        let key_pair = self.generate_key_pair()?;

        let now = OffsetDateTime::now_utc();
        let expires_at = now + time::Duration::days(self.config.key_validity_days as i64);
        let version = self.get_next_version().await;

        let new_key = EnclaveKey {
            key_id: key_id.clone(),
            key_pair,
            created_at: now,
            expires_at,
            state: KeyState::Active,
            version,
        };

        // 保存到密封存储
        self.save_key(&new_key).await?;

        // 如果有当前密钥，先将其转为历史密钥
        let old_key = {
            let mut current = self.current_key.write().await;
            current.take()
        };

        if let Some(old_key) = old_key {
            self.archive_key(old_key).await?;
        }

        // 设置新密钥
        {
            let mut current = self.current_key.write().await;
            *current = Some(new_key.clone());
        }

        info!("新密钥已生成并保存: {}", key_id);
        Ok(new_key)
    }

    /// 轮换密钥
    ///
    /// 生成新密钥并将当前密钥标记为仅解密状态
    pub async fn rotate_key(&mut self) -> Result<(), KeyError> {
        info!("开始密钥轮换");

        // 将当前密钥标记为仅解密
        let mut current = self.current_key.write().await;
        if let Some(ref mut key) = *current {
            key.state = KeyState::DecryptOnly;
            self.save_key(key).await?;
        }
        drop(current);

        // 生成新密钥
        self.generate_key().await?;

        info!("密钥轮换完成");
        Ok(())
    }

    /// 签名数据
    ///
    /// 使用当前活跃密钥对数据进行签名
    pub async fn sign(&self, data: &[u8]) -> Result<Signature, KeyError> {
        let current = self.current_key.read().await;
        let key = current
            .as_ref()
            .ok_or(KeyError::KeyNotFound("没有活跃密钥".to_string()))?;

        // 检查密钥状态
        if key.state != KeyState::Active {
            return Err(KeyError::InvalidState {
                key_id: key.key_id.clone(),
                state: key.state,
            });
        }

        // 检查密钥是否过期
        if OffsetDateTime::now_utc() > key.expires_at {
            return Err(KeyError::KeyExpired(key.key_id.clone()));
        }

        // 执行签名
        let signature_data = self.perform_sign(data, &key.key_pair)?;

        Ok(Signature {
            data: signature_data,
            algorithm: key.key_pair.algorithm.clone(),
            key_id: key.key_id.clone(),
            timestamp: OffsetDateTime::now_utc(),
            public_key: key.key_pair.public_key.clone(),
        })
    }

    /// 验证签名
    ///
    /// 使用指定的密钥验证签名。如果未指定密钥，尝试使用所有可用密钥验证。
    pub async fn verify(&self, data: &[u8], signature: &Signature) -> Result<bool, KeyError> {
        // 首先尝试用当前密钥验证
        let current = self.current_key.read().await;
        if let Some(ref key) = *current {
            if key.key_id == signature.key_id {
                return self.perform_verify(data, signature, &key.key_pair.public_key);
            }
        }
        drop(current);

        // 尝试用历史密钥验证
        let historical = self.historical_keys.read().await;
        for key in historical.iter() {
            if key.key_id == signature.key_id {
                return self.perform_verify(data, signature, &key.key_pair.public_key);
            }
        }

        Err(KeyError::KeyNotFound(signature.key_id.clone()))
    }

    /// 获取当前密钥信息
    pub async fn current_key(&self) -> Option<EnclaveKey> {
        self.current_key.read().await.clone()
    }

    /// 获取历史密钥列表
    pub async fn historical_keys(&self) -> Vec<EnclaveKey> {
        self.historical_keys.read().await.iter().cloned().collect()
    }

    /// 获取密钥数量统计
    pub async fn key_stats(&self) -> KeyStats {
        KeyStats {
            current: if self.current_key.read().await.is_some() {
                1
            } else {
                0
            },
            historical: self.historical_keys.read().await.len(),
        }
    }

    /// 紧急撤销密钥
    pub async fn revoke_key(&mut self, key_id: &str) -> Result<(), KeyError> {
        info!("撤销密钥: {}", key_id);

        // 检查是否是当前密钥
        let mut current = self.current_key.write().await;
        if let Some(ref mut key) = *current {
            if key.key_id == key_id {
                key.state = KeyState::Revoked;
                self.save_key(key).await?;
                *current = None;
                return Ok(());
            }
        }
        drop(current);

        // 检查历史密钥
        let mut historical = self.historical_keys.write().await;
        if let Some(pos) = historical.iter().position(|k| k.key_id == key_id) {
            if let Some(mut key) = historical.remove(pos) {
                key.state = KeyState::Revoked;
                self.save_key(&key).await?;
            }
        }

        Ok(())
    }

    // ========== 内部方法 ==========

    /// 生成密钥对
    fn generate_key_pair(&self) -> Result<SigningKeyPair, KeyError> {
        match self.config.algorithm {
            KeyAlgorithm::Ed25519 => self.generate_ed25519_key_pair(),
            KeyAlgorithm::EcdsaP256 => Err(KeyError::GenerationFailed(
                "ECDSA-P256 not yet implemented".to_string(),
            )),
        }
    }

    /// 生成 Ed25519 密钥对
    fn generate_ed25519_key_pair(&self) -> Result<SigningKeyPair, KeyError> {
        use ed25519_dalek::SigningKey;
        use rand::rngs::OsRng;

        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        Ok(SigningKeyPair {
            private_key: signing_key.to_bytes().to_vec(),
            public_key: verifying_key.to_bytes().to_vec(),
            algorithm: "Ed25519".to_string(),
        })
    }

    /// 执行签名
    fn perform_sign(&self, data: &[u8], key_pair: &SigningKeyPair) -> Result<Vec<u8>, KeyError> {
        match key_pair.algorithm.as_str() {
            "Ed25519" => self.sign_ed25519(data, key_pair),
            _ => Err(KeyError::SignFailed(format!(
                "不支持的算法: {}",
                key_pair.algorithm
            ))),
        }
    }

    /// Ed25519 签名
    fn sign_ed25519(&self, data: &[u8], key_pair: &SigningKeyPair) -> Result<Vec<u8>, KeyError> {
        use ed25519_dalek::{Signer, SigningKey};

        let secret_key: [u8; 32] = key_pair
            .private_key
            .as_slice()
            .try_into()
            .map_err(|_| KeyError::SignFailed("Invalid private key length".to_string()))?;

        let signing_key = SigningKey::from_bytes(&secret_key);

        let signature = signing_key.sign(data);
        Ok(signature.to_bytes().to_vec())
    }

    /// 执行验证
    fn perform_verify(
        &self,
        data: &[u8],
        signature: &Signature,
        public_key: &[u8],
    ) -> Result<bool, KeyError> {
        match signature.algorithm.as_str() {
            "Ed25519" => self.verify_ed25519(data, signature, public_key),
            _ => Err(KeyError::VerifyFailed(format!(
                "不支持的算法: {}",
                signature.algorithm
            ))),
        }
    }

    /// Ed25519 验证
    fn verify_ed25519(
        &self,
        data: &[u8],
        signature: &Signature,
        public_key: &[u8],
    ) -> Result<bool, KeyError> {
        use ed25519_dalek::{Signature as EdSignature, Verifier, VerifyingKey};

        let public_key_bytes: [u8; 32] = public_key
            .try_into()
            .map_err(|_| KeyError::VerifyFailed("Invalid public key length".to_string()))?;

        let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
            .map_err(|e| KeyError::VerifyFailed(e.to_string()))?;

        let sig_bytes: [u8; 64] = signature
            .data
            .as_slice()
            .try_into()
            .map_err(|_| KeyError::VerifyFailed("Invalid signature length".to_string()))?;

        let ed_sig = EdSignature::from_bytes(&sig_bytes);

        match verifying_key.verify(data, &ed_sig) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// 保存密钥到密封存储
    async fn save_key(&self, key: &EnclaveKey) -> Result<(), KeyError> {
        // 保存密钥数据
        let key_data =
            serde_json::to_vec(key).map_err(|e| KeyError::StorageError(e.to_string()))?;

        let sealed = self
            .storage
            .sealing()
            .seal_data(&key_data, key.key_id.as_bytes(), self.config.seal_policy)
            .map_err(|e| KeyError::StorageError(e.to_string()))?;

        self.storage
            .store(&format!("key_{}", key.key_id), &sealed)
            .map_err(|e| KeyError::StorageError(e.to_string()))?;

        // 更新密钥索引
        let mut index = self.load_key_index().await.unwrap_or_default();

        match key.state {
            KeyState::Active => {
                // 如果是新密钥，将旧密钥移到历史列表
                if let Some(old_key_id) = index.current_key_id.replace(key.key_id.clone()) {
                    if !index.historical_key_ids.contains(&old_key_id) {
                        index.historical_key_ids.insert(0, old_key_id);
                    }
                }
            }
            KeyState::DecryptOnly | KeyState::Archived => {
                // 添加到历史列表
                if !index.historical_key_ids.contains(&key.key_id) {
                    index.historical_key_ids.push(key.key_id.clone());
                }
            }
            KeyState::Revoked => {
                // 添加到撤销列表
                if !index.revoked_key_ids.contains(&key.key_id) {
                    index.revoked_key_ids.push(key.key_id.clone());
                }
                // 从其他列表移除
                index.historical_key_ids.retain(|id| id != &key.key_id);
                if index.current_key_id.as_ref() == Some(&key.key_id) {
                    index.current_key_id = None;
                }
            }
        }

        // 限制历史密钥数量
        while index.historical_key_ids.len() > self.config.max_historical_keys {
            index.historical_key_ids.pop();
        }

        self.save_key_index(&index).await?;

        Ok(())
    }

    /// 从密封存储加载当前密钥
    ///
    /// 通过密钥索引找到当前活跃密钥并加载
    async fn load_current_key(&self) -> Result<EnclaveKey, KeyError> {
        debug!("从密封存储加载当前密钥");

        // 加载密钥索引
        let index = self.load_key_index().await?;

        // 获取当前密钥ID
        let key_id = index
            .current_key_id
            .ok_or_else(|| KeyError::KeyNotFound("没有当前密钥".to_string()))?;

        // 加载并解封密钥
        let key = self.load_key_by_id(&key_id).await?;

        // 验证密钥状态为Active
        if key.state != KeyState::Active {
            return Err(KeyError::InvalidState {
                key_id: key.key_id.clone(),
                state: key.state,
            });
        }

        info!("成功加载当前密钥: {}", key_id);
        Ok(key)
    }

    /// 加载历史密钥
    ///
    /// 加载所有非当前密钥到历史密钥列表
    async fn load_historical_keys(&self) -> Result<(), KeyError> {
        debug!("加载历史密钥");

        let index = self.load_key_index().await?;
        let mut historical = self.historical_keys.write().await;
        historical.clear();

        for key_id in &index.historical_key_ids {
            match self.load_key_by_id(key_id).await {
                Ok(key) => {
                    historical.push_back(key);
                }
                Err(e) => {
                    warn!("加载历史密钥 {} 失败: {}", key_id, e);
                    // 继续加载其他密钥
                }
            }
        }

        info!("已加载 {} 个历史密钥", historical.len());
        Ok(())
    }

    /// 根据ID加载密钥
    async fn load_key_by_id(&self, key_id: &str) -> Result<EnclaveKey, KeyError> {
        let storage_key = format!("key_{}", key_id);

        let sealed_data = self
            .storage
            .load(&storage_key)
            .map_err(|e| KeyError::StorageError(format!("加载密钥失败: {}", e)))?;

        let key_data = self
            .storage
            .sealing()
            .unseal_data(&sealed_data)
            .map_err(|e| KeyError::StorageError(format!("解封密钥失败: {}", e)))?;

        let key: EnclaveKey = serde_json::from_slice(&key_data)
            .map_err(|e| KeyError::StorageError(format!("解析密钥失败: {}", e)))?;

        Ok(key)
    }

    /// 加载密钥索引
    async fn load_key_index(&self) -> Result<KeyIndex, KeyError> {
        match self.storage.load("key_index") {
            Ok(sealed_data) => {
                let data = self
                    .storage
                    .sealing()
                    .unseal_data(&sealed_data)
                    .map_err(|e| KeyError::StorageError(format!("解封密钥索引失败: {}", e)))?;

                let index: KeyIndex = serde_json::from_slice(&data)
                    .map_err(|e| KeyError::StorageError(format!("解析密钥索引失败: {}", e)))?;

                Ok(index)
            }
            Err(_) => {
                // 索引不存在，返回空索引
                Ok(KeyIndex::default())
            }
        }
    }

    /// 保存密钥索引
    async fn save_key_index(&self, index: &KeyIndex) -> Result<(), KeyError> {
        let data = serde_json::to_vec(index)
            .map_err(|e| KeyError::StorageError(format!("序列化密钥索引失败: {}", e)))?;

        let sealed = self
            .storage
            .sealing()
            .seal_data(&data, b"key_index", self.config.seal_policy)
            .map_err(|e| KeyError::StorageError(format!("密封密钥索引失败: {}", e)))?;

        self.storage
            .store("key_index", &sealed)
            .map_err(|e| KeyError::StorageError(format!("存储密钥索引失败: {}", e)))?;

        Ok(())
    }

    /// 归档密钥
    async fn archive_key(&self, key: EnclaveKey) -> Result<(), KeyError> {
        let mut historical = self.historical_keys.write().await;

        // 检查是否达到最大历史密钥数
        if historical.len() >= self.config.max_historical_keys {
            historical.pop_back();
        }

        historical.push_front(key);
        Ok(())
    }

    /// 获取下一个版本号
    async fn get_next_version(&self) -> u32 {
        let current = self.current_key.read().await;
        current.as_ref().map(|k| k.version + 1).unwrap_or(1)
    }
}

/// 密钥统计
#[derive(Debug, Clone)]
pub struct KeyStats {
    /// 当前密钥数量
    pub current: usize,
    /// 历史密钥数量
    pub historical: usize,
}

/// 密钥索引
///
/// 用于快速查找和管理密钥的索引结构
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct KeyIndex {
    /// 当前活跃密钥ID
    pub current_key_id: Option<String>,
    /// 历史密钥ID列表（按时间倒序）
    pub historical_key_ids: Vec<String>,
    /// 已撤销密钥ID列表
    pub revoked_key_ids: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_test_storage() -> Arc<SealedStorage> {
        let temp_dir =
            std::env::temp_dir().join(format!("credbridge_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        Arc::new(SealedStorage::new(temp_dir.to_string_lossy().to_string()))
    }

    #[tokio::test]
    async fn test_key_manager_creation() {
        let storage = create_test_storage();
        let config = KeyConfig::default();
        let manager = EnclaveKeyManager::new(storage, config);

        let stats = manager.key_stats().await;
        assert_eq!(stats.current, 0);
        assert_eq!(stats.historical, 0);
    }

    #[tokio::test]
    async fn test_generate_key() {
        let storage = create_test_storage();
        let config = KeyConfig::default();
        let mut manager = EnclaveKeyManager::new(storage, config);

        let key = manager.generate_key().await.unwrap();
        assert_eq!(key.state, KeyState::Active);
        assert!(key.version > 0);

        let stats = manager.key_stats().await;
        assert_eq!(stats.current, 1);
    }

    #[tokio::test]
    async fn test_sign_and_verify() {
        let storage = create_test_storage();
        let config = KeyConfig::default();
        let mut manager = EnclaveKeyManager::new(storage, config);

        // 生成密钥
        manager.generate_key().await.unwrap();

        // 签名
        let data = b"test data to sign";
        let signature = manager.sign(data).await.unwrap();
        assert_eq!(signature.algorithm, "Ed25519");

        // 验证
        let valid = manager.verify(data, &signature).await.unwrap();
        assert!(valid);

        // 验证错误的签名
        let invalid = manager.verify(b"wrong data", &signature).await.unwrap();
        assert!(!invalid);
    }

    #[tokio::test]
    async fn test_key_rotation() {
        let storage = create_test_storage();
        let config = KeyConfig::default();
        let mut manager = EnclaveKeyManager::new(storage, config);

        // 生成初始密钥
        let key1 = manager.generate_key().await.unwrap();
        let key_id1 = key1.key_id.clone();

        // 轮换密钥
        manager.rotate_key().await.unwrap();

        let key2 = manager.current_key().await.unwrap();
        assert_ne!(key2.key_id, key_id1);
        assert_eq!(key2.version, 2);

        let stats = manager.key_stats().await;
        assert_eq!(stats.current, 1);
        assert_eq!(stats.historical, 1);
    }

    #[test]
    fn test_key_state_display() {
        assert_eq!(KeyState::Active.to_string(), "active");
        assert_eq!(KeyState::DecryptOnly.to_string(), "decrypt_only");
    }

    #[test]
    fn test_key_algorithm_display() {
        assert_eq!(KeyAlgorithm::Ed25519.to_string(), "Ed25519");
        assert_eq!(KeyAlgorithm::EcdsaP256.to_string(), "ECDSA-P256");
    }
}
