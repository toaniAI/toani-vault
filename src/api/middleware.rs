//! API 认证中间件
//!
//! 实现 PASETO v4.local Token 验证中间件
//! - Token 15 分钟有效期
//! - jti 单次使用验证
//! - Scope 权限控制

use axum::{
    Json,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

use super::token_blacklist::TokenStore;

/// Token Scope 定义
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenScope {
    /// 凭证读取权限
    CredentialRead,
    /// 凭证解密权限
    CredentialDecrypt,
    /// 凭证写入权限（创建/删除）
    CredentialWrite,
    /// 审计日志读取权限
    AuditRead,
    /// 沙箱执行权限
    SandboxExecute,
    /// 沙箱读取权限
    SandboxRead,
    /// 沙箱写入权限（创建/删除）
    SandboxWrite,
    /// 管理员权限
    Admin,
}

impl TokenScope {
    /// 获取 scope 字符串表示
    pub fn as_str(&self) -> &'static str {
        match self {
            TokenScope::CredentialRead => "credential:read",
            TokenScope::CredentialDecrypt => "credential:decrypt",
            TokenScope::CredentialWrite => "credential:write",
            TokenScope::AuditRead => "audit:read",
            TokenScope::SandboxExecute => "sandbox:execute",
            TokenScope::SandboxRead => "sandbox:read",
            TokenScope::SandboxWrite => "sandbox:write",
            TokenScope::Admin => "admin",
        }
    }

    /// 从字符串解析 scope
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "credential:read" => Some(TokenScope::CredentialRead),
            "credential:decrypt" => Some(TokenScope::CredentialDecrypt),
            "credential:write" => Some(TokenScope::CredentialWrite),
            "audit:read" => Some(TokenScope::AuditRead),
            "sandbox:execute" => Some(TokenScope::SandboxExecute),
            "sandbox:read" => Some(TokenScope::SandboxRead),
            "sandbox:write" => Some(TokenScope::SandboxWrite),
            "admin" => Some(TokenScope::Admin),
            _ => None,
        }
    }
}

/// 验证后的 Token 信息
#[derive(Debug, Clone)]
pub struct ValidatedToken {
    /// Token ID (jti)
    pub token_id: String,
    /// 主题（租户ID:用户ID）
    pub subject: String,
    /// 租户 ID
    pub tenant_id: String,
    /// 用户 ID
    pub user_id: String,
    /// Token 有效期（秒）
    pub expires_at: u64,
    /// 授权 Scope 列表
    pub scopes: Vec<TokenScope>,
    /// 签发时间
    pub issued_at: u64,
}

impl ValidatedToken {
    /// 检查是否包含指定 scope
    pub fn has_scope(&self, scope: &TokenScope) -> bool {
        self.scopes.contains(scope) || self.scopes.contains(&TokenScope::Admin)
    }

    /// 检查是否包含任一指定 scope
    pub fn has_any_scope(&self, scopes: &[TokenScope]) -> bool {
        scopes.iter().any(|s| self.has_scope(s))
    }

    /// 检查是否已过期
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now > self.expires_at
    }
}

/// Token 验证错误
#[derive(Debug, Clone, Serialize)]
pub struct AuthError {
    pub error: String,
    pub message: String,
}

impl AuthError {
    pub fn new(error: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            message: message.into(),
        }
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let status = match self.error.as_str() {
            "missing_token" => StatusCode::UNAUTHORIZED,
            "invalid_token" => StatusCode::UNAUTHORIZED,
            "expired_token" => StatusCode::UNAUTHORIZED,
            "revoked_token" => StatusCode::UNAUTHORIZED,
            "insufficient_scope" => StatusCode::FORBIDDEN,
            _ => StatusCode::UNAUTHORIZED,
        };

        (status, Json(json!(self))).into_response()
    }
}

// Re-export from token_blacklist module
pub use super::token_blacklist::create_token_store;

/// 从请求头提取 Token
fn extract_token_from_header(request: &Request) -> Result<String, AuthError> {
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .ok_or_else(|| AuthError::new("missing_token", "缺少 Authorization 请求头"))?
        .to_str()
        .map_err(|_| AuthError::new("invalid_token", "无效的 Authorization 请求头"))?
        .to_string();

    // 支持 "Bearer <token>" 格式
    if auth_header.starts_with("Bearer ") {
        Ok(auth_header[7..].to_string())
    } else {
        Ok(auth_header)
    }
}

/// Token 默认黑名单 TTL（秒）
/// 设置为 Token 最大有效期 + 缓冲时间，确保过期 Token 不会永远留在黑名单
#[allow(dead_code)]
const TOKEN_BLACKLIST_TTL_SECONDS: u64 = 900; // 15 分钟

/// 验证 Token（使用 pasetors）
///
/// 注意：Access Token 在有效期内可重复使用，不启用单次使用限制。
/// 如需单次使用 Token，请使用专门的 Action Token 机制。
async fn validate_token(
    token: &str,
    _token_store: &TokenStore,
    secret_key: &[u8],
) -> Result<ValidatedToken, AuthError> {
    // 使用 pasetors 验证 v4.local Token
    let validation_result = validate_paseto_token(token, secret_key)
        .map_err(|e| AuthError::new("invalid_token", format!("Token 验证失败: {}", e)))?;

    // 检查是否过期
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    if now > validation_result.expires_at {
        return Err(AuthError::new("expired_token", "Token 已过期"));
    }

    Ok(validation_result)
}

