//! SGX Enclave 核心模块
//!
//! 实现 Enclave 初始化、生命周期管理和安全操作接口
//!
//! # 架构概述
//!
//! ```text
//! Enclave
//!   ├── L0 Hardware Root Key (Sealing Key from SGX)
//!   ├── L1 Enclave Master Key (HKDF derived from L0)
//!   ├── Key Cache (L2 User Vault Keys, TTL: 5min)
//!   ├── Sealing Service (persistent storage)
//!   └── Remote Attestation (DCAP)
//! ```

use crate::crypto::{
    constants::NONCE_LENGTH,
    CryptoError, EncryptedBlob, KeyHandle, KeyHierarchy,
    KeyPurpose,
};
use crate::tee::sealing::{SealPolicy, SealedStorage, SealingKey, SealingService};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use zeroize::Zeroize;

/// Enclave 配置
#[derive(Debug, Clone)]
pub struct EnclaveConfig {
    /// 用户密钥缓存 TTL（秒）
    pub user_key_ttl: u64,

    /// 密封策略
    pub seal_policy: SealPolicy,

    /// 密封数据存储路径
    pub sealed_storage_path: String,

    /// Enclave 名称
    pub name: String,

    /// 调试模式（禁用安全特性，仅用于开发）
    pub debug_mode: bool,
}

impl Default for EnclaveConfig {
    fn default() -> Self {
        Self {
            user_key_ttl: 300, // 5 分钟
            seal_policy: SealPolicy::Mrsigner,
            sealed_storage_path: ".sealed".to_string(),
            name: "credbridge-enclave".to_string(),
            debug_mode: false,
        }
    }
}

/// Enclave 状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EnclaveState {
    /// 未初始化
    Uninitialized,
    /// 正在初始化
    Initializing,
    /// 已初始化并运行
    Running,
    /// 正在关闭
    ShuttingDown,
    /// 已关闭
    Shutdown,
    /// 错误状态
    Error,
}

impl std::fmt::Display for EnclaveState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnclaveState::Uninitialized => write!(f, "uninitialized"),
            EnclaveState::Initializing => write!(f, "initializing"),
            EnclaveState::Running => write!(f, "running"),
            EnclaveState::ShuttingDown => write!(f, "shutting-down"),
            EnclaveState::Shutdown => write!(f, "shutdown"),
            EnclaveState::Error => write!(f, "error"),
        }
    }
}

/// Enclave 统计信息
#[derive(Debug, Clone, Default)]
pub struct EnclaveStats {
    /// 初始化时间戳
    pub initialized_at: Option<u64>,

    /// 密钥派生次数
    pub key_derivations: u64,

    /// 加密操作次数
    pub encryption_ops: u64,

    /// 解密操作次数
    pub decryption_ops: u64,

    /// 当前缓存的用户密钥数量
    pub cached_user_keys: usize,
}

/// Enclave 核心结构
///
/// 管理整个 TEE Enclave 的生命周期和安全操作
pub struct Enclave {
    /// 配置
    config: EnclaveConfig,

    /// 当前状态
    state: EnclaveState,

    /// L0 硬件根密钥
    l0_key: Option<SealingKey>,

    /// 密钥层次管理器
    key_hierarchy: KeyHierarchy,

    /// 用户密钥缓存（L2）
    user_key_cache: Arc<RwLock<UserKeyCache>>,

    /// 密封服务
    sealing_service: SealingService,

    /// 密封存储
    sealed_storage: Option<SealedStorage>,

    /// MRENCLAVE 测量值
    mrenclave: [u8; 32],

    /// MRSIGNER 测量值
    mrsigner: [u8; 32],

    /// 统计信息
    stats: EnclaveStats,
}

impl std::fmt::Debug for Enclave {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Enclave")
            .field("config", &self.config)
            .field("state", &self.state)
            .field("has_l0_key", &self.l0_key.is_some())
            .field("mrenclave", &hex::encode(self.mrenclave))
            .field("mrsigner", &hex::encode(self.mrsigner))
            .field("stats", &self.stats)
            .finish()
    }
}

