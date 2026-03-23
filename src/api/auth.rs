//! 认证 API 模块
//!
//! 提供登录、Token 签发和刷新功能
//! - POST /api/v1/auth/login - 用户登录
//! - POST /api/v1/auth/refresh - Token 刷新
//! - POST /api/v1/tokens - 创建新 Token

use axum::{
    Extension, Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use bcrypt::{DEFAULT_COST, hash, verify};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::audit::{AuditAction, AuditEntry, MemoryAuditStorage, Outcome, RedactedParam};

use super::i18n::{I18nParams, ResolvedLocale, invalid_locale_response, normalize_locale};
use super::middleware::{TokenScope, ValidatedToken};
use super::response::{ApiErrorResponse, ErrorCode};

/// 认证 API 状态
#[derive(Clone)]
pub struct AuthApiState {
    /// Token 密钥（32 字节）
    pub secret_key: Vec<u8>,
    /// 用户存储（内存模拟）
    pub user_store: Arc<MemoryUserStore>,
    /// 共享审计存储
    pub audit_storage: Option<Arc<tokio::sync::Mutex<MemoryAuditStorage>>>,
    /// 已签发 Token 状态
    issued_tokens: Arc<RwLock<HashMap<String, IssuedTokenRecord>>>,
}

#[derive(Debug, Clone)]
struct IssuedTokenRecord {
    tenant_id: String,
    expires_at: u64,
}

/// 内存用户存储
#[derive(Debug, Clone)]
pub struct MemoryUserStore {
    users: Arc<RwLock<HashMap<String, UserInfo>>>,
}

/// 用户信息
#[derive(Debug, Clone)]
pub struct UserInfo {
    pub username: String,
    pub user_id: String,
    pub tenant_id: String,
    pub password_hash: String,
    pub scopes: Vec<TokenScope>,
    pub locale: Option<String>,
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

        // 使用 bcrypt 哈希密码（在编译时生成哈希值）
        // admin123 的 bcrypt 哈希
        let admin_password_hash =
            hash("admin123", DEFAULT_COST).expect("Failed to hash admin password");
        // user123 的 bcrypt 哈希
        let user_password_hash =
            hash("user123", DEFAULT_COST).expect("Failed to hash user password");

        // 添加默认测试用户
        users.insert(
            "admin".to_string(),
            UserInfo {
                username: "admin".to_string(),
                user_id: "user-001".to_string(),
                tenant_id: "tenant-001".to_string(),
                password_hash: admin_password_hash,
                scopes: vec![TokenScope::Admin],
                locale: Some("en-US".to_string()),
            },
        );

        users.insert(
            "user".to_string(),
            UserInfo {
                username: "user".to_string(),
                user_id: "user-002".to_string(),
                tenant_id: "tenant-001".to_string(),
                password_hash: user_password_hash,
                scopes: vec![TokenScope::CredentialRead, TokenScope::CredentialDecrypt],
                locale: None,
            },
        );

        Self {
            users: Arc::new(RwLock::new(users)),
        }
    }
}

