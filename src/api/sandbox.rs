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
//! - POST   /api/v1/sandbox/sessions/:id/screenshot - 截图
//! - POST   /api/v1/sandbox/sessions/:id/export   - 导出数据

use crate::api::context::{ApiContext, RequestContext};
use crate::api::middleware::{TokenScope, ValidatedToken, require_scope};
use crate::api::response::{ApiErrorResponse, ApiSuccessResponse, ErrorCode};
use crate::api::websocket::handle_socket;
use crate::tee::sandbox::{
    config::SandboxConfig,
    error::SandboxError,
    pool::{NsjailSandboxPool, SandboxPool},
    repository::{PostgresSandboxRepository, SandboxOperationRecord, SandboxRepository},
    session::SandboxSession,
    types::{OperationRequest, OperationType, SessionId, SessionRequest},
};
use axum::{
    Extension, Json,
    extract::{Path, State, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use time::OffsetDateTime;
use tracing::{error, info, warn};
use uuid::Uuid;

/// 沙箱 API 状态
#[derive(Clone)]
pub struct SandboxState {
    /// 沙箱池
    pub pool: Arc<dyn SandboxPool>,
    /// 沙箱配置
    pub config: SandboxConfig,
    /// 持久化仓储
    pub repository: Option<Arc<dyn SandboxRepository>>,
}

impl SandboxState {
    /// 创建新的沙箱状态
    pub async fn new(
        config: SandboxConfig,
        database_pool: Option<PgPool>,
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
        })
    }

    /// 创建简化版沙箱状态（用于测试）
    pub fn new_with_pool(pool: Arc<dyn SandboxPool>, config: SandboxConfig) -> Self {
        Self {
            pool,
            config,
            repository: None,
        }
    }
}

// ==================== 请求/响应类型 ====================

/// 创建会话请求
#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    /// 凭证 ID
    pub credential_id: Uuid,
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
    /// 沙箱 ID
    pub sandbox_id: Uuid,
    /// 会话状态
    pub status: String,
    /// 创建时间
    pub created_at: String,
    /// 过期时间
    pub expires_at: String,
}

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
    /// 沙箱 ID
    pub sandbox_id: Uuid,
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
    /// 沙箱 ID
    pub sandbox_id: Uuid,
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
#[derive(Debug, Deserialize)]
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

/// 截图响应
#[derive(Debug, Serialize)]
pub struct ScreenshotResponse {
    /// 截图数据（Base64 编码）
    pub screenshot_base64: String,
    /// 格式
    pub format: String,
    /// 宽度
    pub width: u32,
    /// 高度
    pub height: u32,
}

/// 导出数据请求
#[derive(Debug, Deserialize)]
pub struct ExportDataRequest {
    /// 导出格式
    pub format: ExportFormat,
    /// 数据选择器
    pub selectors: Vec<String>,
}

