//! 认证 API 模块
//!
//! 提供登录、Token 签发和刷新功能
//! - POST /api/v1/auth/login - 用户登录
//! - POST /api/v1/auth/refresh - Token 刷新
//! - POST /api/v1/tokens - 创建新 Token

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json, Router,
    routing::{post, get},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use super::middleware::{TokenScope, ValidatedToken};

/// 认证 API 状态
#[derive(Clone)]
pub struct AuthApiState {
    /// Token 密钥（32 字节）
    pub secret_key: Vec<u8>,
    /// 用户存储（内存模拟）
    pub user_store: Arc<MemoryUserStore>,
}

/// 内存用户存储
#[derive(Debug, Clone)]
pub struct MemoryUserStore {
    users: std::collections::HashMap<String, UserInfo>,
}

/// 用户信息
#[derive(Debug, Clone)]
pub struct UserInfo {
    pub user_id: String,
    pub tenant_id: String,
    pub password_hash: String,
    pub scopes: Vec<TokenScope>,
}

impl Default for MemoryUserStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryUserStore {
    /// 创建新的用户存储
    pub fn new() -> Self {
        let mut users = std::collections::HashMap::new();

        // 添加默认测试用户
        users.insert(
            "admin".to_string(),
            UserInfo {
                user_id: "user-001".to_string(),
                tenant_id: "tenant-001".to_string(),
                password_hash: "admin123".to_string(), // 生产环境应使用 bcrypt 哈希
                scopes: vec![TokenScope::Admin],
            },
        );

        users.insert(
            "user".to_string(),
            UserInfo {
                user_id: "user-002".to_string(),
                tenant_id: "tenant-001".to_string(),
                password_hash: "user123".to_string(),
                scopes: vec![TokenScope::CredentialRead, TokenScope::CredentialDecrypt],
            },
        );

        Self { users }
    }
}

impl AuthApiState {
    /// 创建认证 API 状态
    pub fn new() -> Self {
        use rand::RngCore;
        
        // 生成随机密钥（生产环境应使用安全的随机数生成器）
        let mut secret_key = vec![0u8; 32];
        rand::thread_rng().fill_bytes(&mut secret_key);
        
        Self {
            secret_key,
            user_store: Arc::new(MemoryUserStore::new()),
        }
    }
}

impl MemoryUserStore {
    /// 验证用户凭据
    pub fn verify_user(&self, username: &str, password: &str) -> Option<UserInfo> {
        self.users.get(username).and_then(|user| {
            if user.password_hash == password {
                Some(user.clone())
            } else {
                None
            }
        })
    }

    /// 获取用户信息
    pub fn get_user(&self, user_id: &str) -> Option<&UserInfo> {
        self.users.values().find(|u| u.user_id == user_id)
    }
}

// ============================================================================
// 请求/响应模型
// ============================================================================

/// 登录请求
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// 用户名
    pub username: String,
    /// 密码
    pub password: String,
}

/// 登录响应
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    /// 访问 Token
    pub access_token: String,
    /// 刷新 Token
    pub refresh_token: String,
    /// Token 类型
    pub token_type: String,
    /// 过期时间（秒）
    pub expires_in: u64,
    /// 用户 ID
    pub user_id: String,
    /// 租户 ID
    pub tenant_id: String,
    /// 授权 Scope
    pub scope: String,
}

/// Token 创建请求
#[derive(Debug, Deserialize)]
pub struct CreateTokenRequest {
    /// 用户 ID（可选，管理员可指定）
    pub user_id: Option<String>,
    /// 请求的 Scope 列表
    pub scopes: Vec<String>,
    /// Token 有效期（秒），默认 900（15 分钟）
    pub expires_in: Option<u64>,
    /// 关联的凭证 ID 列表（可选，用于受限 Token）
    pub credential_ids: Option<Vec<String>>,
}

