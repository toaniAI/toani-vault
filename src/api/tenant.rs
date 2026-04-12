//! 租户管理 API
//!
//! 提供租户配置管理、租户生命周期管理的 RESTful API。
//!
//! # API 端点
//!
//! - `POST /api/v1/tenants` - 创建租户
//! - `GET /api/v1/tenants/:id` - 获取租户信息
//! - `GET /api/v1/tenants/:id/config` - 获取租户配置
//! - `PUT /api/v1/tenants/:id/config` - 更新租户配置
//! - `POST /api/v1/tenants/:id/activate` - 激活租户
//! - `POST /api/v1/tenants/:id/suspend` - 暂停租户
//! - `DELETE /api/v1/tenants/:id` - 删除租户
//! - `GET /api/v1/tenants` - 列出租户（管理员）
//!
//! # 权限控制
//!
//! - 创建租户: `admin` scope
//! - 查看/更新租户配置: `admin` 或租户成员
//! - 激活/暂停/删除租户: `admin` scope

use axum::{
    Extension, Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::tenant::{
    CreateTenantRequest, FeatureFlags, PartialTenantConfig, QuotaLimits, Tenant, TenantConfig,
    TenantConfigError, TenantConfigStore, TenantId as TenantIdType, TenantManager, TenantService,
    TenantSettings,
};
use crate::{api::middleware::ValidatedToken, auth::AuthService};

/// API 状态
#[derive(Clone)]
pub struct TenantApiState<S: TenantConfigStore + Clone + Send + Sync + 'static> {
    pub tenant_manager: Arc<TenantManager<S>>,
    pub tenant_service: Arc<dyn TenantService>,
    pub auth_service: Option<Arc<dyn AuthService>>,
}

impl<S: TenantConfigStore + Clone + Send + Sync + 'static> TenantApiState<S> {
    pub fn new(tenant_manager: TenantManager<S>, tenant_service: Arc<dyn TenantService>) -> Self {
        Self {
            tenant_manager: Arc::new(tenant_manager),
            tenant_service,
            auth_service: None,
        }
    }
}

/// 创建租户响应
#[derive(Debug, Serialize)]
pub struct CreateTenantResponse {
    pub success: bool,
    pub data: TenantData,
    pub initialization: Vec<InitializationStepDto>,
    pub meta: ResponseMeta,
}

#[derive(Debug, Serialize)]
pub struct GetTenantResponse {
    pub success: bool,
    pub data: TenantData,
    pub meta: ResponseMeta,
}

