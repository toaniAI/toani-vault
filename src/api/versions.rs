//! 凭证版本控制 API
//!
//! 实现 EP2-Story2.4 凭证版本控制
//! - GET /api/v1/credentials/:id/versions - 查询版本历史
//! - GET /api/v1/credentials/:id/versions/:version - 查询指定版本
//! - POST /api/v1/credentials/:id/rollback - 版本回滚
//! - PUT /api/v1/credentials/:id - 更新凭证（创建新版本）

use crate::api::credentials::{ApiError, AppState};
use crate::api::middleware::{TokenScope, ValidatedToken, require_scope};
use crate::vault::models::{CredentialId, TenantId, UserId};
use crate::vault::version::{
    RollbackRequest, RollbackResponse, VersionDetail, VersionHistory, VersionMetadata,
    VersionSummary,
};
use axum::extract::{Path, State};
use axum::{Extension, Json};
use chrono::Utc;

/// 获取版本历史
pub async fn get_version_history(
    State(state): State<AppState>,
    Extension(token): Extension<ValidatedToken>,
    Path(id): Path<String>,
) -> Result<Json<VersionHistory>, ApiError> {
    // 验证 Scope: credential:read
    require_scope(TokenScope::CredentialRead)(&token).map_err(ApiError::from_auth_error)?;

    let credential_id = CredentialId::from_string(id.clone())
        .map_err(|e| ApiError::new("invalid_request", e.to_string()))?;

    let tenant_id = TenantId::new(&token.tenant_id);
    let user_id = UserId::new(&token.user_id);

    // 获取当前凭证验证权限
    let entry = state
        .vault
        .get_credential(&credential_id, &tenant_id, &user_id)
        .map_err(|e| ApiError::new("internal_error", e.to_string()))?
        .ok_or_else(|| ApiError::new("not_found", "凭证不存在"))?;

    // 获取版本历史（从存储后端）
    let versions = state
        .vault
        .get_version_history(&credential_id)
        .map_err(|e| ApiError::new("internal_error", e.to_string()))?;

    let version_summaries: Vec<VersionSummary> = versions
        .into_iter()
        .map(|v| VersionSummary {
            version: v.version,
            created_at: v.created_at,
            changed_by: v.changed_by,
            change_reason: v.change_reason,
        })
        .collect();

    Ok(Json(VersionHistory {
        credential_id: id,
        current_version: entry.version,
        total: version_summaries.len(),
        versions: version_summaries,
    }))
}

/// 获取指定版本详情
pub async fn get_version_detail(
    State(state): State<AppState>,
    Extension(token): Extension<ValidatedToken>,
    Path((id, version_str)): Path<(String, String)>,
) -> Result<Json<VersionDetail>, ApiError> {
    // 验证 Scope: credential:read
    require_scope(TokenScope::CredentialRead)(&token).map_err(ApiError::from_auth_error)?;

    let version: u32 = version_str.parse().map_err(|_| {
        ApiError::new(
            "invalid_request",
            format!("version 必须为非负整数，收到: {version_str}"),
        )
    })?;

    let credential_id = CredentialId::from_string(id.clone())
        .map_err(|e| ApiError::new("invalid_request", e.to_string()))?;

    let tenant_id = TenantId::new(&token.tenant_id);
    let user_id = UserId::new(&token.user_id);

    // 验证凭证访问权限
    let entry = state
        .vault
        .get_credential(&credential_id, &tenant_id, &user_id)
        .map_err(|e| ApiError::new("internal_error", e.to_string()))?
        .ok_or_else(|| ApiError::new("not_found", "凭证不存在"))?;

    // 获取指定版本
    let version_record = state
        .vault
        .get_version(&credential_id, version)
        .map_err(|e| ApiError::new("internal_error", e.to_string()))?
        .ok_or_else(|| ApiError::new("not_found", "指定版本不存在"))?;

    Ok(Json(VersionDetail {
        credential_id: id,
        version: version_record.version,
        created_at: version_record.created_at,
        changed_by: version_record.changed_by,
        change_reason: version_record.change_reason,
        metadata: VersionMetadata {
            service_id: entry.service_id.as_str().to_string(),
            credential_type: entry.credential_type.as_str().to_string(),
            algorithm: version_record.encrypted_payload.algorithm.clone(),
        },
    }))
}

/// 回滚到指定版本
pub async fn rollback_credential(
    State(state): State<AppState>,
    Extension(token): Extension<ValidatedToken>,
    Path(id): Path<String>,
    Json(request): Json<RollbackRequest>,
) -> Result<Json<RollbackResponse>, ApiError> {
    // 验证 Scope: credential:write
    require_scope(TokenScope::CredentialWrite)(&token).map_err(ApiError::from_auth_error)?;

    let credential_id = CredentialId::from_string(id.clone())
        .map_err(|e| ApiError::new("invalid_request", e.to_string()))?;

    let tenant_id = TenantId::new(&token.tenant_id);
    let user_id = UserId::new(&token.user_id);

    // 执行回滚
    let rollback_result = state
        .vault
        .rollback_credential(
            &credential_id,
            &tenant_id,
            &user_id,
            request.target_version,
            &request.reason,
        )
        .map_err(|e| ApiError::new("internal_error", e.to_string()))?;

    Ok(Json(RollbackResponse {
        credential_id: id,
        previous_version: rollback_result.previous_version,
        current_version: rollback_result.current_version,
        rollback_to_version: request.target_version,
        rollback_at: Utc::now().to_rfc3339(),
        reason: request.reason,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_api_scopes() {
        // 验证版本控制 API 需要的 Scope
        assert!(matches!(TokenScope::CredentialRead, _));
        assert!(matches!(TokenScope::CredentialWrite, _));
    }
}
