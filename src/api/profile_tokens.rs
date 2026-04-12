use std::collections::HashSet;

use super::{
    auth::AuthApiState,
    middleware::{TokenScope, ValidatedToken},
    response::ApiErrorResponse,
    tokens::{
        TOKEN_SECRET_KEY, TokenMetadataResponse, map_token_metadata, parse_uuid_str,
        unix_to_datetime,
    },
};
use crate::audit::{AuditAction, Outcome};
use crate::auth::{
    ApiTokenMetadata, ApiTokenSubjectType, ApiTokenType, MembershipStatus, TenantMembership,
};
use crate::token::{
    MAX_TOKEN_TTL_SECONDS, MIN_TOKEN_TTL_SECONDS, PasetoToken, TOKEN_ISSUED_FROM_AUTOMATION,
    TokenClaims,
};
use axum::{
    Extension, Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    routing::{get, post},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

const AUTOMATION_TOKEN_KIND: &str = "user_automation";
const AUTOMATION_PERMISSION_SOURCE: &str = "membership_subset";
const AUTOMATION_DEFAULT_TOKEN_TTL_SECONDS: u64 = 86_400;
const CREATED_VIA_PROFILE: &str = "profile_dashboard";
const CREATED_VIA_CLI: &str = "cli";
const CREATED_VIA_SDK: &str = "sdk";

#[derive(Debug, Deserialize)]
pub struct CreateAutomationTokenRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub scopes: Vec<String>,
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
    #[serde(default)]
    pub created_via: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateAutomationTokenResponse {
    pub token_value: String,
    pub token_preview: String,
    #[serde(flatten)]
    pub metadata: TokenMetadataResponse,
}

pub fn profile_token_routes(state: AuthApiState) -> Router {
    Router::new()
        .route(
            "/profile/automation-tokens",
            get(list_automation_tokens_handler).post(create_automation_token_handler),
        )
        .route(
            "/profile/automation-tokens/:token_id",
            get(get_automation_token_handler),
        )
        .route(
            "/profile/automation-tokens/:token_id/revoke",
            post(revoke_automation_token_handler),
        )
        .with_state(state)
}

/// 映射 JSON 反序列化错误为 invalid_request，确保返回 400 而不是 422
fn map_create_request_rejection(error: JsonRejection) -> ApiErrorResponse {
    ApiErrorResponse::invalid_request(format!("Invalid automation token request payload: {error}"))
}

async fn create_automation_token_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    payload: Result<Json<CreateAutomationTokenRequest>, JsonRejection>,
) -> Result<Json<CreateAutomationTokenResponse>, ApiErrorResponse> {
    // 处理 JSON 反序列化错误（如必填字段缺失），返回 400 而不是默认的 422
    let Json(request) = payload.map_err(map_create_request_rejection)?;

    ensure_user_token_manager(&token, &[TokenScope::TokensWrite, TokenScope::Admin])?;
    let membership = load_active_membership(&state, &token).await?;

    if request.name.trim().is_empty() {
        return Err(ApiErrorResponse::invalid_request("Token name is required"));
    }
    if request.scopes.is_empty() {
        return Err(ApiErrorResponse::invalid_request(
            "At least one scope is required",
        ));
    }

    let granted_scopes = validate_scope_subset(&request.scopes, &membership, &token)?;
    let ttl_seconds = normalize_automation_ttl(request.ttl_seconds);
    let mut claims = TokenClaims::new(
        token.subject.clone(),
        token.tenant_id.clone(),
        granted_scopes.join(" "),
        false,
        ttl_seconds,
    )
    .with_subject_type(token.subject_type.clone())
    .with_issued_from(TOKEN_ISSUED_FROM_AUTOMATION)
    .with_membership_id(membership.id.to_string());
    if let Some(session_id) = token.session_id() {
        claims = claims.with_session_id(session_id.to_string());
    }

    let paseto_key = PasetoToken::key_from_bytes(&TOKEN_SECRET_KEY)
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;
    let token_value = PasetoToken::sign(&claims, &paseto_key)
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;
    let token_prefix = build_token_prefix(&token_value);

    let mut metadata = ApiTokenMetadata::new(
        claims.jti.clone(),
        ApiTokenType::UserAccessToken,
        ApiTokenSubjectType::User,
        parse_uuid_str(&token.user_id, "user_id")?,
        parse_uuid_str(&token.tenant_id, "tenant_id")?,
        TOKEN_ISSUED_FROM_AUTOMATION,
        unix_to_datetime(claims.exp)?,
    )
    .with_token_kind(AUTOMATION_TOKEN_KIND)
    .with_token_name(request.name.trim())
    .with_token_prefix(token_prefix.clone())
    .with_scopes(granted_scopes.clone())
    .with_membership_id(membership.id)
    .with_membership_role_snapshot(membership.role.as_str())
    .with_permission_source(AUTOMATION_PERMISSION_SOURCE)
    .with_created_via(normalize_created_via(request.created_via.as_deref()))
    .with_display_name(request.name.trim());
    if let Some(session_id) = token.session_id() {
        metadata = metadata.with_session_id(parse_uuid_str(session_id, "session_id")?);
    }
    if let Some(description) = request.description.as_deref()
        && !description.trim().is_empty()
    {
        metadata = metadata.with_description(description.trim());
    }

    let stored = state
        .auth_service
        .create_api_token_metadata(&metadata)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

    state
        .record_audit(
            AuditAction::TokenIssue,
            &token.user_id,
            Outcome::Success,
            Some(serde_json::json!({
                "audit_event": "automation_token_issued",
                "token_id": claims.jti,
                "tenant_id": token.tenant_id,
                "membership_id": membership.id,
                "scopes": granted_scopes,
                "created_via": normalize_created_via(request.created_via.as_deref()),
            })),
        )
        .await;

    Ok(Json(CreateAutomationTokenResponse {
        token_value,
        token_preview: build_token_preview(Some(&token_prefix)),
        metadata: map_token_metadata(stored),
    }))
}

