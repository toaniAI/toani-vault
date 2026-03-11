//! API 模块
//!
//! HTTP API 路由和处理器

pub mod attestation;
pub mod audit;
pub mod audit_models;
pub mod auth;
pub mod context;
pub mod credentials;
pub mod middleware;
pub mod rate_limit;
pub mod routes;
pub mod tenant;
pub mod tenant_middleware;
pub mod token_blacklist;

/// API 版本
pub const API_VERSION: &str = "v1";

/// API 基础路径
pub const API_BASE_PATH: &str = "/api/v1";

// 重新导出主要类型
pub use attestation::{
    attestation_routes, init_attestation_api, AttestationApiConfig, AttestationInitError,
    AttestationState,
};
pub use audit::{audit_routes, AuditApiState, AuditStorage, MemoryAuditStorageAdapter};
pub use audit_models::*;
pub use context::{
    CrossTenantErrorResponse, RequestContext, TenantId, TenantIsolationError, TenantQueryBuilder,
    RlsContext, verify_tenant_access, verify_tenant_access_from_param,
};
pub use credentials::{AppState, AuditLogger, DefaultAuditLogger};
pub use middleware::{TokenScope, ValidatedToken};
pub use tenant::{
    tenant_routes, TenantApiState,
};
pub use tenant_middleware::{
    TenantIsolationConfig, TenantIsolationState, TenantMiddlewareBuilder,
    tenant_isolation_middleware, cross_tenant_check_middleware,
    validate_path_tenant_id, validate_query_tenant_id, RequestContextExt,
};
pub use auth::{auth_routes, AuthApiState};
