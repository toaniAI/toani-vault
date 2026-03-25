//! 审计日志查询 API
//!
//! 实现审计日志的查询、导出和验证功能
//!
//! # API 端点
//!
//! ```text
//! GET  /api/v1/audit/logs          - 查询审计日志列表
//! GET  /api/v1/audit/logs/:id      - 查询审计日志详情
//! POST /api/v1/audit/export        - 导出审计日志
//! POST /api/v1/audit/verify        - 验证审计日志完整性
//! ```
//!
//! # 权限要求
//!
//! 所有端点需要 Scope: `audit:read` 或 `admin`

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ring::digest::{SHA256, digest};
use ring::signature::{ED25519, UnparsedPublicKey};
use std::sync::Arc;

use crate::api::audit_models::*;
use crate::api::middleware::{TokenScope, ValidatedToken};
use crate::audit::{AuditFilter, MemoryAuditStorage, SignedAuditEntry, VerificationProof};

/// 审计 API 状态
#[derive(Clone)]
pub struct AuditApiState {
    /// 审计存储
    pub storage: Arc<dyn AuditStorage>,
    /// Ed25519 验证公钥（32 字节原始格式）
    pub verifier_public_key: Vec<u8>,
}

impl std::fmt::Debug for AuditApiState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditApiState")
            .field("storage", &"<dyn AuditStorage>")
            .finish()
    }
}

/// 审计存储 trait
///
/// 抽象审计存储操作，支持不同的后端实现
#[async_trait::async_trait]
pub trait AuditStorage: Send + Sync {
    /// 查询审计日志
    async fn query(
        &self,
        filter: AuditFilter,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<SignedAuditEntry>, u64), String>;

    /// 按 ID 获取审计条目
    async fn get_by_id(&self, id: &str) -> Result<Option<SignedAuditEntry>, String>;

    /// 按索引获取审计条目
    async fn get_by_index(&self, index: u64) -> Result<Option<SignedAuditEntry>, String>;

    /// 获取验证证明
    async fn get_verification_proof(&self, index: u64)
    -> Result<Option<VerificationProof>, String>;

    /// 验证条目
    async fn verify_entry(&self, index: u64) -> Result<bool, String>;

    /// 获取所有条目（用于导出）
    async fn get_all(&self, filter: AuditFilter) -> Result<Vec<SignedAuditEntry>, String>;
}

/// 内存审计存储适配器
#[derive(Clone)]
pub struct MemoryAuditStorageAdapter {
    storage: Arc<tokio::sync::Mutex<MemoryAuditStorage>>,
}

impl std::fmt::Debug for MemoryAuditStorageAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryAuditStorageAdapter")
            .field("storage", &"<Arc<Mutex<MemoryAuditStorage>>>")
            .finish()
    }
}

impl MemoryAuditStorageAdapter {
    /// 创建新的内存存储适配器
    pub fn new(storage: MemoryAuditStorage) -> Self {
        Self {
            storage: Arc::new(tokio::sync::Mutex::new(storage)),
        }
    }

    /// 从共享的存储创建适配器
    ///
    /// 用于让多个组件共享同一个存储实例
    pub fn from_shared_storage(storage: Arc<tokio::sync::Mutex<MemoryAuditStorage>>) -> Self {
        Self { storage }
    }
}

#[async_trait::async_trait]
impl AuditStorage for MemoryAuditStorageAdapter {
    async fn query(
        &self,
        filter: AuditFilter,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<SignedAuditEntry>, u64), String> {
        let storage = self.storage.lock().await;

        // 获取所有条目
        let all_entries = storage
            .query_recent(100_000)
            .map_err(|e| format!("{e:?}"))?;

        // 过滤
        let mut filtered: Vec<_> = all_entries
            .into_iter()
            .filter(|e| {
                if let Some(start) = filter.start_time
                    && e.entry.timestamp < start
                {
                    return false;
                }
                if let Some(end) = filter.end_time
                    && e.entry.timestamp > end
                {
                    return false;
                }
                if let Some(ref user_hash) = filter.user_id_hash
                    && &e.entry.user_id_hash != user_hash
                {
                    return false;
                }
                if let Some(action) = filter.action
                    && e.entry.action != action
                {
                    return false;
                }
                if let Some(tier) = filter.risk_tier
                    && e.entry.risk_tier != tier
                {
                    return false;
                }
                if let Some(outcome) = filter.outcome
                    && e.entry.outcome != outcome
                {
                    return false;
                }
                if let Some(ref service) = filter.service
                    && &e.entry.service != service
                {
                    return false;
                }
                true
            })
            .collect();

        // 审计日志列表统一为“最新优先”：
        // 先按操作时间倒序，再按日志索引倒序做稳定兜底，避免同毫秒时间戳导致顺序抖动。
        filtered.sort_by(|a, b| {
            b.entry
                .timestamp
                .cmp(&a.entry.timestamp)
                .then_with(|| b.log_index.cmp(&a.log_index))
        });

        let total = filtered.len() as u64;

        // 分页
        let paged: Vec<_> = filtered.into_iter().skip(offset).take(limit).collect();

        Ok((paged, total))
    }

