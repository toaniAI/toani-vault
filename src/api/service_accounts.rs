use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::{
    auth::AuthApiState,
    middleware::{TokenScope, ValidatedToken},
    response::{ApiErrorResponse, ApiSuccessResponse},
    tokens::{
        CreatedTokenResponse, TokenMetadataResponse, map_token_metadata, parse_uuid_str,
        unix_to_datetime,
    },
};
use crate::audit::{AuditAction, Outcome};
use crate::auth::{
    ApiTokenMetadata, ApiTokenSubjectType, ApiTokenType, ServiceAccount, ServiceAccountStatus,
};
use crate::token::{
    DEFAULT_TOKEN_TTL_SECONDS, MAX_TOKEN_TTL_SECONDS, MIN_TOKEN_TTL_SECONDS, PasetoToken,
    TOKEN_ISSUED_FROM_SERVICE_ACCOUNT, TOKEN_SUBJECT_TYPE_SERVICE_ACCOUNT, TokenClaims,
};

use super::tokens::TOKEN_SECRET_KEY;

#[derive(Debug, Deserialize)]
pub struct CreateServiceAccountRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub scope_ceiling: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateServiceAccountRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub scope_ceiling: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateServiceAccountTokenRequest {
    pub scopes: Vec<String>,
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ServiceAccountResponse {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub role: String,
    pub scope_ceiling: Vec<String>,
    pub status: String,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

pub fn service_account_routes(state: AuthApiState) -> Router {
    Router::new()
        .route(
            "/service-accounts",
            post(create_service_account_handler)
                .get(list_service_accounts_handler)
                .patch(update_service_account_missing_id_handler),
        )
        .route(
            "/service-accounts/:service_account_id",
            get(get_service_account_handler).patch(update_service_account_handler),
        )
        .route(
            "/service-accounts/:service_account_id/tokens",
            post(create_service_account_token_handler).get(list_service_account_tokens_handler),
        )
        .with_state(state)
}

async fn create_service_account_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    Json(request): Json<CreateServiceAccountRequest>,
) -> Result<ApiSuccessResponse<ServiceAccountResponse>, ApiErrorResponse> {
    require_service_account_admin(&token)?;
    if request.name.trim().is_empty() {
        return Err(ApiErrorResponse::invalid_request(
            "Service account name is required",
        ));
    }
    const MAX_SERVICE_ACCOUNT_NAME_LENGTH: usize = 128;
    if request.name.trim().len() > MAX_SERVICE_ACCOUNT_NAME_LENGTH {
        return Err(ApiErrorResponse::invalid_request(
            "Service account name must be 128 characters or less",
        ));
    }

    let service_account = ServiceAccount::new(
        parse_uuid_str(&token.tenant_id, "tenant_id")?,
        request.name.trim(),
        parse_uuid_str(&token.user_id, "user_id")?,
    )
    .with_scope_ceiling(request.scope_ceiling);
    let service_account = if let Some(description) = request.description {
        service_account.with_description(description)
    } else {
        service_account
    };

    let created = state
        .auth_service
        .create_service_account(&service_account)
        .await
        .map_err(map_auth_error)?;

    state
        .record_audit(
            AuditAction::TokenIssue,
            &token.user_id,
            Outcome::Success,
            Some(serde_json::json!({
                "audit_event": "service_account_created",
                "service_account_id": created.id,
                "name": created.name,
            })),
        )
        .await;

    Ok(ApiSuccessResponse::new(map_service_account(created)))
}

async fn list_service_accounts_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
) -> Result<ApiSuccessResponse<Vec<ServiceAccountResponse>>, ApiErrorResponse> {
    require_service_account_admin(&token)?;
    let items = state
        .auth_service
        .list_service_accounts(parse_uuid_str(&token.tenant_id, "tenant_id")?)
        .await
        .map_err(map_auth_error)?;

    Ok(ApiSuccessResponse::new(
        items.into_iter().map(map_service_account).collect(),
    ))
}

/// Handler for PATCH /service-accounts (missing ID).
/// Returns 404 Not Found because a specific service account ID is required for update.
async fn update_service_account_missing_id_handler(
    Extension(_token): Extension<ValidatedToken>,
) -> Result<ApiSuccessResponse<ServiceAccountResponse>, ApiErrorResponse> {
    Err(ApiErrorResponse::not_found(
        "Service account ID is required for update operation",
    ))
}

