//! cgroup 资源限制管理
//!
//! 支持 cgroup v2（统一层次结构）

use crate::tee::sandbox::error::SecurityError;
use std::path::PathBuf;
use tokio::fs;
use tracing::{debug, info};
use uuid::Uuid;

/// cgroup 管理器
pub struct CgroupManager {
    /// cgroup 根路径
    cgroup_root: PathBuf,
    /// 沙箱 cgroup 路径
    sandbox_cgroup: PathBuf,
}

/// 资源限制
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// CPU 限制（百分比，0-100）
    pub cpu_percent: u32,
    /// 内存限制（字节）
    pub memory_bytes: u64,
    /// 最大进程数
    pub max_pids: u64,
    /// IO 权重（1-10000）
    pub io_weight: Option<u16>,
    /// 读取带宽限制（bytes/秒）
    pub io_read_bps: Option<u64>,
    /// 写入带宽限制（bytes/秒）
    pub io_write_bps: Option<u64>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu_percent: 50,
            memory_bytes: 512 * 1024 * 1024, // 512MB
            max_pids: 50,
            io_weight: Some(100),
            io_read_bps: None,
            io_write_bps: None,
        }
    }
}

impl CgroupManager {
    /// 创建新的 cgroup 管理器
    pub fn new(sandbox_id: Uuid) -> Result<Self, SecurityError> {
        let cgroup_root = PathBuf::from("/sys/fs/cgroup");

        // 检查 cgroup v2 是否可用
        if !cgroup_root.join("cgroup.controllers").exists() {
            return Err(SecurityError::Cgroup("cgroup v2 not available".to_string()));
        }

        let sandbox_cgroup = cgroup_root
            .join("credbridge")
            .join("sandbox")
            .join(sandbox_id.to_string());

        Ok(Self {
            cgroup_root,
            sandbox_cgroup,
        })
    }

    /// 创建 cgroup
    pub async fn create(&self) -> Result<(), SecurityError> {
        // 确保父目录存在
        let parent = self.sandbox_cgroup.parent().unwrap();
        fs::create_dir_all(parent)
            .await
            .map_err(|e| SecurityError::Cgroup(format!("Failed to create parent cgroup: {}", e)))?;

        // 创建沙箱 cgroup
        fs::create_dir_all(&self.sandbox_cgroup)
            .await
            .map_err(|e| SecurityError::Cgroup(format!("Failed to create cgroup: {}", e)))?;

        debug!("Created cgroup at {:?}", self.sandbox_cgroup);
        Ok(())
    }

    /// 删除 cgroup
    pub async fn delete(&self) -> Result<(), SecurityError> {
        if self.sandbox_cgroup.exists() {
            // 确保 cgroup 为空
            let procs = self.read_processes().await?;
            for pid in procs {
                self.remove_process(pid).await?;
            }

            fs::remove_dir(&self.sandbox_cgroup)
                .await
                .map_err(|e| SecurityError::Cgroup(format!("Failed to delete cgroup: {}", e)))?;

            debug!("Deleted cgroup at {:?}", self.sandbox_cgroup);
        }

        Ok(())
    }

    /// 应用资源限制
    pub async fn apply_limits(&self, limits: &ResourceLimits) -> Result<(), SecurityError> {
        // CPU 限制 (quota 和 period)
        // cpu.max 格式: "quota period"，quota 以微秒为单位
        let cpu_quota = (limits.cpu_percent as u64 * 1000) / 100; // Convert to proportion
        let cpu_max = format!("{} 100000", cpu_quota * 1000);
        self.write_cgroup_file("cpu.max", cpu_max).await?;

        // 内存限制
        self.write_cgroup_file("memory.max", limits.memory_bytes.to_string())
            .await?;

        // 启用内存交换限制（如果可用）
        let swap_max_path = self.sandbox_cgroup.join("memory.swap.max");
        if swap_max_path.exists() {
            self.write_cgroup_file("memory.swap.max", limits.memory_bytes.to_string())
                .await?;
        }

        // 进程数限制
        self.write_cgroup_file("pids.max", limits.max_pids.to_string())
            .await?;

        // IO 限制
        if let Some(weight) = limits.io_weight {
            self.write_cgroup_file("io.weight", weight.to_string())
                .await?;
        }

        info!(
            "Applied cgroup limits: CPU {}%, Memory {}MB, PIDs {}",
            limits.cpu_percent,
            limits.memory_bytes / (1024 * 1024),
            limits.max_pids
        );

        Ok(())
    }

    /// 添加进程到 cgroup
    pub async fn add_process(&self, pid: u32) -> Result<(), SecurityError> {
        self.write_cgroup_file("cgroup.procs", pid.to_string())
            .await?;
        debug!("Added process {} to cgroup {:?}", pid, self.sandbox_cgroup);
        Ok(())
    }