/// Token 创建响应
#[derive(Debug, Serialize)]
pub struct CreateTokenResponse {
    /// 访问 Token
    pub access_token: String,
    /// Token ID (jti)
    pub token_id: String,
    /// Token 类型
    pub token_type: String,
    /// 过期时间（秒）
    pub expires_in: u64,
    /// 授权 Scope
    pub scope: String,
    /// 签发时间
    pub issued_at: u64,
    /// 过期时间戳
    pub expires_at: u64,
}

/// Token 刷新请求
#[derive(Debug, Deserialize)]
pub struct RefreshTokenRequest {
    /// 刷新 Token
    pub refresh_token: String,
}

/// Token 刷新响应
#[derive(Debug, Serialize)]
pub struct RefreshTokenResponse {
    /// 新访问 Token
    pub access_token: String,
    /// 新刷新 Token
    pub refresh_token: String,
    /// Token 类型
    pub token_type: String,
    /// 过期时间（秒）
    pub expires_in: u64,
}

/// Token 验证请求
#[derive(Debug, Deserialize)]
pub struct VerifyTokenRequest {
    /// Token
    pub token: String,
}

/// Token 验证响应
#[derive(Debug, Serialize)]
pub struct VerifyTokenResponse {
    /// 是否有效
    pub valid: bool,
    /// Token ID
    pub token_id: Option<String>,
    /// 用户 ID
    pub user_id: Option<String>,
    /// 租户 ID
    pub tenant_id: Option<String>,
    /// Scope 列表
    pub scopes: Option<Vec<String>>,
    /// 过期时间
    pub expires_at: Option<u64>,
}

/// 错误响应
#[derive(Debug, Serialize)]
pub struct AuthErrorResponse {
    pub error: String,
    pub error_description: String,
}

// ============================================================================
// API 处理器
// ============================================================================

/// 创建认证路由
pub fn auth_routes() -> Router<AuthApiState> {
    Router::new()
        // 登录
        .route("/auth/login", post(login_handler))
        // Token 刷新
        .route("/auth/refresh", post(refresh_handler))
        // Token 创建
        .route("/tokens", post(create_token_handler))
        // Token 验证
        .route("/tokens/verify", post(verify_token_handler))
        // 获取当前用户信息
        .route("/auth/me", get(current_user_handler))
}

/// 登录处理器
pub async fn login_handler(
    State(state): State<AuthApiState>,
    Json(request): Json<LoginRequest>,
) -> Response {
    // 验证用户凭据
    let user = match state.user_store.verify_user(&request.username, &request.password) {
        Some(u) => u,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(AuthErrorResponse {
                    error: "invalid_credentials".to_string(),
                    error_description: "用户名或密码错误".to_string(),
                }),
            ).into_response();
        }
    };

    // 生成 Access Token
    let access_token = match generate_paseto_token(
        &state.secret_key,
        &user.user_id,
        &user.tenant_id,
        &user.scopes,
        900, // 15 分钟
    ) {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthErrorResponse {
                    error: "token_generation_failed".to_string(),
                    error_description: format!("Token 生成失败: {}", e),
                }),
            ).into_response();
        }
    };

    // 生成 Refresh Token
    let refresh_token = match generate_refresh_token(&user.user_id, &user.tenant_id) {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthErrorResponse {
                    error: "token_generation_failed".to_string(),
                    error_description: format!("Refresh Token 生成失败: {}", e),
                }),
            ).into_response();
        }
    };

    // 构建响应
    let scope = user.scopes.iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    (
        StatusCode::OK,
        Json(LoginResponse {
            access_token,
            refresh_token,
            token_type: "Bearer".to_string(),
            expires_in: 900,
            user_id: user.user_id,
            tenant_id: user.tenant_id,
            scope,
        }),
    ).into_response()
}

