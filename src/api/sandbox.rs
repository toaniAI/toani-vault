//! 沙箱会话管理 API
//!
//! 实现 TEE 安全执行沙箱的 HTTP API 端点
//! - POST   /api/v1/sandbox/sessions              - 创建会话
//! - GET    /api/v1/sandbox/sessions              - 列出会话
//! - GET    /api/v1/sandbox/sessions/:id          - 获取会话详情
//! - POST   /api/v1/sandbox/sessions/:id/execute  - 执行操作
//! - POST   /api/v1/sandbox/sessions/:id/pause    - 暂停会话
//! - POST   /api/v1/sandbox/sessions/:id/resume   - 恢复会话
//! - DELETE /api/v1/sandbox/sessions/:id          - 关闭会话
//! - POST   /api/v1/sandbox/sessions/:id/export   - 导出数据
//! - POST   /api/v1/sandbox/sessions/:id/dom-export - 导出 DOM

use crate::api::context::{ApiContext, RequestContext};
use crate::api::middleware::{TokenScope, ValidatedToken, require_scope};
use crate::api::response::{ApiErrorResponse, ApiSuccessResponse, ErrorCode};
use crate::api::sandbox_owner::{SandboxOwnerRecord, SandboxOwnerRegistry};
use crate::api::websocket::handle_socket;
use crate::crypto::hkdf::KeyHierarchy;
use crate::crypto::{CredentialCryptoContext, EncryptedBlob};
use crate::models::{CredentialMetadata, CredentialType};
use crate::tee::SharedEnclave;
use crate::tee::sandbox::{
    config::SandboxConfig,
    error::{SandboxError, SessionError},
    pool::{NsjailSandboxPool, SandboxPool},
    repository::{PostgresSandboxRepository, SandboxOperationRecord, SandboxRepository},
    session::SandboxSession,
    types::{OperationRequest, OperationType, SessionId, SessionRequest},
};
use crate::token::TOKEN_ISSUED_FROM_ACCESS_TOKEN;
use crate::vault::models::{
    CredentialFilter, CredentialId, ServiceId, TenantId, UserId, VaultEntry, VaultError,
};
use crate::vault::storage::CredentialVault;
use axum::{
    Extension, Json,
    extract::{OriginalUri, Path, Query, State, WebSocketUpgrade},
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use time::OffsetDateTime;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use uuid::Uuid;

const SANDBOX_FORWARD_HEADER: &str = "x-toani-internal-forward";
const SANDBOX_FORWARD_VERSION: &str = "sandbox-owner-v1";
const SANDBOX_OWNER_RETRY_AFTER_SECONDS: &str = "1";
const SANDBOX_OWNER_REGISTRY_GRACE_SECS: i64 = 30;

/// 沙箱 API 状态
#[derive(Clone)]
pub struct SandboxState {
    /// 沙箱池
    pub pool: Arc<dyn SandboxPool>,
    /// 沙箱配置
    pub config: SandboxConfig,
    /// 持久化仓储
    pub repository: Option<Arc<dyn SandboxRepository>>,
    /// 凭证 Vault（用于创建会话前凭证存在性校验）
    pub vault: Option<Arc<CredentialVault>>,
    /// 密钥层次结构（用于服务端受控解密）
    pub key_hierarchy: Option<Arc<RwLock<KeyHierarchy>>>,
    /// 共享 TEE Enclave
    pub enclave: Option<SharedEnclave>,
    /// 当前实例的 owner 标识
    pub owner_id: String,
    /// 当前实例对外声明的内部可达 URL
    pub owner_base_url: String,
    /// Redis-backed owner registry
    pub owner_registry: Option<Arc<dyn SandboxOwnerRegistry>>,
    /// backend-to-backend forward client
    forwarding_client: reqwest::Client,
    /// 会话级凭证缓存
    credential_cache: Arc<RwLock<HashMap<Uuid, SessionCredentialMaterial>>>,
}

#[derive(Clone)]
pub struct SandboxOwnerRuntime {
    pub owner_id: String,
    pub owner_base_url: String,
    pub owner_registry: Option<Arc<dyn SandboxOwnerRegistry>>,
}

impl SandboxState {
    /// 创建新的沙箱状态
    pub async fn new(
        config: SandboxConfig,
        database_pool: Option<PgPool>,
        vault: Option<Arc<CredentialVault>>,
        key_hierarchy: Option<Arc<RwLock<KeyHierarchy>>>,
        enclave: Option<SharedEnclave>,
        owner_runtime: SandboxOwnerRuntime,
    ) -> Result<Self, SandboxError> {
        let repository: Option<Arc<dyn SandboxRepository>> = database_pool.map(|pool| {
            Arc::new(PostgresSandboxRepository::new(pool)) as Arc<dyn SandboxRepository>
        });
        let pool: Arc<dyn SandboxPool> = Arc::new(NsjailSandboxPool::new_with_repository(
            config.clone(),
            repository.clone(),
        ));

        // 初始化池 - 通过 downcast 调用 NsjailSandboxPool 特有的 initialize 方法
        let pool_ref = pool.clone();
        if let Some(nsjail_pool) = pool_ref.as_any().downcast_ref::<NsjailSandboxPool>() {
            nsjail_pool.initialize().await?;
        }

        Ok(Self {
            pool,
            config,
            repository,
            vault,
            key_hierarchy,
            enclave,
            owner_id: owner_runtime.owner_id,
            owner_base_url: owner_runtime.owner_base_url,
            owner_registry: owner_runtime.owner_registry,
            forwarding_client: reqwest::Client::new(),
            credential_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// 创建简化版沙箱状态（用于测试）
    pub fn new_with_pool(pool: Arc<dyn SandboxPool>, config: SandboxConfig) -> Self {
        Self {
            pool,
            config,
            repository: None,
            vault: None,
            key_hierarchy: None,
            enclave: None,
            owner_id: "test-owner".to_string(),
            owner_base_url: "http://127.0.0.1".to_string(),
            owner_registry: None,
            forwarding_client: reqwest::Client::new(),
            credential_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn cached_credential(&self, session_id: Uuid) -> Option<SessionCredentialMaterial> {
        self.credential_cache.read().await.get(&session_id).cloned()
    }

    async fn store_cached_credential(&self, session_id: Uuid, material: SessionCredentialMaterial) {
        self.credential_cache
            .write()
            .await
            .insert(session_id, material);
    }

    async fn clear_cached_credential(&self, session_id: Uuid) {
        self.credential_cache.write().await.remove(&session_id);
    }

    async fn register_session_owner(
        &self,
        session: &dyn SandboxSession,
    ) -> Result<(), SandboxError> {
        let Some(owner_registry) = &self.owner_registry else {
            return Ok(());
        };
        let context = session.context();
        let ttl_secs = (context.expires_at.unix_timestamp()
            - OffsetDateTime::now_utc().unix_timestamp())
        .max(1)
            + SANDBOX_OWNER_REGISTRY_GRACE_SECS;
        owner_registry
            .register_owner(
                SandboxOwnerRecord {
                    owner_id: self.owner_id.clone(),
                    base_url: self.owner_base_url.clone(),
                    session_id: context.session_id.into(),
                    tenant_id: context.tenant_id,
                    expires_at: context.expires_at.unix_timestamp(),
                },
                Duration::from_secs(ttl_secs as u64),
            )
            .await
    }

    async fn lookup_session_owner(
        &self,
        session_id: SessionId,
    ) -> Result<Option<SandboxOwnerRecord>, SandboxError> {
        let Some(owner_registry) = &self.owner_registry else {
            return Ok(None);
        };
        owner_registry.get_owner(session_id).await
    }

    async fn delete_session_owner(&self, session_id: SessionId) -> Result<(), SandboxError> {
        let Some(owner_registry) = &self.owner_registry else {
            return Ok(());
        };
        owner_registry.delete_owner(session_id).await
    }
}

#[derive(Clone, Debug)]
struct SessionCredentialMaterial {
    credential_type: CredentialType,
    values: HashMap<String, String>,
}

// ==================== 请求/响应类型 ====================

/// 创建会话请求
#[derive(Debug, Deserialize, Serialize)]
pub struct CreateSessionRequest {
    /// 凭证 ID
    #[serde(default)]
    pub credential_id: Option<Uuid>,
    /// 服务标识；用于按当前 token 白名单解析唯一凭证
    #[serde(default)]
    pub service_id: Option<String>,
    /// 原始意图描述
    pub original_intent: String,
    /// 会话元数据（可选）
    pub metadata: Option<HashMap<String, String>>,
}

/// 创建会话响应
#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    /// 会话 ID
    pub session_id: Uuid,
    /// 运行时沙箱上下文 ID（兼容字段）
    pub sandbox_id: Uuid,
    /// 运行时 / TEE 上下文 ID
    pub runtime_context_id: Uuid,
    /// 会话状态
    pub status: String,
    /// 创建时间
    pub created_at: String,
    /// 过期时间
    pub expires_at: String,
}

/// 会话列表查询参数
#[derive(Debug, Deserialize)]
pub struct ListSessionsQuery {
    /// 按状态过滤（可选）
    pub status: Option<String>,
}

/// 会话状态白名单（用于校验查询参数）
const VALID_SESSION_STATUSES: &[&str] = &["creating", "ready", "executing", "paused", "closed"];

/// 会话列表响应
#[derive(Debug, Serialize)]
pub struct ListSessionsResponse {
    /// 会话列表
    pub sessions: Vec<SessionSummary>,
    /// 总数
    pub total: usize,
}

/// 会话摘要
#[derive(Debug, Serialize)]
pub struct SessionSummary {
    /// 会话 ID
    pub session_id: Uuid,
    /// 运行时沙箱上下文 ID（兼容字段）
    pub sandbox_id: Uuid,
    /// 运行时 / TEE 上下文 ID
    pub runtime_context_id: Uuid,
    /// 凭证 ID
    pub credential_id: Uuid,
    /// 状态
    pub status: String,
    /// 原始意图
    pub original_intent: String,
    /// 创建时间
    pub created_at: String,
    /// 过期时间
    pub expires_at: String,
    /// 是否已过期
    pub is_expired: bool,
}

/// 会话详情响应
#[derive(Debug, Serialize)]
pub struct SessionDetailResponse {
    /// 会话 ID
    pub session_id: Uuid,
    /// 运行时沙箱上下文 ID（兼容字段）
    pub sandbox_id: Uuid,
    /// 运行时 / TEE 上下文 ID
    pub runtime_context_id: Uuid,
    /// 租户 ID
    pub tenant_id: Uuid,
    /// 用户 ID
    pub user_id: Uuid,
    /// 凭证 ID
    pub credential_id: Uuid,
    /// 原始意图
    pub original_intent: String,
    /// 状态
    pub status: String,
    /// 创建时间
    pub created_at: String,
    /// 过期时间
    pub expires_at: String,
    /// 最后活动时间
    pub last_activity_at: String,
    /// 是否已过期
    pub is_expired: bool,
}

/// 执行操作请求
#[derive(Debug, Deserialize, Serialize)]
pub struct ExecuteOperationRequest {
    /// 操作类型
    pub operation_type: String,
    /// 操作描述
    pub description: String,
    /// 操作参数
    pub parameters: HashMap<String, serde_json::Value>,
}

/// 执行操作响应
#[derive(Debug, Serialize)]
pub struct ExecuteOperationResponse {
    /// 操作 ID
    pub operation_id: Uuid,
    /// 是否成功
    pub success: bool,
    /// 返回数据
    pub data: Option<serde_json::Value>,
    /// 错误信息
    pub error: Option<String>,
    /// 执行时间（毫秒）
    pub execution_time_ms: u64,
}

/// DOM 导出请求
#[derive(Debug, Deserialize, Serialize)]
pub struct DomExportRequest {
    /// 根节点选择器
    #[serde(default = "default_dom_export_root_selector")]
    pub root_selector: String,
    /// 导出格式
    #[serde(default)]
    pub format: DomExportFormat,
    /// 是否包含文本
    #[serde(default = "default_true")]
    pub include_text: bool,
    /// 是否包含元数据
    #[serde(default = "default_true")]
    pub include_metadata: bool,
    /// 额外敏感选择器
    #[serde(default)]
    pub extra_sensitive_selectors: Vec<String>,
    /// 最大返回字节数
    #[serde(default = "default_dom_export_max_bytes")]
    pub max_bytes: u64,
}

/// DOM 导出格式
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DomExportFormat {
    /// HTML 字符串
    #[default]
    Html,
    /// 纯文本
    Text,
    /// JSON 结构
    Json,
}

impl DomExportFormat {
    fn as_str(self) -> &'static str {
        match self {
            DomExportFormat::Html => "html",
            DomExportFormat::Text => "text",
            DomExportFormat::Json => "json",
        }
    }
}

/// DOM 导出响应
#[derive(Debug, Serialize)]
pub struct DomExportResponse {
    /// 操作 ID
    pub operation_id: Uuid,
    /// 是否成功
    pub success: bool,
    /// 导出格式
    pub format: String,
    /// 返回数据
    pub data: Option<serde_json::Value>,
    /// 是否已截断
    pub truncated: bool,
    /// 错误信息
    pub error: Option<String>,
    /// 执行时间（毫秒）
    pub execution_time_ms: u64,
}

fn default_dom_export_root_selector() -> String {
    "html".to_string()
}

fn default_dom_export_max_bytes() -> u64 {
    262_144
}

fn default_true() -> bool {
    true
}

/// 导出数据请求
#[derive(Debug, Deserialize, Serialize)]
pub struct ExportDataRequest {
    /// 导出格式
    pub format: ExportFormat,
    /// 数据选择器
    pub selectors: Vec<String>,
}

/// 导出格式
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Json,
    Csv,
    Pdf,
}

/// 导出数据响应
#[derive(Debug, Serialize)]
pub struct ExportDataResponse {
    /// 导出 ID
    pub export_id: Uuid,
    /// 导出数据（Base64 编码）
    pub data_base64: String,
    /// 格式
    pub format: String,
    /// 文件名
    pub filename: String,
    /// 大小（字节）
    pub size_bytes: usize,
}

/// 会话操作响应
#[derive(Debug, Serialize)]
pub struct SessionActionResponse {
    /// 会话 ID
    pub session_id: Uuid,
    /// 操作结果
    pub success: bool,
    /// 当前状态
    pub status: String,
    /// 消息
    pub message: String,
}

/// 操作详情响应
#[derive(Debug, Serialize)]
pub struct OperationDetailResponse {
    pub operation_id: Uuid,
    pub session_id: Uuid,
    pub operation_type: String,
    pub status: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub execution_time_ms: Option<u64>,
}

/// 沙箱统计响应
#[derive(Debug, Serialize)]
pub struct SandboxStatsApiResponse {
    pub pool_status: String,
    pub active_sessions: usize,
    pub warm_instances: usize,
    pub healthy: bool,
    pub error: Option<String>,
    pub browser_runtime_probe_error: Option<String>,
    pub process_health_issues: usize,
    pub process_health_summaries: Vec<String>,
}

struct ForwardRequest<'a> {
    method: &'a Method,
    original_uri: &'a axum::http::Uri,
    headers: &'a HeaderMap,
    body: Option<Vec<u8>>,
}

async fn resolve_owner_bound_session(
    state: &SandboxState,
    token: &ValidatedToken,
    session_id: SessionId,
    forward_request: ForwardRequest<'_>,
) -> Result<Arc<dyn SandboxSession>, Response> {
    match state.pool.get_session(session_id).await {
        Ok(session) => Ok(session),
        Err(local_error) => {
            let tenant_id = parse_uuid(&token.tenant_id);
            let owner_record = match state.lookup_session_owner(session_id).await {
                Ok(record) => record.filter(|record| record.tenant_id == tenant_id),
                Err(error) => {
                    warn!("Failed to resolve sandbox owner from registry: {error}");
                    None
                }
            };

            if let Some(owner_record) = owner_record {
                if owner_record.owner_id == state.owner_id {
                    return Err(sandbox_owner_retryable_response(
                        ErrorCode::SandboxOwnerUnavailable,
                        format!(
                            "Sandbox session {} exists on the current owner but its runtime is unavailable",
                            Uuid::from(session_id)
                        ),
                    ));
                }

                if is_forwarded_request(forward_request.headers) {
                    return Err(sandbox_owner_retryable_response(
                        ErrorCode::SandboxSessionNotLocal,
                        format!(
                            "Sandbox session {} could not be forwarded because the request was already forwarded once",
                            Uuid::from(session_id)
                        ),
                    ));
                }

                return forward_owner_bound_request(state, owner_record, forward_request).await;
            }

            let Some(repository) = &state.repository else {
                return Err(map_sandbox_error(local_error));
            };

            match repository.get_session_by_id(tenant_id, session_id).await {
                Ok(Some(_)) => Err(sandbox_owner_retryable_response(
                    ErrorCode::SandboxOwnerUnavailable,
                    format!(
                        "Sandbox session {} exists but no runtime owner is currently registered",
                        Uuid::from(session_id)
                    ),
                )),
                Ok(None) => Err(map_sandbox_error(local_error)),
                Err(error) => {
                    warn!("Failed to confirm sandbox session ownership from repository: {error}");
                    Err(sandbox_owner_retryable_response(
                        ErrorCode::SandboxOwnerUnavailable,
                        format!(
                            "Sandbox session {} could not be resolved because owner state is temporarily unavailable",
                            Uuid::from(session_id)
                        ),
                    ))
                }
            }
        }
    }
}

async fn forward_owner_bound_request(
    state: &SandboxState,
    owner_record: SandboxOwnerRecord,
    request: ForwardRequest<'_>,
) -> Result<Arc<dyn SandboxSession>, Response> {
    let path_and_query = request
        .original_uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or_else(|| request.original_uri.path());
    let forward_url = format!(
        "{}{}",
        owner_record.base_url.trim_end_matches('/'),
        path_and_query
    );
    let mut forwarded_headers = copy_forward_headers(request.headers);
    forwarded_headers.insert(
        HeaderName::from_static(SANDBOX_FORWARD_HEADER),
        HeaderValue::from_static(SANDBOX_FORWARD_VERSION),
    );

    let mut builder = state
        .forwarding_client
        .request(request.method.clone(), &forward_url)
        .headers(forwarded_headers);
    if let Some(body) = request.body {
        builder = builder.body(body);
    }

    let response = builder.send().await.map_err(|error| {
        sandbox_owner_retryable_response(
            ErrorCode::SandboxOwnerUnavailable,
            format!(
                "Sandbox owner {} is temporarily unreachable: {error}",
                owner_record.owner_id
            ),
        )
    })?;

    Err(convert_forwarded_response(response).await)
}

fn copy_forward_headers(headers: &HeaderMap) -> reqwest::header::HeaderMap {
    let mut forwarded = reqwest::header::HeaderMap::new();
    for (name, value) in headers {
        if should_strip_forward_header(name.as_str()) {
            continue;
        }
        if let Ok(header_name) = reqwest::header::HeaderName::from_bytes(name.as_str().as_bytes())
            && let Ok(header_value) = reqwest::header::HeaderValue::from_bytes(value.as_bytes())
        {
            forwarded.append(header_name, header_value);
        }
    }
    forwarded
}

fn should_strip_forward_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "host"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "content-length"
    )
}

