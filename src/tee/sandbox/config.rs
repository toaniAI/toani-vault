//! 沙箱配置定义

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

/// 沙箱配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// 沙箱池配置
    pub pool: SandboxPoolConfig,
    /// 资源限制配置
    pub resource_limits: ResourceLimits,
    /// 安全配置
    pub security: SecurityConfig,
    /// 超时配置（秒）
    pub timeout_secs: u64,
    /// 工作目录
    pub working_dir: PathBuf,
    /// nsjail 可执行文件路径
    pub nsjail_path: PathBuf,
    /// 环境变量
    pub env_vars: HashMap<String, String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            pool: SandboxPoolConfig::default(),
            resource_limits: ResourceLimits::default(),
            security: SecurityConfig::default(),
            timeout_secs: 300,
            working_dir: PathBuf::from("/tmp/sandbox"),
            nsjail_path: resolve_nsjail_path(),
            env_vars: HashMap::new(),
        }
    }
}

impl SandboxConfig {
    /// 从环境变量加载沙箱配置。
    ///
    /// 当前仅覆盖开发环境里最容易漂移的 nsjail 路径，其余字段保持默认值。
    pub fn from_env() -> Self {
        Self {
            nsjail_path: resolve_nsjail_path(),
            ..Self::default()
        }
    }
}

fn resolve_nsjail_path() -> PathBuf {
    for env_name in ["CREDBRIDGE_SANDBOX_NSJAIL_PATH", "NSJAIL_PATH"] {
        if let Ok(path) = env::var(env_name) {
            let trimmed = path.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed);
            }
        }
    }

    for candidate in [
        Path::new("/usr/bin/nsjail"),
        Path::new("/usr/local/bin/nsjail"),
    ] {
        if candidate.exists() {
            return candidate.to_path_buf();
        }
    }

    PathBuf::from("/usr/bin/nsjail")
}

/// 沙箱池配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxPoolConfig {
    /// 最大热实例数
    pub max_warm_instances: usize,
    /// 热实例 TTL（秒）
    pub warm_instance_ttl_secs: u64,
    /// 会话超时（分钟）
    pub session_timeout_minutes: u64,
    /// 清理间隔（秒）
    pub cleanup_interval_secs: u64,
    /// 最大并发会话数
    pub max_concurrent_sessions: usize,
    /// 最小热实例数
    pub min_warm_instances: usize,
}

impl Default for SandboxPoolConfig {
    fn default() -> Self {
        Self {
            max_warm_instances: 10,
            warm_instance_ttl_secs: 300,
            session_timeout_minutes: 30,
            cleanup_interval_secs: 60,
            max_concurrent_sessions: 100,
            min_warm_instances: 2,
        }
    }
}

/// 资源限制
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// CPU 限制（百分比）
    pub cpu_percent: u32,
    /// 内存限制（MB）
    pub memory_limit_mb: u64,
    /// 最大进程数
    pub max_pids: u64,
    /// 最大文件描述符数
    pub max_fds: u64,
    /// 磁盘读取限制（MB/s）
    pub disk_read_mbps: Option<u64>,
    /// 磁盘写入限制（MB/s）
    pub disk_write_mbps: Option<u64>,
    /// 网络带宽限制（MB/s）
    pub network_mbps: Option<u64>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu_percent: 50,
            memory_limit_mb: 512,
            max_pids: 50,
            max_fds: 1024,
            disk_read_mbps: None,
            disk_write_mbps: None,
            network_mbps: None,
        }
    }
}

/// 安全配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Namespace 配置
    pub namespace: NamespaceConfig,
    /// cgroup 配置
    pub cgroup: CgroupConfig,
    /// seccomp 配置
    pub seccomp: SeccompConfig,
    /// 启用特权模式（仅用于测试）
    pub privileged: bool,
    /// 只读文件系统
    pub read_only_root: bool,
    /// 禁止新特权
    pub no_new_privs: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            namespace: NamespaceConfig::default(),
            cgroup: CgroupConfig::default(),
            seccomp: SeccompConfig::default(),
            privileged: false,
            read_only_root: true,
            no_new_privs: true,
        }
    }
}

/// Namespace 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceConfig {
    /// 启用 PID namespace
    pub pid: bool,
    /// 启用 Network namespace
    pub network: bool,
    /// 启用 Mount namespace
    pub mount: bool,
    /// 启用 IPC namespace
    pub ipc: bool,
    /// 启用 UTS namespace
    pub uts: bool,
    /// 启用 User namespace
    pub user: bool,
    /// 启用 Cgroup namespace
    pub cgroup: bool,
    /// 挂载点配置
    pub mount_points: Vec<MountConfig>,
}

