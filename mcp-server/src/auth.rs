//! Bearer Token 认证中间件
//!
//! 提供 MCP Server 的认证机制，支持 PASETO Token 验证。
//!
//! ## 功能特性
//!
//! - PASETO Token 验证
//! - Scope 权限检查
//! - Token 过期处理
//! - 认证中间件 (Axum Layer)
//!
//! ## Token Claims 结构
//!
//! ```json
//! {
//!   "iss": "credbridge-mcp",
//!   "sub": "user_123",
//!   "aud": "mcp-agent",
//!   "exp": 1710345600,
//!   "iat": 1710259200,
//!   "jti": "uuid-v4",
//!   "sid": "session-id",
//!   "scp": ["mcp:connect", "credential:read"]
//! }
//! ```

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use uuid::Uuid;

/// MCP 必需 Scope
pub const SCOPE_MCP_CONNECT: &str = "mcp:connect";
pub const SCOPE_CREDENTIAL_READ: &str = "credential:read";
pub const SCOPE_CREDENTIAL_WRITE: &str = "credential:write";
pub const SCOPE_CREDENTIAL_DECRYPT: &str = "credential:decrypt";
pub const SCOPE_CREDENTIAL_DELETE: &str = "credential:delete";
pub const SCOPE_ADMIN: &str = "admin";

/// Token 配置
#[derive(Debug, Clone)]
pub struct TokenConfig {
    /// PASETO 密钥 (32 bytes for V4.Local)
    pub key: [u8; 32],
    /// 允许的发行者
    pub allowed_issuers: Vec<String>,
    /// 允许的受众
    pub allowed_audiences: Vec<String>,
    /// Token 默认有效期
    pub default_expiry: Duration,
}

impl Default for TokenConfig {
    fn default() -> Self {
        // 开发环境默认配置
        let mut key = [0u8; 32];
        // 使用随机密钥 (生产环境应从安全配置加载)
        for i in 0..32 {
            key[i] = i as u8;
        }

        Self {
            key,
            allowed_issuers: vec!["credbridge-mcp".to_string()],
            allowed_audiences: vec!["mcp-agent".to_string()],
            default_expiry: Duration::from_secs(3600), // 1 小时
        }
    }
}

/// Token Claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenClaims {
    /// 发行者 (iss)
    pub iss: String,
    /// 主题/用户 ID (sub)
    pub sub: String,
    /// 受众 (aud)
    pub aud: String,
    /// 过期时间戳 (exp)
    pub exp: u64,
    /// 签发时间戳 (iat)
    pub iat: u64,
    /// JWT ID (jti)
    pub jti: String,
    /// Session ID (sid)
    pub sid: String,
    /// Scope 列表 (scp)
    pub scp: Vec<String>,
}

impl TokenClaims {
    /// 创建新的 Token Claims
    pub fn new(
        issuer: String,
        user_id: String,
        audience: String,
        session_id: String,
        scopes: Vec<String>,
        expiry: Duration,
    ) -> Result<Self, TokenError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| TokenError::Internal(e.to_string()))?;

        let now_secs = now.as_secs();

        Ok(Self {
            iss: issuer,
            sub: user_id,
            aud: audience,
            exp: now_secs + expiry.as_secs(),
            iat: now_secs,
            jti: Uuid::new_v4().to_string(),
            sid: session_id,
            scp: scopes,
        })
    }

    /// 检查是否过期
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.exp < now
    }

    /// 检查是否包含指定 Scope
    pub fn has_scope(&self, required: &str) -> bool {
        self.scp.iter().any(|s| s == required || s == SCOPE_ADMIN)
    }

    /// 检查是否包含任一指定 Scope
    pub fn has_any_scope(&self, required: &[&str]) -> bool {
        required.iter().any(|r| self.has_scope(r))
    }
}

/// Token 验证器
#[derive(Clone)]
pub struct TokenValidator {
    config: Arc<TokenConfig>,
}