async fn list_automation_tokens_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
) -> Result<Json<Vec<TokenMetadataResponse>>, ApiErrorResponse> {
    ensure_user_token_manager(&token, &[TokenScope::TokensRead, TokenScope::Admin])?;
    let membership = load_active_membership(&state, &token).await?;
    let user_id = parse_uuid_str(&token.user_id, "user_id")?;

    let items = state
        .auth_service
        .list_api_tokens(membership.tenant_id)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

    Ok(Json(
        items
            .into_iter()
            .filter(|item| {
                item.subject_type == ApiTokenSubjectType::User
                    && item.subject_id == user_id
                    && item.token_kind == AUTOMATION_TOKEN_KIND
            })
            .map(map_token_metadata)
            .collect(),
    ))
}

async fn get_automation_token_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(token_id): Path<String>,
) -> Result<Json<TokenMetadataResponse>, ApiErrorResponse> {
    ensure_user_token_manager(&token, &[TokenScope::TokensRead, TokenScope::Admin])?;
    let membership = load_active_membership(&state, &token).await?;
    let user_id = parse_uuid_str(&token.user_id, "user_id")?;

    let item = state
        .auth_service
        .get_api_token_metadata(&token_id)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?
        .ok_or_else(|| ApiErrorResponse::not_found("Automation token not found"))?;

    if item.tenant_id != membership.tenant_id
        || item.subject_id != user_id
        || item.subject_type != ApiTokenSubjectType::User
        || item.token_kind != AUTOMATION_TOKEN_KIND
    {
        return Err(ApiErrorResponse::forbidden(
            "Cannot access another user's automation token",
        ));
    }

    Ok(Json(map_token_metadata(item)))
}

async fn revoke_automation_token_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(token_id): Path<String>,
) -> Result<Json<TokenMetadataResponse>, ApiErrorResponse> {
    ensure_user_token_manager(
        &token,
        &[
            TokenScope::TokensRevoke,
            TokenScope::TokensWrite,
            TokenScope::Admin,
        ],
    )?;
    let membership = load_active_membership(&state, &token).await?;
    let user_id = parse_uuid_str(&token.user_id, "user_id")?;

    let item = state
        .auth_service
        .get_api_token_metadata(&token_id)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?
        .ok_or_else(|| ApiErrorResponse::not_found("Automation token not found"))?;

    if item.tenant_id != membership.tenant_id
        || item.subject_id != user_id
        || item.subject_type != ApiTokenSubjectType::User
        || item.token_kind != AUTOMATION_TOKEN_KIND
    {
        return Err(ApiErrorResponse::forbidden(
            "Cannot revoke another user's automation token",
        ));
    }

    let revoked = state
        .auth_service
        .revoke_api_token_metadata(&token_id, Utc::now())
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?
        .ok_or_else(|| ApiErrorResponse::not_found("Automation token not found"))?;

    state
        .record_audit(
            AuditAction::TokenRevoke,
            &token.user_id,
            Outcome::Success,
            Some(serde_json::json!({
                "audit_event": "automation_token_revoked",
                "token_id": token_id,
                "tenant_id": token.tenant_id,
            })),
        )
        .await;

    Ok(Json(map_token_metadata(revoked)))
}

