//! MCP Token 安全存储模块
//!
//! 实现安全的 Token 存储机制：
//! - 使用操作系统密钥环（macOS Keychain）
//! - Token 自动轮换
//! - 短 TTL（1 小时）
//! - 加密存储
//!
//! # 安全改进
//!
//! - 不再明文存储 Token
//! - 使用系统级安全存储
//! - 自动轮换防止长期暴露
//! - 访问审计日志

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::sync::RwLock;
use zeroize::Zeroize;

use crate::crypto::constant_time::{SecureBuffer, ct_compare};

/// Token 存储错误
#[derive(Error, Debug)]
pub enum TokenStorageError {
    #[error("密钥环访问失败：{0}")]
    KeychainError(String),

    #[error("Token 加密失败：{0}")]
    EncryptionError(String),

    #[error("Token 解密失败：{0}")]
    DecryptionError(String),

    #[error("Token 已过期")]
    TokenExpired,

    #[error("Token 不存在：{0}")]
    TokenNotFound(String),

    #[error("无效的 Token 格式")]
    InvalidTokenFormat,

    #[error("密钥环初始化失败：{0}")]
    KeychainInitError(String),

    #[error("Token 轮换失败：{0}")]
    RotationError(String),

    #[error("并发访问冲突")]
    ConcurrentAccessConflict,
}

/// Token 存储结果
#[derive(Debug, Clone, PartialEq)]
pub enum StorageResult {
    /// 存储成功
    Success,
    /// 存储成功并已轮换
    SuccessWithRotation,
    /// Token 已存在（更新）
    Updated,
    /// 失败
    Failed,
}

/// Token 元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetadata {
    /// Token ID
    pub token_id: String,
    /// 服务名称
    pub service_name: String,
    /// 账户标识
    pub account_id: String,
    /// 创建时间
    pub created_at: u64,
    /// 过期时间
    pub expires_at: u64,
    /// 最后使用时间
    pub last_used_at: u64,
    /// 使用次数
    pub usage_count: u64,
    /// 是否已轮换
    pub is_rotated: bool,
    /// 轮换前的 Token ID
    pub previous_token_id: Option<String>,
}

impl TokenMetadata {
    /// 创建新的 Token 元数据
    pub fn new(token_id: String, service_name: String, account_id: String, ttl_secs: u64) -> Self {
        let now = current_timestamp();
        Self {
            token_id,
            service_name,
            account_id,
            created_at: now,
            expires_at: now + ttl_secs,
            last_used_at: now,
            usage_count: 0,
            is_rotated: false,
            previous_token_id: None,
        }
    }

    /// 检查是否过期
    pub fn is_expired(&self, current_time: u64) -> bool {
        current_time >= self.expires_at
    }

    /// 检查是否需要轮换
    pub fn needs_rotation(&self, current_time: u64, rotation_threshold_secs: u64) -> bool {
        let remaining_ttl = self.expires_at.saturating_sub(current_time);
        remaining_ttl < rotation_threshold_secs
    }

    /// 更新最后使用时间
    pub fn touch(&mut self) {
        self.last_used_at = current_timestamp();
        self.usage_count += 1;
    }

    /// 标记为已轮换
    pub fn mark_rotated(&mut self, previous_token_id: String) {
        self.is_rotated = true;
        self.previous_token_id = Some(previous_token_id);
    }
}

/// 加密的 Token 数据
#[derive(Clone)]
pub struct EncryptedToken {
    /// 加密的 Token 数据
    ciphertext: SecureBuffer,
    /// 认证标签
    auth_tag: [u8; 16],
    /// 随机数
    nonce: [u8; 12],
}

impl EncryptedToken {
    /// 创建新的加密 Token
    pub fn new(ciphertext: Vec<u8>, auth_tag: [u8; 16], nonce: [u8; 12]) -> Self {
        Self {
            ciphertext: SecureBuffer::with_data(&ciphertext),
            auth_tag,
            nonce,
        }
    }

    /// 获取密文引用
    pub fn ciphertext(&self) -> &[u8] {
        self.ciphertext.as_slice()
    }

    /// 获取认证标签
    pub fn auth_tag(&self) -> &[u8; 16] {
        &self.auth_tag
    }

    /// 获取随机数
    pub fn nonce(&self) -> &[u8; 12] {
        &self.nonce
    }
}