/// 租户数据 DTO
#[derive(Debug, Serialize)]
pub struct TenantData {
    pub id: String,
    pub name: String,
    pub status: String,
    pub tier: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&Tenant> for TenantData {
    fn from(tenant: &Tenant) -> Self {
        Self {
            id: tenant.id.to_string(),
            name: tenant.name.clone(),
            status: tenant.status.to_string(),
            tier: get_tier_from_config(&tenant.config),
            created_at: tenant.created_at.to_rfc3339(),
            updated_at: tenant.updated_at.to_rfc3339(),
        }
    }
}

/// 初始化步骤 DTO
#[derive(Debug, Serialize)]
pub struct InitializationStepDto {
    pub step: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 响应元数据
#[derive(Debug, Serialize)]
pub struct ResponseMeta {
    pub request_id: String,
    pub timestamp: String,
}

/// 租户配置响应
#[derive(Debug, Serialize)]
pub struct TenantConfigResponse {
    pub success: bool,
    pub data: TenantConfigData,
    pub meta: ResponseMeta,
}

/// 租户配置数据
#[derive(Debug, Serialize)]
pub struct TenantConfigData {
    pub tenant_id: String,
    pub tier: String,
    pub max_credentials: u64,
    pub max_sandbox_sessions: u64,
    pub feature_flags: FeatureFlagsDto,
    pub quota_limits: QuotaLimitsDto,
    pub settings: TenantSettingsDto,
    pub version: u64,
    pub updated_at: Option<String>,
    pub updated_by: Option<String>,
}

/// 功能开关 DTO
#[derive(Debug, Serialize)]
pub struct FeatureFlagsDto {
    pub enable_credential_encryption: bool,
    pub enable_audit_logging: bool,
    pub enable_token_revocation: bool,
    pub enable_mfa: bool,
    pub enable_remote_attestation: bool,
    pub enable_auto_rotation: bool,
    pub allow_cors: bool,
    pub enable_ip_whitelist: bool,
    pub enable_webhooks: bool,
    pub enable_sso: bool,
    pub enable_custom_crypto: bool,
    pub enable_advanced_audit: bool,
}

impl From<&FeatureFlags> for FeatureFlagsDto {
    fn from(flags: &FeatureFlags) -> Self {
        Self {
            enable_credential_encryption: flags.enable_credential_encryption,
            enable_audit_logging: flags.enable_audit_logging,
            enable_token_revocation: flags.enable_token_revocation,
            enable_mfa: flags.enable_mfa,
            enable_remote_attestation: flags.enable_remote_attestation,
            enable_auto_rotation: flags.enable_auto_rotation,
            allow_cors: flags.allow_cors,
            enable_ip_whitelist: flags.enable_ip_whitelist,
            enable_webhooks: flags.enable_webhooks,
            enable_sso: flags.enable_sso,
            enable_custom_crypto: flags.enable_custom_crypto,
            enable_advanced_audit: flags.enable_advanced_audit,
        }
    }
}

/// 配额限制 DTO
#[derive(Debug, Serialize)]
pub struct QuotaLimitsDto {
    pub max_credentials: u64,
    pub max_tokens_per_user: u64,
    pub max_requests_per_minute: u64,
    pub max_users: u64,
    pub max_connectors: u64,
    pub max_webhooks: u64,
    pub storage_quota_mb: u64,
    pub audit_retention_days: u64,
    pub max_token_ttl_seconds: u64,
    pub max_batch_size: u64,
    pub max_sandbox_sessions: u64,
}

impl From<&QuotaLimits> for QuotaLimitsDto {
    fn from(limits: &QuotaLimits) -> Self {
        Self {
            max_credentials: limits.max_credentials,
            max_tokens_per_user: limits.max_tokens_per_user,
            max_requests_per_minute: limits.max_requests_per_minute,
            max_users: limits.max_users,
            max_connectors: limits.max_connectors,
            max_webhooks: limits.max_webhooks,
            storage_quota_mb: limits.storage_quota_mb,
            audit_retention_days: limits.audit_retention_days,
            max_token_ttl_seconds: limits.max_token_ttl_seconds,
            max_batch_size: limits.max_batch_size,
            max_sandbox_sessions: limits.max_sandbox_sessions,
        }
    }
}

/// 租户设置 DTO
#[derive(Debug, Serialize)]
pub struct TenantSettingsDto {
    pub token_ttl_seconds: u64,
    pub session_timeout_seconds: u64,
    pub max_login_attempts: u32,
    pub lockout_duration_seconds: u64,
    pub password_min_length: u32,
    pub require_password_complexity: bool,
    pub require_mfa: bool,
    pub timezone: String,
    pub language: String,
}

impl From<&TenantSettings> for TenantSettingsDto {
    fn from(settings: &TenantSettings) -> Self {
        Self {
            token_ttl_seconds: settings.token_ttl_seconds,
            session_timeout_seconds: settings.session_timeout_seconds,
            max_login_attempts: settings.max_login_attempts,
            lockout_duration_seconds: settings.lockout_duration_seconds,
            password_min_length: settings.password_min_length,
            require_password_complexity: settings.require_password_complexity,
            require_mfa: settings.require_mfa,
            timezone: settings.timezone.clone(),
            language: settings.language.clone(),
        }
    }
}

/// 更新配置请求
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateConfigRequest {
    #[serde(default)]
    pub feature_flags: Option<FeatureFlagsUpdate>,
    #[serde(default)]
    pub quota_limits: Option<QuotaLimitsUpdate>,
    #[serde(default)]
    pub settings: Option<TenantSettingsUpdate>,
}

/// 功能开关更新
#[derive(Debug, Default, Deserialize)]
pub struct FeatureFlagsUpdate {
    #[serde(default)]
    pub enable_credential_encryption: Option<bool>,
    #[serde(default)]
    pub enable_audit_logging: Option<bool>,
    #[serde(default)]
    pub enable_token_revocation: Option<bool>,
    #[serde(default)]
    pub enable_mfa: Option<bool>,
    #[serde(default)]
    pub enable_remote_attestation: Option<bool>,
    #[serde(default)]
    pub enable_auto_rotation: Option<bool>,
    #[serde(default)]
    pub allow_cors: Option<bool>,
    #[serde(default)]
    pub enable_ip_whitelist: Option<bool>,
    #[serde(default)]
    pub enable_webhooks: Option<bool>,
    #[serde(default)]
    pub enable_sso: Option<bool>,
    #[serde(default)]
    pub enable_custom_crypto: Option<bool>,
    #[serde(default)]
    pub enable_advanced_audit: Option<bool>,
}

impl FeatureFlagsUpdate {
    fn into_feature_flags(self, base: &FeatureFlags) -> FeatureFlags {
        FeatureFlags {
            enable_credential_encryption: self
                .enable_credential_encryption
                .unwrap_or(base.enable_credential_encryption),
            enable_audit_logging: self
                .enable_audit_logging
                .unwrap_or(base.enable_audit_logging),
            enable_token_revocation: self
                .enable_token_revocation
                .unwrap_or(base.enable_token_revocation),
            enable_mfa: self.enable_mfa.unwrap_or(base.enable_mfa),
            enable_remote_attestation: self
                .enable_remote_attestation
                .unwrap_or(base.enable_remote_attestation),
            enable_auto_rotation: self
                .enable_auto_rotation
                .unwrap_or(base.enable_auto_rotation),
            allow_cors: self.allow_cors.unwrap_or(base.allow_cors),
            enable_ip_whitelist: self.enable_ip_whitelist.unwrap_or(base.enable_ip_whitelist),
            enable_webhooks: self.enable_webhooks.unwrap_or(base.enable_webhooks),
            enable_sso: self.enable_sso.unwrap_or(base.enable_sso),
            enable_custom_crypto: self
                .enable_custom_crypto
                .unwrap_or(base.enable_custom_crypto),
            enable_advanced_audit: self
                .enable_advanced_audit
                .unwrap_or(base.enable_advanced_audit),
        }
    }
}

/// 配额限制更新
#[derive(Debug, Default, Deserialize)]
pub struct QuotaLimitsUpdate {
    #[serde(default)]
    pub max_credentials: Option<u64>,
    #[serde(default)]
    pub max_tokens_per_user: Option<u64>,
    #[serde(default)]
    pub max_requests_per_minute: Option<u64>,
    #[serde(default)]
    pub max_users: Option<u64>,
    #[serde(default)]
    pub max_connectors: Option<u64>,
    #[serde(default)]
    pub max_webhooks: Option<u64>,
    #[serde(default)]
    pub storage_quota_mb: Option<u64>,
    #[serde(default)]
    pub audit_retention_days: Option<u64>,
    #[serde(default)]
    pub max_token_ttl_seconds: Option<u64>,
    #[serde(default)]
    pub max_batch_size: Option<u64>,
    #[serde(default)]
    pub max_sandbox_sessions: Option<u64>,
}

impl QuotaLimitsUpdate {
    fn into_quota_limits(self, base: &QuotaLimits) -> QuotaLimits {
        QuotaLimits {
            max_credentials: self.max_credentials.unwrap_or(base.max_credentials),
            max_tokens_per_user: self.max_tokens_per_user.unwrap_or(base.max_tokens_per_user),
            max_requests_per_minute: self
                .max_requests_per_minute
                .unwrap_or(base.max_requests_per_minute),
            max_users: self.max_users.unwrap_or(base.max_users),
            max_connectors: self.max_connectors.unwrap_or(base.max_connectors),
            max_webhooks: self.max_webhooks.unwrap_or(base.max_webhooks),
            storage_quota_mb: self.storage_quota_mb.unwrap_or(base.storage_quota_mb),
            audit_retention_days: self
                .audit_retention_days
                .unwrap_or(base.audit_retention_days),
            max_token_ttl_seconds: self
                .max_token_ttl_seconds
                .unwrap_or(base.max_token_ttl_seconds),
            max_batch_size: self.max_batch_size.unwrap_or(base.max_batch_size),
            max_sandbox_sessions: self
                .max_sandbox_sessions
                .unwrap_or(base.max_sandbox_sessions),
        }
    }
}

/// 租户设置更新
#[derive(Debug, Default, Deserialize)]
pub struct TenantSettingsUpdate {
    #[serde(default)]
    pub token_ttl_seconds: Option<u64>,
    #[serde(default)]
    pub session_timeout_seconds: Option<u64>,
    #[serde(default)]
    pub max_login_attempts: Option<u32>,
    #[serde(default)]
    pub lockout_duration_seconds: Option<u64>,
    #[serde(default)]
    pub password_min_length: Option<u32>,
    #[serde(default)]
    pub require_password_complexity: Option<bool>,
    #[serde(default)]
    pub require_mfa: Option<bool>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
}

impl TenantSettingsUpdate {
    fn into_settings(self, base: &TenantSettings) -> TenantSettings {
        TenantSettings {
            token_ttl_seconds: self.token_ttl_seconds.unwrap_or(base.token_ttl_seconds),
            session_timeout_seconds: self
                .session_timeout_seconds
                .unwrap_or(base.session_timeout_seconds),
            max_login_attempts: self.max_login_attempts.unwrap_or(base.max_login_attempts),
            lockout_duration_seconds: self
                .lockout_duration_seconds
                .unwrap_or(base.lockout_duration_seconds),
            password_min_length: self.password_min_length.unwrap_or(base.password_min_length),
            require_password_complexity: self
                .require_password_complexity
                .unwrap_or(base.require_password_complexity),
            require_mfa: self.require_mfa.unwrap_or(base.require_mfa),
            allowed_callback_urls: base.allowed_callback_urls.clone(),
            timezone: self.timezone.unwrap_or_else(|| base.timezone.clone()),
            language: self.language.unwrap_or_else(|| base.language.clone()),
            metadata: base.metadata.clone(),
        }
    }
}

impl UpdateConfigRequest {
    fn into_partial_config(self, base_config: &TenantConfig) -> PartialTenantConfig {
        PartialTenantConfig {
            feature_flags: self
                .feature_flags
                .map(|f| f.into_feature_flags(&base_config.feature_flags)),
            quota_limits: self
                .quota_limits
                .map(|q| q.into_quota_limits(&base_config.quota_limits)),
            settings: self
                .settings
                .map(|s| s.into_settings(&base_config.settings)),
        }
    }
}

/// 错误响应
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub success: bool,
    pub error: ErrorDetail,
    pub meta: ResponseMeta,
}

#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
}

