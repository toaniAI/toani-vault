//! 密钥轮换策略模块
//!
//! 实现自动化密钥轮换机制：
//! - L2 密钥：每 90 天轮换
//! - L3 密钥：每次加密轮换
//!
//! # 轮换策略
//!
//! | 密钥级别 | 轮换周期 | 触发条件 |
//! |---------|---------|---------|
//! | L1 | 手动/紧急 | 安全事件 |
//! | L2 | 90 天 | 定时轮换 |
//! | L3 | 每次加密 | 每密钥轮换 |
//!
//! # 安全特性
//!
//! - 密钥版本追踪
//! - 平滑过渡期（新旧密钥并存）
//! - 自动过期清理
//! - 轮换审计日志

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::sync::RwLock;

use super::constant_time::ct_compare;
use super::hkdf::KeyVersion;

// 确保 KeyVersion 的序列化特性可用

/// 轮换错误类型
#[derive(Error, Debug)]
pub enum KeyRotationError {
    #[error("密钥不存在：{0}")]
    KeyNotFound(String),

    #[error("轮换策略冲突：{0}")]
    RotationPolicyConflict(String),

    #[error("密钥版本不兼容：{0}")]
    VersionIncompatible(String),

    #[error("轮换超时：{0}")]
    RotationTimeout(String),

    #[error("密钥派生失败：{0}")]
    DerivationError(String),

    #[error("无效的密钥状态：{0}")]
    InvalidKeyState(String),

    #[error("并发轮换冲突")]
    ConcurrentRotationConflict,
}

/// 轮换结果
#[derive(Debug, Clone, PartialEq)]
pub enum RotationResult {
    /// 轮换成功
    Success,
    /// 轮换成功，旧密钥已归档
    SuccessWithArchival,
    /// 轮换失败
    Failed,
    /// 轮换被跳过（未到轮换时间）
    Skipped,
}

/// 密钥状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum KeyState {
    /// 活跃（可用于加密和解密）
    Active = 0,
    /// 仅解密（新加密使用新密钥）
    DecryptOnly = 1,
    /// 已归档（保留用于历史数据解密）
    Archived = 2,
    /// 已撤销（不再使用）
    Revoked = 3,
    /// 待删除（等待安全清理）
    PendingDeletion = 4,
}

impl KeyState {
    /// 从 u8 转换
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Active),
            1 => Some(Self::DecryptOnly),
            2 => Some(Self::Archived),
            3 => Some(Self::Revoked),
            4 => Some(Self::PendingDeletion),
            _ => None,
        }
    }

    /// 检查是否可用于加密
    pub fn can_encrypt(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// 检查是否可用于解密
    pub fn can_decrypt(&self) -> bool {
        matches!(self, Self::Active | Self::DecryptOnly | Self::Archived)
    }
}

/// 密钥轮换策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RotationPolicy {
    /// L2 密钥：定时轮换（90 天）
    L2Scheduled {
        /// 轮换间隔（秒）
        interval_secs: u64,
        /// 最后轮换时间
        last_rotation: u64,
    },
    /// L3 密钥：每密钥轮换
    L3PerEncryption,
    /// L1 密钥：手动/紧急轮换
    L1Manual {
        /// 是否紧急轮换
        is_emergency: bool,
    },
}

impl RotationPolicy {
    /// 检查是否需要轮换
    pub fn needs_rotation(&self, current_time: u64) -> bool {
        match self {
            Self::L2Scheduled {
                interval_secs,
                last_rotation,
            } => current_time.saturating_sub(*last_rotation) >= *interval_secs,
            Self::L3PerEncryption => true,  // 总是需要轮换
            Self::L1Manual { .. } => false, // 手动触发
        }
    }

    /// 获取 L2 轮换间隔（秒）
    pub const L2_DEFAULT_INTERVAL_SECS: u64 = 90 * 24 * 60 * 60; // 90 天
}