    /// 从 cgroup 移除进程
    pub async fn remove_process(&self, pid: u32) -> Result<(), SecurityError> {
        // 将进程移动到根 cgroup
        let root_procs = self.cgroup_root.join("cgroup.procs");
        fs::write(&root_procs, pid.to_string()).await.map_err(|e| {
            SecurityError::Cgroup(format!("Failed to remove process from cgroup: {}", e))
        })?;
        Ok(())
    }

    /// 读取 cgroup 中的进程列表
    pub async fn read_processes(&self) -> Result<Vec<u32>, SecurityError> {
        let content = self.read_cgroup_file("cgroup.procs").await?;
        let pids: Vec<u32> = content
            .lines()
            .filter_map(|line| line.trim().parse().ok())
            .collect();
        Ok(pids)
    }

    /// 获取内存使用量
    pub async fn get_memory_usage(&self) -> Result<u64, SecurityError> {
        let content = self.read_cgroup_file("memory.current").await?;
        content
            .trim()
            .parse()
            .map_err(|e| SecurityError::Cgroup(format!("Failed to parse memory usage: {}", e)))
    }

    /// 获取内存限制
    pub async fn get_memory_limit(&self) -> Result<u64, SecurityError> {
        let content = self.read_cgroup_file("memory.max").await?;
        content
            .trim()
            .parse()
            .map_err(|e| SecurityError::Cgroup(format!("Failed to parse memory limit: {}", e)))
    }

    /// 获取 CPU 统计
    pub async fn get_cpu_stats(&self) -> Result<CpuStats, SecurityError> {
        let content = self.read_cgroup_file("cpu.stat").await?;
        let mut stats = CpuStats::default();

        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() == 2 {
                match parts[0] {
                    "usage_usec" => {
                        stats.usage_usec = parts[1].parse().unwrap_or(0);
                    }
                    "user_usec" => {
                        stats.user_usec = parts[1].parse().unwrap_or(0);
                    }
                    "system_usec" => {
                        stats.system_usec = parts[1].parse().unwrap_or(0);
                    }
                    "nr_periods" => {
                        stats.nr_periods = parts[1].parse().unwrap_or(0);
                    }
                    "nr_throttled" => {
                        stats.nr_throttled = parts[1].parse().unwrap_or(0);
                    }
                    "throttled_usec" => {
                        stats.throttled_usec = parts[1].parse().unwrap_or(0);
                    }
                    _ => {}
                }
            }
        }

        Ok(stats)
    }

    /// 检查是否超出资源限制
    pub async fn check_limits(&self, limits: &ResourceLimits) -> Result<(), SecurityError> {
        let memory_usage = self.get_memory_usage().await?;
        let memory_limit = limits.memory_bytes;

        if memory_usage >= memory_limit {
            return Err(SecurityError::ResourceLimitExceeded {
                resource: "memory".to_string(),
                current: memory_usage,
                limit: memory_limit,
            });
        }

        let pids = self.read_processes().await?;
        if pids.len() as u64 >= limits.max_pids {
            return Err(SecurityError::ResourceLimitExceeded {
                resource: "pids".to_string(),
                current: pids.len() as u64,
                limit: limits.max_pids,
            });
        }

        Ok(())
    }

    // 辅助方法

    async fn write_cgroup_file(
        &self,
        filename: &str,
        content: impl AsRef<[u8]>,
    ) -> Result<(), SecurityError> {
        let path = self.sandbox_cgroup.join(filename);
        fs::write(&path, content)
            .await
            .map_err(|e| SecurityError::Cgroup(format!("Failed to write {}: {}", filename, e)))
    }

    async fn read_cgroup_file(&self, filename: &str) -> Result<String, SecurityError> {
        let path = self.sandbox_cgroup.join(filename);
        fs::read_to_string(&path)
            .await
            .map_err(|e| SecurityError::Cgroup(format!("Failed to read {}: {}", filename, e)))
    }
}

/// CPU 统计信息
#[derive(Debug, Clone, Default)]
pub struct CpuStats {
    /// CPU 使用时间（微秒）
    pub usage_usec: u64,
    /// 用户态时间（微秒）
    pub user_usec: u64,
    /// 内核态时间（微秒）
    pub system_usec: u64,
    /// 周期数
    pub nr_periods: u64,
    /// 被限制次数
    pub nr_throttled: u64,
    /// 被限制时间（微秒）
    pub throttled_usec: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.cpu_percent, 50);
        assert_eq!(limits.memory_bytes, 512 * 1024 * 1024);
        assert_eq!(limits.max_pids, 50);
    }

    #[test]
    fn test_cgroup_manager_creation() {
        // 测试创建管理器（可能在非 Linux 环境下失败）
        let _result = CgroupManager::new(Uuid::new_v4());
        // 结果取决于运行环境
    }

    #[test]
    fn test_cpu_stats_default() {
        let stats = CpuStats::default();
        assert_eq!(stats.usage_usec, 0);
        assert_eq!(stats.nr_throttled, 0);
    }
}
