//! seccomp-bpf 系统调用过滤
//!
//! 使用 BPF 程序限制进程可以执行的系统调用

use std::collections::HashSet;

/// seccomp 过滤器
#[derive(Debug, Clone)]
pub struct SeccompFilter {
    /// 策略类型
    pub policy: SystemCallPolicy,
    /// 允许的系统调用列表（白名单模式）
    pub allowlist: HashSet<String>,
    /// 拒绝的系统调用列表（黑名单模式）
    pub denylist: HashSet<String>,
    /// 自定义 BPF 程序
    pub custom_bpf: Option<Vec<u8>>,
}

/// 系统调用策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemCallPolicy {
    /// 白名单模式（只允许指定的调用）
    Allowlist,
    /// 黑名单模式（只拒绝指定的调用）
    Denylist,
    /// 浏览器沙箱策略
    Browser,
    /// 最小权限策略
    Minimal,
    /// 自定义 BPF 程序
    Custom,
}

impl SeccompFilter {
    /// 创建新的 seccomp 过滤器
    pub fn new(policy: SystemCallPolicy) -> Self {
        let mut filter = Self {
            policy,
            allowlist: HashSet::new(),
            denylist: HashSet::new(),
            custom_bpf: None,
        };

        // 根据策略初始化列表
        match policy {
            SystemCallPolicy::Browser => {
                filter.init_browser_policy();
            }
            SystemCallPolicy::Minimal => {
                filter.init_minimal_policy();
            }
            SystemCallPolicy::Allowlist => {
                filter.init_standard_allowlist();
            }
            _ => {}
        }

        filter
    }

    /// 添加允许的调用
    pub fn allow(mut self, syscall: &str) -> Self {
        self.allowlist.insert(syscall.to_string());
        self
    }

    /// 添加拒绝的调用
    pub fn deny(mut self, syscall: &str) -> Self {
        self.denylist.insert(syscall.to_string());
        self
    }

    /// 检查系统调用是否允许
    pub fn is_allowed(&self, syscall: &str) -> bool {
        match self.policy {
            SystemCallPolicy::Allowlist | SystemCallPolicy::Browser | SystemCallPolicy::Minimal => {
                self.allowlist.contains(syscall)
            }
            SystemCallPolicy::Denylist => !self.denylist.contains(syscall),
            SystemCallPolicy::Custom => {
                // 自定义策略下，先检查黑名单
                if self.denylist.contains(syscall) {
                    false
                } else {
                    // 如果设置了白名单，检查白名单
                    self.allowlist.is_empty() || self.allowlist.contains(syscall)
                }
            }
        }
    }

    /// 初始化浏览器沙箱策略
    fn init_browser_policy(&mut self) {
        // 浏览器沙箱允许的系统调用
        let allowed = vec![
            // 基本文件操作
            "read",
            "write",
            "open",
            "openat",
            "close",
            "lseek",
            "pread64",
            "pwrite64",
            // 内存管理
            "mmap",
            "munmap",
            "mprotect",
            "brk",
            "sbrk",
            // 进程管理
            "exit",
            "exit_group",
            "getpid",
            "getppid",
            "getpgrp",
            "gettid",
            // 时间管理
            "gettimeofday",
            "clock_gettime",
            "nanosleep",
            // 信号处理
            "rt_sigaction",
            "rt_sigprocmask",
            "sigaltstack",
            // 线程
            "clone",
            "clone3",
            "futex",
            "set_robust_list",
            // 文件系统
            "stat",
            "fstat",
            "lstat",
            "statx",
            "access",
            "faccessat",
            "getcwd",
            "chdir",
            // 网络
            "socket",
            "socketpair",
            "connect",
            "bind",
            "listen",
            "accept",
            "accept4",
            "sendto",
            "recvfrom",
            "sendmsg",
            "recvmsg",
            "shutdown",
            "setsockopt",
            "getsockopt",
            "getsockname",
            "getpeername",
            // epoll
            "epoll_create",
            "epoll_create1",
            "epoll_ctl",
            "epoll_wait",
            "epoll_pwait",
            // 管道
            "pipe",
            "pipe2",
            // 事件 fd
            "eventfd",
            "eventfd2",
            // 定时器
            "timerfd_create",
            "timerfd_settime",
            "timerfd_gettime",
            // 其他
            "fcntl",
            "ioctl",
            "fcntl64",
            "ioctl",
            "prctl",
            "arch_prctl",
            "set_tid_address",
            "set_robust_list",
            "getrandom",
            "readlink",
            "readlinkat",
            "getdents",
            "getdents64",
            "uname",
            "sysinfo",
        ];

        for syscall in allowed {
            self.allowlist.insert(syscall.to_string());
        }

        // 拒绝危险的调用
        let denied = vec![
            "execve",
            "execveat",
            "fork",
            "vfork",
            "ptrace",
            "process_vm_writev",
            "kexec_load",
            "kexec_file_load",
            "open_by_handle_at",
            "init_module",
            "finit_module",
            "delete_module",
            "ioperm",
            "iopl",
            "swapon",
            "swapoff",
            "reboot",
            "sethostname",
            "setdomainname",
        ];

        for syscall in denied {
            self.denylist.insert(syscall.to_string());
        }
    }