/// 密钥元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyMetadata {
    /// 密钥 ID
    pub key_id: String,
    /// 密钥版本
    pub version: KeyVersion,
    /// 密钥状态
    pub state: KeyState,
    /// 创建时间
    pub created_at: u64,
    /// 最后使用时间
    pub last_used_at: u64,
    /// 轮换策略
    pub rotation_policy: RotationPolicy,
    /// 过期时间（可选）
    pub expires_at: Option<u64>,
    /// 租户 ID
    pub tenant_id: String,
    /// 用户 ID（L2/L3 密钥）
    pub user_id: Option<String>,
}

impl KeyMetadata {
    /// 创建新的密钥元数据
    pub fn new(key_id: String, tenant_id: String, rotation_policy: RotationPolicy) -> Self {
        let now = current_timestamp();
        Self {
            key_id,
            version: KeyVersion::current(),
            state: KeyState::Active,
            created_at: now,
            last_used_at: now,
            rotation_policy,
            expires_at: None,
            tenant_id,
            user_id: None,
        }
    }

    /// 设置用户 ID
    pub fn with_user_id(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// 设置过期时间
    pub fn with_expiry(mut self, expires_at: u64) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// 检查密钥是否已过期
    pub fn is_expired(&self, current_time: u64) -> bool {
        self.expires_at
            .map(|exp| current_time >= exp)
            .unwrap_or(false)
    }

    /// 检查是否需要轮换
    pub fn needs_rotation(&self, current_time: u64) -> bool {
        if self.is_expired(current_time) {
            return true;
        }

        self.rotation_policy.needs_rotation(current_time)
    }

    /// 更新最后使用时间
    pub fn update_last_used(&mut self) {
        self.last_used_at = current_timestamp();
    }

    /// 转换密钥状态
    pub fn transition_state(&mut self, new_state: KeyState) -> Result<(), KeyRotationError> {
        // 验证状态转换合法性
        if !self.is_valid_transition(self.state, new_state) {
            return Err(KeyRotationError::InvalidKeyState(format!(
                "非法的状态转换：{:?} -> {:?}",
                self.state, new_state
            )));
        }

        self.state = new_state;
        Ok(())
    }

    /// 检查状态转换是否合法
    fn is_valid_transition(&self, from: KeyState, to: KeyState) -> bool {
        matches!(
            (from, to),
            (KeyState::Active, KeyState::DecryptOnly)
                | (KeyState::Active, KeyState::Archived)
                | (KeyState::Active, KeyState::Revoked)
                | (KeyState::DecryptOnly, KeyState::Archived)
                | (KeyState::DecryptOnly, KeyState::Revoked)
                | (KeyState::Archived, KeyState::PendingDeletion)
                | (KeyState::Revoked, KeyState::PendingDeletion)
        )
    }
}

/// 密钥轮换管理器
pub struct KeyRotationManager {
    /// 密钥元数据存储
    key_metadata: RwLock<HashMap<String, KeyMetadata>>,
    /// 当前活跃密钥 ID
    active_key_id: RwLock<Option<String>>,
    /// 轮换中的密钥（防止并发轮换）
    rotating_keys: RwLock<HashMap<String, u64>>,
    /// 轮换统计
    stats: RotationStats,
    /// 配置
    config: RotationConfig,
}

/// 轮换配置
#[derive(Debug, Clone)]
pub struct RotationConfig {
    /// L2 密钥轮换间隔（秒）
    pub l2_rotation_interval_secs: u64,
    /// L3 密钥是否启用每加密轮换
    pub l3_per_encryption: bool,
    /// 密钥归档保留期（秒）
    pub archival_retention_secs: u64,
    /// 自动清理过期密钥
    pub auto_cleanup_expired: bool,
    /// 最大历史密钥数量
    pub max_historical_keys: usize,
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            l2_rotation_interval_secs: RotationPolicy::L2_DEFAULT_INTERVAL_SECS,
            l3_per_encryption: true,
            archival_retention_secs: 365 * 24 * 60 * 60, // 1 年
            auto_cleanup_expired: true,
            max_historical_keys: 5,
        }
    }
}