    async fn get_by_id(&self, id: &str) -> Result<Option<SignedAuditEntry>, String> {
        let storage = self.storage.lock().await;
        // 搜索所有条目
        let all_entries = storage
            .query_recent(100_000)
            .map_err(|e| format!("{e:?}"))?;
        Ok(all_entries.into_iter().find(|e| e.entry.id == id))
    }

    async fn get_by_index(&self, index: u64) -> Result<Option<SignedAuditEntry>, String> {
        let storage = self.storage.lock().await;
        storage.get_by_index(index).map_err(|e| format!("{e:?}"))
    }

    async fn get_verification_proof(
        &self,
        _index: u64,
    ) -> Result<Option<VerificationProof>, String> {
        // 内存存储不支持验证证明
        Ok(None)
    }

    async fn verify_entry(&self, index: u64) -> Result<bool, String> {
        let storage = self.storage.lock().await;

        // 获取条目
        let entry = storage.get_by_index(index).map_err(|e| e.to_string())?;

        if entry.is_none() {
            return Ok(false);
        }

        // 验证整个链
        storage.verify().map_err(|e| e.to_string())
    }

    async fn get_all(&self, filter: AuditFilter) -> Result<Vec<SignedAuditEntry>, String> {
        let (entries, _) = self.query(filter, 0, 100_000).await?;
        Ok(entries)
    }
}

/// 检查权限
fn has_audit_permission(token: &ValidatedToken) -> bool {
    token.scopes.contains(&TokenScope::AuditRead) || token.scopes.contains(&TokenScope::Admin)
}

/// 从请求扩展中提取 Token
fn extract_token_from_request(req: &axum::extract::Request) -> Option<&ValidatedToken> {
    req.extensions().get::<ValidatedToken>()
}

/// 查询审计日志列表
///
/// GET /api/v1/audit/logs
pub async fn list_audit_logs(
    State(state): State<AuditApiState>,
    req: axum::extract::Request,
) -> Response {
    // 从查询参数解析
    let params: Query<AuditLogQueryRequest> = match Query::try_from_uri(req.uri()) {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(AuditLogListResponse::error("无效的查询参数")),
            )
                .into_response();
        }
    };

    // 提取 Token
    let token = match extract_token_from_request(&req) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(AuditLogListResponse::error("未提供有效的 Token")),
            )
                .into_response();
        }
    };

    // 验证权限
    if !has_audit_permission(token) {
        return (
            StatusCode::FORBIDDEN,
            Json(AuditLogListResponse::error(
                "权限不足：需要 audit:read 或 admin 权限",
            )),
        )
            .into_response();
    }

    // 验证参数
    if let Err(e) = params.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(AuditLogListResponse::error(e)),
        )
            .into_response();
    }

    // 构建过滤器
    let filter = AuditFilter {
        start_time: params.start_time,
        end_time: params.end_time,
        user_id_hash: params.user_id_hash.clone(),
        action: params.action,
        risk_tier: params.risk_tier,
        outcome: params.outcome,
        service: params.service.clone(),
    };

    // 查询
    match state
        .storage
        .query(filter, params.offset(), params.page_size)
        .await
    {
        Ok((entries, total)) => {
            let items: Vec<AuditLogListItem> = entries.iter().map(AuditLogListItem::from).collect();
            let response =
                AuditLogListResponse::success(items, total, params.page, params.page_size);
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            tracing::error!("查询审计日志失败: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuditLogListResponse::error("查询审计日志失败")),
            )
                .into_response()
        }
    }
}