/// 导出格式
#[derive(Debug, Clone, Copy, Deserialize)]
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

    // 构建会话请求
    let session_request = SessionRequest {
        tenant_id: parse_uuid(&token.tenant_id),
        user_id: parse_uuid(&token.user_id),
        credential_id: request.credential_id,
        original_intent: request.original_intent,
        metadata: request.metadata,
    };

    // 获取会话
    match state.pool.acquire_session(session_request).await {
        Ok(session) => {
            let context = session.context();
            let response = CreateSessionResponse {
                session_id: context.session_id.into(),
                sandbox_id: context.sandbox_id.into(),
                status: format!("{:?}", session.status().await).to_lowercase(),
                created_at: context.created_at.to_string(),
                expires_at: context.expires_at.to_string(),
            };

            info!("Session {} created successfully", response.session_id);

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
    Extension(token): Extension<ValidatedToken>,
) -> Response {
    // 验证 Scope: sandbox:read
    if let Err(e) = check_scope(&token, TokenScope::SandboxRead).await {
        return e;
    }

    let tenant_id = parse_uuid(&token.tenant_id);

    if let Some(repository) = &state.repository {
        match repository.list_sessions_by_tenant(tenant_id).await {
            Ok(records) => {
                let sessions = records
                    .into_iter()
                    .map(|record| SessionSummary {
                        session_id: record.session_id.into(),
                        sandbox_id: record.sandbox_id,
                        credential_id: record.credential_id,
                        status: record.status.clone(),
                        original_intent: record.original_intent,
                        created_at: record.started_at.to_rfc3339(),
                        expires_at: record.expires_at.to_rfc3339(),
                        is_expired: chrono::Utc::now() > record.expires_at
                            || record.status == "expired",
                    })
                    .collect::<Vec<_>>();
                let response = ListSessionsResponse {
                    total: sessions.len(),
                    sessions,
                };
                return Json(ApiSuccessResponse::new(response)).into_response();
            }
            Err(error) => {
                warn!(
                    "Failed to read sandbox sessions from repository, fallback to memory: {error}"
                );
            }
        }
    }

    let mut sessions = Vec::new();
    if let Some(pool) = state.pool.as_any().downcast_ref::<NsjailSandboxPool>() {
        for session in pool.list_active_sessions().await {
            let context = session.context().clone();
            if context.tenant_id != tenant_id {
                continue;
            }
            sessions.push(SessionSummary {
                session_id: context.session_id.into(),
                sandbox_id: context.sandbox_id.into(),
                credential_id: context.credential_id,
                status: session.status().await.to_string(),
                original_intent: context.original_intent.clone(),
                created_at: context.created_at.to_string(),
                expires_at: context.expires_at.to_string(),
                is_expired: context.is_expired(),
            });
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
    Path(id): Path<Uuid>,
) -> Response {
    // 验证 Scope: sandbox:read
    if let Err(e) = check_scope(&token, TokenScope::SandboxRead).await {
        return e;
    }

    let session_id = SessionId::from(id);
    let tenant_id = parse_uuid(&token.tenant_id);

    if let Some(repository) = &state.repository {
        match repository.get_session_by_id(tenant_id, session_id).await {
            Ok(Some(record)) => {
                let response = SessionDetailResponse {
                    session_id: record.session_id.into(),
                    sandbox_id: record.sandbox_id,
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
                warn!(
                    "Failed to read sandbox session from repository, fallback to memory: {error}"
                );
            }
        }
    }

    match state.pool.get_session(session_id).await {
        Ok(session) => {
            let context = session.context();
            let status = session.status().await;
            let last_activity = *context.last_activity_at.read().await;

            let response = SessionDetailResponse {
                session_id: context.session_id.into(),
                sandbox_id: context.sandbox_id.into(),
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
        Err(e) => {
            warn!("Session {} not found: {}", id, e);
            map_sandbox_error(e).into_response()
        }
    }
}

/// POST /api/v1/sandbox/sessions/:id/execute - 执行操作
pub async fn execute_operation(
    State(state): State<SandboxState>,
    Extension(token): Extension<ValidatedToken>,
    Path(id): Path<Uuid>,
    Json(request): Json<ExecuteOperationRequest>,
) -> Response {
    // 验证 Scope: sandbox:execute
    if let Err(e) = check_scope(&token, TokenScope::SandboxExecute).await {
        return e;
    }

    let session_id = SessionId::from(id);

    // 获取会话
    let session = match state.pool.get_session(session_id).await {
        Ok(s) => s,
        Err(e) => return map_sandbox_error(e).into_response(),
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

    // 构建操作请求
    let operation = OperationRequest {
        operation_id: Uuid::new_v4(),
        operation_type,
        description: request.description,
        parameters: request.parameters,
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
    Extension(token): Extension<ValidatedToken>,
    Path(id): Path<Uuid>,
) -> Response {
    // 验证 Scope: sandbox:write
    if let Err(e) = check_scope(&token, TokenScope::SandboxWrite).await {
        return e;
    }

    let session_id = SessionId::from(id);

    match state.pool.get_session(session_id).await {
        Ok(session) => match session.pause().await {
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
        },
        Err(e) => map_sandbox_error(e).into_response(),
    }
}

/// POST /api/v1/sandbox/sessions/:id/resume - 恢复会话
pub async fn resume_session(
    State(state): State<SandboxState>,
    Extension(token): Extension<ValidatedToken>,
    Path(id): Path<Uuid>,
) -> Response {
    // 验证 Scope: sandbox:write
    if let Err(e) = check_scope(&token, TokenScope::SandboxWrite).await {
        return e;
    }

    let session_id = SessionId::from(id);

    match state.pool.get_session(session_id).await {
        Ok(session) => match session.resume().await {
            Ok(_) => {
                if let Some(repository) = &state.repository
                    && let Err(error) = repository.update_session_status(session_id, "active").await
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
        },
        Err(e) => map_sandbox_error(e).into_response(),
    }
}

/// DELETE /api/v1/sandbox/sessions/:id - 关闭会话
pub async fn close_session(
    State(state): State<SandboxState>,
    Extension(token): Extension<ValidatedToken>,
    Path(id): Path<Uuid>,
) -> Response {
    // 验证 Scope: sandbox:write
    if let Err(e) = check_scope(&token, TokenScope::SandboxWrite).await {
        return e;
    }

    let session_id = SessionId::from(id);

    match state.pool.release_session(session_id).await {
        Ok(_) => {
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

/// POST /api/v1/sandbox/sessions/:id/screenshot - 截图
pub async fn take_screenshot(
    State(state): State<SandboxState>,
    Extension(token): Extension<ValidatedToken>,
    Path(id): Path<Uuid>,
) -> Response {
    // 验证 Scope: sandbox:execute
    if let Err(e) = check_scope(&token, TokenScope::SandboxExecute).await {
        return e;
    }

    let session_id = SessionId::from(id);

    // 获取会话
    let session = match state.pool.get_session(session_id).await {
        Ok(s) => s,
        Err(e) => return map_sandbox_error(e).into_response(),
    };

    // 构建截图操作
    let operation = OperationRequest {
        operation_id: Uuid::new_v4(),
        operation_type: OperationType::Screenshot,
        description: "Take screenshot".to_string(),
        parameters: HashMap::new(),
        created_at: OffsetDateTime::now_utc(),
    };

    match session.execute_operation(operation).await {
        Ok(result) => {
            // 从执行结果中提取截图数据
            let screenshot_data = result.screenshot.unwrap_or_default();
            let base64_data = STANDARD.encode(&screenshot_data);

            let response = ScreenshotResponse {
                screenshot_base64: base64_data,
                format: "png".to_string(),
                width: 1920, // 默认值，实际应该从结果获取
                height: 1080,
            };

            Json(ApiSuccessResponse::new(response)).into_response()
        }
        Err(e) => map_sandbox_error(e).into_response(),
    }
}

/// POST /api/v1/sandbox/sessions/:id/export - 导出数据
pub async fn export_data(
    State(state): State<SandboxState>,
    Extension(token): Extension<ValidatedToken>,
    Path(id): Path<Uuid>,
    Json(request): Json<ExportDataRequest>,
) -> Response {
    // 验证 Scope: sandbox:execute
    if let Err(e) = check_scope(&token, TokenScope::SandboxExecute).await {
        return e;
    }

    let session_id = SessionId::from(id);

    // 获取会话
    let session = match state.pool.get_session(session_id).await {
        Ok(s) => s,
        Err(e) => return map_sandbox_error(e).into_response(),
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
    check_scope(token, TokenScope::SandboxWrite).await?;
    check_scope(token, TokenScope::CredentialDecrypt).await
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
        "screenshot" => Some(OperationType::Screenshot),
        "export" => Some(OperationType::Export),
        "execute_script" => Some(OperationType::ExecuteScript),
        "wait" => Some(OperationType::Wait),
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
        SandboxError::Session(e) => match e {
            crate::tee::sandbox::error::SessionError::NotFound { .. } => {
                (ErrorCode::NotFound, e.to_string(), StatusCode::NOT_FOUND)
            }
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
        _ => (
            ErrorCode::InternalError,
            error.to_string(),
            StatusCode::INTERNAL_SERVER_ERROR,
        ),
    };

    (status, Json(ApiErrorResponse::new(code, message))).into_response()
}

// ==================== 路由构建 ====================

/// 构建沙箱 API 路由
pub fn sandbox_routes() -> axum::Router<SandboxState> {
    use axum::routing::{delete, get, post};

    axum::Router::new()
        // 会话管理
        .route("/sandbox/sessions", post(create_session))
        .route("/sandbox/sessions", get(list_sessions))
        .route("/sandbox/sessions/:id", get(get_session))
        .route("/sandbox/sessions/:id/execute", post(execute_operation))
        .route("/sandbox/operations/:operation_id", get(get_operation))
        .route("/sandbox/sessions/:id/pause", post(pause_session))
        .route("/sandbox/sessions/:id/resume", post(resume_session))
        .route("/sandbox/sessions/:id", delete(close_session))
        .route("/sandbox/sessions/:id/screenshot", post(take_screenshot))
        .route("/sandbox/sessions/:id/export", post(export_data))
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
    if let Err(e) = check_scope(&token, TokenScope::SandboxExecute).await {
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
    use crate::tee::sandbox::{SessionId, repository::SandboxOperationRecord};
    use axum::body::to_bytes;
    use chrono::Utc;
    use serde_json::Value;
    use uuid::Uuid;

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
            parse_operation_type("screenshot"),
            Some(OperationType::Screenshot)
        ));
        assert!(parse_operation_type("invalid").is_none());
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
    async fn test_check_create_session_scopes_requires_credential_decrypt() {
        let token = create_mock_token("tenant_123", "user_456", vec![TokenScope::SandboxWrite]);

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
                .contains("credential:decrypt"),
            "message should mention required decrypt scope"
        );
    }

    #[tokio::test]
    async fn test_check_create_session_scopes_accepts_required_scopes() {
        let token = create_mock_token(
            "tenant_123",
            "user_456",
            vec![TokenScope::SandboxWrite, TokenScope::CredentialDecrypt],
        );

        check_create_session_scopes(&token)
            .await
            .expect("token with sandbox:write + credential:decrypt should pass");
    }
}