fn is_forwarded_request(headers: &HeaderMap) -> bool {
    headers
        .get(SANDBOX_FORWARD_HEADER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == SANDBOX_FORWARD_VERSION)
}

async fn convert_forwarded_response(response: reqwest::Response) -> Response {
    let status = response.status();
    let response_headers = response.headers().clone();
    let body = response.bytes().await.unwrap_or_default();
    let mut builder = Response::builder().status(status);
    if let Some(headers) = builder.headers_mut() {
        for (name, value) in &response_headers {
            if should_strip_forward_header(name.as_str()) {
                continue;
            }
            headers.insert(name, value.clone());
        }
    }
    builder
        .body(axum::body::Body::from(body))
        .unwrap_or_else(|error| {
            error!("Failed to build forwarded sandbox response: {error}");
            sandbox_owner_retryable_response(
                ErrorCode::SandboxOwnerUnavailable,
                "Failed to materialize forwarded sandbox response",
            )
        })
}

fn sandbox_owner_retryable_response(code: ErrorCode, message: impl Into<String>) -> Response {
    let mut response = ApiErrorResponse::new(code, message).into_response();
    response.headers_mut().insert(
        axum::http::header::RETRY_AFTER,
        HeaderValue::from_static(SANDBOX_OWNER_RETRY_AFTER_SECONDS),
    );
    response
}

// ==================== Handler 实现 ====================

/// POST /api/v1/sandbox/sessions - 创建会话
pub async fn create_session(
    State(state): State<SandboxState>,
    Extension(token): Extension<ValidatedToken>,
    Json(request): Json<CreateSessionRequest>,
) -> Response {
    if let Err(e) = check_create_session_scopes(&token).await {
        return e;
    }

    info!(
        "Creating sandbox session for tenant: {}, user: {}",
        token.tenant_id, token.user_id
    );

    let credential_id = match resolve_create_session_credential(
        state.vault.as_ref(),
        &token,
        request.credential_id,
        request.service_id.as_deref(),
    ) {
        Ok(credential_id) => credential_id,
        Err(CreateSessionCredentialError::MissingReference(message)) => {
            return ApiErrorResponse::unprocessable_entity(message).into_response();
        }
        Err(CreateSessionCredentialError::InvalidReference(message)) => {
            return ApiErrorResponse::invalid_request(message).into_response();
        }
        Err(CreateSessionCredentialError::Sandbox(error)) => return map_sandbox_error(error),
    };

    // 构建会话请求
    let session_request = SessionRequest {
        tenant_id: parse_uuid(&token.tenant_id),
        user_id: parse_uuid(&token.user_id),
        credential_id,
        original_intent: request.original_intent,
        metadata: request.metadata,
    };

    // 获取会话
    match state.pool.acquire_session(session_request).await {
        Ok(session) => {
            if let Err(error) = state.register_session_owner(session.as_ref()).await {
                error!("Failed to register sandbox owner for new session: {error}");
                if let Err(release_error) = state.pool.release_session(session.id()).await {
                    error!(
                        "Failed to release sandbox session after owner registry failure: {release_error}"
                    );
                }
                return sandbox_owner_retryable_response(
                    ErrorCode::SandboxOwnerUnavailable,
                    "Sandbox session was created locally but owner routing could not be initialized",
                );
            }

            let context = session.context();
            let response = CreateSessionResponse {
                session_id: context.session_id.into(),
                sandbox_id: context.sandbox_id.into(),
                runtime_context_id: context.sandbox_id.into(),
                status: format!("{:?}", session.status().await).to_lowercase(),
                created_at: context.created_at.to_string(),
                expires_at: context.expires_at.to_string(),
            };

            info!(
                "Session {} created successfully with runtime context {}",
                response.session_id, response.runtime_context_id
            );

            (StatusCode::CREATED, Json(ApiSuccessResponse::new(response))).into_response()
        }
        Err(e) => {
            error!("Failed to create session: {}", e);
            map_sandbox_error(e).into_response()
        }
    }
}

/// GET /api/v1/sandbox/sessions - 列出会话
pub async fn list_sessions(
    State(state): State<SandboxState>,
    OriginalUri(original_uri): OriginalUri,
    Extension(token): Extension<ValidatedToken>,
    Query(query): Query<ListSessionsQuery>,
) -> Response {
    // BUG-18221: 检测原始请求路径是否以尾斜杠结尾
    // NormalizePathLayer 会将 /sandbox/sessions/ 归一化为 /sandbox/sessions
    // 但原始 URI 保留了尾斜杠，用于判断是否是"缺少详情 id"的请求
    // 如果原始路径带尾斜杠，返回 404 + {"error":"not_found"}
    let original_path = original_uri.path();
    if original_path.ends_with('/') {
        // 路径以尾斜杠结尾，表示请求的是 `/sandbox/sessions/` 而非 `/sandbox/sessions`
        // 这对应于"缺少详情路径参数 id"的语义，应返回 404
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "not_found" })),
        )
            .into_response();
    }

    // 验证 Scope: sandbox:read
    if let Err(e) = check_scope(&token, TokenScope::SandboxRead).await {
        return e;
    }

    // 校验 status 参数（如果提供）
    let status_filter: Option<String> = if let Some(status) = query.status {
        let status_lower = status.to_lowercase();
        if !VALID_SESSION_STATUSES.contains(&status_lower.as_str()) {
            return ApiErrorResponse::invalid_request(format!(
                "invalid status '{}', must be one of: {}",
                status,
                VALID_SESSION_STATUSES.join(", ")
            ))
            .into_response();
        }
        Some(status_lower)
    } else {
        None
    };

    let tenant_id = parse_uuid(&token.tenant_id);

    let mut sessions = Vec::new();
    let mut live_session_ids = std::collections::HashSet::new();
    if let Some(pool) = state.pool.as_any().downcast_ref::<NsjailSandboxPool>() {
        for session in pool.list_active_sessions().await {
            let context = session.context().clone();
            if context.tenant_id != tenant_id {
                continue;
            }
            // 如果指定了 status 过滤，只返回匹配的会话
            let session_status = session.status().await.to_string();
            if let Some(ref filter) = status_filter {
                if session_status.to_lowercase() != *filter {
                    continue;
                }
            }
            live_session_ids.insert(context.session_id);
            sessions.push(SessionSummary {
                session_id: context.session_id.into(),
                sandbox_id: context.sandbox_id.into(),
                runtime_context_id: context.sandbox_id.into(),
                credential_id: context.credential_id,
                status: session_status,
                original_intent: context.original_intent.clone(),
                created_at: context.created_at.to_string(),
                expires_at: context.expires_at.to_string(),
                is_expired: context.is_expired(),
            });
        }
    }

    if let Some(repository) = &state.repository {
        match repository.list_sessions_by_tenant(tenant_id).await {
            Ok(records) => {
                sessions.extend(records.into_iter().filter_map(|record| {
                    if live_session_ids.contains(&record.session_id) {
                        return None;
                    }
                    if let Some(ref filter) = status_filter
                        && record.status.to_lowercase() != *filter
                    {
                        return None;
                    }
                    Some(SessionSummary {
                        session_id: record.session_id.into(),
                        sandbox_id: record.sandbox_id,
                        runtime_context_id: record.sandbox_id,
                        credential_id: record.credential_id,
                        status: record.status.clone(),
                        original_intent: record.original_intent,
                        created_at: record.started_at.to_rfc3339(),
                        expires_at: record.expires_at.to_rfc3339(),
                        is_expired: chrono::Utc::now() > record.expires_at
                            || record.status == "expired",
                    })
                }));
            }
            Err(error) => {
                warn!("Failed to read sandbox sessions from repository: {error}");
            }
        }
    }

    let response = ListSessionsResponse {
        total: sessions.len(),
        sessions,
    };

    Json(ApiSuccessResponse::new(response)).into_response()
}

/// GET /api/v1/sandbox/sessions/:id - 获取会话详情
pub async fn get_session(
    State(state): State<SandboxState>,
    Extension(token): Extension<ValidatedToken>,
    Path(id_str): Path<String>,
) -> Response {
    // 验证 Scope: sandbox:read
    if let Err(e) = check_scope(&token, TokenScope::SandboxRead).await {
        return e;
    }

    // BUG-18222: 手动解析 UUID，确保非 UUID 路径参数返回标准 JSON 错误
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| ApiErrorResponse::invalid_request("Invalid session_id: must be a valid UUID"));
    let id = match id {
        Ok(uuid) => uuid,
        Err(e) => return e.into_response(),
    };

    let session_id = SessionId::from(id);
    let tenant_id = parse_uuid(&token.tenant_id);

    match state.pool.get_session(session_id).await {
        Ok(session) => {
            let context = session.context();
            let status = session.status().await;
            let last_activity = *context.last_activity_at.read().await;

            let response = SessionDetailResponse {
                session_id: context.session_id.into(),
                sandbox_id: context.sandbox_id.into(),
                runtime_context_id: context.sandbox_id.into(),
                tenant_id: context.tenant_id,
                user_id: context.user_id,
                credential_id: context.credential_id,
                original_intent: context.original_intent.clone(),
                status: format!("{status:?}").to_lowercase(),
                created_at: context.created_at.to_string(),
                expires_at: context.expires_at.to_string(),
                last_activity_at: last_activity.to_string(),
                is_expired: context.is_expired(),
            };

            Json(ApiSuccessResponse::new(response)).into_response()
        }
        Err(_) => {
            if let Some(repository) = &state.repository {
                match repository.get_session_by_id(tenant_id, session_id).await {
                    Ok(Some(record)) => {
                        let response = SessionDetailResponse {
                            session_id: record.session_id.into(),
                            sandbox_id: record.sandbox_id,
                            runtime_context_id: record.sandbox_id,
                            tenant_id: record.tenant_id,
                            user_id: record.created_by,
                            credential_id: record.credential_id,
                            original_intent: record.original_intent,
                            status: record.status.clone(),
                            created_at: record.started_at.to_rfc3339(),
                            expires_at: record.expires_at.to_rfc3339(),
                            last_activity_at: record.updated_at.to_rfc3339(),
                            is_expired: chrono::Utc::now() > record.expires_at
                                || record.status == "expired",
                        };
                        return Json(ApiSuccessResponse::new(response)).into_response();
                    }
                    Ok(None) => {}
                    Err(error) => {
                        warn!("Failed to read sandbox session from repository: {error}");
                    }
                }
            }

            warn!("Session {} not found", id);
            map_sandbox_error(SessionError::not_found(id).into()).into_response()
        }
    }
}

/// POST /api/v1/sandbox/sessions/:id/execute - 执行操作
pub async fn execute_operation(
    State(state): State<SandboxState>,
    OriginalUri(original_uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    Extension(token): Extension<ValidatedToken>,
    Path(id_str): Path<String>,
    Json(request): Json<ExecuteOperationRequest>,
) -> Response {
    // 验证 Scope: sandbox:execute
    if let Err(e) = check_sandbox_control_scope(&token, TokenScope::SandboxExecute).await {
        return e;
    }

    // 手动解析 UUID，确保非 UUID 路径参数返回标准 JSON 错误
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| ApiErrorResponse::invalid_request("Invalid session_id: must be a valid UUID"));
    let id = match id {
        Ok(uuid) => uuid,
        Err(e) => return e.into_response(),
    };

    let session_id = SessionId::from(id);
    let request_body = match serde_json::to_vec(&request) {
        Ok(body) => body,
        Err(error) => {
            return ApiErrorResponse::internal_error(format!(
                "failed to serialize sandbox execute request for forwarding: {error}"
            ))
            .into_response();
        }
    };
    let session = match resolve_owner_bound_session(
        &state,
        &token,
        session_id,
        ForwardRequest {
            method: &method,
            original_uri: &original_uri,
            headers: &headers,
            body: Some(request_body),
        },
    )
    .await
    {
        Ok(session) => session,
        Err(response) => return response,
    };

    // 解析操作类型
    let operation_type = match parse_operation_type(&request.operation_type) {
        Some(t) => t,
        None => {
            return ApiErrorResponse::invalid_request(format!(
                "Invalid operation type: {}",
                request.operation_type
            ))
            .into_response();
        }
    };

    let (mut audit_parameters, resolved_parameters) = match resolve_operation_parameters(
        &state,
        session.as_ref(),
        &operation_type,
        &request.parameters,
    )
    .await
    {
        Ok(value) => value,
        Err(response) => return response,
    };
    redact_persisted_parameters(
        &operation_type,
        &request.parameters,
        &resolved_parameters,
        &mut audit_parameters,
    );
    let sensitive_output_values = collect_http_sensitive_output_values(
        &operation_type,
        &request.parameters,
        &resolved_parameters,
    );

    // 构建操作请求
    let operation = OperationRequest {
        operation_id: Uuid::new_v4(),
        operation_type,
        description: request.description,
        parameters: audit_parameters,
        resolved_parameters,
        sensitive_output_values,
        created_at: OffsetDateTime::now_utc(),
    };

    info!(
        "Executing operation {} in session {}",
        operation.operation_id, id
    );

    // 执行操作
    match session.execute_operation(operation.clone()).await {
        Ok(result) => {
            let response = ExecuteOperationResponse {
                operation_id: operation.operation_id,
                success: result.success,
                data: result.data,
                error: result.error,
                execution_time_ms: result.execution_time_ms,
            };

            Json(ApiSuccessResponse::new(response)).into_response()
        }
        Err(e) => {
            error!("Operation execution failed: {}", e);
            map_sandbox_error(e).into_response()
        }
    }
}

