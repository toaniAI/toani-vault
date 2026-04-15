//! TEE 安全执行沙箱模块
//!
//! 实现基于 nsjail 的多层隔离沙箱架构
//! 支持 AI Agent 在可信执行环境内完成敏感操作
//!
//! # 模块结构
//!
//! ```text
//! sandbox/
//! ├── mod.rs           - 模块导出
//! ├── types.rs         - 核心类型定义
//! ├── config.rs        - 沙箱配置
//! ├── error.rs         - 错误类型
//! ├── pool.rs          - 沙箱池管理
//! ├── nsjail.rs        - nsjail 沙箱实现
//! ├── session.rs       - 会话管理
//! ├── credential_ns.rs - 凭证命名空间
//! ├── review/          - AI 审核引擎
//! └── security/        - 安全机制
//! ```
//!
//! # 核心功能
//!
//! - **NsjailSandbox**: 基于 nsjail 的进程级隔离
//! - **SandboxPool**: 热实例池，快速启动优化
//! - **SandboxSession**: 会话生命周期管理
//! - **CredentialNamespace**: 凭证命名空间隔离
//! - **Security**: Namespaces + cgroups + seccomp 三层隔离
//!
//! # 使用示例
//!
//! ```rust,no_run
//! use vault_service::tee::sandbox::{SandboxPool, SandboxConfig, SessionRequest};
//!
//! async fn example() -> Result<(), Box<dyn std::error::Error>> {
//!     // 创建沙箱池
//!     let config = SandboxConfig::default();
//!     let pool = SandboxPool::new(config).await?;
//!
//!     // 获取会话
//!     let request = SessionRequest {
//!         tenant_id: "tenant_123".to_string(),
//!         user_id: "user_456".to_string(),
//!         credential_id: "cred_789".to_string(),
//!         original_intent: "查询投资组合".to_string(),
//!         metadata: Default::default(),
//!     };
//!     let session = pool.acquire_session(request).await?;
//!
//!     // 执行操作
//!     let operation = vault_service::tee::sandbox::OperationRequest {
//!         operation_type: vault_service::tee::sandbox::OperationType::Query,
//!         ..Default::default()
//!     };
//!     let result = session.execute_operation(operation).await?;
//!     Ok(())
//! }
//! ```

pub mod browser_runtime;
pub mod config;
pub mod credential_ns;
pub mod error;
pub mod export;
pub mod nsjail;
pub mod pool;
pub mod repository;
pub mod review;
pub mod security;
pub mod session;
pub mod types;

// 公开导出 - 类型
pub use types::{
    ExecutionResult, OperationRequest, OperationStatus, OperationType, SandboxId, SandboxStatus,
    SessionContext, SessionId, SessionRequest, SessionStatus, WarmInstanceInfo,
};

// 公开导出 - 配置
pub use config::{
    CgroupConfig, NsjailConfig, SandboxConfig, SandboxPoolConfig, SeccompConfig, SecurityConfig,
};

// 公开导出 - 错误
pub use error::{SandboxError, SecurityError, SessionError};

// 公开导出 - 核心组件
pub use nsjail::SandboxProcessHealth;
pub use nsjail::{NsjailSandbox, WarmNsjailInstance};
pub use pool::{NsjailSandboxPool, SandboxPool};
pub use repository::{
    CompleteSandboxOperationRecord, NewSandboxOperationRecord, NewSandboxSessionRecord,
    PostgresSandboxRepository, SandboxRepository, metadata_to_json, to_chrono_utc,
};
pub use session::{ActiveNsjailSession, SandboxSession};

// 公开导出 - 安全
pub use security::{
    CgroupManager, NamespaceConfig, ResourceLimits, SeccompFilter, SystemCallPolicy,
};

// 公开导出 - 凭证命名空间
pub use credential_ns::{CredentialHandle, CredentialNamespaceManager};

// 公开导出 - AI 审核
pub use review::{
    OperationReviewer, OperationReviewerConfig, PromptInjectionDetector, ReviewConfig,
    ReviewContext, ReviewResult, RiskLevel, SuggestedAction,
};

/// 沙箱模块版本
pub const SANDBOX_VERSION: &str = "0.1.0";

/// 默认沙箱池大小
pub const DEFAULT_POOL_SIZE: usize = 10;

/// 默认会话超时（分钟）
pub const DEFAULT_SESSION_TIMEOUT_MINUTES: u64 = 30;

/// 默认热实例 TTL（秒）
pub const DEFAULT_WARM_INSTANCE_TTL_SECS: u64 = 300;

/// 最大并发会话数
pub const MAX_CONCURRENT_SESSIONS: usize = 100;

/// 沙箱健康状态
#[derive(Debug, Clone)]
pub struct SandboxHealth {
    /// 池状态
    pub pool_status: PoolStatus,
    /// 活跃会话数
    pub active_sessions: usize,
    /// 热实例数
    pub warm_instances: usize,
    /// 是否健康
    pub healthy: bool,
    /// 错误信息
    pub error: Option<String>,
    /// 进程树自检异常数量
    pub process_health_issues: usize,
    /// 进程树自检摘要
    pub process_health_summaries: Vec<String>,
}

/// 池状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PoolStatus {
    /// 正在初始化
    Initializing,
    /// 正常运行
    Running,
    /// 资源不足
    ResourceConstrained,
    /// 已关闭
    Shutdown,
}

impl std::fmt::Display for PoolStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PoolStatus::Initializing => write!(f, "initializing"),
            PoolStatus::Running => write!(f, "running"),
            PoolStatus::ResourceConstrained => write!(f, "resource_constrained"),
            PoolStatus::Shutdown => write!(f, "shutdown"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_version() {
        assert_eq!(SANDBOX_VERSION, "0.1.0");
    }

    #[test]
    fn test_default_constants() {
        assert_eq!(DEFAULT_POOL_SIZE, 10);
        assert_eq!(DEFAULT_SESSION_TIMEOUT_MINUTES, 30);
        assert_eq!(DEFAULT_WARM_INSTANCE_TTL_SECS, 300);
        assert_eq!(MAX_CONCURRENT_SESSIONS, 100);
    }

    #[test]
    fn test_pool_status_display() {
        assert_eq!(PoolStatus::Running.to_string(), "running");
        assert_eq!(PoolStatus::Shutdown.to_string(), "shutdown");
        assert_eq!(PoolStatus::Initializing.to_string(), "initializing");
        assert_eq!(
            PoolStatus::ResourceConstrained.to_string(),
            "resource_constrained"
        );
    }
}
