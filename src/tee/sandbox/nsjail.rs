//! nsjail 沙箱实现

use crate::tee::sandbox::{
    config::NsjailConfig,
    error::{SandboxError, SecurityError},
    types::{SandboxId, SandboxStatus, WarmInstanceInfo},
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::process::{Child, Command};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// nsjail 沙箱
#[allow(dead_code)]
pub struct NsjailSandbox {
    /// 沙箱 ID
    pub id: SandboxId,
    /// 配置
    config: NsjailConfig,
    /// 进程
    process: Option<Child>,
    /// 状态
    status: Arc<RwLock<SandboxStatus>>,
    /// 创建时间
    created_at: OffsetDateTime,
    /// cgroup 路径
    cgroup_path: Option<PathBuf>,
    /// 环境变量（用于凭证注入）
    credential_env: HashMap<String, String>,
}

impl NsjailSandbox {
    /// 创建新的沙箱实例
    pub fn new(config: NsjailConfig) -> Self {
        Self {
            id: SandboxId::new(),
            config,
            process: None,
            status: Arc::new(RwLock::new(SandboxStatus::Creating)),
            created_at: OffsetDateTime::now_utc(),
            cgroup_path: None,
            credential_env: HashMap::new(),
        }
    }

    /// 启动沙箱
    pub async fn start(&mut self) -> Result<(), SandboxError> {
        {
            let status = self.status.read().await;
            if *status != SandboxStatus::Creating {
                return Err(SandboxError::AlreadyRunning {
                    sandbox_id: self.id.into(),
                });
            }
        }

        info!("Starting nsjail sandbox: {}", self.id);

        // 准备工作目录
        self.prepare_working_dir().await?;

        // 设置 cgroup
        self.setup_cgroup().await?;

        // 构建 nsjail 命令
        let nsjail_path = &self.config.sandbox.nsjail_path;
        let args = self.config.to_args();

        debug!("Running: {} {:?}", nsjail_path.display(), args);

        // 启动进程
        let mut cmd = Command::new(nsjail_path);
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());

        // 设置环境变量（包含凭证）
        for (key, value) in &self.credential_env {
            cmd.env(key, value);
        }

        match cmd.spawn() {
            Ok(child) => {
                let pid = child.id().unwrap_or(0);
                info!("Nsjail sandbox started: {} (PID: {})", self.id, pid);
                self.process = Some(child);
                *self.status.write().await = SandboxStatus::Running;
                Ok(())
            }
            Err(e) => {
                error!("Failed to start nsjail sandbox {}: {}", self.id, e);
                *self.status.write().await = SandboxStatus::Error;
                Err(SandboxError::Process(format!(
                    "Failed to start nsjail: {}",
                    e
                )))
            }
        }
    }

    /// 停止沙箱
    pub async fn stop(&mut self) -> Result<(), SandboxError> {
        let mut status = self.status.write().await;

        if *status == SandboxStatus::Closed {
            return Ok(());
        }

        info!("Stopping nsjail sandbox: {}", self.id);

        if let Some(mut process) = self.process.take() {
            // 尝试优雅终止
            match process.start_kill() {
                Ok(_) => {
                    // 等待进程退出
                    let timeout = tokio::time::Duration::from_secs(5);
                    match tokio::time::timeout(timeout, process.wait()).await {
                        Ok(Ok(_)) => {
                            info!("Nsjail sandbox {} stopped gracefully", self.id);
                        }
                        _ => {
                            warn!(
                                "Nsjail sandbox {} did not stop gracefully, killing",
                                self.id
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to kill nsjail sandbox {}: {}", self.id, e);
                }
            }
        }

        // 清理 cgroup
        if let Some(ref cgroup_path) = self.cgroup_path {
            if let Err(e) = self.cleanup_cgroup(cgroup_path).await {
                warn!("Failed to cleanup cgroup for {}: {}", self.id, e);
            }
        }

        // 清理工作目录
        if let Err(e) = self.cleanup_working_dir().await {
            warn!("Failed to cleanup working dir for {}: {}", self.id, e);
        }

        *status = SandboxStatus::Closed;
        info!("Nsjail sandbox {} stopped and cleaned up", self.id);

        Ok(())
    }

    /// 检查沙箱是否正在运行
    pub async fn is_running(&self) -> bool {
        let status = self.status.read().await;
        if *status != SandboxStatus::Running {
            return false;
        }

        // 检查进程状态
        if let Some(ref process) = self.process {
            if let Some(pid) = process.id() {
                return Self::check_process_running(pid);
            }
        }

        false
    }

    /// 获取沙箱状态
    pub async fn status(&self) -> SandboxStatus {
        *self.status.read().await
    }

    /// 获取进程 ID
    pub fn pid(&self) -> Option<u32> {
        self.process.as_ref().and_then(|p| p.id())
    }

    /// 设置凭证环境变量
    pub fn set_credential_env(&mut self, key: String, value: String) {
        self.credential_env.insert(key, value);
    }

    /// 获取沙箱统计信息
    pub async fn stats(&self) -> Result<SandboxStats, SandboxError> {
        if !self.is_running().await {
            return Err(SandboxError::NotRunning {
                sandbox_id: self.id.into(),
            });
        }

        // 获取进程统计信息
        let pid = self
            .process
            .as_ref()
            .and_then(|p| p.id())
            .ok_or_else(|| SandboxError::process("No process ID available"))?;

        let stats = Self::read_process_stats(pid).await?;

        Ok(stats)
    }

    /// 准备沙箱以复用（热实例）
    pub async fn prepare_for_reuse(&mut self) -> Result<(), SandboxError> {
        // 清理会话状态，但保持进程运行
        self.credential_env.clear();
        Ok(())
    }

    // 私有辅助方法

    async fn prepare_working_dir(&self) -> Result<(), SandboxError> {
        let work_dir = self.config.sandbox.working_dir.join(self.id.to_string());
        tokio::fs::create_dir_all(&work_dir)
            .await
            .map_err(|e| SandboxError::Io(e))?;
        Ok(())
    }

    async fn cleanup_working_dir(&self) -> Result<(), SandboxError> {
        let work_dir = self.config.sandbox.working_dir.join(self.id.to_string());
        if work_dir.exists() {
            tokio::fs::remove_dir_all(&work_dir)
                .await
                .map_err(|e| SandboxError::Io(e))?;
        }
        Ok(())
    }

    async fn setup_cgroup(&mut self) -> Result<(), SandboxError> {
        let cgroup_config = &self.config.sandbox.security.cgroup;
        let cgroup_path = cgroup_config
            .cgroup_root
            .join("credbridge")
            .join("sandbox")
            .join(self.id.to_string());

        tokio::fs::create_dir_all(&cgroup_path)
            .await
            .map_err(|e| SandboxError::Security(SecurityError::Cgroup(e.to_string())))?;

        // 设置资源限制
        let limits = &self.config.sandbox.resource_limits;

        // CPU 限制 (cgroup v2)
        let cpu_max_path = cgroup_path.join("cpu.max");
        let cpu_quota = limits.cpu_percent * 1000; // Convert to microseconds
        let cpu_max = format!("{} 100000", cpu_quota);
        tokio::fs::write(&cpu_max_path, cpu_max)
            .await
            .map_err(|e| SandboxError::Security(SecurityError::Cgroup(e.to_string())))?;

        // 内存限制
        let memory_max_path = cgroup_path.join("memory.max");
        let memory_limit = limits.memory_limit_mb * 1024 * 1024;
        tokio::fs::write(&memory_max_path, memory_limit.to_string())
            .await
            .map_err(|e| SandboxError::Security(SecurityError::Cgroup(e.to_string())))?;

        // PIDs 限制
        let pids_max_path = cgroup_path.join("pids.max");
        tokio::fs::write(&pids_max_path, limits.max_pids.to_string())
            .await
            .map_err(|e| SandboxError::Security(SecurityError::Cgroup(e.to_string())))?;

        self.cgroup_path = Some(cgroup_path);
        Ok(())
    }

    async fn cleanup_cgroup(&self, cgroup_path: &PathBuf) -> Result<(), SandboxError> {
        if cgroup_path.exists() {
            tokio::fs::remove_dir(cgroup_path)
                .await
                .map_err(|e| SandboxError::Security(SecurityError::Cgroup(e.to_string())))?;
        }
        Ok(())
    }

    fn check_process_running(pid: u32) -> bool {
        // 检查进程是否存在
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }

    async fn read_process_stats(pid: u32) -> Result<SandboxStats, SandboxError> {
        let stat_path = format!("/proc/{}/stat", pid);
        let stat_content = tokio::fs::read_to_string(&stat_path)
            .await
            .map_err(|e| SandboxError::Io(e))?;

        // 解析 /proc/PID/stat
        // 格式: pid (comm) state ppid pgrp session tty_nr tpgid flags minflt cminflt majflt cmajflt utime stime cutime cstime priority nice num_threads itrealvalue starttime vsize rss rsslim ...
        let parts: Vec<&str> = stat_content.split_whitespace().collect();

        if parts.len() < 24 {
            return Err(SandboxError::process("Invalid stat format"));
        }

        // 解析内存使用 (RSS in pages, convert to bytes)
        let rss_pages: u64 = parts[23].parse().unwrap_or(0);
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGE_SIZE) as u64 };
        let memory_usage_bytes = rss_pages * page_size;

        // 解析虚拟内存 (vsize in bytes)
        let virtual_memory: u64 = parts[22].parse().unwrap_or(0);

        // 解析 CPU 时间 (utime + stime in clock ticks)
        let utime: u64 = parts[13].parse().unwrap_or(0);
        let stime: u64 = parts[14].parse().unwrap_or(0);
        let clock_ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) as u64 };
        let cpu_time_ms = ((utime + stime) * 1000) / clock_ticks;

        // 读取打开的文件描述符数
        let fd_count = Self::count_open_fds(pid).await.unwrap_or(0);

        Ok(SandboxStats {
            pid,
            memory_usage_bytes,
            virtual_memory_bytes: virtual_memory,
            cpu_time_ms,
            fd_count,
        })
    }

    async fn count_open_fds(pid: u32) -> Result<usize, std::io::Error> {
        let fd_dir = format!("/proc/{}/fd", pid);
        let mut entries = tokio::fs::read_dir(&fd_dir).await?;
        let mut count = 0;

        while entries.next_entry().await?.is_some() {
            count += 1;
        }

        Ok(count)
    }
}