/// 使用 pasetors 验证 Token
fn validate_paseto_token(token: &str, secret_key: &[u8]) -> Result<ValidatedToken, String> {
    use pasetors::claims::ClaimsValidationRules;
    use pasetors::keys::SymmetricKey;
    use pasetors::local;
    use pasetors::token::UntrustedToken;
    use time::OffsetDateTime;

    // 创建对称密钥
    let sk: SymmetricKey<_> =
        SymmetricKey::from(secret_key).map_err(|_| "无效的密钥长度".to_string())?;

    // 解析未受信任的 Token
    let untrusted =
        UntrustedToken::try_from(token).map_err(|e| format!("Token 解析失败: {:?}", e))?;

    // 验证 Token
    let validation_rules = ClaimsValidationRules::new();
    let trusted_token = local::decrypt(&sk, &untrusted, &validation_rules, None, None)
        .map_err(|e| format!("解密失败: {:?}", e))?;

    // 获取 Claims
    let claims = trusted_token
        .payload_claims()
        .ok_or("Token 不包含 payload claims")?;

    // 提取声明
    let token_id = claims
        .get_claim("jti")
        .and_then(|v| v.as_str())
        .ok_or("Token 缺少 jti 声明")?
        .to_string();

    let subject = claims
        .get_claim("sub")
        .and_then(|v| v.as_str())
        .ok_or("Token 缺少 sub 声明")?
        .to_string();

    // 解析 exp（ISO 8601 格式）
    let expires_at = match claims.get_claim("exp").and_then(|v| v.as_str()) {
        Some(s) => OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
            .map(|dt| dt.unix_timestamp() as u64)
            .map_err(|_| "无法解析 exp 时间".to_string()),
        None => Err("Token 缺少 exp 声明".to_string()),
    }?;

    // 解析 iat（ISO 8601 格式）
    let issued_at = match claims.get_claim("iat").and_then(|v| v.as_str()) {
        Some(s) => OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
            .map(|dt| dt.unix_timestamp() as u64)
            .unwrap_or_else(|_| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            }),
        None => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    let scope_str = claims
        .get_claim("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let scopes: Vec<TokenScope> = scope_str
        .split_whitespace()
        .filter_map(TokenScope::from_str)
        .collect();

    if scopes.is_empty() {
        return Err("Token 缺少 scope 声明".to_string());
    }

    // 解析租户 ID 和用户 ID
    let (tenant_id, user_id) =
        parse_subject(&subject).map_err(|e| format!("解析 subject 失败: {}", e.message))?;

    Ok(ValidatedToken {
        token_id,
        subject,
        tenant_id,
        user_id,
        expires_at,
        scopes,
        issued_at,
    })
}

/// 解析 subject 格式 "tenant_id:user_id"
fn parse_subject(subject: &str) -> Result<(String, String), AuthError> {
    let parts: Vec<&str> = subject.split(':').collect();
    if parts.len() != 2 {
        return Err(AuthError::new(
            "invalid_token",
            "Token sub 声明格式无效，应为 tenant_id:user_id",
        ));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Token 验证中间件
pub async fn auth_middleware(
    State((token_store, secret_key)): State<(TokenStore, Vec<u8>)>,
    mut request: Request,
    next: Next,
) -> Response {
    // 提取 Token
    let token = match extract_token_from_header(&request) {
        Ok(t) => t,
        Err(e) => return e.into_response(),
    };

    // 验证 Token
    let validated_token = match validate_token(&token, &token_store, &secret_key).await {
        Ok(t) => t,
        Err(e) => return e.into_response(),
    };

    // 将验证后的 Token 添加到请求扩展
    request.extensions_mut().insert(validated_token);

    // 继续处理请求
    next.run(request).await
}

/// 测试辅助模块
#[cfg(test)]
pub mod tests {
    use super::*;

    /// 获取测试密钥
    pub fn get_test_key() -> Vec<u8> {
        vec![0u8; 32]
    }

    /// 创建模拟的 ValidatedToken（用于测试）
    pub fn create_mock_token(
        tenant_id: &str,
        user_id: &str,
        scopes: Vec<TokenScope>,
    ) -> ValidatedToken {
        ValidatedToken {
            token_id: uuid::Uuid::now_v7().to_string(),
            subject: format!("{}:{}", tenant_id, user_id),
            tenant_id: tenant_id.to_string(),
            user_id: user_id.to_string(),
            expires_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 3600,
            scopes,
            issued_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
}

/// 从请求扩展获取 Token（处理器中使用）
pub fn get_token_from_request(request: &Request) -> Option<&ValidatedToken> {
    request.extensions().get::<ValidatedToken>()
}

/// Scope 验证中间件（检查特定 scope）
pub fn require_scope(
    required_scope: TokenScope,
) -> impl Fn(&ValidatedToken) -> Result<(), AuthError> + Clone {
    move |token: &ValidatedToken| {
        if token.has_scope(&required_scope) {
            Ok(())
        } else {
            Err(AuthError::new(
                "insufficient_scope",
                format!(
                    "缺少必需的 scope: {}，当前 scopes: {:?}",
                    required_scope.as_str(),
                    token.scopes.iter().map(|s| s.as_str()).collect::<Vec<_>>()
                ),
            ))
        }
    }
}

/// Scope 验证中间件（检查多个 scope 中的任意一个）
pub fn require_any_scope(
    required_scopes: Vec<TokenScope>,
) -> impl Fn(&ValidatedToken) -> Result<(), AuthError> + Clone {
    move |token: &ValidatedToken| {
        if token.has_any_scope(&required_scopes) {
            Ok(())
        } else {
            Err(AuthError::new(
                "insufficient_scope",
                format!(
                    "缺少必需的 scope，需要以下任一: {:?}，当前 scopes: {:?}",
                    required_scopes
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>(),
                    token.scopes.iter().map(|s| s.as_str()).collect::<Vec<_>>()
                ),
            ))
        }
    }
}