impl Default for AuthApiState {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthApiState {
    /// 创建认证 API 状态
    pub fn new() -> Self {
        use rand::RngCore;
        use rand::rngs::OsRng;

        // 生成随机密钥（使用密码学安全的 OsRng）
        let mut secret_key = vec![0u8; 32];
        OsRng.fill_bytes(&mut secret_key);

        Self {
            secret_key,
            user_store: Arc::new(MemoryUserStore::new()),
            audit_storage: None,
            issued_tokens: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 创建带共享审计存储的认证 API 状态
    pub fn with_audit_storage(audit_storage: Arc<tokio::sync::Mutex<MemoryAuditStorage>>) -> Self {
        let mut state = Self::new();
        state.audit_storage = Some(audit_storage);
        state
    }

    async fn record_token_validation_audit(&self, validated: &ValidatedToken) {
        let Some(storage) = &self.audit_storage else {
            return;
        };

        let entry = AuditEntry::new(
            crate::audit::events::hash_user_id(&validated.user_id),
            "session",
            "auth",
            AuditAction::TokenValidate,
            Outcome::Success,
            "software_mode",
            validated.token_id.clone(),
        )
        .with_param(
            "verified_token_id",
            RedactedParam::Plain(validated.token_id.clone()),
        )
        .with_param(
            "tenant_id",
            RedactedParam::Plain(validated.tenant_id.clone()),
        );

        if let Err(error) = storage.lock().await.record(entry) {
            log::warn!("[AUDIT] Token validation audit record failed: {error:?}");
        }
    }

    async fn record_token_issue_audit(&self, validated: &ValidatedToken) {
        let Some(storage) = &self.audit_storage else {
            return;
        };

        let entry = AuditEntry::new(
            crate::audit::events::hash_user_id(&validated.user_id),
            "session",
            "auth",
            AuditAction::TokenIssue,
            Outcome::Success,
            "software_mode",
            validated.token_id.clone(),
        )
        .with_param(
            "issued_token_id",
            RedactedParam::Plain(validated.token_id.clone()),
        )
        .with_param(
            "tenant_id",
            RedactedParam::Plain(validated.tenant_id.clone()),
        )
        .with_param(
            "expires_at",
            RedactedParam::Plain(validated.expires_at.to_string()),
        );

        if let Err(error) = storage.lock().await.record(entry) {
            log::warn!("[AUDIT] Token issue audit record failed: {error:?}");
        }
    }

    async fn register_issued_token(&self, validated: &ValidatedToken) {
        self.issued_tokens.write().await.insert(
            validated.token_id.clone(),
            IssuedTokenRecord {
                tenant_id: validated.tenant_id.clone(),
                expires_at: validated.expires_at,
            },
        );
    }

    async fn count_active_tokens_for_tenant(&self, tenant_id: &str) -> u64 {
        let now = now_timestamp();
        let mut issued_tokens = self.issued_tokens.write().await;
        issued_tokens.retain(|_, token| token.expires_at > now);

        issued_tokens
            .values()
            .filter(|token| token.tenant_id == tenant_id)
            .count() as u64
    }
}

impl MemoryUserStore {
    /// 验证用户凭据
    pub async fn verify_user(&self, username: &str, password: &str) -> Option<UserInfo> {
        self.users.read().await.get(username).and_then(|user| {
            match verify(password, &user.password_hash) {
                Ok(true) => Some(user.clone()),
                Ok(false) => None,
                Err(_) => None,
            }
        })
    }

    /// 获取用户信息
    pub async fn get_user(&self, user_id: &str) -> Option<UserInfo> {
        self.users
            .read()
            .await
            .values()
            .find(|u| u.user_id == user_id)
            .cloned()
    }

    /// 更新用户 locale 偏好
    pub async fn update_locale(&self, user_id: &str, locale: Option<String>) -> Option<UserInfo> {
        let mut users = self.users.write().await;
        let username = users
            .iter()
            .find(|(_, user)| user.user_id == user_id)
            .map(|(username, _)| username.clone())?;
        let user = users.get_mut(&username)?;
        user.locale = locale;
        Some(user.clone())
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

/// 用户信息（用于登录响应）
#[derive(Debug, Serialize)]
pub struct UserInfoResponse {
    pub id: String,
    pub username: String,
    pub email: String,
    pub role: String,
    pub tenant_id: String,
    pub mfa_enabled: bool,
    pub locale: String,
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
    /// 用户信息
    pub user: UserInfoResponse,
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
#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenStatsResponse {
    pub active_tokens: u64,
}

/// 登录错误响应
#[derive(Debug, Serialize)]
pub struct AuthErrorResponse {
    pub error: String,
    pub message: String,
    pub error_description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub i18n: Option<super::i18n::I18nMetadata>,
    pub locale: String,
}

#[derive(Debug, Serialize)]
pub struct UserPreferencesResponse {
    pub locale: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserPreferencesRequest {
    pub locale: String,
}

// ============================================================================
// API 处理器
// ============================================================================

/// 创建公开认证路由
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
}

/// 创建受保护的认证路由
pub fn protected_auth_routes() -> Router<AuthApiState> {
    Router::new()
        .route("/auth/me", get(current_user_handler))
        .route("/tokens/stats", get(token_stats_handler))
        .route("/users/me/preferences", get(get_user_preferences_handler))
        .route(
            "/users/me/preferences",
            patch(update_user_preferences_handler),
        )
}

fn auth_error_response(
    status: StatusCode,
    error: &str,
    locale: &ResolvedLocale,
    key: &str,
    params: I18nParams,
) -> Response {
    let message = super::i18n::translate(locale.as_str(), key, &params);
    let payload = AuthErrorResponse {
        error: error.to_string(),
        message: message.clone(),
        error_description: message,
        i18n: Some(super::i18n::I18nMetadata::new(key).with_params(params)),
        locale: locale.as_str().to_string(),
    };

    let mut response = (status, Json(payload)).into_response();
    super::i18n::set_content_language(response.headers_mut(), locale.as_str());
    response
}

/// 登录处理器
pub async fn login_handler(
    State(state): State<AuthApiState>,
    locale: ResolvedLocale,
    Json(request): Json<LoginRequest>,
) -> Response {
    // 验证用户凭据
    let user = match state
        .user_store
        .verify_user(&request.username, &request.password)
        .await
    {
        Some(u) => u,
        None => {
            return auth_error_response(
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                &locale,
                "errors.auth.invalid_credentials",
                I18nParams::new(),
            );
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
            let mut params = I18nParams::new();
            params.insert("reason".to_string(), Value::String(e));
            return auth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "token_generation_failed",
                &locale,
                "errors.auth.token_generation_failed",
                params,
            );
        }
    };

    // 生成 Refresh Token
    let refresh_token = match generate_refresh_token(&user.user_id, &user.tenant_id) {
        Ok(t) => t,
        Err(e) => {
            let mut params = I18nParams::new();
            params.insert("reason".to_string(), Value::String(e));
            return auth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "token_generation_failed",
                &locale,
                "errors.auth.refresh_token_generation_failed",
                params,
            );
        }
    };

    // 构建用户信息
    let user_response = UserInfoResponse {
        id: user.user_id.clone(),
        username: user.username.clone(),
        email: format!("{}@credbridge.local", user.username),
        role: if user.scopes.contains(&TokenScope::Admin) {
            "admin".to_string()
        } else {
            "user".to_string()
        },
        tenant_id: user.tenant_id.clone(),
        mfa_enabled: false,
        locale: user
            .locale
            .unwrap_or_else(|| normalize_locale(locale.as_str()).to_string()),
    };

    // 构建响应
    let mut response = (
        StatusCode::OK,
        Json(LoginResponse {
            access_token,
            refresh_token,
            token_type: "Bearer".to_string(),
            expires_in: 900,
            user: user_response,
        }),
    )
        .into_response();
    super::i18n::set_content_language(response.headers_mut(), locale.as_str());
    response
}

/// Token 创建处理器
pub async fn create_token_handler(
    State(state): State<AuthApiState>,
    locale: ResolvedLocale,
    Json(request): Json<CreateTokenRequest>,
) -> Response {
    // 确定 user_id
    let user_id = request.user_id.unwrap_or_else(|| "anonymous".to_string());

    // 解析 Scope
    let scopes: Vec<TokenScope> = request
        .scopes
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();

    if scopes.is_empty() {
        return auth_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_scope",
            &locale,
            "errors.auth.invalid_scope",
            I18nParams::new(),
        );
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
            let mut params = I18nParams::new();
            params.insert("reason".to_string(), Value::String(e));
            return auth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "token_generation_failed",
                &locale,
                "errors.auth.token_generation_failed",
                params,
            );
        }
    };