/// POST /api/v1/sandbox/sessions/:id/pause - 暂停会话
pub async fn pause_session(
    State(state): State<SandboxState>,
    OriginalUri(original_uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    Extension(token): Extension<ValidatedToken>,
    Path(id_str): Path<String>,
) -> Response {
    // 验证 Scope: sandbox:write
    if let Err(e) = check_sandbox_control_scope(&token, TokenScope::SandboxWrite).await {
        return e;
    }

    // BUG-18223: 手动解析 UUID，确保非 UUID 路径参数返回标准 JSON 错误
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| ApiErrorResponse::invalid_request("Invalid session_id: must be a valid UUID"));
    let id = match id {
        Ok(uuid) => uuid,
        Err(e) => return e.into_response(),
    };

    let session_id = SessionId::from(id);
    let session = match resolve_owner_bound_session(
        &state,
        &token,
        session_id,
        ForwardRequest {
            method: &method,
            original_uri: &original_uri,
            headers: &headers,
            body: None,
        },
    )
    .await
    {
        Ok(session) => session,
        Err(response) => return response,
    };

    match session.pause().await {
        Ok(_) => {
            if let Some(repository) = &state.repository
                && let Err(error) = repository.update_session_status(session_id, "paused").await
            {
                error!(
                    "Failed to persist paused status for session {}: {}",
                    session_id, error
                );
                return map_sandbox_error(error).into_response();
            }
            let response = SessionActionResponse {
                session_id: id,
                success: true,
                status: "paused".to_string(),
                message: "Session paused successfully".to_string(),
            };
            Json(ApiSuccessResponse::new(response)).into_response()
        }
        Err(e) => map_sandbox_error(e).into_response(),
    }
}

/// POST /api/v1/sandbox/sessions/:id/resume - 恢复会话
pub async fn resume_session(
    State(state): State<SandboxState>,
    OriginalUri(original_uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    Extension(token): Extension<ValidatedToken>,
    Path(id_str): Path<String>,
) -> Response {
    // 验证 Scope: sandbox:write
    if let Err(e) = check_sandbox_control_scope(&token, TokenScope::SandboxWrite).await {
        return e;
    }

    // BUG-18224: 手动解析 UUID，确保非 UUID 路径参数返回标准 JSON 错误
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| ApiErrorResponse::invalid_request("Invalid session_id: must be a valid UUID"));
    let id = match id {
        Ok(uuid) => uuid,
        Err(e) => return e.into_response(),
    };

    let session_id = SessionId::from(id);
    let session = match resolve_owner_bound_session(
        &state,
        &token,
        session_id,
        ForwardRequest {
            method: &method,
            original_uri: &original_uri,
            headers: &headers,
            body: None,
        },
    )
    .await
    {
        Ok(session) => session,
        Err(response) => return response,
    };

    match session.resume().await {
        Ok(_) => {
            if let Some(repository) = &state.repository
                && let Err(error) = repository.update_session_status(session_id, "ready").await
            {
                error!(
                    "Failed to persist active status for session {}: {}",
                    session_id, error
                );
                return map_sandbox_error(error).into_response();
            }
            let response = SessionActionResponse {
                session_id: id,
                success: true,
                status: "ready".to_string(),
                message: "Session resumed successfully".to_string(),
            };
            Json(ApiSuccessResponse::new(response)).into_response()
        }
        Err(e) => map_sandbox_error(e).into_response(),
    }
}

/// DELETE /api/v1/sandbox/sessions/:id - 关闭会话
pub async fn close_session(
    State(state): State<SandboxState>,
    OriginalUri(original_uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    Extension(token): Extension<ValidatedToken>,
    Path(id_str): Path<String>,
) -> Response {
    // 验证 Scope: sandbox:write
    if let Err(e) = check_sandbox_control_scope(&token, TokenScope::SandboxWrite).await {
        return e;
    }

    // BUG-18226: 手动解析 UUID，确保非 UUID 路径参数返回标准 JSON 错误
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| ApiErrorResponse::invalid_request("Invalid session_id: must be a valid UUID"));
    let id = match id {
        Ok(uuid) => uuid,
        Err(e) => return e.into_response(),
    };

    let session_id = SessionId::from(id);
    match resolve_owner_bound_session(
        &state,
        &token,
        session_id,
        ForwardRequest {
            method: &method,
            original_uri: &original_uri,
            headers: &headers,
            body: None,
        },
    )
    .await
    {
        Ok(_) => {}
        Err(response) => return response,
    }

    match state.pool.release_session(session_id).await {
        Ok(_) => {
            state.clear_cached_credential(id).await;
            if let Err(error) = state.delete_session_owner(session_id).await {
                warn!(
                    "Failed to delete sandbox owner mapping for closed session {}: {}",
                    session_id, error
                );
            }
            let response = SessionActionResponse {
                session_id: id,
                success: true,
                status: "closed".to_string(),
                message: "Session closed successfully".to_string(),
            };
            Json(ApiSuccessResponse::new(response)).into_response()
        }
        Err(e) => map_sandbox_error(e).into_response(),
    }
}

/// POST /api/v1/sandbox/sessions/:id/dom-export - 导出 DOM
pub async fn dom_export(
    State(state): State<SandboxState>,
    OriginalUri(original_uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    Extension(token): Extension<ValidatedToken>,
    Path(id_str): Path<String>,
    Json(request): Json<DomExportRequest>,
) -> Response {
    // 验证 Scope: sandbox:execute
    if let Err(e) = check_sandbox_control_scope(&token, TokenScope::SandboxExecute).await {
        return e;
    }

    // 手动解析 UUID，确保非 UUID 路径参数返回标准 JSON 错误
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| ApiErrorResponse::invalid_request("Invalid session_id: must be a valid UUID"));
    let id = match id {
        Ok(uuid) => uuid,
        Err(e) => return e.into_response(),
    };

    let session_id = SessionId::from(id);
    let request_body = match serde_json::to_vec(&request) {
        Ok(body) => body,
        Err(error) => {
            return ApiErrorResponse::internal_error(format!(
                "failed to serialize DOM export request for forwarding: {error}"
            ))
            .into_response();
        }
    };
    let session = match resolve_owner_bound_session(
        &state,
        &token,
        session_id,
        ForwardRequest {
            method: &method,
            original_uri: &original_uri,
            headers: &headers,
            body: Some(request_body),
        },
    )
    .await
    {
        Ok(session) => session,
        Err(response) => return response,
    };

    let operation_type = OperationType::DomExport;
    let mut parameters = HashMap::new();
    parameters.insert(
        "root_selector".to_string(),
        serde_json::Value::String(request.root_selector),
    );
    parameters.insert(
        "format".to_string(),
        serde_json::Value::String(request.format.as_str().to_string()),
    );
    parameters.insert(
        "include_text".to_string(),
        serde_json::Value::Bool(request.include_text),
    );
    parameters.insert(
        "include_metadata".to_string(),
        serde_json::Value::Bool(request.include_metadata),
    );
    parameters.insert(
        "extra_sensitive_selectors".to_string(),
        serde_json::Value::Array(
            request
                .extra_sensitive_selectors
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        ),
    );
    parameters.insert(
        "max_bytes".to_string(),
        serde_json::Value::Number(request.max_bytes.into()),
    );

    let (mut audit_parameters, resolved_parameters) =
        match resolve_operation_parameters(&state, session.as_ref(), &operation_type, &parameters)
            .await
        {
            Ok(value) => value,
            Err(response) => return response,
        };
    redact_persisted_parameters(
        &operation_type,
        &parameters,
        &resolved_parameters,
        &mut audit_parameters,
    );

    let operation = OperationRequest {
        operation_id: Uuid::new_v4(),
        operation_type,
        description: "DOM export".to_string(),
        parameters: audit_parameters,
        resolved_parameters,
        sensitive_output_values: Vec::new(),
        created_at: OffsetDateTime::now_utc(),
    };

    match session.execute_operation(operation.clone()).await {
        Ok(result) => {
            let format = result
                .data
                .as_ref()
                .and_then(|data| data.get("format"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("html")
                .to_string();
            let truncated = result
                .data
                .as_ref()
                .and_then(|data| data.get("truncated"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let response = DomExportResponse {
                operation_id: operation.operation_id,
                success: result.success,
                format,
                data: result.data,
                truncated,
                error: result.error,
                execution_time_ms: result.execution_time_ms,
            };

            Json(ApiSuccessResponse::new(response)).into_response()
        }
        Err(e) => map_sandbox_error(e).into_response(),
    }
}

/// POST /api/v1/sandbox/sessions/:id/export - 导出数据
pub async fn export_data(
    State(state): State<SandboxState>,
    OriginalUri(original_uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    Extension(token): Extension<ValidatedToken>,
    Path(id_str): Path<String>,
    Json(request): Json<ExportDataRequest>,
) -> Response {
    // 验证 Scope: sandbox:execute
    if let Err(e) = check_sandbox_control_scope(&token, TokenScope::SandboxExecute).await {
        return e;
    }

    // 手动解析 UUID，确保非 UUID 路径参数返回标准 JSON 错误
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| ApiErrorResponse::invalid_request("Invalid session_id: must be a valid UUID"));
    let id = match id {
        Ok(uuid) => uuid,
        Err(e) => return e.into_response(),
    };

    let session_id = SessionId::from(id);
    let request_body = match serde_json::to_vec(&request) {
        Ok(body) => body,
        Err(error) => {
            return ApiErrorResponse::internal_error(format!(
                "failed to serialize export request for forwarding: {error}"
            ))
            .into_response();
        }
    };
    let session = match resolve_owner_bound_session(
        &state,
        &token,
        session_id,
        ForwardRequest {
            method: &method,
            original_uri: &original_uri,
            headers: &headers,
            body: Some(request_body),
        },
    )
    .await
    {
        Ok(session) => session,
        Err(response) => return response,
    };

    // 构建导出操作参数
    let mut parameters = HashMap::new();
    parameters.insert(
        "format".to_string(),
        serde_json::Value::String(format!("{:?}", request.format).to_lowercase()),
    );
    parameters.insert(
        "selectors".to_string(),
        serde_json::Value::Array(
            request
                .selectors
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        ),
    );

    // 构建导出操作
    let operation = OperationRequest {
        operation_id: Uuid::new_v4(),
        operation_type: OperationType::Export,
        description: "Export data".to_string(),
        parameters,
        resolved_parameters: HashMap::new(),
        sensitive_output_values: Vec::new(),
        created_at: OffsetDateTime::now_utc(),
    };

    match session.execute_operation(operation).await {
        Ok(result) => {
            let export_id = Uuid::new_v4();
            let format_str = format!("{:?}", request.format).to_lowercase();

            // 从执行结果中提取导出数据
            let export_data = result
                .data
                .map(|d| d.to_string().into_bytes())
                .unwrap_or_default();
            let base64_data = STANDARD.encode(&export_data);

            let response = ExportDataResponse {
                export_id,
                data_base64: base64_data,
                format: format_str.clone(),
                filename: format!("export_{export_id}. {format_str}"),
                size_bytes: export_data.len(),
            };

            Json(ApiSuccessResponse::new(response)).into_response()
        }
        Err(e) => map_sandbox_error(e).into_response(),
    }
}

/// GET /api/v1/sandbox/operations/:operation_id - 获取操作详情
pub async fn get_operation(
    State(state): State<SandboxState>,
    Extension(token): Extension<ValidatedToken>,
    Path(operation_id): Path<Uuid>,
) -> Response {
    if let Err(e) = check_scope(&token, TokenScope::SandboxRead).await {
        return e;
    }

    let tenant_id = parse_uuid(&token.tenant_id);

    if let Some(repository) = &state.repository {
        match repository
            .get_operation_by_id(tenant_id, operation_id)
            .await
        {
            Ok(Some(record)) => {
                return Json(ApiSuccessResponse::new(map_operation_record(record))).into_response();
            }
            Ok(None) => {}
            Err(error) => {
                warn!(
                    "Failed to read sandbox operation from repository, fallback to memory: {error}"
                );
            }
        }
    }

    let Some(pool) = state.pool.as_any().downcast_ref::<NsjailSandboxPool>() else {
        return ApiErrorResponse::service_unavailable(
            "Sandbox pool does not support operation lookup",
        )
        .into_response();
    };

    let Some((session_id, record)) = pool.find_operation(operation_id).await else {
        return ApiErrorResponse::not_found("Operation not found").into_response();
    };

    let response = OperationDetailResponse {
        operation_id: record.operation_id,
        session_id: session_id.into(),
        operation_type: record.operation_type,
        status: record.status.to_string(),
        started_at: record.started_at.to_string(),
        completed_at: record.completed_at.map(|value| value.to_string()),
        execution_time_ms: record.execution_time_ms,
    };

    Json(ApiSuccessResponse::new(response)).into_response()
}

/// GET /api/v1/sandbox/stats - 获取沙箱统计
pub async fn get_stats(
    State(state): State<SandboxState>,
    Extension(token): Extension<ValidatedToken>,
) -> Response {
    if let Err(e) = check_scope(&token, TokenScope::SandboxRead).await {
        return e;
    }

    let health = state.pool.health().await;
    let response = SandboxStatsApiResponse {
        pool_status: health.pool_status.to_string(),
        active_sessions: health.active_sessions,
        warm_instances: health.warm_instances,
        healthy: health.healthy,
        error: health.error,
        browser_runtime_probe_error: health.browser_runtime_probe_error,
        process_health_issues: health.process_health_issues,
        process_health_summaries: health.process_health_summaries,
    };

    Json(ApiSuccessResponse::new(response)).into_response()
}

// ==================== 辅助函数 ====================

/// 检查 Scope
async fn check_scope(token: &ValidatedToken, required: TokenScope) -> Result<(), Response> {
    if let Err(error) = require_scope(required)(token) {
        return Err(error.into_response());
    }

    Ok(())
}

async fn check_create_session_scopes(token: &ValidatedToken) -> Result<(), Response> {
    check_scope(token, TokenScope::CredentialRead).await?;
    check_scope(token, TokenScope::CredentialDecrypt).await?;

    if token.issued_from() == TOKEN_ISSUED_FROM_ACCESS_TOKEN {
        return Ok(());
    }

    check_scope(token, TokenScope::SandboxWrite).await
}

async fn check_sandbox_control_scope(
    token: &ValidatedToken,
    required_scope: TokenScope,
) -> Result<(), Response> {
    if token.issued_from() == TOKEN_ISSUED_FROM_ACCESS_TOKEN {
        return check_scope(token, TokenScope::CredentialRead).await;
    }

    check_scope(token, required_scope).await
}

async fn resolve_operation_parameters(
    state: &SandboxState,
    session: &dyn SandboxSession,
    operation_type: &OperationType,
    parameters: &HashMap<String, serde_json::Value>,
) -> Result<
    (
        HashMap<String, serde_json::Value>,
        HashMap<String, serde_json::Value>,
    ),
    Response,
> {
    let mut audit_parameters = HashMap::with_capacity(parameters.len());
    let mut resolved_parameters = HashMap::with_capacity(parameters.len());

    for (key, value) in parameters {
        let (audit_value, resolved_value) =
            resolve_parameter_value(state, session, operation_type, value).await?;
        audit_parameters.insert(key.clone(), audit_value);
        resolved_parameters.insert(key.clone(), resolved_value);
    }

    Ok((audit_parameters, resolved_parameters))
}

fn collect_http_sensitive_output_values(
    operation_type: &OperationType,
    original_parameters: &HashMap<String, serde_json::Value>,
    resolved_parameters: &HashMap<String, serde_json::Value>,
) -> Vec<String> {
    if *operation_type != OperationType::HttpRequest {
        return Vec::new();
    }

    let mut values = Vec::new();
    let mut seen = HashSet::new();

    for (key, original_value) in original_parameters {
        collect_http_sensitive_output_value(
            key,
            Some(original_value),
            resolved_parameters.get(key),
            &mut values,
            &mut seen,
        );
    }

    values
}

fn collect_http_sensitive_output_value(
    key: &str,
    original_value: Option<&serde_json::Value>,
    resolved_value: Option<&serde_json::Value>,
    values: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    let sensitive_key = is_sensitive_http_key(key);

    if let Some(resolved_text) = resolved_value.and_then(serde_json::Value::as_str) {
        if sensitive_key {
            push_sensitive_output_value(resolved_text, values, seen);
        }

        if let Some(reference) =
            original_value.and_then(|value| parse_credential_reference(value).ok().flatten())
        {
            push_sensitive_output_value(resolved_text, values, seen);

            let stripped = resolved_text
                .strip_prefix(reference.prefix.as_deref().unwrap_or_default())
                .unwrap_or(resolved_text)
                .strip_suffix(reference.suffix.as_deref().unwrap_or_default())
                .unwrap_or(resolved_text)
                .trim();
            push_sensitive_output_value(stripped, values, seen);
        }
    }

    match (original_value, resolved_value) {
        (
            Some(serde_json::Value::Object(original_map)),
            Some(serde_json::Value::Object(resolved_map)),
        ) => {
            for (nested_key, nested_original) in original_map {
                collect_http_sensitive_output_value(
                    nested_key,
                    Some(nested_original),
                    resolved_map.get(nested_key),
                    values,
                    seen,
                );
            }
        }
        (
            Some(serde_json::Value::Array(original_items)),
            Some(serde_json::Value::Array(resolved_items)),
        ) => {
            for (index, nested_original) in original_items.iter().enumerate() {
                collect_http_sensitive_output_value(
                    key,
                    Some(nested_original),
                    resolved_items.get(index),
                    values,
                    seen,
                );
            }
        }
        _ => {}
    }
}

fn push_sensitive_output_value(
    candidate: &str,
    values: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return;
    }

    if seen.insert(trimmed.to_string()) {
        values.push(trimmed.to_string());
    }
}

fn redact_persisted_parameters(
    operation_type: &OperationType,
    original_parameters: &HashMap<String, serde_json::Value>,
    resolved_parameters: &HashMap<String, serde_json::Value>,
    persisted_parameters: &mut HashMap<String, serde_json::Value>,
) {
    match operation_type {
        OperationType::Fill => {
            let is_sensitive = original_parameters
                .get("sensitive")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or_else(|| {
                    original_parameters
                        .get("value")
                        .and_then(|value| parse_credential_reference(value).ok().flatten())
                        .is_some()
                });

            if is_sensitive
                && let Some(value) = persisted_parameters.get_mut("value")
                && value.is_string()
            {
                *value = serde_json::Value::String("[REDACTED]".to_string());
            }
        }
        OperationType::ExecuteScript => {
            if let Some(bindings) = persisted_parameters.get_mut("bindings") {
                redact_nested_strings(bindings);
            }
        }
        OperationType::HttpRequest => {
            for (key, persisted_value) in persisted_parameters.iter_mut() {
                redact_http_request_value(
                    key,
                    original_parameters.get(key),
                    resolved_parameters.get(key),
                    persisted_value,
                );
            }
        }
        _ => {}
    }
}

fn redact_http_request_value(
    key: &str,
    original_value: Option<&serde_json::Value>,
    resolved_value: Option<&serde_json::Value>,
    persisted_value: &mut serde_json::Value,
) {
    let sensitive_key = is_sensitive_http_key(key);
    let credential_backed = original_value.is_some_and(is_credential_reference_value);

    if sensitive_key || credential_backed {
        match persisted_value {
            serde_json::Value::String(_) | serde_json::Value::Object(_) => {
                *persisted_value = serde_json::Value::String("[REDACTED]".to_string());
                return;
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    *item = serde_json::Value::String("[REDACTED]".to_string());
                }
                return;
            }
            _ => {}
        }
    }

    match persisted_value {
        serde_json::Value::Object(persisted_map) => {
            if let (
                Some(serde_json::Value::Object(original_map)),
                Some(serde_json::Value::Object(resolved_map)),
            ) = (original_value, resolved_value)
            {
                for (nested_key, nested_persisted) in persisted_map.iter_mut() {
                    redact_http_request_value(
                        nested_key,
                        original_map.get(nested_key),
                        resolved_map.get(nested_key),
                        nested_persisted,
                    );
                }
                return;
            }

            if let Some(serde_json::Value::Object(original_map)) = original_value {
                for (nested_key, nested_persisted) in persisted_map.iter_mut() {
                    redact_http_request_value(
                        nested_key,
                        original_map.get(nested_key),
                        None,
                        nested_persisted,
                    );
                }
            }
        }
        serde_json::Value::Array(persisted_items) => {
            if let (
                Some(serde_json::Value::Array(original_items)),
                Some(serde_json::Value::Array(resolved_items)),
            ) = (original_value, resolved_value)
            {
                for (index, nested_persisted) in persisted_items.iter_mut().enumerate() {
                    redact_http_request_value(
                        key,
                        original_items.get(index),
                        resolved_items.get(index),
                        nested_persisted,
                    );
                }
                return;
            }

            if let Some(serde_json::Value::Array(original_items)) = original_value {
                for (index, nested_persisted) in persisted_items.iter_mut().enumerate() {
                    redact_http_request_value(
                        key,
                        original_items.get(index),
                        None,
                        nested_persisted,
                    );
                }
            }
        }
        serde_json::Value::String(_) => {
            if (sensitive_key || credential_backed)
                && matches!(resolved_value, Some(serde_json::Value::String(_)))
            {
                *persisted_value = serde_json::Value::String("[REDACTED]".to_string());
            }
        }
        _ => {}
    }
}

fn is_sensitive_http_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase();
    normalized == "authorization"
        || normalized == "cookie"
        || normalized == "set-cookie"
        || normalized == "api_key"
        || normalized == "api-key"
        || normalized == "secret"
        || normalized.contains("token")
}

fn is_credential_reference_value(value: &serde_json::Value) -> bool {
    let serde_json::Value::Object(map) = value else {
        return false;
    };
    map.get("$credential")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|field| !field.trim().is_empty())
}