/// 轮换统计
#[derive(Debug, Default)]
pub struct RotationStats {
    /// 总轮换次数
    pub total_rotations: AtomicU64,
    /// 成功轮换次数
    pub successful_rotations: AtomicU64,
    /// 失败轮换次数
    pub failed_rotations: AtomicU64,
    /// 当前活跃密钥数
    pub active_keys: AtomicUsize,
    /// 归档密钥数
    pub archived_keys: AtomicUsize,
}

impl RotationStats {
    /// 记录轮换成功
    pub fn record_success(&self) {
        self.total_rotations.fetch_add(1, Ordering::SeqCst);
        self.successful_rotations.fetch_add(1, Ordering::SeqCst);
    }

    /// 记录轮换失败
    pub fn record_failure(&self) {
        self.total_rotations.fetch_add(1, Ordering::SeqCst);
        self.failed_rotations.fetch_add(1, Ordering::SeqCst);
    }

    /// 更新活跃密钥数
    pub fn update_active_keys(&self, count: usize) {
        self.active_keys.store(count, Ordering::SeqCst);
    }

    /// 更新归档密钥数
    pub fn update_archived_keys(&self, count: usize) {
        self.archived_keys.store(count, Ordering::SeqCst);
    }

    /// 获取统计快照
    pub fn snapshot(&self) -> StatsSnapshot {
        StatsSnapshot {
            total_rotations: self.total_rotations.load(Ordering::SeqCst),
            successful_rotations: self.successful_rotations.load(Ordering::SeqCst),
            failed_rotations: self.failed_rotations.load(Ordering::SeqCst),
            active_keys: self.active_keys.load(Ordering::SeqCst),
            archived_keys: self.archived_keys.load(Ordering::SeqCst),
        }
    }
}

/// 统计快照
#[derive(Debug, Clone)]
pub struct StatsSnapshot {
    pub total_rotations: u64,
    pub successful_rotations: u64,
    pub failed_rotations: u64,
    pub active_keys: usize,
    pub archived_keys: usize,
}

impl KeyRotationManager {
    /// 创建新的轮换管理器
    pub fn new(config: RotationConfig) -> Self {
        Self {
            key_metadata: RwLock::new(HashMap::new()),
            active_key_id: RwLock::new(None),
            rotating_keys: RwLock::new(HashMap::new()),
            stats: RotationStats::default(),
            config,
        }
    }

    /// 注册新密钥
    pub async fn register_key(&self, metadata: KeyMetadata) -> Result<(), KeyRotationError> {
        // 在独立作用域中执行写操作，确保在调用 update_stats 前释放写锁
        {
            let mut keys = self.key_metadata.write().await;

            if metadata.state == KeyState::Active {
                // 设置为首个活跃密钥
                *self.active_key_id.write().await = Some(metadata.key_id.clone());
            }

            keys.insert(metadata.key_id.clone(), metadata);
        }
        self.update_stats().await;

        Ok(())
    }

    /// 获取密钥元数据
    pub async fn get_key_metadata(&self, key_id: &str) -> Option<KeyMetadata> {
        let keys = self.key_metadata.read().await;
        keys.get(key_id).cloned()
    }

    /// 获取活跃密钥
    pub async fn get_active_key(&self) -> Option<KeyMetadata> {
        let active_id = self.active_key_id.read().await.clone()?;
        let keys = self.key_metadata.read().await;
        keys.get(&active_id).cloned()
    }

