//! CredBridge Vault Service - 监控与告警模块
//!
//! 提供服务健康检查、指标收集和告警功能。

pub mod alerting;
pub mod api;
pub mod metrics;

// 重新导出主要类型
pub use alerting::{
    AlertEvent, AlertManager, AlertManagerConfig, AlertRule, AlertSeverity, AlertType,
    WebhookConfig, WebhookNotifier, run_alert_processor,
};
pub use metrics::MetricsCollector;

pub use api::{
    health_check, health_check_detail, DetailedHealthResponse, HealthConfig, HealthResponse,
    HealthState, HealthStatus,
};

/// 模块版本
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