#[allow(clippy::result_large_err)]
fn ensure_user_token_manager(
    token: &ValidatedToken,
    required_scopes: &[TokenScope],
) -> Result<(), ApiErrorResponse> {
    if !token.is_user_subject() {
        return Err(ApiErrorResponse::forbidden(
            "Only user tokens can manage automation tokens",
        ));
    }
    if token.membership_id().is_none() {
        return Err(ApiErrorResponse::forbidden(
            "An active tenant membership is required",
        ));
    }
    if !token.has_any_scope(required_scopes) {
        return Err(ApiErrorResponse::forbidden(
            "Missing required scope for automation token management",
        ));
    }
    Ok(())
}

async fn load_active_membership(
    state: &AuthApiState,
    token: &ValidatedToken,
) -> Result<TenantMembership, ApiErrorResponse> {
    let membership_id = parse_uuid_str(token.membership_id().unwrap_or_default(), "membership_id")?;
    let membership = state
        .auth_service
        .get_membership_by_id(membership_id)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?
        .ok_or_else(|| ApiErrorResponse::forbidden("Active membership not found"))?;

    let token_user_id = parse_uuid_str(&token.user_id, "user_id")?;
    if membership.user_id != token_user_id
        || membership.tenant_id.to_string() != token.tenant_id
        || membership.status != MembershipStatus::Active
    {
        return Err(ApiErrorResponse::forbidden(
            "Current membership is not allowed to manage automation tokens",
        ));
    }

    Ok(membership)
}

#[allow(clippy::result_large_err)]
fn validate_scope_subset(
    requested_scopes: &[String],
    membership: &TenantMembership,
    token: &ValidatedToken,
) -> Result<Vec<String>, ApiErrorResponse> {
    let membership_scopes = membership.scopes.iter().cloned().collect::<HashSet<_>>();
    let has_admin = membership_scopes.contains(TokenScope::Admin.as_str());

    requested_scopes
        .iter()
        .map(|scope| {
            let parsed = TokenScope::parse(scope).ok_or_else(|| {
                ApiErrorResponse::invalid_request(format!("Invalid scope: {scope}"))
            })?;
            let normalized = parsed.as_str().to_string();
            if !(has_admin || membership_scopes.contains(&normalized)) {
                return Err(ApiErrorResponse::forbidden(
                    "Requested scopes must be a subset of the current membership scopes",
                ));
            }
            if !token.has_scope(&parsed) {
                return Err(ApiErrorResponse::forbidden(
                    "Requested scopes must be a subset of the current token scopes",
                ));
            }
            Ok(normalized)
        })
        .collect()
}

fn build_token_prefix(token_value: &str) -> String {
    token_value.chars().take(18).collect()
}

fn normalize_automation_ttl(ttl_seconds: Option<u64>) -> u64 {
    ttl_seconds
        .unwrap_or(AUTOMATION_DEFAULT_TOKEN_TTL_SECONDS)
        .clamp(MIN_TOKEN_TTL_SECONDS, MAX_TOKEN_TTL_SECONDS)
}

fn build_token_preview(token_prefix: Option<&str>) -> String {
    format!("{}...", token_prefix.unwrap_or("v4.local"))
}

