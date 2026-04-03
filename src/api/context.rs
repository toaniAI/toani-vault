//! 请求上下文模块
//!
//! 提供请求级别的上下文管理，包含租户ID、用户信息等
//! - 从 Token 中提取 tenant_id
//! - 注入到请求扩展中供后续处理器使用
//! - 支持数据库 RLS（行级安全）上下文设置

use axum::{
    extract::{FromRequestParts, Request},
    http::{StatusCode, request::Parts},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::api::middleware::ValidatedToken;
use crate::utils::sql::escape_sql_string;

/// API 上下文
///
/// 用于在处理器之间共享依赖
#[derive(Clone)]
pub struct ApiContext {
    /// 数据库连接池
    pub db: Option<Arc<sqlx::PgPool>>,
    /// 凭证 Vault
    pub vault: Option<Arc<crate::vault::storage::CredentialVault>>,
    /// 应用配置
    pub config: Option<Arc<std::collections::HashMap<String, String>>>,
}

impl ApiContext {
    /// 创建新的 API 上下文
    pub fn new(
        db: Option<Arc<sqlx::PgPool>>,
        vault: Option<Arc<crate::vault::storage::CredentialVault>>,
        config: Option<Arc<std::collections::HashMap<String, String>>>,
    ) -> Self {
        Self { db, vault, config }
    }

    /// 从请求上下文创建
    pub fn from_request_context(_ctx: &RequestContext) -> Self {
        // 这里简化处理，实际应该从全局状态获取
        Self {
            db: None,
            vault: None,
            config: None,
        }
    }
}

/// 请求上下文
///
/// 包含当前请求的所有上下文信息，从 Token 中提取。
/// 支持 membership-based 的租户隔离和权限控制。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestContext {
    /// 租户ID（从 membership 中获取）
    pub tenant_id: String,
    /// 用户ID
    pub user_id: String,
    /// Token ID (jti)
    pub token_id: String,
    /// 授权 Scope 列表（从 membership 中获取）
    pub scopes: Vec<String>,
    /// 成员资格 ID（可选，用于 membership-based 隔离）
    pub membership_id: Option<String>,
    /// 会话 ID（可选）
    pub session_id: Option<String>,
    /// 请求ID（用于追踪）
    pub request_id: String,
    /// 当前请求解析后的 locale
    pub resolved_locale: String,
}

impl RequestContext {
    /// 创建新的请求上下文
    pub fn new(
        tenant_id: impl Into<String>,
        user_id: impl Into<String>,
        token_id: impl Into<String>,
        scopes: Vec<String>,
    ) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            user_id: user_id.into(),
            token_id: token_id.into(),
            scopes,
            membership_id: None,
            session_id: None,
            request_id: generate_request_id(),
            resolved_locale: crate::api::i18n::DEFAULT_LOCALE.to_string(),
        }
    }

    /// 从 ValidatedToken 创建请求上下文
    ///
    /// 使用 membership 数据填充 tenant_id 和 scopes。
    pub fn from_validated_token(token: &ValidatedToken) -> Self {
        Self {
            tenant_id: token.tenant_id.clone(),
            user_id: token.user_id.clone(),
            token_id: token.token_id.clone(),
            scopes: token
                .scopes
                .iter()
                .map(|s| s.as_str().to_string())
                .collect(),
            membership_id: token.membership_id.clone(),
            session_id: token.session_id().map(|s| s.to_string()),
            request_id: generate_request_id(),
            resolved_locale: crate::api::i18n::DEFAULT_LOCALE.to_string(),
        }
    }

    /// 检查是否拥有指定 scope
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.contains(&scope.to_string()) || self.scopes.contains(&"admin".to_string())
    }

    /// 检查是否拥有任一指定 scope
    pub fn has_any_scope(&self, scopes: &[&str]) -> bool {
        scopes.iter().any(|s| self.has_scope(s))
    }

    /// 获取租户ID
    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }

    /// 获取用户ID
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// 获取请求ID
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    /// 获取成员资格 ID
    pub fn membership_id(&self) -> Option<&str> {
        self.membership_id.as_deref()
    }

    /// 获取会话 ID
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// 设置请求ID
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = request_id.into();
        self
    }

    /// 设置已解析的 locale
    pub fn with_resolved_locale(mut self, locale: impl Into<String>) -> Self {
        self.resolved_locale = locale.into();
        self
    }

    /// 设置成员资格 ID
    pub fn with_membership_id(mut self, membership_id: impl Into<String>) -> Self {
        self.membership_id = Some(membership_id.into());
        self
    }

    /// 设置会话 ID
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }
}