/// 沙箱统计信息
#[derive(Debug, Clone)]
pub struct SandboxStats {
    /// 进程 ID
    pub pid: u32,
    /// 内存使用量（字节）
    pub memory_usage_bytes: u64,
    /// 虚拟内存（字节）
    pub virtual_memory_bytes: u64,
    /// CPU 时间（毫秒）
    pub cpu_time_ms: u64,
    /// 打开的文件描述符数
    pub fd_count: usize,
}

/// 热 nsjail 实例
pub struct WarmNsjailInstance {
    /// 实例信息
    pub info: WarmInstanceInfo,
    /// 沙箱实例
    pub sandbox: Option<NsjailSandbox>,
    /// 最后检查时间
    pub last_health_check: OffsetDateTime,
}

impl WarmNsjailInstance {
    /// 创建新的热实例
    pub fn new(instance_id: SandboxId, pid: u32) -> Self {
        Self {
            info: WarmInstanceInfo {
                instance_id,
                created_at: OffsetDateTime::now_utc(),
                last_used_at: None,
                pid,
            },
            sandbox: None,
            last_health_check: OffsetDateTime::now_utc(),
        }
    }

    /// 检查实例是否健康
    pub fn is_healthy(&self) -> bool {
        // 检查进程是否仍在运行
        unsafe { libc::kill(self.info.pid as i32, 0) == 0 }
    }