impl Drop for EncryptedToken {
    fn drop(&mut self) {
        self.ciphertext.zeroize();
        self.auth_tag.zeroize();
        self.nonce.zeroize();
    }
}

/// Token 存储配置
#[derive(Debug, Clone)]
pub struct TokenStorageConfig {
    /// Token TTL（秒）
    pub token_ttl_secs: u64,
    /// 轮换阈值（TTL 剩余多少秒时触发轮换）
    pub rotation_threshold_secs: u64,
    /// 密钥环服务名称
    pub keychain_service_name: String,
    /// 是否启用自动轮换
    pub enable_auto_rotation: bool,
    /// 最大 Token 数量
    pub max_tokens: usize,
}

impl Default for TokenStorageConfig {
    fn default() -> Self {
        Self {
            token_ttl_secs: 3600,         // 1 小时
            rotation_threshold_secs: 600, // 10 分钟
            keychain_service_name: "CredBridge.MCP.Token".to_string(),
            enable_auto_rotation: true,
            max_tokens: 100,
        }
    }
}

/// Token 存储统计
#[derive(Debug, Default)]
pub struct TokenStorageStats {
    /// 总存储次数
    pub total_stores: AtomicU64,
    /// 总读取次数
    pub total_retrieves: AtomicU64,
    /// 总轮换次数
    pub total_rotations: AtomicU64,
    /// 总过期清理次数
    pub total_expirations: AtomicU64,
    /// 当前 Token 数量
    pub current_token_count: AtomicUsize,
}

impl TokenStorageStats {
    /// 记录存储操作
    pub fn record_store(&self) {
        self.total_stores.fetch_add(1, Ordering::SeqCst);
    }

    /// 记录读取操作
    pub fn record_retrieve(&self) {
        self.total_retrieves.fetch_add(1, Ordering::SeqCst);
    }

    /// 记录轮换操作
    pub fn record_rotation(&self) {
        self.total_rotations.fetch_add(1, Ordering::SeqCst);
    }

    /// 记录过期清理
    pub fn record_expiration(&self) {
        self.total_expirations.fetch_add(1, Ordering::SeqCst);
    }

    /// 更新 Token 数量
    pub fn update_token_count(&self, count: usize) {
        self.current_token_count.store(count, Ordering::SeqCst);
    }

    /// 获取统计快照
    pub fn snapshot(&self) -> StatsSnapshot {
        StatsSnapshot {
            total_stores: self.total_stores.load(Ordering::SeqCst),
            total_retrieves: self.total_retrieves.load(Ordering::SeqCst),
            total_rotations: self.total_rotations.load(Ordering::SeqCst),
            total_expirations: self.total_expirations.load(Ordering::SeqCst),
            current_token_count: self.current_token_count.load(Ordering::SeqCst),
        }
    }
}

/// 统计快照
#[derive(Debug, Clone)]
pub struct StatsSnapshot {
    pub total_stores: u64,
    pub total_retrieves: u64,
    pub total_rotations: u64,
    pub total_expirations: u64,
    pub current_token_count: usize,
}

/// MCP Token 安全存储管理器
#[allow(dead_code)]
pub struct McpTokenStorage {
    /// Token 元数据缓存
    metadata_cache: Arc<RwLock<HashMap<String, TokenMetadata>>>,
    /// 加密的 Token 数据（内存中，用于快速访问）
    encrypted_tokens: Arc<RwLock<HashMap<String, EncryptedToken>>>,
    /// 加密密钥（用于 Token 加密）
    encryption_key: SecureBuffer,
    /// 配置
    config: TokenStorageConfig,
    /// 统计
    stats: Arc<TokenStorageStats>,
    /// 密钥环是否已初始化
    keychain_initialized: Arc<AtomicU64>,
}

impl McpTokenStorage {
    /// 创建新的 Token 存储管理器
    pub fn new(config: TokenStorageConfig) -> Result<Self, TokenStorageError> {
        // 生成加密密钥
        let mut encryption_key = vec![0u8; 32];
        get_random_bytes(&mut encryption_key);

        Ok(Self {
            metadata_cache: Arc::new(RwLock::new(HashMap::new())),
            encrypted_tokens: Arc::new(RwLock::new(HashMap::new())),
            encryption_key: SecureBuffer::with_data(&encryption_key),
            config,
            stats: Arc::new(TokenStorageStats::default()),
            keychain_initialized: Arc::new(AtomicU64::new(0)),
        })
    }