impl Default for NamespaceConfig {
    fn default() -> Self {
        Self {
            pid: true,
            network: true,
            mount: true,
            ipc: true,
            uts: true,
            user: true,
            cgroup: true,
            mount_points: vec![
                MountConfig {
                    src: PathBuf::from("/bin"),
                    dst: PathBuf::from("/bin"),
                    mount_type: MountType::Bind,
                    read_only: true,
                },
                MountConfig {
                    src: PathBuf::from("/lib"),
                    dst: PathBuf::from("/lib"),
                    mount_type: MountType::Bind,
                    read_only: true,
                },
                MountConfig {
                    src: PathBuf::from("/lib64"),
                    dst: PathBuf::from("/lib64"),
                    mount_type: MountType::Bind,
                    read_only: true,
                },
                MountConfig {
                    src: PathBuf::from("/usr"),
                    dst: PathBuf::from("/usr"),
                    mount_type: MountType::Bind,
                    read_only: true,
                },
            ],
        }
    }
}

/// 挂载配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountConfig {
    /// 源路径
    pub src: PathBuf,
    /// 目标路径
    pub dst: PathBuf,
    /// 挂载类型
    pub mount_type: MountType,
    /// 是否只读
    pub read_only: bool,
}

/// 挂载类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MountType {
    /// 绑定挂载
    Bind,
    /// tmpfs
    Tmpfs,
    /// proc
    Proc,
    /// sysfs
    Sysfs,
    /// devfs
    Devfs,
}

/// cgroup 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CgroupConfig {
    /// cgroup 版本
    pub version: CgroupVersion,
    /// cgroup 根路径
    pub cgroup_root: PathBuf,
    /// 控制器配置
    pub controllers: Vec<String>,
}

impl Default for CgroupConfig {
    fn default() -> Self {
        Self {
            version: CgroupVersion::V2,
            cgroup_root: PathBuf::from("/sys/fs/cgroup"),
            controllers: vec![
                "cpu".to_string(),
                "memory".to_string(),
                "pids".to_string(),
                "io".to_string(),
            ],
        }
    }
}

/// cgroup 版本
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CgroupVersion {
    /// cgroup v1
    V1,
    /// cgroup v2
    V2,
}

/// seccomp 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeccompConfig {
    /// 模式
    pub mode: SeccompMode,
    /// 默认策略
    pub default_policy: SeccompPolicy,
    /// 额外的允许列表
    pub allowlist: Vec<String>,
    /// 额外的拒绝列表
    pub denylist: Vec<String>,
    /// 自定义 BPF 程序路径
    pub custom_bpf_path: Option<PathBuf>,
}

impl Default for SeccompConfig {
    fn default() -> Self {
        Self {
            mode: SeccompMode::Allowlist,
            default_policy: SeccompPolicy::Browser,
            allowlist: Vec::new(),
            denylist: vec![
                "execve".to_string(),
                "execveat".to_string(),
                "fork".to_string(),
                "vfork".to_string(),
                "clone".to_string(),
                "ptrace".to_string(),
                "process_vm_writev".to_string(),
            ],
            custom_bpf_path: None,
        }
    }
}

/// seccomp 模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeccompMode {
    /// 白名单模式
    Allowlist,
    /// 黑名单模式
    Denylist,
    /// 自定义 BPF
    Custom,
}

/// seccomp 策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeccompPolicy {
    /// 浏览器沙箱策略
    Browser,
    /// 最小权限策略
    Minimal,
    /// 网络服务策略
    Network,
    /// 自定义策略
    Custom,
}

/// Nsjail 配置
#[derive(Debug, Clone)]
pub struct NsjailConfig {
    /// 基本配置
    pub sandbox: SandboxConfig,
    /// 命令
    pub command: Vec<String>,
    /// 工作目录
    pub cwd: PathBuf,
    /// 环境变量
    pub env: HashMap<String, String>,
    /// UID 映射
    pub uid_map: UidMap,
    /// GID 映射
    pub gid_map: GidMap,
}

impl Default for NsjailConfig {
    fn default() -> Self {
        Self {
            sandbox: SandboxConfig::default(),
            command: vec!["sh".to_string()],
            cwd: PathBuf::from("/"),
            env: HashMap::new(),
            uid_map: UidMap::default(),
            gid_map: GidMap::default(),
        }
    }
}

/// UID 映射
#[derive(Debug, Clone)]
pub struct UidMap {
    /// 外部 UID
    pub outside_uid: u32,
    /// 内部 UID
    pub inside_uid: u32,
    /// 数量
    pub count: u32,
}

impl Default for UidMap {
    fn default() -> Self {
        Self {
            outside_uid: 1000,
            inside_uid: 0,
            count: 1,
        }
    }
}

/// GID 映射
#[derive(Debug, Clone)]
pub struct GidMap {
    /// 外部 GID
    pub outside_gid: u32,
    /// 内部 GID
    pub inside_gid: u32,
    /// 数量
    pub count: u32,
}

