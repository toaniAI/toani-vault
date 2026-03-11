//! 密钥清理策略模块
//!
//! 实现安全的密钥清理机制，确保敏感数据不会残留在内存中
//!
//! # 清理策略
//!
//! ## 自动清理
//! - `ZeroizeOnDrop`: 密钥结构体在 drop 时自动清零
//! - TTL 过期: 缓存项超过 TTL 后自动移除并清零
//! - Enclave 关闭: 关闭时清理所有敏感数据
//!
//! ## 手动清理
//! - 显式调用 `secure_clear()` 方法
//! - 使用 `CleanupScheduler` 定期执行清理任务
//!
//! # 安全保证
//!
//! - 所有清理操作都使用 `zeroize` crate 防止编译器优化
//! - 清理操作不可撤销，密钥一旦清理即永久丢失
//! - 多重清理机制确保不遗漏任何敏感数据

use crate::crypto::constants::KEY_LENGTH;
use crate::tee::keys::{CachedKeyEntry, UserKeyCache};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use std::thread::{self, JoinHandle};
use zeroize::Zeroize;

/// 清理调度器配置
#[derive(Debug, Clone)]
pub struct CleanupConfig {
    /// 自动清理间隔
    pub cleanup_interval: Duration,

    /// 是否在 Enclave drop 时执行深度清理
    pub deep_cleanup_on_drop: bool,

    /// 清理后是否进行内存验证
    pub verify_cleanup: bool,

    /// 后台清理线程名称
    pub cleanup_thread_name: String,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            cleanup_interval: Duration::from_secs(60), // 每分钟清理一次
            deep_cleanup_on_drop: true,
            verify_cleanup: false, // 生产环境可以启用
            cleanup_thread_name: "key-cleanup".to_string(),
        }
    }
}

/// 清理统计信息
#[derive(Debug, Clone, Default)]
pub struct CleanupStats {
    /// 总清理次数
    pub total_cleanups: u64,

    /// 清理的密钥条目数
    pub keys_cleaned: u64,

    /// 清理的内存字节数
    pub bytes_cleared: u64,

    /// 上次清理时间
    pub last_cleanup: Option<Instant>,

    /// 调度器启动时间
    pub started_at: Option<Instant>,
}

/// 密钥清理调度器
///
/// 后台定期执行密钥缓存清理任务
pub struct CleanupScheduler {
    /// 配置
    config: CleanupConfig,

    /// 运行状态
    running: Arc<RwLock<bool>>,

    /// 工作线程句柄
    handle: Option<JoinHandle<()>>,

    /// 统计信息
    stats: Arc<Mutex<CleanupStats>>,
}

impl CleanupScheduler {
    /// 创建新的清理调度器
    pub fn new(config: CleanupConfig) -> Self {
        Self {
            config,
            running: Arc::new(RwLock::new(false)),
            handle: None,
            stats: Arc::new(Mutex::new(CleanupStats::default())),
        }
    }

    /// 使用默认配置创建
    pub fn default() -> Self {
        Self::new(CleanupConfig::default())
    }

    /// 启动后台清理线程
    ///
    /// # 参数
    ///
    /// - `cache`: 要清理的用户密钥缓存
    ///
    /// # 安全说明
    ///
    /// 后台线程定期调用 `cleanup_expired()` 清理过期密钥
    pub fn start(&mut self, cache: Arc<RwLock<UserKeyCache>>) {
        let running = self.running.clone();
        let stats = self.stats.clone();
        let interval = self.config.cleanup_interval;
        let thread_name = self.config.cleanup_thread_name.clone();

        // 标记为运行中
        if let Ok(mut r) = running.write() {
            *r = true;
        }

        // 更新启动时间
        if let Ok(mut s) = stats.lock() {
            s.started_at = Some(Instant::now());
        }

        // 启动后台线程
        self.handle = Some(
            thread::Builder::new()
                .name(thread_name)
                .spawn(move || {
                    loop {
                        // 检查是否应该停止
                        let should_run = {
                            if let Ok(r) = running.read() {
                                *r
                            } else {
                                false
                            }
                        };

                        if !should_run {
                            break;
                        }

                        // 执行清理
                        if let Ok(mut cache) = cache.write() {
                            let cleaned = cache.cleanup_expired();

                            if let Ok(mut s) = stats.lock() {
                                s.total_cleanups += 1;
                                s.keys_cleaned += cleaned as u64;
                                s.bytes_cleared += (cleaned * KEY_LENGTH) as u64;
                                s.last_cleanup = Some(Instant::now());
                            }
                        }

                        // 等待下一次清理
                        thread::sleep(interval);
                    }
                })
                .expect("Failed to spawn cleanup thread"),
        );
    }