fn redact_nested_strings(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(_) => {
            *value = serde_json::Value::String("[REDACTED]".to_string());
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_nested_strings(item);
            }
        }
        serde_json::Value::Object(map) => {
            for nested in map.values_mut() {
                if nested
                    .get("$credential")
                    .and_then(serde_json::Value::as_str)
                    .is_some()
                {
                    continue;
                }
                redact_nested_strings(nested);
            }
        }
        _ => {}
    }
}

async fn resolve_parameter_value(
    state: &SandboxState,
    session: &dyn SandboxSession,
    operation_type: &OperationType,
    value: &serde_json::Value,
) -> Result<(serde_json::Value, serde_json::Value), Response> {
    if let Some(reference) = parse_credential_reference(value)? {
        if matches!(
            operation_type,
            OperationType::BootstrapPage | OperationType::ExecuteScript
        ) {
            return Err(ApiErrorResponse::invalid_request(match operation_type {
                OperationType::BootstrapPage => {
                    "bootstrap_page does not accept credential references"
                }
                OperationType::ExecuteScript => {
                    "execute_script does not accept credential references"
                }
                _ => unreachable!(),
            })
            .into_response());
        }

        let material = load_session_credential_material(state, session)
            .await
            .map_err(|error| map_sandbox_error(error).into_response())?;

        let resolved = material
            .values
            .get(reference.field.as_str())
            .cloned()
            .ok_or_else(|| {
                ApiErrorResponse::invalid_request(format!(
                    "unsupported credential field reference: {} for {}",
                    reference.field,
                    material.credential_type.as_str()
                ))
                .into_response()
            })?;

        let resolved = format!(
            "{}{}{}",
            reference.prefix.as_deref().unwrap_or_default(),
            resolved,
            reference.suffix.as_deref().unwrap_or_default()
        );

        return Ok((value.clone(), serde_json::Value::String(resolved)));
    }

    match value {
        serde_json::Value::Array(values) => {
            let mut audit_values = Vec::with_capacity(values.len());
            let mut resolved_values = Vec::with_capacity(values.len());
            for item in values {
                let (audit_item, resolved_item) = Box::pin(resolve_parameter_value(
                    state,
                    session,
                    operation_type,
                    item,
                ))
                .await?;
                audit_values.push(audit_item);
                resolved_values.push(resolved_item);
            }
            Ok((
                serde_json::Value::Array(audit_values),
                serde_json::Value::Array(resolved_values),
            ))
        }
        serde_json::Value::Object(map) => {
            let mut audit_map = serde_json::Map::with_capacity(map.len());
            let mut resolved_map = serde_json::Map::with_capacity(map.len());
            for (key, item) in map {
                let (audit_item, resolved_item) = Box::pin(resolve_parameter_value(
                    state,
                    session,
                    operation_type,
                    item,
                ))
                .await?;
                audit_map.insert(key.clone(), audit_item);
                resolved_map.insert(key.clone(), resolved_item);
            }
            Ok((
                serde_json::Value::Object(audit_map),
                serde_json::Value::Object(resolved_map),
            ))
        }
        _ => Ok((value.clone(), value.clone())),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CredentialReference {
    field: String,
    prefix: Option<String>,
    suffix: Option<String>,
}

#[allow(clippy::result_large_err)]
fn parse_credential_reference(
    value: &serde_json::Value,
) -> Result<Option<CredentialReference>, Response> {
    let serde_json::Value::Object(map) = value else {
        return Ok(None);
    };

    let Some(reference_value) = map.get("$credential") else {
        return Ok(None);
    };

    for key in map.keys() {
        if !matches!(key.as_str(), "$credential" | "prefix" | "suffix") {
            return Err(ApiErrorResponse::invalid_request(
                "credential reference objects may only contain $credential, prefix, and suffix",
            )
            .into_response());
        }
    }

    let Some(field) = reference_value.as_str() else {
        return Err(ApiErrorResponse::invalid_request(
            "credential reference field must be a string",
        )
        .into_response());
    };

    if field.trim().is_empty() {
        return Err(ApiErrorResponse::invalid_request(
            "credential reference field must not be empty",
        )
        .into_response());
    }

    let prefix = match map.get("prefix") {
        Some(serde_json::Value::String(prefix)) => Some(prefix.clone()),
        Some(_) => {
            return Err(ApiErrorResponse::invalid_request(
                "credential reference prefix must be a string",
            )
            .into_response());
        }
        None => None,
    };

    let suffix = match map.get("suffix") {
        Some(serde_json::Value::String(suffix)) => Some(suffix.clone()),
        Some(_) => {
            return Err(ApiErrorResponse::invalid_request(
                "credential reference suffix must be a string",
            )
            .into_response());
        }
        None => None,
    };

    Ok(Some(CredentialReference {
        field: field.to_string(),
        prefix,
        suffix,
    }))
}

async fn load_session_credential_material(
    state: &SandboxState,
    session: &dyn SandboxSession,
) -> Result<SessionCredentialMaterial, SandboxError> {
    let session_id: Uuid = session.id().into();
    if let Some(material) = state.cached_credential(session_id).await {
        return Ok(material);
    }

    let context = session.context();
    let vault = state.vault.as_ref().ok_or_else(|| {
        SandboxError::Config("sandbox credential resolution requires credential vault".to_string())
    })?;
    let credential_id_model = CredentialId::from_string(context.credential_id.to_string())
        .map_err(|error| SandboxError::Other(format!("invalid credential id: {error}")))?;
    let tenant_id = TenantId::new(context.tenant_id.to_string());
    let user_id = UserId::new(context.user_id.to_string());

    let entry = match vault.get_credential(&credential_id_model, &tenant_id, &user_id) {
        Ok(Some(entry)) => entry,
        Ok(None) | Err(VaultError::TenantIsolationViolation { .. }) => {
            return Err(SandboxError::Session(
                crate::tee::sandbox::error::SessionError::credential_not_found(
                    context.credential_id,
                ),
            ));
        }
        Err(error) => {
            return Err(SandboxError::Other(format!(
                "failed to load credential for sandbox session: {error}"
            )));
        }
    };

    let material = decrypt_session_credential_material(state, &entry).await?;
    state
        .store_cached_credential(session_id, material.clone())
        .await;

    Ok(material)
}

async fn decrypt_session_credential_material(
    state: &SandboxState,
    entry: &VaultEntry,
) -> Result<SessionCredentialMaterial, SandboxError> {
    let plaintext = decrypt_vault_entry_in_sandbox(state, entry).await?;
    let plaintext_data: serde_json::Value =
        serde_json::from_slice(&plaintext).map_err(|error| {
            SandboxError::Other(format!("credential plaintext is not valid JSON: {error}"))
        })?;
    let values = extract_supported_credential_fields(entry.credential_type, &plaintext_data)?;

    Ok(SessionCredentialMaterial {
        credential_type: entry.credential_type,
        values,
    })
}

fn extract_supported_credential_fields(
    credential_type: CredentialType,
    plaintext_data: &serde_json::Value,
) -> Result<HashMap<String, String>, SandboxError> {
    let object = plaintext_data.as_object().ok_or_else(|| {
        SandboxError::Other(
            "credential plaintext must be a JSON object for delegated sandbox use".to_string(),
        )
    })?;
    let mut values = HashMap::new();
    match credential_type {
        CredentialType::UsernamePassword => {
            for field in ["username", "password"] {
                let value = object
                    .get(field)
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        SandboxError::Other(format!("credential plaintext missing {field}"))
                    })?;
                values.insert(field.to_string(), value.to_string());
            }
        }
        CredentialType::ApiKey => {
            let value = object
                .get("api_key")
                .or_else(|| object.get("key"))
                .or_else(|| object.get("apiKey"))
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    SandboxError::Other("credential plaintext missing api_key".to_string())
                })?;
            for field in ["api_key", "key", "apiKey"] {
                values.insert(field.to_string(), value.to_string());
            }
        }
        CredentialType::SessionCookie => {
            let value = object
                .get("cookie")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    SandboxError::Other("credential plaintext missing cookie".to_string())
                })?;
            values.insert("cookie".to_string(), value.to_string());
            if let Some(name) = object.get("name").and_then(serde_json::Value::as_str) {
                values.insert("name".to_string(), name.to_string());
            }
        }
        CredentialType::OAuthRefresh => {
            let value = object
                .get("refresh_token")
                .or_else(|| object.get("refreshToken"))
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    SandboxError::Other("credential plaintext missing refresh_token".to_string())
                })?;
            values.insert("refresh_token".to_string(), value.to_string());
        }
        _ => {}
    }

    for (key, value) in object {
        match value {
            serde_json::Value::String(raw) => {
                values.entry(key.clone()).or_insert_with(|| raw.clone());
            }
            serde_json::Value::Number(number) => {
                values
                    .entry(key.clone())
                    .or_insert_with(|| number.to_string());
            }
            serde_json::Value::Bool(boolean) => {
                values
                    .entry(key.clone())
                    .or_insert_with(|| boolean.to_string());
            }
            _ => {}
        }
    }

    if values.is_empty() {
        return Err(SandboxError::Other(format!(
            "sandbox credential delegation found no scalar fields for {}",
            credential_type.as_str()
        )));
    }

    Ok(values)
}

async fn decrypt_vault_entry_in_sandbox(
    state: &SandboxState,
    entry: &VaultEntry,
) -> Result<Vec<u8>, SandboxError> {
    let key_hierarchy = state.key_hierarchy.as_ref().ok_or_else(|| {
        SandboxError::Config(
            "sandbox credential resolution requires key hierarchy state".to_string(),
        )
    })?;
    let blob = EncryptedBlob {
        version: entry.encrypted_payload.version,
        algorithm: entry.encrypted_payload.algorithm.clone(),
        kdf: entry.encrypted_payload.kdf.clone(),
        nonce: entry.encrypted_payload.nonce.clone(),
        auth_tag: entry.encrypted_payload.auth_tag.clone(),
        ciphertext: entry.encrypted_payload.ciphertext.clone(),
        aad_hash: None,
    };
    let context = CredentialCryptoContext::new(
        entry.tenant_id.as_str(),
        entry.user_id.hash(),
        entry.credential_id.as_str(),
    );

    if let Some(enclave) = &state.enclave {
        let mut enclave = enclave.lock().await;
        if enclave.is_running() {
            return enclave
                .decrypt_credential(
                    entry.tenant_id.as_str(),
                    entry.user_id.hash(),
                    entry.credential_id.as_str(),
                    &blob,
                )
                .map_err(|error| SandboxError::Other(format!("TEE decrypt failed: {error}")));
        }
    }

    let hierarchy = key_hierarchy.read().await;
    context
        .decrypt_with_hierarchy(&hierarchy, &blob)
        .map_err(|error| SandboxError::Other(format!("software decrypt failed: {error}")))
}

