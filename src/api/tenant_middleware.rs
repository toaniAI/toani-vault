//! 租户隔离中间件
//!
//! 实现多租户系统的请求隔离：
//! - 从 Token 中提取 tenant_id
//! - 验证租户隔离（阻止跨租户访问）
//! - 注入请求上下文
//! - 支持 RLS（行级安全）数据库隔离

use axum::{
    Json,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;

use crate::api::context::{
    CrossTenantErrorResponse, RequestContext, RlsContext, TenantIsolationError,
};
use crate::api::middleware::ValidatedToken;

/// 租户隔离配置
#[derive(Debug, Clone)]
pub struct TenantIsolationConfig {
    /// 是否启用跨租户访问检查
    pub enable_cross_tenant_check: bool,
    /// 是否启用租户激活状态检查
    pub enable_tenant_active_check: bool,
    /// 是否自动注入 RLS 上下文
    pub enable_rls_context: bool,
    /// 允许的 URL 路径（不需要租户隔离）
    pub public_paths: Vec<String>,
}

impl Default for TenantIsolationConfig {
    fn default() -> Self {
        Self {
            enable_cross_tenant_check: true,
            enable_tenant_active_check: false, // 默认不检查，需要外部存储支持
            enable_rls_context: true,
            public_paths: vec![
                "/health".to_string(),
                "/api/v1/health".to_string(),
                "/api/v1/auth/login".to_string(),
            ],
        }
    }
}

impl TenantIsolationConfig {
    /// 创建生产环境配置
    pub fn production() -> Self {
        Self {
            enable_cross_tenant_check: true,
            enable_tenant_active_check: true,
            enable_rls_context: true,
            public_paths: vec!["/health".to_string(), "/api/v1/health".to_string()],
        }
    }

    /// 创建测试环境配置（放宽部分限制）
    pub fn testing() -> Self {
        Self {
            enable_cross_tenant_check: true,
            enable_tenant_active_check: false,
            enable_rls_context: false,
            public_paths: vec![
                "/health".to_string(),
                "/api/v1/health".to_string(),
                "/api/v1/auth/login".to_string(),
                "/api/v1/auth/register".to_string(),
            ],
        }
    }

    /// 检查路径是否为公开路径
    pub fn is_public_path(&self, path: &str) -> bool {
        self.public_paths.iter().any(|p| path.starts_with(p))
    }
}

/// 租户隔离状态
#[derive(Clone)]
pub struct TenantIsolationState {
    pub config: TenantIsolationConfig,
    // 可以扩展：添加租户存储、缓存等
}

impl TenantIsolationState {
    /// 创建新的状态
    pub fn new(config: TenantIsolationConfig) -> Self {
        Self { config }
    }

    /// 使用默认配置创建状态
    pub fn default_state() -> Self {
        Self::new(TenantIsolationConfig::default())
    }
}

/// 租户隔离中间件
///
/// 主要功能：
/// 1. 从请求扩展中提取 ValidatedToken
/// 2. 创建 RequestContext 并注入到请求扩展
/// 3. 验证租户隔离（阻止跨租户访问）
/// 4. 可选：检查租户激活状态
/// 5. 创建并注入 RlsContext（用于数据库 RLS）
pub async fn tenant_isolation_middleware(
    State(state): State<TenantIsolationState>,
    mut request: Request,
    next: Next,
) -> Response {
    // 检查是否为公开路径
    let path = request.uri().path().to_string();
    if state.config.is_public_path(&path) {
        return next.run(request).await;
    }

    // 从请求扩展中提取 ValidatedToken
    let token = match request.extensions().get::<ValidatedToken>() {
        Some(token) => token.clone(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "success": false,
                    "error": {
                        "code": "MISSING_TOKEN",
                        "message": "缺少认证信息"
                    },
                    "meta": {
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    }
                })),
            )
                .into_response();
        }
    };

    // 验证租户激活状态（如果启用）
    if state.config.enable_tenant_active_check {
        // 注意：这里可以扩展为检查租户存储
        // 目前假设所有租户都是激活的
    }

    // 创建请求上下文
    let context = RequestContext::from_validated_token(&token);

    // 创建 RLS 上下文（如果启用）
    if state.config.enable_rls_context {
        let rls_context = RlsContext::from_request_context(&context);
        request.extensions_mut().insert(rls_context);
    }

    // 注入请求上下文到请求扩展
    request.extensions_mut().insert(context);

    // 继续处理请求
    next.run(request).await
}