/// 生成请求ID
fn generate_request_id() -> String {
    format!("req_{}", uuid::Uuid::now_v7())
}

/// 从请求中提取上下文
pub fn extract_context(request: &Request) -> Option<&RequestContext> {
    request.extensions().get::<RequestContext>()
}

/// 将上下文注入到请求中
pub fn inject_context(request: &mut Request, context: RequestContext) {
    request.extensions_mut().insert(context);
}

/// 租户ID提取器
///
/// 用于在处理器中直接提取租户ID
#[derive(Debug, Clone)]
pub struct TenantId(pub String);

impl TenantId {
    /// 获取租户ID字符串
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[async_trait::async_trait]
impl<S> FromRequestParts<S> for TenantId
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<RequestContext>()
            .map(|ctx| TenantId(ctx.tenant_id.clone()))
            .ok_or((
                StatusCode::UNAUTHORIZED,
                "Missing tenant context - authentication required",
            ))
    }
}

/// 请求上下文提取器
///
/// 用于在处理器中直接提取完整的请求上下文
#[async_trait::async_trait]
impl<S> FromRequestParts<S> for RequestContext
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<RequestContext>().cloned().ok_or((
            StatusCode::UNAUTHORIZED,
            "Missing request context - authentication required",
        ))
    }
}

/// 数据库 RLS 上下文
///
/// 用于 PostgreSQL RLS（行级安全）的会话变量设置。
/// 支持 membership-based 的租户隔离。
#[derive(Debug, Clone)]
pub struct RlsContext {
    /// 租户ID
    pub tenant_id: String,
    /// 用户ID
    pub user_id: String,
    /// 成员资格 ID（可选）
    pub membership_id: Option<String>,
    /// 允许的 Scope
    pub scopes: Vec<String>,
    /// 是否为管理员（绕过 RLS 检查）
    pub is_admin: bool,
}

impl RlsContext {
    /// 从请求上下文创建 RLS 上下文
    pub fn from_request_context(ctx: &RequestContext) -> Self {
        Self {
            tenant_id: ctx.tenant_id.clone(),
            user_id: ctx.user_id.clone(),
            membership_id: ctx.membership_id.clone(),
            scopes: ctx.scopes.clone(),
            is_admin: ctx.scopes.contains(&"admin".to_string()),
        }
    }

    /// 创建新的 RLS 上下文
    pub fn new(
        tenant_id: impl Into<String>,
        user_id: impl Into<String>,
        scopes: Vec<String>,
    ) -> Self {
        let tenant_id = tenant_id.into();
        let user_id = user_id.into();
        let is_admin = scopes.contains(&"admin".to_string());
        Self {
            tenant_id,
            user_id,
            membership_id: None,
            scopes,
            is_admin,
        }
    }

    /// 设置成员资格 ID
    pub fn with_membership_id(mut self, membership_id: impl Into<String>) -> Self {
        self.membership_id = Some(membership_id.into());
        self
    }

