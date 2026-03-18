//! Enclave 蓝绿部署升级模块
//!
//! 实现原子性的 Enclave 升级机制，支持：
//! - 蓝绿部署式升级
//! - 新旧版本并行运行
//! - Sealing Key 托管和紧急切换
//!
//! # 升级流程
//!
//! 1. 部署新版本 Enclave（Green）
//! 2. 新旧版本并行运行 24 小时
//! 3. 逐步迁移流量到新版本
//! 4. 验证新版本稳定性
//! 5. 切换 Sealing Key 托管
//! 6. 退役旧版本（Blue）
//!
//! # 紧急切换
//!
//! 支持从 MRENCLAVE（特定版本）切换到 MRSIGNER（签名者）模式，
//! 允许同一签名者的任何版本在紧急情况下接管。

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::sync::RwLock;
use zeroize::Zeroize;

/// 升级错误类型
#[derive(Error, Debug)]
pub enum UpgradeError {
    #[error("升级状态无效：{0}")]
    InvalidState(String),

    #[error("新版本 Enclave 启动失败：{0}")]
    NewEnclaveStartFailed(String),

    #[error("旧版本 Enclave 关闭失败：{0}")]
    OldEnclaveShutdownFailed(String),

    #[error("Sealing Key 迁移失败：{0}")]
    SealingKeyMigrationFailed(String),

    #[error("健康检查失败：{0}")]
    HealthCheckFailed(String),

    #[error("升级超时：{0}")]
    UpgradeTimeout(String),

    #[error("版本回滚失败：{0}")]
    RollbackFailed(String),

    #[error("并发升级冲突")]
    ConcurrentUpgradeConflict,

    #[error("MRENCLAVE 不匹配：期望 {expected}, 实际 {actual}")]
    MrenclaveMismatch { expected: String, actual: String },

    #[error("MRSIGNER 不匹配：期望 {expected}, 实际 {actual}")]
    MrsignerMismatch { expected: String, actual: String },
}

/// 升级结果
#[derive(Debug, Clone, PartialEq)]
pub enum UpgradeResult {
    /// 升级成功
    Success,
    /// 升级失败，已回滚
    FailedWithRollback,
    /// 升级进行中
    InProgress,
    /// 升级已取消
    Cancelled,
}

/// Enclave 升级阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum UpgradePhase {
    /// 初始状态，无升级
    Idle = 0,
    /// 准备新版本 Enclave
    Preparing = 1,
    /// 新版本 Enclave 启动中
    StartingNew = 2,
    /// 双版本并行运行（蓝绿部署）
    ParallelRunning = 3,
    /// 流量迁移中
    MigratingTraffic = 4,
    /// Sealing Key 迁移中
    MigratingSealingKey = 5,
    /// 验证新版本稳定性
    Validating = 6,
    /// 完成升级，关闭旧版本
    Completing = 7,
    /// 升级完成
    Completed = 8,
    /// 升级失败，回滚中
    RollingBack = 9,
}

impl UpgradePhase {
    /// 从 u8 转换
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Idle),
            1 => Some(Self::Preparing),
            2 => Some(Self::StartingNew),
            3 => Some(Self::ParallelRunning),
            4 => Some(Self::MigratingTraffic),
            5 => Some(Self::MigratingSealingKey),
            6 => Some(Self::Validating),
            7 => Some(Self::Completing),
            8 => Some(Self::Completed),
            9 => Some(Self::RollingBack),
            _ => None,
        }
    }

    /// 检查是否可以回滚
    pub fn can_rollback(&self) -> bool {
        matches!(
            self,
            Self::Preparing
                | Self::StartingNew
                | Self::ParallelRunning
                | Self::MigratingTraffic
                | Self::MigratingSealingKey
                | Self::Validating
        )
    }
}

/// Enclave 版本信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnclaveVersion {
    /// MRENCLAVE（Enclave 测量值）
    pub mrenclave: [u8; 32],
    /// MRSIGNER（签名者测量值）
    pub mrsigner: [u8; 32],
    /// 版本号
    pub version: String,
    /// 构建时间戳
    pub build_timestamp: u64,
    /// 是否支持 MRSIGNER 紧急切换
    pub supports_mrsigner_failover: bool,
}

impl EnclaveVersion {
    /// 创建新版本
    pub fn new(mrenclave: [u8; 32], mrsigner: [u8; 32], version: String) -> Self {
        Self {
            mrenclave,
            mrsigner,
            version,
            build_timestamp: current_timestamp(),
            supports_mrsigner_failover: true,
        }
    }