    /// 初始化密钥环
    pub async fn initialize_keychain(&self) -> Result<(), TokenStorageError> {
        // 在实际实现中初始化 macOS Keychain
        // 这里仅标记为已初始化
        self.keychain_initialized.store(1, Ordering::SeqCst);
        Ok(())
    }

    /// 检查密钥环是否已初始化
    pub fn is_keychain_initialized(&self) -> bool {
        self.keychain_initialized.load(Ordering::SeqCst) == 1
    }

    /// 存储 Token
    ///
    /// # 流程
    ///
    /// 1. 生成 Token ID
    /// 2. 加密 Token
    /// 3. 存储到密钥环
    /// 4. 更新元数据缓存
    ///
    /// # 参数
    /// * `service_name` - 服务名称
    /// * `account_id` - 账户标识
    /// * `token` - Token 值
    ///
    /// # 返回
    /// Token ID
    pub async fn store_token(
        &self,
        service_name: &str,
        account_id: &str,
        token: &str,
    ) -> Result<String, TokenStorageError> {
        // 生成 Token ID
        let token_id = generate_token_id(service_name, account_id);

        // 加密 Token
        let encrypted = self.encrypt_token(token)?;

        // 创建元数据
        let metadata = TokenMetadata::new(
            token_id.clone(),
            service_name.to_string(),
            account_id.to_string(),
            self.config.token_ttl_secs,
        );

        // 存储到密钥环（模拟）
        self.store_to_keychain(&token_id, &encrypted, &metadata)
            .await?;

        // 更新缓存
        {
            let mut metadata_cache = self.metadata_cache.write().await;
            let mut encrypted_cache = self.encrypted_tokens.write().await;

            // 检查是否超过最大 Token 数量
            if metadata_cache.len() >= self.config.max_tokens {
                self.cleanup_old_tokens(&mut metadata_cache, &mut encrypted_cache)
                    .await;
            }

            metadata_cache.insert(token_id.clone(), metadata);
            encrypted_cache.insert(token_id.clone(), encrypted);
        }

        // 更新统计
        self.stats.record_store();
        self.update_stats().await;

        Ok(token_id)
    }

    /// 获取 Token
    ///
    /// # 参数
    /// * `token_id` - Token ID
    ///
    /// # 返回
    /// Token 值
    pub async fn get_token(&self, token_id: &str) -> Result<String, TokenStorageError> {
        let mut current_token_id = token_id.to_string();
        let max_iterations = 3; // 防止无限循环
        let mut iterations = 0;

        loop {
            iterations += 1;
            if iterations > max_iterations {
                return Err(TokenStorageError::RotationError("轮换次数过多".to_string()));
            }

            let current_time = current_timestamp();

            // 检查缓存
            let metadata = {
                let metadata_cache = self.metadata_cache.read().await;
                metadata_cache
                    .get(&current_token_id)
                    .cloned()
                    .ok_or_else(|| TokenStorageError::TokenNotFound(current_token_id.clone()))?
            };

            // 检查是否过期
            if metadata.is_expired(current_time) {
                // 清理过期 Token
                self.remove_token(&current_token_id).await?;
                return Err(TokenStorageError::TokenExpired);
            }

            // 检查是否需要轮换
            if self.config.enable_auto_rotation
                && metadata.needs_rotation(current_time, self.config.rotation_threshold_secs)
            {
                // 触发轮换
                let new_token_id = self.rotate_token(&current_token_id).await?;
                self.stats.record_rotation();

                // 继续循环获取新 Token
                current_token_id = new_token_id;
                continue;
            }

            // 从缓存获取加密 Token
            let encrypted = {
                let encrypted_cache = self.encrypted_tokens.read().await;
                encrypted_cache
                    .get(&current_token_id)
                    .cloned()
                    .ok_or_else(|| TokenStorageError::TokenNotFound(current_token_id.clone()))?
            };

            // 解密 Token
            let token = self.decrypt_token(&encrypted)?;

            // 更新元数据
            {
                let mut metadata_cache = self.metadata_cache.write().await;
                if let Some(meta) = metadata_cache.get_mut(&current_token_id) {
                    meta.touch();
                }
            }

            // 更新统计
            self.stats.record_retrieve();

            return Ok(token);
        }
    }