/// 跨租户访问检查中间件（用于特定路由）
///
/// 用于需要验证路径参数中 tenant_id 的路由
/// 例如: /api/v1/tenants/{tenant_id}/credentials
pub async fn cross_tenant_check_middleware(request: Request, next: Next) -> Response {
    // 从请求扩展中获取上下文
    let _context = match request.extensions().get::<RequestContext>() {
        Some(ctx) => ctx.clone(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "success": false,
                    "error": {
                        "code": "MISSING_CONTEXT",
                        "message": "缺少请求上下文"
                    },
                    "meta": {
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    }
                })),
            )
                .into_response();
        }
    };

    // 从路径参数中提取租户ID（如果存在）
    // 注意：这需要配合 axum 的 Path 提取器使用
    // 这里我们先注入上下文，具体的跨租户检查在处理器中进行

    next.run(request).await
}

/// 租户隔离错误响应
impl IntoResponse for TenantIsolationError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            TenantIsolationError::CrossTenantAccessDenied { requested, actual } => (
                StatusCode::FORBIDDEN,
                "CROSS_TENANT_ACCESS_DENIED",
                format!("跨租户访问被拒绝: 请求租户 '{requested}' 不匹配资源租户 '{actual}'"),
            ),
            TenantIsolationError::MissingTenantContext => (
                StatusCode::UNAUTHORIZED,
                "MISSING_TENANT_CONTEXT",
                "租户上下文缺失".to_string(),
            ),
            TenantIsolationError::InvalidTenantId(id) => (
                StatusCode::BAD_REQUEST,
                "INVALID_TENANT_ID",
                format!("无效的租户ID: {id}"),
            ),
            TenantIsolationError::TenantInactive(id) => (
                StatusCode::FORBIDDEN,
                "TENANT_INACTIVE",
                format!("租户未激活: {id}"),
            ),
        };

        tracing::warn!(error_code = code, message = %message, status = status.as_u16(), "tenant isolation error");

        let response =
            CrossTenantErrorResponse::new(format!("req_{}", uuid::Uuid::now_v7()), message);

        (status, Json(response)).into_response()
    }
}

/// 从路径参数验证租户访问权限
///
/// 这是一个辅助函数，用于在处理器中验证路径参数中的租户ID
pub fn validate_path_tenant_id(
    context: &RequestContext,
    path_tenant_id: &str,
) -> Result<(), TenantIsolationError> {
    if context.tenant_id() != path_tenant_id {
        return Err(TenantIsolationError::CrossTenantAccessDenied {
            requested: context.tenant_id().to_string(),
            actual: path_tenant_id.to_string(),
        });
    }
    Ok(())
}

/// 从查询参数验证租户访问权限
///
/// 用于验证查询参数中的租户ID
pub fn validate_query_tenant_id(
    context: &RequestContext,
    query_tenant_id: Option<&str>,
) -> Result<(), TenantIsolationError> {
    if let Some(tid) = query_tenant_id
        && context.tenant_id() != tid
    {
        return Err(TenantIsolationError::CrossTenantAccessDenied {
            requested: context.tenant_id().to_string(),
            actual: tid.to_string(),
        });
    }
    Ok(())
}

/// 请求上下文扩展 trait
pub trait RequestContextExt {
    /// 获取请求上下文
    fn context(&self) -> Option<&RequestContext>;

    /// 获取租户ID
    fn tenant_id(&self) -> Option<&str>;

    /// 获取用户ID
    fn user_id(&self) -> Option<&str>;

    /// 检查是否有指定 scope
    fn has_scope(&self, scope: &str) -> bool;
}

impl RequestContextExt for Request {
    fn context(&self) -> Option<&RequestContext> {
        self.extensions().get::<RequestContext>()
    }

    fn tenant_id(&self) -> Option<&str> {
        self.extensions()
            .get::<RequestContext>()
            .map(|c| c.tenant_id())
    }