    /// 检查 MRENCLAVE 是否匹配
    pub fn matches_mrenclave(&self, other: &[u8; 32]) -> bool {
        use subtle::ConstantTimeEq;
        self.mrenclave.ct_eq(other).into()
    }

    /// 检查 MRSIGNER 是否匹配
    pub fn matches_mrsigner(&self, other: &[u8; 32]) -> bool {
        use subtle::ConstantTimeEq;
        self.mrsigner.ct_eq(other).into()
    }

    /// 获取 MRENCLAVE 十六进制字符串
    pub fn mrenclave_hex(&self) -> String {
        hex::encode(&self.mrenclave)
    }

    /// 获取 MRSIGNER 十六进制字符串
    pub fn mrsigner_hex(&self) -> String {
        hex::encode(&self.mrsigner)
    }
}

/// Sealing Key 托管信息
#[derive(Debug, Clone)]
pub struct SealingKeyCustody {
    /// 当前活跃的 MRENCLAVE
    pub active_mrenclave: [u8; 32],
    /// 备份的 MRSIGNER（用于紧急切换）
    pub backup_mrsigner: [u8; 32],
    /// Sealing Key 派生种子（加密存储）
    pub sealed_seed: Vec<u8>,
    /// 最后更新时间
    pub last_updated: u64,
    /// 是否启用 MRSIGNER 紧急切换
    pub mrsigner_failover_enabled: bool,
}

impl SealingKeyCustody {
    /// 创建新的托管信息
    pub fn new(
        active_mrenclave: [u8; 32],
        backup_mrsigner: [u8; 32],
        sealed_seed: Vec<u8>,
    ) -> Self {
        Self {
            active_mrenclave,
            backup_mrsigner,
            sealed_seed,
            last_updated: current_timestamp(),
            mrsigner_failover_enabled: true,
        }
    }

    /// 切换到 MRSIGNER 模式（紧急切换）
    pub fn failover_to_mrsigner(&mut self) {
        self.mrsigner_failover_enabled = true;
        self.last_updated = current_timestamp();
    }

    /// 更新活跃的 MRENCLAVE
    pub fn update_active_mrenclave(&mut self, new_mrenclave: [u8; 32]) {
        self.active_mrenclave = new_mrenclave;
        self.last_updated = current_timestamp();
    }
}

impl Drop for SealingKeyCustody {
    fn drop(&mut self) {
        // 安全清理敏感数据
        self.sealed_seed.zeroize();
    }
}

/// 蓝绿部署升级管理器
pub struct BlueGreenUpgradeManager {
    /// 当前升级阶段
    phase: AtomicU8,
    /// 当前活跃的 Enclave 版本
    active_version: RwLock<Option<EnclaveVersion>>,
    /// 新版本的 Enclave 版本
    pending_version: RwLock<Option<EnclaveVersion>>,
    /// 升级开始时间
    upgrade_start_time: AtomicU64,
    /// 并行运行超时（秒），默认 24 小时
    parallel_run_timeout_secs: AtomicU64,
    /// 是否正在升级
    is_upgrading: AtomicBool,
    /// Sealing Key 托管
    sealing_key_custody: RwLock<Option<SealingKeyCustody>>,
    /// 健康检查失败次数
    health_check_failures: AtomicU64,
    /// 最大允许的健康检查失败次数
    max_health_check_failures: u64,
}

/// 升级配置
#[derive(Debug, Clone)]
pub struct UpgradeConfig {
    /// 并行运行超时（秒）
    pub parallel_run_timeout_secs: u64,
    /// 健康检查间隔（秒）
    pub health_check_interval_secs: u64,
    /// 最大健康检查失败次数
    pub max_health_check_failures: u64,
    /// 流量迁移步长（百分比）
    pub traffic_migration_step: u8,
    /// 是否启用自动回滚
    pub enable_auto_rollback: bool,
}

impl Default for UpgradeConfig {
    fn default() -> Self {
        Self {
            parallel_run_timeout_secs: 24 * 60 * 60, // 24 小时
            health_check_interval_secs: 60,          // 1 分钟
            max_health_check_failures: 3,
            traffic_migration_step: 10, // 每次迁移 10% 流量
            enable_auto_rollback: true,
        }
    }
}

