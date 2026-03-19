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
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
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
        hex::encode(self.mrenclave)
    }

    /// 获取 MRSIGNER 十六进制字符串
    pub fn mrsigner_hex(&self) -> String {
        hex::encode(self.mrsigner)
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
    /// 当前流向新版本的流量百分比（0-100）
    /// 使用原子操作追踪路由权重，无需外部负载均衡器即可记录迁移状态
    traffic_to_new_pct: AtomicU8,
    /// 升级配置快照（用于健康检查等异步操作）
    config: UpgradeConfig,
    /// 升级完成时间戳（0 表示未完成）
    upgrade_complete_time: AtomicU64,
    /// 新版本 Enclave 的 PID（原子存储，避免多线程 env::set_var UB）
    new_enclave_pid: AtomicU32,
    /// 新版本 Enclave 的子进程句柄（持有所有权，避免 mem::forget）
    current_enclave_child: RwLock<Option<std::process::Child>>,
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
    /// 新版本 Enclave 健康检查 URL（如 "http://127.0.0.1:9081/health"）
    /// None 时跳过 HTTP 检查，仅追踪权重状态
    pub new_enclave_health_url: Option<String>,
    /// 健康检查 HTTP 超时（秒）
    pub health_check_timeout_secs: u64,
}

impl Default for UpgradeConfig {
    fn default() -> Self {
        Self {
            parallel_run_timeout_secs: 24 * 60 * 60, // 24 小时
            health_check_interval_secs: 60,          // 1 分钟
            max_health_check_failures: 3,
            traffic_migration_step: 10, // 每次迁移 10% 流量
            enable_auto_rollback: true,
            new_enclave_health_url: None,
            health_check_timeout_secs: 5,
        }
    }
}

impl BlueGreenUpgradeManager {
    /// 创建新的升级管理器
    pub fn new(config: UpgradeConfig) -> Self {
        let max_failures = config.max_health_check_failures;
        let timeout_secs = config.parallel_run_timeout_secs;
        Self {
            phase: AtomicU8::new(UpgradePhase::Idle as u8),
            active_version: RwLock::new(None),
            pending_version: RwLock::new(None),
            upgrade_start_time: AtomicU64::new(0),
            parallel_run_timeout_secs: AtomicU64::new(timeout_secs),
            is_upgrading: AtomicBool::new(false),
            sealing_key_custody: RwLock::new(None),
            health_check_failures: AtomicU64::new(0),
            max_health_check_failures: max_failures,
            traffic_to_new_pct: AtomicU8::new(0),
            config,
            upgrade_complete_time: AtomicU64::new(0),
            new_enclave_pid: AtomicU32::new(0),
            current_enclave_child: RwLock::new(None),
        }
    }

    /// 获取当前流向新版本的流量百分比
    pub fn traffic_to_new_percentage(&self) -> u8 {
        self.traffic_to_new_pct.load(Ordering::SeqCst)
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
        // 检查是否有并发的升级（使用 compare_exchange 而非 compare_exchange_weak 避免伪失败）
        if self
            .is_upgrading
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
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
        if let Some(active) = self.active_version.read().await.as_ref()
            && active.mrenclave == version.mrenclave
        {
            return Err(UpgradeError::InvalidState(
                "新版本与当前版本相同".to_string(),
            ));
        }

        Ok(())
    }