/// Token 创建处理器
pub async fn create_token_handler(
    State(state): State<AuthApiState>,
    Json(request): Json<CreateTokenRequest>,
) -> Response {
    // 确定 user_id
    let user_id = request.user_id.unwrap_or_else(|| "anonymous".to_string());

    // 解析 Scope
    let scopes: Vec<TokenScope> = request.scopes
        .iter()
        .filter_map(|s| TokenScope::from_str(s))
        .collect();

    if scopes.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(AuthErrorResponse {
                error: "invalid_scope".to_string(),
                error_description: "至少需要一个有效的 scope".to_string(),
            }),
        ).into_response();
    }

    let expires_in = request.expires_in.unwrap_or(900);

    // 生成 Token
    let access_token = match generate_paseto_token(
        &state.secret_key,
        &user_id,
        "default-tenant",
        &scopes,
        expires_in,
    ) {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthErrorResponse {
                    error: "token_generation_failed".to_string(),
                    error_description: format!("Token 生成失败: {}", e),
                }),
            ).into_response();
        }
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let token_id = Uuid::now_v7().to_string();

    let scope = scopes.iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    (
        StatusCode::OK,
        Json(CreateTokenResponse {
            access_token,
            token_id,
            token_type: "Bearer".to_string(),
            expires_in,
            scope,
            issued_at: now,
            expires_at: now + expires_in,
        }),
    ).into_response()
}

/// Token 刷新处理器
pub async fn refresh_handler(
    State(state): State<AuthApiState>,
    Json(request): Json<RefreshTokenRequest>,
) -> Response {
    // 验证 Refresh Token
    let (user_id, tenant_id) = match verify_refresh_token(&request.refresh_token) {
        Ok((uid, tid)) => (uid, tid),
        Err(e) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(AuthErrorResponse {
                    error: "invalid_refresh_token".to_string(),
                    error_description: format!("Refresh Token 无效: {}", e),
                }),
            ).into_response();
        }
    };

    // 获取用户信息
    let user = match state.user_store.get_user(&user_id) {
        Some(u) => u.clone(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(AuthErrorResponse {
                    error: "user_not_found".to_string(),
                    error_description: "用户不存在".to_string(),
                }),
            ).into_response();
        }
    };

    // 生成新的 Access Token
    let access_token = match generate_paseto_token(
        &state.secret_key,
        &user.user_id,
        &user.tenant_id,
        &user.scopes,
        900,
    ) {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthErrorResponse {
                    error: "token_generation_failed".to_string(),
                    error_description: format!("Token 生成失败: {}", e),
                }),
            ).into_response();
        }
    };

    // 生成新的 Refresh Token
    let refresh_token = match generate_refresh_token(&user.user_id, &user.tenant_id) {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthErrorResponse {
                    error: "token_generation_failed".to_string(),
                    error_description: format!("Refresh Token 生成失败: {}", e),
                }),
            ).into_response();
        }
    };

    (
        StatusCode::OK,
        Json(RefreshTokenResponse {
            access_token,
            refresh_token,
            token_type: "Bearer".to_string(),
            expires_in: 900,
        }),
    ).into_response()
}

/// Token 验证处理器
pub async fn verify_token_handler(
    State(state): State<AuthApiState>,
    Json(request): Json<VerifyTokenRequest>,
) -> Response {
    match verify_paseto_token(&request.token, &state.secret_key) {
        Ok(validated) => {
            let scopes: Vec<String> = validated.scopes.iter()
                .map(|s| s.as_str().to_string())
                .collect();

            (
                StatusCode::OK,
                Json(VerifyTokenResponse {
                    valid: true,
                    token_id: Some(validated.token_id),
                    user_id: Some(validated.user_id),
                    tenant_id: Some(validated.tenant_id),
                    scopes: Some(scopes),
                    expires_at: Some(validated.expires_at),
                }),
            ).into_response()
        }
        Err(e) => {
            (
                StatusCode::OK,
                Json(VerifyTokenResponse {
                    valid: false,
                    token_id: None,
                    user_id: None,
                    tenant_id: None,
                    scopes: None,
                    expires_at: None,
                }),
            ).into_response()
        }
    }
}