    let validated = match verify_paseto_token(&access_token, &state.secret_key, locale.as_str()) {
        Ok(validated) => validated,
        Err(e) => {
            let mut params = I18nParams::new();
            params.insert("reason".to_string(), Value::String(e));
            return auth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "token_generation_failed",
                &locale,
                "errors.auth.token_generation_failed",
                params,
            );
        }
    };

    state.register_issued_token(&validated).await;
    state.record_token_issue_audit(&validated).await;

    let scope = scopes
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    let mut response = (
        StatusCode::OK,
        Json(CreateTokenResponse {
            access_token,
            token_id: validated.token_id,
            token_type: "Bearer".to_string(),
            expires_in,
            scope,
            issued_at: validated.issued_at,
            expires_at: validated.expires_at,
        }),
    )
        .into_response();
    super::i18n::set_content_language(response.headers_mut(), locale.as_str());
    response
}

/// Token 刷新处理器
pub async fn refresh_handler(
    State(state): State<AuthApiState>,
    locale: ResolvedLocale,
    Json(request): Json<RefreshTokenRequest>,
) -> Response {
    // 验证 Refresh Token
    let (user_id, _tenant_id) = match verify_refresh_token(&request.refresh_token) {
        Ok((uid, tid)) => (uid, tid),
        Err(e) => {
            let mut params = I18nParams::new();
            params.insert("reason".to_string(), Value::String(e));
            return auth_error_response(
                StatusCode::UNAUTHORIZED,
                "invalid_refresh_token",
                &locale,
                "errors.auth.invalid_refresh_token",
                params,
            );
        }
    };

    // 获取用户信息
    let user = match state.user_store.get_user(&user_id).await {
        Some(u) => u,
        None => {
            return auth_error_response(
                StatusCode::UNAUTHORIZED,
                "user_not_found",
                &locale,
                "errors.auth.user_not_found",
                I18nParams::new(),
            );
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
            let mut params = I18nParams::new();
            params.insert("reason".to_string(), Value::String(e));
            return auth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "token_generation_failed",
                &locale,
                "errors.auth.token_generation_failed",
                params,
            );
        }
    };

    // 生成新的 Refresh Token
    let refresh_token = match generate_refresh_token(&user.user_id, &user.tenant_id) {
        Ok(t) => t,
        Err(e) => {
            let mut params = I18nParams::new();
            params.insert("reason".to_string(), Value::String(e));
            return auth_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "token_generation_failed",
                &locale,
                "errors.auth.refresh_token_generation_failed",
                params,
            );
        }
    };

    let mut response = (
        StatusCode::OK,
        Json(RefreshTokenResponse {
            access_token,
            refresh_token,
            token_type: "Bearer".to_string(),
            expires_in: 900,
        }),
    )
        .into_response();
    super::i18n::set_content_language(response.headers_mut(), locale.as_str());
    response
}

