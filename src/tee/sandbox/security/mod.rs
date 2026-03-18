//! 沙箱安全机制
//!
//! 实现多层隔离机制：
//! - Linux Namespaces：进程隔离
//! - cgroups：资源限制
//! - seccomp：系统调用过滤

pub mod cgroup;
pub mod namespace;
pub mod seccomp;

// 公开导出
pub use cgroup::{CgroupManager, ResourceLimits};
pub use namespace::NamespaceConfig;
pub use seccomp::{SeccompFilter, SystemCallPolicy};

use crate::tee::sandbox::error::SecurityError;
use std::path::PathBuf;
use tracing::{debug, error, info, warn};

/// 安全检查器
///
/// 执行多种安全检查，防止沙箱逃逸
pub struct SecurityChecker {
    /// 检查项目
    checks: Vec<Box<dyn SecurityCheck + Send + Sync>>,
}

/// 安全检查 trait
pub trait SecurityCheck {
    /// 执行检查
    fn check(&self) -> Result<(), SecurityError>;
    /// 检查名称
    fn name(&self) -> &'static str;
}

impl SecurityChecker {
    /// 创建新的安全检查器
    pub fn new() -> Self {
        Self { checks: Vec::new() }
    }

    /// 添加检查
    pub fn add_check(&mut self, check: Box<dyn SecurityCheck + Send + Sync>) {
        self.checks.push(check);
    }

    /// 执行所有检查
    pub fn run_all(&self) -> Result<(), Vec<SecurityError>> {
        let mut errors = Vec::new();

        for check in &self.checks {
            debug!("Running security check: {}", check.name());
            if let Err(e) = check.check() {
                warn!("Security check '{}' failed: {}", check.name(), e);
                errors.push(e);
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Default for SecurityChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// 检查沙箱逃逸尝试
pub fn check_escape_attempt(pid: u32) -> Result<(), SecurityError> {
    // 检查 /proc/PID/status 中的 CapEff
    let status_path = PathBuf::from(format!("/proc/{}/status", pid));

    if let Ok(status) = std::fs::read_to_string(&status_path) {
        // 检查是否有不应该有的 capability
        for line in status.lines() {
            if line.starts_with("CapEff:") {
                let caps = line.split(':').nth(1).map(|s| s.trim()).unwrap_or("0");

                // 检查是否有危险的 capabilities
                if caps != "0" && caps != "0000000000000000" {
                    // 需要更详细的检查，这里简化处理
                    warn!("Process {} has capabilities: {}", pid, caps);
                }
            }
        }
    }

    Ok(())
}

/// 安全审计日志
#[derive(Debug)]
pub struct SecurityAuditLog {
    /// 事件类型
    pub event_type: SecurityEventType,
    /// 严重程度
    pub severity: SecuritySeverity,
    /// 详细信息
    pub details: String,
    /// 时间戳
    pub timestamp: time::OffsetDateTime,
}

/// 安全事件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityEventType {
    /// 沙箱创建
    SandboxCreated,
    /// 沙箱销毁
    SandboxDestroyed,
    /// 系统调用拦截
    SyscallIntercepted,
    /// 资源限制触发
    ResourceLimitTriggered,
    /// 逃逸尝试
    EscapeAttempt,
    /// 凭证访问
    CredentialAccess,
    /// 配置变更
    ConfigChanged,
}

/// 安全严重程度
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SecuritySeverity {
    /// 信息
    Info,
    /// 低
    Low,
    /// 中
    Medium,
    /// 高
    High,
    /// 严重
    Critical,
}

/// 记录安全审计日志
pub fn log_security_event(
    event_type: SecurityEventType,
    severity: SecuritySeverity,
    details: impl Into<String>,
) {
    let log = SecurityAuditLog {
        event_type,
        severity,
        details: details.into(),
        timestamp: time::OffsetDateTime::now_utc(),
    };

    match severity {
        SecuritySeverity::Critical | SecuritySeverity::High => {
            error!(
                "Security Event: {:?} - {} - {}",
                log.event_type, log.severity as u8, log.details
            );
        }
        SecuritySeverity::Medium => {
            warn!("Security Event: {:?} - {}", log.event_type, log.details);
        }
        _ => {
            info!("Security Event: {:?} - {}", log.event_type, log.details);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_severity_order() {
        assert!(SecuritySeverity::Info < SecuritySeverity::Low);
        assert!(SecuritySeverity::Low < SecuritySeverity::Medium);
        assert!(SecuritySeverity::Medium < SecuritySeverity::High);
        assert!(SecuritySeverity::High < SecuritySeverity::Critical);
    }

    #[test]
    fn test_security_event_type() {
        assert_eq!(
            format!("{:?}", SecurityEventType::SandboxCreated),
            "SandboxCreated"
        );
        assert_eq!(
            format!("{:?}", SecurityEventType::EscapeAttempt),
            "EscapeAttempt"
        );
    }

    #[test]
    fn test_log_security_event() {
        // 只测试不 panic
        log_security_event(
            SecurityEventType::SandboxCreated,
            SecuritySeverity::Info,
            "Test sandbox created",
        );

        log_security_event(
            SecurityEventType::EscapeAttempt,
            SecuritySeverity::Critical,
            "Escape attempt detected",
        );
    }
}