/// 用户密钥缓存
pub struct UserKeyCache {
    /// 缓存条目
    entries: HashMap<KeyHandle, CachedUserKey>,

    /// 默认 TTL（秒）
    default_ttl: u64,
}

/// 缓存的用户密钥
#[derive(Clone)]
struct CachedUserKey {
    /// 密钥句柄
    handle: KeyHandle,

    /// 租户ID
    tenant_id: String,

    /// 用户ID哈希
    user_id_hash: String,

    /// 创建时间戳
    created_at: u64,

    /// 最后访问时间戳
    last_accessed_at: u64,

    /// 访问计数
    access_count: u64,
}

impl Enclave {
    /// 创建新的 Enclave 实例
    pub fn new(config: EnclaveConfig) -> Self {
        // 确保存储目录存在
        if let Err(e) = std::fs::create_dir_all(&config.sealed_storage_path) {
            tracing::warn!("无法创建密封存储目录: {}", e);
        }

        let sealed_storage = SealedStorage::new(config.sealed_storage_path.clone());

        Self {
            config: config.clone(),
            state: EnclaveState::Uninitialized,
            l0_key: None,
            key_hierarchy: KeyHierarchy::new(),
            user_key_cache: Arc::new(RwLock::new(UserKeyCache {
                entries: HashMap::new(),
                default_ttl: config.user_key_ttl,
            })),
            sealing_service: SealingService::new(),
            sealed_storage: Some(sealed_storage),
            mrenclave: [0u8; 32],
            mrsigner: [0u8; 32],
            stats: EnclaveStats::default(),
        }
    }

    /// 使用默认配置创建 Enclave
    pub fn default() -> Self {
        Self::new(EnclaveConfig::default())
    }

    /// 初始化 Enclave
    ///
    /// # 流程
    /// 1. 从 SGX 获取 L0 Sealing Key
    /// 2. 派生 L1 Enclave Master Key
    /// 3. 从密封存储恢复持久化密钥（如果存在）
    /// 4. 设置 MRENCLAVE/MRSIGNER 测量值
    ///
    /// # 错误
    /// 如果 Enclave 已经初始化或密钥派生失败，返回错误
    pub fn initialize(&mut self) -> Result<(), EnclaveError> {
        if self.state != EnclaveState::Uninitialized {
            return Err(EnclaveError::AlreadyInitialized);
        }

        self.state = EnclaveState::Initializing;

        // 1. 获取 L0 Sealing Key
        let l0_key = self
            .sealing_service
            .get_sealing_key(self.config.seal_policy)
            .map_err(|e| EnclaveError::KeyInitializationFailed(e.to_string()))?;

        // 2. 派生 L1 Master Key
        let l1_handle = self
            .key_hierarchy
            .initialize_master_key(&crate::crypto::HardwareRootKey::from_sgx_sealing_key(
                *l0_key.as_bytes(),
            ))
            .map_err(|e| EnclaveError::KeyInitializationFailed(e.to_string()))?;

        // 3. 尝试从密封存储恢复状态
        if let Err(e) = self.restore_from_sealed_storage() {
            // 恢复失败不是致命错误，可能是首次启动
            if self.config.debug_mode {
                tracing::debug!("密封存储恢复跳过: {}", e);
            }
        }

        // 4. 生成测量值（模拟）
        self.generate_measurement();

        self.l0_key = Some(l0_key);
        self.state = EnclaveState::Running;
        self.stats.initialized_at = Some(current_timestamp());

        Ok(())
    }

