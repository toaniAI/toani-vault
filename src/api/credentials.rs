//! 凭证 CRUD API
//!
//! 实现 EP2-Story2.2 凭证管理 API
//! - POST /api/v1/credentials - 创建凭证
//! - GET /api/v1/credentials - 获取凭证列表
//! - GET /api/v1/credentials/:id - 获取凭证详情
//! - POST /api/v1/credentials/:id/decrypt - 解密凭证
//! - DELETE /api/v1/credentials/:id - 删除凭证

use crate::api::audit::AuditStorage;
use crate::api::i18n::I18nMetadata;
use crate::api::middleware::{
    AuthError, TokenScope, ValidatedToken, require_any_scope, require_scope,
};
use crate::crypto::CredentialCryptoContext;
use crate::crypto::hkdf::KeyHierarchy;
use crate::models::{
    CredentialCustomFunction, CredentialMetadata, CredentialProvider, CredentialType,
};
use crate::tee::SharedEnclave;
use crate::tee::sandbox::domain_policy::validate_allowed_domains;
use crate::tee::sandbox::function_runtime::validate_custom_functions;
use crate::vault::models::{
    CreateCredentialRequest, CredentialFilter, CredentialId, EncryptedPayload, ServiceId, TenantId,
    UserId, VaultError,
};
use crate::vault::storage::{CredentialConfigUpdate, CredentialVault};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::warn;

/// 应用状态
#[derive(Clone)]
pub struct AppState {
    /// 凭证 Vault
    pub vault: Arc<CredentialVault>,
    /// 密钥层次结构
    pub key_hierarchy: Arc<RwLock<KeyHierarchy>>,
    /// 共享 TEE Enclave 实例（已接入状态，待切换实际加解密路径）
    pub enclave: SharedEnclave,
    /// 审计日志记录器
    pub audit_logger: Arc<dyn AuditLogger>,
}

#[derive(Debug, Clone)]
struct TeeRuntimeSnapshot {
    enclave_initialized: bool,
    execution_mode: &'static str,
    mrenclave_label: String,
}

async fn tee_runtime_snapshot(state: &AppState) -> TeeRuntimeSnapshot {
    let enclave = state.enclave.lock().await;

    TeeRuntimeSnapshot {
        enclave_initialized: enclave.is_running(),
        execution_mode: if enclave.is_running() {
            "tee_enforced"
        } else {
            "software_fallback"
        },
        mrenclave_label: if enclave.is_running() {
            hex::encode(enclave.mrenclave())
        } else {
            "software_mode".to_string()
        },
    }
}

/// 审计日志记录器 trait
pub trait AuditLogger: Send + Sync {
    fn log_credential_created(
        &self,
        tenant_id: &str,
        user_id: &str,
        credential_id: &str,
        jti: &str,
        mrenclave: &str,
    );
    fn log_credential_accessed(
        &self,
        tenant_id: &str,
        user_id: &str,
        credential_id: &str,
        jti: &str,
        mrenclave: &str,
    );
    fn log_credential_deleted(
        &self,
        tenant_id: &str,
        user_id: &str,
        credential_id: &str,
        jti: &str,
        mrenclave: &str,
    );
    fn log_decryption_attempt(
        &self,
        tenant_id: &str,
        user_id: &str,
        credential_id: &str,
        success: bool,
        jti: &str,
        mrenclave: &str,
    );
}

/// 默认审计日志记录器（打印到控制台）
pub struct DefaultAuditLogger;

impl AuditLogger for DefaultAuditLogger {
    fn log_credential_created(
        &self,
        tenant_id: &str,
        user_id: &str,
        credential_id: &str,
        jti: &str,
        mrenclave: &str,
    ) {
        tracing::info!(
            "[AUDIT] Credential created - tenant: {tenant_id}, user: {user_id}, credential: {credential_id}, jti: {jti}, mrenclave: {mrenclave}"
        );
    }

    fn log_credential_accessed(
        &self,
        tenant_id: &str,
        user_id: &str,
        credential_id: &str,
        jti: &str,
        mrenclave: &str,
    ) {
        tracing::info!(
            "[AUDIT] Credential accessed - tenant: {tenant_id}, user: {user_id}, credential: {credential_id}, jti: {jti}, mrenclave: {mrenclave}"
        );
    }

    fn log_credential_deleted(
        &self,
        tenant_id: &str,
        user_id: &str,
        credential_id: &str,
        jti: &str,
        mrenclave: &str,
    ) {
        tracing::info!(
            "[AUDIT] Credential deleted - tenant: {tenant_id}, user: {user_id}, credential: {credential_id}, jti: {jti}, mrenclave: {mrenclave}"
        );
    }

    fn log_decryption_attempt(
        &self,
        tenant_id: &str,
        user_id: &str,
        credential_id: &str,
        success: bool,
        jti: &str,
        mrenclave: &str,
    ) {
        tracing::info!(
            "[AUDIT] Decryption attempt - tenant: {tenant_id}, user: {user_id}, credential: {credential_id}, success: {success}, jti: {jti}, mrenclave: {mrenclave}"
        );
    }
}