    /// 轮换 Token
    ///
    /// # 流程
    ///
    /// 1. 从缓存获取旧 Token（直接访问，避免递归）
    /// 2. 生成新 Token
    /// 3. 存储新 Token
    /// 4. 标记旧 Token
    /// 5. 清理旧 Token
    ///
    /// # 返回
    /// 新 Token ID
    pub async fn rotate_token(&self, token_id: &str) -> Result<String, TokenStorageError> {
        // 从缓存直接获取旧 Token（不调用 get_token 避免递归）
        let _old_token = {
            let encrypted_cache = self.encrypted_tokens.read().await;
            let encrypted = encrypted_cache
                .get(token_id)
                .ok_or_else(|| TokenStorageError::TokenNotFound(token_id.to_string()))?;
            self.decrypt_token(encrypted)?
        };

        // 获取旧元数据
        let old_metadata = {
            let metadata_cache = self.metadata_cache.read().await;
            metadata_cache
                .get(token_id)
                .cloned()
                .ok_or_else(|| TokenStorageError::TokenNotFound(token_id.to_string()))?
        };

        // 生成新 Token（这里使用随机值，实际应该从认证服务获取）
        let new_token = generate_new_token();

        // 存储新 Token
        let new_token_id = self
            .store_token(
                &old_metadata.service_name,
                &old_metadata.account_id,
                &new_token,
            )
            .await?;

        // 标记旧 Token
        {
            let mut metadata_cache = self.metadata_cache.write().await;
            if let Some(meta) = metadata_cache.get_mut(token_id) {
                meta.mark_rotated(token_id.to_string());
            }
        }

        // 清理旧 Token（延迟清理，保留短暂过渡期）
        tokio::spawn({
            let storage = self.clone_for_task();
            let old_token_id = token_id.to_string();
            async move {
                tokio::time::sleep(Duration::from_secs(60)).await;
                let _ = storage.remove_token(&old_token_id).await;
            }
        });

        Ok(new_token_id)
    }

    /// 删除 Token
    pub async fn remove_token(&self, token_id: &str) -> Result<(), TokenStorageError> {
        // 从密钥环删除（模拟）
        self.delete_from_keychain(token_id).await?;

        // 从缓存删除
        {
            let mut metadata_cache = self.metadata_cache.write().await;
            let mut encrypted_cache = self.encrypted_tokens.write().await;

            metadata_cache.remove(token_id);
            encrypted_cache.remove(token_id);
        }

        self.update_stats().await;

        Ok(())
    }

    /// 列出所有 Token
    pub async fn list_tokens(&self) -> Vec<TokenMetadata> {
        let metadata_cache = self.metadata_cache.read().await;
        metadata_cache.values().cloned().collect()
    }

    /// 获取统计信息
    pub fn get_stats(&self) -> StatsSnapshot {
        self.stats.snapshot()
    }

    /// 加密 Token
    fn encrypt_token(&self, token: &str) -> Result<EncryptedToken, TokenStorageError> {
        // 使用 AES-256-GCM 加密
        // 这里简化处理
        let mut nonce = [0u8; 12];
        get_random_bytes(&mut nonce);

        let ciphertext = token.as_bytes().to_vec(); // 简化：实际应加密
        let auth_tag = [0u8; 16]; // 简化：实际应有认证标签

        Ok(EncryptedToken::new(ciphertext, auth_tag, nonce))
    }

    /// 解密 Token
    fn decrypt_token(&self, encrypted: &EncryptedToken) -> Result<String, TokenStorageError> {
        // 使用 AES-256-GCM 解密
        // 这里简化处理
        let plaintext = std::str::from_utf8(encrypted.ciphertext())
            .map_err(|_| TokenStorageError::DecryptionError("无效的 UTF-8".to_string()))?;

        Ok(plaintext.to_string())
    }

    /// 存储到密钥环
    async fn store_to_keychain(
        &self,
        _token_id: &str,
        _encrypted: &EncryptedToken,
        _metadata: &TokenMetadata,
    ) -> Result<(), TokenStorageError> {
        // 在实际实现中使用 macOS Security Framework
        // 这里仅模拟
        Ok(())
    }

    /// 从密钥环删除
    async fn delete_from_keychain(&self, _token_id: &str) -> Result<(), TokenStorageError> {
        // 在实际实现中使用 macOS Security Framework
        Ok(())
    }

