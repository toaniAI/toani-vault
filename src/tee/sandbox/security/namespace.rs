//! Linux Namespace 配置
//!
//! 提供进程隔离：PID, Network, Mount, IPC, UTS, User, Cgroup

use crate::tee::sandbox::error::SecurityError;
use std::path::PathBuf;

/// Namespace 配置
#[derive(Debug, Clone)]
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
        }
    }
}

impl NamespaceConfig {
    /// 创建完全隔离配置
    pub fn full_isolation() -> Self {
        Self {
            pid: true,
            network: true,
            mount: true,
            ipc: true,
            uts: true,
            user: true,
            cgroup: true,
        }
    }

    /// 创建最小隔离配置（仅用户命名空间）
    pub fn minimal_isolation() -> Self {
        Self {
            pid: false,
            network: false,
            mount: true,
            ipc: false,
            uts: false,
            user: true,
            cgroup: false,
        }
    }

    /// 创建网络沙箱配置
    pub fn network_sandbox() -> Self {
        Self {
            pid: true,
            network: true,
            mount: true,
            ipc: false,
            uts: true,
            user: true,
            cgroup: false,
        }
    }

    /// 检查是否启用了所有关键命名空间
    pub fn is_fully_isolated(&self) -> bool {
        self.pid && self.network && self.mount && self.user
    }

    /// 获取启用的命名空间列表
    pub fn enabled_namespaces(&self) -> Vec<&'static str> {
        let mut namespaces = Vec::new();
        if self.pid {
            namespaces.push("pid");
        }
        if self.network {
            namespaces.push("net");
        }
        if self.mount {
            namespaces.push("mnt");
        }
        if self.ipc {
            namespaces.push("ipc");
        }
        if self.uts {
            namespaces.push("uts");
        }
        if self.user {
            namespaces.push("user");
        }
        if self.cgroup {
            namespaces.push("cgroup");
        }
        namespaces
    }
}

/// Namespace 工具
pub struct NamespaceUtil;

impl NamespaceUtil {
    /// 检查进程是否在指定的命名空间中
    pub fn check_namespace(pid: u32, namespace: &str) -> Result<bool, SecurityError> {
        let ns_path = format!("/proc/{}/ns/{}", pid, namespace);
        match std::fs::read_link(&ns_path) {
            Ok(link) => {
                // 如果链接目标是 "net:[4026531992]" 这样的格式，说明在命名空间中
                let link_str = link.to_string_lossy();
                Ok(link_str.starts_with(namespace))
            }
            Err(e) => Err(SecurityError::Namespace(format!(
                "Failed to read namespace link: {}",
                e
            ))),
        }
    }

    /// 获取进程的命名空间信息
    pub fn get_namespaces(pid: u32) -> Result<NamespaceInfo, SecurityError> {
        let ns_dir = format!("/proc/{}/ns", pid);
        let mut info = NamespaceInfo::default();

        for entry in std::fs::read_dir(&ns_dir)
            .map_err(|e| SecurityError::Namespace(format!("Failed to read ns dir: {}", e)))?
        {
            let entry = entry
                .map_err(|e| SecurityError::Namespace(format!("Failed to read entry: {}", e)))?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if let Ok(target) = std::fs::read_link(&path) {
                let target_str = target.to_string_lossy().to_string();

                match name.as_str() {
                    "pid" => info.pid_ns = Some(target_str),
                    "net" => info.net_ns = Some(target_str),
                    "mnt" => info.mnt_ns = Some(target_str),
                    "ipc" => info.ipc_ns = Some(target_str),
                    "uts" => info.uts_ns = Some(target_str),
                    "user" => info.user_ns = Some(target_str),
                    "cgroup" => info.cgroup_ns = Some(target_str),
                    _ => {}
                }
            }
        }

        Ok(info)
    }

    /// 检查两个进程是否在同一命名空间中
    pub fn in_same_namespace(pid1: u32, pid2: u32, namespace: &str) -> Result<bool, SecurityError> {
        let ns1 = format!("/proc/{}/ns/{}", pid1, namespace);
        let ns2 = format!("/proc/{}/ns/{}", pid2, namespace);

        let target1 = std::fs::read_link(&ns1)
            .map_err(|e| SecurityError::Namespace(format!("Failed to read ns: {}", e)))?;
        let target2 = std::fs::read_link(&ns2)
            .map_err(|e| SecurityError::Namespace(format!("Failed to read ns: {}", e)))?;

        Ok(target1 == target2)
    }
}

/// 命名空间信息
#[derive(Debug, Clone, Default)]
pub struct NamespaceInfo {
    /// PID namespace
    pub pid_ns: Option<String>,
    /// Network namespace
    pub net_ns: Option<String>,
    /// Mount namespace
    pub mnt_ns: Option<String>,
    /// IPC namespace
    pub ipc_ns: Option<String>,
    /// UTS namespace
    pub uts_ns: Option<String>,
    /// User namespace
    pub user_ns: Option<String>,
    /// Cgroup namespace
    pub cgroup_ns: Option<String>,
}

/// 挂载配置
#[derive(Debug, Clone)]
pub struct MountConfig {
    /// 源路径
    pub src: PathBuf,
    /// 目标路径
    pub dst: PathBuf,
    /// 挂载类型
    pub mount_type: MountType,
    /// 是否只读
    pub read_only: bool,
    /// 挂载选项
    pub options: Vec<String>,
}