/// 获取当前用户信息处理器
pub async fn current_user_handler(
    State(state): State<AuthApiState>,
) -> Response {
    // 注意：此处理器应该由认证中间件保护
    // 返回示例用户信息用于测试
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "user_id": "user-001",
            "tenant_id": "tenant-001",
            "username": "admin",
            "scopes": ["admin"]
        })),
    ).into_response()
}

// ============================================================================
// Token 辅助函数
// ============================================================================

/// 生成 PASETO Token
fn generate_paseto_token(
    secret_key: &[u8],
    user_id: &str,
    tenant_id: &str,
    scopes: &[TokenScope],
    expires_in: u64,
) -> Result<String, String> {
    use pasetors::keys::SymmetricKey;
    use pasetors::local;
    use pasetors::claims::Claims;
    use pasetors::version4::V4;
    use std::time::{SystemTime, UNIX_EPOCH, Duration};

    // 创建对称密钥
    let sk: SymmetricKey<V4> = SymmetricKey::from(secret_key)
        .map_err(|e| format!("无效的密钥：{:?}", e))?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 构建 Claims（使用 expires_in Duration）
    let mut claims = Claims::new_expires_in(&Duration::from_secs(expires_in))
        .map_err(|e| format!("创建 Claims 失败：{:?}", e))?;

    // 设置标准声明
    claims.issuer("credbridge-vault")
        .map_err(|e| format!("设置 iss 失败：{:?}", e))?;
    claims.subject(&format!("{}:{}", tenant_id, user_id))
        .map_err(|e| format!("设置 sub 失败：{:?}", e))?;
    claims.audience(tenant_id)
        .map_err(|e| format!("设置 aud 失败：{:?}", e))?;
    claims.token_identifier(&Uuid::now_v7().to_string())
        .map_err(|e| format!("设置 jti 失败：{:?}", e))?;

    // 添加自定义声明（scope）
    let scope_str = scopes.iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    claims.add_additional("scope", serde_json::json!(scope_str))
        .map_err(|e| format!("添加 scope 失败：{:?}", e))?;

    // 加密 Token
    let token = local::encrypt(&sk, &claims, None, None)
        .map_err(|e| format!("Token 加密失败：{:?}", e))?;

    Ok(token.to_string())
}