/// 创建租户处理器
pub async fn create_tenant_handler<S: TenantConfigStore + Clone + Send + Sync + 'static>(
    State(state): State<TenantApiState<S>>,
    Extension(token): Extension<ValidatedToken>,
    Json(mut request): Json<CreateTenantRequest>,
) -> Result<Json<CreateTenantResponse>, (StatusCode, Json<serde_json::Value>)> {
    let creator_user_id = uuid::Uuid::parse_str(&token.user_id).map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "success": false,
                "error": {
                    "code": "INVALID_USER_ID",
                    "message": "当前会话中的用户标识无效"
                },
                "meta": {
                    "request_id": uuid::Uuid::now_v7().to_string(),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            })),
        )
    })?;

    request.owner_user_id = Some(creator_user_id);

    match state
        .tenant_manager
        .create_tenant(request, Some(creator_user_id.to_string()))
        .await
    {
        Ok(result) => {
            state
                .tenant_service
                .upsert_tenant(result.tenant.clone())
                .await
                .map_err(|error| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({
                            "success": false,
                            "error": {
                                "code": "TENANT_PERSIST_FAILED",
                                "message": format!("持久化租户信息失败: {error}")
                            },
                            "meta": {
                                "request_id": uuid::Uuid::now_v7().to_string(),
                                "timestamp": chrono::Utc::now().to_rfc3339()
                            }
                        })),
                    )
                })?;

            if let Some(auth_service) = &state.auth_service {
                let tenant_uuid =
                    uuid::Uuid::parse_str(&result.tenant.id.to_string()).map_err(|_| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({
                                "success": false,
                                "error": {
                                    "code": "INVALID_TENANT_ID",
                                    "message": "新租户标识无效"
                                },
                                "meta": {
                                    "request_id": uuid::Uuid::now_v7().to_string(),
                                    "timestamp": chrono::Utc::now().to_rfc3339()
                                }
                            })),
                        )
                    })?;

                auth_service
                    .create_owner_membership(tenant_uuid, creator_user_id)
                    .await
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({
                                "success": false,
                                "error": {
                                    "code": "OWNER_MEMBERSHIP_FAILED",
                                    "message": format!("创建租户所有者成员资格失败: {e}")
                                },
                                "meta": {
                                    "request_id": uuid::Uuid::now_v7().to_string(),
                                    "timestamp": chrono::Utc::now().to_rfc3339()
                                }
                            })),
                        )
                    })?;

                auth_service
                    .update_user(creator_user_id, None, Some(tenant_uuid), None)
                    .await
                    .map_err(|e| {
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({
                                "success": false,
                                "error": {
                                    "code": "USER_UPDATE_FAILED",
                                    "message": format!("更新用户默认租户失败: {e}")
                                },
                                "meta": {
                                    "request_id": uuid::Uuid::now_v7().to_string(),
                                    "timestamp": chrono::Utc::now().to_rfc3339()
                                }
                            })),
                        )
                    })?;
            }

            let response = CreateTenantResponse {
                success: true,
                data: TenantData::from(&result.tenant),
                initialization: result
                    .initialization_steps
                    .into_iter()
                    .map(|s| InitializationStepDto {
                        step: s.step,
                        success: s.success,
                        error: s.error,
                    })
                    .collect(),
                meta: ResponseMeta {
                    request_id: uuid::Uuid::now_v7().to_string(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                },
            };
            Ok(Json(response))
        }
        Err(e) => {
            let (code, message) = match e {
                crate::tenant::TenantCreationError::InvalidName => {
                    ("INVALID_NAME", "租户名称不能为空")
                }
                crate::tenant::TenantCreationError::NameAlreadyExists(_) => {
                    ("NAME_EXISTS", "租户名称已被使用")
                }
                _ => ("INTERNAL_ERROR", "创建租户失败"),
            };
            Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "success": false,
                    "error": {
                        "code": code,
                        "message": message
                    },
                    "meta": {
                        "request_id": uuid::Uuid::now_v7().to_string(),
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    }
                })),
            ))
        }
    }
}