/// 获取审计日志详情
///
/// GET /api/v1/audit/logs/:id
pub async fn get_audit_log_detail(
    State(state): State<AuditApiState>,
    Path(id): Path<String>,
    req: axum::extract::Request,
) -> Response {
    // 提取 Token
    let token = match extract_token_from_request(&req) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(AuditLogDetailResponse::error("未提供有效的 Token")),
            )
                .into_response();
        }
    };
    // 验证权限
    if !has_audit_permission(token) {
        return (
            StatusCode::FORBIDDEN,
            Json(AuditLogDetailResponse::error(
                "权限不足：需要 audit:read 或 admin 权限",
            )),
        )
            .into_response();
    }

    // 先尝试按 ID 查找
    let entry = match state.storage.get_by_id(&id).await {
        Ok(Some(e)) => e,
        Ok(None) => {
            // 尝试将 ID 作为索引解析
            if let Ok(index) = id.parse::<u64>() {
                match state.storage.get_by_index(index).await {
                    Ok(Some(e)) => e,
                    Ok(None) => {
                        return (
                            StatusCode::NOT_FOUND,
                            Json(AuditLogDetailResponse::error("审计条目未找到")),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(AuditLogDetailResponse::error(e)),
                        )
                            .into_response();
                    }
                }
            } else {
                return (
                    StatusCode::NOT_FOUND,
                    Json(AuditLogDetailResponse::error("审计条目未找到")),
                )
                    .into_response();
            }
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuditLogDetailResponse::error(e)),
            )
                .into_response();
        }
    };

    // 获取验证证明
    let proof = match state.storage.get_verification_proof(entry.log_index).await {
        Ok(Some(p)) => Some(MerkleProofResponse {
            inclusion_proof: p.inclusion_proof,
            consistency_proof: p.consistency_proof,
            tree_size: p.tree_size,
            root_hash: p.root_hash,
            transaction_id: p.transaction_id,
        }),
        _ => None,
    };

    let data = AuditLogDetailData::from((entry, proof));
    let response = AuditLogDetailResponse::success(data);
    (StatusCode::OK, Json(response)).into_response()
}

/// 导出审计日志
///
/// POST /api/v1/audit/export
pub async fn export_audit_logs(
    State(state): State<AuditApiState>,
    req: axum::extract::Request,
) -> Response {
    // 解析请求体（需要所有权）
    let (parts, body) = req.into_parts();
    let bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(AuditExportResponse::error(format!("读取请求体失败：{e}"))),
            )
                .into_response();
        }
    };

    let params: AuditExportRequest = match serde_json::from_slice(&bytes) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(AuditExportResponse::error(format!("无效的请求体：{e}"))),
            )
                .into_response();
        }
    };

    // 重新构建请求以提取 Token
    let req = axum::extract::Request::from_parts(parts, axum::body::Body::empty());

    // 提取 Token
    let token = match extract_token_from_request(&req) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(AuditExportResponse::error("未提供有效的 Token")),
            )
                .into_response();
        }
    };
    // 验证权限
    if !has_audit_permission(token) {
        return (
            StatusCode::FORBIDDEN,
            Json(AuditExportResponse::error(
                "权限不足：需要 audit:read 或 admin 权限",
            )),
        )
            .into_response();
    }

    // 验证参数
    if let Err(e) = params.validate() {
        return (StatusCode::BAD_REQUEST, Json(AuditExportResponse::error(e))).into_response();
    }

    // 构建过滤器
    let filter = AuditFilter {
        start_time: params.start_time,
        end_time: params.end_time,
        user_id_hash: params.user_id_hash.clone(),
        action: params.action,
        risk_tier: None,
        outcome: None,
        service: None,
    };

    // 获取数据
    let entries = match state.storage.get_all(filter).await {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuditExportResponse::error(e)),
            )
                .into_response();
        }
    };

    // 生成导出内容
    let (content, _content_type) = match params.format {
        ExportFormat::Json => {
            let json = match serde_json::to_string_pretty(&entries) {
                Ok(j) => j,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(AuditExportResponse::error(format!("序列化失败: {e}"))),
                    )
                        .into_response();
                }
            };
            (json, "application/json")
        }
        ExportFormat::Csv => {
            let csv = export_to_csv(&entries);
            (csv, "text/csv")
        }
    };

    // 计算完整性哈希
    let integrity_hash = hex::encode(digest(&SHA256, content.as_bytes()).as_ref());

    // 生成导出 ID
    let export_id = uuid::Uuid::now_v7().to_string();

    // Base64 编码内容
    let content_base64 = STANDARD.encode(&content);

    let data = AuditExportData {
        export_id,
        format: params.format,
        content: content_base64,
        integrity_hash,
        count: entries.len() as u64,
        generated_at: current_timestamp_millis(),
    };

    let response = AuditExportResponse::success(data);
    (StatusCode::OK, Json(response)).into_response()
}