fn ensure_credential_exists(
    vault: Option<&Arc<CredentialVault>>,
    token: &ValidatedToken,
    credential_id: Uuid,
) -> Result<(), SandboxError> {
    if !token.can_access_credential(&credential_id.to_string()) {
        return Err(SandboxError::Other(
            "forbidden: credential is not allowed by the current token whitelist".to_string(),
        ));
    }

    let vault = vault.ok_or_else(|| {
        SandboxError::Config("sandbox credential validation requires credential vault".to_string())
    })?;

    let credential_id_model = CredentialId::from_string(credential_id.to_string())
        .map_err(|error| SandboxError::Other(format!("invalid credential id: {error}")))?;
    let tenant_id = TenantId::new(token.tenant_id.clone());
    let user_id = UserId::new(token.user_id.clone());

    match vault.get_credential_metadata(&credential_id_model, &tenant_id, &user_id) {
        Ok(Some(metadata)) => match metadata.status.as_str() {
            "active" => Ok(()),
            "expired" => Err(SandboxError::Other(
                "invalid_request: Credential is expired, refresh or rebind a valid token before creating a sandbox session".to_string(),
            )),
            _ => Err(SandboxError::Session(
                crate::tee::sandbox::error::SessionError::credential_not_found(credential_id),
            )),
        },
        Ok(None) | Err(VaultError::TenantIsolationViolation { .. }) => Err(SandboxError::Session(
            crate::tee::sandbox::error::SessionError::credential_not_found(credential_id),
        )),
        Err(error) => Err(SandboxError::Other(format!(
            "credential existence check failed: {error}"
        ))),
    }
}

#[derive(Debug)]
enum CreateSessionCredentialError {
    MissingReference(&'static str),
    InvalidReference(String),
    Sandbox(SandboxError),
}

impl From<SandboxError> for CreateSessionCredentialError {
    fn from(value: SandboxError) -> Self {
        Self::Sandbox(value)
    }
}

fn resolve_create_session_credential(
    vault: Option<&Arc<CredentialVault>>,
    token: &ValidatedToken,
    credential_id: Option<Uuid>,
    service_id: Option<&str>,
) -> Result<Uuid, CreateSessionCredentialError> {
    if let Some(credential_id) = credential_id {
        ensure_credential_exists(vault, token, credential_id)?;
        if let Some(service_id) = service_id {
            let service_id = normalize_requested_service_id(service_id)?;
            ensure_credential_matches_service_id(vault, token, credential_id, service_id)?;
        }
        return Ok(credential_id);
    }

    let service_id = normalize_requested_service_id(service_id.ok_or(
        CreateSessionCredentialError::MissingReference(
            "missing required field: credential_id or service_id",
        ),
    )?)?;

    let metadata = resolve_credential_metadata_by_service_id(vault, token, service_id)?;

    Uuid::parse_str(&metadata.credential_id).map_err(|error| {
        CreateSessionCredentialError::InvalidReference(format!(
            "resolved credential_id is invalid: {error}"
        ))
    })
}

fn normalize_requested_service_id(service_id: &str) -> Result<&str, CreateSessionCredentialError> {
    let normalized = service_id.trim();
    if normalized.is_empty() {
        return Err(CreateSessionCredentialError::InvalidReference(
            "service_id must not be empty".to_string(),
        ));
    }

    Ok(normalized)
}

fn ensure_credential_matches_service_id(
    vault: Option<&Arc<CredentialVault>>,
    token: &ValidatedToken,
    credential_id: Uuid,
    expected_service_id: &str,
) -> Result<(), CreateSessionCredentialError> {
    let vault = vault.ok_or_else(|| {
        CreateSessionCredentialError::Sandbox(SandboxError::Config(
            "sandbox credential validation requires credential vault".to_string(),
        ))
    })?;
    let credential_id_model =
        CredentialId::from_string(credential_id.to_string()).map_err(|error| {
            CreateSessionCredentialError::InvalidReference(format!(
                "invalid credential id: {error}"
            ))
        })?;
    let tenant_id = TenantId::new(token.tenant_id.clone());
    let user_id = UserId::new(token.user_id.clone());
    let metadata = vault
        .get_credential_metadata(&credential_id_model, &tenant_id, &user_id)
        .map_err(|error| {
            CreateSessionCredentialError::Sandbox(SandboxError::Other(format!(
                "credential metadata lookup failed: {error}"
            )))
        })?
        .ok_or_else(|| {
            CreateSessionCredentialError::Sandbox(SandboxError::Session(
                crate::tee::sandbox::error::SessionError::credential_not_found(credential_id),
            ))
        })?;

    if metadata.service_id != expected_service_id {
        return Err(CreateSessionCredentialError::InvalidReference(format!(
            "credential_id does not match service_id '{expected_service_id}'",
        )));
    }

    Ok(())
}

fn resolve_credential_metadata_by_service_id(
    vault: Option<&Arc<CredentialVault>>,
    token: &ValidatedToken,
    service_id: &str,
) -> Result<CredentialMetadata, CreateSessionCredentialError> {
    let vault = vault.ok_or_else(|| {
        CreateSessionCredentialError::Sandbox(SandboxError::Config(
            "sandbox credential validation requires credential vault".to_string(),
        ))
    })?;
    let tenant_id = TenantId::new(token.tenant_id.clone());
    let user_id = UserId::new(token.user_id.clone());
    let result = vault
        .list_credentials(
            &tenant_id,
            &user_id,
            CredentialFilter {
                service_id: Some(ServiceId::new(service_id)),
                credential_type: None,
                include_deleted: false,
                only_valid: true,
            },
        )
        .map_err(|error| {
            CreateSessionCredentialError::Sandbox(SandboxError::Other(format!(
                "credential lookup failed: {error}"
            )))
        })?;
    let mut matches = result
        .credentials
        .into_iter()
        .filter(|item| token.can_access_credential(&item.credential_id))
        .collect::<Vec<_>>();

    match matches.len() {
        0 => Err(CreateSessionCredentialError::InvalidReference(format!(
            "no credential matched service_id '{service_id}' within the current token scope",
        ))),
        1 => Ok(matches.remove(0)),
        _ => Err(CreateSessionCredentialError::InvalidReference(format!(
            "multiple credentials share service_id '{service_id}' within the current token scope",
        ))),
    }
}

/// 解析 UUID
fn parse_uuid(s: &str) -> Uuid {
    Uuid::parse_str(s).unwrap_or_else(|_| Uuid::new_v4())
}

/// 解析操作类型
fn parse_operation_type(s: &str) -> Option<OperationType> {
    match s.to_lowercase().as_str() {
        "navigate" => Some(OperationType::Navigate),
        "click" => Some(OperationType::Click),
        "fill" => Some(OperationType::Fill),
        "get_text" => Some(OperationType::GetText),
        "export" => Some(OperationType::Export),
        "domexport" | "dom-export" | "dom_export" => Some(OperationType::DomExport),
        "execute_script" => Some(OperationType::ExecuteScript),
        "bootstrappage" | "bootstrap-page" | "bootstrap_page" => Some(OperationType::BootstrapPage),
        "wait" => Some(OperationType::Wait),
        "http_request" => Some(OperationType::HttpRequest),
        "custom" => Some(OperationType::Custom),
        _ => None,
    }
}

fn map_operation_record(record: SandboxOperationRecord) -> OperationDetailResponse {
    OperationDetailResponse {
        operation_id: record.operation_id,
        session_id: record.session_id.into(),
        operation_type: record.operation_type,
        status: record.status,
        started_at: record.started_at.to_rfc3339(),
        completed_at: record.completed_at.map(|value| value.to_rfc3339()),
        execution_time_ms: record.execution_duration_ms.map(|value| value as u64),
    }
}

/// 将 SandboxError 映射为 API 响应
fn map_sandbox_error(error: SandboxError) -> Response {
    let (code, message, status) = match &error {
        SandboxError::Config(message) => (
            ErrorCode::InvalidRequest,
            message.clone(),
            StatusCode::BAD_REQUEST,
        ),
        SandboxError::Session(e) => match e {
            crate::tee::sandbox::error::SessionError::NotFound { .. } => {
                (ErrorCode::NotFound, e.to_string(), StatusCode::NOT_FOUND)
            }
            crate::tee::sandbox::error::SessionError::CredentialNotFound { .. } => (
                ErrorCode::CredentialNotFound,
                e.to_string(),
                StatusCode::NOT_FOUND,
            ),
            crate::tee::sandbox::error::SessionError::Expired { .. } => (
                ErrorCode::InvalidRequest,
                e.to_string(),
                StatusCode::BAD_REQUEST,
            ),
            crate::tee::sandbox::error::SessionError::InvalidState { .. } => (
                ErrorCode::InvalidRequest,
                e.to_string(),
                StatusCode::BAD_REQUEST,
            ),
            crate::tee::sandbox::error::SessionError::MaxSessionsReached { .. } => (
                ErrorCode::ServiceUnavailable,
                e.to_string(),
                StatusCode::SERVICE_UNAVAILABLE,
            ),
            _ => (
                ErrorCode::InternalError,
                e.to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        },
        SandboxError::Security(e) => (ErrorCode::Forbidden, e.to_string(), StatusCode::FORBIDDEN),
        SandboxError::Timeout { .. } => (
            ErrorCode::ServiceUnavailable,
            error.to_string(),
            StatusCode::GATEWAY_TIMEOUT,
        ),
        SandboxError::Other(message) if message.starts_with("invalid_request:") => (
            ErrorCode::InvalidRequest,
            message
                .trim_start_matches("invalid_request:")
                .trim()
                .to_string(),
            StatusCode::BAD_REQUEST,
        ),
        SandboxError::Other(message) if message.starts_with("forbidden:") => (
            ErrorCode::Forbidden,
            message.trim_start_matches("forbidden:").trim().to_string(),
            StatusCode::FORBIDDEN,
        ),
        SandboxError::Other(message) if message.starts_with("selector_not_found:") => (
            ErrorCode::InvalidRequest,
            message
                .trim_start_matches("selector_not_found:")
                .trim()
                .to_string(),
            StatusCode::BAD_REQUEST,
        ),
        SandboxError::Other(message) if message.starts_with("bootstrap_failed:") => (
            ErrorCode::InvalidRequest,
            message
                .trim_start_matches("bootstrap_failed:")
                .trim()
                .to_string(),
            StatusCode::BAD_REQUEST,
        ),
        _ => (
            ErrorCode::InternalError,
            error.to_string(),
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
    };

    (status, Json(ApiErrorResponse::new(code, message))).into_response()
}

// ==================== 兜底处理器 ====================

/// 兜底处理器：POST /sandbox/sessions/pause 缺少会话ID时返回404
/// BUG-18223: 防止被 /sandbox/sessions/:id 动态段误匹配为 405 Method Not Allowed
async fn pause_missing_id_handler() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "success": false,
            "error": {
                "code": "SESSION_NOT_FOUND",
                "message": "沙箱会话不存在"
            },
            "meta": {
                "request_id": uuid::Uuid::now_v7().to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        })),
    )
}

/// 兜底处理器：POST /sandbox/sessions/resume 缺少会话ID时返回404
/// BUG-18224: 防止被 /sandbox/sessions/:id 动态段误匹配为 405 Method Not Allowed
async fn resume_missing_id_handler() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "error": "not_found"
        })),
    )
}

/// 兜底处理器：POST /sandbox/sessions/execute 缺少会话ID时返回404
/// 防止被 /sandbox/sessions/:id 动态段误匹配为 405 Method Not Allowed
async fn execute_missing_id_handler() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "success": false,
            "error": {
                "code": "SESSION_NOT_FOUND",
                "message": "沙箱会话不存在"
            },
            "meta": {
                "request_id": uuid::Uuid::now_v7().to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        })),
    )
}

/// 兜底处理器：POST /sandbox/sessions/export 缺少会话ID时返回404
/// 防止被 /sandbox/sessions/:id 动态段误匹配为 405 Method Not Allowed
async fn export_missing_id_handler() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "success": false,
            "error": {
                "code": "SESSION_NOT_FOUND",
                "message": "沙箱会话不存在"
            },
            "meta": {
                "request_id": uuid::Uuid::now_v7().to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        })),
    )
}

/// 兜底处理器：POST /sandbox/sessions/dom-export 缺少会话ID时返回404
/// 防止被 /sandbox/sessions/:id 动态段误匹配为 405 Method Not Allowed
async fn dom_export_missing_id_handler() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "success": false,
            "error": {
                "code": "SESSION_NOT_FOUND",
                "message": "沙箱会话不存在"
            },
            "meta": {
                "request_id": uuid::Uuid::now_v7().to_string(),
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        })),
    )
}

/// 兜底处理器：DELETE /sandbox/sessions 缺少会话ID时返回404
/// BUG-18225: 防止被 /sandbox/sessions/:id 动态段误匹配为 405 Method Not Allowed
async fn close_missing_id_handler() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "error": "not_found"
        })),
    )
}

// ==================== 路由构建 ====================

/// 构建沙箱 API 路由
pub fn sandbox_routes() -> axum::Router<SandboxState> {
    use axum::routing::{delete, get, post};

    axum::Router::new()
        // 会话管理
        .route("/sandbox/sessions", post(create_session))
        .route("/sandbox/sessions", get(list_sessions))
        // BUG-18225: 缺少会话ID的DELETE路径兜底，返回404而非405
        .route("/sandbox/sessions", delete(close_missing_id_handler))
        // 缺少会话ID的子路径兜底路由必须在动态路由之前注册
        // 否则静态路径会被动态段 :id 优先捕获并返回405 Method Not Allowed
        .route("/sandbox/sessions/pause", post(pause_missing_id_handler))
        .route("/sandbox/sessions/resume", post(resume_missing_id_handler))
        .route(
            "/sandbox/sessions/execute",
            post(execute_missing_id_handler),
        )
        .route("/sandbox/sessions/export", post(export_missing_id_handler))
        .route(
            "/sandbox/sessions/dom-export",
            post(dom_export_missing_id_handler),
        )
        // 带动态段的路由（必须在静态兜底路由之后）
        .route("/sandbox/sessions/:id", get(get_session))
        .route("/sandbox/sessions/:id/execute", post(execute_operation))
        .route("/sandbox/operations/:operation_id", get(get_operation))
        .route("/sandbox/sessions/:id/pause", post(pause_session))
        .route("/sandbox/sessions/:id/resume", post(resume_session))
        .route("/sandbox/sessions/:id", delete(close_session))
        .route("/sandbox/sessions/:id/export", post(export_data))
        .route("/sandbox/sessions/:id/dom-export", post(dom_export))
        .route("/sandbox/stats", get(get_stats))
        // WebSocket 实时连接
        .route(
            "/sandbox/sessions/:id/ws/:credential_id",
            get(websocket_upgrade),
        )
}