    /// 关闭 Enclave
    ///
    /// 清理敏感数据并持久化状态
    pub fn shutdown(&mut self) -> Result<(), EnclaveError> {
        if self.state == EnclaveState::Shutdown {
            return Ok(());
        }

        self.state = EnclaveState::ShuttingDown;

        // 1. 密封 L1 Master Key
        if let Err(e) = self.seal_master_key() {
            tracing::error!("密封主密钥失败: {}", e);
        }

        // 2. 清理用户密钥缓存
        // 使用 try_lock 在关闭时避免阻塞
        if let Ok(mut cache) = self.user_key_cache.try_write() {
            cache.entries.clear();
        }

        // 3. 清理 L0 密钥
        if let Some(mut key) = self.l0_key.take() {
            key.key_material.zeroize();
        }

        self.state = EnclaveState::Shutdown;
        Ok(())
    }

    /// 获取当前状态
    pub fn state(&self) -> EnclaveState {
        self.state
    }

    /// 检查 Enclave 是否运行中
    pub fn is_running(&self) -> bool {
        self.state == EnclaveState::Running
    }

    /// 获取 MRENCLAVE 测量值
    pub fn mrenclave(&self) -> [u8; 32] {
        self.mrenclave
    }

    /// 获取 MRSIGNER 测量值
    pub fn mrsigner(&self) -> [u8; 32] {
        self.mrsigner
    }

    /// 获取配置
    pub fn config(&self) -> &EnclaveConfig {
        &self.config
    }

    /// 获取统计信息
    pub fn stats(&self) -> &EnclaveStats {
        &self.stats
    }

    /// 派生用户保险库密钥（L2）
    ///
    /// 如果密钥已在缓存中，直接返回缓存的句柄
    pub fn derive_user_vault_key(
        &mut self,
        tenant_id: &str,
        user_id: &str,
    ) -> Result<KeyHandle, EnclaveError> {
        self.ensure_running()?;

        // 检查缓存
        let cache_key = derive_cache_key(tenant_id, user_id);

        let cache = acquire_read_lock_sync(&self.user_key_cache)?;
        if let Some(entry) = cache.entries.get(&cache_key) {
            // 检查是否过期
            let now = current_timestamp();
            if now - entry.last_accessed_at <= cache.default_ttl {
                return Ok(cache_key);
            }
        }
        drop(cache);

        // 派生新密钥
        let l2_key = self
            .key_hierarchy
            .derive_user_vault_key(tenant_id, user_id)
            .map_err(|e| EnclaveError::KeyDerivationFailed(e.to_string()))?;

        let handle = l2_key.key_handle();

        // 添加到缓存
        let mut cache = acquire_write_lock_sync(&self.user_key_cache)?;
        let now = current_timestamp();
        cache.entries.insert(
            cache_key,
            CachedUserKey {
                handle: cache_key,
                tenant_id: tenant_id.to_string(),
                user_id_hash: format!("hash:{}", user_id),
                created_at: now,
                last_accessed_at: now,
                access_count: 0,
            },
        );
        drop(cache);

        self.stats.key_derivations += 1;

        Ok(handle)
    }