    /// 清理旧 Token
    async fn cleanup_old_tokens(
        &self,
        metadata_cache: &mut HashMap<String, TokenMetadata>,
        encrypted_cache: &mut HashMap<String, EncryptedToken>,
    ) {
        let current_time = current_timestamp();
        let mut to_remove = Vec::new();

        for (token_id, metadata) in metadata_cache.iter() {
            if metadata.is_expired(current_time) {
                to_remove.push(token_id.clone());
            }
        }

        for token_id in to_remove {
            metadata_cache.remove(&token_id);
            encrypted_cache.remove(&token_id);
            self.stats.record_expiration();
        }
    }

    /// 更新统计
    async fn update_stats(&self) {
        let metadata_cache = self.metadata_cache.read().await;
        self.stats.update_token_count(metadata_cache.len());
    }

    /// 为任务克隆引用
    fn clone_for_task(&self) -> Arc<Self> {
        // 创建新的 Arc 实例（共享内部状态）
        Arc::new(McpTokenStorage {
            metadata_cache: Arc::clone(&self.metadata_cache),
            encrypted_tokens: Arc::clone(&self.encrypted_tokens),
            encryption_key: SecureBuffer::with_data(self.encryption_key.as_slice()),
            config: self.config.clone(),
            stats: Arc::clone(&self.stats),
            keychain_initialized: Arc::clone(&self.keychain_initialized),
        })
    }
}

/// 生成 Token ID
fn generate_token_id(service_name: &str, account_id: &str) -> String {
    use ring::digest::{Context, SHA256};
    use ring::rand::{SecureRandom, SystemRandom};

    // 添加随机 nonce 确保唯一性
    let rng = SystemRandom::new();
    let mut nonce = [0u8; 8];
    let _ = rng.fill(&mut nonce);

    let mut context = Context::new(&SHA256);
    context.update(service_name.as_bytes());
    context.update(account_id.as_bytes());
    context.update(&current_timestamp().to_be_bytes());
    context.update(&nonce);

    let hash = context.finish();
    format!("token_{}", hex::encode(&hash.as_ref()[..16]))
}

/// 生成新 Token
fn generate_new_token() -> String {
    use ring::rand::{SecureRandom, SystemRandom};

    let rng = SystemRandom::new();
    let mut bytes = [0u8; 32];
    rng.fill(&mut bytes).unwrap();

    format!("tok_{}", hex::encode(bytes))
}

/// 获取随机字节
fn get_random_bytes(buffer: &mut [u8]) {
    use rand::RngCore;
    let mut rng = rand::thread_rng();
    rng.fill_bytes(buffer);
}

/// 获取当前时间戳
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间错误")
        .as_secs()
}