/// 获取租户配置处理器
pub async fn get_tenant_handler<S: TenantConfigStore + Clone + Send + Sync + 'static>(
    State(state): State<TenantApiState<S>>,
    Path(tenant_id): Path<String>,
) -> Result<Json<GetTenantResponse>, (StatusCode, Json<serde_json::Value>)> {
    let tenant_id = TenantIdType::from(tenant_id);

    match state.tenant_service.get_tenant(&tenant_id).await {
        Ok(Some(tenant)) => Ok(Json(GetTenantResponse {
            success: true,
            data: TenantData::from(&tenant),
            meta: ResponseMeta {
                request_id: uuid::Uuid::now_v7().to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            },
        })),
        Ok(None) | Err(TenantConfigError::NotFound(_)) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "success": false,
                "error": {
                    "code": "TENANT_NOT_FOUND",
                    "message": "租户不存在"
                },
                "meta": {
                    "request_id": uuid::Uuid::now_v7().to_string(),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            })),
        )),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": {
                    "code": "INTERNAL_ERROR",
                    "message": "获取租户失败"
                },
                "meta": {
                    "request_id": uuid::Uuid::now_v7().to_string(),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            })),
        )),
    }
}

/// 获取租户配置处理器
pub async fn get_tenant_config_handler<S: TenantConfigStore + Clone + Send + Sync + 'static>(
    State(state): State<TenantApiState<S>>,
    Path(tenant_id): Path<String>,
) -> Result<Json<TenantConfigResponse>, (StatusCode, Json<serde_json::Value>)> {
    let tenant_id = TenantIdType::from(tenant_id);

    // BUG-18265: 全零 UUID 是保留的系统租户标识，外部 API 应视作不存在并返回 404
    // 防止数据库中存在的 system tenant (id=00000000-...) 被误当作普通租户返回
    if let Ok(uuid) = uuid::Uuid::parse_str(tenant_id.as_str()) {
        if uuid == uuid::Uuid::nil() {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({
                    "success": false,
                    "error": {
                        "code": "TENANT_NOT_FOUND",
                        "message": "租户不存在"
                    },
                    "meta": {
                        "request_id": uuid::Uuid::now_v7().to_string(),
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    }
                })),
            ));
        }
    }

    match state.tenant_manager.get_config(&tenant_id).await {
        Ok(config) => {
            let response = TenantConfigResponse {
                success: true,
                data: TenantConfigData {
                    tenant_id: tenant_id.to_string(),
                    tier: get_tier_from_config(&config),
                    max_credentials: config.quota_limits.max_credentials,
                    max_sandbox_sessions: config.quota_limits.max_sandbox_sessions,
                    feature_flags: FeatureFlagsDto::from(&config.feature_flags),
                    quota_limits: QuotaLimitsDto::from(&config.quota_limits),
                    settings: TenantSettingsDto::from(&config.settings),
                    version: config.version,
                    updated_at: config.updated_at.map(|t| t.to_rfc3339()),
                    updated_by: config.updated_by,
                },
                meta: ResponseMeta {
                    request_id: uuid::Uuid::now_v7().to_string(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                },
            };
            Ok(Json(response))
        }
        Err(TenantConfigError::NotFound(_)) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "success": false,
                "error": {
                    "code": "TENANT_NOT_FOUND",
                    "message": "租户不存在"
                },
                "meta": {
                    "request_id": uuid::Uuid::now_v7().to_string(),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            })),
        )),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": {
                    "code": "INTERNAL_ERROR",
                    "message": "获取配置失败"
                },
                "meta": {
                    "request_id": uuid::Uuid::now_v7().to_string(),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            })),
        )),
    }
}