    /// 生成 PostgreSQL RLS 设置 SQL（事务级别）
    ///
    /// 使用 SET LOCAL 确保变量仅在事务级别生效，避免连接池污染
    /// 这些语句必须在事务内执行
    pub fn to_sql_transaction_local(&self) -> String {
        let membership_sql = self
            .membership_id
            .as_ref()
            .map(|id| {
                format!(
                    " SET LOCAL app.current_membership_id = '{}';",
                    escape_sql_string(id)
                )
            })
            .unwrap_or_default();

        format!(
            "SET LOCAL app.current_tenant_id = '{}'; \
             SET LOCAL app.current_user_id = '{}'; \
             SET LOCAL app.current_scopes = '{}'; \
             SET LOCAL app.is_admin = '{}';{}",
            escape_sql_string(&self.tenant_id),
            escape_sql_string(&self.user_id),
            escape_sql_string(&self.scopes.join(",")),
            self.is_admin,
            membership_sql
        )
    }

    /// 生成 PostgreSQL RLS 设置 SQL（会话级别）
    ///
    /// 这些语句应该在连接获取后立即执行
    pub fn to_sql_statements(&self) -> Vec<String> {
        let mut statements = vec![
            format!(
                "SET app.current_tenant_id = '{}'",
                escape_sql_string(&self.tenant_id)
            ),
            format!(
                "SET app.current_user_id = '{}'",
                escape_sql_string(&self.user_id)
            ),
            format!(
                "SET app.current_scopes = '{}'",
                escape_sql_string(&self.scopes.join(","))
            ),
            format!("SET app.is_admin = '{}'", self.is_admin),
        ];

        if let Some(ref membership_id) = self.membership_id {
            statements.push(format!(
                "SET app.current_membership_id = '{}'",
                escape_sql_string(membership_id)
            ));
        }

        statements
    }

    /// 检查是否有管理员权限
    pub fn is_admin(&self) -> bool {
        self.is_admin
    }

    /// 检查是否有指定 scope
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.contains(&scope.to_string()) || self.is_admin
    }
}

/// 带租户隔离的数据库查询构建器
///
/// 自动添加 tenant_id 过滤条件
pub struct TenantQueryBuilder {
    tenant_id: String,
    base_query: String,
}

impl TenantQueryBuilder {
    /// 创建新的查询构建器
    pub fn new(tenant_id: impl Into<String>, base_query: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            base_query: base_query.into(),
        }
    }

    /// 构建带租户过滤的查询
    ///
    /// 自动添加 WHERE tenant_id = ? 条件
    pub fn build(&self) -> (String, Vec<String>) {
        let query = if self.base_query.to_uppercase().contains("WHERE") {
            format!(
                "{} AND tenant_id = '{}'",
                self.base_query,
                escape_sql_string(&self.tenant_id)
            )
        } else {
            format!(
                "{} WHERE tenant_id = '{}'",
                self.base_query,
                escape_sql_string(&self.tenant_id)
            )
        };

        (query, vec![self.tenant_id.clone()])
    }

    /// 获取租户ID
    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }
}

/// 租户隔离错误
#[derive(Debug, Clone, thiserror::Error)]
pub enum TenantIsolationError {
    #[error("跨租户访问被拒绝: 请求租户 {requested} 不匹配资源租户 {actual}")]
    CrossTenantAccessDenied { requested: String, actual: String },

    #[error("租户上下文缺失")]
    MissingTenantContext,

    #[error("无效的租户ID: {0}")]
    InvalidTenantId(String),

    #[error("租户未激活: {0}")]
    TenantInactive(String),
}

/// 验证资源访问权限
///
/// 检查请求上下文是否有权访问指定租户的资源
pub fn verify_tenant_access(
    ctx: &RequestContext,
    resource_tenant_id: &str,
) -> Result<(), TenantIsolationError> {
    if ctx.tenant_id != resource_tenant_id {
        return Err(TenantIsolationError::CrossTenantAccessDenied {
            requested: ctx.tenant_id.clone(),
            actual: resource_tenant_id.to_string(),
        });
    }
    Ok(())
}

/// 验证资源访问权限（从路径参数）
///
/// 用于验证 URL 路径中的租户ID是否与 Token 中的一致
pub fn verify_tenant_access_from_param(
    ctx: &RequestContext,
    param_tenant_id: &str,
) -> Result<(), TenantIsolationError> {
    verify_tenant_access(ctx, param_tenant_id)
}