    /// 初始化最小权限策略
    fn init_minimal_policy(&mut self) {
        // 最小权限策略只允许最基本的调用
        let allowed = vec![
            "read",
            "write",
            "open",
            "openat",
            "close",
            "exit",
            "exit_group",
            "mmap",
            "munmap",
            "brk",
        ];

        for syscall in allowed {
            self.allowlist.insert(syscall.to_string());
        }

        // 拒绝所有其他调用
        self.denylist.insert("*".to_string());
    }

    /// 初始化标准白名单
    fn init_standard_allowlist(&mut self) {
        // 基本系统调用
        let allowed = vec![
            "read",
            "write",
            "open",
            "openat",
            "close",
            "exit",
            "exit_group",
            "mmap",
            "munmap",
            "mprotect",
            "brk",
            "getpid",
            "getppid",
        ];

        for syscall in allowed {
            self.allowlist.insert(syscall.to_string());
        }
    }

    /// 生成 libseccomp 风格的 BPF 规则字符串
    pub fn to_seccomp_string(&self) -> String {
        let mut result = String::new();

        match self.policy {
            SystemCallPolicy::Allowlist | SystemCallPolicy::Browser | SystemCallPolicy::Minimal => {
                result.push_str("# 白名单模式\n");
                result.push_str("mode: whitelist\n\n");

                for syscall in &self.allowlist {
                    result.push_str(&format!("allow {syscall}\n"));
                }

                result.push_str("\n# 拒绝其他所有\n");
                result.push_str("deny *\n");
            }
            SystemCallPolicy::Denylist => {
                result.push_str("# 黑名单模式\n");
                result.push_str("mode: blacklist\n\n");

                for syscall in &self.denylist {
                    result.push_str(&format!("deny {syscall}\n"));
                }

                result.push_str("\n# 允许其他所有\n");
                result.push_str("allow *\n");
            }
            SystemCallPolicy::Custom => {
                result.push_str("# 自定义模式\n");
                result.push_str("mode: custom\n");
            }
        }

        result
    }

    /// 验证系统调用名称
    pub fn validate_syscall_name(name: &str) -> bool {
        if name.is_empty() {
            return false;
        }

        // 系统调用名应该只包含小写字母、数字和下划线
        // 且必须以字母开头，不能以数字或下划线开头或结尾
        let mut chars = name.chars();
        let first = chars.next().unwrap();
        let last = name.chars().last().unwrap();

        // 必须以字母开头
        if !first.is_ascii_lowercase() {
            return false;
        }

        // 不能以数字或下划线结尾
        if last.is_ascii_digit() || last == '_' {
            return false;
        }

        // 中间字符可以是小写字母、数字或下划线
        name.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    }
}

impl Default for SeccompFilter {
    fn default() -> Self {
        Self::new(SystemCallPolicy::Browser)
    }
}