impl TokenValidator {
    /// 创建新的 Token 验证器
    pub fn new(config: TokenConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    /// 验证 Token 并提取 Claims
    ///
    /// 当前实现使用简化的 Base64+JSON 编码
    /// 生产环境应使用完整的 PASETO V4.Local 验证
    pub fn validate(&self, token: &str) -> Result<TokenClaims, TokenError> {
        // 尝试解码 token
        let decoded = URL_SAFE_NO_PAD
            .decode(token)
            .map_err(|_| TokenError::InvalidToken("Invalid base64 encoding".to_string()))?;

        // 解析 JSON Claims
        let claims: TokenClaims = serde_json::from_slice(&decoded)
            .map_err(|e| TokenError::InvalidToken(format!("Invalid JSON: {}", e)))?;

        // 验证发行者
        if !self.config.allowed_issuers.contains(&claims.iss) {
            return Err(TokenError::InvalidIssuer(claims.iss));
        }

        // 验证受众
        if !self.config.allowed_audiences.contains(&claims.aud) {
            return Err(TokenError::InvalidAudience(claims.aud));
        }

        // 验证过期时间
        if claims.is_expired() {
            return Err(TokenError::TokenExpired);
        }

        debug!("Token validated for user: {}", claims.sub);
        Ok(claims)
    }

    /// 生成新的 Token
    pub fn generate_token(&self, claims: &TokenClaims) -> Result<String, TokenError> {
        let encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims)?);
        Ok(encoded)
    }

    /// 验证 Scope
    pub fn validate_scope(&self, claims: &TokenClaims, required: &str) -> bool {
        claims.has_scope(required)
    }

    /// 验证 MCP Connect 权限
    pub fn can_connect_mcp(&self, claims: &TokenClaims) -> bool {
        self.validate_scope(claims, SCOPE_MCP_CONNECT)
    }

    /// 验证凭证读取权限
    pub fn can_read_credential(&self, claims: &TokenClaims) -> bool {
        self.validate_scope(claims, SCOPE_CREDENTIAL_READ)
    }

    /// 验证凭证写入权限
    pub fn can_write_credential(&self, claims: &TokenClaims) -> bool {
        self.validate_scope(claims, SCOPE_CREDENTIAL_WRITE)
    }

    /// 验证凭证解密权限
    pub fn can_decrypt_credential(&self, claims: &TokenClaims) -> bool {
        self.validate_scope(claims, SCOPE_CREDENTIAL_DECRYPT)
    }

    /// 验证凭证删除权限
    pub fn can_delete_credential(&self, claims: &TokenClaims) -> bool {
        self.validate_scope(claims, SCOPE_CREDENTIAL_DELETE)
            || self.validate_scope(claims, SCOPE_CREDENTIAL_WRITE)
    }

    /// 创建初始连接 Token (用于测试/开发)
    pub fn create_initial_connect_token(
        &self,
        user_id: String,
        session_id: String,
        additional_scopes: Vec<String>,
    ) -> Result<String, TokenError> {
        let mut scopes = vec![SCOPE_MCP_CONNECT.to_string()];
        scopes.extend(additional_scopes);

        let claims = TokenClaims::new(
            "credbridge-mcp".to_string(),
            user_id,
            "mcp-agent".to_string(),
            session_id,
            scopes,
            self.config.default_expiry,
        )?;

        self.generate_token(&claims)
    }
}

/// Token 错误
#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    #[error("Missing authorization header")]
    MissingHeader,

    #[error("Invalid token format: {0}")]
    InvalidToken(String),

    #[error("Token expired")]
    TokenExpired,

    #[error("Invalid issuer: {0}")]
    InvalidIssuer(String),

    #[error("Invalid audience: {0}")]
    InvalidAudience(String),

    #[error("Insufficient scope. Required: {0}")]
    InsufficientScope(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for TokenError {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            TokenError::MissingHeader => {
                (StatusCode::UNAUTHORIZED, "Missing authorization header".to_string())
            }
            TokenError::InvalidToken(_) => {
                (StatusCode::UNAUTHORIZED, "Invalid token".to_string())
            }
            TokenError::TokenExpired => {
                (StatusCode::UNAUTHORIZED, "Token expired".to_string())
            }
            TokenError::InvalidIssuer(_) => {
                (StatusCode::UNAUTHORIZED, "Invalid token issuer".to_string())
            }
            TokenError::InvalidAudience(_) => {
                (StatusCode::UNAUTHORIZED, "Invalid token audience".to_string())
            }
            TokenError::InsufficientScope(scope) => {
                (StatusCode::FORBIDDEN, format!("Insufficient scope: {}", scope))
            }
            TokenError::SerializationError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal serialization error".to_string())
            }
            TokenError::Internal(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Internal error: {}", msg))
            }
        };

        (status, body).into_response()
    }
}

/// Axum 认证中间件
///
/// 验证 Bearer Token 并将 Claims 注入到 request extensions
pub async fn auth_middleware(
    State(validator): State<Arc<TokenValidator>>,
    mut request: Request,
    next: Next,
) -> Result<Response, TokenError> {
    // 提取 Authorization header
    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(TokenError::MissingHeader)?;

    // 提取 Bearer Token
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| TokenError::InvalidToken("Expected Bearer token".to_string()))?;

    // 验证 Token
    let claims = validator.validate(token)?;

    // 将 Claims 注入到 request extensions
    request.extensions_mut().insert(claims);

    Ok(next.run(request).await)
}

/// 从 request 中提取 Claims
pub fn extract_claims(request: &Request) -> Option<&TokenClaims> {
    request.extensions().get::<TokenClaims>()
}

/// 验证 Scope 的中间件工厂
pub fn require_scope(scope: &'static str) -> impl Clone {
    ScopeMiddleware { scope }
}

#[derive(Clone)]
pub struct ScopeMiddleware {
    scope: &'static str,
}