    /// 执行 L2 密钥轮换
    ///
    /// # 轮换流程
    ///
    /// 1. 检查是否需要轮换
    /// 2. 创建新版本密钥
    /// 3. 旧密钥转为 DecryptOnly
    /// 4. 新密钥设为 Active
    /// 5. 更新轮换时间
    ///
    /// # 返回
    /// 轮换结果
    pub async fn rotate_l2_key(
        &self,
        tenant_id: &str,
        user_id: &str,
    ) -> Result<RotationResult, KeyRotationError> {
        let current_time = current_timestamp();

        // 查找当前活跃密钥（使用独立作用域确保读锁及时释放）
        let needs_rotation = {
            let keys = self.key_metadata.read().await;
            let current_key = keys.values().find(|k| {
                k.tenant_id == tenant_id
                    && k.user_id.as_deref() == Some(user_id)
                    && k.state == KeyState::Active
                    && matches!(k.rotation_policy, RotationPolicy::L2Scheduled { .. })
            });

            if let Some(current_key) = current_key {
                // 检查是否需要轮换
                current_key.needs_rotation(current_time)
            } else {
                // 没有活跃密钥，需要创建新密钥
                true
            }
        };

        if !needs_rotation {
            return Ok(RotationResult::Skipped);
        }

        // 检查并发轮换
        let rotation_key = format!("{tenant_id}:{user_id}");
        if self
            .rotating_keys
            .write()
            .await
            .insert(rotation_key.clone(), current_time)
            .is_some()
        {
            return Err(KeyRotationError::ConcurrentRotationConflict);
        }

        // 执行轮换
        let result = self
            .perform_l2_rotation(tenant_id, user_id, current_time)
            .await;

        // 清理轮换标记
        self.rotating_keys.write().await.remove(&rotation_key);

        // 更新统计
        match &result {
            Ok(RotationResult::Success) | Ok(RotationResult::SuccessWithArchival) => {
                self.stats.record_success();
            }
            Ok(_) => {}
            Err(_) => self.stats.record_failure(),
        }

        self.update_stats().await;

        result
    }

    /// 执行 L2 轮换核心逻辑
    async fn perform_l2_rotation(
        &self,
        tenant_id: &str,
        user_id: &str,
        current_time: u64,
    ) -> Result<RotationResult, KeyRotationError> {
        // 在独立作用域中执行所有写操作，确保在调用 update_stats 前释放写锁
        {
            let mut keys = self.key_metadata.write().await;

            // 查找并更新旧密钥状态
            for (_, key) in keys.iter_mut() {
                if key.tenant_id == tenant_id
                    && key.user_id.as_deref() == Some(user_id)
                    && key.state == KeyState::Active
                {
                    key.transition_state(KeyState::DecryptOnly)?;
                }
            }

            // 创建新密钥
            let new_key_id = format!("l2_{tenant_id}_{user_id}_v{current_time}");
            let mut new_key = KeyMetadata::new(
                new_key_id.clone(),
                tenant_id.to_string(),
                RotationPolicy::L2Scheduled {
                    interval_secs: self.config.l2_rotation_interval_secs,
                    last_rotation: current_time,
                },
            )
            .with_user_id(user_id.to_string());

            // 设置过期时间
            new_key.expires_at = Some(current_time + self.config.l2_rotation_interval_secs);

            // 注册新密钥
            keys.insert(new_key_id.clone(), new_key);

            // 更新活跃密钥
            *self.active_key_id.write().await = Some(new_key_id);

            // 清理过期归档密钥
            self.cleanup_archived_keys(&mut keys, tenant_id, user_id)
                .await;
        }

        self.update_stats().await;

        Ok(RotationResult::SuccessWithArchival)
    }

    /// L3 密钥轮换（每次加密）
    pub async fn rotate_l3_key(
        &self,
        tenant_id: &str,
        user_id: &str,
        credential_id: &str,
    ) -> Result<String, KeyRotationError> {
        let current_time = current_timestamp();

        // 创建新的 L3 密钥
        let new_key_id = format!("l3_{tenant_id}_{user_id}_{credential_id}_{current_time}");

        let new_key = KeyMetadata::new(
            new_key_id.clone(),
            tenant_id.to_string(),
            RotationPolicy::L3PerEncryption,
        )
        .with_user_id(user_id.to_string())
        .with_expiry(current_time + 3600); // 1 小时 TTL

        // 注册新密钥
        self.register_key(new_key).await?;

        Ok(new_key_id)
    }