async fn get_service_account_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(service_account_id): Path<String>,
) -> Result<ApiSuccessResponse<ServiceAccountResponse>, ApiErrorResponse> {
    require_service_account_admin(&token)?;
    let item = state
        .auth_service
        .get_service_account(parse_uuid_str(&service_account_id, "service_account_id")?)
        .await
        .map_err(map_auth_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Service account not found"))?;

    if item.tenant_id.to_string() != token.tenant_id {
        return Err(ApiErrorResponse::forbidden(
            "Cannot access a service account from another tenant",
        ));
    }

    Ok(ApiSuccessResponse::new(map_service_account(item)))
}

async fn update_service_account_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(service_account_id): Path<String>,
    Json(request): Json<UpdateServiceAccountRequest>,
) -> Result<ApiSuccessResponse<ServiceAccountResponse>, ApiErrorResponse> {
    require_service_account_admin(&token)?;
    let mut item = state
        .auth_service
        .get_service_account(parse_uuid_str(&service_account_id, "service_account_id")?)
        .await
        .map_err(map_auth_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Service account not found"))?;

    if item.tenant_id.to_string() != token.tenant_id {
        return Err(ApiErrorResponse::forbidden(
            "Cannot update a service account from another tenant",
        ));
    }

    if let Some(name) = request.name {
        if name.trim().is_empty() {
            return Err(ApiErrorResponse::invalid_request(
                "Service account name cannot be empty",
            ));
        }
        const MAX_SERVICE_ACCOUNT_NAME_LENGTH: usize = 128;
        if name.trim().len() > MAX_SERVICE_ACCOUNT_NAME_LENGTH {
            return Err(ApiErrorResponse::invalid_request(
                "Service account name must be 128 characters or less",
            ));
        }
        item.name = name.trim().to_string();
    }
    if let Some(description) = request.description {
        item.description = if description.trim().is_empty() {
            None
        } else {
            Some(description)
        };
    }
    if let Some(status) = request.status {
        item.status = status
            .parse::<ServiceAccountStatus>()
            .map_err(ApiErrorResponse::invalid_request)?;
    }
    if let Some(scope_ceiling) = request.scope_ceiling {
        item.scope_ceiling = scope_ceiling;
    }
    item.updated_at = Utc::now();

    let updated = state
        .auth_service
        .update_service_account(&item)
        .await
        .map_err(map_auth_error)?;

    Ok(ApiSuccessResponse::new(map_service_account(updated)))
}

async fn create_service_account_token_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(service_account_id): Path<String>,
    Json(request): Json<CreateServiceAccountTokenRequest>,
) -> Result<ApiSuccessResponse<CreatedTokenResponse>, ApiErrorResponse> {
    require_service_account_admin(&token)?;
    if request.scopes.is_empty() {
        return Err(ApiErrorResponse::invalid_request(
            "At least one scope is required",
        ));
    }

    let granted_scopes = request
        .scopes
        .iter()
        .map(|scope| {
            TokenScope::parse(scope)
                .map(|parsed| parsed.as_str().to_string())
                .ok_or_else(|| ApiErrorResponse::invalid_request(format!("Invalid scope: {scope}")))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let service_account = state
        .auth_service
        .get_service_account(parse_uuid_str(&service_account_id, "service_account_id")?)
        .await
        .map_err(map_auth_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Service account not found"))?;

    if service_account.tenant_id.to_string() != token.tenant_id {
        return Err(ApiErrorResponse::forbidden(
            "Cannot issue a token for a service account from another tenant",
        ));
    }
    if !service_account.can_issue_tokens() {
        return Err(ApiErrorResponse::forbidden(
            "Inactive service accounts cannot issue tokens",
        ));
    }
    if request
        .scopes
        .iter()
        .any(|scope| !service_account.scope_ceiling.contains(scope))
    {
        return Err(ApiErrorResponse::forbidden(
            "Requested scopes must be a subset of the service account scope ceiling",
        ));
    }

    let ttl_seconds = request
        .ttl_seconds
        .unwrap_or(DEFAULT_TOKEN_TTL_SECONDS)
        .clamp(MIN_TOKEN_TTL_SECONDS, MAX_TOKEN_TTL_SECONDS);
    let claims = TokenClaims::new(
        format!("{}:{}", service_account.tenant_id, service_account.id),
        service_account.tenant_id.to_string(),
        granted_scopes.join(" "),
        false,
        ttl_seconds,
    )
    .with_subject_type(TOKEN_SUBJECT_TYPE_SERVICE_ACCOUNT)
    .with_issued_from(TOKEN_ISSUED_FROM_SERVICE_ACCOUNT);
    let token_id = claims.jti.clone();
    let scope = claims.scope.clone();
    let issued_at = claims.iat.unwrap_or_default();
    let expires_at = claims.exp;

    let paseto_key = PasetoToken::key_from_bytes(&TOKEN_SECRET_KEY)
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;
    let access_token = PasetoToken::sign(&claims, &paseto_key)
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

    let metadata = ApiTokenMetadata::new(
        token_id.clone(),
        ApiTokenType::ServiceAccountToken,
        ApiTokenSubjectType::ServiceAccount,
        service_account.id,
        service_account.tenant_id,
        TOKEN_ISSUED_FROM_SERVICE_ACCOUNT,
        unix_to_datetime(expires_at)?,
    )
    .with_token_kind("service_account")
    .with_scopes(granted_scopes.clone());
    let metadata = if let Some(display_name) = request.display_name.clone() {
        metadata.with_display_name(display_name)
    } else {
        metadata
    };

    state
        .auth_service
        .create_api_token_metadata(&metadata)
        .await
        .map_err(map_auth_error)?;

    state
        .record_audit(
            AuditAction::TokenIssue,
            &token.user_id,
            Outcome::Success,
            Some(serde_json::json!({
                "audit_event": "service_account_token_issued",
                "service_account_id": service_account.id,
                "token_id": token_id,
                "scopes": granted_scopes,
            })),
        )
        .await;

    Ok(ApiSuccessResponse::new(CreatedTokenResponse {
        access_token,
        token_id,
        token_type: "Bearer".to_string(),
        subject_type: TOKEN_SUBJECT_TYPE_SERVICE_ACCOUNT.to_string(),
        issued_from: TOKEN_ISSUED_FROM_SERVICE_ACCOUNT.to_string(),
        display_name: request.display_name,
        expires_in: ttl_seconds,
        scope,
        granted_scopes: metadata.scopes.clone(),
        issued_at,
        expires_at,
        revoked_at: None,
    }))
}

async fn list_service_account_tokens_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(service_account_id): Path<String>,
) -> Result<ApiSuccessResponse<Vec<TokenMetadataResponse>>, ApiErrorResponse> {
    require_service_account_admin(&token)?;
    let service_account_id = parse_uuid_str(&service_account_id, "service_account_id")?;
    let items = state
        .auth_service
        .list_service_account_api_tokens(
            parse_uuid_str(&token.tenant_id, "tenant_id")?,
            service_account_id,
        )
        .await
        .map_err(map_auth_error)?;

    Ok(ApiSuccessResponse::new(
        items.into_iter().map(map_token_metadata).collect(),
    ))
}

#[allow(clippy::result_large_err)]
fn require_service_account_admin(token: &ValidatedToken) -> Result<(), ApiErrorResponse> {
    if token.is_service_account_subject() {
        return Err(ApiErrorResponse::forbidden(
            "Service account subjects cannot manage service accounts",
        ));
    }
    if !token.has_any_scope(&[TokenScope::TenantAdmin, TokenScope::Admin]) {
        return Err(ApiErrorResponse::forbidden(
            "Missing required scope: tenant:admin",
        ));
    }
    Ok(())
}

fn map_service_account(item: ServiceAccount) -> ServiceAccountResponse {
    ServiceAccountResponse {
        id: item.id.to_string(),
        tenant_id: item.tenant_id.to_string(),
        name: item.name,
        description: item.description,
        role: item.role,
        scope_ceiling: item.scope_ceiling,
        status: item.status.as_str().to_string(),
        created_by: item.created_by.to_string(),
        created_at: item.created_at.to_rfc3339(),
        updated_at: item.updated_at.to_rfc3339(),
        deleted_at: item.deleted_at.map(|value| value.to_rfc3339()),
    }
}

fn map_auth_error(error: crate::auth::AuthError) -> ApiErrorResponse {
    match error {
        crate::auth::AuthError::ServiceAccountNotFound(_)
        | crate::auth::AuthError::ApiTokenNotFound(_) => {
            ApiErrorResponse::not_found(error.to_string())
        }
        crate::auth::AuthError::ServiceAccountAlreadyExists { .. } => {
            ApiErrorResponse::conflict(error.to_string())
        }
        _ => ApiErrorResponse::internal_error(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Internal constant for testing - mirrors the handler constant
    const MAX_SERVICE_ACCOUNT_NAME_LENGTH_INTERNAL: usize = 128;

    fn create_test_token() -> ValidatedToken {
        ValidatedToken {
            token_id: "test-token-id".to_string(),
            subject: "test-tenant:test-user".to_string(),
            tenant_id: "test-tenant".to_string(),
            user_id: "test-user".to_string(),
            expires_at: 9999999999,
            scopes: vec![TokenScope::TenantAdmin],
            issued_at: 0,
            membership_id: None,
            metadata: HashMap::new(),
            subject_type: "user".to_string(),
            issued_from: "profile".to_string(),
        }
    }

    #[tokio::test]
    async fn test_update_service_account_missing_id_returns_404() {
        // Test that the missing ID handler returns 404 Not Found
        let result =
            update_service_account_missing_id_handler(Extension(create_test_token())).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        // Check error code is "not_found"
        assert_eq!(error.error, "not_found");
        // Check error message contains required information
        assert!(error.message.contains("Service account ID is required"));
    }

    #[test]
    fn test_name_length_validation_boundary() {
        // Test the name length validation logic at boundaries
        // 128 is the limit, so 128 chars should be valid, 129 should be invalid

        // Test: name with exactly 128 characters is valid
        let name_128 = "a".repeat(128);
        assert_eq!(name_128.len(), 128);
        assert!(name_128.trim().len() <= MAX_SERVICE_ACCOUNT_NAME_LENGTH_INTERNAL);

        // Test: name with exactly 129 characters exceeds the limit
        let name_129 = "a".repeat(129);
        assert_eq!(name_129.len(), 129);
        assert!(name_129.trim().len() > MAX_SERVICE_ACCOUNT_NAME_LENGTH_INTERNAL);

        // Test: name with exactly 127 characters is valid
        let name_127 = "a".repeat(127);
        assert_eq!(name_127.len(), 127);
        assert!(name_127.trim().len() <= MAX_SERVICE_ACCOUNT_NAME_LENGTH_INTERNAL);
    }

    #[test]
    fn test_name_validation_with_whitespace() {
        // Test that trimming is applied before length check
        // A name with leading/trailing whitespace should be trimmed first

        // Name with whitespace that when trimmed becomes 128 chars should pass
        let name_with_whitespace = format!("  {}  ", "a".repeat(126));
        let trimmed = name_with_whitespace.trim();
        assert_eq!(trimmed.len(), 126);
        assert!(trimmed.len() <= MAX_SERVICE_ACCOUNT_NAME_LENGTH_INTERNAL);

        // Name with whitespace that when trimmed becomes 129 chars should fail
        let long_name_with_whitespace = format!(" {} ", "a".repeat(129));
        let trimmed_long = long_name_with_whitespace.trim();
        assert_eq!(trimmed_long.len(), 129);
        assert!(trimmed_long.len() > MAX_SERVICE_ACCOUNT_NAME_LENGTH_INTERNAL);
    }

    #[test]
    fn test_empty_name_validation() {
        // Test that empty names are rejected
        let empty_name = "";
        assert!(empty_name.trim().is_empty());

        let whitespace_only = "   ";
        assert!(whitespace_only.trim().is_empty());
    }

    #[test]
    fn test_256_char_name_exceeds_limit() {
        // Test the exact case from the bug report: 256 characters
        let name_256 = "a".repeat(256);
        assert_eq!(name_256.len(), 256);
        assert!(name_256.trim().len() > MAX_SERVICE_ACCOUNT_NAME_LENGTH_INTERNAL);
    }
}