/// 映射 JSON 反序列化错误为 invalid_request
fn map_config_update_rejection(error: JsonRejection) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "success": false,
            "error": "invalid_request",
            "message": format!("Invalid tenant config update payload: {error}")
        })),
    )
}

/// 更新租户配置处理器
pub async fn update_tenant_config_handler<S: TenantConfigStore + Clone + Send + Sync + 'static>(
    State(state): State<TenantApiState<S>>,
    Path(tenant_id): Path<String>,
    payload: Result<Json<UpdateConfigRequest>, JsonRejection>,
) -> Result<Json<TenantConfigResponse>, (StatusCode, Json<serde_json::Value>)> {
    let Json(request) = payload.map_err(map_config_update_rejection)?;
    let tenant_id = TenantIdType::from(tenant_id);

    // 先获取当前配置以进行部分更新
    let current_config = match state.tenant_manager.get_config(&tenant_id).await {
        Ok(config) => config,
        Err(TenantConfigError::NotFound(_)) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({
                    "success": false,
                    "error": {
                        "code": "TENANT_NOT_FOUND",
                        "message": "租户不存在"
                    },
                    "meta": {
                        "request_id": uuid::Uuid::now_v7().to_string(),
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    }
                })),
            ));
        }
        Err(_) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "success": false,
                    "error": {
                        "code": "INTERNAL_ERROR",
                        "message": "获取当前配置失败"
                    },
                    "meta": {
                        "request_id": uuid::Uuid::now_v7().to_string(),
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    }
                })),
            ));
        }
    };

    let partial = request.into_partial_config(&current_config);

    match state
        .tenant_manager
        .update_config(&tenant_id, partial, "api_user")
        .await
    {
        Ok(config) => {
            let response = TenantConfigResponse {
                success: true,
                data: TenantConfigData {
                    tenant_id: tenant_id.to_string(),
                    tier: get_tier_from_config(&config),
                    max_credentials: config.quota_limits.max_credentials,
                    max_sandbox_sessions: config.quota_limits.max_sandbox_sessions,
                    feature_flags: FeatureFlagsDto::from(&config.feature_flags),
                    quota_limits: QuotaLimitsDto::from(&config.quota_limits),
                    settings: TenantSettingsDto::from(&config.settings),
                    version: config.version,
                    updated_at: config.updated_at.map(|t| t.to_rfc3339()),
                    updated_by: config.updated_by,
                },
                meta: ResponseMeta {
                    request_id: uuid::Uuid::now_v7().to_string(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                },
            };
            Ok(Json(response))
        }
        Err(TenantConfigError::ValidationError(msg)) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": {
                    "code": "VALIDATION_ERROR",
                    "message": msg
                },
                "meta": {
                    "request_id": uuid::Uuid::now_v7().to_string(),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            })),
        )),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": {
                    "code": "INTERNAL_ERROR",
                    "message": "更新配置失败"
                },
                "meta": {
                    "request_id": uuid::Uuid::now_v7().to_string(),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            })),
        )),
    }
}

/// 获取租户列表处理器（管理员）
pub async fn list_tenants_handler<S: TenantConfigStore + Clone + Send + Sync + 'static>(
    State(state): State<TenantApiState<S>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.tenant_service.list_tenants().await {
        Ok(tenants) => {
            let tenant_list: Vec<TenantData> = tenants.iter().map(TenantData::from).collect();
            Ok(Json(json!({
                "success": true,
                "data": {
                    "tenants": tenant_list,
                    "total": tenant_list.len()
                },
                "meta": {
                    "request_id": uuid::Uuid::now_v7().to_string(),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            })))
        }
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": {
                    "code": "INTERNAL_ERROR",
                    "message": "获取租户列表失败"
                },
                "meta": {
                    "request_id": uuid::Uuid::now_v7().to_string(),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            })),
        )),
    }
}

/// 激活租户处理器
pub async fn activate_tenant_handler<S: TenantConfigStore + Clone + Send + Sync + 'static>(
    State(state): State<TenantApiState<S>>,
    Path(tenant_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let tenant_id = TenantIdType::from(tenant_id);

    match state
        .tenant_service
        .activate_tenant(&tenant_id, Some("api_user".to_string()))
        .await
    {
        Ok(tenant) => Ok(Json(json!({
            "success": true,
            "data": {
                "id": tenant.id.to_string(),
                "status": tenant.status.to_string(),
                "activated_at": chrono::Utc::now().to_rfc3339()
            },
            "meta": {
                "request_id": uuid::Uuid::now_v7().to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        }))),
        Err(TenantConfigError::NotFound(_)) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "success": false,
                "error": {
                    "code": "TENANT_NOT_FOUND",
                    "message": "租户不存在"
                },
                "meta": {
                    "request_id": uuid::Uuid::now_v7().to_string(),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            })),
        )),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": {
                    "code": "INTERNAL_ERROR",
                    "message": "激活租户失败"
                },
                "meta": {
                    "request_id": uuid::Uuid::now_v7().to_string(),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            })),
        )),
    }
}