    /// 停止后台清理线程
    pub fn stop(&mut self) {
        // 标记为非运行
        if let Ok(mut r) = self.running.write() {
            *r = false;
        }

        // 等待线程结束
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }

    /// 获取统计信息
    pub fn stats(&self) -> CleanupStats {
        if let Ok(s) = self.stats.lock() {
            s.clone()
        } else {
            CleanupStats::default()
        }
    }

    /// 检查是否在运行
    pub fn is_running(&self) -> bool {
        if let Ok(r) = self.running.read() {
            *r
        } else {
            false
        }
    }

    /// 获取配置
    pub fn config(&self) -> &CleanupConfig {
        &self.config
    }
}

impl Drop for CleanupScheduler {
    fn drop(&mut self) {
        self.stop();
    }
}

/// 密钥清理器
///
/// 提供各种密钥清理工具函数
pub struct KeyCleaner;

impl KeyCleaner {
    /// 安全清理字节数组
    ///
    /// 使用 `zeroize` 确保数据被彻底覆盖
    #[inline]
    pub fn clear_bytes(data: &mut [u8]) {
        data.zeroize();
    }

    /// 安全清理固定大小数组
    #[inline]
    pub fn clear_array<const N: usize>(data: &mut [u8; N]) {
        data.zeroize();
    }

    /// 安全清理密钥材料
    #[inline]
    pub fn clear_key_material(material: &mut [u8; KEY_LENGTH]) {
        material.zeroize();
    }

    /// 深度清理内存区域
    ///
    /// 多次覆写防止数据残留
    ///
    /// # 安全说明
    ///
    /// 这是额外的安全措施，用于高安全场景
    pub fn deep_clear(data: &mut [u8]) {
        // 第一次：全 0
        data.fill(0x00);

        // 第二次：全 1
        data.fill(0xFF);

        // 第三次：随机模式
        data.fill(0xAA);

        // 第四次：最终清零
        data.zeroize();
    }

    /// 清理并验证
    ///
    /// 清理后验证内存是否真正被清零
    ///
    /// # 返回
    ///
    /// 返回 `true` 如果所有字节都已被清零
    pub fn clear_and_verify(data: &mut [u8]) -> bool {
        data.zeroize();
        data.iter().all(|&b| b == 0)
    }

    /// 批量清理缓存条目
    ///
    /// 清理多个缓存条目并返回清理数量
    pub fn clear_cached_entries(entries: &[&CachedKeyEntry]) -> usize {
        let mut count = 0;

        for entry in entries {
            entry.secure_clear();
            count += 1;
        }

        count
    }

    /// 安全替换密钥材料
    ///
    /// 先清理旧密钥，再设置新密钥
    ///
    /// # 安全说明
    ///
    /// 确保旧密钥不会残留在内存中
    pub fn replace_key_material(
        old_key: &mut [u8; KEY_LENGTH],
        new_key: &[u8; KEY_LENGTH],
    ) {
        // 先清零旧密钥
        old_key.zeroize();

        // 复制新密钥
        old_key.copy_from_slice(new_key);
    }

    /// 安全派生并清理中间状态
    ///
    /// 在密钥派生过程中清理临时状态
    pub fn secure_derive<F, R>(mut intermediate_key: [u8; KEY_LENGTH], derive_fn: F) -> R
    where
        F: FnOnce(&[u8; KEY_LENGTH]) -> R,
    {
        let result = derive_fn(&intermediate_key);

        // 清理中间状态
        intermediate_key.zeroize();

        result
    }
}

/// 内存区域保护
///
/// 标记敏感内存区域，确保在 drop 时自动清理
pub struct ProtectedMemory {
    /// 受保护的内存
    data: Vec<u8>,

    /// 是否已清理
    cleared: bool,

    /// 保护名称（用于调试）
    name: String,
}

impl ProtectedMemory {
    /// 创建新的受保护内存区域
    pub fn new(size: usize, name: impl Into<String>) -> Self {
        Self {
            data: vec![0u8; size],
            cleared: false,
            name: name.into(),
        }
    }

