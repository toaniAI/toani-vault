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
        let mut config = Self {
            nsjail_path: resolve_nsjail_path(),
            ..Self::default()
        };

        if let Some(enabled) = env_bool("CREDBRIDGE_SANDBOX_CGROUP_ENABLED") {
            config.security.cgroup.enabled = enabled;
        }

        if let Some(required) = env_bool("CREDBRIDGE_SANDBOX_CGROUP_REQUIRED") {
            config.security.cgroup.required = required;
        }

        if let Ok(root) = env::var("CREDBRIDGE_SANDBOX_CGROUP_ROOT") {
            let trimmed = root.trim();
            if !trimmed.is_empty() {
                config.security.cgroup.cgroup_root = PathBuf::from(trimmed);
            }
        }

        config
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

fn env_bool(name: &str) -> Option<bool> {
    env::var(name).ok().and_then(|value| match value.trim() {
        "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON" => Some(true),
        "0" | "false" | "FALSE" | "no" | "NO" | "off" | "OFF" => Some(false),
        _ => None,
    })
}

fn default_true() -> bool {
    true
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
    /// 是否启用 cgroup 资源限制
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// cgroup 初始化失败时是否阻止沙箱启动
    #[serde(default)]
    pub required: bool,
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
            enabled: true,
            required: false,
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
    /// 为 browser runtime 使用单独的放宽 seccomp 策略，允许 node/playwright/chromium 启动。
    pub disable_seccomp_for_browser_runtime: bool,
    /// 是否为当前 jail 启用 user namespace 隔离。
    pub enable_user_namespace: bool,
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
            disable_seccomp_for_browser_runtime: false,
            enable_user_namespace: true,
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
            outside_uid: 100000,
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
            outside_gid: 100000,
            inside_gid: 0,
            count: 1,
        }
    }
}