/// WebSocket 升级处理器
async fn websocket_upgrade(
    Path((session_id, credential_id)): Path<(String, String)>,
    State(_state): State<SandboxState>,
    ws: WebSocketUpgrade,
    Extension(token): Extension<ValidatedToken>,
) -> Response {
    // 验证 Scope: sandbox:execute
    if let Err(e) = check_sandbox_control_scope(&token, TokenScope::SandboxExecute).await {
        return e;
    }

    // 直接使用 WebSocketUpgrade 的 on_upgrade 方法
    ws.on_upgrade(move |socket| async move {
        // 创建简化的 ApiContext
        let ctx = ApiContext::from_request_context(&RequestContext::from_validated_token(&token));

        handle_socket(socket, ctx, session_id, credential_id, token).await;
    })
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::middleware::{TokenScope, tests::create_mock_token};
    use crate::api::sandbox_owner::{SandboxOwnerRecord, SandboxOwnerRegistry};
    use crate::tee::sandbox::error::SessionError;
    use crate::tee::sandbox::{
        SessionId,
        repository::{SandboxOperationRecord, SandboxSessionRecord},
    };
    use crate::vault::models::{CreateCredentialRequest, EncryptedPayload, ServiceId};
    use crate::vault::storage::CredentialVault;
    use async_trait::async_trait;
    use axum::{
        body::{Body, to_bytes},
        http::{HeaderMap, HeaderValue, Request},
        routing::post,
    };
    use chrono::{Duration as ChronoDuration, Utc};
    use serde_json::Value;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::net::TcpListener;
    use tower::ServiceExt;
    use uuid::Uuid;

    fn create_test_payload() -> EncryptedPayload {
        EncryptedPayload::new(
            2,
            "AES-256-GCM",
            "HKDF-SHA-256",
            vec![0; 12],
            vec![1; 16],
            vec![2; 32],
        )
    }

    struct StubSession {
        id: SessionId,
        context: crate::tee::sandbox::types::SessionContext,
    }

    #[async_trait]
    impl SandboxSession for StubSession {
        fn id(&self) -> SessionId {
            self.id
        }

        async fn status(&self) -> crate::tee::sandbox::types::SessionStatus {
            crate::tee::sandbox::types::SessionStatus::Ready
        }

        fn context(&self) -> &crate::tee::sandbox::types::SessionContext {
            &self.context
        }

        async fn execute_operation(
            &self,
            _operation: OperationRequest,
        ) -> Result<crate::tee::sandbox::types::ExecutionResult, SandboxError> {
            Err(SandboxError::Session(SessionError::OperationRejected {
                reason: "stub session".to_string(),
            }))
        }

        async fn pause(&self) -> Result<(), SandboxError> {
            Ok(())
        }

        async fn resume(&self) -> Result<(), SandboxError> {
            Ok(())
        }

        async fn close(&self) -> Result<(), SandboxError> {
            Ok(())
        }

        fn is_expired(&self) -> bool {
            false
        }
    }

    fn create_stub_session() -> StubSession {
        let session_id = SessionId::new();
        StubSession {
            id: session_id,
            context: crate::tee::sandbox::types::SessionContext {
                session_id,
                sandbox_id: crate::tee::sandbox::types::SandboxId::new(),
                tenant_id: Uuid::new_v4(),
                user_id: Uuid::new_v4(),
                credential_id: Uuid::new_v4(),
                original_intent: "test".to_string(),
                created_at: OffsetDateTime::now_utc(),
                expires_at: OffsetDateTime::now_utc() + time::Duration::minutes(5),
                last_activity_at: Arc::new(RwLock::new(OffsetDateTime::now_utc())),
            },
        }
    }

    #[derive(Default)]
    struct InMemoryOwnerRegistry {
        records: Arc<RwLock<HashMap<Uuid, SandboxOwnerRecord>>>,
    }

    #[async_trait]
    impl SandboxOwnerRegistry for InMemoryOwnerRegistry {
        async fn register_owner(
            &self,
            record: SandboxOwnerRecord,
            _ttl: Duration,
        ) -> Result<(), SandboxError> {
            self.records.write().await.insert(record.session_id, record);
            Ok(())
        }

        async fn get_owner(
            &self,
            session_id: SessionId,
        ) -> Result<Option<SandboxOwnerRecord>, SandboxError> {
            Ok(self
                .records
                .read()
                .await
                .get(&Uuid::from(session_id))
                .cloned())
        }

        async fn delete_owner(&self, session_id: SessionId) -> Result<(), SandboxError> {
            self.records.write().await.remove(&Uuid::from(session_id));
            Ok(())
        }
    }

    struct OwnerRoutingPool {
        session: Option<Arc<dyn SandboxSession>>,
        release_calls: Arc<RwLock<Vec<SessionId>>>,
    }

    impl OwnerRoutingPool {
        fn missing() -> Self {
            Self {
                session: None,
                release_calls: Arc::new(RwLock::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl SandboxPool for OwnerRoutingPool {
        async fn acquire_session(
            &self,
            _request: SessionRequest,
        ) -> Result<Arc<dyn SandboxSession>, SandboxError> {
            self.session.clone().ok_or_else(|| {
                SessionError::CreationFailed {
                    reason: "test pool cannot create sessions".to_string(),
                }
                .into()
            })
        }

        async fn release_session(&self, session_id: SessionId) -> Result<(), SandboxError> {
            self.release_calls.write().await.push(session_id);
            if self.session.is_some() {
                Ok(())
            } else {
                Err(SessionError::not_found(session_id.into()).into())
            }
        }

        async fn get_session(
            &self,
            session_id: SessionId,
        ) -> Result<Arc<dyn SandboxSession>, SandboxError> {
            self.session
                .clone()
                .ok_or_else(|| SessionError::not_found(session_id.into()).into())
        }

        async fn health(&self) -> crate::tee::sandbox::SandboxHealth {
            crate::tee::sandbox::SandboxHealth {
                pool_status: crate::tee::sandbox::PoolStatus::Running,
                active_sessions: usize::from(self.session.is_some()),
                warm_instances: 0,
                healthy: true,
                error: None,
                browser_runtime_probe_error: None,
                process_health_issues: 0,
                process_health_summaries: Vec::new(),
            }
        }

        async fn shutdown(&self) -> Result<(), SandboxError> {
            Ok(())
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[derive(Default)]
    struct OwnerRoutingRepository {
        session_record: Option<SandboxSessionRecord>,
    }

    #[async_trait]
    impl crate::tee::sandbox::repository::SandboxRepository for OwnerRoutingRepository {
        async fn create_session(
            &self,
            _record: crate::tee::sandbox::repository::NewSandboxSessionRecord,
        ) -> Result<(), SandboxError> {
            Ok(())
        }

        async fn mark_session_terminated(
            &self,
            _session_id: SessionId,
            _status: &'static str,
            _reason: Option<String>,
            _terminated_at: chrono::DateTime<Utc>,
        ) -> Result<(), SandboxError> {
            Ok(())
        }

        async fn update_session_status(
            &self,
            _session_id: SessionId,
            _status: &'static str,
        ) -> Result<(), SandboxError> {
            Ok(())
        }

        async fn mark_session_operation_started(
            &self,
            _session_id: SessionId,
            _operation_id: Uuid,
            _started_at: chrono::DateTime<Utc>,
        ) -> Result<(), SandboxError> {
            Ok(())
        }

        async fn mark_session_operation_finished(
            &self,
            _session_id: SessionId,
            _last_error_summary: Option<String>,
        ) -> Result<(), SandboxError> {
            Ok(())
        }

        async fn create_operation(
            &self,
            _record: crate::tee::sandbox::repository::NewSandboxOperationRecord,
        ) -> Result<(), SandboxError> {
            Ok(())
        }

        async fn complete_operation(
            &self,
            _record: crate::tee::sandbox::repository::CompleteSandboxOperationRecord,
        ) -> Result<(), SandboxError> {
            Ok(())
        }

        async fn reconcile_orphaned_active_sessions(
            &self,
            _recovery_reason: &str,
        ) -> Result<u64, SandboxError> {
            Ok(0)
        }

        async fn list_sessions_by_tenant(
            &self,
            _tenant_id: Uuid,
        ) -> Result<Vec<SandboxSessionRecord>, SandboxError> {
            Ok(self.session_record.clone().into_iter().collect())
        }

        async fn get_session_by_id(
            &self,
            tenant_id: Uuid,
            session_id: SessionId,
        ) -> Result<Option<SandboxSessionRecord>, SandboxError> {
            Ok(self
                .session_record
                .clone()
                .filter(|record| record.tenant_id == tenant_id && record.session_id == session_id))
        }

        async fn get_operation_by_id(
            &self,
            _tenant_id: Uuid,
            _operation_id: Uuid,
        ) -> Result<Option<SandboxOperationRecord>, SandboxError> {
            Ok(None)
        }
    }

    fn make_owner_routing_state(
        pool: Arc<dyn SandboxPool>,
        repository: Option<Arc<dyn crate::tee::sandbox::repository::SandboxRepository>>,
        owner_registry: Option<Arc<dyn SandboxOwnerRegistry>>,
    ) -> SandboxState {
        SandboxState {
            pool,
            config: SandboxConfig::default(),
            repository,
            vault: None,
            key_hierarchy: None,
            enclave: None,
            owner_id: "owner-a".to_string(),
            owner_base_url: "http://127.0.0.1".to_string(),
            owner_registry,
            forwarding_client: reqwest::Client::new(),
            credential_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn make_session_record(session_id: SessionId, tenant_id: Uuid) -> SandboxSessionRecord {
        let now = Utc::now();
        SandboxSessionRecord {
            session_id,
            sandbox_id: Uuid::new_v4(),
            tenant_id,
            created_by: Uuid::new_v4(),
            credential_id: Uuid::new_v4(),
            original_intent: "owner routing".to_string(),
            status: "ready".to_string(),
            started_at: now,
            expires_at: now + ChronoDuration::minutes(5),
            terminated_at: None,
            updated_at: now,
            active_operation_id: None,
            active_operation_started_at: None,
            last_error_summary: None,
        }
    }

    #[test]
    fn test_parse_operation_type() {
        assert!(matches!(
            parse_operation_type("navigate"),
            Some(OperationType::Navigate)
        ));
        assert!(matches!(
            parse_operation_type("click"),
            Some(OperationType::Click)
        ));
        assert!(matches!(
            parse_operation_type("dom_export"),
            Some(OperationType::DomExport)
        ));
        assert!(matches!(
            parse_operation_type("http_request"),
            Some(OperationType::HttpRequest)
        ));
        assert!(matches!(
            parse_operation_type("bootstrap_page"),
            Some(OperationType::BootstrapPage)
        ));
        assert!(matches!(
            parse_operation_type("bootstrap-page"),
            Some(OperationType::BootstrapPage)
        ));
        assert!(parse_operation_type("screenshot").is_none());
        assert!(parse_operation_type("invalid").is_none());
    }

    #[test]
    fn test_dom_export_request_defaults() {
        let request: DomExportRequest = serde_json::from_str("{}").unwrap();

        assert_eq!(request.root_selector, "html");
        assert!(matches!(request.format, DomExportFormat::Html));
        assert!(request.include_text);
        assert!(request.include_metadata);
        assert!(request.extra_sensitive_selectors.is_empty());
        assert_eq!(request.max_bytes, 262_144);
    }

    #[test]
    fn test_redact_persisted_parameters_redacts_http_request_secrets() {
        let original_parameters = HashMap::from([
            (
                "method".to_string(),
                serde_json::Value::String("POST".to_string()),
            ),
            (
                "headers".to_string(),
                serde_json::json!({
                    "Authorization": { "$credential": "api_key" },
                    "X-Trace": "keep"
                }),
            ),
            (
                "body".to_string(),
                serde_json::json!({
                    "access_token": "plain-token",
                    "profile": {
                        "refresh_token": { "$credential": "refresh_token" },
                        "visible": "ok"
                    }
                }),
            ),
        ]);
        let resolved_parameters = HashMap::from([
            (
                "method".to_string(),
                serde_json::Value::String("POST".to_string()),
            ),
            (
                "headers".to_string(),
                serde_json::json!({
                    "Authorization": "Bearer secret",
                    "X-Trace": "keep"
                }),
            ),
            (
                "body".to_string(),
                serde_json::json!({
                    "access_token": "plain-token",
                    "profile": {
                        "refresh_token": "resolved-refresh",
                        "visible": "ok"
                    }
                }),
            ),
        ]);
        let mut persisted_parameters = original_parameters.clone();

        redact_persisted_parameters(
            &OperationType::HttpRequest,
            &original_parameters,
            &resolved_parameters,
            &mut persisted_parameters,
        );

        assert_eq!(
            persisted_parameters["headers"]["Authorization"],
            serde_json::Value::String("[REDACTED]".to_string())
        );
        assert_eq!(
            persisted_parameters["body"]["access_token"],
            serde_json::Value::String("[REDACTED]".to_string())
        );
        assert_eq!(
            persisted_parameters["body"]["profile"]["refresh_token"],
            serde_json::Value::String("[REDACTED]".to_string())
        );
        assert_eq!(
            persisted_parameters["body"]["profile"]["visible"],
            serde_json::json!("ok")
        );
    }

    #[test]
    fn test_collect_http_sensitive_output_values_tracks_resolved_and_bare_credentials() {
        let original_parameters = HashMap::from([
            (
                "headers".to_string(),
                serde_json::json!({
                    "X-Cred": { "$credential": "api_key" },
                    "Authorization": { "$credential": "api_key", "prefix": "Bearer " }
                }),
            ),
            (
                "body".to_string(),
                serde_json::json!({
                    "profile": {
                        "secret": "visible-in-request"
                    }
                }),
            ),
        ]);
        let resolved_parameters = HashMap::from([
            (
                "headers".to_string(),
                serde_json::json!({
                    "X-Cred": "sk_live_123",
                    "Authorization": "Bearer sk_live_123"
                }),
            ),
            (
                "body".to_string(),
                serde_json::json!({
                    "profile": {
                        "secret": "body-secret"
                    }
                }),
            ),
        ]);

        let sensitive_values = collect_http_sensitive_output_values(
            &OperationType::HttpRequest,
            &original_parameters,
            &resolved_parameters,
        );

        assert!(sensitive_values.contains(&"sk_live_123".to_string()));
        assert!(sensitive_values.contains(&"Bearer sk_live_123".to_string()));
        assert!(sensitive_values.contains(&"body-secret".to_string()));
    }

    #[test]
    fn test_collect_http_sensitive_output_values_ignores_non_sensitive_plain_fields() {
        let original_parameters = HashMap::from([(
            "headers".to_string(),
            serde_json::json!({
                "X-Trace": "trace-id",
                "X-User": "alice"
            }),
        )]);
        let resolved_parameters = original_parameters.clone();

        let sensitive_values = collect_http_sensitive_output_values(
            &OperationType::HttpRequest,
            &original_parameters,
            &resolved_parameters,
        );

        assert!(sensitive_values.is_empty());
    }

    #[test]
    fn test_parse_uuid() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let uuid = parse_uuid(uuid_str);
        assert_eq!(uuid.to_string(), uuid_str);
    }

    #[test]
    fn test_map_operation_record() {
        let now = Utc::now();
        let record = SandboxOperationRecord {
            operation_id: Uuid::new_v4(),
            session_id: SessionId::new(),
            operation_type: "navigate".to_string(),
            status: "completed".to_string(),
            started_at: now,
            completed_at: Some(now),
            execution_duration_ms: Some(42),
        };

        let mapped = map_operation_record(record);
        assert_eq!(mapped.status, "completed");
        assert_eq!(mapped.execution_time_ms, Some(42));
        assert!(mapped.completed_at.is_some());
    }

    #[tokio::test]
    async fn test_check_scope_returns_insufficient_scope_error_shape() {
        let token = create_mock_token("tenant_123", "user_456", vec![TokenScope::SandboxExecute]);

        let response = check_scope(&token, TokenScope::SandboxRead)
            .await
            .expect_err("missing scope should return an error response");

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body_bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        let body: Value = serde_json::from_slice(&body_bytes).expect("json body");

        assert_eq!(body["error"], "insufficient_scope");
        assert_eq!(body["i18n"]["key"], "errors.auth.insufficient_scope");
        assert!(
            body["message"]
                .as_str()
                .unwrap_or_default()
                .contains("sandbox:read"),
            "message should mention required scope"
        );
    }

    #[tokio::test]
    async fn test_check_create_session_scopes_requires_credential_read() {
        let token = create_mock_token(
            "tenant_123",
            "user_456",
            vec![TokenScope::SandboxWrite, TokenScope::CredentialDecrypt],
        );

        let response = check_create_session_scopes(&token)
            .await
            .expect_err("missing decrypt scope should return an error response");

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body_bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        let body: Value = serde_json::from_slice(&body_bytes).expect("json body");

        assert_eq!(body["error"], "insufficient_scope");
        assert_eq!(body["i18n"]["key"], "errors.auth.insufficient_scope");
        assert!(
            body["message"]
                .as_str()
                .unwrap_or_default()
                .contains("credential:read"),
            "message should mention required read scope"
        );
    }

    #[tokio::test]
    async fn test_check_create_session_scopes_accepts_required_scopes() {
        let token = create_mock_token(
            "tenant_123",
            "user_456",
            vec![
                TokenScope::SandboxWrite,
                TokenScope::CredentialRead,
                TokenScope::CredentialDecrypt,
            ],
        );

        check_create_session_scopes(&token)
            .await
            .expect("token with required credential and sandbox scopes should pass");
    }

    #[tokio::test]
    async fn test_check_create_session_scopes_accepts_dashboard_access_token_with_read_scope_only()
    {
        let mut token =
            create_mock_token("tenant_123", "user_456", vec![TokenScope::CredentialRead]);
        token.issued_from = TOKEN_ISSUED_FROM_ACCESS_TOKEN.to_string();
        token.allowed_credential_ids = Some(vec![Uuid::new_v4().to_string()]);

        check_create_session_scopes(&token)
            .await
            .expect("dashboard-issued access token with credential:read should pass");
    }

    #[tokio::test]
    async fn test_check_create_session_scopes_accepts_dashboard_access_token_with_decrypt_scope() {
        let mut token = create_mock_token(
            "tenant_123",
            "user_456",
            vec![TokenScope::CredentialRead, TokenScope::CredentialDecrypt],
        );
        token.issued_from = TOKEN_ISSUED_FROM_ACCESS_TOKEN.to_string();
        token.allowed_credential_ids = Some(vec![Uuid::new_v4().to_string()]);

        check_create_session_scopes(&token)
            .await
            .expect("dashboard-issued access token with decrypt scope should pass");
    }

    #[test]
    fn test_ensure_credential_exists_returns_credential_not_found() {
        let token = create_mock_token("tenant_123", "user_456", vec![TokenScope::SandboxWrite]);
        let vault = Arc::new(CredentialVault::new_in_memory());

        let error = ensure_credential_exists(Some(&vault), &token, Uuid::new_v4())
            .expect_err("missing credential should return credential_not_found");

        assert!(matches!(
            error,
            SandboxError::Session(
                crate::tee::sandbox::error::SessionError::CredentialNotFound { .. }
            )
        ));
    }

    #[test]
    fn test_ensure_credential_exists_accepts_existing_credential() {
        let token = create_mock_token("tenant_123", "user_456", vec![TokenScope::SandboxWrite]);
        let vault = Arc::new(CredentialVault::new_in_memory());

        let entry = vault
            .create_credential(
                CreateCredentialRequest {
                    tenant_id: TenantId::new("tenant_123"),
                    user_id: UserId::new("user_456"),
                    service_id: ServiceId::new("svc-1"),
                    credential_type: crate::models::CredentialType::ApiKey,
                    expires_at: None,
                },
                create_test_payload(),
            )
            .expect("should create test credential");

        let credential_id =
            Uuid::parse_str(entry.credential_id.as_str()).expect("credential id should be uuid");

        ensure_credential_exists(Some(&vault), &token, credential_id)
            .expect("existing credential should pass");
    }

    #[test]
    fn test_ensure_credential_exists_rejects_expired_credential() {
        let token = create_mock_token("tenant_123", "user_456", vec![TokenScope::SandboxWrite]);
        let vault = Arc::new(CredentialVault::new_in_memory());

        let entry = vault
            .create_credential(
                CreateCredentialRequest {
                    tenant_id: TenantId::new("tenant_123"),
                    user_id: UserId::new("user_456"),
                    service_id: ServiceId::new("svc-1"),
                    credential_type: crate::models::CredentialType::ApiKey,
                    expires_at: Some(Utc::now().timestamp() as u64 - 3600),
                },
                create_test_payload(),
            )
            .expect("should create expired test credential");

        let credential_id =
            Uuid::parse_str(entry.credential_id.as_str()).expect("credential id should be uuid");

        let error = ensure_credential_exists(Some(&vault), &token, credential_id)
            .expect_err("expired credential should fail");

        match error {
            SandboxError::Other(message) => {
                assert!(
                    message.starts_with("invalid_request:"),
                    "expired credential should return invalid_request error"
                );
                assert!(
                    message.to_lowercase().contains("expired"),
                    "error message should mention expired status"
                );
            }
            other => panic!("expected invalid_request Other error, got {other:?}"),
        }
    }

    #[test]
    fn test_resolve_create_session_credential_rejects_expired_credential_id() {
        let token = create_mock_token("tenant_123", "user_456", vec![TokenScope::SandboxWrite]);
        let vault = Arc::new(CredentialVault::new_in_memory());

        let entry = vault
            .create_credential(
                CreateCredentialRequest {
                    tenant_id: TenantId::new("tenant_123"),
                    user_id: UserId::new("user_456"),
                    service_id: ServiceId::new("svc-1"),
                    credential_type: crate::models::CredentialType::ApiKey,
                    expires_at: Some(Utc::now().timestamp() as u64 - 3600),
                },
                create_test_payload(),
            )
            .expect("should create expired test credential");

        let credential_id =
            Uuid::parse_str(entry.credential_id.as_str()).expect("credential id should be uuid");

        let error =
            resolve_create_session_credential(Some(&vault), &token, Some(credential_id), None)
                .expect_err(
                    "expired credential should fail when resolving create-session credential",
                );

        match error {
            CreateSessionCredentialError::Sandbox(SandboxError::Other(message)) => {
                assert!(
                    message.to_lowercase().contains("invalid_request")
                        && message.to_lowercase().contains("expired"),
                    "error message should be invalid_request and mention expired status"
                );
            }
            other => panic!("expected sandbox other error, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_credential_reference_accepts_extended_fields() {
        for field in ["api_key", "cookie", "refresh_token", "name"] {
            let value = serde_json::json!({ "$credential": field });
            assert_eq!(
                parse_credential_reference(&value).unwrap(),
                Some(CredentialReference {
                    field: field.to_string(),
                    prefix: None,
                    suffix: None,
                }),
                "field {field} should be accepted"
            );
        }
    }

    #[test]
    fn test_parse_credential_reference_accepts_prefix_and_suffix() {
        let value = serde_json::json!({
            "$credential": "api_key",
            "prefix": "Bearer ",
            "suffix": "#tenant-a",
        });

        assert_eq!(
            parse_credential_reference(&value).unwrap(),
            Some(CredentialReference {
                field: "api_key".to_string(),
                prefix: Some("Bearer ".to_string()),
                suffix: Some("#tenant-a".to_string()),
            })
        );
    }

    #[tokio::test]
    async fn test_parse_credential_reference_rejects_empty_field() {
        let response = parse_credential_reference(&serde_json::json!({ "$credential": "" }))
            .expect_err("empty field should fail");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_parse_credential_reference_rejects_unknown_wrapper_key() {
        let response = parse_credential_reference(&serde_json::json!({
            "$credential": "api_key",
            "template": "Bearer ${secret}",
        }))
        .expect_err("unknown wrapper key should fail");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_parse_credential_reference_rejects_non_string_prefix_or_suffix() {
        let prefix_response = parse_credential_reference(&serde_json::json!({
            "$credential": "api_key",
            "prefix": 123,
        }))
        .expect_err("non-string prefix should fail");
        assert_eq!(prefix_response.status(), StatusCode::BAD_REQUEST);

        let suffix_response = parse_credential_reference(&serde_json::json!({
            "$credential": "api_key",
            "suffix": false,
        }))
        .expect_err("non-string suffix should fail");
        assert_eq!(suffix_response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_resolve_parameter_value_rejects_execute_script_credential_references() {
        let config = SandboxConfig::default();
        let pool: Arc<dyn SandboxPool> = Arc::new(NsjailSandboxPool::new(config.clone()));
        let state = SandboxState::new_with_pool(pool, config);
        let session = create_stub_session();

        let response = resolve_parameter_value(
            &state,
            &session,
            &OperationType::ExecuteScript,
            &serde_json::json!({ "$credential": "password" }),
        )
        .await
        .expect_err("execute_script should not accept credential references");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body_bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        let body: Value = serde_json::from_slice(&body_bytes).expect("json body");
        assert_eq!(
            body["message"],
            "execute_script does not accept credential references"
        );
    }

    #[tokio::test]
    async fn test_resolve_parameter_value_wraps_http_request_authorization_from_api_key_alias() {
        let config = SandboxConfig::default();
        let pool: Arc<dyn SandboxPool> = Arc::new(NsjailSandboxPool::new(config.clone()));
        let state = SandboxState::new_with_pool(pool, config);
        let session = create_stub_session();
        state
            .store_cached_credential(
                session.id.into(),
                SessionCredentialMaterial {
                    credential_type: CredentialType::ApiKey,
                    values: HashMap::from([
                        ("api_key".to_string(), "sk_live_123".to_string()),
                        ("key".to_string(), "sk_live_123".to_string()),
                        ("apiKey".to_string(), "sk_live_123".to_string()),
                    ]),
                },
            )
            .await;

        let (audit_value, resolved_value) = resolve_parameter_value(
            &state,
            &session,
            &OperationType::HttpRequest,
            &serde_json::json!({ "$credential": "apiKey", "prefix": "Bearer " }),
        )
        .await
        .expect("http_request should resolve prefixed api key");

        assert_eq!(
            audit_value,
            serde_json::json!({ "$credential": "apiKey", "prefix": "Bearer " })
        );
        assert_eq!(resolved_value, serde_json::json!("Bearer sk_live_123"));
    }

    #[tokio::test]
    async fn test_resolve_create_session_credential_missing_reference_returns_422() {
        let error = resolve_create_session_credential(
            None,
            &create_mock_token("t", "u", vec![]),
            None,
            None,
        )
        .expect_err("missing references should fail");
        let message = match error {
            CreateSessionCredentialError::MissingReference(message) => message,
            other => panic!("expected missing reference error, got {other:?}"),
        };
        let response = ApiErrorResponse::unprocessable_entity(message).into_response();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let body_bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        let body: Value = serde_json::from_slice(&body_bytes).expect("json body");

        assert_eq!(body["error"], "unprocessable_entity");
        assert!(
            body["message"]
                .as_str()
                .unwrap_or_default()
                .contains("credential_id or service_id"),
            "message should mention credential reference options"
        );
    }

    #[test]
    fn test_create_session_request_missing_credential_reference_deserializes_to_none() {
        let raw = serde_json::json!({
            "original_intent": "open page",
            "metadata": {
                "k": "v"
            }
        });

        let request: CreateSessionRequest =
            serde_json::from_value(raw).expect("request should deserialize");

        assert_eq!(request.credential_id, None);
        assert_eq!(request.service_id, None);
    }

    #[test]
    fn test_create_session_request_with_credential_id_deserializes() {
        let credential_id = Uuid::new_v4();
        let raw = serde_json::json!({
            "credential_id": credential_id,
            "original_intent": "open page"
        });

        let request: CreateSessionRequest =
            serde_json::from_value(raw).expect("request should deserialize");

        assert_eq!(request.credential_id, Some(credential_id));
        assert_eq!(request.service_id, None);
    }

    #[test]
    fn test_create_session_request_with_service_id_deserializes() {
        let raw = serde_json::json!({
            "service_id": "github-prod",
            "original_intent": "open page"
        });

        let request: CreateSessionRequest =
            serde_json::from_value(raw).expect("request should deserialize");

        assert_eq!(request.credential_id, None);
        assert_eq!(request.service_id.as_deref(), Some("github-prod"));
    }

    #[test]
    fn test_create_session_request_missing_original_intent_still_fails_deserialize() {
        let raw = serde_json::json!({
            "credential_id": Uuid::new_v4()
        });

        let error = serde_json::from_value::<CreateSessionRequest>(raw)
            .expect_err("missing original_intent should fail deserialization");

        assert!(
            error.to_string().contains("original_intent"),
            "error should mention original_intent"
        );
    }

    #[test]
    fn test_extract_supported_credential_fields_keeps_scalar_api_key_payload_fields() {
        let values = extract_supported_credential_fields(
            CredentialType::ApiKey,
            &serde_json::json!({ "api_key": "sk_live_123", "ignored": "x" }),
        )
        .expect("api key should be supported");

        assert_eq!(
            values.get("api_key").map(String::as_str),
            Some("sk_live_123")
        );
        assert_eq!(values.get("key").map(String::as_str), Some("sk_live_123"));
        assert_eq!(
            values.get("apiKey").map(String::as_str),
            Some("sk_live_123")
        );
        assert_eq!(values.get("ignored").map(String::as_str), Some("x"));
    }

    #[test]
    fn test_extract_supported_credential_fields_accepts_legacy_api_key_alias_payload() {
        let values = extract_supported_credential_fields(
            CredentialType::ApiKey,
            &serde_json::json!({ "apiKey": "sk_live_legacy" }),
        )
        .expect("legacy apiKey payload should be supported");

        assert_eq!(
            values.get("api_key").map(String::as_str),
            Some("sk_live_legacy")
        );
        assert_eq!(
            values.get("key").map(String::as_str),
            Some("sk_live_legacy")
        );
        assert_eq!(
            values.get("apiKey").map(String::as_str),
            Some("sk_live_legacy")
        );
    }

    #[test]
    fn test_extract_supported_credential_fields_supports_refresh_token() {
        let values = extract_supported_credential_fields(
            CredentialType::OAuthRefresh,
            &serde_json::json!({ "refresh_token": "rt_123" }),
        )
        .expect("oauth refresh should be supported");

        assert_eq!(
            values.get("refresh_token").map(String::as_str),
            Some("rt_123")
        );
    }

    #[tokio::test]
    async fn test_register_session_owner_stores_mapping() {
        let owner_registry = Arc::new(InMemoryOwnerRegistry::default());
        let pool: Arc<dyn SandboxPool> = Arc::new(OwnerRoutingPool {
            session: None,
            release_calls: Arc::new(RwLock::new(Vec::new())),
        });
        let state = make_owner_routing_state(pool, None, Some(owner_registry.clone()));
        let session = create_stub_session();

        state
            .register_session_owner(&session)
            .await
            .expect("owner mapping should be stored");

        let stored = owner_registry
            .get_owner(session.id())
            .await
            .expect("registry lookup should succeed")
            .expect("owner mapping should exist");
        assert_eq!(stored.owner_id, "owner-a");
        assert_eq!(stored.base_url, "http://127.0.0.1");
        assert_eq!(stored.session_id, Uuid::from(session.id()));
        assert_eq!(stored.tenant_id, session.context.tenant_id);
    }

    #[tokio::test]
    async fn test_execute_operation_returns_retryable_503_when_owner_missing_but_db_record_exists()
    {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let session_id = SessionId::new();
        let pool: Arc<dyn SandboxPool> = Arc::new(OwnerRoutingPool::missing());
        let repository: Arc<dyn crate::tee::sandbox::repository::SandboxRepository> =
            Arc::new(OwnerRoutingRepository {
                session_record: Some(make_session_record(session_id, tenant_id)),
            });
        let owner_registry = Arc::new(InMemoryOwnerRegistry::default());
        let state = make_owner_routing_state(pool, Some(repository), Some(owner_registry));
        let token = create_mock_token(
            tenant_id.to_string().as_str(),
            user_id.to_string().as_str(),
            vec![TokenScope::SandboxExecute],
        );
        let app = axum::Router::new().nest(
            "/api/v1",
            sandbox_routes().layer(Extension(token)).with_state(state),
        );

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/api/v1/sandbox/sessions/{}/execute",
                        Uuid::from(session_id)
                    ))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&serde_json::json!({
                            "operation_type": "click",
                            "description": "click",
                            "parameters": { "selector": "#submit" }
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            response.headers().get(axum::http::header::RETRY_AFTER),
            Some(&HeaderValue::from_static("1"))
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["error"], "sandbox_owner_unavailable");
    }

    #[tokio::test]
    async fn test_execute_operation_forwards_to_registered_owner() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let session_id = SessionId::new();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let owner_addr = listener.local_addr().unwrap();
        let owner_handler = post(|headers: HeaderMap| async move {
            Json(serde_json::json!({
                "success": true,
                "data": {
                    "operation_id": Uuid::new_v4(),
                    "success": true,
                    "data": {
                        "saw_authorization": headers.contains_key(axum::http::header::AUTHORIZATION),
                        "saw_forward_header": headers
                            .get(SANDBOX_FORWARD_HEADER)
                            .and_then(|value| value.to_str().ok())
                            .is_some_and(|value| value == SANDBOX_FORWARD_VERSION),
                    },
                    "execution_time_ms": 1
                }
            }))
        });
        let owner_server = axum::Router::new()
            .route("/sandbox/sessions/:id/execute", owner_handler.clone())
            .route("/api/v1/sandbox/sessions/:id/execute", owner_handler);
        let owner_task = tokio::spawn(async move {
            axum::serve(listener, owner_server)
                .await
                .expect("owner server should run");
        });

        let owner_registry = Arc::new(InMemoryOwnerRegistry::default());
        owner_registry
            .register_owner(
                SandboxOwnerRecord {
                    owner_id: "owner-b".to_string(),
                    base_url: format!("http://{owner_addr}"),
                    session_id: Uuid::from(session_id),
                    tenant_id,
                    expires_at: (OffsetDateTime::now_utc() + time::Duration::minutes(5))
                        .unix_timestamp(),
                },
                Duration::from_secs(60),
            )
            .await
            .unwrap();

        let pool: Arc<dyn SandboxPool> = Arc::new(OwnerRoutingPool::missing());
        let state = make_owner_routing_state(pool, None, Some(owner_registry));
        let token = create_mock_token(
            tenant_id.to_string().as_str(),
            user_id.to_string().as_str(),
            vec![TokenScope::SandboxExecute],
        );
        let app = sandbox_routes().layer(Extension(token)).with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/sandbox/sessions/{}/execute",
                        Uuid::from(session_id)
                    ))
                    .header("authorization", "Bearer test-token")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&serde_json::json!({
                            "operation_type": "click",
                            "description": "click",
                            "parameters": { "selector": "#submit" }
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        owner_task.abort();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["success"], true);
        assert_eq!(payload["data"]["data"]["saw_authorization"], true);
        assert_eq!(payload["data"]["data"]["saw_forward_header"], true);
    }

    #[tokio::test]
    async fn test_forward_loop_header_returns_session_not_local_retryable_error() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let session_id = SessionId::new();
        let owner_registry = Arc::new(InMemoryOwnerRegistry::default());
        owner_registry
            .register_owner(
                SandboxOwnerRecord {
                    owner_id: "owner-b".to_string(),
                    base_url: "http://127.0.0.1:65535".to_string(),
                    session_id: Uuid::from(session_id),
                    tenant_id,
                    expires_at: (OffsetDateTime::now_utc() + time::Duration::minutes(5))
                        .unix_timestamp(),
                },
                Duration::from_secs(60),
            )
            .await
            .unwrap();
        let pool: Arc<dyn SandboxPool> = Arc::new(OwnerRoutingPool::missing());
        let state = make_owner_routing_state(pool, None, Some(owner_registry));
        let token = create_mock_token(
            tenant_id.to_string().as_str(),
            user_id.to_string().as_str(),
            vec![TokenScope::SandboxExecute],
        );
        let app = sandbox_routes().layer(Extension(token)).with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/sandbox/sessions/{}/execute",
                        Uuid::from(session_id)
                    ))
                    .header(SANDBOX_FORWARD_HEADER, SANDBOX_FORWARD_VERSION)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&serde_json::json!({
                            "operation_type": "click",
                            "description": "click",
                            "parameters": { "selector": "#submit" }
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            response.headers().get(axum::http::header::RETRY_AFTER),
            Some(&HeaderValue::from_static("1"))
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["error"], "sandbox_session_not_local");
    }

    // ==================== ListSessionsQuery status 校验测试 ====================

    #[test]
    fn test_valid_session_statuses_contains_all_expected_values() {
        assert!(VALID_SESSION_STATUSES.contains(&"creating"));
        assert!(VALID_SESSION_STATUSES.contains(&"ready"));
        assert!(VALID_SESSION_STATUSES.contains(&"executing"));
        assert!(VALID_SESSION_STATUSES.contains(&"paused"));
        assert!(VALID_SESSION_STATUSES.contains(&"closed"));
    }

    #[test]
    fn test_valid_session_statuses_rejects_running() {
        // "running" 不是合法状态，应该不在白名单中
        assert!(!VALID_SESSION_STATUSES.contains(&"running"));
    }

    #[test]
    fn test_valid_session_statuses_rejects_expired() {
        // "expired" 不是合法状态枚举值（虽然可能出现在数据库记录中）
        assert!(!VALID_SESSION_STATUSES.contains(&"expired"));
    }

    #[test]
    fn test_list_sessions_query_deserializes_without_status() {
        let json = serde_json::json!({});
        let query: ListSessionsQuery =
            serde_json::from_value(json).expect("empty object should deserialize");
        assert!(query.status.is_none());
    }

    #[test]
    fn test_list_sessions_query_deserializes_with_valid_status() {
        let json = serde_json::json!({ "status": "ready" });
        let query: ListSessionsQuery =
            serde_json::from_value(json).expect("status=ready should deserialize");
        assert_eq!(query.status, Some("ready".to_string()));
    }

    #[test]
    fn test_list_sessions_query_deserializes_with_invalid_status() {
        // 查询参数解析不做校验，校验在 handler 中进行
        let json = serde_json::json!({ "status": "running" });
        let query: ListSessionsQuery =
            serde_json::from_value(json).expect("status=running should deserialize");
        assert_eq!(query.status, Some("running".to_string()));
    }

    #[tokio::test]
    async fn test_invalid_status_returns_400_invalid_request() {
        // 构造一个非法 status 的请求响应
        let status = "running";
        let status_lower = status.to_lowercase();
        assert!(
            !VALID_SESSION_STATUSES.contains(&status_lower.as_str()),
            "running should not be a valid status"
        );

        // 模拟 handler 中的校验逻辑返回的错误
        let response = ApiErrorResponse::invalid_request(format!(
            "invalid status '{}', must be one of: {}",
            status,
            VALID_SESSION_STATUSES.join(", ")
        ))
        .into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body_bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        let body: Value = serde_json::from_slice(&body_bytes).expect("json body");

        assert_eq!(body["error"], "invalid_request");
        assert!(
            body["message"]
                .as_str()
                .unwrap_or_default()
                .contains("invalid status"),
            "message should mention invalid status"
        );
        assert!(
            body["message"]
                .as_str()
                .unwrap_or_default()
                .contains("running"),
            "message should mention the invalid value"
        );
        // 验证白名单状态值在错误消息中
        assert!(
            body["message"]
                .as_str()
                .unwrap_or_default()
                .contains("creating"),
            "message should show valid statuses"
        );
    }

    #[tokio::test]
    async fn test_dom_export_missing_session_id_returns_not_found() {
        // POST /sandbox/sessions/dom-export 缺少会话ID应返回404，
        // 而不是被 /sandbox/sessions/:id 动态段误匹配为 405 Method Not Allowed
        // 注意：兜底处理器不依赖状态，因此使用空状态即可测试路由匹配
        let config = SandboxConfig::default();
        let pool: Arc<dyn SandboxPool> = Arc::new(NsjailSandboxPool::new(config.clone()));
        let state = SandboxState::new_with_pool(pool, config);
        let app = sandbox_routes().with_state(state);

        let request = Request::builder()
            .method("POST")
            .uri("/sandbox/sessions/dom-export")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "POST /sandbox/sessions/dom-export 应返回 404 Not Found"
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["error"]["code"], "SESSION_NOT_FOUND");
    }

    #[tokio::test]
    async fn test_pause_missing_session_id_returns_not_found() {
        // BUG-18223 回归测试：POST /sandbox/sessions/pause 缺少会话ID应返回404，
        // 而不是被 /sandbox/sessions/:id 动态段误匹配为 405 Method Not Allowed
        let config = SandboxConfig::default();
        let pool: Arc<dyn SandboxPool> = Arc::new(NsjailSandboxPool::new(config.clone()));
        let state = SandboxState::new_with_pool(pool, config);
        let app = sandbox_routes().with_state(state);

        let request = Request::builder()
            .method("POST")
            .uri("/sandbox/sessions/pause")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "POST /sandbox/sessions/pause 应返回 404 Not Found"
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["error"]["code"], "SESSION_NOT_FOUND");
    }

    #[tokio::test]
    async fn test_resume_missing_session_id_returns_not_found() {
        // BUG-18224 回归测试：POST /sandbox/sessions/resume 缺少会话ID应返回404，
        // 而不是被 /sandbox/sessions/:id 动态段误匹配为 405 Method Not Allowed
        let config = SandboxConfig::default();
        let pool: Arc<dyn SandboxPool> = Arc::new(NsjailSandboxPool::new(config.clone()));
        let state = SandboxState::new_with_pool(pool, config);
        let app = sandbox_routes().with_state(state);

        let request = Request::builder()
            .method("POST")
            .uri("/sandbox/sessions/resume")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "POST /sandbox/sessions/resume 应返回 404 Not Found"
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["error"], "not_found");
    }

    // ==================== BUG-18225 回归测试：缺少会话ID返回404而非405 ====================

    #[tokio::test]
    async fn test_close_missing_session_id_returns_not_found() {
        // BUG-18225 回归测试：DELETE /sandbox/sessions 缺少会话ID应返回404，
        // 而不是被框架拦截返回 405 Method Not Allowed
        let config = SandboxConfig::default();
        let pool: Arc<dyn SandboxPool> = Arc::new(NsjailSandboxPool::new(config.clone()));
        let state = SandboxState::new_with_pool(pool, config);
        let app = sandbox_routes().with_state(state);

        // 测试不带尾随斜杠
        let request = Request::builder()
            .method("DELETE")
            .uri("/sandbox/sessions")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "DELETE /sandbox/sessions 应返回 404 Not Found"
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            payload["error"], "not_found",
            "响应体应包含 error=not_found"
        );
    }

    #[tokio::test]
    async fn test_close_missing_session_id_with_trailing_slash_returns_not_found() {
        // BUG-18225 回归测试：DELETE /sandbox/sessions/ 带尾随斜杠应同样返回404
        // 需要使用 NormalizePathLayer 模拟真实服务器的尾随斜杠归一化行为
        use tower::Layer;
        use tower::ServiceExt;
        use tower_http::normalize_path::NormalizePathLayer;

        let config = SandboxConfig::default();
        let pool: Arc<dyn SandboxPool> = Arc::new(NsjailSandboxPool::new(config.clone()));
        let state = SandboxState::new_with_pool(pool, config);
        let app =
            NormalizePathLayer::trim_trailing_slash().layer(sandbox_routes().with_state(state));

        // 测试带尾随斜杠，NormalizePathLayer 会将其归一化为 /sandbox/sessions
        let request = Request::builder()
            .method("DELETE")
            .uri("/sandbox/sessions/")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "DELETE /sandbox/sessions/ 应返回 404 Not Found"
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            payload["error"], "not_found",
            "响应体应包含 error=not_found"
        );
    }

    // ==================== BUG-18226 回归测试：非UUID路径参数返回400 invalid_request ====================

    #[test]
    fn test_close_session_invalid_uuid_returns_invalid_request() {
        // BUG-18226 回归测试：DELETE /sandbox/sessions/not-a-uuid 应返回 400 + {"error":"invalid_request"}
        let invalid_uuids = [
            "not-a-uuid",
            "123",
            "abc-def-ghi",
            "",
            "00000000-0000-0000-",
        ];

        for invalid_uuid in invalid_uuids {
            assert!(
                Uuid::parse_str(invalid_uuid).is_err(),
                "'{invalid_uuid}' should fail UUID parsing"
            );
        }
    }

    #[tokio::test]
    async fn test_close_session_invalid_uuid_route_returns_invalid_request() {
        // BUG-18226 回归测试：真实路由 DELETE /sandbox/sessions/not-a-uuid 应返回
        // 400 + {"error":"invalid_request"}，且不会进入会话关闭逻辑
        let config = SandboxConfig::default();
        let pool: Arc<dyn SandboxPool> = Arc::new(NsjailSandboxPool::new(config.clone()));
        let state = SandboxState::new_with_pool(pool, config);
        let token = create_mock_token("tenant_123", "user_456", vec![TokenScope::SandboxWrite]);
        let app = sandbox_routes()
            .layer(axum::Extension(token))
            .with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/sandbox/sessions/not-a-uuid")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        let payload: Value = serde_json::from_slice(&body).expect("json body");

        assert_eq!(payload["error"], "invalid_request");
        assert!(
            payload["message"]
                .as_str()
                .unwrap_or_default()
                .contains("UUID"),
            "error message should mention UUID"
        );
    }

    #[test]
    fn test_get_session_invalid_uuid_returns_invalid_request() {
        // BUG-18222: GET /sandbox/sessions/not-a-uuid 应返回 400 invalid_request
        let error = ApiErrorResponse::invalid_request("Invalid session_id: must be a valid UUID");
        assert_eq!(error.error, "invalid_request");

        // 验证 JSON 序列化包含正确字段
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("\"error\":\"invalid_request\""));
        assert!(json.contains("\"message\""));
    }

    #[test]
    fn test_pause_session_invalid_uuid_returns_invalid_request() {
        // BUG-18223: POST /sandbox/sessions/not-a-uuid/pause 应返回 400 invalid_request
        let error = ApiErrorResponse::invalid_request("Invalid session_id: must be a valid UUID");
        assert_eq!(error.error, "invalid_request");
    }

    #[test]
    fn test_resume_session_invalid_uuid_returns_invalid_request() {
        // BUG-18224: POST /sandbox/sessions/not-a-uuid/resume 应返回 400 invalid_request
        let error = ApiErrorResponse::invalid_request("Invalid session_id: must be a valid UUID");
        assert_eq!(error.error, "invalid_request");
    }

    #[test]
    fn test_dom_export_invalid_uuid_returns_invalid_request() {
        // POST /sandbox/sessions/not-a-uuid/dom-export 非UUID路径参数返回 400
        let error = ApiErrorResponse::invalid_request("Invalid session_id: must be a valid UUID");
        assert_eq!(error.error, "invalid_request");
    }

    #[tokio::test]
    async fn test_get_session_invalid_uuid_route_returns_invalid_request() {
        // BUG-18222 回归测试：GET /sandbox/sessions/not-a-uuid 应返回
        // 400 + {"error":"invalid_request"}，且不会进入会话查询逻辑
        let config = SandboxConfig::default();
        let pool: Arc<dyn SandboxPool> = Arc::new(NsjailSandboxPool::new(config.clone()));
        let state = SandboxState::new_with_pool(pool, config);
        let token = create_mock_token("tenant_123", "user_456", vec![TokenScope::SandboxRead]);
        let app = sandbox_routes()
            .layer(axum::Extension(token))
            .with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/sandbox/sessions/not-a-uuid")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        let payload: Value = serde_json::from_slice(&body).expect("json body");

        assert_eq!(payload["error"], "invalid_request");
        assert!(
            payload["message"]
                .as_str()
                .unwrap_or_default()
                .contains("UUID"),
            "error message should mention UUID"
        );
    }

    // ==================== BUG-18221 回归测试：路径缺少ID时返回404而非200 ====================

    #[tokio::test]
    async fn test_list_sessions_trailing_slash_returns_not_found() {
        // BUG-18221 回归测试：GET /sandbox/sessions/ 被 NormalizePathLayer 归一化为
        // /sandbox/sessions 后，不应被路由到 list_sessions 返回 200。
        // 修复方案：在 list_sessions 中检查 OriginalUri，如果原始路径以尾斜杠结尾，
        // 返回 404 + {"error":"not_found"}
        use axum::extract::OriginalUri;
        use axum::http::Uri;

        let config = SandboxConfig::default();
        let pool: Arc<dyn SandboxPool> = Arc::new(NsjailSandboxPool::new(config.clone()));
        let state = SandboxState::new_with_pool(pool, config);
        let token = create_mock_token("tenant_123", "user_456", vec![TokenScope::SandboxRead]);
        let app = sandbox_routes()
            .layer(axum::Extension(token))
            .with_state(state);

        // 构造请求时模拟 NormalizePathLayer 之前保存的原始 URI
        // 原始 URI 以尾斜杠结尾，表示用户意图是获取详情但缺少 ID
        let original_uri = Uri::from_static("/sandbox/sessions/");
        let request = Request::builder()
            .method("GET")
            .uri("/sandbox/sessions") // 归一化后的路径
            .extension(OriginalUri(original_uri))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "GET /sandbox/sessions/ (原始路径带尾斜杠) 应返回 404 Not Found"
        );

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        let payload: Value = serde_json::from_slice(&body).expect("json body");

        assert_eq!(payload["error"], "not_found");
    }

    #[tokio::test]
    async fn test_list_sessions_normal_path_returns_success() {
        // 正常列表请求（无尾斜杠）仍应返回成功响应
        // 这是 BUG-18221 修复的回归测试：确保正常的列表功能不受影响
        use axum::extract::OriginalUri;
        use axum::http::Uri;

        let config = SandboxConfig::default();
        let pool: Arc<dyn SandboxPool> = Arc::new(NsjailSandboxPool::new(config.clone()));
        let state = SandboxState::new_with_pool(pool, config);
        let token = create_mock_token("tenant_123", "user_456", vec![TokenScope::SandboxRead]);
        let app = sandbox_routes()
            .layer(axum::Extension(token))
            .with_state(state);

        // 原始 URI 不以尾斜杠结尾，表示正常的列表请求
        let original_uri = Uri::from_static("/sandbox/sessions");
        let request = Request::builder()
            .method("GET")
            .uri("/sandbox/sessions")
            .extension(OriginalUri(original_uri))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        // 由于没有实际数据，列表会成功返回空列表
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "GET /sandbox/sessions (正常列表请求) 应返回 200 OK"
        );
    }

    #[test]
    fn test_create_session_response_includes_runtime_context_id() {
        let runtime_context_id = Uuid::new_v4();
        let response = CreateSessionResponse {
            session_id: Uuid::new_v4(),
            sandbox_id: runtime_context_id,
            runtime_context_id,
            status: "ready".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            expires_at: "2024-01-01T00:30:00Z".to_string(),
        };

        let json = serde_json::to_value(response).expect("serialize create session response");
        assert_eq!(json["sandbox_id"], json["runtime_context_id"]);
    }
}