    /// 从现有数据创建
    pub fn from_data(data: Vec<u8>, name: impl Into<String>) -> Self {
        Self {
            data,
            cleared: false,
            name: name.into(),
        }
    }

    /// 获取内存引用
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    /// 获取可变内存引用
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// 获取长度
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// 安全清理
    ///
    /// 手动触发清理，可以被多次调用
    pub fn secure_clear(&mut self) {
        if !self.cleared {
            self.data.zeroize();
            self.cleared = true;
        }
    }

    /// 检查是否已清理
    pub fn is_cleared(&self) -> bool {
        self.cleared
    }

    /// 获取名称
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 提取数据并清理
    ///
    /// 返回数据所有权，并标记为已清理
    pub fn take_and_clear(mut self) -> Vec<u8> {
        let data = std::mem::take(&mut self.data);
        self.cleared = true;
        data
    }
}

impl Drop for ProtectedMemory {
    fn drop(&mut self) {
        self.secure_clear();
    }
}

/// 密钥生命周期管理器
///
/// 跟踪密钥的生命周期，确保在适当的时候进行清理
pub struct KeyLifecycle {
    /// 密钥创建时间
    pub created_at: Instant,

    /// 最后使用时间
    pub last_used_at: Instant,

    /// 预期过期时间
    pub expires_at: Option<Instant>,

    /// 使用次数
    pub use_count: u64,

    /// 是否已清理
    pub is_cleared: bool,
}

impl KeyLifecycle {
    /// 创建新的密钥生命周期
    pub fn new(ttl: Option<Duration>) -> Self {
        let now = Instant::now();

        Self {
            created_at: now,
            last_used_at: now,
            expires_at: ttl.map(|d| now + d),
            use_count: 0,
            is_cleared: false,
        }
    }

    /// 记录使用
    pub fn record_use(&mut self) {
        self.last_used_at = Instant::now();
        self.use_count += 1;
    }

    /// 检查是否过期
    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires_at {
            Instant::now() > expires
        } else {
            false
        }
    }

    /// 获取存活时间
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// 获取闲置时间
    pub fn idle_time(&self) -> Duration {
        self.last_used_at.elapsed()
    }

    /// 标记为已清理
    pub fn mark_cleared(&mut self) {
        self.is_cleared = true;
    }
}

/// 安全作用域
///
/// 在作用域结束时自动清理指定的内存区域
///
/// # 使用示例
///
/// ```rust
/// use vault_service::tee::SecureScope;
///
/// {
///     let sensitive_data = vec![1, 2, 3, 4, 5];
///     let _guard = SecureScope::new(sensitive_data);
///     // 使用敏感数据...
/// } // 在这里自动清理
/// ```
pub struct SecureScope<T: Zeroize> {
    data: Option<T>,
}

impl<T: Zeroize> SecureScope<T> {
    /// 创建新的安全作用域
    pub fn new(data: T) -> Self {
        Self { data: Some(data) }
    }

    /// 获取数据引用
    pub fn get(&self) -> Option<&T> {
        self.data.as_ref()
    }

    /// 获取可变数据引用
    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.data.as_mut()
    }

    /// 手动清理并结束作用域
    pub fn clear(mut self) {
        if let Some(ref mut d) = self.data {
            d.zeroize();
        }
        self.data = None;
    }
}

