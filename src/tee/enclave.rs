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

use crate::config::TeeRuntimeMode;
use crate::crypto::keys::RootKeySource;
use crate::crypto::{
    CredentialCryptoContext, CryptoError, EncryptedBlob, HardwareRootKey, KeyHandle, KeyHierarchy,
};
use crate::tee::host_runtime::{SgxHostRuntime, SharedEnclaveRuntime};
use crate::tee::keys::{KeyManager, KeyManagerError};
use crate::tee::provider::{ProviderIdentity, ProviderRequest, bootstrap_enclave};
use crate::tee::sealing::SealPolicy;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use zeroize::Zeroize;

/// Enclave 配置
#[derive(Debug, Clone)]
pub struct EnclaveConfig {
    /// 运行模式
    pub runtime_mode: TeeRuntimeMode,

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
            runtime_mode: TeeRuntimeMode::Simulation,
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

    /// 共享 runtime 句柄（硬件模式）
    runtime: Option<SharedEnclaveRuntime>,

    /// L0 硬件根密钥
    l0_key: Option<HardwareRootKey>,

    /// 实际使用的 L0 来源
    root_key_source: Option<RootKeySource>,

    /// 密钥层次管理器
    key_hierarchy: KeyHierarchy,

    /// 用户密钥缓存（L2）
    user_key_cache: Arc<RwLock<UserKeyCache>>,

    /// L1 持久化管理器
    key_manager: Option<KeyManager>,

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
            .field("runtime_loaded", &self.runtime.is_some())
            .field("has_l0_key", &self.l0_key.is_some())
            .field("root_key_source", &self.root_key_source)
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
#[allow(dead_code)]
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