impl NsjailConfig {
    const BROWSER_RUNTIME_RELAXED_SYSCALLS: [&'static str; 5] =
        ["execve", "execveat", "fork", "vfork", "clone"];

    /// 生成 nsjail 命令行参数
    pub fn to_args(&self) -> Vec<String> {
        let mut args = vec!["--mode".to_string(), "o".to_string()]; // One-shot mode

        // Namespace
        let ns = &self.sandbox.security.namespace;
        if !ns.pid {
            args.push("--disable_clone_newpid".to_string());
        }
        if !ns.network {
            args.push("--disable_clone_newnet".to_string());
        }
        if !ns.mount {
            // nsjail uses NEWNS (not NEWMNT) flag naming.
            args.push("--disable_clone_newns".to_string());
        }
        if !ns.ipc {
            args.push("--disable_clone_newipc".to_string());
        }
        if !ns.uts {
            args.push("--disable_clone_newuts".to_string());
        }
        if !ns.user || !self.enable_user_namespace {
            args.push("--disable_clone_newuser".to_string());
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
            match (mount.mount_type, mount.read_only) {
                (MountType::Bind, true) => args.push("--bindmount_ro".to_string()),
                (MountType::Bind, false) => args.push("--bindmount".to_string()),
                _ => args.push("--bindmount_ro".to_string()),
            }
            args.push(format!("{}:{}", mount.src.display(), mount.dst.display()));
        }

        // Seccomp
        if !self.sandbox.security.privileged {
            args.push("--seccomp_string".to_string());
            args.push(if self.disable_seccomp_for_browser_runtime {
                self.generate_browser_runtime_seccomp_bpf()
            } else {
                self.generate_seccomp_bpf()
            });
        }

        // Working directory
        args.push("--cwd".to_string());
        args.push(self.cwd.to_string_lossy().to_string());

        // Environment variables
        for (key, value) in &self.env {
            args.push("--env".to_string());
            args.push(format!("{key}={value}"));
        }

        if self.enable_user_namespace {
            // UID/GID mapping only applies when user namespace isolation is enabled.
            args.push("--uid_mapping".to_string());
            args.push(format!(
                "{}:{}:{}",
                self.uid_map.inside_uid, self.uid_map.outside_uid, self.uid_map.count
            ));

            args.push("--gid_mapping".to_string());
            args.push(format!(
                "{}:{}:{}",
                self.gid_map.inside_gid, self.gid_map.outside_gid, self.gid_map.count
            ));
        }

        // Command
        args.push("--".to_string());
        args.extend(self.command.clone());

        args
    }

    /// 生成 seccomp kafel 策略字符串（供 nsjail --seccomp_string 使用）
    fn generate_seccomp_bpf(&self) -> String {
        self.generate_seccomp_bpf_with_denylist(&self.sandbox.security.seccomp.denylist)
    }

    fn generate_browser_runtime_seccomp_bpf(&self) -> String {
        let denylist = self
            .sandbox
            .security
            .seccomp
            .denylist
            .iter()
            .filter(|syscall| !Self::BROWSER_RUNTIME_RELAXED_SYSCALLS.contains(&syscall.as_str()))
            .cloned()
            .collect::<Vec<_>>();

        self.generate_seccomp_bpf_with_denylist(&denylist)
    }

    fn generate_seccomp_bpf_with_denylist(&self, denylist: &[String]) -> String {
        let policy_name = match self.sandbox.security.seccomp.default_policy {
            SeccompPolicy::Browser => "browser",
            SeccompPolicy::Minimal => "minimal",
            SeccompPolicy::Network => "network",
            SeccompPolicy::Custom => "custom",
        };

        let mut bpf = format!("POLICY {policy_name} {{\n");

        if !denylist.is_empty() {
            // kafel 不接受末尾逗号，这里显式 join，避免生成无法编译的策略。
            let deny_list = denylist
                .iter()
                .map(|syscall| format!("    {syscall}"))
                .collect::<Vec<_>>()
                .join(",\n");
            bpf.push_str("  DENY {\n");
            bpf.push_str(&deny_list);
            bpf.push('\n');
            bpf.push_str("  }\n");
        }

        bpf.push_str("}\n");
        // kafel 策略结尾：指定默认行为
        bpf.push_str(&format!("USE {policy_name} DEFAULT ALLOW\n"));
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
        assert!(!args.contains(&"--disable_clone_newmnt".to_string()));
        assert!(!args.contains(&"false".to_string()));
    }

    #[test]
    fn test_nsjail_config_uses_writable_bindmount_when_requested() {
        let mut config = NsjailConfig::default();
        config.sandbox.security.namespace.mount_points = vec![MountConfig {
            src: PathBuf::from("/tmp/source"),
            dst: PathBuf::from("/tmp/destination"),
            mount_type: MountType::Bind,
            read_only: false,
        }];

        let args = config.to_args();

        assert!(args.contains(&"--bindmount".to_string()));
        assert!(!args.contains(&"--bindmount_ro".to_string()));
    }

    #[test]
    fn test_nsjail_config_disables_mount_namespace_with_newns_flag() {
        let mut config = NsjailConfig::default();
        config.sandbox.security.namespace.mount = false;
        let args = config.to_args();

        assert!(args.contains(&"--disable_clone_newns".to_string()));
        assert!(!args.contains(&"--disable_clone_newmnt".to_string()));
    }

    #[test]
    fn test_nsjail_config_skips_seccomp_for_browser_runtime() {
        let config = NsjailConfig {
            disable_seccomp_for_browser_runtime: true,
            ..NsjailConfig::default()
        };

        let args = config.to_args();

        assert!(args.contains(&"--seccomp_string".to_string()));
        let seccomp_idx = args
            .iter()
            .position(|arg| arg == "--seccomp_string")
            .expect("seccomp string should be present");
        let seccomp_policy = &args[seccomp_idx + 1];
        assert!(!seccomp_policy.contains("    execve\n"));
        assert!(!seccomp_policy.contains("    execveat\n"));
        assert!(!seccomp_policy.contains("    fork\n"));
        assert!(!seccomp_policy.contains("    vfork\n"));
        assert!(!seccomp_policy.contains("    clone\n"));
        assert!(seccomp_policy.contains("ptrace"));
        assert!(seccomp_policy.contains("process_vm_writev"));
    }

    #[test]
    fn test_nsjail_config_keeps_seccomp_enabled_by_default() {
        let config = NsjailConfig::default();

        let args = config.to_args();

        assert!(args.contains(&"--seccomp_string".to_string()));
    }

    #[test]
    fn test_nsjail_config_disables_userns_when_requested() {
        let config = NsjailConfig {
            enable_user_namespace: false,
            ..NsjailConfig::default()
        };

        let args = config.to_args();

        assert!(args.contains(&"--disable_clone_newuser".to_string()));
        assert!(!args.contains(&"--uid_mapping".to_string()));
        assert!(!args.contains(&"--gid_mapping".to_string()));
    }

    #[test]
    fn test_nsjail_config_defaults_to_non_root_inside_uid_gid() {
        let config = NsjailConfig::default();
        let args = config.to_args();
        let uid_mapping_idx = args
            .iter()
            .position(|arg| arg == "--uid_mapping")
            .expect("uid mapping arg should be present");
        let gid_mapping_idx = args
            .iter()
            .position(|arg| arg == "--gid_mapping")
            .expect("gid mapping arg should be present");

        assert_eq!(args[uid_mapping_idx + 1], "0:100000:1");
        assert_eq!(args[gid_mapping_idx + 1], "0:100000:1");
    }

    #[test]
    fn test_nsjail_config_uid_gid_mappings_use_inside_outside_count_order() {
        let config = NsjailConfig {
            uid_map: UidMap {
                inside_uid: 2000,
                outside_uid: 3000,
                count: 2,
            },
            gid_map: GidMap {
                inside_gid: 4000,
                outside_gid: 5000,
                count: 3,
            },
            ..NsjailConfig::default()
        };

        let args = config.to_args();
        let uid_mapping_idx = args
            .iter()
            .position(|arg| arg == "--uid_mapping")
            .expect("uid mapping arg should be present");
        let gid_mapping_idx = args
            .iter()
            .position(|arg| arg == "--gid_mapping")
            .expect("gid mapping arg should be present");

        assert_eq!(args[uid_mapping_idx + 1], "2000:3000:2");
        assert_eq!(args[gid_mapping_idx + 1], "4000:5000:3");
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
    fn test_generate_seccomp_bpf_does_not_emit_trailing_comma() {
        let config = NsjailConfig::default();
        let policy = config.generate_seccomp_bpf();

        assert!(policy.contains("DENY {\n"));
        assert!(!policy.contains(",\n  }\n"));
        assert!(policy.contains("USE browser DEFAULT ALLOW"));
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

    #[test]
    fn test_from_env_reads_cgroup_overrides() {
        unsafe {
            std::env::set_var("CREDBRIDGE_SANDBOX_CGROUP_ENABLED", "false");
            std::env::set_var("CREDBRIDGE_SANDBOX_CGROUP_REQUIRED", "true");
            std::env::set_var("CREDBRIDGE_SANDBOX_CGROUP_ROOT", "/tmp/cgroup-test");
        }

        let config = SandboxConfig::from_env();
        assert!(!config.security.cgroup.enabled);
        assert!(config.security.cgroup.required);
        assert_eq!(
            config.security.cgroup.cgroup_root,
            PathBuf::from("/tmp/cgroup-test")
        );

        unsafe {
            std::env::remove_var("CREDBRIDGE_SANDBOX_CGROUP_ENABLED");
            std::env::remove_var("CREDBRIDGE_SANDBOX_CGROUP_REQUIRED");
            std::env::remove_var("CREDBRIDGE_SANDBOX_CGROUP_ROOT");
        }
    }
}