/// 跨租户访问错误响应
#[derive(Debug, Clone, Serialize)]
pub struct CrossTenantErrorResponse {
    pub success: bool,
    pub error: CrossTenantError,
    pub meta: ErrorMeta,
}

#[derive(Debug, Clone, Serialize)]
pub struct CrossTenantError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorMeta {
    pub request_id: String,
    pub timestamp: String,
}

impl CrossTenantErrorResponse {
    pub fn new(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            success: false,
            error: CrossTenantError {
                code: "CROSS_TENANT_ACCESS_DENIED".to_string(),
                message: message.into(),
            },
            meta: ErrorMeta {
                request_id: request_id.into(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            },
        }
    }
}

/// 租户信息存储（用于管理租户状态）
#[derive(Debug, Clone)]
pub struct TenantInfo {
    pub tenant_id: String,
    pub tenant_name: String,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 租户存储 trait
#[async_trait::async_trait]
pub trait TenantStorage: Send + Sync {
    async fn get_tenant(&self, tenant_id: &str) -> Option<TenantInfo>;
    async fn is_tenant_active(&self, tenant_id: &str) -> bool;
}

/// 内存租户存储（用于测试）
pub struct MemoryTenantStorage {
    tenants: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, TenantInfo>>>,
}

impl Default for MemoryTenantStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryTenantStorage {
    pub fn new() -> Self {
        Self {
            tenants: std::sync::Arc::new(
                tokio::sync::RwLock::new(std::collections::HashMap::new()),
            ),
        }
    }

    pub async fn add_tenant(&self, tenant: TenantInfo) {
        let mut tenants = self.tenants.write().await;
        tenants.insert(tenant.tenant_id.clone(), tenant);
    }
}

#[async_trait::async_trait]
impl TenantStorage for MemoryTenantStorage {
    async fn get_tenant(&self, tenant_id: &str) -> Option<TenantInfo> {
        let tenants = self.tenants.read().await;
        tenants.get(tenant_id).cloned()
    }

    async fn is_tenant_active(&self, tenant_id: &str) -> bool {
        self.get_tenant(tenant_id)
            .await
            .map(|t| t.is_active)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_context_creation() {
        let ctx = RequestContext::new(
            "tenant_123",
            "user_456",
            "token_789",
            vec!["credential:read".to_string()],
        );

        assert_eq!(ctx.tenant_id, "tenant_123");
        assert_eq!(ctx.user_id, "user_456");
        assert_eq!(ctx.token_id, "token_789");
        assert!(ctx.has_scope("credential:read"));
        assert!(!ctx.has_scope("admin"));
        assert!(ctx.membership_id.is_none());
        assert!(ctx.session_id.is_none());
    }

    #[test]
    fn test_request_context_with_membership() {
        let ctx = RequestContext::new(
            "tenant_123",
            "user_456",
            "token_789",
            vec!["credential:read".to_string()],
        )
        .with_membership_id("membership_001")
        .with_session_id("session_001");

        assert_eq!(ctx.membership_id(), Some("membership_001"));
        assert_eq!(ctx.session_id(), Some("session_001"));
    }

    #[test]
    fn test_scope_check_with_admin() {
        let ctx = RequestContext::new(
            "tenant_123",
            "user_456",
            "token_789",
            vec!["admin".to_string()],
        );

        assert!(ctx.has_scope("credential:read"));
        assert!(ctx.has_scope("credential:write"));
        assert!(ctx.has_scope("admin"));
    }

    #[test]
    fn test_verify_tenant_access() {
        let ctx = RequestContext::new(
            "tenant_123",
            "user_456",
            "token_789",
            vec!["credential:read".to_string()],
        );

        assert!(verify_tenant_access(&ctx, "tenant_123").is_ok());
        assert!(verify_tenant_access(&ctx, "tenant_456").is_err());
    }

    #[test]
    fn test_sql_escape() {
        assert_eq!(
            escape_sql_string("test' OR '1'='1"),
            "test\\' OR \\'1\\'=\\'1"
        );
        assert_eq!(escape_sql_string("test\\value"), "test\\\\value");
    }

    #[test]
    fn test_tenant_query_builder() {
        let builder = TenantQueryBuilder::new("tenant_123", "SELECT * FROM credentials");
        let (query, params) = builder.build();

        assert!(query.contains("WHERE tenant_id = 'tenant_123'"));
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], "tenant_123");
    }