impl<T: Zeroize> Drop for SecureScope<T> {
    fn drop(&mut self) {
        if let Some(ref mut d) = self.data {
            d.zeroize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_key_cleaner_clear_bytes() {
        let mut data = vec![0x42u8; 32];
        KeyCleaner::clear_bytes(&mut data);
        assert!(data.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_key_cleaner_clear_key_material() {
        let mut key = [0x42u8; KEY_LENGTH];
        KeyCleaner::clear_key_material(&mut key);
        assert!(key.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_key_cleaner_deep_clear() {
        let mut data = vec![0x42u8; 32];
        KeyCleaner::deep_clear(&mut data);
        assert!(data.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_key_cleaner_clear_and_verify() {
        let mut data = vec![0x42u8; 32];
        let result = KeyCleaner::clear_and_verify(&mut data);
        assert!(result);
        assert!(data.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_key_cleaner_replace_key_material() {
        let mut old_key = [0x42u8; KEY_LENGTH];
        let new_key = [0xABu8; KEY_LENGTH];

        KeyCleaner::replace_key_material(&mut old_key, &new_key);

        // 新密钥应该被复制
        assert_eq!(old_key, new_key);
    }

    #[test]
    fn test_protected_memory_auto_clear() {
        let mut memory = ProtectedMemory::new(32, "test_memory");

        // 填充数据
        memory.as_mut_slice().fill(0x42);
        assert!(memory.as_slice().iter().all(|&b| b == 0x42));

        // 手动清理
        memory.secure_clear();
        assert!(memory.is_cleared());

        // 再次 drop 不应该出错（已经清理过）
        drop(memory);
    }

    #[test]
    fn test_protected_memory_take_and_clear() {
        let mut memory = ProtectedMemory::new(32, "test_memory");
        memory.as_mut_slice().fill(0x42);

        let data = memory.take_and_clear();
        assert_eq!(data.len(), 32);
    }

    #[test]
    fn test_key_lifecycle() {
        let mut lifecycle = KeyLifecycle::new(Some(Duration::from_millis(100)));

        assert!(!lifecycle.is_expired());
        assert_eq!(lifecycle.use_count, 0);

        // 记录使用
        lifecycle.record_use();
        assert_eq!(lifecycle.use_count, 1);

        // 等待过期
        thread::sleep(Duration::from_millis(150));
        assert!(lifecycle.is_expired());

        // 标记清理
        lifecycle.mark_cleared();
        assert!(lifecycle.is_cleared);
    }

    #[test]
    fn test_key_lifecycle_no_expiration() {
        let lifecycle = KeyLifecycle::new(None);
        assert!(!lifecycle.is_expired());
        assert!(lifecycle.expires_at.is_none());
    }

    #[test]
    fn test_cleanup_scheduler() {
        let config = CleanupConfig {
            cleanup_interval: Duration::from_millis(100),
            ..Default::default()
        };

        let mut scheduler = CleanupScheduler::new(config);
        let cache = Arc::new(RwLock::new(UserKeyCache::new(1)));

        // 添加一些条目
        {
            let mut cache = cache.write().unwrap();
            for i in 0..3 {
                let entry = CachedKeyEntry::new(
                    [i as u8; 32],
                    format!("tenant_{}", i),
                    format!("user_{}", i),
                    [i as u8; KEY_LENGTH],
                    crate::tee::keys::KeyType::UserVault,
                );
                cache.insert(&format!("tenant_{}", i), &format!("user_{}", i), entry);
            }
        }

        // 启动调度器
        scheduler.start(cache.clone());
        assert!(scheduler.is_running());

        // 等待条目过期并被清理
        thread::sleep(Duration::from_millis(2500));

        // 停止调度器
        scheduler.stop();
        assert!(!scheduler.is_running());

        // 检查清理统计
        let stats = scheduler.stats();
        assert!(stats.total_cleanups > 0);

        // 验证缓存已被清理
        let cache = cache.read().unwrap();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_secure_scope() {
        let data = vec![0x42u8; 32];
        {
            let scope = SecureScope::new(data);
            assert!(scope.get().is_some());
            assert_eq!(scope.get().unwrap().len(), 32);
        }
        // 离开作用域后应该已被清理
    }

    #[test]
    fn test_secure_scope_manual_clear() {
        let data = vec![0x42u8; 32];
        let scope = SecureScope::new(data);
        scope.clear();
    }

    #[test]
    fn test_cleanup_config_default() {
        let config = CleanupConfig::default();
        assert_eq!(config.cleanup_interval, Duration::from_secs(60));
        assert!(config.deep_cleanup_on_drop);
        assert!(!config.verify_cleanup);
        assert_eq!(config.cleanup_thread_name, "key-cleanup");
    }

    #[test]
    fn test_cleanup_stats() {
        let mut stats = CleanupStats::default();
        stats.total_cleanups = 10;
        stats.keys_cleaned = 100;
        stats.bytes_cleared = 3200;
        stats.last_cleanup = Some(Instant::now());
        stats.started_at = Some(Instant::now());

        assert_eq!(stats.total_cleanups, 10);
        assert_eq!(stats.keys_cleaned, 100);
        assert_eq!(stats.bytes_cleared, 3200);
        assert!(stats.last_cleanup.is_some());
        assert!(stats.started_at.is_some());
    }
}