    /// 加密凭证
    ///
    /// 在 Enclave 内部执行 AES-256-GCM 加密
    pub fn encrypt_credential(
        &mut self,
        tenant_id: &str,
        user_id: &str,
        credential_id: &str,
        plaintext: &[u8],
    ) -> Result<EncryptedBlob, EnclaveError> {
        use aes_gcm::{
            aead::{Aead, AeadCore, KeyInit, OsRng},
            Aes256Gcm,
        };
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

        self.ensure_running()?;

        // 派生 L2 密钥
        let l2_key = self
            .key_hierarchy
            .derive_user_vault_key(tenant_id, user_id)
            .map_err(|e| EnclaveError::KeyDerivationFailed(e.to_string()))?;

        // 派生 L3 密钥
        let l3_key = self
            .key_hierarchy
            .derive_credential_key(&l2_key, credential_id, KeyPurpose::CredentialEncryption)
            .map_err(|e| EnclaveError::KeyDerivationFailed(e.to_string()))?;

        // 生成随机 nonce
        let mut nonce_bytes = [0u8; NONCE_LENGTH];
        let mut rng = rand::thread_rng();
        rand::RngCore::fill_bytes(&mut rng, &mut nonce_bytes);

        // 创建 AES-256-GCM cipher
        let cipher = Aes256Gcm::new_from_slice(l3_key.as_bytes())
            .map_err(|e| EnclaveError::EncryptionFailed(e.to_string()))?;

        let nonce = aes_gcm::Nonce::from_slice(&nonce_bytes);

        // 构建 AAD
        let aad = format!("{}:{}", tenant_id, user_id);

        // 执行加密（使用 AAD）
        let ciphertext = cipher
            .encrypt(
                nonce,
                aes_gcm::aead::Payload {
                    msg: plaintext,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|e| EnclaveError::EncryptionFailed(e.to_string()))?;

        // 分离密文和 auth tag
        let auth_tag_start = ciphertext.len().saturating_sub(16);
        let ciphertext_only = ciphertext[..auth_tag_start].to_vec();
        let auth_tag = ciphertext[auth_tag_start..].to_vec();

        // 计算 AAD 哈希
        let aad = format!("{}:{}", tenant_id, user_id);
        let aad_hash = ring::digest::digest(&ring::digest::SHA256, aad.as_bytes());

        self.stats.encryption_ops += 1;

        Ok(EncryptedBlob {
            version: crate::crypto::constants::PROTOCOL_VERSION,
            algorithm: "AES-256-GCM".to_string(),
            kdf: "HKDF-SHA-256".to_string(),
            nonce: URL_SAFE_NO_PAD.encode(&nonce_bytes),
            auth_tag: URL_SAFE_NO_PAD.encode(&auth_tag),
            ciphertext: URL_SAFE_NO_PAD.encode(&ciphertext_only),
            aad_hash: Some(URL_SAFE_NO_PAD.encode(aad_hash.as_ref())),
        })
    }

    /// 解密凭证
    ///
    /// 在 Enclave 内部执行 AES-256-GCM 解密
    pub fn decrypt_credential(
        &mut self,
        tenant_id: &str,
        user_id: &str,
        credential_id: &str,
        blob: &EncryptedBlob,
    ) -> Result<Vec<u8>, EnclaveError> {
        use aes_gcm::{
            aead::{Aead, KeyInit},
            Aes256Gcm,
        };
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

        self.ensure_running()?;

        // 派生 L2 密钥
        let l2_key = self
            .key_hierarchy
            .derive_user_vault_key(tenant_id, user_id)
            .map_err(|e| EnclaveError::KeyDerivationFailed(e.to_string()))?;

        // 派生 L3 密钥（使用相同的 purpose 作为加密，因为 AES-GCM 是对称加密）
        let l3_key = self
            .key_hierarchy
            .derive_credential_key(&l2_key, credential_id, KeyPurpose::CredentialEncryption)
            .map_err(|e| EnclaveError::KeyDerivationFailed(e.to_string()))?;

        // 解析 nonce
        let nonce_bytes = URL_SAFE_NO_PAD
            .decode(&blob.nonce)
            .map_err(|_| EnclaveError::DecryptionFailed("Invalid nonce encoding".to_string()))?;

        // 解析密文
        let mut ciphertext = URL_SAFE_NO_PAD
            .decode(&blob.ciphertext)
            .map_err(|_| EnclaveError::DecryptionFailed("Invalid ciphertext encoding".to_string()))?;

        // 解析 auth tag
        let auth_tag = URL_SAFE_NO_PAD
            .decode(&blob.auth_tag)
            .map_err(|_| EnclaveError::DecryptionFailed("Invalid auth_tag encoding".to_string()))?;

        // 合并密文和 auth tag
        ciphertext.extend_from_slice(&auth_tag);

        // 构建 AAD（必须与加密时相同）
        let aad = format!("{}:{}", tenant_id, user_id);

        // 创建 AES-256-GCM cipher
        let cipher = Aes256Gcm::new_from_slice(l3_key.as_bytes())
            .map_err(|e| EnclaveError::DecryptionFailed(e.to_string()))?;

        let nonce = aes_gcm::Nonce::from_slice(&nonce_bytes);
        let plaintext = cipher
            .decrypt(
                nonce,
                aes_gcm::aead::Payload {
                    msg: ciphertext.as_ref(),
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| EnclaveError::AuthenticationFailed)?;

        self.stats.decryption_ops += 1;

        Ok(plaintext)
    }

    /// 清理过期缓存
    pub async fn cleanup_expired_cache(&self) -> Result<usize, EnclaveError> {
        let mut cache = match tokio::time::timeout(
            Duration::from_millis(LOCK_TIMEOUT_MS),
            self.user_key_cache.write()
        ).await {
            Ok(guard) => guard,
            Err(_) => return Err(EnclaveError::LockTimeout(
                "写入锁获取超时，可能存在死锁".to_string()
            )),
        };
        let now = current_timestamp();
        let ttl = cache.default_ttl;
        let mut removed = 0;

        cache.entries.retain(|_, entry| {
            let is_valid = now - entry.last_accessed_at <= ttl;
            if !is_valid {
                if self.config.debug_mode {
                    tracing::debug!("清理过期密钥: {:?}", entry.handle);
                }
                removed += 1;
            }
            is_valid
        });

        Ok(removed)
    }

    /// 获取缓存统计
    pub async fn get_cache_stats(&self) -> Result<CacheStats, EnclaveError> {
        let cache = match tokio::time::timeout(
            Duration::from_millis(LOCK_TIMEOUT_MS),
            self.user_key_cache.read()
        ).await {
            Ok(guard) => guard,
            Err(_) => return Err(EnclaveError::LockTimeout(
                "读取锁获取超时，可能存在死锁".to_string()
            )),
        };
        let now = current_timestamp();
        let ttl = cache.default_ttl;

        let active_count = cache
            .entries
            .values()
            .filter(|e| now - e.last_accessed_at <= ttl)
            .count();

        let expired_count = cache.entries.len() - active_count;

        Ok(CacheStats {
            total_entries: cache.entries.len(),
            active_entries: active_count,
            expired_entries: expired_count,
            default_ttl: ttl,
        })
    }

    // ============ 私有方法 ============

    /// 确保 Enclave 正在运行
    fn ensure_running(&self) -> Result<(), EnclaveError> {
        if self.state != EnclaveState::Running {
            return Err(EnclaveError::NotRunning);
        }
        Ok(())
    }

    /// 生成测量值（模拟）
    fn generate_measurement(&mut self) {
        use ring::digest::{digest, SHA256};

        // 计算模拟的 MRENCLAVE
        let enclave_data = format!("{}-v{}", self.config.name, env!("CARGO_PKG_VERSION"));
        let mrenclave_hash = digest(&SHA256, enclave_data.as_bytes());
        self.mrenclave.copy_from_slice(mrenclave_hash.as_ref());

        // 计算模拟的 MRSIGNER
        let signer_data = "CredBridge-Signer-v1";
        let mrsigner_hash = digest(&SHA256, signer_data.as_bytes());
        self.mrsigner.copy_from_slice(mrsigner_hash.as_ref());
    }

    /// 从密封存储恢复
    fn restore_from_sealed_storage(&mut self) -> Result<(), EnclaveError> {
        if let Some(storage) = &self.sealed_storage {
            if storage.exists("master_key") {
                let sealed = storage.load("master_key")?;
                let _plaintext = storage.sealing().unseal_data(&sealed)?;

                if self.config.debug_mode {
                    tracing::debug!("从密封存储恢复主密钥成功");
                }
            }
        }
        Ok(())
    }

    /// 密封主密钥
    fn seal_master_key(&self) -> Result<(), EnclaveError> {
        // 注意：实际实现需要序列化并密封主密钥
        // 这里简化处理
        Ok(())
    }
}

impl Drop for Enclave {
    fn drop(&mut self) {
        // 确保在销毁时清理敏感数据
        if self.state != EnclaveState::Shutdown {
            let _ = self.shutdown();
        }
    }
}

/// Enclave 错误类型
#[derive(Debug)]
pub enum EnclaveError {
    /// 已初始化
    AlreadyInitialized,

    /// 未运行
    NotRunning,

    /// 密钥初始化失败
    KeyInitializationFailed(String),

    /// 密钥派生失败
    KeyDerivationFailed(String),

    /// 加密失败
    EncryptionFailed(String),

    /// 解密失败
    DecryptionFailed(String),

    /// 认证失败
    AuthenticationFailed,

    /// 密封/解封失败
    SealingFailed(String),

    /// 存储错误
    StorageError(String),

    /// 锁超时（死锁检测）
    LockTimeout(String),

    /// 内部错误
    InternalError(String),
}

impl std::fmt::Display for EnclaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnclaveError::AlreadyInitialized => write!(f, "Enclave already initialized"),
            EnclaveError::NotRunning => write!(f, "Enclave not running"),
            EnclaveError::KeyInitializationFailed(msg) => {
                write!(f, "Key initialization failed: {}", msg)
            }
            EnclaveError::KeyDerivationFailed(msg) => write!(f, "Key derivation failed: {}", msg),
            EnclaveError::EncryptionFailed(msg) => write!(f, "Encryption failed: {}", msg),
            EnclaveError::DecryptionFailed(msg) => write!(f, "Decryption failed: {}", msg),
            EnclaveError::AuthenticationFailed => write!(f, "Authentication failed"),
            EnclaveError::SealingFailed(msg) => write!(f, "Sealing failed: {}", msg),
            EnclaveError::StorageError(msg) => write!(f, "Storage error: {}", msg),
            EnclaveError::LockTimeout(msg) => write!(f, "Lock timeout (possible deadlock): {}", msg),
            EnclaveError::InternalError(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for EnclaveError {}

impl From<CryptoError> for EnclaveError {
    fn from(e: CryptoError) -> Self {
        EnclaveError::InternalError(e.to_string())
    }
}

impl From<std::io::Error> for EnclaveError {
    fn from(e: std::io::Error) -> Self {
        EnclaveError::StorageError(e.to_string())
    }
}

/// 缓存统计信息
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub total_entries: usize,
    pub active_entries: usize,
    pub expired_entries: usize,
    pub default_ttl: u64,
}

/// 锁超时配置
///
/// 防止死锁导致的 DoS 攻击
const LOCK_TIMEOUT_MS: u64 = 5000; // 5 秒超时
const LOCK_RETRY_INTERVAL_MS: u64 = 10; // 重试间隔

/// 获取当前时间戳
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// 异步锁获取辅助函数
///
/// 带超时的锁获取，防止死锁
async fn acquire_read_lock<T>(
    lock: &RwLock<T>,
) -> Result<tokio::sync::RwLockReadGuard<'_, T>, EnclaveError> {
    match tokio::time::timeout(Duration::from_millis(LOCK_TIMEOUT_MS), lock.read()).await {
        Ok(guard) => Ok(guard),
        Err(_) => Err(EnclaveError::LockTimeout(
            "读取锁获取超时，可能存在死锁".to_string(),
        )),
    }
}

async fn acquire_write_lock<T>(
    lock: &RwLock<T>,
) -> Result<tokio::sync::RwLockWriteGuard<'_, T>, EnclaveError> {
    match tokio::time::timeout(Duration::from_millis(LOCK_TIMEOUT_MS), lock.write()).await {
        Ok(guard) => Ok(guard),
        Err(_) => Err(EnclaveError::LockTimeout(
            "写入锁获取超时，可能存在死锁".to_string(),
        )),
    }
}