impl MountConfig {
    /// 创建绑定挂载配置
    pub fn bind_mount(src: impl Into<PathBuf>, dst: impl Into<PathBuf>, read_only: bool) -> Self {
        Self {
            src: src.into(),
            dst: dst.into(),
            mount_type: MountType::Bind,
            read_only,
            options: Vec::new(),
        }
    }

    /// 创建 tmpfs 挂载配置
    pub fn tmpfs(dst: impl Into<PathBuf>, size: Option<&str>) -> Self {
        let mut options = Vec::new();
        if let Some(s) = size {
            options.push(format!("size={}", s));
        }

        Self {
            src: PathBuf::new(),
            dst: dst.into(),
            mount_type: MountType::Tmpfs,
            read_only: false,
            options,
        }
    }

    /// 转换为 mount 命令参数
    pub fn to_mount_args(&self) -> Vec<String> {
        let mut args = vec!["-t".to_string(), self.mount_type.to_string()];

        let mut options = self.options.clone();
        if self.read_only {
            options.push("ro".to_string());
        }

        if !options.is_empty() {
            args.push("-o".to_string());
            args.push(options.join(","));
        }

        args.push(self.src.to_string_lossy().to_string());
        args.push(self.dst.to_string_lossy().to_string());

        args
    }
}

/// 挂载类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountType {
    /// 绑定挂载
    Bind,
    /// tmpfs
    Tmpfs,
    /// proc
    Proc,
    /// sysfs
    Sysfs,
    /// devtmpfs
    Devtmpfs,
}

impl std::fmt::Display for MountType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MountType::Bind => write!(f, "bind"),
            MountType::Tmpfs => write!(f, "tmpfs"),
            MountType::Proc => write!(f, "proc"),
            MountType::Sysfs => write!(f, "sysfs"),
            MountType::Devtmpfs => write!(f, "devtmpfs"),
        }
    }
}

/// 创建标准沙箱挂载点
pub fn create_standard_mounts() -> Vec<MountConfig> {
    vec![
        MountConfig::bind_mount("/bin", "/bin", true),
        MountConfig::bind_mount("/lib", "/lib", true),
        MountConfig::bind_mount("/lib64", "/lib64", true),
        MountConfig::bind_mount("/usr", "/usr", true),
        MountConfig::bind_mount("/sbin", "/sbin", true),
        MountConfig::tmpfs("/tmp", Some("100m")),
        MountConfig::tmpfs("/var/tmp", Some("100m")),
        MountConfig::proc(PathBuf::from("/proc")),
        MountConfig::sysfs(PathBuf::from("/sys")),
        MountConfig::devtmpfs(PathBuf::from("/dev")),
    ]
}

impl MountConfig {
    fn proc(dst: PathBuf) -> Self {
        Self {
            src: PathBuf::new(),
            dst,
            mount_type: MountType::Proc,
            read_only: false,
            options: Vec::new(),
        }
    }

    fn sysfs(dst: PathBuf) -> Self {
        Self {
            src: PathBuf::new(),
            dst,
            mount_type: MountType::Sysfs,
            read_only: true,
            options: Vec::new(),
        }
    }

    fn devtmpfs(dst: PathBuf) -> Self {
        Self {
            src: PathBuf::new(),
            dst,
            mount_type: MountType::Devtmpfs,
            read_only: false,
            options: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_config_default() {
        let config = NamespaceConfig::default();
        assert!(config.pid);
        assert!(config.network);
        assert!(config.mount);
        assert!(config.user);
    }

    #[test]
    fn test_namespace_config_full_isolation() {
        let config = NamespaceConfig::full_isolation();
        assert!(config.is_fully_isolated());
        assert_eq!(config.enabled_namespaces().len(), 7);
    }

    #[test]
    fn test_namespace_config_minimal() {
        let config = NamespaceConfig::minimal_isolation();
        assert!(!config.is_fully_isolated());
        assert!(!config.pid);
        assert!(!config.network);
        assert!(config.user);
    }

    #[test]
    fn test_mount_config() {
        let mount = MountConfig::bind_mount("/host/path", "/container/path", true);
        assert_eq!(mount.src, PathBuf::from("/host/path"));
        assert_eq!(mount.dst, PathBuf::from("/container/path"));
        assert!(mount.read_only);
        assert_eq!(mount.mount_type, MountType::Bind);
    }

    #[test]
    fn test_mount_config_to_args() {
        let mount = MountConfig::bind_mount("/host", "/container", true);
        let args = mount.to_mount_args();
        assert!(args.contains(&"bind".to_string()));
        assert!(args.contains(&"/host".to_string()));
        assert!(args.contains(&"/container".to_string()));
    }

    #[test]
    fn test_mount_type_display() {
        assert_eq!(MountType::Bind.to_string(), "bind");
        assert_eq!(MountType::Tmpfs.to_string(), "tmpfs");
        assert_eq!(MountType::Proc.to_string(), "proc");
    }

    #[test]
    fn test_namespace_info_default() {
        let info = NamespaceInfo::default();
        assert!(info.pid_ns.is_none());
        assert!(info.net_ns.is_none());
    }
}