/// 暂停租户处理器
pub async fn suspend_tenant_handler<S: TenantConfigStore + Clone + Send + Sync + 'static>(
    State(state): State<TenantApiState<S>>,
    Path(tenant_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let tenant_id = TenantIdType::from(tenant_id);
    let reason = body
        .get("reason")
        .and_then(|r| r.as_str())
        .map(String::from);

    match state
        .tenant_service
        .suspend_tenant(&tenant_id, reason, Some("api_user".to_string()))
        .await
    {
        Ok(tenant) => Ok(Json(json!({
            "success": true,
            "data": {
                "id": tenant.id.to_string(),
                "status": tenant.status.to_string(),
                "suspended_at": chrono::Utc::now().to_rfc3339()
            },
            "meta": {
                "request_id": uuid::Uuid::now_v7().to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        }))),
        Err(TenantConfigError::NotFound(_)) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "success": false,
                "error": {
                    "code": "TENANT_NOT_FOUND",
                    "message": "租户不存在"
                },
                "meta": {
                    "request_id": uuid::Uuid::now_v7().to_string(),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            })),
        )),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": {
                    "code": "INTERNAL_ERROR",
                    "message": "暂停租户失败"
                },
                "meta": {
                    "request_id": uuid::Uuid::now_v7().to_string(),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            })),
        )),
    }
}

/// 删除租户处理器
pub async fn delete_tenant_handler<S: TenantConfigStore + Clone + Send + Sync + 'static>(
    State(state): State<TenantApiState<S>>,
    Path(tenant_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let tenant_id = TenantIdType::from(tenant_id);

    match state
        .tenant_service
        .delete_tenant(&tenant_id, Some("api_user".to_string()))
        .await
    {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(TenantConfigError::NotFound(_)) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "success": false,
                "error": {
                    "code": "TENANT_NOT_FOUND",
                    "message": "租户不存在"
                },
                "meta": {
                    "request_id": uuid::Uuid::now_v7().to_string(),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            })),
        )),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": {
                    "code": "INTERNAL_ERROR",
                    "message": "删除租户失败"
                },
                "meta": {
                    "request_id": uuid::Uuid::now_v7().to_string(),
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            })),
        )),
    }
}

async fn suspend_tenant_missing_id_handler() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "success": false,
            "error": {
                "code": "TENANT_NOT_FOUND",
                "message": "租户不存在"
            },
            "meta": {
                "request_id": uuid::Uuid::now_v7().to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        })),
    )
}

/// 兜底处理器：POST /tenants/activate 缺少租户ID时返回404
/// 防止被 /tenants/:id 动态段误匹配为 405 Method Not Allowed
async fn activate_tenant_missing_id_handler() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "success": false,
            "error": {
                "code": "TENANT_NOT_FOUND",
                "message": "租户不存在"
            },
            "meta": {
                "request_id": uuid::Uuid::now_v7().to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        })),
    )
}

/// 兜底处理器：PUT /tenants/config 缺少租户ID时返回404
/// 防止被 /tenants/:id 动态段误匹配为 405 Method Not Allowed
async fn update_tenant_config_missing_id_handler() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "success": false,
            "error": {
                "code": "TENANT_NOT_FOUND",
                "message": "租户不存在"
            },
            "meta": {
                "request_id": uuid::Uuid::now_v7().to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        })),
    )
}

/// 兜底处理器：GET /tenants/config 缺少租户ID时返回404
/// BUG-18264: 防止被 /tenants/:id 动态段误吸收，将 "config" 当作 tenant_id 导致 500
async fn get_tenant_config_missing_id_handler() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "success": false,
            "error": {
                "code": "TENANT_NOT_FOUND",
                "message": "租户不存在"
            },
            "meta": {
                "request_id": uuid::Uuid::now_v7().to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        })),
    )
}

/// 从配置推断租户层级
fn get_tier_from_config(config: &TenantConfig) -> String {
    if config.feature_flags.enable_sso {
        "enterprise".to_string()
    } else if config.feature_flags.enable_mfa && config.quota_limits.max_credentials >= 10_000 {
        "pro".to_string()
    } else {
        "free".to_string()
    }
}