    /// 启动新版本 Enclave
    ///
    /// 尝试通过环境变量 `TEE_NEW_ENCLAVE_BINARY` 指定的路径启动新版本 Enclave 进程。
    /// 若环境变量未设置，则假定新版本已由外部部署系统启动，直接进入并行运行阶段。
    ///
    /// 外部部署方式（推荐）：在调用此方法前，由 CI/CD 系统启动新版本进程并设置健康检查 URL。
    pub async fn start_new_enclave(&self) -> Result<(), UpgradeError> {
        let current_phase = self.current_phase();
        if current_phase != UpgradePhase::StartingNew {
            return Err(UpgradeError::InvalidState(format!(
                "当前阶段 {:?} 不允许启动新 Enclave",
                current_phase
            )));
        }

        // 尝试通过环境变量获取新版本二进制路径
        if let Ok(binary_path) = std::env::var("TEE_NEW_ENCLAVE_BINARY") {
            log::info!("Starting new enclave binary: {}", binary_path);

            // 以独立进程启动新版本 Enclave（非阻塞）
            match std::process::Command::new(&binary_path)
                .env("TEE_ROLE", "green")  // 标记为新版本（绿色）
                .spawn()
            {
                Ok(child) => {
                    let pid = child.id();
                    log::info!("New enclave process started with PID {}", pid);
                    // 将 PID 存储到原子变量，避免多线程 env::set_var UB
                    self.new_enclave_pid.store(pid, Ordering::SeqCst);
                    // 持有 child 所有权，避免 mem::forget 导致的资源泄漏
                    *self.current_enclave_child.write().await = Some(child);
                }
                Err(e) => {
                    return Err(UpgradeError::NewEnclaveStartFailed(format!(
                        "Failed to start new enclave binary '{}': {}. \
                         Ensure the binary exists and is executable.",
                        binary_path, e
                    )));
                }
            }
        } else {
            // 未配置二进制路径：假定新版本已由外部系统部署
            log::info!(
                "TEE_NEW_ENCLAVE_BINARY not set. Assuming new enclave has been deployed \
                 externally (e.g., via Kubernetes rolling update or docker run). \
                 Proceeding to parallel running phase."
            );
        }

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
    ///
    /// 若配置了 `new_enclave_health_url`，则通过 HTTP GET 请求检查新版本 Enclave 的健康端点。
    /// 期望返回 HTTP 2xx 状态码视为健康。
    ///
    /// 若未配置健康检查 URL，则记录警告并假设健康（适用于无 HTTP 接口的 Enclave）。
    pub async fn perform_health_check(&self) -> Result<bool, UpgradeError> {
        let pending = self.pending_version.read().await;
        if pending.is_none() {
            return Err(UpgradeError::InvalidState("新版本未设置".to_string()));
        }
        drop(pending); // 尽早释放读锁

        let healthy = if let Some(ref health_url) = self.config.new_enclave_health_url {
            // 通过 HTTP 检查新版本 Enclave 健康端点
            self.check_http_health(health_url).await
        } else {
            // 未配置健康检查 URL：记录警告，假设健康
            log::warn!(
                "No health check URL configured (new_enclave_health_url). \
                 Assuming new enclave is healthy. Configure health URL for production upgrades."
            );
            true
        };

        if !healthy {
            let failures = self.health_check_failures.fetch_add(1, Ordering::SeqCst) + 1;
            log::warn!("Health check failed ({}/{})", failures, self.max_health_check_failures);
            if failures >= self.max_health_check_failures {
                return Err(UpgradeError::HealthCheckFailed(format!(
                    "健康检查连续失败次数达到上限 ({})",
                    failures
                )));
            }
        } else {
            // 重置失败计数
            self.health_check_failures.store(0, Ordering::SeqCst);
        }

        Ok(healthy)
    }

    /// 通过 HTTP GET 检查 Enclave 健康端点
    ///
    /// 返回 true 表示收到 2xx 响应；网络错误或非 2xx 状态码返回 false。
    async fn check_http_health(&self, url: &str) -> bool {
        use std::time::Duration;
        use tokio::io::AsyncWriteExt;

        // 确保超时至少为 1 秒，避免 health_check_timeout_secs=0 时立即超时
        let timeout_secs = self.config.health_check_timeout_secs.max(1);

        // 解析 URL 获取 scheme、host 和 port
        let scheme = if url.starts_with("https://") {
            "https"
        } else if url.starts_with("http://") {
            "http"
        } else {
            log::error!("Health check URL must start with http:// or https://: {}", url);
            return false;
        };

        let (host, port, path) = match parse_health_url(url) {
            Some(parts) => parts,
            None => {
                log::error!("Invalid health check URL: {}", url);
                return false;
            }
        };

        // 根据 scheme 建立连接，分别处理
        let addr = format!("{}:{}", host, port);

        if scheme == "https" {
            // HTTPS: 使用 TLS 连接
            use tokio_rustls::TlsConnector;
            use rustls::ClientConfig;
            use std::sync::Arc;
            use webpki_roots::TLS_SERVER_ROOTS;

            let tcp_stream = match tokio::time::timeout(
                Duration::from_secs(timeout_secs),
                tokio::net::TcpStream::connect(&addr),
            )
            .await
            {
                Ok(Ok(stream)) => stream,
                Ok(Err(e)) => {
                    log::warn!("Health check TCP connect failed ({}): {}", addr, e);
                    return false;
                }
                Err(_) => {
                    log::warn!("Health check TCP connect timed out after {}s ({})", timeout_secs, addr);
                    return false;
                }
            };

            // 配置 TLS
            let root_store = Arc::new(rustls::RootCertStore::from_iter(
                TLS_SERVER_ROOTS.iter().cloned()
            ));
            let config = ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth();

            let connector = TlsConnector::from(Arc::new(config));
            let host_owned = host.clone();
            let domain = match rustls::pki_types::ServerName::try_from(host_owned) {
                Ok(d) => d,
                Err(e) => {
                    log::warn!("Invalid DNS name '{}': {}", host, e);
                    return false;
                }
            };

            let mut tls_stream = match tokio::time::timeout(
                Duration::from_secs(timeout_secs),
                connector.connect(domain, tcp_stream),
            )
            .await
            {
                Ok(Ok(s)) => s,
                Ok(Err(e)) => {
                    log::warn!("Health check TLS handshake failed: {}", e);
                    return false;
                }
                Err(_) => {
                    log::warn!("Health check TLS handshake timed out after {}s", timeout_secs);
                    return false;
                }
            };

            // 发送 HTTP 请求到 TLS 流
            let request = format!(
                "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
                path, host
            );

            match tokio::time::timeout(
                Duration::from_secs(timeout_secs),
                tls_stream.write_all(request.as_bytes()),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    log::warn!("Health check HTTPS write failed: {}", e);
                    return false;
                }
                Err(_) => {
                    log::warn!("Health check HTTPS write timed out after {}s", timeout_secs);
                    return false;
                }
            }

            // 读取响应
            use tokio::io::AsyncBufReadExt;
            let mut reader = tokio::io::BufReader::new(tls_stream);
            let mut response_line = String::with_capacity(256);

            match tokio::time::timeout(
                Duration::from_secs(timeout_secs),
                reader.read_line(&mut response_line),
            )
            .await
            {
                Ok(Ok(n)) if n > 0 => {
                let first_line = response_line.trim();
                if let Some(status_str) = first_line.split_whitespace().nth(1) {
                    if let Ok(status) = status_str.parse::<u16>() {
                        let is_healthy = (200..300).contains(&status);
                        log::debug!("Health check {} returned HTTP {}", url, status);
                        return is_healthy;
                    }
                }
                }
                Ok(Ok(_)) => {
                    log::warn!("Health check HTTPS read returned empty");
                    return false;
                }
                Ok(Err(e)) => {
                    log::warn!("Health check HTTPS read failed: {}", e);
                    return false;
                }
                Err(_) => {
                    log::warn!("Health check HTTPS read timed out after {}s", timeout_secs);
                    return false;
                }
            };

            log::warn!("Health check {} returned unparseable response", url);
            return false;
        } else {
            // HTTP: 使用明文 TCP 连接
            let mut tcp_stream = match tokio::time::timeout(
                Duration::from_secs(timeout_secs),
                tokio::net::TcpStream::connect(&addr),
            )
            .await
            {
                Ok(Ok(stream)) => stream,
                Ok(Err(e)) => {
                    log::warn!("Health check TCP connect failed ({}): {}", addr, e);
                    return false;
                }
                Err(_) => {
                    log::warn!("Health check TCP connect timed out after {}s ({})", timeout_secs, addr);
                    return false;
                }
            };

            // 发送最小化 HTTP/1.1 GET 请求
            let request = format!(
                "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
                path, host
            );

            match tokio::time::timeout(
                Duration::from_secs(timeout_secs),
                tcp_stream.write_all(request.as_bytes()),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    log::warn!("Health check HTTP write failed: {}", e);
                    return false;
                }
                Err(_) => {
                    log::warn!("Health check HTTP write timed out after {}s", timeout_secs);
                    return false;
                }
            }

            // 读取 HTTP 响应头（循环读取直到完整接收）
            // HTTP 响应头通常较小，但需要确保读取完整以避免解析错误
            use tokio::io::AsyncBufReadExt;
            let mut reader = tokio::io::BufReader::new(tcp_stream);
            let mut response_line = String::with_capacity(256);

            match tokio::time::timeout(
                Duration::from_secs(timeout_secs),
                reader.read_line(&mut response_line),
            )
            .await
            {
                Ok(Ok(n)) if n > 0 => {
                    // 解析状态行：HTTP/1.1 2xx ...
                    let first_line = response_line.trim();
                    // 状态行格式：HTTP/x.x STATUS_CODE REASON
                    if let Some(status_str) = first_line.split_whitespace().nth(1) {
                        if let Ok(status) = status_str.parse::<u16>() {
                            let is_healthy = (200..300).contains(&status);
                            log::debug!("Health check {} returned HTTP {}", url, status);
                            return is_healthy;
                        }
                    }
                }
                Ok(Ok(_)) => {
                    log::warn!("Health check HTTP read returned empty");
                    return false;
                }
                Ok(Err(e)) => {
                    log::warn!("Health check HTTP read failed: {}", e);
                    return false;
                }
                Err(_) => {
                    log::warn!("Health check HTTP read timed out after {}s", timeout_secs);
                    return false;
                }
            };

            log::warn!("Health check {} returned unparseable response", url);
            false
        }
    }

    /// 迁移流量到新版本
    ///
    /// 更新内部流量权重状态（`traffic_to_new_pct`），追踪有多少百分比的请求
    /// 应路由到新版本 Enclave。调用者负责根据此权重值调整实际路由
    ///（Nginx upstream_weight、Envoy weighted_cluster 等）。
    ///
    /// 每次调用均推进到 `MigratingSealingKey` 阶段，表示流量迁移操作已完成，
    /// 准备进行 Sealing Key 迁移。渐进式迁移应多次调用此函数，每次传入更高的百分比。
    ///
    /// # 参数
    /// * `percentage` - 流向新版本的目标百分比（0-100，超出范围将被截断至100）
    pub async fn migrate_traffic(&self, percentage: u8) -> Result<(), UpgradeError> {
        let current_phase = self.current_phase();
        if current_phase != UpgradePhase::ParallelRunning
            && current_phase != UpgradePhase::MigratingTraffic
        {
            return Err(UpgradeError::InvalidState(format!(
                "当前阶段 {:?} 不允许迁移流量",
                current_phase
            )));
        }

        // 限制百分比在 0-100 范围内
        let clamped = percentage.min(100);

        // 设置阶段为 MigratingTraffic
        self.phase
            .store(UpgradePhase::MigratingTraffic as u8, Ordering::SeqCst);

        // 更新流量权重（原子写入，调用者读取此值来配置路由）
        let prev = self.traffic_to_new_pct.swap(clamped, Ordering::SeqCst);
        log::info!(
            "Traffic migration: {}% -> {}% routed to new enclave version",
            prev,
            clamped
        );

        // 推进到 Sealing Key 迁移阶段
        // 仅当流量达到 100% 时才推进，否则停留在 MigratingTraffic
        if clamped == 100 {
            self.phase
                .store(UpgradePhase::MigratingSealingKey as u8, Ordering::SeqCst);
        }

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
    ///
    /// 执行以下步骤：
    /// 1. 将待升级版本提升为活跃版本
    /// 2. 重置流量权重（100% 流向新版本即旧的 active）
    /// 3. 记录完成时间戳
    /// 4. 发送 SIGTERM 给旧版本进程（如果配置了 PID 文件路径）
    /// 5. 重置升级状态标志
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

        // 将新版本设为活跃版本（持有两个锁期间状态一致）
        {
            let mut active = self.active_version.write().await;
            let mut pending = self.pending_version.write().await;

            let new_version = pending.take().ok_or_else(|| {
                UpgradeError::InvalidState("No pending version to complete upgrade".to_string())
            })?;
            let old_version = active.replace(new_version);
            if let Some(ref old) = old_version {
                log::info!(
                    "Upgrade complete: old enclave {} retired, new enclave {} is now active",
                    old.version,
                    active.as_ref().map(|v| v.version.as_str()).unwrap_or("unknown")
                );

                // 尝试向旧版本发送停止信号
                // 通过环境变量 TEE_OLD_ENCLAVE_PID 获取旧进程 PID（可选）
                if let Ok(pid_str) = std::env::var("TEE_OLD_ENCLAVE_PID") {
                    if let Ok(pid) = pid_str.trim().parse::<u32>() {
                        // 安全地发送 SIGTERM 信号（优雅停止）
                        #[cfg(unix)]
                        {
                            use std::process::Command;
                            match Command::new("kill").args(["-TERM", &pid.to_string()]).status() {
                                Ok(s) if s.success() => {
                                    log::info!("Sent SIGTERM to old enclave PID {}", pid);
                                }
                                Ok(s) => {
                                    log::warn!("kill -TERM {} exited with status: {}", pid, s);
                                }
                                Err(e) => {
                                    log::warn!("Failed to send SIGTERM to PID {}: {}", pid, e);
                                }
                            }
                        }
                        #[cfg(not(unix))]
                        {
                            log::warn!(
                                "SIGTERM not supported on this platform. \
                                 Old enclave PID {} must be terminated manually.",
                                pid
                            );
                        }
                    }
                }
            }
        }

        // 记录完成时间戳
        self.upgrade_complete_time
            .store(current_timestamp(), Ordering::SeqCst);

        // 重置流量权重（升级完成后，"新版本"即为活跃版本，权重概念失效）
        self.traffic_to_new_pct.store(0, Ordering::SeqCst);

        // 重置升级控制状态
        self.health_check_failures.store(0, Ordering::SeqCst);

        // 设置完成状态（先重置 is_upgrading，再设置 phase）
        self.is_upgrading.store(false, Ordering::SeqCst);
        self.phase
            .store(UpgradePhase::Completed as u8, Ordering::SeqCst);

        log::info!("Blue-green upgrade completed successfully at timestamp {}",
                   self.upgrade_complete_time.load(Ordering::SeqCst));

        Ok(UpgradeResult::Success)
    }

    /// 执行回滚
    ///
    /// 将系统恢复到升级前状态：
    /// 1. 丢弃待升级版本信息
    /// 2. 恢复 Sealing Key 为旧版本 MRENCLAVE 绑定
    /// 3. 重置流量权重到 0（全部回到旧版本）
    /// 4. 重置所有升级状态计数器
    pub async fn rollback(&self) -> Result<UpgradeResult, UpgradeError> {
        let current_phase = self.current_phase();
        if !current_phase.can_rollback() {
            return Err(UpgradeError::InvalidState(format!(
                "当前阶段 {:?} 不允许回滚",
                current_phase
            )));
        }

        log::warn!("Initiating blue-green upgrade rollback from phase {:?}", current_phase);

        // 设置回滚阶段
        self.phase
            .store(UpgradePhase::RollingBack as u8, Ordering::SeqCst);

        // 1. 清理新版本信息
        {
            let mut pending = self.pending_version.write().await;
            if let Some(ref v) = *pending {
                log::info!("Discarding pending enclave version: {}", v.version);
            }
            *pending = None;
        }

        // 2. 恢复 Sealing Key 托管到旧版本的 MRENCLAVE 绑定
        // 若 Sealing Key 已迁移到新版本 MRENCLAVE，将其恢复为旧版本（active_version）
        {
            let active = self.active_version.read().await;
            let mut custody = self.sealing_key_custody.write().await;
            if let Some(ref mut custody_data) = *custody {
                if let Some(ref active_ver) = *active {
                    // 将 active_mrenclave 恢复为当前活跃版本（即旧版本）
                    custody_data.update_active_mrenclave(active_ver.mrenclave);
                    // 禁用 MRSIGNER 紧急切换模式（恢复到严格 MRENCLAVE 绑定）
                    custody_data.mrsigner_failover_enabled = false;
                    log::info!(
                        "Sealing key custody restored to active enclave MRENCLAVE: {}",
                        active_ver.mrenclave_hex()
                    );
                } else {
                    // 没有活跃版本时，启用 MRSIGNER 回退以保证密钥可用性
                    custody_data.failover_to_mrsigner();
                    log::warn!(
                        "No active enclave version found during rollback. \
                         Enabling MRSIGNER failover mode to preserve key access."
                    );
                }
            }
        }

        // 3. 重置流量权重（全部回到旧版本）
        let prev_traffic = self.traffic_to_new_pct.swap(0, Ordering::SeqCst);
        if prev_traffic > 0 {
            log::info!("Traffic weight reset: {}% that was routed to new enclave reverted to old enclave", prev_traffic);
        }

        // 4. 重置所有计数器和状态
        self.health_check_failures.store(0, Ordering::SeqCst);
        self.upgrade_start_time.store(0, Ordering::SeqCst);

        // 恢复到空闲状态（先重置 is_upgrading，再设置 phase）
        self.is_upgrading.store(false, Ordering::SeqCst);
        self.phase.store(UpgradePhase::Idle as u8, Ordering::SeqCst);

        log::info!("Rollback completed. System restored to pre-upgrade state.");

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

/// 解析健康检查 URL，提取 (host, port, path)
///
/// 支持格式：`http://host:port/path`（仅 http，不支持 https）
fn parse_health_url(url: &str) -> Option<(String, u16, String)> {
    // 去除 http:// 前缀
    let without_scheme = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))?;

    // 分离 host:port 和 path
    let (host_port, path) = if let Some(slash_idx) = without_scheme.find('/') {
        let path = &without_scheme[slash_idx..];
        (&without_scheme[..slash_idx], path.to_string())
    } else {
        (without_scheme, "/".to_string())
    };

    // 分离 host 和 port
    let (host, port) = if let Some(colon_idx) = host_port.rfind(':') {
        let host = &host_port[..colon_idx];
        let port_str = &host_port[colon_idx + 1..];
        let port = port_str.parse::<u16>().ok()?;
        (host.to_string(), port)
    } else {
        // 默认端口 80
        (host_port.to_string(), 80u16)
    };

    if host.is_empty() {
        return None;
    }

    Some((host, port, path))
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

        // 迁移流量 - 50% 时停留在 MigratingTraffic
        manager.migrate_traffic(50).await.unwrap();
        assert_eq!(manager.current_phase(), UpgradePhase::MigratingTraffic);

        // 迁移流量 - 100% 时才推进到 MigratingSealingKey
        manager.migrate_traffic(100).await.unwrap();
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
