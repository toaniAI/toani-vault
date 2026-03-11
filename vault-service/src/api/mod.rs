//! API 路由模块
//!
//! 提供健康检查和指标收集的 HTTP API 端点

pub mod health;
pub mod metrics;

// 重新导出健康检查类型
pub use health::{
    health_check, health_check_detail, ComponentHealth, DetailedHealthResponse, HealthConfig,
    HealthResponse, HealthState, HealthStatus, SystemInfo, TeeHealthDetails,
};

// 重新导出指标类型
pub use metrics::{metrics_endpoint, MetricsAuthConfig, MetricsState, validate_metrics_access};