/// 危险的系统调用列表
pub const DANGEROUS_SYSCALLS: &[&str] = &[
    "execve",
    "execveat",
    "fork",
    "vfork",
    "clone",
    "ptrace",
    "process_vm_writev",
    "process_vm_readv",
    "kexec_load",
    "kexec_file_load",
    "init_module",
    "finit_module",
    "delete_module",
    "ioperm",
    "iopl",
    "modify_ldt",
    "lookup_dcookie",
    "perf_event_open",
    "bpf",
];

/// 检查是否为危险系统调用
pub fn is_dangerous_syscall(syscall: &str) -> bool {
    DANGEROUS_SYSCALLS.contains(&syscall)
}

/// 系统调用编号（x86_64 Linux）
pub mod syscalls {
    // 常见的 x86_64 系统调用编号
    pub const SYS_READ: i64 = 0;
    pub const SYS_WRITE: i64 = 1;
    pub const SYS_OPEN: i64 = 2;
    pub const SYS_CLOSE: i64 = 3;
    pub const SYS_STAT: i64 = 4;
    pub const SYS_FSTAT: i64 = 5;
    pub const SYS_LSTAT: i64 = 6;
    pub const SYS_POLL: i64 = 7;
    pub const SYS_LSEEK: i64 = 8;
    pub const SYS_MMAP: i64 = 9;
    pub const SYS_MPROTECT: i64 = 10;
    pub const SYS_MUNMAP: i64 = 11;
    pub const SYS_BRK: i64 = 12;
    pub const SYS_RT_SIGACTION: i64 = 13;
    pub const SYS_RT_SIGPROCMASK: i64 = 14;
    pub const SYS_IOCTL: i64 = 16;
    pub const SYS_PREAD64: i64 = 17;
    pub const SYS_PWRITE64: i64 = 18;
    pub const SYS_READV: i64 = 19;
    pub const SYS_WRITEV: i64 = 20;
    pub const SYS_ACCESS: i64 = 21;
    pub const SYS_PIPE: i64 = 22;
    pub const SYS_SCHED_YIELD: i64 = 24;
    pub const SYS_MREMAP: i64 = 25;
    pub const SYS_DUP: i64 = 32;
    pub const SYS_DUP2: i64 = 33;
    pub const SYS_NANOSLEEP: i64 = 35;
    pub const SYS_GETPID: i64 = 39;
    pub const SYS_SOCKET: i64 = 41;
    pub const SYS_CONNECT: i64 = 42;
    pub const SYS_ACCEPT: i64 = 43;
    pub const SYS_SENDTO: i64 = 44;
    pub const SYS_RECVFROM: i64 = 45;
    pub const SYS_SENDMSG: i64 = 46;
    pub const SYS_RECVMSG: i64 = 47;
    pub const SYS_SHUTDOWN: i64 = 48;
    pub const SYS_BIND: i64 = 49;
    pub const SYS_LISTEN: i64 = 50;
    pub const SYS_GETSOCKNAME: i64 = 51;
    pub const SYS_GETPEERNAME: i64 = 52;
    pub const SYS_SOCKETPAIR: i64 = 53;
    pub const SYS_SETSOCKOPT: i64 = 54;
    pub const SYS_GETSOCKOPT: i64 = 55;
    pub const SYS_CLONE: i64 = 56;
    pub const SYS_FORK: i64 = 57;
    pub const SYS_VFORK: i64 = 58;
    pub const SYS_EXECVE: i64 = 59;
    pub const SYS_EXIT: i64 = 60;
    pub const SYS_WAIT4: i64 = 61;
    pub const SYS_KILL: i64 = 62;
    pub const SYS_UNAME: i64 = 63;
    pub const SYS_FCNTL: i64 = 72;
    pub const SYS_FSYNC: i64 = 74;
    pub const SYS_TRUNCATE: i64 = 76;
    pub const SYS_FTRUNCATE: i64 = 77;
    pub const SYS_GETCWD: i64 = 79;
    pub const SYS_CHDIR: i64 = 80;
    pub const SYS_RENAME: i64 = 82;
    pub const SYS_MKDIR: i64 = 83;
    pub const SYS_RMDIR: i64 = 84;
    pub const SYS_CREAT: i64 = 85;
    pub const SYS_LINK: i64 = 86;
    pub const SYS_UNLINK: i64 = 87;
    pub const SYS_SYMLINK: i64 = 88;
    pub const SYS_READLINK: i64 = 89;
    pub const SYS_CHMOD: i64 = 90;
    pub const SYS_CHOWN: i64 = 92;
    pub const SYS_GETTIMEOFDAY: i64 = 96;
    pub const SYS_GETRLIMIT: i64 = 97;
    pub const SYS_GETRUSAGE: i64 = 98;
    pub const SYS_SYSINFO: i64 = 99;
    pub const SYS_GETUID: i64 = 102;
    pub const SYS_GETGID: i64 = 104;
    pub const SYS_GETEUID: i64 = 107;
    pub const SYS_GETEGID: i64 = 108;
    pub const SYS_SETUID: i64 = 105;
    pub const SYS_SETGID: i64 = 106;
    pub const SYS_GETPPID: i64 = 110;
    pub const SYS_SETSID: i64 = 112;
    pub const SYS_SETREUID: i64 = 113;
    pub const SYS_SETREGID: i64 = 114;
    pub const SYS_GETGROUPS: i64 = 115;
    pub const SYS_SETGROUPS: i64 = 116;
    pub const SYS_SETRESUID: i64 = 117;
    pub const SYS_GETRESUID: i64 = 118;
    pub const SYS_SETRESGID: i64 = 119;
    pub const SYS_GETRESGID: i64 = 120;
    pub const SYS_GETPGID: i64 = 121;
    pub const SYS_SETFSUID: i64 = 122;
    pub const SYS_SETFSGID: i64 = 123;
    pub const SYS_GETSID: i64 = 124;
    pub const SYS_CAPGET: i64 = 125;
    pub const SYS_PTRACE: i64 = 101;
    pub const SYS_PRCTL: i64 = 157;
    pub const SYS_ARCH_PRCTL: i64 = 158;
    pub const SYS_OPENAT: i64 = 257;
    pub const SYS_MKDIRAT: i64 = 258;
    pub const SYS_MKNODAT: i64 = 259;
    pub const SYS_FCHOWNAT: i64 = 260;
    pub const SYS_FUTIMESAT: i64 = 261;
    pub const SYS_NEWFSTATAT: i64 = 262;
    pub const SYS_UNLINKAT: i64 = 263;
    pub const SYS_RENAMEAT: i64 = 264;
    pub const SYS_LINKAT: i64 = 265;
    pub const SYS_SYMLINKAT: i64 = 266;
    pub const SYS_READLINKAT: i64 = 267;
    pub const SYS_FCHMODAT: i64 = 268;
    pub const SYS_FACCESSAT: i64 = 269;
    pub const SYS_PSELECT6: i64 = 270;
    pub const SYS_PPOLL: i64 = 271;
    pub const SYS_UNSHARE: i64 = 272;
    pub const SYS_SET_ROBUST_LIST: i64 = 273;
    pub const SYS_GET_ROBUST_LIST: i64 = 274;
    pub const SYS_SPLICE: i64 = 275;
    pub const SYS_TEE: i64 = 276;
    pub const SYS_SYNC_FILE_RANGE: i64 = 277;
    pub const SYS_VMSPLICE: i64 = 278;
    pub const SYS_MOVE_PAGES: i64 = 279;
    pub const SYS_UTIMENSAT: i64 = 280;
    pub const SYS_EPOLL_PWAIT: i64 = 281;
    pub const SYS_SIGNALFD: i64 = 282;
    pub const SYS_TIMERFD_CREATE: i64 = 283;
    pub const SYS_EVENTFD: i64 = 284;
    pub const SYS_FALLOCATE: i64 = 285;
    pub const SYS_TIMERFD_SETTIME: i64 = 286;
    pub const SYS_TIMERFD_GETTIME: i64 = 287;
    pub const SYS_ACCEPT4: i64 = 288;
    pub const SYS_SIGNALFD4: i64 = 289;
    pub const SYS_EVENTFD2: i64 = 290;
    pub const SYS_EPOLL_CREATE1: i64 = 291;
    pub const SYS_DUP3: i64 = 292;
    pub const SYS_PIPE2: i64 = 293;
    pub const SYS_INOTIFY_INIT1: i64 = 294;
    pub const SYS_PREADV: i64 = 295;
    pub const SYS_PWRITEV: i64 = 296;
    pub const SYS_RT_TGSIGQUEUEINFO: i64 = 297;
    pub const SYS_PERF_EVENT_OPEN: i64 = 298;
    pub const SYS_RECVMMSG: i64 = 299;
    pub const SYS_FANOTIFY_INIT: i64 = 300;
    pub const SYS_FANOTIFY_MARK: i64 = 301;
    pub const SYS_PRLIMIT64: i64 = 302;
    pub const SYS_NAME_TO_HANDLE_AT: i64 = 303;
    pub const SYS_OPEN_BY_HANDLE_AT: i64 = 304;
    pub const SYS_CLOCK_ADJTIME: i64 = 305;
    pub const SYS_SYNCFS: i64 = 306;
    pub const SYS_SENDMMSG: i64 = 307;
    pub const SYS_SETNS: i64 = 308;
    pub const SYS_GETCPU: i64 = 309;
    pub const SYS_PROCESS_VM_READV: i64 = 310;
    pub const SYS_PROCESS_VM_WRITEV: i64 = 311;
    pub const SYS_KCMP: i64 = 312;
    pub const SYS_FINIT_MODULE: i64 = 313;
    pub const SYS_SCHED_SETATTR: i64 = 314;
    pub const SYS_SCHED_GETATTR: i64 = 315;
    pub const SYS_RENAMEAT2: i64 = 316;
    pub const SYS_SECCOMP: i64 = 317;
    pub const SYS_GETRANDOM: i64 = 318;
    pub const SYS_MEMFD_CREATE: i64 = 319;
    pub const SYS_KEXEC_FILE_LOAD: i64 = 320;
    pub const SYS_BPF: i64 = 321;
    pub const SYS_EXECVEAT: i64 = 322;
    pub const SYS_USERFAULTFD: i64 = 323;
    pub const SYS_MEMBARRIER: i64 = 324;
    pub const SYS_MLOCK2: i64 = 325;
    pub const SYS_COPY_FILE_RANGE: i64 = 326;
    pub const SYS_PREADV2: i64 = 327;
    pub const SYS_PWRITEV2: i64 = 328;
    pub const SYS_PKEY_MPROTECT: i64 = 329;
    pub const SYS_PKEY_ALLOC: i64 = 330;
    pub const SYS_PKEY_FREE: i64 = 331;
    pub const SYS_STATX: i64 = 332;
    pub const SYS_IO_PGETEVENTS: i64 = 333;
    pub const SYS_RSEQ: i64 = 334;
    pub const SYS_PIDFD_SEND_SIGNAL: i64 = 424;
    pub const SYS_IO_URING_SETUP: i64 = 425;
    pub const SYS_IO_URING_ENTER: i64 = 426;
    pub const SYS_IO_URING_REGISTER: i64 = 427;
    pub const SYS_OPEN_TREE: i64 = 428;
    pub const SYS_MOVE_MOUNT: i64 = 429;
    pub const SYS_FSOPEN: i64 = 430;
    pub const SYS_FSCONFIG: i64 = 431;
    pub const SYS_FSMOUNT: i64 = 432;
    pub const SYS_FSPICK: i64 = 433;
    pub const SYS_PIDFD_OPEN: i64 = 434;
    pub const SYS_CLONE3: i64 = 435;
    pub const SYS_OPENAT2: i64 = 437;
    pub const SYS_PIDFD_GETFD: i64 = 438;
    pub const SYS_FACCESSAT2: i64 = 439;
    pub const SYS_PROCESS_MADVISE: i64 = 440;
    pub const SYS_EPOLL_PWAIT2: i64 = 441;
    pub const SYS_MOUNT_SETATTR: i64 = 442;
    pub const SYS_QUOTACTL_FD: i64 = 443;
    pub const SYS_LANDLOCK_CREATE_RULESET: i64 = 444;
    pub const SYS_LANDLOCK_ADD_RULE: i64 = 445;
    pub const SYS_LANDLOCK_RESTRICT_SELF: i64 = 446;
    pub const SYS_MEMFD_SECRET: i64 = 447;
    pub const SYS_PROCESS_MRELEASE: i64 = 448;
    pub const SYS_FUTEX_WAITV: i64 = 449;
    pub const SYS_SET_MPOLICY_HOME_NODE: i64 = 450;
    pub const SYS_CACHESTAT: i64 = 451;
    pub const SYS_FCHMODAT2: i64 = 452;
    pub const SYS_MAP_SHADOW_STACK: i64 = 453;
    pub const SYS_FUTEX_WAKE: i64 = 454;
    pub const SYS_FUTEX_WAIT: i64 = 455;
    pub const SYS_FUTEX_REQUEUE: i64 = 456;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seccomp_filter_default() {
        let filter = SeccompFilter::default();
        assert_eq!(filter.policy, SystemCallPolicy::Browser);
        assert!(!filter.allowlist.is_empty());
    }