        Self {
            config: config.clone(),
            state: EnclaveState::Uninitialized,
            runtime: None,
            l0_key: None,
            root_key_source: None,
            key_hierarchy: KeyHierarchy::new(),
            user_key_cache: Arc::new(RwLock::new(UserKeyCache {
                entries: HashMap::new(),
                default_ttl: config.user_key_ttl,
            })),
            key_manager: None,
            mrenclave: [0u8; 32],
            mrsigner: [0u8; 32],
            stats: EnclaveStats::default(),
        }
    }

    /// 创建一个绑定到共享 runtime 的 Enclave。
    pub fn with_runtime(config: EnclaveConfig, runtime: SharedEnclaveRuntime) -> Self {
        let mut enclave = Self::new(config);
        enclave.runtime = Some(runtime);
        enclave
    }

    /// 使用默认配置创建 Enclave
    pub fn with_default_config() -> Self {
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
        let init_result = (|| -> Result<(), EnclaveError> {
            let runtime = self.runtime.clone();
            let runtime = self.ensure_hardware_runtime(runtime)?;
            let provider_request = ProviderRequest {
                runtime_mode: self.config.runtime_mode,
                seal_policy: self.config.seal_policy,
                enclave_name: self.config.name.clone(),
                runtime: runtime.clone(),
            };
            let bootstrap = bootstrap_enclave(&provider_request)
                .map_err(|e| EnclaveError::ProviderBootstrapFailed(e.to_string()))?;

            self.apply_provider_identity(bootstrap.identity);

            let key_manager = if let Some(runtime) = runtime.clone() {
                KeyManager::new_with_runtime(
                    self.config.sealed_storage_path.clone(),
                    self.mrsigner,
                    self.mrenclave,
                    runtime,
                )
            } else {
                KeyManager::new(
                    self.config.sealed_storage_path.clone(),
                    self.mrsigner,
                    self.mrenclave,
                )
            };
            let restored = self.restore_from_sealed_storage(&key_manager)?;

            if !restored {
                self.key_hierarchy
                    .initialize_master_key(&bootstrap.l0_key)
                    .map_err(|e| EnclaveError::KeyInitializationFailed(e.to_string()))?;
            }

            self.root_key_source = Some(bootstrap.l0_key.source());
            self.l0_key = Some(bootstrap.l0_key);
            self.key_manager = Some(key_manager);
            self.state = EnclaveState::Running;
            self.stats.initialized_at = Some(current_timestamp());

            Ok(())
        })();

        if let Err(error) = init_result {
            self.state = EnclaveState::Error;
            return Err(error);
        }

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
        if self.key_manager.is_some() && self.key_hierarchy.export_master_key_material().is_some() {
            if let Err(e) = self.seal_master_key() {
                tracing::error!("密封主密钥失败: {}", e);
            }
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

        self.key_manager.take();
        self.runtime.take();

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

    /// 获取实际使用的 L0 来源。
    pub fn root_key_source(&self) -> Option<RootKeySource> {
        self.root_key_source
    }

    /// 获取统计信息
    pub fn stats(&self) -> &EnclaveStats {
        &self.stats
    }

    /// 基于 Enclave 当前生效的 L1 构建一个软件侧层次快照。
    ///
    /// 用于现有 API 的软件回退路径，避免主启动链重复初始化一份 L0/L1。
    pub fn bootstrap_key_hierarchy(&self) -> Result<KeyHierarchy, EnclaveError> {
        let version = *self.key_hierarchy.key_version();
        let mut hierarchy = KeyHierarchy::with_version(version.major, version.minor);
        let mut master_key_material =
            self.key_hierarchy
                .export_master_key_material()
                .ok_or_else(|| {
                    EnclaveError::KeyInitializationFailed("L1 主密钥未初始化".to_string())
                })?;
        hierarchy.install_master_key(master_key_material);
        master_key_material.zeroize();
        Ok(hierarchy)
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
                user_id_hash: format!("hash:{user_id}"),
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
        user_id_hash: &str,
        credential_id: &str,
        plaintext: &[u8],
    ) -> Result<EncryptedBlob, EnclaveError> {
        self.ensure_running()?;
        // Phase A keeps credential crypto in the host process. Hardware mode still gains a
        // hardware-rooted L0 sealing key and real attestation, but credential encrypt/decrypt do
        // not traverse enclave ECALLs until the later migration phase.

        let context = CredentialCryptoContext::new(tenant_id, user_id_hash, credential_id);
        let blob = context
            .encrypt_with_hierarchy(&self.key_hierarchy, plaintext)
            .map_err(|e| EnclaveError::EncryptionFailed(e.to_string()))?;

        self.stats.encryption_ops += 1;
        Ok(blob)
    }

    /// 解密凭证
    ///
    /// 在 Enclave 内部执行 AES-256-GCM 解密
    pub fn decrypt_credential(
        &mut self,
        tenant_id: &str,
        user_id_hash: &str,
        credential_id: &str,
        blob: &EncryptedBlob,
    ) -> Result<Vec<u8>, EnclaveError> {
        self.ensure_running()?;
        // Phase A keeps credential crypto in the host process. Hardware mode still gains a
        // hardware-rooted L0 sealing key and real attestation, but credential encrypt/decrypt do
        // not traverse enclave ECALLs until the later migration phase.

        let context = CredentialCryptoContext::new(tenant_id, user_id_hash, credential_id);
        let plaintext = context
            .decrypt_with_hierarchy(&self.key_hierarchy, blob)
            .map_err(|e| match e {
                crate::crypto::CryptoError::AuthenticationFailed => {
                    EnclaveError::AuthenticationFailed
                }
                other => EnclaveError::DecryptionFailed(other.to_string()),
            })?;

        self.stats.decryption_ops += 1;

        Ok(plaintext)
    }

    /// 清理过期缓存
    pub async fn cleanup_expired_cache(&self) -> Result<usize, EnclaveError> {
        let mut cache = match tokio::time::timeout(
            Duration::from_millis(LOCK_TIMEOUT_MS),
            self.user_key_cache.write(),
        )
        .await
        {
            Ok(guard) => guard,
            Err(_) => {
                return Err(EnclaveError::LockTimeout(
                    "写入锁获取超时，可能存在死锁".to_string(),
                ));
            }
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
            self.user_key_cache.read(),
        )
        .await
        {
            Ok(guard) => guard,
            Err(_) => {
                return Err(EnclaveError::LockTimeout(
                    "读取锁获取超时，可能存在死锁".to_string(),
                ));
            }
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

    fn apply_provider_identity(&mut self, identity: ProviderIdentity) {
        self.mrenclave = identity.mrenclave;
        self.mrsigner = identity.mrsigner;
    }

    fn ensure_hardware_runtime(
        &mut self,
        runtime: Option<SharedEnclaveRuntime>,
    ) -> Result<Option<SharedEnclaveRuntime>, EnclaveError> {
        if !self.config.runtime_mode.is_hardware() {
            return Ok(None);
        }

        if let Some(runtime) = runtime {
            self.runtime = Some(runtime.clone());
            return Ok(Some(runtime));
        }

        let loaded_runtime = SgxHostRuntime::load_from_env().map_err(|error| {
            EnclaveError::ProviderBootstrapFailed(format!(
                "{error}; refusing to fall back to simulation"
            ))
        })?;
        let loaded_runtime: SharedEnclaveRuntime = Arc::new(loaded_runtime);
        self.runtime = Some(loaded_runtime.clone());
        Ok(Some(loaded_runtime))
    }

    /// 从密封存储恢复
    fn restore_from_sealed_storage(
        &mut self,
        key_manager: &KeyManager,
    ) -> Result<bool, EnclaveError> {
        if !key_manager.has_sealed_master_key() {
            return Ok(false);
        }

        let (mut master_key_material, _metadata) = match key_manager.restore_master_key() {
            Ok(restored) => restored,
            Err(error) if self.should_reinitialize_after_restore_failure(&error) => {
                tracing::warn!("检测到损坏的 simulation 密封主密钥，已忽略并重新生成: {error}");
                key_manager
                    .delete_sealed_master_key()
                    .map_err(|delete_error| {
                        EnclaveError::SealingFailed(delete_error.to_string())
                    })?;
                return Ok(false);
            }
            Err(error) => {
                return Err(EnclaveError::SealingFailed(error.to_string()));
            }
        };
        self.key_hierarchy.install_master_key(master_key_material);
        master_key_material.zeroize();

        if self.config.debug_mode {
            tracing::debug!("从密封存储恢复主密钥成功");
        }

        Ok(true)
    }

    fn should_reinitialize_after_restore_failure(&self, error: &KeyManagerError) -> bool {
        self.config.runtime_mode == TeeRuntimeMode::Simulation
            && matches!(
                error,
                KeyManagerError::KeyNotFound
                    | KeyManagerError::StorageError(_)
                    | KeyManagerError::SealingFailed(_)
                    | KeyManagerError::InvalidMetadata
            )
    }

    /// 密封主密钥
    fn seal_master_key(&self) -> Result<(), EnclaveError> {
        let key_manager = self
            .key_manager
            .as_ref()
            .ok_or_else(|| EnclaveError::SealingFailed("KeyManager 未初始化".to_string()))?;
        let mut master_key_material = self
            .key_hierarchy
            .export_master_key_material()
            .ok_or_else(|| EnclaveError::SealingFailed("L1 主密钥未初始化".to_string()))?;

        let result = key_manager
            .seal_master_key(&master_key_material, self.config.seal_policy)
            .map_err(|e| EnclaveError::SealingFailed(e.to_string()));
        master_key_material.zeroize();
        result
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

    /// Provider 启动失败
    ProviderBootstrapFailed(String),

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
                write!(f, "Key initialization failed: {msg}")
            }
            EnclaveError::ProviderBootstrapFailed(msg) => {
                write!(f, "Provider bootstrap failed: {msg}")
            }
            EnclaveError::KeyDerivationFailed(msg) => write!(f, "Key derivation failed: {msg}"),
            EnclaveError::EncryptionFailed(msg) => write!(f, "Encryption failed: {msg}"),
            EnclaveError::DecryptionFailed(msg) => write!(f, "Decryption failed: {msg}"),
            EnclaveError::AuthenticationFailed => write!(f, "Authentication failed"),
            EnclaveError::SealingFailed(msg) => write!(f, "Sealing failed: {msg}"),
            EnclaveError::StorageError(msg) => write!(f, "Storage error: {msg}"),
            EnclaveError::LockTimeout(msg) => {
                write!(f, "Lock timeout (possible deadlock): {msg}")
            }
            EnclaveError::InternalError(msg) => write!(f, "Internal error: {msg}"),
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
#[allow(dead_code)]
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

#[allow(dead_code)]
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
    use ring::digest::{SHA256, digest};

    let data = format!("{tenant_id}:{user_id}");
    let hash = digest(&SHA256, data.as_bytes());

    let mut key = [0u8; 32];
    key.copy_from_slice(hash.as_ref());
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "tee-hardware")]
    use crate::crypto::EncryptedBlob;
    #[cfg(feature = "tee-hardware")]
    use crate::tee::ffi_types::{
        EnclaveIdentity, SGX_REPORT_DATA_LEN, SGX_SEALING_KEY_LEN, SGX_TARGET_INFO_LEN,
    };
    #[cfg(feature = "tee-hardware")]
    use crate::tee::host_runtime::{EnclaveRuntime, HostRuntimeError, SharedEnclaveRuntime};
    use crate::vault::models::UserId;
    #[cfg(feature = "tee-hardware")]
    use base64::Engine as _;
    use std::path::PathBuf;
    #[cfg(feature = "tee-hardware")]
    use std::sync::Arc;

    fn hashed_user_id(raw: &str) -> String {
        UserId::new(raw).hash().to_string()
    }

    fn temp_sealed_storage_path(test_name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "credbridge-enclave-{test_name}-{}",
            uuid::Uuid::new_v4()
        ))
    }

    #[cfg(feature = "tee-hardware")]
    struct RuntimeCryptoMock;

    #[cfg(feature = "tee-hardware")]
    impl EnclaveRuntime for RuntimeCryptoMock {
        fn get_identity(&self) -> Result<EnclaveIdentity, HostRuntimeError> {
            Ok(EnclaveIdentity::new([0x11; 32], [0x22; 32]))
        }

        fn get_targeted_report(
            &self,
            _target_info: [u8; SGX_TARGET_INFO_LEN],
            _report_data: [u8; SGX_REPORT_DATA_LEN],
        ) -> Result<Vec<u8>, HostRuntimeError> {
            Ok(vec![0u8; 432])
        }

        fn get_sealing_key(
            &self,
            _policy: SealPolicy,
        ) -> Result<[u8; SGX_SEALING_KEY_LEN], HostRuntimeError> {
            Ok([0x33; SGX_SEALING_KEY_LEN])
        }

        fn encrypt_credential(
            &self,
            _tenant_id: &str,
            _user_id_hash: &str,
            _credential_id: &str,
            plaintext: &[u8],
        ) -> Result<EncryptedBlob, HostRuntimeError> {
            Ok(EncryptedBlob {
                version: 1,
                algorithm: "MOCK-RUNTIME".to_string(),
                kdf: "MOCK".to_string(),
                nonce: "mocknonce".to_string(),
                auth_tag: "mocktag".to_string(),
                ciphertext: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(plaintext),
                aad_hash: None,
            })
        }

        fn decrypt_credential(
            &self,
            _tenant_id: &str,
            _user_id_hash: &str,
            _credential_id: &str,
            blob: &EncryptedBlob,
        ) -> Result<Vec<u8>, HostRuntimeError> {
            use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

            URL_SAFE_NO_PAD
                .decode(&blob.ciphertext)
                .map_err(|error| HostRuntimeError::LoadFailed {
                    path: PathBuf::new(),
                    detail: error.to_string(),
                })
        }
    }

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
        assert_eq!(enclave.root_key_source(), Some(RootKeySource::Simulation));

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
        let user_hash = hashed_user_id("user_1");

        // 加密
        let blob = enclave
            .encrypt_credential("tenant_1", &user_hash, "cred_1", plaintext)
            .unwrap();

        // nonce 是 base64 编码的，12字节编码后是16个字符
        assert_eq!(blob.nonce.len(), 16);
        assert!(!blob.ciphertext.is_empty());

        // 解密
        let decrypted = enclave
            .decrypt_credential("tenant_1", &user_hash, "cred_1", &blob)
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
        let user_1_hash = hashed_user_id("user_1");
        let user_2_hash = hashed_user_id("user_2");

        // 不同租户/用户应该产生不同的密文
        let blob1 = enclave
            .encrypt_credential("tenant_1", &user_1_hash, "cred_1", plaintext)
            .unwrap();
        let blob2 = enclave
            .encrypt_credential("tenant_2", &user_1_hash, "cred_1", plaintext)
            .unwrap();
        let blob3 = enclave
            .encrypt_credential("tenant_1", &user_2_hash, "cred_1", plaintext)
            .unwrap();

        // 密文应该不同
        assert_ne!(blob1.ciphertext, blob2.ciphertext);
        assert_ne!(blob1.ciphertext, blob3.ciphertext);

        // 但都可以正确解密
        assert_eq!(
            enclave
                .decrypt_credential("tenant_1", &user_1_hash, "cred_1", &blob1)
                .unwrap(),
            plaintext
        );
        assert_eq!(
            enclave
                .decrypt_credential("tenant_2", &user_1_hash, "cred_1", &blob2)
                .unwrap(),
            plaintext
        );
        assert_eq!(
            enclave
                .decrypt_credential("tenant_1", &user_2_hash, "cred_1", &blob3)
                .unwrap(),
            plaintext
        );
    }

    #[test]
    fn test_enclave_tamper_detection() {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

        let config = EnclaveConfig {
            debug_mode: true,
            ..Default::default()
        };

        let mut enclave = Enclave::new(config);
        enclave.initialize().unwrap();

        let plaintext = b"Sensitive data";
        let user_hash = hashed_user_id("user_1");
        let blob = enclave
            .encrypt_credential("tenant_1", &user_hash, "cred_1", plaintext)
            .unwrap();

        // 篡改密文应该导致认证失败
        let mut tampered_blob = blob.clone();

        // 解码、篡改、重新编码
        let mut ciphertext_bytes = URL_SAFE_NO_PAD.decode(&tampered_blob.ciphertext).unwrap();
        if !ciphertext_bytes.is_empty() {
            ciphertext_bytes[0] ^= 0xFF; // 翻转第一个字节
        }
        tampered_blob.ciphertext = URL_SAFE_NO_PAD.encode(&ciphertext_bytes);

        assert!(matches!(
            enclave
                .decrypt_credential("tenant_1", &user_hash, "cred_1", &tampered_blob)
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
            runtime_mode: TeeRuntimeMode::Simulation,
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
    fn test_bootstrap_key_hierarchy_reuses_current_l1() {
        let config = EnclaveConfig {
            debug_mode: true,
            ..Default::default()
        };

        let mut enclave = Enclave::new(config);
        enclave.initialize().unwrap();

        let hierarchy = enclave.bootstrap_key_hierarchy().unwrap();
        let l2_key = hierarchy
            .derive_user_vault_key("tenant_1", "user_1")
            .unwrap();

        assert_eq!(l2_key.tenant_id(), "tenant_1");
        assert!(l2_key.user_id_hash().contains("user_1"));
    }

    #[test]
    fn test_seal_and_restore_master_key_closed_loop() {
        let storage_path = temp_sealed_storage_path("restore-loop");
        let storage_path_str = storage_path.to_string_lossy().to_string();

        let config = EnclaveConfig {
            sealed_storage_path: storage_path_str.clone(),
            debug_mode: true,
            ..Default::default()
        };

        let user_hash = hashed_user_id("restored-user");
        let plaintext = b"persistent secret";

        let original_blob = {
            let mut enclave = Enclave::new(config.clone());
            enclave.initialize().unwrap();
            let blob = enclave
                .encrypt_credential("tenant_1", &user_hash, "cred_1", plaintext)
                .unwrap();
            enclave.shutdown().unwrap();
            blob
        };

        let mut restored_enclave = Enclave::new(config);
        restored_enclave.initialize().unwrap();
        let decrypted = restored_enclave
            .decrypt_credential("tenant_1", &user_hash, "cred_1", &original_blob)
            .unwrap();

        assert_eq!(
            restored_enclave.root_key_source(),
            Some(RootKeySource::Simulation)
        );
        assert_eq!(decrypted, plaintext);

        let _ = restored_enclave.shutdown();
        let _ = std::fs::remove_dir_all(storage_path);
    }

    #[test]
    fn test_hardware_mode_fails_closed_without_provider() {
        let storage_path = temp_sealed_storage_path("hardware-fail-closed");
        let config = EnclaveConfig {
            runtime_mode: TeeRuntimeMode::Hardware,
            sealed_storage_path: storage_path.to_string_lossy().to_string(),
            ..Default::default()
        };

        let mut enclave = Enclave::new(config);
        let error = enclave.initialize().unwrap_err();

        assert!(matches!(error, EnclaveError::ProviderBootstrapFailed(_)));
        assert!(
            error
                .to_string()
                .contains("refusing to fall back to simulation")
        );

        let _ = std::fs::remove_dir_all(storage_path);
    }

    #[test]
    #[cfg(feature = "tee-hardware")]
    fn test_hardware_runtime_encrypt_decrypt_uses_runtime_proxy() {
        let storage_path = temp_sealed_storage_path("runtime-crypto");
        let runtime: SharedEnclaveRuntime = Arc::new(RuntimeCryptoMock);
        let config = EnclaveConfig {
            runtime_mode: TeeRuntimeMode::Hardware,
            sealed_storage_path: storage_path.to_string_lossy().to_string(),
            ..Default::default()
        };

        let mut enclave = Enclave::with_runtime(config, runtime);
        enclave.initialize().unwrap();

        let plaintext = b"runtime-only secret";
        let user_hash = hashed_user_id("runtime-user");
        let blob = enclave
            .encrypt_credential("tenant_1", &user_hash, "cred_1", plaintext)
            .unwrap();

        assert_eq!(blob.algorithm, "MOCK-RUNTIME");

        let decrypted = enclave
            .decrypt_credential("tenant_1", &user_hash, "cred_1", &blob)
            .unwrap();
        assert_eq!(decrypted, plaintext);

        let _ = enclave.shutdown();
        let _ = std::fs::remove_dir_all(storage_path);
    }

    #[test]
    fn test_simulation_mode_recovers_from_corrupt_sealed_master_key() {
        let storage_path = temp_sealed_storage_path("simulation-corrupt-sealed");
        let storage_path_str = storage_path.to_string_lossy().to_string();

        let config = EnclaveConfig {
            sealed_storage_path: storage_path_str.clone(),
            ..Default::default()
        };

        let key_manager = KeyManager::new(storage_path_str, [7u8; 32], [9u8; 32]);
        std::fs::create_dir_all(&storage_path).unwrap();
        std::fs::write(
            storage_path.join("enclave_master_key.sealed"),
            b"not-a-valid-sealed-payload",
        )
        .unwrap();

        assert!(key_manager.has_sealed_master_key());

        let mut enclave = Enclave::new(config);
        enclave.initialize().unwrap();

        assert!(enclave.is_running());
        assert_eq!(enclave.root_key_source(), Some(RootKeySource::Simulation));
        assert!(!key_manager.has_sealed_master_key());

        let _ = enclave.shutdown();
        let _ = std::fs::remove_dir_all(storage_path);
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
        let user_hash = hashed_user_id("user_1");
        let blob = enclave
            .encrypt_credential("tenant_1", &user_hash, "cred_1", plaintext)
            .unwrap();
        enclave
            .decrypt_credential("tenant_1", &user_hash, "cred_1", &blob)
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