    /// 检查是否过期
    pub fn is_expired(&self, ttl_secs: u64) -> bool {
        let now = OffsetDateTime::now_utc();
        let age = now - self.info.created_at;
        age.whole_seconds() as u64 > ttl_secs
    }

    /// 更新使用时间
    pub fn touch(&mut self) {
        self.info.last_used_at = Some(OffsetDateTime::now_utc());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tee::sandbox::config::{SandboxConfig, SandboxPoolConfig};

    fn create_test_config() -> NsjailConfig {
        NsjailConfig {
            sandbox: SandboxConfig::default(),
            command: vec!["sleep".to_string(), "100".to_string()],
            cwd: PathBuf::from("/"),
            env: HashMap::new(),
            uid_map: Default::default(),
            gid_map: Default::default(),
        }
    }

    #[test]
    fn test_nsjail_sandbox_creation() {
        let config = create_test_config();
        let sandbox = NsjailSandbox::new(config);

        assert_eq!(sandbox.process.is_none(), true);
    }

    #[test]
    fn test_sandbox_stats_creation() {
        let stats = SandboxStats {
            pid: 1234,
            memory_usage_bytes: 1024 * 1024,
            virtual_memory_bytes: 1024 * 1024 * 10,
            cpu_time_ms: 1000,
            fd_count: 10,
        };

        assert_eq!(stats.pid, 1234);
        assert_eq!(stats.memory_usage_bytes, 1048576);
    }

    #[tokio::test]
    async fn test_warm_instance_expired() {
        let instance_id = SandboxId::new();
        let mut instance = WarmNsjailInstance::new(instance_id, 1234);

        // 修改创建时间使其过期
        instance.info.created_at = OffsetDateTime::now_utc() - time::Duration::seconds(400);

        assert!(instance.is_expired(300)); // 300秒TTL
        assert!(!instance.is_expired(500)); // 500秒TTL
    }
}