/// Token 验证处理器
pub async fn verify_token_handler(
    State(state): State<AuthApiState>,
    locale: ResolvedLocale,
    Json(request): Json<VerifyTokenRequest>,
) -> Response {
    let mut response = match verify_paseto_token(&request.token, &state.secret_key, locale.as_str())
    {
        Ok(validated) => {
            state.record_token_validation_audit(&validated).await;
            let scopes: Vec<String> = validated
                .scopes
                .iter()
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
            )
                .into_response()
        }
        Err(_e) => (
            StatusCode::OK,
            Json(VerifyTokenResponse {
                valid: false,
                token_id: None,
                user_id: None,
                tenant_id: None,
                scopes: None,
                expires_at: None,
            }),
        )
            .into_response(),
    };
    super::i18n::set_content_language(response.headers_mut(), locale.as_str());
    response
}

pub async fn token_stats_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    locale: ResolvedLocale,
) -> Response {
    let active_tokens = state.count_active_tokens_for_tenant(&token.tenant_id).await;
    let mut response = (
        StatusCode::OK,
        Json(TokenStatsResponse { active_tokens }),
    )
        .into_response();
    super::i18n::set_content_language(response.headers_mut(), locale.as_str());
    response
}

/// 获取当前用户信息处理器
pub async fn current_user_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    locale: ResolvedLocale,
) -> Response {
    let Some(user) = state.user_store.get_user(&token.user_id).await else {
        return ApiErrorResponse::localized(
            ErrorCode::Unauthorized,
            locale.as_str(),
            "errors.auth.user_not_found",
            I18nParams::new(),
        )
        .into_response();
    };

    let mut response = (
        StatusCode::OK,
        Json(serde_json::json!({
            "user_id": user.user_id,
            "tenant_id": user.tenant_id,
            "username": user.username,
            "scopes": user.scopes.iter().map(|scope| scope.as_str()).collect::<Vec<_>>(),
            "locale": user.locale.unwrap_or_else(|| normalize_locale(locale.as_str()).to_string()),
        })),
    )
        .into_response();
    super::i18n::set_content_language(response.headers_mut(), locale.as_str());
    response
}