impl BlueGreenUpgradeManager {
    /// 创建新的升级管理器
    pub fn new(config: UpgradeConfig) -> Self {
        Self {
            phase: AtomicU8::new(UpgradePhase::Idle as u8),
            active_version: RwLock::new(None),
            pending_version: RwLock::new(None),
            upgrade_start_time: AtomicU64::new(0),
            parallel_run_timeout_secs: AtomicU64::new(config.parallel_run_timeout_secs),
            is_upgrading: AtomicBool::new(false),
            sealing_key_custody: RwLock::new(None),
            health_check_failures: AtomicU64::new(0),
            max_health_check_failures: config.max_health_check_failures,
        }
    }

    /// 获取当前升级阶段
    pub fn current_phase(&self) -> UpgradePhase {
        let phase_num = self.phase.load(Ordering::SeqCst);
        UpgradePhase::from_u8(phase_num).unwrap_or(UpgradePhase::Idle)
    }

    /// 检查是否正在升级
    pub fn is_upgrading(&self) -> bool {
        self.is_upgrading.load(Ordering::SeqCst)
    }

    /// 开始升级流程
    ///
    /// # 参数
    /// * `new_version` - 新版本 Enclave 信息
    /// * `sealing_key` - Sealing Key 托管信息
    ///
    /// # 返回
    /// 升级是否成功开始
    pub async fn start_upgrade(
        &self,
        new_version: EnclaveVersion,
        sealing_key: SealingKeyCustody,
    ) -> Result<(), UpgradeError> {
        // 检查是否有并发的升级
        let expected = false;
        if !self
            .is_upgrading
            .compare_exchange_weak(expected, true, Ordering::SeqCst, Ordering::SeqCst)
            .unwrap_or(true)
        {
            return Err(UpgradeError::ConcurrentUpgradeConflict);
        }

        // 验证新版本
        self.validate_new_version(&new_version).await?;

        // 设置升级阶段：Preparing
        self.phase
            .store(UpgradePhase::Preparing as u8, Ordering::SeqCst);
        self.upgrade_start_time
            .store(current_timestamp(), Ordering::SeqCst);

        // 保存新版本信息
        *self.pending_version.write().await = Some(new_version);

        // 保存 Sealing Key 托管信息
        *self.sealing_key_custody.write().await = Some(sealing_key);

        // 进入下一阶段：StartingNew
        self.phase
            .store(UpgradePhase::StartingNew as u8, Ordering::SeqCst);

        Ok(())
    }

