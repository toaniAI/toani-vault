//! API 模块
//!
//! HTTP API 路由和处理器

pub mod approvals;
pub mod attestation;
pub mod audit;
pub mod audit_models;
pub mod auth;
pub mod context;
pub mod credentials;
pub mod i18n;
pub mod logging_middleware;
pub mod middleware;
pub mod notifications;
pub mod oauth_broker;
pub mod rate_limit;
pub mod response;
pub mod routes;
pub mod sandbox;
pub mod sandbox_owner;
pub mod service_accounts;
pub mod tenant;
pub mod tenant_middleware;
pub mod token_blacklist;
pub mod tokens;
pub mod versions;
pub mod websocket;

/// API 版本
pub const API_VERSION: &str = "v1";

/// API 基础路径
pub const API_BASE_PATH: &str = "/api/v1";

// 重新导出主要类型
pub use approvals::{ApprovalApiState, approval_routes};
pub use attestation::{
    AttestationApiConfig, AttestationInitError, AttestationState, attestation_routes,
    init_attestation_api,
};
pub use audit::{
    AuditApiState, AuditStorage, ImmuDbAuditStorageAdapter, MemoryAuditStorageAdapter,
    PostgresAuditStorageAdapter, audit_routes,
};
pub use audit_models::*;
pub use auth::{AuthApiState, auth_routes};
pub use context::{
    ApiContext, CrossTenantErrorResponse, RequestContext, RlsContext, TenantId,
    TenantIsolationError, TenantQueryBuilder, verify_tenant_access,
    verify_tenant_access_from_param,
};
pub use credentials::{AppState, AuditLogger, DefaultAuditLogger};
pub use i18n::{LocaleResolverState, ResolvedLocale, locale_middleware};
pub use middleware::{TokenScope, ValidatedToken};
pub use notifications::notifications_routes;
pub use oauth_broker::{OAuthBrokerApiState, oauth_broker_routes};
pub use sandbox::{SandboxState, sandbox_routes};
pub use service_accounts::service_account_routes;
pub use tenant::{TenantApiState, tenant_routes};
pub use tenant_middleware::{
    RequestContextExt, TenantIsolationConfig, TenantIsolationState, TenantMiddlewareBuilder,
    cross_tenant_check_middleware, tenant_isolation_middleware, validate_path_tenant_id,
    validate_query_tenant_id,
};
pub use tokens::token_routes;