fn normalize_created_via(created_via: Option<&str>) -> &'static str {
    match created_via {
        Some(CREATED_VIA_CLI) => CREATED_VIA_CLI,
        Some(CREATED_VIA_SDK) => CREATED_VIA_SDK,
        _ => CREATED_VIA_PROFILE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::middleware::ValidatedToken;
    use crate::auth::{MembershipRole, MembershipSource};
    use std::collections::HashMap;
    use uuid::Uuid;

    fn membership_with_scopes(scopes: Vec<&str>) -> TenantMembership {
        TenantMembership {
            id: Uuid::now_v7(),
            tenant_id: Uuid::now_v7(),
            user_id: Uuid::now_v7(),
            role: MembershipRole::Admin,
            status: MembershipStatus::Active,
            invited_by: None,
            joined_at: None,
            source: MembershipSource::System,
            scopes: scopes.into_iter().map(str::to_string).collect(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn scope_subset_accepts_membership_scopes() {
        let membership = membership_with_scopes(vec!["credential:read", "audit:read"]);
        let token = ValidatedToken::mock(
            &membership.tenant_id.to_string(),
            &membership.user_id.to_string(),
            vec![TokenScope::CredentialRead, TokenScope::AuditRead],
        );
        let result = validate_scope_subset(
            &["credential:read".to_string(), "audit:read".to_string()],
            &membership,
            &token,
        )
        .expect("membership subset should be accepted");

        assert_eq!(result, vec!["credential:read", "audit:read"]);
    }

    #[test]
    fn scope_subset_rejects_outside_membership_scope() {
        let membership = membership_with_scopes(vec!["credential:read"]);
        let token = ValidatedToken::mock(
            &membership.tenant_id.to_string(),
            &membership.user_id.to_string(),
            vec![TokenScope::CredentialRead, TokenScope::AuditRead],
        );
        let error = validate_scope_subset(&["audit:read".to_string()], &membership, &token)
            .expect_err("non-subset scope should be rejected");

        assert_eq!(error.error, "forbidden");
    }

    #[test]
    fn scope_subset_rejects_outside_current_token_scope() {
        let membership = membership_with_scopes(vec!["credential:read", "audit:read"]);
        let token = ValidatedToken::mock(
            &membership.tenant_id.to_string(),
            &membership.user_id.to_string(),
            vec![TokenScope::CredentialRead],
        );

        let error = validate_scope_subset(&["audit:read".to_string()], &membership, &token)
            .expect_err("current token subset should be enforced");

        assert_eq!(error.error, "forbidden");
        assert!(error.message.contains("current token scopes"));
    }

    #[test]
    fn normalize_created_via_defaults_to_profile() {
        assert_eq!(normalize_created_via(None), CREATED_VIA_PROFILE);
        assert_eq!(normalize_created_via(Some("unknown")), CREATED_VIA_PROFILE);
        assert_eq!(
            normalize_created_via(Some(CREATED_VIA_CLI)),
            CREATED_VIA_CLI
        );
        assert_eq!(
            normalize_created_via(Some(CREATED_VIA_SDK)),
            CREATED_VIA_SDK
        );
    }

    #[test]
    fn automation_manager_requires_user_token_with_scope() {
        let mut metadata = HashMap::new();
        metadata.insert("membership_id".to_string(), Uuid::now_v7().to_string());
        let token = ValidatedToken {
            token_id: "token_1".to_string(),
            subject: "tenant:user".to_string(),
            tenant_id: Uuid::now_v7().to_string(),
            user_id: Uuid::now_v7().to_string(),
            expires_at: u64::MAX,
            scopes: vec![TokenScope::TenantRead],
            issued_at: 0,
            membership_id: Some("legacy-membership".to_string()),
            metadata,
            subject_type: "user".to_string(),
            issued_from: "automation".to_string(),
        };

        let error = ensure_user_token_manager(&token, &[TokenScope::TokensRead])
            .expect_err("missing token scope must fail");
        assert_eq!(error.error, "forbidden");
        assert!(error.message.contains("Missing required scope"));
    }

    #[test]
    fn normalize_automation_ttl_defaults_to_24_hours() {
        assert_eq!(
            normalize_automation_ttl(None),
            AUTOMATION_DEFAULT_TOKEN_TTL_SECONDS
        );
    }

    #[test]
    fn normalize_automation_ttl_clamps_to_min_and_max() {
        assert_eq!(
            normalize_automation_ttl(Some(MIN_TOKEN_TTL_SECONDS.saturating_sub(1))),
            MIN_TOKEN_TTL_SECONDS
        );
        assert_eq!(
            normalize_automation_ttl(Some(MAX_TOKEN_TTL_SECONDS.saturating_add(1))),
            MAX_TOKEN_TTL_SECONDS
        );
    }

    #[test]
    fn map_create_request_rejection_returns_invalid_request() {
        // 使用 MissingJsonContentType 测试 rejection 映射逻辑
        let rejection = JsonRejection::MissingJsonContentType(
            axum::extract::rejection::MissingJsonContentType::default(),
        );
        let response = map_create_request_rejection(rejection);

        // 验证错误结构符合预期：error 为 invalid_request
        assert_eq!(response.error, "invalid_request");
        // 验证消息包含关键信息
        assert!(
            response
                .message
                .contains("Invalid automation token request payload")
        );
    }
}