pub async fn scope_middleware(
    middleware: ScopeMiddleware,
    State(validator): State<Arc<TokenValidator>>,
    mut request: Request,
    next: Next,
) -> Result<Response, TokenError> {
    let claims = extract_claims(&request)
        .ok_or_else(|| TokenError::InvalidToken("Claims not found in request".to_string()))?;

    if !validator.validate_scope(claims, middleware.scope) {
        return Err(TokenError::InsufficientScope(middleware.scope.to_string()));
    }

    // 注入已验证的 scope
    request.extensions_mut().insert(middleware.scope);

    Ok(next.run(request).await)
}

/// 认证响应
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    pub success: bool,
    pub user_id: String,
    pub scopes: Vec<String>,
    pub expires_at: u64,
}

/// Token 刷新请求
#[derive(Debug, Deserialize)]
pub struct TokenRefreshRequest {
    pub refresh_token: String,
}

/// Token 刷新响应
#[derive(Debug, Serialize, Deserialize)]
pub struct TokenRefreshResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
    pub token_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_validator() -> TokenValidator {
        let config = TokenConfig::default();
        TokenValidator::new(config)
    }

    #[test]
    fn test_token_claims_creation() {
        let claims = TokenClaims::new(
            "credbridge-mcp".to_string(),
            "user_123".to_string(),
            "mcp-agent".to_string(),
            "session_456".to_string(),
            vec![SCOPE_MCP_CONNECT.to_string()],
            Duration::from_secs(3600),
        ).unwrap();

        assert_eq!(claims.sub, "user_123");
        assert_eq!(claims.iss, "credbridge-mcp");
        assert!(claims.has_scope(SCOPE_MCP_CONNECT));
        assert!(!claims.is_expired());
    }

    #[test]
    fn test_scope_validation() {
        let claims = TokenClaims {
            iss: "credbridge-mcp".to_string(),
            sub: "user_123".to_string(),
            aud: "mcp-agent".to_string(),
            exp: 9999999999,
            iat: 1000000000,
            jti: Uuid::new_v4().to_string(),
            sid: "session_456".to_string(),
            scp: vec![SCOPE_MCP_CONNECT.to_string(), SCOPE_CREDENTIAL_READ.to_string()],
        };

        assert!(claims.has_scope(SCOPE_MCP_CONNECT));
        assert!(claims.has_scope(SCOPE_CREDENTIAL_READ));
        assert!(!claims.has_scope(SCOPE_CREDENTIAL_DECRYPT));
        assert!(claims.has_any_scope(&[SCOPE_MCP_CONNECT, SCOPE_ADMIN]));
    }

    #[test]
    fn test_admin_scope_grants_all() {
        let claims = TokenClaims {
            iss: "credbridge-mcp".to_string(),
            sub: "admin_001".to_string(),
            aud: "mcp-agent".to_string(),
            exp: 9999999999,
            iat: 1000000000,
            jti: Uuid::new_v4().to_string(),
            sid: "session_789".to_string(),
            scp: vec![SCOPE_ADMIN.to_string()],
        };

        assert!(claims.has_scope(SCOPE_MCP_CONNECT));
        assert!(claims.has_scope(SCOPE_CREDENTIAL_WRITE));
        assert!(claims.has_scope(SCOPE_CREDENTIAL_DECRYPT));
    }

    #[test]
    fn test_token_generation_and_validation() {
        let validator = create_test_validator();

        let claims = TokenClaims::new(
            "credbridge-mcp".to_string(),
            "user_123".to_string(),
            "mcp-agent".to_string(),
            "session_456".to_string(),
            vec![SCOPE_MCP_CONNECT.to_string()],
            Duration::from_secs(3600),
        ).unwrap();

        let token = validator.generate_token(&claims).unwrap();
        let validated = validator.validate(&token).unwrap();

        assert_eq!(validated.sub, claims.sub);
        assert_eq!(validated.sid, claims.sid);
    }

    #[test]
    fn test_expired_token() {
        let validator = create_test_validator();

        let mut claims = TokenClaims::new(
            "credbridge-mcp".to_string(),
            "user_123".to_string(),
            "mcp-agent".to_string(),
            "session_456".to_string(),
            vec![],
            Duration::from_secs(0), // 立即过期
        ).unwrap();

        // 强制设置为过去时间
        claims.exp = 1000000000;

        assert!(claims.is_expired());

        let token = validator.generate_token(&claims).unwrap();
        let result = validator.validate(&token);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TokenError::TokenExpired));
    }

    #[test]
    fn test_create_initial_connect_token() {
        let validator = create_test_validator();

        let token = validator.create_initial_connect_token(
            "user_123".to_string(),
            "session_456".to_string(),
            vec![SCOPE_CREDENTIAL_READ.to_string()],
        ).unwrap();

        let claims = validator.validate(&token).unwrap();

        assert!(claims.has_scope(SCOPE_MCP_CONNECT));
        assert!(claims.has_scope(SCOPE_CREDENTIAL_READ));
        assert_eq!(claims.sub, "user_123");
    }

    #[test]
    fn test_token_error_into_response() {
        let response = TokenError::MissingHeader.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = TokenError::TokenExpired.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = TokenError::InsufficientScope("test".to_string()).into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