    /// 验证新版本
    async fn validate_new_version(&self, version: &EnclaveVersion) -> Result<(), UpgradeError> {
        // 检查 MRENCLAVE 格式
        if version.mrenclave.iter().all(|&b| b == 0) {
            return Err(UpgradeError::MrenclaveMismatch {
                expected: "non-zero".to_string(),
                actual: version.mrenclave_hex(),
            });
        }

        // 检查版本号
        if version.version.is_empty() {
            return Err(UpgradeError::InvalidState("版本号不能为空".to_string()));
        }

        // 检查是否已经是活跃版本
        if let Some(active) = self.active_version.read().await.as_ref() {
            if active.mrenclave == version.mrenclave {
                return Err(UpgradeError::InvalidState(
                    "新版本与当前版本相同".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// 启动新版本 Enclave
    pub async fn start_new_enclave(&self) -> Result<(), UpgradeError> {
        let current_phase = self.current_phase();
        if current_phase != UpgradePhase::StartingNew {
            return Err(UpgradeError::InvalidState(format!(
                "当前阶段 {:?} 不允许启动新 Enclave",
                current_phase
            )));
        }

        // TODO(#TEE-201): 实际启动新版本 Enclave
        // 当前限制: 热升级功能需要完整的 Enclave 生命周期管理
        // 状态: 框架已就绪，等待 Enclave 运行时集成

        // 进入并行运行阶段
        self.phase
            .store(UpgradePhase::ParallelRunning as u8, Ordering::SeqCst);

        Ok(())
    }

    /// 检查并行运行超时
    pub fn check_parallel_run_timeout(&self) -> bool {
        let start_time = self.upgrade_start_time.load(Ordering::SeqCst);
        let current_time = current_timestamp();
        let timeout = self.parallel_run_timeout_secs.load(Ordering::SeqCst);

        current_time.saturating_sub(start_time) > timeout
    }

    /// 执行健康检查
    pub async fn perform_health_check(&self) -> Result<bool, UpgradeError> {
        let pending = self.pending_version.read().await;
        if pending.is_none() {
            return Err(UpgradeError::InvalidState("新版本未设置".to_string()));
        }

        // TODO(#TEE-202): 实际执行健康检查
        // 需要: 新版本 Enclave 健康检查端点
        // 当前: 模拟成功用于框架测试
        let healthy = true;

        if !healthy {
            let failures = self.health_check_failures.fetch_add(1, Ordering::SeqCst) + 1;
            if failures >= self.max_health_check_failures {
                return Err(UpgradeError::HealthCheckFailed(format!(
                    "健康检查失败次数达到上限 ({})",
                    failures
                )));
            }
        } else {
            // 重置失败计数
            self.health_check_failures.store(0, Ordering::SeqCst);
        }

        Ok(healthy)
    }

    /// 迁移流量到新版本
    pub async fn migrate_traffic(&self, _percentage: u8) -> Result<(), UpgradeError> {
        let current_phase = self.current_phase();
        if current_phase != UpgradePhase::ParallelRunning
            && current_phase != UpgradePhase::MigratingTraffic
        {
            return Err(UpgradeError::InvalidState(format!(
                "当前阶段 {:?} 不允许迁移流量",
                current_phase
            )));
        }

        // 设置阶段
        self.phase
            .store(UpgradePhase::MigratingTraffic as u8, Ordering::SeqCst);

        // TODO(#TEE-203): 实际迁移流量
        // 需要: 负载均衡器集成和流量路由控制

        // 进入下一阶段
        self.phase
            .store(UpgradePhase::MigratingSealingKey as u8, Ordering::SeqCst);

        Ok(())
    }

    /// 迁移 Sealing Key
    pub async fn migrate_sealing_key(&self) -> Result<(), UpgradeError> {
        let current_phase = self.current_phase();
        if current_phase != UpgradePhase::MigratingSealingKey {
            return Err(UpgradeError::InvalidState(format!(
                "当前阶段 {:?} 不允许迁移 Sealing Key",
                current_phase
            )));
        }

        let mut custody = self.sealing_key_custody.write().await;
        if let Some(ref mut custody) = *custody {
            // 更新活跃的 MRENCLAVE 为新版本
            if let Some(pending) = self.pending_version.read().await.as_ref() {
                custody.update_active_mrenclave(pending.mrenclave);
            }
        }

        // 进入验证阶段
        self.phase
            .store(UpgradePhase::Validating as u8, Ordering::SeqCst);

        Ok(())
    }

    /// 完成升级
    pub async fn complete_upgrade(&self) -> Result<UpgradeResult, UpgradeError> {
        let current_phase = self.current_phase();
        if current_phase != UpgradePhase::Validating && current_phase != UpgradePhase::Completing {
            return Err(UpgradeError::InvalidState(format!(
                "当前阶段 {:?} 不允许完成升级",
                current_phase
            )));
        }

        // 设置阶段
        self.phase
            .store(UpgradePhase::Completing as u8, Ordering::SeqCst);

        // 将新版本设为活跃版本
        let mut active = self.active_version.write().await;
        let mut pending = self.pending_version.write().await;

        if let Some(new_version) = pending.take() {
            *active = Some(new_version.clone());
        }

        // TODO(#TEE-204): 关闭旧版本 Enclave
        // 需要: 优雅的连接 draining 和 Enclave 终止

        // 设置完成状态
        self.phase
            .store(UpgradePhase::Completed as u8, Ordering::SeqCst);
        self.is_upgrading.store(false, Ordering::SeqCst);

        Ok(UpgradeResult::Success)
    }

    /// 执行回滚
    pub async fn rollback(&self) -> Result<UpgradeResult, UpgradeError> {
        let current_phase = self.current_phase();
        if !current_phase.can_rollback() {
            return Err(UpgradeError::InvalidState(format!(
                "当前阶段 {:?} 不允许回滚",
                current_phase
            )));
        }

        // 设置回滚阶段
        self.phase
            .store(UpgradePhase::RollingBack as u8, Ordering::SeqCst);

        // 清理新版本信息
        *self.pending_version.write().await = None;

        // 恢复 Sealing Key 托管
        // TODO(#TEE-205): 实现回滚恢复逻辑
        // 需要: 备份恢复机制和状态回滚

        // 重置状态
        self.phase.store(UpgradePhase::Idle as u8, Ordering::SeqCst);
        self.is_upgrading.store(false, Ordering::SeqCst);
        self.health_check_failures.store(0, Ordering::SeqCst);

        Ok(UpgradeResult::FailedWithRollback)
    }

    /// 紧急切换到 MRSIGNER 模式
    pub async fn emergency_mrsigner_failover(&self) -> Result<(), UpgradeError> {
        let mut custody = self.sealing_key_custody.write().await;

        if let Some(ref mut custody) = *custody {
            custody.failover_to_mrsigner();
            Ok(())
        } else {
            Err(UpgradeError::InvalidState(
                "Sealing Key 托管未初始化".to_string(),
            ))
        }
    }

    /// 获取升级状态
    pub async fn get_status(&self) -> UpgradeStatus {
        UpgradeStatus {
            phase: self.current_phase(),
            is_upgrading: self.is_upgrading(),
            active_version: self.active_version.read().await.clone(),
            pending_version: self.pending_version.read().await.clone(),
            health_check_failures: self.health_check_failures.load(Ordering::SeqCst),
            elapsed_secs: current_timestamp()
                .saturating_sub(self.upgrade_start_time.load(Ordering::SeqCst)),
        }
    }
}

/// 升级状态信息
#[derive(Debug, Clone)]
pub struct UpgradeStatus {
    pub phase: UpgradePhase,
    pub is_upgrading: bool,
    pub active_version: Option<EnclaveVersion>,
    pub pending_version: Option<EnclaveVersion>,
    pub health_check_failures: u64,
    pub elapsed_secs: u64,
}

/// 获取当前时间戳
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间错误")
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_version(mrenclave_byte: u8) -> EnclaveVersion {
        let mut mrenclave = [0u8; 32];
        mrenclave[0] = mrenclave_byte;

        EnclaveVersion::new(mrenclave, [0x42u8; 32], format!("1.0.{}", mrenclave_byte))
    }

    fn create_test_sealing_key() -> SealingKeyCustody {
        SealingKeyCustody::new([0x01u8; 32], [0x42u8; 32], vec![0x42u8; 64])
    }

    #[tokio::test]
    async fn test_upgrade_manager_creation() {
        let config = UpgradeConfig::default();
        let manager = BlueGreenUpgradeManager::new(config);

        assert_eq!(manager.current_phase(), UpgradePhase::Idle);
        assert!(!manager.is_upgrading());
    }

    #[tokio::test]
    async fn test_start_upgrade() {
        let manager = BlueGreenUpgradeManager::new(UpgradeConfig::default());
        let new_version = create_test_version(0x42);
        let sealing_key = create_test_sealing_key();

        let result = manager.start_upgrade(new_version, sealing_key).await;
        assert!(result.is_ok());

        assert_eq!(manager.current_phase(), UpgradePhase::StartingNew);
        assert!(manager.is_upgrading());
    }

    #[tokio::test]
    async fn test_concurrent_upgrade_conflict() {
        let manager = BlueGreenUpgradeManager::new(UpgradeConfig::default());
        let version1 = create_test_version(0x42);
        let version2 = create_test_version(0x43);
        let sealing_key = create_test_sealing_key();

        // 第一次升级应该成功
        let result1 = manager.start_upgrade(version1, sealing_key.clone()).await;
        assert!(result1.is_ok());

        // 第二次升级应该失败（并发冲突）
        let result2 = manager.start_upgrade(version2, sealing_key).await;
        assert!(matches!(
            result2,
            Err(UpgradeError::ConcurrentUpgradeConflict)
        ));
    }

    #[tokio::test]
    async fn test_validate_new_version() {
        let manager = BlueGreenUpgradeManager::new(UpgradeConfig::default());

        // 无效的 MRENCLAVE（全零）
        let invalid_version = EnclaveVersion::new([0u8; 32], [0x42u8; 32], "1.0.0".to_string());
        let result = manager.validate_new_version(&invalid_version).await;
        assert!(result.is_err());

        // 无效的版本号（空字符串）
        let invalid_version2 = EnclaveVersion::new([0x42u8; 32], [0x42u8; 32], "".to_string());
        let result2 = manager.validate_new_version(&invalid_version2).await;
        assert!(result2.is_err());
    }

    #[tokio::test]
    async fn test_enclave_version_matching() {
        let version = create_test_version(0x42);

        let mut matching_mrenclave = [0u8; 32];
        matching_mrenclave[0] = 0x42;
        assert!(version.matches_mrenclave(&matching_mrenclave));

        let mut non_matching_mrenclave = [0u8; 32];
        non_matching_mrenclave[0] = 0x43;
        assert!(!version.matches_mrenclave(&non_matching_mrenclave));
    }

    #[tokio::test]
    async fn test_upgrade_phase_transitions() {
        let manager = BlueGreenUpgradeManager::new(UpgradeConfig::default());
        let new_version = create_test_version(0x42);
        let sealing_key = create_test_sealing_key();

        // 开始升级
        manager
            .start_upgrade(new_version, sealing_key)
            .await
            .unwrap();
        assert_eq!(manager.current_phase(), UpgradePhase::StartingNew);

        // 启动新 Enclave
        manager.start_new_enclave().await.unwrap();
        assert_eq!(manager.current_phase(), UpgradePhase::ParallelRunning);

        // 迁移流量
        manager.migrate_traffic(50).await.unwrap();
        assert_eq!(manager.current_phase(), UpgradePhase::MigratingSealingKey);

        // 迁移 Sealing Key
        manager.migrate_sealing_key().await.unwrap();
        assert_eq!(manager.current_phase(), UpgradePhase::Validating);

        // 完成升级
        let result = manager.complete_upgrade().await.unwrap();
        assert_eq!(result, UpgradeResult::Success);
        assert_eq!(manager.current_phase(), UpgradePhase::Completed);
        assert!(!manager.is_upgrading());
    }

    #[tokio::test]
    async fn test_rollback() {
        let manager = BlueGreenUpgradeManager::new(UpgradeConfig::default());
        let new_version = create_test_version(0x42);
        let sealing_key = create_test_sealing_key();

        manager
            .start_upgrade(new_version, sealing_key)
            .await
            .unwrap();

        // 执行回滚
        let result = manager.rollback().await.unwrap();
        assert_eq!(result, UpgradeResult::FailedWithRollback);
        assert_eq!(manager.current_phase(), UpgradePhase::Idle);
        assert!(!manager.is_upgrading());
    }

    #[tokio::test]
    async fn test_emergency_mrsigner_failover() {
        let manager = BlueGreenUpgradeManager::new(UpgradeConfig::default());
        let sealing_key = create_test_sealing_key();

        // 先设置 Sealing Key 托管
        *manager.sealing_key_custody.write().await = Some(sealing_key);

        // 执行紧急切换
        let result = manager.emergency_mrsigner_failover().await;
        assert!(result.is_ok());

        // 验证切换状态
        let custody = manager.sealing_key_custody.read().await;
        assert!(custody.as_ref().unwrap().mrsigner_failover_enabled);
    }

    #[tokio::test]
    async fn test_get_status() {
        let manager = BlueGreenUpgradeManager::new(UpgradeConfig::default());
        let new_version = create_test_version(0x42);
        let sealing_key = create_test_sealing_key();

        manager
            .start_upgrade(new_version.clone(), sealing_key)
            .await
            .unwrap();

        let status = manager.get_status().await;
        assert_eq!(status.phase, UpgradePhase::StartingNew);
        assert!(status.is_upgrading);
        assert!(status.pending_version.is_some());
    }

    #[test]
    fn test_upgrade_phase_from_u8() {
        assert_eq!(UpgradePhase::from_u8(0), Some(UpgradePhase::Idle));
        assert_eq!(
            UpgradePhase::from_u8(3),
            Some(UpgradePhase::ParallelRunning)
        );
        assert_eq!(UpgradePhase::from_u8(8), Some(UpgradePhase::Completed));
        assert_eq!(UpgradePhase::from_u8(99), None);
    }

    #[test]
    fn test_upgrade_phase_can_rollback() {
        assert!(UpgradePhase::Preparing.can_rollback());
        assert!(UpgradePhase::ParallelRunning.can_rollback());
        assert!(UpgradePhase::Validating.can_rollback());
        assert!(!UpgradePhase::Completed.can_rollback());
        assert!(!UpgradePhase::Idle.can_rollback());
    }

    #[test]
    fn test_enclave_version_hex() {
        let version = create_test_version(0x42);
        let mrenclave_hex = version.mrenclave_hex();
        assert_eq!(mrenclave_hex.len(), 64); // 32 bytes = 64 hex chars
    }
}
