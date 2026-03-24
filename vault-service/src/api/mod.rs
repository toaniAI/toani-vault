//! API 路由模块
//!
//! 提供健康检查、指标收集和 TEE 远程认证的 HTTP API 端点

pub mod attestation;
pub mod health;
pub mod metrics;

// 重新导出健康检查类型
pub use health::{
    ComponentHealth, DetailedHealthResponse, HealthConfig, HealthResponse, HealthState,
    HealthStatus, SystemInfo, TeeHealthDetails, health_check, health_check_detail,
};

// 重新导出指标类型
pub use metrics::{MetricsAuthConfig, MetricsState, metrics_endpoint, validate_metrics_access};

// 重新导出 attestation 类型
pub use attestation::{
    AttestationApiConfig, AttestationState, AttestationStatus, AttestationStatusResponse,
    QuoteResponse, QuoteStatus, TeeType, attestation_routes, get_quote, get_status,
    init_attestation_api,
};