use crate::audit::{AuditAction, AuditEntry, Outcome, RedactedParam};

/// 存储审计日志记录器 - 将日志写入统一审计后端并打印到控制台
pub struct StorageAuditLogger {
    storage: Arc<dyn AuditStorage>,
}

struct CredentialAuditContext<'a> {
    tenant_id: &'a str,
    user_id: &'a str,
    credential_id: &'a str,
    jti: &'a str,
    mrenclave: &'a str,
}

impl StorageAuditLogger {
    /// 创建新的存储审计日志记录器
    pub fn new(storage: Arc<dyn AuditStorage>) -> Self {
        Self { storage }
    }

    /// 记录审计事件到存储
    ///
    /// - `jti`: 从 ValidatedToken.token_id 获取的 action token JTI
    /// - `mrenclave`: TEE MRENCLAVE 测量值，软件模式下使用 "software_mode"
    fn record_to_storage(
        &self,
        action: AuditAction,
        outcome: Outcome,
        context: CredentialAuditContext<'_>,
    ) {
        let storage = Arc::clone(&self.storage);
        let user_id_hash = crate::audit::events::hash_user_id(context.user_id);
        let entry = AuditEntry::new(
            user_id_hash,
            "session",
            "credentials",
            action,
            outcome,
            context.mrenclave,
            context.jti,
        )
        .with_param(
            "credential_id",
            RedactedParam::Plain(context.credential_id.to_string()),
        )
        .with_param(
            "tenant_id",
            RedactedParam::Plain(context.tenant_id.to_string()),
        );

        tokio::spawn(async move {
            if let Err(error) = storage.record(entry).await {
                tracing::warn!("[AUDIT] 存储审计日志失败：{error}");
            }
        });
    }
}

impl AuditLogger for StorageAuditLogger {
    fn log_credential_created(
        &self,
        tenant_id: &str,
        user_id: &str,
        credential_id: &str,
        jti: &str,
        mrenclave: &str,
    ) {
        // 打印到控制台
        tracing::info!(
            "[AUDIT] Credential created - tenant: {tenant_id}, user: {user_id}, credential: {credential_id}"
        );

        // 写入存储
        self.record_to_storage(
            AuditAction::CredentialCreate,
            Outcome::Success,
            CredentialAuditContext {
                tenant_id,
                user_id,
                credential_id,
                jti,
                mrenclave,
            },
        );
    }

    fn log_credential_accessed(
        &self,
        tenant_id: &str,
        user_id: &str,
        credential_id: &str,
        jti: &str,
        mrenclave: &str,
    ) {
        // 打印到控制台
        tracing::info!(
            "[AUDIT] Credential accessed - tenant: {tenant_id}, user: {user_id}, credential: {credential_id}"
        );

        // 写入存储
        self.record_to_storage(
            AuditAction::CredentialAccess,
            Outcome::Success,
            CredentialAuditContext {
                tenant_id,
                user_id,
                credential_id,
                jti,
                mrenclave,
            },
        );
    }

    fn log_credential_deleted(
        &self,
        tenant_id: &str,
        user_id: &str,
        credential_id: &str,
        jti: &str,
        mrenclave: &str,
    ) {
        // 打印到控制台
        tracing::info!(
            "[AUDIT] Credential deleted - tenant: {tenant_id}, user: {user_id}, credential: {credential_id}"
        );

        // 写入存储
        self.record_to_storage(
            AuditAction::CredentialDelete,
            Outcome::Success,
            CredentialAuditContext {
                tenant_id,
                user_id,
                credential_id,
                jti,
                mrenclave,
            },
        );
    }

    fn log_decryption_attempt(
        &self,
        tenant_id: &str,
        user_id: &str,
        credential_id: &str,
        success: bool,
        jti: &str,
        mrenclave: &str,
    ) {
        // 打印到控制台
        tracing::info!(
            "[AUDIT] Decryption attempt - tenant: {tenant_id}, user: {user_id}, credential: {credential_id}, success: {success}"
        );

        // 写入存储
        let outcome = if success {
            Outcome::Success
        } else {
            Outcome::Failure
        };
        self.record_to_storage(
            AuditAction::CredentialDecrypt,
            outcome,
            CredentialAuditContext {
                tenant_id,
                user_id,
                credential_id,
                jti,
                mrenclave,
            },
        );
    }
}

/// API 错误响应
#[derive(Debug, Clone, Serialize)]
#[allow(clippy::result_large_err)]
pub struct ApiError {
    pub error: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub i18n: Option<I18nMetadata>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub locale: String,
}