    /// 清理归档密钥
    async fn cleanup_archived_keys(
        &self,
        keys: &mut HashMap<String, KeyMetadata>,
        tenant_id: &str,
        user_id: &str,
    ) {
        let current_time = current_timestamp();
        let retention_cutoff = current_time.saturating_sub(self.config.archival_retention_secs);

        let mut to_remove = Vec::new();

        for (key_id, key) in keys.iter() {
            if key.tenant_id == tenant_id
                && key.user_id.as_deref() == Some(user_id)
                && key.state == KeyState::Archived
                && key.created_at < retention_cutoff
            {
                to_remove.push(key_id.clone());
            }
        }

        // 限制保留的历史密钥数量
        let mut historical_keys: Vec<_> = keys
            .iter()
            .filter(|(_, k)| {
                k.tenant_id == tenant_id
                    && k.user_id.as_deref() == Some(user_id)
                    && k.state == KeyState::Archived
            })
            .collect();

        historical_keys.sort_by(|a, b| b.1.created_at.cmp(&a.1.created_at));

        if historical_keys.len() > self.config.max_historical_keys {
            for (_, key) in historical_keys
                .into_iter()
                .skip(self.config.max_historical_keys)
            {
                to_remove.push(key.key_id.clone());
            }
        }

        // 删除密钥
        for key_id in to_remove {
            keys.remove(&key_id);
        }
    }

    /// 撤销密钥
    pub async fn revoke_key(&self, key_id: &str) -> Result<(), KeyRotationError> {
        // 在独立作用域中执行写操作，确保在调用 update_stats 前释放写锁
        {
            let mut keys = self.key_metadata.write().await;

            // 先获取必要信息，避免 borrow 冲突
            let key_info = keys
                .get(key_id)
                .map(|k| (k.tenant_id.clone(), k.user_id.clone(), k.state))
                .ok_or_else(|| KeyRotationError::KeyNotFound(key_id.to_string()))?;

            // 更新状态
            let key = keys.get_mut(key_id).unwrap();
            key.transition_state(KeyState::Revoked)?;

            // 如果是活跃密钥，需要选择新的活跃密钥
            if key_info.2 == KeyState::Active {
                let new_active = keys.values().find(|k| {
                    k.tenant_id == key_info.0
                        && k.user_id == key_info.1
                        && k.state == KeyState::DecryptOnly
                });

                if let Some(new_active_key) = new_active {
                    *self.active_key_id.write().await = Some(new_active_key.key_id.clone());
                }
            }
        }

        self.update_stats().await;

        Ok(())
    }

    /// 获取轮换统计
    pub fn get_stats(&self) -> StatsSnapshot {
        self.stats.snapshot()
    }

    /// 更新统计
    async fn update_stats(&self) {
        let keys = self.key_metadata.read().await;

        let active_count = keys
            .values()
            .filter(|k| k.state == KeyState::Active)
            .count();

        let archived_count = keys
            .values()
            .filter(|k| k.state == KeyState::Archived)
            .count();

        self.stats.update_active_keys(active_count);
        self.stats.update_archived_keys(archived_count);
    }

    /// 检查所有密钥的轮换状态
    pub async fn check_all_keys_rotation(&self) -> Vec<String> {
        let current_time = current_timestamp();
        let keys = self.key_metadata.read().await;

        keys.values()
            .filter(|k| k.needs_rotation(current_time) && k.state == KeyState::Active)
            .map(|k| k.key_id.clone())
            .collect()
    }
}

/// 获取当前时间戳
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间错误")
        .as_secs()
}