    #[test]
    fn test_tenant_query_builder_with_existing_where() {
        let builder = TenantQueryBuilder::new(
            "tenant_123",
            "SELECT * FROM credentials WHERE is_active = true",
        );
        let (query, _params) = builder.build();

        assert!(query.contains("WHERE is_active = true"));
        assert!(query.contains("AND tenant_id = 'tenant_123'"));
    }

    #[test]
    fn test_rls_context_sql() {
        let ctx = RlsContext {
            tenant_id: "tenant_123".to_string(),
            user_id: "user_456".to_string(),
            membership_id: None,
            scopes: vec!["read".to_string(), "write".to_string()],
            is_admin: false,
        };

        let statements = ctx.to_sql_statements();
        assert_eq!(statements.len(), 4);
        assert!(statements[0].contains("SET app.current_tenant_id = 'tenant_123'"));
        assert!(statements[1].contains("SET app.current_user_id = 'user_456'"));
        assert!(statements[2].contains("SET app.current_scopes = 'read,write'"));
        assert!(statements[3].contains("SET app.is_admin = 'false'"));
    }

    #[test]
    fn test_rls_context_with_membership() {
        let ctx = RlsContext::new("tenant_123", "user_456", vec!["read".to_string()])
            .with_membership_id("membership_001");

        let statements = ctx.to_sql_statements();
        assert_eq!(statements.len(), 5);
        assert!(statements[4].contains("SET app.current_membership_id = 'membership_001'"));

        let sql = ctx.to_sql_transaction_local();
        assert!(sql.contains("SET LOCAL app.current_membership_id = 'membership_001'"));
    }

    #[test]
    fn test_rls_context_transaction_local() {
        let ctx = RlsContext {
            tenant_id: "tenant_123".to_string(),
            user_id: "user_456".to_string(),
            membership_id: None,
            scopes: vec!["read".to_string(), "admin".to_string()],
            is_admin: true,
        };

        let sql = ctx.to_sql_transaction_local();
        assert!(sql.contains("SET LOCAL app.current_tenant_id = 'tenant_123'"));
        assert!(sql.contains("SET LOCAL app.is_admin = 'true'"));
    }

    #[test]
    fn test_rls_context_is_admin() {
        let admin_ctx = RlsContext::new("tenant_1", "user_1", vec!["admin".to_string()]);
        assert!(admin_ctx.is_admin());
        assert!(admin_ctx.has_scope("any_scope"));

        let normal_ctx = RlsContext::new("tenant_2", "user_2", vec!["read".to_string()]);
        assert!(!normal_ctx.is_admin());
        assert!(normal_ctx.has_scope("read"));
        assert!(!normal_ctx.has_scope("write"));
    }

    #[test]
    fn test_rls_context_from_request() {
        let req_ctx = RequestContext::new(
            "tenant_123",
            "user_456",
            "token_789",
            vec!["admin".to_string(), "write".to_string()],
        )
        .with_membership_id("membership_001");

        let rls_ctx = RlsContext::from_request_context(&req_ctx);
        assert_eq!(rls_ctx.tenant_id, "tenant_123");
        assert_eq!(rls_ctx.user_id, "user_456");
        assert_eq!(rls_ctx.membership_id, Some("membership_001".to_string()));
        assert!(rls_ctx.is_admin);
    }
}