    #[test]
    fn test_seccomp_filter_allowlist() {
        let filter = SeccompFilter::new(SystemCallPolicy::Allowlist);
        assert!(filter.is_allowed("read"));
        assert!(!filter.is_allowed("execve"));
    }

    #[test]
    fn test_seccomp_filter_browser() {
        let filter = SeccompFilter::new(SystemCallPolicy::Browser);
        assert!(filter.is_allowed("read"));
        assert!(filter.is_allowed("write"));
        assert!(filter.is_allowed("socket"));
        assert!(!filter.is_allowed("execve"));
        assert!(!filter.is_allowed("fork"));
    }

    #[test]
    fn test_seccomp_filter_denylist() {
        let filter = SeccompFilter::new(SystemCallPolicy::Denylist)
            .deny("execve")
            .deny("fork");

        assert!(filter.is_allowed("read"));
        assert!(!filter.is_allowed("execve"));
    }

    #[test]
    fn test_seccomp_filter_builder() {
        let filter = SeccompFilter::new(SystemCallPolicy::Allowlist).allow("custom_syscall");

        assert!(filter.is_allowed("custom_syscall"));
    }

    #[test]
    fn test_is_dangerous_syscall() {
        assert!(is_dangerous_syscall("execve"));
        assert!(is_dangerous_syscall("fork"));
        assert!(is_dangerous_syscall("ptrace"));
        assert!(!is_dangerous_syscall("read"));
        assert!(!is_dangerous_syscall("write"));
    }

    #[test]
    fn test_validate_syscall_name() {
        assert!(SeccompFilter::validate_syscall_name("read"));
        assert!(SeccompFilter::validate_syscall_name("openat"));
        assert!(SeccompFilter::validate_syscall_name("io_uring_enter"));
        assert!(!SeccompFilter::validate_syscall_name("Read")); // 大写开头
        assert!(!SeccompFilter::validate_syscall_name("2read")); // 数字开头
        assert!(!SeccompFilter::validate_syscall_name("read_")); // 下划线结尾
        assert!(!SeccompFilter::validate_syscall_name("read2")); // 数字结尾
    }

    #[test]
    fn test_to_seccomp_string() {
        let filter = SeccompFilter::new(SystemCallPolicy::Minimal);
        let config = filter.to_seccomp_string();
        assert!(config.contains("whitelist"));
        assert!(config.contains("allow read"));
        assert!(config.contains("deny *"));
    }
}
