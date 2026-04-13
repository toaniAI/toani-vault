//! 沙箱错误类型定义

use std::fmt;
use thiserror::Error;
use uuid::Uuid;

/// 沙箱通用错误
#[derive(Error, Debug)]
pub enum SandboxError {
    /// 配置错误
    #[error("配置错误: {0}")]
    Config(String),

    /// 进程错误
    #[error("进程错误: {0}")]
    Process(String),

    /// 沙箱未运行
    #[error("沙箱未运行: {sandbox_id}")]
    NotRunning { sandbox_id: Uuid },

    /// 沙箱已经在运行
    #[error("沙箱已在运行: {sandbox_id}")]
    AlreadyRunning { sandbox_id: Uuid },

    /// 会话错误
    #[error("会话错误: {0}")]
    Session(#[from] SessionError),

    /// 安全错误
    #[error("安全错误: {0}")]
    Security(#[from] SecurityError),

    /// IO 错误
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// 超时错误
    #[error("操作超时: {operation}")]
    Timeout { operation: String },

    /// 资源不足
    #[error("资源不足: {resource}")]
    ResourceExhausted { resource: String },

    /// 池错误
    #[error("沙箱池错误: {0}")]
    Pool(String),

    /// 序列化错误
    #[error("序列化错误: {0}")]
    Serialization(String),

    /// 其他错误
    #[error("沙箱错误: {0}")]
    Other(String),
}

/// 会话错误
#[derive(Error, Debug)]
pub enum SessionError {
    /// 会话未找到
    #[error("会话未找到: {session_id}")]
    NotFound { session_id: Uuid },

    /// 会话已过期
    #[error("会话已过期: {session_id}")]
    Expired { session_id: Uuid },

    /// 会话状态错误
    #[error("会话状态错误: {session_id}, 当前状态: {current}, 期望状态: {expected}")]
    InvalidState {
        session_id: Uuid,
        current: String,
        expected: String,
    },

    /// 凭证访问被拒绝
    #[error("凭证访问被拒绝: 会话 {session_id} 无权访问凭证 {credential_id}")]
    CredentialAccessDenied {
        session_id: Uuid,
        credential_id: Uuid,
    },

    /// 凭证不存在
    #[error("凭证不存在: {credential_id}")]
    CredentialNotFound { credential_id: Uuid },

    /// 最大会话数限制
    #[error("达到最大会话数限制: {max}")]
    MaxSessionsReached { max: usize },

    /// 操作被拒绝
    #[error("操作被拒绝: {reason}")]
    OperationRejected { reason: String },

    /// 会话创建失败
    #[error("会话创建失败: {reason}")]
    CreationFailed { reason: String },
}

/// 安全错误
#[derive(Error, Debug)]
pub enum SecurityError {
    /// Namespace 错误
    #[error("Namespace 错误: {0}")]
    Namespace(String),

    /// cgroup 错误
    #[error("cgroup 错误: {0}")]
    Cgroup(String),

    /// seccomp 错误
    #[error("seccomp 错误: {0}")]
    Seccomp(String),

    /// 系统调用被拒绝
    #[error("系统调用被拒绝: {syscall}")]
    SyscallDenied { syscall: String },

    /// 资源限制 exceeded
    #[error("资源限制超出: {resource} = {current}, 限制 = {limit}")]
    ResourceLimitExceeded {
        resource: String,
        current: u64,
        limit: u64,
    },

    /// 凭证隔离错误
    #[error("凭证隔离错误: {0}")]
    CredentialIsolation(String),

    /// 沙箱逃逸检测
    #[error("沙箱逃逸检测: {details}")]
    EscapeDetected { details: String },

    /// 权限错误
    #[error("权限错误: {0}")]
    Permission(String),
}

/// 导出错误
#[derive(Error, Debug)]
pub enum ExportError {
    /// 截图错误
    #[error("截图错误: {0}")]
    Screenshot(String),

    /// 冻结错误
    #[error("页面冻结错误: {0}")]
    Freeze(String),

    /// 审核错误
    #[error("内容审核错误: {0}")]
    ContentReview(String),

    /// 脱敏错误
    #[error("脱敏处理错误: {0}")]
    Redaction(String),

    /// 水印错误
    #[error("水印添加错误: {0}")]
    Watermark(String),

    /// 签名错误
    #[error("签名错误: {0}")]
    Signature(String),

    /// 验证错误
    #[error("验证错误: {0}")]
    Verification(String),

    /// 序列化错误
    #[error("序列化错误: {0}")]
    Serialization(String),

    /// 配置错误
    #[error("配置错误: {0}")]
    ConfigurationError(String),

    /// 浏览器连接错误
    #[error("浏览器连接错误: {0}")]
    BrowserConnectionError(String),

    /// 浏览器错误
    #[error("浏览器错误: {0}")]
    BrowserError(String),
}

/// 审核错误
#[derive(Error, Debug)]
pub enum ReviewError {
    /// LLM 服务错误
    #[error("LLM 服务错误: {0}")]
    LlmService(String),

    /// 审核超时
    #[error("审核超时")]
    Timeout,

    /// 提示词注入检测
    #[error("检测到提示词注入")]
    PromptInjection,

    /// 审核规则错误
    #[error("审核规则错误: {0}")]
    RuleEngine(String),

    /// 响应解析错误
    #[error("响应解析错误: {0}")]
    ParseResponse(String),
}

impl SandboxError {
    /// 创建配置错误
    pub fn config(msg: impl Into<String>) -> Self {
        SandboxError::Config(msg.into())
    }

    /// 创建进程错误
    pub fn process(msg: impl Into<String>) -> Self {
        SandboxError::Process(msg.into())
    }

    /// 检查是否为超时错误
    pub fn is_timeout(&self) -> bool {
        matches!(self, SandboxError::Timeout { .. })
    }

    /// 检查是否为资源不足错误
    pub fn is_resource_exhausted(&self) -> bool {
        matches!(self, SandboxError::ResourceExhausted { .. })
    }
}

impl SessionError {
    /// 创建会话未找到错误
    pub fn not_found(session_id: Uuid) -> Self {
        SessionError::NotFound { session_id }
    }

    /// 创建会话过期错误
    pub fn expired(session_id: Uuid) -> Self {
        SessionError::Expired { session_id }
    }

    /// 创建状态错误
    pub fn invalid_state(
        session_id: Uuid,
        current: impl fmt::Display,
        expected: impl fmt::Display,
    ) -> Self {
        SessionError::InvalidState {
            session_id,
            current: current.to_string(),
            expected: expected.to_string(),
        }
    }

    /// 创建凭证不存在错误
    pub fn credential_not_found(credential_id: Uuid) -> Self {
        SessionError::CredentialNotFound { credential_id }
    }
}

impl SecurityError {
    /// 创建系统调用被拒绝错误
    pub fn syscall_denied(syscall: impl Into<String>) -> Self {
        SecurityError::SyscallDenied {
            syscall: syscall.into(),
        }
    }

    /// 创建资源限制超出错误
    pub fn resource_limit_exceeded(resource: impl Into<String>, current: u64, limit: u64) -> Self {
        SecurityError::ResourceLimitExceeded {
            resource: resource.into(),
            current,
            limit,
        }
    }

    /// 创建沙箱逃逸检测错误
    pub fn escape_detected(details: impl Into<String>) -> Self {
        SecurityError::EscapeDetected {
            details: details.into(),
        }
    }
}

impl ReviewError {
    /// 创建 LLM 服务错误
    pub fn llm_service(msg: impl Into<String>) -> Self {
        ReviewError::LlmService(msg.into())
    }
}

/// 转换为 HTTP 状态码的 trait
pub trait HttpStatusCode {
    fn http_status_code(&self) -> u16;
}

impl HttpStatusCode for SandboxError {
    fn http_status_code(&self) -> u16 {
        match self {
            SandboxError::Config(_) => 400,
            SandboxError::NotRunning { .. } => 404,
            SandboxError::AlreadyRunning { .. } => 409,
            SandboxError::Session(e) => e.http_status_code(),
            SandboxError::Timeout { .. } => 504,
            SandboxError::ResourceExhausted { .. } => 503,
            _ => 500,
        }
    }
}

impl HttpStatusCode for SessionError {
    fn http_status_code(&self) -> u16 {
        match self {
            SessionError::NotFound { .. } => 404,
            SessionError::CredentialNotFound { .. } => 404,
            SessionError::Expired { .. } => 401,
            SessionError::InvalidState { .. } => 409,
            SessionError::CredentialAccessDenied { .. } => 403,
            SessionError::MaxSessionsReached { .. } => 503,
            SessionError::OperationRejected { .. } => 403,
            SessionError::CreationFailed { .. } => 500,
        }
    }
}

impl HttpStatusCode for SecurityError {
    fn http_status_code(&self) -> u16 {
        match self {
            SecurityError::SyscallDenied { .. } => 403,
            SecurityError::EscapeDetected { .. } => 403,
            SecurityError::CredentialIsolation(_) => 403,
            SecurityError::Permission(_) => 403,
            _ => 500,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_error_helpers() {
        let err = SandboxError::config("invalid config");
        assert!(matches!(err, SandboxError::Config(_)));
        assert!(!err.is_timeout());

        let timeout_err = SandboxError::Timeout {
            operation: "start".to_string(),
        };
        assert!(timeout_err.is_timeout());
        assert_eq!(timeout_err.http_status_code(), 504);
    }

    #[test]
    fn test_session_error_helpers() {
        let session_id = Uuid::new_v4();
        let err = SessionError::not_found(session_id);
        assert!(matches!(err, SessionError::NotFound { .. }));
        assert_eq!(err.http_status_code(), 404);

        let state_err = SessionError::invalid_state(session_id, "closed", "ready");
        assert_eq!(state_err.http_status_code(), 409);

        let credential_err = SessionError::credential_not_found(Uuid::new_v4());
        assert_eq!(credential_err.http_status_code(), 404);
    }

    #[test]
    fn test_security_error_helpers() {
        let err = SecurityError::syscall_denied("execve");
        assert!(matches!(err, SecurityError::SyscallDenied { .. }));
        assert_eq!(err.http_status_code(), 403);

        let escape_err = SecurityError::escape_detected("ptrace detected");
        assert_eq!(escape_err.http_status_code(), 403);
    }
}