/// 验证 PASETO Token
fn verify_paseto_token(token: &str, secret_key: &[u8]) -> Result<ValidatedToken, String> {
    use pasetors::keys::SymmetricKey;
    use pasetors::local;
    use pasetors::token::UntrustedToken;
    use pasetors::claims::ClaimsValidationRules;
    use time::OffsetDateTime;

    // 创建对称密钥
    let sk: SymmetricKey<_> = SymmetricKey::from(secret_key)
        .map_err(|e| format!("无效的密钥: {:?}", e))?;

    // 解析 Token
    let untrusted = UntrustedToken::try_from(token)
        .map_err(|e| format!("Token 解析失败: {:?}", e))?;

    // 解密验证（使用默认验证规则自动验证 exp）
    let validation_rules = ClaimsValidationRules::new();
    let trusted = local::decrypt(&sk, &untrusted, &validation_rules, None, None)
        .map_err(|e| format!("Token 验证失败: {:?}", e))?;

    // 提取 Claims
    let claims = trusted.payload_claims()
        .ok_or("Token 缺少 payload claims")?;

    let token_id = claims.get_claim("jti")
        .and_then(|v| v.as_str())
        .ok_or("Token 缺少 jti")?
        .to_string();

    let subject = claims.get_claim("sub")
        .and_then(|v| v.as_str())
        .ok_or("Token 缺少 sub")?
        .to_string();

    // 解析 exp（ISO 8601 格式）
    let expires_at = match claims.get_claim("exp").and_then(|v| v.as_str()) {
        Some(s) => {
            OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
                .map(|dt| dt.unix_timestamp() as u64)
                .map_err(|_| "无法解析 exp 时间")
        }
        None => Err("Token 缺少 exp"),
    }?;

    let issued_at = claims.get_claim("iat")
        .and_then(|v| v.as_str())
        .and_then(|s| {
            OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
                .map(|dt| dt.unix_timestamp() as u64)
                .ok()
        })
        .unwrap_or(now_timestamp());

    let scope_str = claims.get_claim("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let scopes: Vec<TokenScope> = scope_str
        .split_whitespace()
        .filter_map(TokenScope::from_str)
        .collect();

    // 解析 tenant_id 和 user_id
    let parts: Vec<&str> = subject.split(':').collect();
    let (tenant_id, user_id) = if parts.len() == 2 {
        (parts[0].to_string(), parts[1].to_string())
    } else {
        ("unknown".to_string(), subject.clone())
    };

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

/// 生成 Refresh Token
fn generate_refresh_token(user_id: &str, tenant_id: &str) -> Result<String, String> {
    // 简单实现：使用 UUID 作为 Refresh Token
    // 生产环境应使用更安全的方式
    let token = format!("rt_{}_{}_{}", tenant_id, user_id, Uuid::now_v7());
    Ok(token)
}

/// 验证 Refresh Token
fn verify_refresh_token(token: &str) -> Result<(String, String), String> {
    // 简单实现：解析 Refresh Token 格式
    let parts: Vec<&str> = token.split('_').collect();
    if parts.len() < 4 || parts[0] != "rt" {
        return Err("无效的 Refresh Token 格式".to_string());
    }

    Ok((parts[2].to_string(), parts[1].to_string()))
}

/// 获取当前时间戳
fn now_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn get_test_key() -> Vec<u8> {
        vec![0u8; 32]
    }

    fn get_test_state() -> AuthApiState {
        AuthApiState {
            secret_key: get_test_key(),
            user_store: Arc::new(MemoryUserStore::new()),
        }
    }

    #[test]
    fn test_user_verification() {
        let store = MemoryUserStore::new();

        // 正确凭据
        let user = store.verify_user("admin", "admin123");
        assert!(user.is_some());
        let user = user.unwrap();
        assert_eq!(user.user_id, "user-001");

        // 错误凭据
        let user = store.verify_user("admin", "wrong");
        assert!(user.is_none());
    }

    #[test]
    fn test_token_generation() {
        let key = get_test_key();
        let scopes = vec![TokenScope::CredentialRead];

        let token = generate_paseto_token(&key, "user-001", "tenant-001", &scopes, 900);
        assert!(token.is_ok());

        let token = token.unwrap();
        assert!(token.starts_with("v4.local."));
    }

    #[test]
    fn test_token_verification() {
        let key = get_test_key();
        let scopes = vec![TokenScope::CredentialRead, TokenScope::CredentialDecrypt];

        let token = generate_paseto_token(&key, "user-001", "tenant-001", &scopes, 900)
            .unwrap();

        let validated = verify_paseto_token(&token, &key);
        if let Err(ref e) = validated {
            eprintln!("Token verification error: {}", e);
        }
        assert!(validated.is_ok(), "Token verification should succeed");

        let validated = validated.unwrap();
        assert_eq!(validated.user_id, "user-001");
        assert_eq!(validated.tenant_id, "tenant-001");
        assert_eq!(validated.scopes.len(), 2);
    }

    #[test]
    fn test_invalid_token() {
        let key = get_test_key();

        let result = verify_paseto_token("invalid_token", &key);
        assert!(result.is_err());
    }

    #[test]
    fn test_refresh_token() {
        let refresh_token = generate_refresh_token("user-001", "tenant-001").unwrap();

        let (user_id, tenant_id) = verify_refresh_token(&refresh_token).unwrap();
        assert_eq!(user_id, "user-001");
        assert_eq!(tenant_id, "tenant-001");
    }
}