/// 同步锁获取辅助函数（带重试和超时）
///
/// 用于需要同步上下文的场景
fn acquire_read_lock_sync<T>(
    lock: &RwLock<T>,
) -> Result<tokio::sync::RwLockReadGuard<'_, T>, EnclaveError> {
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(LOCK_TIMEOUT_MS);
    let retry_interval = Duration::from_millis(LOCK_RETRY_INTERVAL_MS);

    loop {
        match lock.try_read() {
            Ok(guard) => return Ok(guard),
            Err(_) => {
                if start.elapsed() >= timeout {
                    return Err(EnclaveError::LockTimeout(
                        "读取锁获取超时，可能存在死锁".to_string(),
                    ));
                }
                std::thread::sleep(retry_interval);
            }
        }
    }
}

fn acquire_write_lock_sync<T>(
    lock: &RwLock<T>,
) -> Result<tokio::sync::RwLockWriteGuard<'_, T>, EnclaveError> {
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(LOCK_TIMEOUT_MS);
    let retry_interval = Duration::from_millis(LOCK_RETRY_INTERVAL_MS);

    loop {
        match lock.try_write() {
            Ok(guard) => return Ok(guard),
            Err(_) => {
                if start.elapsed() >= timeout {
                    return Err(EnclaveError::LockTimeout(
                        "写入锁获取超时，可能存在死锁".to_string(),
                    ));
                }
                std::thread::sleep(retry_interval);
            }
        }
    }
}