pub async fn get_user_preferences_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    locale: ResolvedLocale,
) -> Response {
    let Some(user) = state.user_store.get_user(&token.user_id).await else {
        return ApiErrorResponse::localized(
            ErrorCode::Unauthorized,
            locale.as_str(),
            "errors.auth.user_not_found",
            I18nParams::new(),
        )
        .into_response();
    };

    let mut response = (
        StatusCode::OK,
        Json(UserPreferencesResponse {
            locale: user
                .locale
                .unwrap_or_else(|| normalize_locale(locale.as_str()).to_string()),
        }),
    )
        .into_response();
    super::i18n::set_content_language(response.headers_mut(), locale.as_str());
    response
}

pub async fn update_user_preferences_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    locale: ResolvedLocale,
    Json(request): Json<UpdateUserPreferencesRequest>,
) -> Response {
    let normalized = normalize_locale(&request.locale).to_string();
    if normalized != request.locale {
        return invalid_locale_response(locale.as_str(), &request.locale);
    }

    let Some(user) = state
        .user_store
        .update_locale(&token.user_id, Some(normalized.clone()))
        .await
    else {
        return ApiErrorResponse::localized(
            ErrorCode::Unauthorized,
            locale.as_str(),
            "errors.auth.user_not_found",
            I18nParams::new(),
        )
        .into_response();
    };

    let mut response = (
        StatusCode::OK,
        Json(UserPreferencesResponse {
            locale: user.locale.unwrap_or(normalized),
        }),
    )
        .into_response();
    super::i18n::set_content_language(response.headers_mut(), locale.as_str());
    response
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
    use pasetors::claims::Claims;
    use pasetors::keys::SymmetricKey;
    use pasetors::local;
    use pasetors::version4::V4;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    // 创建对称密钥
    let sk: SymmetricKey<V4> =
        SymmetricKey::from(secret_key).map_err(|e| format!("无效的密钥：{e:?}"))?;

    let _now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time before Unix epoch")
        .as_secs();

    // 构建 Claims（使用 expires_in Duration）
    let mut claims = Claims::new_expires_in(&Duration::from_secs(expires_in))
        .map_err(|e| format!("创建 Claims 失败：{e:?}"))?;

    // 设置标准声明
    claims
        .issuer("credbridge-vault")
        .map_err(|e| format!("设置 iss 失败：{e:?}"))?;
    claims
        .subject(&format!("{tenant_id}:{user_id}"))
        .map_err(|e| format!("设置 sub 失败：{e:?}"))?;
    claims
        .audience(tenant_id)
        .map_err(|e| format!("设置 aud 失败：{e:?}"))?;
    claims
        .token_identifier(&Uuid::now_v7().to_string())
        .map_err(|e| format!("设置 jti 失败：{e:?}"))?;

    // 添加自定义声明（scope）
    let scope_str = scopes
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    claims
        .add_additional("scope", serde_json::json!(scope_str))
        .map_err(|e| format!("添加 scope 失败：{e:?}"))?;

    // 加密 Token
    let token =
        local::encrypt(&sk, &claims, None, None).map_err(|e| format!("Token 加密失败：{e:?}"))?;

    Ok(token.to_string())
}