impl Default for GidMap {
    fn default() -> Self {
        Self {
            outside_gid: 1000,
            inside_gid: 0,
            count: 1,
        }
    }
}

impl NsjailConfig {
    /// 生成 nsjail 命令行参数
    pub fn to_args(&self) -> Vec<String> {
        let mut args = vec!["--mode".to_string(), "o".to_string()]; // One-shot mode

        // Namespace
        let ns = &self.sandbox.security.namespace;
        if ns.pid {
            args.push("--disable_clone_newpid".to_string());
            args.push("false".to_string());
        }
        if ns.network {
            args.push("--disable_clone_newnet".to_string());
            args.push("false".to_string());
        }
        if ns.mount {
            args.push("--disable_clone_newmnt".to_string());
            args.push("false".to_string());
        }
        if ns.ipc {
            args.push("--disable_clone_newipc".to_string());
            args.push("false".to_string());
        }
        if ns.uts {
            args.push("--disable_clone_newuts".to_string());
            args.push("false".to_string());
        }
        if ns.user {
            args.push("--disable_clone_newuser".to_string());
            args.push("false".to_string());
        }

        // Resource limits
        let limits = &self.sandbox.resource_limits;
        args.push("--max_cpus".to_string());
        args.push(format!("{}", limits.cpu_percent / 100 + 1));

        args.push("--rlimit_as".to_string());
        args.push(format!("{}", limits.memory_limit_mb * 1024 * 1024));

        args.push("--rlimit_nproc".to_string());
        args.push(format!("{}", limits.max_pids));

        args.push("--rlimit_nofile".to_string());
        args.push(format!("{}", limits.max_fds));

        // Mount points
        for mount in &ns.mount_points {
            args.push("--bindmount_ro".to_string());
            args.push(format!("{}:{}", mount.src.display(), mount.dst.display()));
        }

        // Seccomp
        if !self.sandbox.security.privileged {
            args.push("--seccomp_string".to_string());
            args.push(self.generate_seccomp_bpf());
        }

        // Working directory
        args.push("--cwd".to_string());
        args.push(self.cwd.to_string_lossy().to_string());

        // Environment variables
        for (key, value) in &self.env {
            args.push("--env".to_string());
            args.push(format!("{key}={value}"));
        }

        // UID/GID mapping
        args.push("--uid_mapping".to_string());
        args.push(format!(
            "{}:{}:{}",
            self.uid_map.outside_uid, self.uid_map.inside_uid, self.uid_map.count
        ));

        args.push("--gid_mapping".to_string());
        args.push(format!(
            "{}:{}:{}",
            self.gid_map.outside_gid, self.gid_map.inside_gid, self.gid_map.count
        ));

        // Command
        args.push("--".to_string());
        args.extend(self.command.clone());

        args
    }

    /// 生成 seccomp BPF 字符串
    fn generate_seccomp_bpf(&self) -> String {
        // 简化的 seccomp 策略
        // 实际实现中需要更完整的 BPF 程序
        let denylist = &self.sandbox.security.seccomp.denylist;
        let policy = match self.sandbox.security.seccomp.default_policy {
            SeccompPolicy::Browser => "POLICY browser {",
            SeccompPolicy::Minimal => "POLICY minimal {",
            SeccompPolicy::Network => "POLICY network {",
            SeccompPolicy::Custom => "POLICY custom {",
        };

        let mut bpf = format!("{policy}\n");

        // 添加拒绝的系统调用
        for syscall in denylist {
            bpf.push_str(&format!("  DENY {syscall}\n"));
        }

        bpf.push_str("  ALLOW_ALL\n}");
        bpf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_sandbox_config() {
        let config = SandboxConfig::default();
        assert_eq!(config.timeout_secs, 300);
        assert_eq!(config.pool.max_warm_instances, 10);
        assert_eq!(config.resource_limits.memory_limit_mb, 512);
        assert!(!config.nsjail_path.as_os_str().is_empty());
    }

    #[test]
    fn test_nsjail_config_to_args() {
        let config = NsjailConfig::default();
        let args = config.to_args();
        assert!(args.contains(&"--mode".to_string()));
        assert!(args.contains(&"o".to_string()));
    }

    #[test]
    fn test_seccomp_mode() {
        assert_eq!(
            serde_json::to_string(&SeccompMode::Allowlist).unwrap(),
            "\"allowlist\""
        );
        assert_eq!(
            serde_json::to_string(&SeccompMode::Denylist).unwrap(),
            "\"denylist\""
        );
    }

    #[test]
    fn test_from_env_prefers_nsjail_path_override() {
        unsafe {
            std::env::set_var("NSJAIL_PATH", "/custom/nsjail");
        }

        let config = SandboxConfig::from_env();
        assert_eq!(config.nsjail_path, PathBuf::from("/custom/nsjail"));

        unsafe {
            std::env::remove_var("NSJAIL_PATH");
        }
    }
}