/// 导出为 CSV 格式
fn export_to_csv(entries: &[SignedAuditEntry]) -> String {
    let mut csv = String::new();

    // 表头
    csv.push_str("id,timestamp,user_id_hash,session_id,service,action,risk_tier,outcome,tee_mrenclave,action_token_jti,log_index,content_hash\n");

    // 数据行
    for entry in entries {
        let line = format!(
            "{},{},{},{},{},{},{},{},{},{},{},{}\n",
            entry.entry.id,
            entry.entry.timestamp,
            entry.entry.user_id_hash,
            entry.entry.session_id,
            entry.entry.service,
            entry.entry.action,
            entry.entry.risk_tier,
            entry.entry.outcome,
            entry.entry.tee_mrenclave,
            entry.entry.action_token_jti,
            entry.log_index,
            hex::encode(entry.content_hash)
        );
        csv.push_str(&line);
    }

    csv
}

/// 验证审计日志
///
/// POST /api/v1/audit/verify
pub async fn verify_audit_log(
    State(state): State<AuditApiState>,
    req: axum::extract::Request,
) -> Response {
    // 解析请求体（需要所有权）
    let (parts, body) = req.into_parts();
    let bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(AuditVerifyResponse::error(format!("读取请求体失败：{e}"))),
            )
                .into_response();
        }
    };

    let params: AuditVerifyRequest = match serde_json::from_slice(&bytes) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(AuditVerifyResponse::error(format!("无效的请求体：{e}"))),
            )
                .into_response();
        }
    };

    // 重新构建请求以提取 Token
    let req = axum::extract::Request::from_parts(parts, axum::body::Body::empty());

    // 提取 Token
    let token = match extract_token_from_request(&req) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(AuditVerifyResponse::error("未提供有效的 Token")),
            )
                .into_response();
        }
    };
    // 验证权限
    if !has_audit_permission(token) {
        return (
            StatusCode::FORBIDDEN,
            Json(AuditVerifyResponse::error(
                "权限不足：需要 audit:read 或 admin 权限",
            )),
        )
            .into_response();
    }

    // 获取条目
    let entry = if let Some(index) = params.log_index {
        match state.storage.get_by_index(index).await {
            Ok(Some(e)) => e,
            Ok(None) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(AuditVerifyResponse::not_found(format!("索引: {index}"))),
                )
                    .into_response();
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(AuditVerifyResponse::error(e)),
                )
                    .into_response();
            }
        }
    } else {
        match state.storage.get_by_id(&params.id).await {
            Ok(Some(e)) => e,
            Ok(None) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(AuditVerifyResponse::not_found(&params.id)),
                )
                    .into_response();
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(AuditVerifyResponse::error(e)),
                )
                    .into_response();
            }
        }
    };

    // 执行验证
    let mut details = Vec::new();

    // 1. 验证内容哈希：重新计算条目内容哈希并与存储值比较
    let computed_content_hash = entry.entry.content_hash();
    let content_hash_match = computed_content_hash == entry.content_hash;
    details.push(VerificationDetail {
        step: "内容哈希验证".to_string(),
        passed: content_hash_match,
        message: Some(format!("哈希: {}", hex::encode(entry.content_hash))),
    });

    // 2. 验证签名（Ed25519，与 AuditRecorder::record 签名数据格式一致）
    // 首先检查公钥是否为空，如果为空则返回错误
    let signature_valid = if state.verifier_public_key.is_empty() {
        tracing::warn!("[AUDIT-VERIFY] 公钥未配置，无法验证签名");
        // 公钥未配置，签名验证跳过
        false
    } else {
        let combined_data = [entry.content_hash.as_slice(), entry.prev_hash.as_slice()].concat();
        let combined_hash = digest(&SHA256, &combined_data);
        let pub_key = UnparsedPublicKey::new(&ED25519, &state.verifier_public_key);
        pub_key
            .verify(combined_hash.as_ref(), &entry.signature)
            .is_ok()
    };
    details.push(VerificationDetail {
        step: "数字签名验证".to_string(),
        passed: signature_valid,
        message: Some(format!("签名者: {}", entry.signer_fingerprint)),
    });

    // 3. 验证 Merkle 证明
    let merkle_proof_valid: bool = state
        .storage
        .verify_entry(entry.log_index)
        .await
        .unwrap_or_default();
    details.push(VerificationDetail {
        step: "Merkle Tree 验证".to_string(),
        passed: merkle_proof_valid,
        message: Some(format!("Merkle 根: {}", hex::encode(entry.merkle_root))),
    });

    // 总体验证结果
    let verified = content_hash_match && signature_valid && merkle_proof_valid;

    let data = AuditVerifyData {
        id: entry.entry.id.clone(),
        log_index: entry.log_index,
        verified,
        content_hash_match,
        signature_valid,
        merkle_proof_valid,
        details,
        verified_at: current_timestamp_millis(),
    };

    let response = if verified {
        AuditVerifyResponse::verified(data)
    } else {
        AuditVerifyResponse::invalid(data)
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// 创建审计 API 路由
pub fn audit_routes(state: AuditApiState) -> Router {
    Router::new()
        .route("/audit/logs", get(list_audit_logs))
        .route("/audit/logs/:id", get(get_audit_log_detail))
        .route("/audit/export", post(export_audit_logs))
        .route("/audit/verify", post(verify_audit_log))
        .with_state(state)
}

/// 获取当前 Unix 时间戳（毫秒）
fn current_timestamp_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间错误")
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::events::{AuditAction, AuditEntry, Outcome};

    fn create_test_token() -> ValidatedToken {
        ValidatedToken {
            token_id: "test_jti".to_string(),
            subject: "tenant:user".to_string(),
            tenant_id: "tenant".to_string(),
            user_id: "user".to_string(),
            expires_at: 9999999999,
            scopes: vec![TokenScope::AuditRead],
            issued_at: 1000,
        }
    }

    fn create_test_admin_token() -> ValidatedToken {
        ValidatedToken {
            token_id: "admin_jti".to_string(),
            subject: "tenant:admin".to_string(),
            tenant_id: "tenant".to_string(),
            user_id: "admin".to_string(),
            expires_at: 9999999999,
            scopes: vec![TokenScope::Admin],
            issued_at: 1000,
        }
    }

    #[allow(dead_code)]
    fn create_test_storage() -> MemoryAuditStorageAdapter {
        let storage = MemoryAuditStorage::new(1000).unwrap();
        MemoryAuditStorageAdapter::new(storage)
    }

    #[test]
    fn test_has_audit_permission() {
        let audit_token = create_test_token();
        assert!(has_audit_permission(&audit_token));

        let admin_token = create_test_admin_token();
        assert!(has_audit_permission(&admin_token));

        let no_scope_token = ValidatedToken {
            token_id: "test".to_string(),
            subject: "tenant:user".to_string(),
            tenant_id: "tenant".to_string(),
            user_id: "user".to_string(),
            expires_at: 9999999999,
            scopes: vec![TokenScope::CredentialRead],
            issued_at: 1000,
        };
        assert!(!has_audit_permission(&no_scope_token));
    }

    #[test]
    fn test_export_to_csv() {
        let entry = SignedAuditEntry {
            entry: AuditEntry::new(
                "user_hash",
                "session",
                "service",
                AuditAction::CredentialDecrypt,
                Outcome::Success,
                "mrenclave",
                "jti",
            ),
            content_hash: [1u8; 32],
            prev_hash: [0u8; 32],
            signature: vec![1, 2, 3],
            signer_fingerprint: "test_fp".to_string(),
            log_index: 0,
            merkle_root: [2u8; 32],
        };

        let csv = export_to_csv(&[entry]);
        assert!(csv.contains("id,timestamp,user_id_hash"));
        assert!(csv.contains("credential_decrypt"));
        assert!(csv.contains("success"));
    }
}