/// 验证 PASETO Token
fn verify_paseto_token(
    token: &str,
    secret_key: &[u8],
    _locale: &str,
) -> Result<ValidatedToken, String> {
    use pasetors::claims::ClaimsValidationRules;
    use pasetors::keys::SymmetricKey;
    use pasetors::local;
    use pasetors::token::UntrustedToken;
    use time::OffsetDateTime;

    // 创建对称密钥
    let sk: SymmetricKey<_> =
        SymmetricKey::from(secret_key).map_err(|e| format!("无效的密钥: {e:?}"))?;

    // 解析 Token
    let untrusted =
        UntrustedToken::try_from(token).map_err(|e| format!("Token 解析失败: {e:?}"))?;

    // 解密验证（使用默认验证规则自动验证 exp）
    let validation_rules = ClaimsValidationRules::new();
    let trusted = local::decrypt(&sk, &untrusted, &validation_rules, None, None)
        .map_err(|e| format!("Token 验证失败: {e:?}"))?;

    // 提取 Claims
    let claims = trusted
        .payload_claims()
        .ok_or("Token 缺少 payload claims")?;

    let token_id = claims
        .get_claim("jti")
        .and_then(|v| v.as_str())
        .ok_or("Token 缺少 jti")?
        .to_string();

    let subject = claims
        .get_claim("sub")
        .and_then(|v| v.as_str())
        .ok_or("Token 缺少 sub")?
        .to_string();

    // 解析 exp（ISO 8601 格式）
    let expires_at = match claims.get_claim("exp").and_then(|v| v.as_str()) {
        Some(s) => OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
            .map(|dt| dt.unix_timestamp() as u64)
            .map_err(|_| "无法解析 exp 时间"),
        None => Err("Token 缺少 exp"),
    }?;

    let issued_at = claims
        .get_claim("iat")
        .and_then(|v| v.as_str())
        .and_then(|s| {
            OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
                .map(|dt| dt.unix_timestamp() as u64)
                .ok()
        })
        .unwrap_or(now_timestamp());

    let scope_str = claims
        .get_claim("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let scopes: Vec<TokenScope> = scope_str
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
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
        .expect("System time before Unix epoch")
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

    #[allow(dead_code)]
    fn get_test_state() -> AuthApiState {
        AuthApiState {
            secret_key: get_test_key(),
            user_store: Arc::new(MemoryUserStore::new()),
            audit_storage: None,
            issued_tokens: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    #[tokio::test]
    async fn test_token_stats_handler_counts_only_unexpired_tokens_for_current_tenant() {
        let state = get_test_state();
        let now = now_timestamp();

        state
            .issued_tokens
            .write()
            .await
            .insert(
                "token-active".to_string(),
                IssuedTokenRecord {
                    tenant_id: "tenant-001".to_string(),
                    expires_at: now + 300,
                },
            );
        state
            .issued_tokens
            .write()
            .await
            .insert(
                "token-expired".to_string(),
                IssuedTokenRecord {
                    tenant_id: "tenant-001".to_string(),
                    expires_at: now.saturating_sub(1),
                },
            );
        state
            .issued_tokens
            .write()
            .await
            .insert(
                "token-other-tenant".to_string(),
                IssuedTokenRecord {
                    tenant_id: "tenant-002".to_string(),
                    expires_at: now + 300,
                },
            );

        let response = token_stats_handler(
            State(state.clone()),
            Extension(ValidatedToken {
                token_id: "viewer-token".to_string(),
                subject: "tenant-001:user-001".to_string(),
                tenant_id: "tenant-001".to_string(),
                user_id: "user-001".to_string(),
                expires_at: now + 300,
                scopes: vec![TokenScope::Admin],
                issued_at: now,
            }),
            ResolvedLocale::default(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let stats: TokenStatsResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(stats.active_tokens, 1);

        let issued_tokens = state.issued_tokens.read().await;
        assert!(!issued_tokens.contains_key("token-expired"));
    }

    #[tokio::test]
    async fn test_create_token_handler_registers_token_and_records_issue_audit() {
        let audit_storage = Arc::new(tokio::sync::Mutex::new(
            MemoryAuditStorage::new(16).unwrap(),
        ));
        let state = AuthApiState::with_audit_storage(audit_storage.clone());

        let response = create_token_handler(
            State(state.clone()),
            ResolvedLocale::default(),
            Json(CreateTokenRequest {
                user_id: Some("user-001".to_string()),
                scopes: vec!["audit:read".to_string()],
                expires_in: Some(900),
                credential_ids: None,
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let created: CreateTokenResponse = serde_json::from_slice(&body).unwrap();

        let active_tokens = state.count_active_tokens_for_tenant("default-tenant").await;
        assert_eq!(active_tokens, 1);
        let issued_tokens = state.issued_tokens.read().await;
        let stored = issued_tokens.get(&created.token_id).unwrap();
        assert_eq!(stored.expires_at, created.expires_at);

        let entries = audit_storage.lock().await.query_recent(10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].entry.action, AuditAction::TokenIssue);
        assert_eq!(entries[0].entry.outcome, Outcome::Success);
    }

    #[tokio::test]
    async fn test_user_verification() {
        let store = MemoryUserStore::new();

        // 正确凭据
        let user = store.verify_user("admin", "admin123").await;
        assert!(user.is_some());
        let user = user.unwrap();
        assert_eq!(user.user_id, "user-001");
        assert_eq!(user.locale.as_deref(), Some("en-US"));

        // 错误凭据
        let user = store.verify_user("admin", "wrong").await;
        assert!(user.is_none());
    }

    #[tokio::test]
    async fn test_verify_token_handler_records_audit_on_success() {
        let audit_storage = Arc::new(tokio::sync::Mutex::new(
            MemoryAuditStorage::new(16).unwrap(),
        ));
        let state = AuthApiState::with_audit_storage(audit_storage.clone());
        let token = generate_paseto_token(
            &state.secret_key,
            "user-001",
            "tenant-001",
            &[TokenScope::AuditRead],
            900,
        )
        .unwrap();

        let response = verify_token_handler(
            State(state),
            ResolvedLocale::default(),
            Json(VerifyTokenRequest { token }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);

        let entries = audit_storage.lock().await.query_recent(10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].entry.action, AuditAction::TokenValidate);
        assert_eq!(entries[0].entry.outcome, Outcome::Success);
    }

    #[tokio::test]
    async fn test_verify_token_handler_does_not_record_audit_on_failure() {
        let audit_storage = Arc::new(tokio::sync::Mutex::new(
            MemoryAuditStorage::new(16).unwrap(),
        ));
        let state = AuthApiState::with_audit_storage(audit_storage.clone());

        let response = verify_token_handler(
            State(state),
            ResolvedLocale::default(),
            Json(VerifyTokenRequest {
                token: "invalid-token".to_string(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let entries = audit_storage.lock().await.query_recent(10).unwrap();
        assert!(entries.is_empty());
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

        let token = generate_paseto_token(&key, "user-001", "tenant-001", &scopes, 900).unwrap();

        let validated = verify_paseto_token(&token, &key, "zh-CN");
        if let Err(ref e) = validated {
            eprintln!("Token verification error: {e}");
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

        let result = verify_paseto_token("invalid_token", &key, "zh-CN");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_user_locale() {
        let store = MemoryUserStore::new();
        let updated = store
            .update_locale("user-002", Some("zh-CN".to_string()))
            .await
            .unwrap();

        assert_eq!(updated.locale.as_deref(), Some("zh-CN"));
    }

    #[test]
    fn test_refresh_token() {
        let refresh_token = generate_refresh_token("user-001", "tenant-001").unwrap();

        let (user_id, tenant_id) = verify_refresh_token(&refresh_token).unwrap();
        assert_eq!(user_id, "user-001");
        assert_eq!(tenant_id, "tenant-001");
    }
}