/// 安全比较密钥 ID
pub fn secure_compare_key_ids(a: &str, b: &str) -> bool {
    ct_compare(a.as_bytes(), b.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_key_metadata_creation() {
        let metadata = KeyMetadata::new(
            "test_key".to_string(),
            "tenant_123".to_string(),
            RotationPolicy::L2Scheduled {
                interval_secs: 90 * 24 * 60 * 60,
                last_rotation: current_timestamp(),
            },
        );

        assert_eq!(metadata.key_id, "test_key");
        assert_eq!(metadata.state, KeyState::Active);
        assert!(
            metadata
                .rotation_policy
                .needs_rotation(current_timestamp() + 91 * 24 * 60 * 60)
        );
    }

    #[tokio::test]
    async fn test_key_state_transitions() {
        let mut metadata = KeyMetadata::new(
            "test_key".to_string(),
            "tenant_123".to_string(),
            RotationPolicy::L2Scheduled {
                interval_secs: 90 * 24 * 60 * 60,
                last_rotation: current_timestamp(),
            },
        );

        // 合法转换
        assert!(metadata.transition_state(KeyState::DecryptOnly).is_ok());
        assert!(metadata.transition_state(KeyState::Archived).is_ok());

        // 非法转换
        assert!(metadata.transition_state(KeyState::Active).is_err());
    }

    #[tokio::test]
    async fn test_key_expiry() {
        let current_time = current_timestamp();
        let metadata = KeyMetadata::new(
            "test_key".to_string(),
            "tenant_123".to_string(),
            RotationPolicy::L2Scheduled {
                interval_secs: 90 * 24 * 60 * 60,
                last_rotation: current_time,
            },
        )
        .with_expiry(current_time + 3600);

        assert!(!metadata.is_expired(current_time));
        assert!(metadata.is_expired(current_time + 7200));
    }

    #[tokio::test]
    async fn test_rotation_manager_creation() {
        let config = RotationConfig::default();
        let manager = KeyRotationManager::new(config);

        assert_eq!(manager.get_stats().total_rotations, 0);
    }

    #[tokio::test]
    async fn test_register_and_get_key() {
        let manager = KeyRotationManager::new(RotationConfig::default());

        let metadata = KeyMetadata::new(
            "test_key".to_string(),
            "tenant_123".to_string(),
            RotationPolicy::L2Scheduled {
                interval_secs: 90 * 24 * 60 * 60,
                last_rotation: current_timestamp(),
            },
        );

        manager.register_key(metadata.clone()).await.unwrap();

        let retrieved = manager.get_key_metadata("test_key").await.unwrap();
        assert_eq!(retrieved.key_id, "test_key");
    }

    #[tokio::test]
    async fn test_l2_key_rotation() {
        let manager = KeyRotationManager::new(RotationConfig::default());

        // 注册一个需要轮换的密钥（设置最后轮换时间为 91 天前）
        let old_time = current_timestamp() - (91 * 24 * 60 * 60);
        let metadata = KeyMetadata::new(
            "old_key".to_string(),
            "tenant_123".to_string(),
            RotationPolicy::L2Scheduled {
                interval_secs: 90 * 24 * 60 * 60,
                last_rotation: old_time,
            },
        )
        .with_user_id("user_456".to_string());

        manager.register_key(metadata).await.unwrap();

        // 执行轮换
        let result = manager
            .rotate_l2_key("tenant_123", "user_456")
            .await
            .unwrap();
        assert!(matches!(result, RotationResult::SuccessWithArchival));

        // 验证有新密钥
        let stats = manager.get_stats();
        assert!(stats.successful_rotations > 0);
    }

    #[tokio::test]
    async fn test_l2_rotation_skipped() {
        let manager = KeyRotationManager::new(RotationConfig::default());

        // 注册一个不需要轮换的密钥
        let metadata = KeyMetadata::new(
            "fresh_key".to_string(),
            "tenant_123".to_string(),
            RotationPolicy::L2Scheduled {
                interval_secs: 90 * 24 * 60 * 60,
                last_rotation: current_timestamp(),
            },
        )
        .with_user_id("user_456".to_string());

        manager.register_key(metadata).await.unwrap();

        // 执行轮换（应该被跳过）
        let result = manager
            .rotate_l2_key("tenant_123", "user_456")
            .await
            .unwrap();
        assert!(matches!(result, RotationResult::Skipped));
    }

    #[tokio::test]
    async fn test_l3_key_rotation() {
        let manager = KeyRotationManager::new(RotationConfig::default());

        let key_id = manager
            .rotate_l3_key("tenant_123", "user_456", "credential_789")
            .await
            .unwrap();

        assert!(key_id.starts_with("l3_tenant_123_user_456_credential_789_"));

        // 验证密钥已注册
        let metadata = manager.get_key_metadata(&key_id).await.unwrap();
        assert_eq!(metadata.state, KeyState::Active);
    }

    #[tokio::test]
    async fn test_key_revocation() {
        let manager = KeyRotationManager::new(RotationConfig::default());

        let metadata = KeyMetadata::new(
            "revoke_test_key".to_string(),
            "tenant_123".to_string(),
            RotationPolicy::L2Scheduled {
                interval_secs: 90 * 24 * 60 * 60,
                last_rotation: current_timestamp(),
            },
        );

        manager.register_key(metadata).await.unwrap();

        // 撤销密钥
        manager.revoke_key("revoke_test_key").await.unwrap();

        // 验证状态
        let retrieved = manager.get_key_metadata("revoke_test_key").await.unwrap();
        assert_eq!(retrieved.state, KeyState::Revoked);
    }

    #[tokio::test]
    async fn test_concurrent_rotation_conflict() {
        let manager = KeyRotationManager::new(RotationConfig::default());

        let old_time = current_timestamp() - (91 * 24 * 60 * 60);
        let metadata = KeyMetadata::new(
            "concurrent_test".to_string(),
            "tenant_123".to_string(),
            RotationPolicy::L2Scheduled {
                interval_secs: 90 * 24 * 60 * 60,
                last_rotation: old_time,
            },
        )
        .with_user_id("user_456".to_string());

        manager.register_key(metadata).await.unwrap();

        // 模拟并发轮换
        let rotation_key = "tenant_123:user_456";
        manager
            .rotating_keys
            .write()
            .await
            .insert(rotation_key.to_string(), current_timestamp());

        let result = manager.rotate_l2_key("tenant_123", "user_456").await;
        assert!(matches!(
            result,
            Err(KeyRotationError::ConcurrentRotationConflict)
        ));
    }

    #[test]
    fn test_key_state_can_operations() {
        assert!(KeyState::Active.can_encrypt());
        assert!(KeyState::Active.can_decrypt());

        assert!(!KeyState::DecryptOnly.can_encrypt());
        assert!(KeyState::DecryptOnly.can_decrypt());

        assert!(!KeyState::Archived.can_encrypt());
        assert!(KeyState::Archived.can_decrypt());

        assert!(!KeyState::Revoked.can_encrypt());
        assert!(!KeyState::Revoked.can_decrypt());
    }

    #[test]
    fn test_rotation_policy_needs_rotation() {
        let current_time = current_timestamp();

        let l2_policy = RotationPolicy::L2Scheduled {
            interval_secs: 90 * 24 * 60 * 60,
            last_rotation: current_time - (91 * 24 * 60 * 60),
        };
        assert!(l2_policy.needs_rotation(current_time));

        let l2_policy_fresh = RotationPolicy::L2Scheduled {
            interval_secs: 90 * 24 * 60 * 60,
            last_rotation: current_time,
        };
        assert!(!l2_policy_fresh.needs_rotation(current_time));

        assert!(RotationPolicy::L3PerEncryption.needs_rotation(current_time));
        assert!(
            !RotationPolicy::L1Manual {
                is_emergency: false
            }
            .needs_rotation(current_time)
        );
    }

    #[test]
    fn test_secure_compare_key_ids() {
        assert!(secure_compare_key_ids("key_123", "key_123"));
        assert!(!secure_compare_key_ids("key_123", "key_456"));
    }
}