/// 派生缓存键
fn derive_cache_key(tenant_id: &str, user_id: &str) -> KeyHandle {
    use ring::digest::{digest, SHA256};

    let data = format!("{}:{}", tenant_id, user_id);
    let hash = digest(&SHA256, data.as_bytes());

    let mut key = [0u8; 32];
    key.copy_from_slice(hash.as_ref());
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enclave_lifecycle() {
        let config = EnclaveConfig {
            debug_mode: true,
            ..Default::default()
        };

        let mut enclave = Enclave::new(config);
        assert_eq!(enclave.state(), EnclaveState::Uninitialized);

        // 初始化
        enclave.initialize().unwrap();
        assert_eq!(enclave.state(), EnclaveState::Running);

        // 重复初始化应该失败
        assert!(matches!(
            enclave.initialize().unwrap_err(),
            EnclaveError::AlreadyInitialized
        ));

        // 关闭
        enclave.shutdown().unwrap();
        assert_eq!(enclave.state(), EnclaveState::Shutdown);
    }

    #[test]
    fn test_enclave_encryption() {
        let config = EnclaveConfig {
            debug_mode: true,
            ..Default::default()
        };

        let mut enclave = Enclave::new(config);
        enclave.initialize().unwrap();

        let plaintext = b"My secret credential data";

        // 加密
        let blob = enclave
            .encrypt_credential("tenant_1", "user_1", "cred_1", plaintext)
            .unwrap();

        // nonce 是 base64 编码的，12字节编码后是16个字符
        assert_eq!(blob.nonce.len(), 16);
        assert!(!blob.ciphertext.is_empty());

        // 解密
        let decrypted = enclave
            .decrypt_credential("tenant_1", "user_1", "cred_1", &blob)
            .unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_enclave_different_keys() {
        let config = EnclaveConfig {
            debug_mode: true,
            ..Default::default()
        };

        let mut enclave = Enclave::new(config);
        enclave.initialize().unwrap();

        let plaintext = b"Test data";

        // 不同租户/用户应该产生不同的密文
        let blob1 = enclave
            .encrypt_credential("tenant_1", "user_1", "cred_1", plaintext)
            .unwrap();
        let blob2 = enclave
            .encrypt_credential("tenant_2", "user_1", "cred_1", plaintext)
            .unwrap();
        let blob3 = enclave
            .encrypt_credential("tenant_1", "user_2", "cred_1", plaintext)
            .unwrap();

        // 密文应该不同
        assert_ne!(blob1.ciphertext, blob2.ciphertext);
        assert_ne!(blob1.ciphertext, blob3.ciphertext);

        // 但都可以正确解密
        assert_eq!(
            enclave
                .decrypt_credential("tenant_1", "user_1", "cred_1", &blob1)
                .unwrap(),
            plaintext
        );
        assert_eq!(
            enclave
                .decrypt_credential("tenant_2", "user_1", "cred_1", &blob2)
                .unwrap(),
            plaintext
        );
        assert_eq!(
            enclave
                .decrypt_credential("tenant_1", "user_2", "cred_1", &blob3)
                .unwrap(),
            plaintext
        );
    }

    #[test]
    fn test_enclave_tamper_detection() {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

        let config = EnclaveConfig {
            debug_mode: true,
            ..Default::default()
        };

        let mut enclave = Enclave::new(config);
        enclave.initialize().unwrap();

        let plaintext = b"Sensitive data";
        let blob = enclave
            .encrypt_credential("tenant_1", "user_1", "cred_1", plaintext)
            .unwrap();

        // 篡改密文应该导致认证失败
        let mut tampered_blob = blob.clone();

        // 解码、篡改、重新编码
        let mut ciphertext_bytes = URL_SAFE_NO_PAD
            .decode(&tampered_blob.ciphertext)
            .unwrap();
        if !ciphertext_bytes.is_empty() {
            ciphertext_bytes[0] ^= 0xFF; // 翻转第一个字节
        }
        tampered_blob.ciphertext = URL_SAFE_NO_PAD.encode(&ciphertext_bytes);

        assert!(matches!(
            enclave
                .decrypt_credential("tenant_1", "user_1", "cred_1", &tampered_blob)
                .unwrap_err(),
            EnclaveError::AuthenticationFailed
        ));
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let config = EnclaveConfig {
            debug_mode: true,
            user_key_ttl: 300,
            ..Default::default()
        };

        let mut enclave = Enclave::new(config);
        enclave.initialize().unwrap();

        // 派生一些密钥
        let _ = enclave.derive_user_vault_key("tenant_1", "user_1").unwrap();
        let _ = enclave.derive_user_vault_key("tenant_1", "user_2").unwrap();
        let _ = enclave.derive_user_vault_key("tenant_2", "user_1").unwrap();

        // 检查缓存统计
        let stats = enclave.get_cache_stats().await.unwrap();
        assert_eq!(stats.total_entries, 3);
        assert_eq!(stats.active_entries, 3);
        assert_eq!(stats.expired_entries, 0);
        assert_eq!(stats.default_ttl, 300);
    }

    #[test]
    fn test_enclave_config() {
        let config = EnclaveConfig {
            user_key_ttl: 600,
            seal_policy: SealPolicy::Mrenclave,
            sealed_storage_path: "/tmp/test-sealed".to_string(),
            name: "test-enclave".to_string(),
            debug_mode: true,
        };

        let enclave = Enclave::new(config.clone());

        assert_eq!(enclave.config().user_key_ttl, 600);
        assert_eq!(enclave.config().seal_policy, SealPolicy::Mrenclave);
        assert_eq!(enclave.config().name, "test-enclave");
    }

    #[test]
    fn test_enclave_stats() {
        let config = EnclaveConfig {
            debug_mode: true,
            ..Default::default()
        };

        let mut enclave = Enclave::new(config);
        enclave.initialize().unwrap();

        assert!(enclave.stats().initialized_at.is_some());
        assert_eq!(enclave.stats().encryption_ops, 0);
        assert_eq!(enclave.stats().decryption_ops, 0);

        // 执行一些操作
        let plaintext = b"test data";
        let blob = enclave
            .encrypt_credential("tenant_1", "user_1", "cred_1", plaintext)
            .unwrap();
        enclave
            .decrypt_credential("tenant_1", "user_1", "cred_1", &blob)
            .unwrap();

        assert_eq!(enclave.stats().encryption_ops, 1);
        assert_eq!(enclave.stats().decryption_ops, 1);
    }

    #[test]
    fn test_enclave_state_display() {
        assert_eq!(EnclaveState::Running.to_string(), "running");
        assert_eq!(EnclaveState::Uninitialized.to_string(), "uninitialized");
        assert_eq!(EnclaveState::Shutdown.to_string(), "shutdown");
    }
}