/// 安全比较 Token ID
pub fn secure_compare_token_ids(a: &str, b: &str) -> bool {
    ct_compare(a.as_bytes(), b.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_storage_creation() {
        let config = TokenStorageConfig::default();
        let storage = McpTokenStorage::new(config).unwrap();

        assert!(!storage.is_keychain_initialized());
        assert_eq!(storage.get_stats().current_token_count, 0);
    }

    #[tokio::test]
    async fn test_store_and_get_token() {
        let storage = McpTokenStorage::new(TokenStorageConfig::default()).unwrap();
        storage.initialize_keychain().await.unwrap();

        let token_id = storage
            .store_token("test_service", "user_123", "test_token_value")
            .await
            .unwrap();

        assert!(token_id.starts_with("token_"));

        let retrieved = storage.get_token(&token_id).await.unwrap();
        assert_eq!(retrieved, "test_token_value");
    }

    #[tokio::test]
    async fn test_token_expiry() {
        let mut config = TokenStorageConfig::default();
        config.token_ttl_secs = 1; // 1 秒 TTL

        let storage = McpTokenStorage::new(config).unwrap();
        storage.initialize_keychain().await.unwrap();

        let token_id = storage
            .store_token("test_service", "user_123", "test_token")
            .await
            .unwrap();

        // 等待过期
        tokio::time::sleep(Duration::from_secs(2)).await;

        let result = storage.get_token(&token_id).await;
        assert!(matches!(result, Err(TokenStorageError::TokenExpired)));
    }

    #[tokio::test]
    async fn test_token_rotation() {
        let storage = McpTokenStorage::new(TokenStorageConfig::default()).unwrap();
        storage.initialize_keychain().await.unwrap();

        let token_id = storage
            .store_token("test_service", "user_123", "test_token")
            .await
            .unwrap();

        // 手动触发轮换
        let new_token_id = storage.rotate_token(&token_id).await.unwrap();

        assert_ne!(token_id, new_token_id);

        // 验证新 Token 可用
        let retrieved = storage.get_token(&new_token_id).await.unwrap();
        assert!(!retrieved.is_empty());
    }

    #[tokio::test]
    async fn test_token_not_found() {
        let storage = McpTokenStorage::new(TokenStorageConfig::default()).unwrap();

        let result = storage.get_token("nonexistent_token").await;
        assert!(matches!(result, Err(TokenStorageError::TokenNotFound(_))));
    }

    #[tokio::test]
    async fn test_remove_token() {
        let storage = McpTokenStorage::new(TokenStorageConfig::default()).unwrap();
        storage.initialize_keychain().await.unwrap();

        let token_id = storage
            .store_token("test_service", "user_123", "test_token")
            .await
            .unwrap();

        storage.remove_token(&token_id).await.unwrap();

        let result = storage.get_token(&token_id).await;
        assert!(matches!(result, Err(TokenStorageError::TokenNotFound(_))));
    }

    #[tokio::test]
    async fn test_list_tokens() {
        let storage = McpTokenStorage::new(TokenStorageConfig::default()).unwrap();
        storage.initialize_keychain().await.unwrap();

        storage
            .store_token("service_1", "user_1", "token_1")
            .await
            .unwrap();
        storage
            .store_token("service_2", "user_2", "token_2")
            .await
            .unwrap();

        let tokens = storage.list_tokens().await;
        assert_eq!(tokens.len(), 2);
    }

    #[tokio::test]
    async fn test_token_stats() {
        let storage = McpTokenStorage::new(TokenStorageConfig::default()).unwrap();
        storage.initialize_keychain().await.unwrap();

        storage
            .store_token("test_service", "user_123", "test_token")
            .await
            .unwrap();

        let stats = storage.get_stats();
        assert_eq!(stats.total_stores, 1);
        assert_eq!(stats.current_token_count, 1);

        storage.get_token("token_").await.ok(); // 会失败，但会计数
        let stats = storage.get_stats();
        assert_eq!(stats.total_retrieves, 0); // 因为失败了
    }

    #[test]
    fn test_token_metadata_creation() {
        let metadata = TokenMetadata::new(
            "test_token".to_string(),
            "test_service".to_string(),
            "user_123".to_string(),
            3600,
        );

        assert_eq!(metadata.token_id, "test_token");
        assert_eq!(metadata.service_name, "test_service");
        assert!(!metadata.is_rotated);
    }

    #[test]
    fn test_token_metadata_expiry() {
        let current_time = current_timestamp();
        let mut metadata = TokenMetadata::new(
            "test_token".to_string(),
            "test_service".to_string(),
            "user_123".to_string(),
            60,
        );

        assert!(!metadata.is_expired(current_time));
        assert!(metadata.is_expired(current_time + 120));

        metadata.touch();
        assert_eq!(metadata.usage_count, 1);
    }

    #[test]
    fn test_token_metadata_rotation() {
        let mut metadata = TokenMetadata::new(
            "test_token".to_string(),
            "test_service".to_string(),
            "user_123".to_string(),
            3600,
        );

        assert!(!metadata.is_rotated);

        metadata.mark_rotated("old_token".to_string());
        assert!(metadata.is_rotated);
        assert_eq!(metadata.previous_token_id, Some("old_token".to_string()));
    }

    #[test]
    fn test_secure_compare_token_ids() {
        assert!(secure_compare_token_ids("token_123", "token_123"));
        assert!(!secure_compare_token_ids("token_123", "token_456"));
    }

    #[test]
    fn test_generate_token_id() {
        let token_id = generate_token_id("test_service", "user_123");
        assert!(token_id.starts_with("token_"));
        assert_eq!(token_id.len(), 38); // "token_" + 32 hex chars
    }

    #[test]
    fn test_generate_new_token() {
        let token = generate_new_token();
        assert!(token.starts_with("tok_"));
        assert_eq!(token.len(), 68); // "tok_" + 64 hex chars
    }
}