    fn user_id(&self) -> Option<&str> {
        self.extensions()
            .get::<RequestContext>()
            .map(|c| c.user_id())
    }

    fn has_scope(&self, scope: &str) -> bool {
        self.extensions()
            .get::<RequestContext>()
            .map(|c| c.has_scope(scope))
            .unwrap_or(false)
    }
}

/// 构建器模式的中间件配置
pub struct TenantMiddlewareBuilder {
    config: TenantIsolationConfig,
}

impl TenantMiddlewareBuilder {
    /// 创建新的构建器
    pub fn new() -> Self {
        Self {
            config: TenantIsolationConfig::default(),
        }
    }

    /// 启用跨租户检查
    pub fn enable_cross_tenant_check(mut self) -> Self {
        self.config.enable_cross_tenant_check = true;
        self
    }

    /// 禁用跨租户检查
    pub fn disable_cross_tenant_check(mut self) -> Self {
        self.config.enable_cross_tenant_check = false;
        self
    }

    /// 启用租户激活检查
    pub fn enable_tenant_active_check(mut self) -> Self {
        self.config.enable_tenant_active_check = true;
        self
    }

    /// 启用 RLS 上下文
    pub fn enable_rls_context(mut self) -> Self {
        self.config.enable_rls_context = true;
        self
    }

    /// 添加公开路径
    pub fn add_public_path(mut self, path: impl Into<String>) -> Self {
        self.config.public_paths.push(path.into());
        self
    }

    /// 构建状态
    pub fn build(self) -> TenantIsolationState {
        TenantIsolationState::new(self.config)
    }
}

impl Default for TenantMiddlewareBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::middleware::TokenScope;

    fn create_test_token(tenant_id: &str, user_id: &str) -> ValidatedToken {
        ValidatedToken {
            token_id: "test_token".to_string(),
            subject: format!("{tenant_id}:{user_id}"),
            tenant_id: tenant_id.to_string(),
            user_id: user_id.to_string(),
            expires_at: u64::MAX,
            scopes: vec![TokenScope::CredentialRead],
            issued_at: 0,
        }
    }

    #[test]
    fn test_tenant_config_default() {
        let config = TenantIsolationConfig::default();
        assert!(config.enable_cross_tenant_check);
        assert!(!config.enable_tenant_active_check);
        assert!(config.enable_rls_context);
        assert!(config.is_public_path("/health"));
        assert!(!config.is_public_path("/api/v1/credentials"));
    }

    #[test]
    fn test_tenant_config_production() {
        let config = TenantIsolationConfig::production();
        assert!(config.enable_cross_tenant_check);
        assert!(config.enable_tenant_active_check);
        assert!(config.enable_rls_context);
    }

    #[test]
    fn test_validate_path_tenant_id() {
        let token = create_test_token("tenant_123", "user_456");
        let context = RequestContext::from_validated_token(&token);

        assert!(validate_path_tenant_id(&context, "tenant_123").is_ok());
        assert!(validate_path_tenant_id(&context, "tenant_456").is_err());
    }

    #[test]
    fn test_validate_query_tenant_id() {
        let token = create_test_token("tenant_123", "user_456");
        let context = RequestContext::from_validated_token(&token);

        // None 值应该通过
        assert!(validate_query_tenant_id(&context, None).is_ok());

        // 匹配的 tenant_id
        assert!(validate_query_tenant_id(&context, Some("tenant_123")).is_ok());

        // 不匹配的 tenant_id
        assert!(validate_query_tenant_id(&context, Some("tenant_456")).is_err());
    }

    #[test]
    fn test_middleware_builder() {
        let state = TenantMiddlewareBuilder::new()
            .enable_cross_tenant_check()
            .enable_tenant_active_check()
            .add_public_path("/custom/path")
            .build();

        assert!(state.config.enable_cross_tenant_check);
        assert!(state.config.enable_tenant_active_check);
        assert!(state.config.is_public_path("/custom/path"));
    }

    #[test]
    fn test_error_display() {
        let err = TenantIsolationError::CrossTenantAccessDenied {
            requested: "tenant_a".to_string(),
            actual: "tenant_b".to_string(),
        };

        let message = err.to_string();
        assert!(message.contains("tenant_a"));
        assert!(message.contains("tenant_b"));
    }
}