impl ApiError {
    pub fn new(error: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            message: message.into(),
            details: None,
            i18n: None,
            locale: String::new(),
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    /// 从 AuthError 创建 ApiError，保留 error code、message、i18n 和 locale
    pub fn from_auth_error(auth_error: AuthError) -> Self {
        Self {
            error: auth_error.error,
            message: auth_error.message,
            details: None,
            i18n: auth_error.i18n,
            locale: auth_error.locale,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.error.as_str() {
            "not_found" => StatusCode::NOT_FOUND,
            "invalid_request" => StatusCode::BAD_REQUEST,
            "unauthorized" => StatusCode::UNAUTHORIZED,
            "forbidden" => StatusCode::FORBIDDEN,
            "insufficient_scope" => StatusCode::FORBIDDEN,
            "credential_expired" => StatusCode::UNPROCESSABLE_ENTITY,
            "internal_error" => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        if status.is_server_error() {
            tracing::error!(error_code = %self.error, message = %self.message, status = status.as_u16(), "credential API server error");
        } else if status.is_client_error() {
            tracing::warn!(error_code = %self.error, message = %self.message, status = status.as_u16(), "credential API client error");
        }

        (status, Json(json!(self))).into_response()
    }
}

/// 将 VaultError 转换为 ApiError，租户隔离违规返回 403 而非 500
fn vault_error_to_api_error(e: VaultError) -> ApiError {
    match e {
        VaultError::TenantIsolationViolation { .. } => ApiError::new("forbidden", "无权访问该凭证"),
        VaultError::CredentialNotFound(_) => ApiError::new("not_found", "凭证不存在"),
        _ => ApiError::new("internal_error", e.to_string()),
    }
}

/// 创建凭证请求
#[derive(Debug, Deserialize)]
pub struct CreateCredentialApiRequest {
    /// 服务 ID
    pub service_id: String,
    /// 凭证类型（字符串形式，将在业务层验证）
    pub credential_type: String,
    /// 明文凭证内容（将被加密）
    #[serde(alias = "value")]
    pub plaintext_data: serde_json::Value,
    /// 过期时间（Unix 时间戳，可选）
    pub expires_at: Option<u64>,
    /// Provider（okx / binance / custom）
    #[serde(default)]
    pub provider: Option<String>,
    /// HTTP 请求白名单域名
    #[serde(default)]
    pub allowed_domains: Option<Vec<String>>,
    /// 自定义模板函数
    #[serde(default)]
    pub custom_functions: Option<Vec<CredentialCustomFunction>>,
}

#[derive(Debug, Clone)]
struct ParsedCredentialConfig {
    provider: Option<CredentialProvider>,
    allowed_domains: Vec<String>,
    custom_functions: Vec<CredentialCustomFunction>,
}

fn normalize_oauth_plaintext_data(
    credential_type: CredentialType,
    plaintext_data: &serde_json::Value,
) -> serde_json::Value {
    if credential_type != CredentialType::OAuthRefresh {
        return plaintext_data.clone();
    }

    let mut normalized = plaintext_data.clone();
    let Some(map) = normalized.as_object_mut() else {
        return normalized;
    };

    if !map.contains_key("refreshToken") {
        if let Some(legacy_value) = map.get("refresh_token").cloned() {
            map.insert("refreshToken".to_string(), legacy_value);
        }
    }

    map.remove("refresh_token");
    normalized
}

/// 创建凭证响应
#[derive(Debug, Serialize)]
pub struct CreateCredentialResponse {
    pub credential_id: String,
    pub service_id: String,
    pub credential_type: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub provider: Option<String>,
    pub allowed_domains: Vec<String>,
    pub custom_functions: Vec<CredentialCustomFunction>,
}

#[allow(clippy::result_large_err)]
fn validate_expires_at(expires_at: Option<u64>) -> Result<(), ApiError> {
    let Some(expires_at) = expires_at else {
        return Ok(());
    };

    let current_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ApiError::new("internal_error", "系统时间异常"))?
        .as_secs();

    if expires_at <= current_timestamp {
        return Err(ApiError::new(
            "invalid_request",
            "expires_at must be in the future",
        ));
    }

    Ok(())
}

#[allow(clippy::result_large_err)]
fn parse_credential_provider(provider: &str) -> Result<CredentialProvider, ApiError> {
    match provider.trim() {
        "okx" => Ok(CredentialProvider::Okx),
        "binance" => Ok(CredentialProvider::Binance),
        "custom" => Ok(CredentialProvider::Custom),
        other => Err(ApiError::new(
            "invalid_request",
            format!("unsupported provider: {other}"),
        )),
    }
}

#[allow(clippy::result_large_err)]
fn parse_create_credential_config(
    provider: Option<String>,
    allowed_domains: Option<Vec<String>>,
    custom_functions: Option<Vec<CredentialCustomFunction>>,
) -> Result<ParsedCredentialConfig, ApiError> {
    let provider = provider
        .map(|value| parse_credential_provider(&value))
        .transpose()?;
    let allowed_domains = allowed_domains.unwrap_or_default();
    validate_allowed_domains(&allowed_domains)
        .map_err(|error| ApiError::new("invalid_request", error.to_string()))?;
    let custom_functions = custom_functions.unwrap_or_default();
    validate_custom_functions(&custom_functions)
        .map_err(|error| ApiError::new("invalid_request", error.to_string()))?;

    Ok(ParsedCredentialConfig {
        provider,
        allowed_domains,
        custom_functions,
    })
}

/// POST /api/v1/credentials - 创建凭证
pub async fn create_credential(
    State(state): State<AppState>,
    Extension(token): Extension<ValidatedToken>,
    payload: Result<Json<CreateCredentialApiRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<(StatusCode, Json<CreateCredentialResponse>), ApiError> {
    // 处理 JSON 反序列化错误（如必填字段缺失），返回 400 而不是默认的 422
    let Json(request) =
        payload.map_err(|e| ApiError::new("invalid_request", format!("请求体解析失败: {e}")))?;

    // 验证 Scope: credential:write
    require_scope(TokenScope::CredentialWrite)(&token).map_err(ApiError::from_auth_error)?;

    validate_expires_at(request.expires_at)?;
    let credential_config = parse_create_credential_config(
        request.provider.clone(),
        request.allowed_domains.clone(),
        request.custom_functions.clone(),
    )?;

    // 解析并验证 credential_type，无效时返回 400 invalid_request
    let credential_type = parse_credential_type(&request.credential_type)?;

    // 验证 service_id 非空（空字符串或仅空白字符应返回 400）
    if request.service_id.trim().is_empty() {
        return Err(
            ApiError::new("invalid_request", "service_id cannot be empty").with_details(
                serde_json::json!({
                    "field": "service_id",
                    "received": request.service_id
                }),
            ),
        );
    }

    // 先创建 UserId 对象，用于加密和存储
    let user_id = UserId::new(&token.user_id);
    let tenant_id = TenantId::new(&token.tenant_id);

    // 先生成 credential_id，确保加密时使用的 ID 与存储时一致
    let credential_id = CredentialId::new();
    let normalized_plaintext_data =
        normalize_oauth_plaintext_data(credential_type, &request.plaintext_data);

    // 加密凭证内容（在 TEE 内完成）
    let encrypted_payload = encrypt_credential_in_tee(
        &state,
        tenant_id.as_str(),
        &user_id,
        &credential_id,
        &normalized_plaintext_data,
    )
    .await
    .map_err(|e| ApiError::new("internal_error", e))?;

    // 创建凭证请求
    let create_request = CreateCredentialRequest {
        tenant_id,
        user_id,
        service_id: ServiceId::new(&request.service_id),
        credential_type,
        expires_at: request.expires_at,
        provider: credential_config.provider,
        allowed_domains: credential_config.allowed_domains,
        custom_functions: credential_config.custom_functions,
    };

    // 存储凭证（使用预生成的 credential_id）
    let entry = state
        .vault
        .create_credential_with_id(create_request, encrypted_payload, credential_id)
        .map_err(|e| ApiError::new("internal_error", e.to_string()))?;

    let tee_snapshot = tee_runtime_snapshot(&state).await;
    state.audit_logger.log_credential_created(
        &token.tenant_id,
        &token.user_id,
        entry.credential_id.as_str(),
        &token.token_id,
        &tee_snapshot.mrenclave_label,
    );

    let response = CreateCredentialResponse {
        credential_id: entry.credential_id.as_str().to_string(),
        service_id: entry.service_id.as_str().to_string(),
        credential_type: entry.credential_type.as_str().to_string(),
        created_at: entry.created_at.to_string(),
        expires_at: entry.expires_at.map(|t| t.to_string()),
        provider: entry.provider.map(|provider| provider.as_str().to_string()),
        allowed_domains: entry.allowed_domains.clone(),
        custom_functions: entry.custom_functions.clone(),
    };

    Ok((StatusCode::CREATED, Json(response)))
}

/// 在 TEE 内加密凭证
///
/// 修复说明：
/// - 接收预生成的 credential_id，确保加密和存储使用相同的 ID
/// - 使用 user_id.hash() 派生 L2 密钥，与解密流程一致
/// - 使用 user_id.hash() 构建 AAD，与解密流程一致
async fn encrypt_credential_in_tee(
    state: &AppState,
    tenant_id: &str,
    user_id: &UserId,
    credential_id: &CredentialId,
    plaintext: &serde_json::Value,
) -> Result<EncryptedPayload, String> {
    let tee_snapshot = tee_runtime_snapshot(state).await;
    let plaintext_bytes =
        serde_json::to_vec(plaintext).map_err(|e| format!("明文序列化失败: {e}"))?;
    let context = CredentialCryptoContext::new(tenant_id, user_id.hash(), credential_id.as_str());
    let blob = if tee_snapshot.enclave_initialized {
        let mut enclave = state.enclave.lock().await;
        enclave
            .encrypt_credential(
                tenant_id,
                user_id.hash(),
                credential_id.as_str(),
                &plaintext_bytes,
            )
            .map_err(|e| format!("TEE 加密失败: {e}"))?
    } else {
        warn!(
            tenant_id,
            credential_id = credential_id.as_str(),
            tee_runtime_mode = tee_snapshot.execution_mode,
            "TEE 不可用，create credential 回退到软件密钥路径"
        );

        let hierarchy = state.key_hierarchy.read().await;
        context
            .encrypt_with_hierarchy(&hierarchy, &plaintext_bytes)
            .map_err(|e| format!("软件路径加密失败: {e}"))?
    };

    Ok(EncryptedPayload::from_blob(&blob))
}

/// 凭证列表响应
#[derive(Debug, Serialize)]
pub struct ListCredentialsResponse {
    pub credentials: Vec<CredentialMetadata>,
    pub total: usize,
}

#[derive(Debug, Deserialize)]
pub struct ListCredentialsQuery {
    pub service_id: Option<String>,
    pub credential_type: Option<String>,
    pub only_valid: Option<bool>,
    #[serde(default = "default_page")]
    pub page: usize,
    #[serde(default = "default_page_size")]
    pub page_size: usize,
}

fn default_page() -> usize {
    1
}

fn default_page_size() -> usize {
    20
}

#[allow(clippy::result_large_err)]
fn parse_credential_type(value: &str) -> Result<CredentialType, ApiError> {
    match value {
        "username_password" => Ok(CredentialType::UsernamePassword),
        "oauth_refresh" | "oauth_token" | "o_auth_refresh" => Ok(CredentialType::OAuthRefresh),
        "api_key" => Ok(CredentialType::ApiKey),
        "session_cookie" => Ok(CredentialType::SessionCookie),
        "kyc_document" => Ok(CredentialType::KycDocument),
        "client_certificate" => Ok(CredentialType::ClientCertificate),
        "ssh_key" => Ok(CredentialType::SshKey),
        "database_connection" => Ok(CredentialType::DatabaseConnection),
        _ => Err(ApiError::new(
            "invalid_request",
            format!("不支持的凭证类型: {value}"),
        ).with_details(serde_json::json!({
            "field": "credential_type",
            "received": value,
            "allowed": ["username_password", "oauth_refresh", "oauth_token", "o_auth_refresh", "api_key", "session_cookie", "kyc_document", "client_certificate", "ssh_key", "database_connection"]
        }))),
    }
}

/// 获取当前 Unix 时间戳（秒）
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time before Unix epoch")
        .as_secs()
}

/// GET /api/v1/credentials - 获取凭证列表
pub async fn list_credentials(
    State(state): State<AppState>,
    Extension(token): Extension<ValidatedToken>,
    Query(query): Query<ListCredentialsQuery>,
) -> Result<Json<ListCredentialsResponse>, ApiError> {
    // 验证分页参数
    if query.page == 0 {
        return Err(ApiError::new("invalid_request", "page must be >= 1"));
    }
    if query.page_size == 0 || query.page_size > 100 {
        return Err(ApiError::new(
            "invalid_request",
            "page_size must be between 1 and 100",
        ));
    }

    // 验证 Scope: credential:read
    require_scope(TokenScope::CredentialRead)(&token).map_err(ApiError::from_auth_error)?;

    let tenant_id = TenantId::new(&token.tenant_id);
    let user_id = UserId::new(&token.user_id);

    let service_id = query
        .service_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ServiceId::new);
    let credential_type = query
        .credential_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(parse_credential_type)
        .transpose()?;

    let filter = CredentialFilter {
        service_id,
        credential_type,
        include_deleted: false,
        only_valid: query.only_valid.unwrap_or(false),
    };

    let result = state
        .vault
        .list_credentials(&tenant_id, &user_id, filter)
        .map_err(|e| ApiError::new("internal_error", e.to_string()))?;

    Ok(Json(ListCredentialsResponse {
        credentials: result.credentials,
        total: result.total,
    }))
}

/// 凭证详情响应
#[derive(Debug, Serialize)]
pub struct GetCredentialResponse {
    pub credential_id: String,
    pub service_id: String,
    pub credential_type: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub is_deleted: bool,
    pub status: String,
    pub provider: Option<String>,
    pub allowed_domains: Vec<String>,
    pub custom_functions: Vec<CredentialCustomFunction>,
}

/// GET /api/v1/credentials/:id - 获取凭证详情
pub async fn get_credential(
    State(state): State<AppState>,
    Extension(token): Extension<ValidatedToken>,
    Path(id): Path<String>,
) -> Result<Json<GetCredentialResponse>, ApiError> {
    // 验证 Scope: credential:read
    require_scope(TokenScope::CredentialRead)(&token).map_err(ApiError::from_auth_error)?;

    let credential_id = CredentialId::from_string(id.clone())
        .map_err(|e| ApiError::new("invalid_request", e.to_string()))?;

    let tenant_id = TenantId::new(&token.tenant_id);
    let user_id = UserId::new(&token.user_id);

    let metadata = state
        .vault
        .get_credential_metadata(&credential_id, &tenant_id, &user_id)
        .map_err(vault_error_to_api_error)?
        .ok_or_else(|| ApiError::new("not_found", "凭证不存在"))?;

    // 记录访问审计日志（jti 从 token_id 获取，mrenclave 软件模式固定值）
    state.audit_logger.log_credential_accessed(
        &token.tenant_id,
        &token.user_id,
        &metadata.credential_id,
        &token.token_id,
        "software_mode",
    );

    // 计算状态 (deleted > expired > active)
    let status = if metadata.is_deleted {
        "deleted".to_string()
    } else if metadata
        .expires_at
        .as_ref()
        .map(|exp| {
            chrono::DateTime::parse_from_rfc3339(exp)
                .map(|dt| dt.timestamp() as u64)
                .unwrap_or(0)
                < current_timestamp()
        })
        .unwrap_or(false)
    {
        "expired".to_string()
    } else {
        "active".to_string()
    };

    Ok(Json(GetCredentialResponse {
        credential_id: metadata.credential_id,
        service_id: metadata.service_id,
        credential_type: metadata.credential_type.as_str().to_string(),
        created_at: metadata.created_at,
        expires_at: metadata.expires_at,
        is_deleted: metadata.is_deleted,
        status,
        provider: metadata
            .provider
            .map(|provider| provider.as_str().to_string()),
        allowed_domains: metadata.allowed_domains,
        custom_functions: metadata.custom_functions,
    }))
}

/// 解密凭证请求
#[derive(Debug, Deserialize)]
pub struct DecryptCredentialRequest {
    /// 请求解密的理由（用于审计）
    pub reason: Option<String>,
}

/// 解密凭证响应
#[derive(Debug, Serialize)]
pub struct DecryptCredentialResponse {
    pub credential_id: String,
    pub service_id: String,
    pub credential_type: String,
    /// 解密的明文数据
    pub plaintext_data: serde_json::Value,
}

/// POST /api/v1/credentials/:id/decrypt - 解密凭证
pub async fn decrypt_credential_endpoint(
    State(_state): State<AppState>,
    Extension(_token): Extension<ValidatedToken>,
    Path(_id): Path<String>,
    Json(_request): Json<DecryptCredentialRequest>,
) -> Result<Json<DecryptCredentialResponse>, ApiError> {
    Err(ApiError::new(
        "forbidden",
        "Direct credential decryption is disabled; use sandbox execution instead",
    ))
}

/// 删除凭证响应
#[derive(Debug, Serialize)]
pub struct DeleteCredentialResponse {
    pub credential_id: String,
    pub deleted: bool,
}

/// DELETE /api/v1/credentials/:id - 删除凭证（软删除）
pub async fn delete_credential(
    State(state): State<AppState>,
    Extension(token): Extension<ValidatedToken>,
    Path(id): Path<String>,
) -> Result<Json<DeleteCredentialResponse>, ApiError> {
    // 验证 Scope: credential:write 或 admin
    require_any_scope(vec![TokenScope::CredentialWrite, TokenScope::Admin])(&token)
        .map_err(ApiError::from_auth_error)?;

    let credential_id = CredentialId::from_string(id.clone())
        .map_err(|e| ApiError::new("invalid_request", e.to_string()))?;

    let tenant_id = TenantId::new(&token.tenant_id);
    let user_id = UserId::new(&token.user_id);

    let deleted = state
        .vault
        .delete_credential(&credential_id, &tenant_id, &user_id)
        .map_err(vault_error_to_api_error)?;

    if !deleted {
        return Err(ApiError::new("not_found", "凭证不存在"));
    }

    // 记录审计日志（jti 从 token_id 获取，mrenclave 软件模式固定值）
    state.audit_logger.log_credential_deleted(
        &token.tenant_id,
        &token.user_id,
        credential_id.as_str(),
        &token.token_id,
        "software_mode",
    );

    Ok(Json(DeleteCredentialResponse {
        credential_id: credential_id.as_str().to_string(),
        deleted: true,
    }))
}

/// 更新凭证请求
#[derive(Debug, Deserialize)]
pub struct UpdateCredentialApiRequest {
    /// 新的明文凭证内容
    pub plaintext_data: serde_json::Value,
    /// 变更原因（用于审计）
    pub change_reason: Option<String>,
    /// Provider（显式传 null 可清除）
    #[serde(default)]
    pub provider: Option<Option<String>>,
    /// HTTP 请求白名单域名
    #[serde(default)]
    pub allowed_domains: Option<Vec<String>>,
    /// 自定义模板函数
    #[serde(default)]
    pub custom_functions: Option<Vec<CredentialCustomFunction>>,
}

/// 更新凭证响应
#[derive(Debug, Serialize)]
pub struct UpdateCredentialApiResponse {
    pub credential_id: String,
    pub version: u32,
    pub service_id: String,
    pub credential_type: String,
    pub updated_at: String,
    pub previous_version: u32,
    pub provider: Option<String>,
    pub allowed_domains: Vec<String>,
    pub custom_functions: Vec<CredentialCustomFunction>,
}

/// PUT /api/v1/credentials/:id - 更新凭证（创建新版本）
pub async fn update_credential(
    State(state): State<AppState>,
    Extension(token): Extension<ValidatedToken>,
    Path(id): Path<String>,
    payload: Result<Json<UpdateCredentialApiRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<UpdateCredentialApiResponse>, ApiError> {
    // 处理 JSON 反序列化错误（如必填字段缺失），返回 400 而不是默认的 422
    let Json(request) =
        payload.map_err(|e| ApiError::new("invalid_request", format!("请求体解析失败: {e}")))?;

    // 验证 Scope: credential:write
    require_scope(TokenScope::CredentialWrite)(&token).map_err(ApiError::from_auth_error)?;

    let credential_id = CredentialId::from_string(id.clone())
        .map_err(|e| ApiError::new("invalid_request", e.to_string()))?;
    let config_update = CredentialConfigUpdate {
        provider: request
            .provider
            .map(|provider| {
                provider
                    .map(|value| parse_credential_provider(&value))
                    .transpose()
            })
            .transpose()?,
        allowed_domains: request.allowed_domains.clone(),
        custom_functions: request.custom_functions.clone(),
    };
    if let Some(allowed_domains) = &config_update.allowed_domains {
        validate_allowed_domains(allowed_domains)
            .map_err(|error| ApiError::new("invalid_request", error.to_string()))?;
    }
    if let Some(custom_functions) = &config_update.custom_functions {
        validate_custom_functions(custom_functions)
            .map_err(|error| ApiError::new("invalid_request", error.to_string()))?;
    }

    let tenant_id = TenantId::new(&token.tenant_id);
    let user_id = UserId::new(&token.user_id);

    // 加密新的凭证内容
    let encrypted_payload = encrypt_credential_update(
        &state,
        tenant_id.as_str(),
        &user_id,
        &credential_id,
        &request.plaintext_data,
    )
    .await
    .map_err(|e| ApiError::new("internal_error", e))?;

    // 更新凭证
    let update_result = state
        .vault
        .update_credential_with_version(
            &credential_id,
            &tenant_id,
            &user_id,
            encrypted_payload,
            request.change_reason,
            Some(config_update),
        )
        .map_err(vault_error_to_api_error)?;
    let metadata = state
        .vault
        .get_credential_metadata(&credential_id, &tenant_id, &user_id)
        .map_err(vault_error_to_api_error)?
        .ok_or_else(|| ApiError::new("not_found", "凭证不存在"))?;

    Ok(Json(UpdateCredentialApiResponse {
        credential_id: id,
        version: update_result.new_version,
        service_id: update_result.service_id,
        credential_type: update_result.credential_type,
        updated_at: chrono::Utc::now().to_rfc3339(),
        previous_version: update_result.previous_version,
        provider: metadata
            .provider
            .map(|provider| provider.as_str().to_string()),
        allowed_domains: metadata.allowed_domains,
        custom_functions: metadata.custom_functions,
    }))
}

/// 加密更新后的凭证内容
///
/// 修复说明：
/// - 使用 user_id.hash() 派生 L2 密钥，与解密流程一致
/// - 使用 user_id.hash() 构建 AAD，与解密流程一致
async fn encrypt_credential_update(
    state: &AppState,
    tenant_id: &str,
    user_id: &UserId,
    credential_id: &CredentialId,
    plaintext: &serde_json::Value,
) -> Result<EncryptedPayload, String> {
    let tee_snapshot = tee_runtime_snapshot(state).await;
    let plaintext_bytes =
        serde_json::to_vec(plaintext).map_err(|e| format!("明文序列化失败: {e}"))?;
    let context = CredentialCryptoContext::new(tenant_id, user_id.hash(), credential_id.as_str());
    let blob = if tee_snapshot.enclave_initialized {
        let mut enclave = state.enclave.lock().await;
        enclave
            .encrypt_credential(
                tenant_id,
                user_id.hash(),
                credential_id.as_str(),
                &plaintext_bytes,
            )
            .map_err(|e| format!("TEE 更新加密失败: {e}"))?
    } else {
        warn!(
            tenant_id,
            credential_id = credential_id.as_str(),
            tee_runtime_mode = tee_snapshot.execution_mode,
            "TEE 不可用，update credential 回退到软件密钥路径"
        );

        let hierarchy = state.key_hierarchy.read().await;
        context
            .encrypt_with_hierarchy(&hierarchy, &plaintext_bytes)
            .map_err(|e| format!("软件路径更新加密失败: {e}"))?
    };

    Ok(EncryptedPayload::from_blob(&blob))
}

/// 构建凭证 API 路由
pub fn routes() -> axum::Router<AppState> {
    use axum::routing::{delete, get, post, put};

    axum::Router::new()
        .route("/credentials", post(create_credential))
        .route("/credentials", get(list_credentials))
        .route("/credentials/:id", get(get_credential))
        .route("/credentials/:id", put(update_credential))
        .route(
            "/credentials/:id/decrypt",
            post(decrypt_credential_endpoint),
        )
        .route("/credentials/:id", delete(delete_credential))
        .route(
            "/credentials/:id/versions",
            get(crate::api::versions::get_version_history),
        )
        .route(
            "/credentials/:id/versions/:version",
            get(crate::api::versions::get_version_detail),
        )
        .route(
            "/credentials/:id/rollback",
            post(crate::api::versions::rollback_credential),
        )
}

#[cfg(test)]
mod tests {
    #![allow(unused_imports)]
    use super::*;
    use crate::api::middleware::TokenScope;
    use crate::api::middleware::tests::create_mock_token;
    use serde_json::json;

    #[test]
    fn test_scope_checking() {
        // 测试 Scope 检查逻辑
        let token = create_mock_token(
            "tenant_123",
            "user_456",
            vec![TokenScope::CredentialRead, TokenScope::CredentialWrite],
        );

        assert!(token.has_scope(&TokenScope::CredentialRead));
        assert!(token.has_scope(&TokenScope::CredentialWrite));
        assert!(token.has_scope(&TokenScope::CredentialDecrypt));
        assert!(token.has_scope(&TokenScope::SandboxRead));
        assert!(token.has_scope(&TokenScope::SandboxWrite));
        assert!(token.has_scope(&TokenScope::SandboxExecute));
        assert!(!token.has_scope(&TokenScope::Admin));
    }

    #[test]
    fn test_admin_scope_has_all_permissions() {
        // Admin scope 应该拥有所有权限
        let token = create_mock_token("tenant_123", "user_456", vec![TokenScope::Admin]);

        assert!(token.has_scope(&TokenScope::CredentialRead));
        assert!(token.has_scope(&TokenScope::CredentialWrite));
        assert!(token.has_scope(&TokenScope::CredentialDecrypt));
    }

    #[test]
    fn test_any_scope_checking() {
        let token = create_mock_token("tenant_123", "user_456", vec![TokenScope::CredentialWrite]);

        assert!(token.has_any_scope(&[TokenScope::CredentialRead, TokenScope::CredentialWrite]));
        assert!(!token.has_any_scope(&[TokenScope::CredentialRead, TokenScope::CredentialDecrypt]));
    }

    #[test]
    fn test_create_request_accepts_oauth_aliases() {
        let payloads = [
            json!({
                "service_id": "github",
                "credential_type": "oauth_refresh",
                "plaintext_data": { "refreshToken": "rt_123" }
            }),
            json!({
                "service_id": "github",
                "credential_type": "oauth_token",
                "plaintext_data": { "refresh_token": "rt_legacy" }
            }),
            json!({
                "service_id": "github",
                "credential_type": "o_auth_refresh",
                "plaintext_data": { "refreshToken": "rt_weird" }
            }),
        ];

        for payload in payloads {
            let parsed: CreateCredentialApiRequest = serde_json::from_value(payload).unwrap();
            // 验证 credential_type 可以被成功解析为 OAuthRefresh
            let credential_type = parse_credential_type(&parsed.credential_type).unwrap();
            assert_eq!(credential_type, CredentialType::OAuthRefresh);
        }
    }

    #[test]
    fn test_normalize_oauth_plaintext_data_uses_refresh_token_canonical_key() {
        let normalized = normalize_oauth_plaintext_data(
            CredentialType::OAuthRefresh,
            &json!({
                "refresh_token": "rt_legacy",
                "note": "preserved"
            }),
        );

        assert_eq!(normalized["refreshToken"], "rt_legacy");
        assert!(normalized.get("refresh_token").is_none());
        assert_eq!(normalized["note"], "preserved");
    }

    #[test]
    fn test_normalize_oauth_plaintext_data_preserves_existing_canonical_key() {
        let normalized = normalize_oauth_plaintext_data(
            CredentialType::OAuthRefresh,
            &json!({
                "refreshToken": "rt_new",
                "refresh_token": "rt_old"
            }),
        );

        assert_eq!(normalized["refreshToken"], "rt_new");
        assert!(normalized.get("refresh_token").is_none());
    }

    #[test]
    fn test_validate_expires_at_rejects_past_and_current_timestamps() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        assert!(validate_expires_at(Some(now.saturating_sub(1))).is_err());
        assert!(validate_expires_at(Some(now)).is_err());
        assert!(validate_expires_at(Some(now + 1)).is_ok());
        assert!(validate_expires_at(None).is_ok());
    }

    #[test]
    fn test_create_request_accepts_value_alias_for_plaintext_data() {
        // BUG-18172: 自动化测试使用 "value" 字段代替 "plaintext_data"
        let payload = json!({
            "service_id": "api_tags_empty",
            "credential_type": "api_key",
            "value": "k1"
        });

        let parsed: CreateCredentialApiRequest = serde_json::from_value(payload).unwrap();
        assert_eq!(parsed.service_id, "api_tags_empty");
        assert_eq!(parsed.credential_type, "api_key");
        assert_eq!(parsed.plaintext_data, json!("k1"));
    }

    #[test]
    fn test_parse_create_credential_config_rejects_invalid_provider() {
        let error = parse_create_credential_config(Some("kraken".to_string()), None, None)
            .expect_err("unsupported provider should be rejected");

        assert_eq!(error.error, "invalid_request");
        assert!(error.message.contains("unsupported provider"));
    }

    #[test]
    fn test_parse_create_credential_config_rejects_invalid_allowed_domains() {
        let error = parse_create_credential_config(
            Some("okx".to_string()),
            Some(vec!["https://www.okx.com/path".to_string()]),
            None,
        )
        .expect_err("allowed_domains entries with paths should be rejected");

        assert_eq!(error.error, "invalid_request");
        assert!(error.message.contains("allowed_domains"));
    }
}