/// 创建租户管理路由
pub fn tenant_routes<S: TenantConfigStore + Clone + Send + Sync + 'static>()
-> Router<TenantApiState<S>> {
    Router::new()
        .route(
            "/tenants",
            post(create_tenant_handler::<S>).get(list_tenants_handler::<S>),
        )
        .route(
            "/tenants/:id",
            get(get_tenant_handler::<S>).delete(delete_tenant_handler::<S>),
        )
        .route(
            "/tenants/:id/config",
            get(get_tenant_config_handler::<S>).put(update_tenant_config_handler::<S>),
        )
        .route("/tenants/suspend", post(suspend_tenant_missing_id_handler))
        .route(
            "/tenants/activate",
            post(activate_tenant_missing_id_handler),
        )
        // BUG-18266: PUT /tenants/config 缺少租户ID时返回404
        // BUG-18264: GET /tenants/config 缺少租户ID时返回404
        // 防止被 /tenants/:id 动态段误匹配为 405 Method Not Allowed 或 500 Internal Server Error
        .route(
            "/tenants/config",
            get(get_tenant_config_missing_id_handler).put(update_tenant_config_missing_id_handler),
        )
        .route("/tenants/:id/activate", post(activate_tenant_handler::<S>))
        .route("/tenants/:id/suspend", post(suspend_tenant_handler::<S>))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::{
        api::middleware::{TokenScope, ValidatedToken},
        tenant::{MemoryTenantConfigStore, MemoryTenantStorage, TenantService},
    };

    #[test]
    fn test_get_tier_from_config() {
        let free = TenantConfig::free_tier();
        assert_eq!(get_tier_from_config(&free), "free");

        let pro = TenantConfig::pro_tier();
        assert_eq!(get_tier_from_config(&pro), "pro");

        let enterprise = TenantConfig::enterprise_tier();
        assert_eq!(get_tier_from_config(&enterprise), "enterprise");
    }

    #[test]
    fn test_feature_flags_update() {
        let base = FeatureFlags::default();
        let update = FeatureFlagsUpdate {
            enable_mfa: Some(true),
            enable_sso: Some(true),
            ..Default::default()
        };

        let result = update.into_feature_flags(&base);
        assert!(result.enable_mfa);
        assert!(result.enable_sso);
        // 其他字段保持默认值
        assert!(result.enable_credential_encryption);
    }

    #[test]
    fn test_quota_limits_update() {
        let base = QuotaLimits::default();
        let update = QuotaLimitsUpdate {
            max_credentials: Some(5000),
            max_users: Some(500),
            ..Default::default()
        };

        let result = update.into_quota_limits(&base);
        assert_eq!(result.max_credentials, 5000);
        assert_eq!(result.max_users, 500);
        // 其他字段保持默认值
        assert_eq!(result.max_tokens_per_user, base.max_tokens_per_user);
    }

    #[tokio::test]
    async fn activate_missing_tenant_id_returns_not_found() {
        // BUG-18268 回归测试：POST /tenants/activate 缺少租户ID应返回404，
        // 而不是被 /tenants/:id 动态段误匹配为 405 Method Not Allowed
        let tenant_manager = TenantManager::new_simple(MemoryTenantConfigStore::new());
        let tenant_service = Arc::new(MemoryTenantStorage::new());
        let app = tenant_routes::<MemoryTenantConfigStore>()
            .with_state(TenantApiState::new(tenant_manager, tenant_service));

        let request = Request::builder()
            .method("POST")
            .uri("/tenants/activate")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["error"]["code"], "TENANT_NOT_FOUND");
    }

    #[tokio::test]
    async fn suspend_missing_tenant_id_returns_not_found() {
        let tenant_manager = TenantManager::new_simple(MemoryTenantConfigStore::new());
        let tenant_service = Arc::new(MemoryTenantStorage::new());
        let app = tenant_routes::<MemoryTenantConfigStore>()
            .with_state(TenantApiState::new(tenant_manager, tenant_service));

        let request = Request::builder()
            .method("POST")
            .uri("/tenants/suspend")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"reason":"missing-id"}"#))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["error"]["code"], "TENANT_NOT_FOUND");
    }

    #[tokio::test]
    async fn suspend_existing_tenant_path_still_works() {
        let tenant_manager = TenantManager::new_simple(MemoryTenantConfigStore::new());
        let tenant_service = Arc::new(MemoryTenantStorage::new());
        let tenant = Tenant::new("bug-18270");
        let tenant_id = tenant.id.to_string();
        tenant_service.upsert_tenant(tenant).await.unwrap();

        let app = tenant_routes::<MemoryTenantConfigStore>()
            .with_state(TenantApiState::new(tenant_manager, tenant_service));

        let mut request = Request::builder()
            .method("POST")
            .uri(format!("/tenants/{tenant_id}/suspend"))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"reason":"explicit-id"}"#))
            .unwrap();
        request.extensions_mut().insert(ValidatedToken::mock(
            &tenant_id,
            "test-user",
            vec![TokenScope::TenantAdmin],
        ));

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn activate_zero_uuid_returns_not_found() {
        let tenant_manager = TenantManager::new_simple(MemoryTenantConfigStore::new());
        let tenant_service = Arc::new(MemoryTenantStorage::new());
        let app = tenant_routes::<MemoryTenantConfigStore>()
            .with_state(TenantApiState::new(tenant_manager, tenant_service));

        let zero_uuid = "00000000-0000-0000-0000-000000000000";
        let mut request = Request::builder()
            .method("POST")
            .uri(format!("/tenants/{zero_uuid}/activate"))
            .body(Body::empty())
            .unwrap();
        request.extensions_mut().insert(ValidatedToken::mock(
            zero_uuid,
            "test-user",
            vec![TokenScope::TenantAdmin],
        ));

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["error"]["code"], "TENANT_NOT_FOUND");
    }

    #[tokio::test]
    async fn get_config_zero_uuid_returns_not_found() {
        // BUG-18265 回归测试：GET /tenants/00000000-.../config 必须返回 404，
        // 因为全零 UUID 是保留的系统租户标识，不应暴露给外部 API
        let tenant_manager = TenantManager::new_simple(MemoryTenantConfigStore::new());
        let tenant_service = Arc::new(MemoryTenantStorage::new());
        let app = tenant_routes::<MemoryTenantConfigStore>()
            .with_state(TenantApiState::new(tenant_manager, tenant_service));

        let zero_uuid = "00000000-0000-0000-0000-000000000000";
        let request = Request::builder()
            .method("GET")
            .uri(format!("/tenants/{zero_uuid}/config"))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["error"]["code"], "TENANT_NOT_FOUND");
    }

    #[tokio::test]
    async fn update_config_missing_tenant_id_returns_not_found() {
        // BUG-18266 回归测试：PUT /tenants/config 缺少租户ID应返回404，
        // 而不是被 /tenants/:id 动态段误匹配为 405 Method Not Allowed
        let tenant_manager = TenantManager::new_simple(MemoryTenantConfigStore::new());
        let tenant_service = Arc::new(MemoryTenantStorage::new());
        let app = tenant_routes::<MemoryTenantConfigStore>()
            .with_state(TenantApiState::new(tenant_manager, tenant_service));

        let request = Request::builder()
            .method("PUT")
            .uri("/tenants/config")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"tier":"pro"}"#))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["error"]["code"], "TENANT_NOT_FOUND");
    }

    #[tokio::test]
    async fn get_config_missing_tenant_id_returns_not_found() {
        // BUG-18264 回归测试：GET /tenants/config 缺少租户ID应返回404，
        // 而不是被 /tenants/:id 动态段误吸收导致 500 Internal Server Error
        let tenant_manager = TenantManager::new_simple(MemoryTenantConfigStore::new());
        let tenant_service = Arc::new(MemoryTenantStorage::new());
        let app = tenant_routes::<MemoryTenantConfigStore>()
            .with_state(TenantApiState::new(tenant_manager, tenant_service));

        let request = Request::builder()
            .method("GET")
            .uri("/tenants/config")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["error"]["code"], "TENANT_NOT_FOUND");
    }

    #[tokio::test]
    async fn update_config_with_unknown_field_returns_400() {
        // BUG-18267 回归测试：PUT /tenants/:id/config 传入未知字段（如非法 tier）
        // 应返回 400 Bad Request，而非静默忽略并返回 200
        use crate::tenant::TenantConfigManager;

        let store = MemoryTenantConfigStore::new();
        let config_manager = TenantConfigManager::new(store.clone());
        let tenant_service = Arc::new(MemoryTenantStorage::new());
        let tenant = Tenant::new("bug-18267-test");
        let tenant_id = tenant.id.clone();
        tenant_service.upsert_tenant(tenant).await.unwrap();

        // 创建初始配置
        config_manager
            .create_config(&tenant_id, TenantConfig::default())
            .await
            .unwrap();

        let tenant_manager = TenantManager::new_simple(store);
        let app = tenant_routes::<MemoryTenantConfigStore>()
            .with_state(TenantApiState::new(tenant_manager, tenant_service));

        // 发送包含非法 tier 字段的请求
        let request = Request::builder()
            .method("PUT")
            .uri(format!("/tenants/{tenant_id}/config"))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"tier":"invalid_tier_value"}"#))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["error"], "invalid_request");
    }

    #[tokio::test]
    async fn update_config_with_valid_fields_still_works() {
        // 验证合法配置字段更新不受 deny_unknown_fields 影响
        use crate::tenant::TenantConfigManager;

        let store = MemoryTenantConfigStore::new();
        let config_manager = TenantConfigManager::new(store.clone());
        let tenant_service = Arc::new(MemoryTenantStorage::new());
        let tenant = Tenant::new("bug-18267-valid");
        let tenant_id = tenant.id.clone();
        tenant_service.upsert_tenant(tenant).await.unwrap();

        // 创建初始配置
        config_manager
            .create_config(&tenant_id, TenantConfig::default())
            .await
            .unwrap();

        let tenant_manager = TenantManager::new_simple(store);
        let app = tenant_routes::<MemoryTenantConfigStore>()
            .with_state(TenantApiState::new(tenant_manager, tenant_service));

        // 发送合法的配置更新
        let request = Request::builder()
            .method("PUT")
            .uri(format!("/tenants/{tenant_id}/config"))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"quota_limits":{"max_credentials":500}}"#))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert!(payload["success"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn get_config_valid_tenant_returns_all_required_fields() {
        // BUG-18263 回归测试：GET /tenants/:id/config 返回 200 时，
        // 响应体 data 层必须包含 tier、max_credentials、max_sandbox_sessions 三个顶层字段
        use crate::tenant::TenantConfigManager;

        let store = MemoryTenantConfigStore::new();
        let config_manager = TenantConfigManager::new(store.clone());
        let tenant_service = Arc::new(MemoryTenantStorage::new());
        let tenant = Tenant::new("bug-18263-test");
        let tenant_id = tenant.id.clone();
        tenant_service.upsert_tenant(tenant).await.unwrap();

        // 创建专业版配置（有明确的 tier 特征）
        let config = TenantConfig::pro_tier();
        config_manager
            .create_config(&tenant_id, config.clone())
            .await
            .unwrap();

        let tenant_manager = TenantManager::new_simple(store);
        let app = tenant_routes::<MemoryTenantConfigStore>()
            .with_state(TenantApiState::new(tenant_manager, tenant_service));

        let request = Request::builder()
            .method("GET")
            .uri(format!("/tenants/{tenant_id}/config"))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();

        // 验证成功标志
        assert!(payload["success"].as_bool().unwrap());

        // 验证 data 层包含三个必需的顶层字段
        let data = &payload["data"];
        assert!(data.get("tier").is_some(), "data.tier field missing");
        assert!(
            data.get("max_credentials").is_some(),
            "data.max_credentials field missing"
        );
        assert!(
            data.get("max_sandbox_sessions").is_some(),
            "data.max_sandbox_sessions field missing"
        );

        // 验证字段值正确性
        assert_eq!(data["tier"].as_str().unwrap(), "pro");
        assert_eq!(
            data["max_credentials"].as_u64().unwrap(),
            config.quota_limits.max_credentials
        );
        assert_eq!(
            data["max_sandbox_sessions"].as_u64().unwrap(),
            config.quota_limits.max_sandbox_sessions
        );

        // 验证一致性：data.max_credentials == data.quota_limits.max_credentials
        assert_eq!(
            data["max_credentials"].as_u64().unwrap(),
            data["quota_limits"]["max_credentials"].as_u64().unwrap()
        );
        assert_eq!(
            data["max_sandbox_sessions"].as_u64().unwrap(),
            data["quota_limits"]["max_sandbox_sessions"].as_u64().unwrap()
        );
    }
}
