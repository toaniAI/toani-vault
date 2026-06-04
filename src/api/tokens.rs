use chrono::{DateTime, Utc};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::{
    auth::AuthApiState,
    middleware::{TokenScope, ValidatedToken},
    response::{ApiErrorResponse, PaginatedResponse},
};
use crate::audit::{AuditAction, Outcome};
use crate::auth::{ApiTokenMetadata, ApiTokenSubjectType, ApiTokenType};
use crate::token::{
    DEFAULT_TOKEN_TTL_SECONDS, MAX_TOKEN_TTL_SECONDS, MIN_TOKEN_TTL_SECONDS, PasetoToken,
    TOKEN_ISSUED_FROM_ACCESS_TOKEN, TOKEN_ISSUED_FROM_SERVICE_ACCOUNT,
    TOKEN_SUBJECT_TYPE_SERVICE_ACCOUNT, TOKEN_SUBJECT_TYPE_USER, TokenClaims,
};
use crate::vault::models::{CredentialId, TenantId, UserId};

#[derive(Debug, Clone, Deserialize)]
pub struct CreateTokenRequest {
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub credential_ids: Vec<String>,
    #[serde(default)]
    pub token_name: Option<String>,
    #[serde(default)]
    pub binding_handles: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatedTokenResponse {
    pub access_token: String,
    pub token: String,
    pub token_id: String,
    pub token_name: Option<String>,
    pub token_type: String,
    pub subject_type: String,
    pub issued_from: String,
    pub display_name: Option<String>,
    pub expires_in: u64,
    pub scope: String,
    pub granted_scopes: Vec<String>,
    pub token_plane: String,
    pub credential_ids: Vec<String>,
    pub binding_handles: Vec<String>,
    pub issued_at: u64,
    pub expires_at: u64,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenMetadataResponse {
    pub token_id: String,
    pub token_kind: String,
    pub token_name: Option<String>,
    pub token_prefix: Option<String>,
    pub token_type: String,
    pub subject_type: String,
    pub subject_id: String,
    pub tenant_id: String,
    pub issued_from: String,
    pub token_plane: String,
    pub session_id: Option<String>,
    pub membership_id: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub granted_scopes: Vec<String>,
    pub credential_ids: Vec<String>,
    pub binding_handles: Vec<String>,
    pub issued_membership_role_snapshot: Option<String>,
    pub permission_source: Option<String>,
    pub created_via: Option<String>,
    pub revoked_reason: Option<String>,
    pub expires_at: String,
    pub revoked_at: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

pub type ListTokensResponse = PaginatedResponse<TokenMetadataResponse>;

#[derive(Debug, Deserialize)]
struct ListTokensQuery {
    #[serde(default = "default_page")]
    page: usize,
    #[serde(default = "default_page_size")]
    page_size: usize,
}

fn default_page() -> usize {
    1
}

fn default_page_size() -> usize {
    20
}

#[derive(Debug, Serialize)]
pub struct TokenStatsResponse {
    pub total_tokens: u64,
    pub active_tokens: u64,
    pub revoked_tokens: u64,
}

#[derive(Debug, Serialize)]
pub struct RevokeTokenResponse {
    pub revoked: bool,
    pub token_id: String,
}

pub fn token_routes(state: AuthApiState) -> Router {
    Router::new()
        .route(
            "/tokens",
            post(create_token_handler).get(list_tokens_handler),
        )
        .route("/tokens/:token_id", get(get_token_handler))
        .route("/tokens/stats", get(get_token_stats_handler))
        .route("/tokens/:token_id/revoke", post(revoke_token_handler))
        .with_state(state)
}

pub async fn create_token_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    Json(request): Json<CreateTokenRequest>,
) -> Result<Json<CreatedTokenResponse>, ApiErrorResponse> {
    let token_name = normalize_token_name(request.token_name.clone())?;
    let created = issue_access_token_from_user_token(&state, &token, request, token_name).await?;
    Ok(Json(created))
}

pub async fn issue_access_token_from_user_token(
    state: &AuthApiState,
    token: &ValidatedToken,
    request: CreateTokenRequest,
    token_name: Option<String>,
) -> Result<CreatedTokenResponse, ApiErrorResponse> {
    if !token.is_user_subject() {
        return Err(ApiErrorResponse::forbidden(
            "Only user tokens can issue API access tokens",
        ));
    }

    if token.membership_id().is_none() {
        return Err(ApiErrorResponse::forbidden(
            "An active tenant membership is required",
        ));
    }

    if !token.has_any_scope(&[TokenScope::TokensWrite, TokenScope::Admin]) {
        return Err(ApiErrorResponse::forbidden(
            "Missing required scope: tokens:write",
        ));
    }

    if request.scopes.is_empty() {
        return Err(ApiErrorResponse::invalid_request(
            "At least one scope is required",
        ));
    }

    if request.credential_ids.is_empty() && request.binding_handles.is_empty() {
        return Err(ApiErrorResponse::invalid_request(
            "At least one binding_handle or credential_id is required",
        ));
    }

    let mut requested_scopes = Vec::with_capacity(request.scopes.len());
    for scope in &request.scopes {
        let parsed = TokenScope::parse(scope)
            .ok_or_else(|| ApiErrorResponse::invalid_request(format!("Invalid scope: {scope}")))?;
        requested_scopes.push(parsed);
    }

    if requested_scopes != vec![TokenScope::CredentialRead] {
        return Err(ApiErrorResponse::invalid_request(
            "Only credential:read scope is supported for dashboard-issued tokens",
        ));
    }

    if requested_scopes.iter().any(|scope| !token.has_scope(scope)) {
        return Err(ApiErrorResponse::forbidden(
            "Requested scopes must be a subset of the current token scopes",
        ));
    }

    let credential_ids = if request.credential_ids.is_empty() {
        Vec::new()
    } else {
        resolve_allowed_credential_ids(state, token, &request.credential_ids)?
    };
    let binding_handles = if request.binding_handles.is_empty() {
        Vec::new()
    } else {
        resolve_allowed_binding_handles(state, token, &request.binding_handles).await?
    };

    let ttl_seconds = normalize_ttl(request.expires_in);
    let granted_scopes = TokenScope::expand_credential_read_permissions(&requested_scopes)
        .into_iter()
        .map(|scope| scope.as_str().to_string())
        .collect::<Vec<_>>();
    let scope_string = granted_scopes.join(" ");
    let mut claims = TokenClaims::new(
        token.subject.clone(),
        token.tenant_id.clone(),
        scope_string.clone(),
        false,
        ttl_seconds,
    )
    .with_subject_type(TOKEN_SUBJECT_TYPE_USER)
    .with_issued_from(TOKEN_ISSUED_FROM_ACCESS_TOKEN)
    .with_token_plane("runtime")
    .with_binding_handles(binding_handles.clone());

    if let Some(membership_id) = token.membership_id() {
        claims = claims.with_membership_id(membership_id);
    }
    if let Some(session_id) = token.session_id() {
        claims = claims.with_session_id(session_id);
    }

    let paseto_key = PasetoToken::key_from_bytes(state.token_secret_key.as_slice())
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;
    let access_token = PasetoToken::sign(&claims, &paseto_key)
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;
    let token_id = claims.jti.clone();
    let metadata = ApiTokenMetadata::new(
        token_id.clone(),
        ApiTokenType::UserAccessToken,
        ApiTokenSubjectType::User,
        parse_uuid_str(&token.user_id, "user_id")?,
        parse_uuid_str(&token.tenant_id, "tenant_id")?,
        TOKEN_ISSUED_FROM_ACCESS_TOKEN,
        unix_to_datetime(claims.exp)?,
    )
    .with_token_kind("user_access_token")
    .with_scopes(granted_scopes.clone())
    .with_credential_ids(credential_ids.clone())
    .with_binding_handles(binding_handles.clone())
    .with_token_plane("runtime");
    let metadata = if let Some(token_name) = token_name {
        metadata.with_token_name(token_name)
    } else {
        metadata
    };
    let metadata = if let Some(session_id) = token.session_id() {
        metadata.with_session_id(parse_uuid_str(session_id, "session_id")?)
    } else {
        metadata
    };
    let metadata = if let Some(membership_id) = token.membership_id() {
        metadata.with_membership_id(parse_uuid_str(membership_id, "membership_id")?)
    } else {
        metadata
    };

    state
        .auth_service
        .create_api_token_metadata(&metadata)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

    if let Err(error) = state
        .record_audit(
            AuditAction::TokenIssue,
            &token.user_id,
            Outcome::Success,
            Some(json!({
                "issued_from": TOKEN_ISSUED_FROM_ACCESS_TOKEN,
                "subject_type": TOKEN_SUBJECT_TYPE_USER,
                "token_plane": "runtime",
                "token_id": token_id,
                "scopes": granted_scopes,
                "credential_ids": credential_ids,
                "binding_handles": binding_handles,
                "expires_in": ttl_seconds,
                "session_id": token.session_id(),
                "membership_id": token.membership_id(),
            })),
        )
        .await
    {
        tracing::warn!(
            user_id = %token.user_id,
            token_id = %token_id,
            "token metadata created but audit write failed: {error}"
        );
    }

    Ok(CreatedTokenResponse {
        token: access_token.clone(),
        access_token,
        token_id: claims.jti.clone(),
        token_name: metadata.token_name.clone(),
        token_type: "Bearer".to_string(),
        subject_type: TOKEN_SUBJECT_TYPE_USER.to_string(),
        issued_from: TOKEN_ISSUED_FROM_ACCESS_TOKEN.to_string(),
        token_plane: "runtime".to_string(),
        display_name: metadata.display_name.clone(),
        expires_in: ttl_seconds,
        scope: scope_string,
        granted_scopes: granted_scopes.clone(),
        credential_ids,
        binding_handles,
        issued_at: claims.iat.unwrap_or_else(unix_now),
        expires_at: claims.exp,
        revoked_at: None,
    })
}

#[allow(clippy::result_large_err)]
fn normalize_token_name(token_name: Option<String>) -> Result<Option<String>, ApiErrorResponse> {
    const MAX_TOKEN_NAME_CHARS: usize = 128;

    if let Some(token_name) = token_name {
        let trimmed = token_name.trim();
        if trimmed.is_empty() {
            return Err(ApiErrorResponse::invalid_request(
                "token_name cannot be empty",
            ));
        }
        if trimmed.chars().count() > MAX_TOKEN_NAME_CHARS {
            return Err(ApiErrorResponse::invalid_request(
                "token_name must be 128 characters or less",
            ));
        }
        return Ok(Some(trimmed.to_string()));
    }

    Ok(None)
}

async fn revoke_token_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(token_id): Path<String>,
) -> Result<Json<RevokeTokenResponse>, ApiErrorResponse> {
    // Calculate is_self first to determine appropriate error for session tokens
    let is_self = token.token_id == token_id;

    // Session tokens can only revoke OTHER tokens (with proper permissions),
    // not themselves. Self-revocation must go through /auth/logout.
    if is_self && token.session_id() == Some(token.token_id.as_str()) {
        return Err(ApiErrorResponse::invalid_request(
            "Session tokens must be revoked via /auth/logout",
        ));
    }

    let can_revoke_others = token.has_any_scope(&[
        TokenScope::TokensRevoke,
        TokenScope::TenantAdmin,
        TokenScope::Admin,
    ]);

    if !is_self && !can_revoke_others {
        return Err(ApiErrorResponse::forbidden(
            "Can only revoke the current token unless you have admin revoke permissions",
        ));
    }

    let metadata = state
        .auth_service
        .get_api_token_metadata(&token_id)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

    if let Some(ref item) = metadata
        && item.tenant_id.to_string() != token.tenant_id
    {
        return Err(ApiErrorResponse::forbidden(
            "Cannot revoke a token from another tenant",
        ));
    }

    if !is_self && metadata.is_none() {
        return Err(ApiErrorResponse::not_found("Token metadata not found"));
    }

    let ttl = metadata
        .as_ref()
        .and_then(|item| item.expires_at.timestamp().try_into().ok())
        .unwrap_or(token.expires_at)
        .saturating_sub(unix_now())
        .max(1);
    state
        .token_store
        .blacklist_token(&token_id, ttl)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

    let revoked_at = Utc::now();
    let metadata = state
        .auth_service
        .revoke_api_token_metadata(&token_id, revoked_at)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

    let subject_type = metadata
        .as_ref()
        .map(|item| item.subject_type.as_str())
        .unwrap_or(token.subject_type());
    let issued_from = metadata
        .as_ref()
        .map(|item| item.issued_from.as_str())
        .unwrap_or(token.issued_from());
    let audit_user_id = if subject_type == TOKEN_SUBJECT_TYPE_SERVICE_ACCOUNT {
        "service-account"
    } else if is_self {
        token.user_id.as_str()
    } else {
        "tenant-admin"
    };

    let audit_action = if issued_from == TOKEN_ISSUED_FROM_SERVICE_ACCOUNT {
        "service_account_token_revoked"
    } else {
        "access_token_revoked"
    };

    if let Err(error) = state
        .record_audit(
            AuditAction::TokenRevoke,
            audit_user_id,
            Outcome::Success,
            Some(json!({
                "token_id": token_id,
                "issued_from": issued_from,
                "subject_type": subject_type,
                "audit_event": audit_action,
                "revoked_by": token.user_id,
            })),
        )
        .await
    {
        tracing::warn!(
            revoked_token_id = %token_id,
            revoked_by = %token.user_id,
            "token revoked but audit write failed: {error}"
        );
    }

    Ok(Json(RevokeTokenResponse {
        revoked: true,
        token_id,
    }))
}

async fn list_tokens_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    Query(query): Query<ListTokensQuery>,
) -> Result<Json<ListTokensResponse>, ApiErrorResponse> {
    if query.page == 0 {
        return Err(ApiErrorResponse::invalid_request("page must be >= 1"));
    }
    if query.page_size == 0 || query.page_size > 100 {
        return Err(ApiErrorResponse::invalid_request(
            "page_size must be between 1 and 100",
        ));
    }

    if !token.has_any_scope(&[
        TokenScope::TokensRead,
        TokenScope::Admin,
        TokenScope::TenantAdmin,
    ]) {
        return Err(ApiErrorResponse::forbidden(
            "Missing required scope: tokens:read",
        ));
    }

    let tenant_id = parse_uuid_str(&token.tenant_id, "tenant_id")?;
    let subject_id = parse_uuid_str(&token.user_id, "user_id")?;
    let is_tenant_admin = token.has_any_scope(&[TokenScope::Admin, TokenScope::TenantAdmin]);

    let items = state
        .auth_service
        .list_api_tokens(tenant_id)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

    let visible_items = items
        .into_iter()
        .filter(|item| {
            if is_tenant_admin {
                return true;
            }

            match item.subject_type {
                ApiTokenSubjectType::User => item.subject_id == subject_id,
                ApiTokenSubjectType::ServiceAccount => {
                    token.is_service_account_subject() && item.subject_id == subject_id
                }
            }
        })
        .collect::<Vec<_>>();

    let total = visible_items.len();
    let offset = (query.page - 1) * query.page_size;
    let items = visible_items
        .into_iter()
        .skip(offset)
        .take(query.page_size)
        .map(map_token_metadata)
        .collect();
    Ok(Json(ListTokensResponse::new(
        items,
        query.page,
        query.page_size,
        total,
    )))
}

async fn get_token_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(token_id): Path<String>,
) -> Result<Json<TokenMetadataResponse>, ApiErrorResponse> {
    if !token.has_any_scope(&[
        TokenScope::TokensRead,
        TokenScope::Admin,
        TokenScope::TenantAdmin,
    ]) {
        return Err(ApiErrorResponse::forbidden(
            "Missing required scope: tokens:read",
        ));
    }

    let token_id = Uuid::parse_str(&token_id)
        .map_err(|_| ApiErrorResponse::invalid_request("Invalid token_id format"))?
        .to_string();

    let subject_id = parse_uuid_str(&token.user_id, "user_id")?;
    let is_tenant_admin = token.has_any_scope(&[TokenScope::Admin, TokenScope::TenantAdmin]);
    let item = state
        .auth_service
        .get_api_token_metadata(&token_id)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?
        .ok_or_else(|| ApiErrorResponse::not_found("Token metadata not found"))?;

    if item.tenant_id.to_string() != token.tenant_id {
        return Err(ApiErrorResponse::forbidden(
            "Cannot access a token from another tenant",
        ));
    }

    if !is_tenant_admin && item.subject_id != subject_id {
        return Err(ApiErrorResponse::forbidden(
            "Cannot access another subject's token",
        ));
    }

    Ok(Json(map_token_metadata(item)))
}

async fn get_token_stats_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
) -> Result<Json<TokenStatsResponse>, ApiErrorResponse> {
    if !token.has_any_scope(&[
        TokenScope::TokensRead,
        TokenScope::Admin,
        TokenScope::TenantAdmin,
    ]) {
        return Err(ApiErrorResponse::forbidden(
            "Missing required scope: tokens:read",
        ));
    }

    let tenant_id = parse_uuid_str(&token.tenant_id, "tenant_id")?;
    let subject_id = parse_uuid_str(&token.user_id, "user_id")?;
    let is_tenant_admin = token.has_any_scope(&[TokenScope::Admin, TokenScope::TenantAdmin]);

    let items = state
        .auth_service
        .list_api_tokens(tenant_id)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

    let visible = items.into_iter().filter(|item| {
        if is_tenant_admin {
            return true;
        }
        item.subject_id == subject_id
    });

    let mut total_tokens = 0u64;
    let mut active_tokens = 0u64;
    let mut revoked_tokens = 0u64;
    for item in visible {
        total_tokens += 1;
        if item.revoked_at.is_some() {
            revoked_tokens += 1;
        } else if item.is_active() {
            active_tokens += 1;
        }
    }

    Ok(Json(TokenStatsResponse {
        total_tokens,
        active_tokens,
        revoked_tokens,
    }))
}

#[allow(clippy::result_large_err)]
fn resolve_allowed_credential_ids(
    state: &AuthApiState,
    token: &ValidatedToken,
    requested_ids: &[String],
) -> Result<Vec<String>, ApiErrorResponse> {
    let vault = state
        .vault
        .as_ref()
        .ok_or_else(|| ApiErrorResponse::internal_error("Credential vault is not configured"))?;
    let tenant_id = TenantId::new(token.tenant_id.clone());
    let user_id = UserId::new(token.user_id.clone());
    let mut unique_ids = Vec::new();

    for requested_id in requested_ids {
        let normalized = requested_id.trim();
        if normalized.is_empty() {
            return Err(ApiErrorResponse::invalid_request(
                "credential_ids must not contain empty values",
            ));
        }

        if !token.can_access_credential(normalized) {
            return Err(ApiErrorResponse::forbidden(
                "Requested credential_ids must be allowed by the current token",
            ));
        }

        let credential_id = CredentialId::from_string(normalized.to_string())
            .map_err(|_| ApiErrorResponse::invalid_request("Invalid credential_id"))?;

        match vault.get_credential_metadata(&credential_id, &tenant_id, &user_id) {
            Ok(Some(_)) => {
                let normalized = credential_id.as_str().to_string();
                if !unique_ids.contains(&normalized) {
                    unique_ids.push(normalized);
                }
            }
            Ok(None) => {
                return Err(ApiErrorResponse::forbidden(
                    "Requested credential_ids must be accessible to the current user",
                ));
            }
            Err(error) => {
                return Err(ApiErrorResponse::internal_error(format!(
                    "Credential access check failed: {error}"
                )));
            }
        }
    }

    if unique_ids.is_empty() {
        return Err(ApiErrorResponse::invalid_request(
            "At least one credential_id is required",
        ));
    }

    Ok(unique_ids)
}

#[allow(clippy::result_large_err)]
async fn resolve_allowed_binding_handles(
    state: &AuthApiState,
    token: &ValidatedToken,
    requested_handles: &[String],
) -> Result<Vec<String>, ApiErrorResponse> {
    let broker_service = state.oauth_broker_service.as_ref().ok_or_else(|| {
        ApiErrorResponse::internal_error("OAuth broker service is not configured")
    })?;

    let tenant_id = parse_uuid_str(&token.tenant_id, "tenant_id")?;
    let mut unique_handles = Vec::new();

    for requested_handle in requested_handles {
        let normalized = requested_handle.trim();
        if normalized.is_empty() {
            return Err(ApiErrorResponse::invalid_request(
                "binding_handles must not contain empty values",
            ));
        }

        if !token.can_access_binding_handle(normalized) {
            return Err(ApiErrorResponse::forbidden(
                "Requested binding_handles must be allowed by the current token",
            ));
        }

        let binding = broker_service
            .get_binding_by_handle(tenant_id, normalized)
            .await
            .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

        match binding {
            Some(_) => {
                if !unique_handles.contains(&normalized.to_string()) {
                    unique_handles.push(normalized.to_string());
                }
            }
            None => {
                return Err(ApiErrorResponse::not_found(format!(
                    "Binding handle not found: {normalized}"
                )));
            }
        }
    }

    Ok(unique_handles)
}

fn normalize_ttl(expires_in: Option<u64>) -> u64 {
    expires_in
        .unwrap_or(DEFAULT_TOKEN_TTL_SECONDS)
        .clamp(MIN_TOKEN_TTL_SECONDS, MAX_TOKEN_TTL_SECONDS)
}

#[allow(clippy::result_large_err)]
pub(crate) fn parse_uuid_str(value: &str, field: &str) -> Result<Uuid, ApiErrorResponse> {
    Uuid::parse_str(value)
        .map_err(|_| ApiErrorResponse::not_found(format!("Invalid {field}: resource not found")))
}

#[allow(clippy::result_large_err)]
pub(crate) fn unix_to_datetime(value: u64) -> Result<DateTime<Utc>, ApiErrorResponse> {
    DateTime::<Utc>::from_timestamp(value as i64, 0)
        .ok_or_else(|| ApiErrorResponse::internal_error("Invalid expiration timestamp"))
}

fn format_datetime(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

pub(crate) fn map_token_metadata(item: ApiTokenMetadata) -> TokenMetadataResponse {
    TokenMetadataResponse {
        token_id: item.id,
        token_kind: item.token_kind,
        token_name: item.token_name,
        token_prefix: item.token_prefix,
        token_type: item.token_type.as_str().to_string(),
        subject_type: item.subject_type.as_str().to_string(),
        subject_id: item.subject_id.to_string(),
        tenant_id: item.tenant_id.to_string(),
        issued_from: item.issued_from,
        token_plane: item.token_plane,
        session_id: item.session_id.map(|value| value.to_string()),
        membership_id: item.membership_id.map(|value| value.to_string()),
        display_name: item.display_name,
        description: item.description,
        granted_scopes: item.scopes,
        credential_ids: item.credential_ids,
        binding_handles: item.binding_handles,
        issued_membership_role_snapshot: item.issued_membership_role_snapshot,
        permission_source: item.permission_source,
        created_via: item.created_via,
        revoked_reason: item.revoked_reason,
        expires_at: format_datetime(item.expires_at),
        revoked_at: item.revoked_at.map(format_datetime),
        created_at: format_datetime(item.created_at),
        last_used_at: item.last_used_at.map(format_datetime),
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{
        audit::AuditStorage,
        auth::AuthApiState,
        middleware::{TokenScope, ValidatedToken, validate_paseto_token},
        token_blacklist::create_token_store,
    };
    use crate::auth::{
        error::AuthError,
        models::{
            AuthEventType, AuthSession, CreateUserRequest, ExternalIdentity, IdentityProvider,
            InviteeType, MembershipRole, TenantInvitation, TenantMembership, User,
        },
        service::{AuthService, MfaStatusSnapshot},
    };
    use crate::crypto::constants;
    use crate::models::CredentialType;
    use crate::oauth_broker::{
        AuthTransaction, AuthTransactionStatus, Binding, BindingKind, BindingPolicySnapshot,
        BindingStatus, CreateBindingInput, CreateProviderDefinitionInput, OAuthBrokerError,
        OAuthBrokerService, ProviderDefinition, ProviderValidationResult,
        StartAuthTransactionInput, UpdateProviderDefinitionInput,
    };
    use crate::token::TOKEN_ISSUED_FROM_SESSION;
    use crate::vault::{
        models::{CreateCredentialRequest, EncryptedPayload, ServiceId, TenantId, UserId},
        storage::{CredentialVault, create_credential},
    };
    use async_trait::async_trait;
    use serde_json::Value as JsonValue;
    use std::sync::Arc;
    use uuid::Uuid;

    #[derive(Default)]
    struct DummyAuthService {
        tokens: Vec<ApiTokenMetadata>,
    }

    struct DummyOAuthBrokerService;

    struct FailingAuditStorage;

    #[async_trait]
    impl AuditStorage for FailingAuditStorage {
        async fn record(
            &self,
            _entry: crate::audit::AuditEntry,
        ) -> Result<crate::audit::SignedAuditEntry, String> {
            Err("forced auth audit failure".to_string())
        }

        async fn query(
            &self,
            _filter: crate::audit::AuditFilter,
            _offset: usize,
            _limit: usize,
        ) -> Result<(Vec<crate::audit::SignedAuditEntry>, u64), String> {
            unreachable!("unused in token audit failure tests")
        }

        async fn get_by_id(
            &self,
            _id: &str,
        ) -> Result<Option<crate::audit::SignedAuditEntry>, String> {
            unreachable!("unused in token audit failure tests")
        }

        async fn get_by_index(
            &self,
            _index: u64,
        ) -> Result<Option<crate::audit::SignedAuditEntry>, String> {
            unreachable!("unused in token audit failure tests")
        }

        async fn get_all(
            &self,
            _filter: crate::audit::AuditFilter,
        ) -> Result<Vec<crate::audit::SignedAuditEntry>, String> {
            unreachable!("unused in token audit failure tests")
        }
    }

    #[async_trait]
    impl AuthService for DummyAuthService {
        async fn create_user_from_privy(
            &self,
            _privy_token: &str,
            _hinted_email: Option<&str>,
        ) -> Result<User, AuthError> {
            Err(AuthError::InternalError("unused".to_string()))
        }

        async fn get_or_create_external_identity(
            &self,
            _user_id: Uuid,
            _provider: IdentityProvider,
            _provider_subject: &str,
            _profile: Option<JsonValue>,
        ) -> Result<ExternalIdentity, AuthError> {
            Err(AuthError::InternalError("unused".to_string()))
        }

        async fn create_tenant_invitation(
            &self,
            _tenant_id: Uuid,
            _role: MembershipRole,
            _invitee_type: InviteeType,
            _invitee_email: Option<String>,
            _invitee_wallet: Option<String>,
            _created_by: Uuid,
            _expires_hours: i64,
        ) -> Result<(TenantInvitation, String), AuthError> {
            Err(AuthError::InternalError("unused".to_string()))
        }

        async fn consume_invitation(
            &self,
            _invitation_token: &str,
            _user_id: Uuid,
        ) -> Result<TenantMembership, AuthError> {
            Err(AuthError::InternalError("unused".to_string()))
        }

        async fn create_session(
            &self,
            _user_id: Uuid,
            _identity_id: Option<Uuid>,
            _request: CreateUserRequest,
        ) -> Result<(AuthSession, String), AuthError> {
            Err(AuthError::InternalError("unused".to_string()))
        }

        async fn get_active_membership(
            &self,
            _user_id: Uuid,
            _tenant_id: Uuid,
        ) -> Result<Option<TenantMembership>, AuthError> {
            Err(AuthError::InternalError("unused".to_string()))
        }

        async fn audit_log(
            &self,
            _event_type: AuthEventType,
            _user_id: Option<Uuid>,
            _data: Option<JsonValue>,
        ) -> Result<(), AuthError> {
            Err(AuthError::InternalError("unused".to_string()))
        }

        async fn verify_session(&self, _session_token: &str) -> Result<AuthSession, AuthError> {
            Err(AuthError::InternalError("unused".to_string()))
        }

        async fn revoke_session(&self, _session_id: Uuid, _reason: &str) -> Result<(), AuthError> {
            Err(AuthError::InternalError("unused".to_string()))
        }

        async fn get_user(&self, _user_id: Uuid) -> Result<User, AuthError> {
            Err(AuthError::InternalError("unused".to_string()))
        }

        async fn get_user_identities(
            &self,
            _user_id: Uuid,
        ) -> Result<Vec<ExternalIdentity>, AuthError> {
            Err(AuthError::InternalError("unused".to_string()))
        }

        async fn get_user_memberships(
            &self,
            _user_id: Uuid,
        ) -> Result<Vec<TenantMembership>, AuthError> {
            Err(AuthError::InternalError("unused".to_string()))
        }

        async fn sync_mfa_status(
            &self,
            _user_id: Uuid,
            _privy_token: &str,
        ) -> Result<MfaStatusSnapshot, AuthError> {
            Err(AuthError::InternalError("unused".to_string()))
        }

        async fn get_mfa_status(&self, _user_id: Uuid) -> Result<MfaStatusSnapshot, AuthError> {
            Err(AuthError::InternalError("unused".to_string()))
        }

        async fn list_api_tokens(
            &self,
            _tenant_id: Uuid,
        ) -> Result<Vec<ApiTokenMetadata>, AuthError> {
            Ok(self.tokens.clone())
        }

        async fn get_api_token_metadata(
            &self,
            token_id: &str,
        ) -> Result<Option<ApiTokenMetadata>, AuthError> {
            Ok(self.tokens.iter().find(|item| item.id == token_id).cloned())
        }
    }

    #[async_trait]
    impl OAuthBrokerService for DummyOAuthBrokerService {
        async fn create_provider_definition(
            &self,
            _input: CreateProviderDefinitionInput,
        ) -> Result<ProviderDefinition, OAuthBrokerError> {
            Err(OAuthBrokerError::InternalError("unused".to_string()))
        }

        async fn list_provider_definitions(
            &self,
            _tenant_id: Uuid,
        ) -> Result<Vec<ProviderDefinition>, OAuthBrokerError> {
            Ok(Vec::new())
        }

        async fn get_provider_definition(
            &self,
            _provider_definition_id: Uuid,
        ) -> Result<Option<ProviderDefinition>, OAuthBrokerError> {
            Ok(None)
        }

        async fn update_provider_definition(
            &self,
            _tenant_id: Uuid,
            _provider_definition_id: Uuid,
            _input: UpdateProviderDefinitionInput,
        ) -> Result<ProviderDefinition, OAuthBrokerError> {
            Err(OAuthBrokerError::InternalError("unused".to_string()))
        }

        async fn validate_provider_definition(
            &self,
            _tenant_id: Uuid,
            _provider_definition_id: Uuid,
        ) -> Result<ProviderValidationResult, OAuthBrokerError> {
            Err(OAuthBrokerError::InternalError("unused".to_string()))
        }

        async fn create_binding(
            &self,
            _input: CreateBindingInput,
        ) -> Result<Binding, OAuthBrokerError> {
            Err(OAuthBrokerError::InternalError("unused".to_string()))
        }

        async fn list_bindings(&self, _tenant_id: Uuid) -> Result<Vec<Binding>, OAuthBrokerError> {
            Ok(Vec::new())
        }

        async fn get_binding(
            &self,
            _binding_id: Uuid,
        ) -> Result<Option<Binding>, OAuthBrokerError> {
            Ok(None)
        }

        async fn get_binding_by_handle(
            &self,
            tenant_id: Uuid,
            binding_handle: &str,
        ) -> Result<Option<Binding>, OAuthBrokerError> {
            if tenant_id != Uuid::parse_str("00000000-0000-0000-0000-000000000123").unwrap() {
                return Ok(None);
            }

            if binding_handle == "binding_demo_runtime" {
                return Ok(Some(Binding {
                    id: Uuid::nil(),
                    tenant_id,
                    provider_definition_id: Uuid::nil(),
                    binding_handle: binding_handle.to_string(),
                    alias: "binding-demo".to_string(),
                    purpose: "demo".to_string(),
                    binding_kind: BindingKind::ProviderAppCredential,
                    subject_ref: "tenant_access_token".to_string(),
                    subject_display_name: Some("Demo Binding".to_string()),
                    backing_credential_id: None,
                    status: crate::oauth_broker::BindingStatus::Draft,
                    health_status: Some("draft".to_string()),
                    last_error_code: None,
                    last_error_message: None,
                    cooldown_until: None,
                    rebind_required: false,
                    last_validated_at: None,
                    created_by: Uuid::nil(),
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                }));
            }

            Ok(None)
        }

        async fn get_latest_binding_policy_snapshot(
            &self,
            _binding_id: Uuid,
        ) -> Result<Option<BindingPolicySnapshot>, OAuthBrokerError> {
            Ok(None)
        }

        async fn set_binding_validation_state(
            &self,
            binding_id: Uuid,
            binding_status: BindingStatus,
            health_status: &str,
            _last_error_code: Option<&str>,
            _last_error_message: Option<&str>,
            validated_at: chrono::DateTime<chrono::Utc>,
        ) -> Result<Binding, OAuthBrokerError> {
            Ok(Binding {
                id: binding_id,
                tenant_id: Uuid::parse_str("00000000-0000-0000-0000-000000000123").unwrap(),
                provider_definition_id: Uuid::nil(),
                binding_handle: "binding_demo_runtime".to_string(),
                alias: "binding-demo".to_string(),
                purpose: "demo".to_string(),
                binding_kind: BindingKind::ProviderAppCredential,
                subject_ref: "tenant_access_token".to_string(),
                subject_display_name: Some("Demo Binding".to_string()),
                backing_credential_id: None,
                status: binding_status,
                health_status: Some(health_status.to_string()),
                last_error_code: None,
                last_error_message: None,
                cooldown_until: None,
                rebind_required: false,
                last_validated_at: Some(validated_at),
                created_by: Uuid::nil(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            })
        }

        async fn start_auth_transaction(
            &self,
            _input: StartAuthTransactionInput,
        ) -> Result<AuthTransaction, OAuthBrokerError> {
            Err(OAuthBrokerError::InternalError("unused".to_string()))
        }

        async fn get_auth_transaction(
            &self,
            _transaction_id: Uuid,
        ) -> Result<Option<AuthTransaction>, OAuthBrokerError> {
            Ok(None)
        }

        async fn get_auth_transaction_by_state(
            &self,
            _state: &str,
        ) -> Result<Option<AuthTransaction>, OAuthBrokerError> {
            Ok(None)
        }

        async fn set_auth_transaction_status(
            &self,
            _transaction_id: Uuid,
            _status: AuthTransactionStatus,
            _consumed_at: Option<chrono::DateTime<chrono::Utc>>,
        ) -> Result<AuthTransaction, OAuthBrokerError> {
            Err(OAuthBrokerError::InternalError("unused".to_string()))
        }
    }

    fn seed_test_state() -> (AuthApiState, String) {
        let vault = Arc::new(CredentialVault::new_in_memory());
        let payload = EncryptedPayload::new(
            constants::PROTOCOL_VERSION,
            constants::ALGORITHM_AES_256_GCM,
            constants::KDF_HKDF_SHA256,
            vec![0u8; constants::NONCE_LENGTH],
            vec![0u8; constants::AUTH_TAG_LENGTH],
            vec![1, 2, 3, 4],
        );
        let entry = create_credential(
            vault.as_ref(),
            "00000000-0000-0000-0000-000000000123",
            "00000000-0000-0000-0000-000000000456",
            "sandbox-demo",
            CredentialType::ApiKey,
            payload,
            None,
        )
        .expect("test credential should be created");
        let state = AuthApiState::new_with_token_store(
            Arc::new(DummyAuthService::default()),
            create_token_store(),
        )
        .with_vault(vault)
        .with_oauth_broker_service(Arc::new(DummyOAuthBrokerService));

        (state, entry.credential_id.as_str().to_string())
    }

    fn seed_test_state_with_failing_audit() -> (AuthApiState, String) {
        let (state, credential_id) = seed_test_state();
        (
            state.with_audit_storage(Arc::new(FailingAuditStorage)),
            credential_id,
        )
    }

    fn seed_test_state_with_runtime_approval_credential() -> (AuthApiState, String) {
        let vault = Arc::new(CredentialVault::new_in_memory());
        let payload = EncryptedPayload::new(
            constants::PROTOCOL_VERSION,
            constants::ALGORITHM_AES_256_GCM,
            constants::KDF_HKDF_SHA256,
            vec![0u8; constants::NONCE_LENGTH],
            vec![0u8; constants::AUTH_TAG_LENGTH],
            vec![1, 2, 3, 4],
        );
        let entry = vault
            .create_credential(
                CreateCredentialRequest {
                    tenant_id: TenantId::new("00000000-0000-0000-0000-000000000123"),
                    user_id: UserId::new("00000000-0000-0000-0000-000000000456"),
                    service_id: ServiceId::new("sandbox-runtime-approval"),
                    credential_type: CredentialType::ApiKey,
                    expires_at: None,
                    requires_approval: true,
                    provider: None,
                    allowed_domains: Vec::new(),
                    custom_functions: Vec::new(),
                },
                payload,
            )
            .expect("runtime approval credential should be created");
        let state = AuthApiState::new_with_token_store(
            Arc::new(DummyAuthService::default()),
            create_token_store(),
        )
        .with_vault(vault)
        .with_oauth_broker_service(Arc::new(DummyOAuthBrokerService));

        (state, entry.credential_id.as_str().to_string())
    }

    fn create_token_request(
        credential_id: &str,
        scopes: &[&str],
        expires_in: Option<u64>,
    ) -> CreateTokenRequest {
        CreateTokenRequest {
            scopes: scopes.iter().map(|scope| (*scope).to_string()).collect(),
            expires_in,
            credential_ids: vec![credential_id.to_string()],
            token_name: None,
            binding_handles: Vec::new(),
        }
    }

    fn session_token(scopes: Vec<TokenScope>) -> ValidatedToken {
        let mut token = ValidatedToken::mock(
            "00000000-0000-0000-0000-000000000123",
            "00000000-0000-0000-0000-000000000456",
            scopes,
        );
        token.metadata.insert(
            "session_id".to_string(),
            "00000000-0000-0000-0000-000000000789".to_string(),
        );
        token.metadata.insert(
            "membership_id".to_string(),
            "00000000-0000-0000-0000-000000000321".to_string(),
        );
        token.membership_id = Some("00000000-0000-0000-0000-000000000321".to_string());
        token
    }

    fn seed_token_list_state(tokens: Vec<ApiTokenMetadata>) -> AuthApiState {
        AuthApiState::new_with_token_store(
            Arc::new(DummyAuthService { tokens }),
            create_token_store(),
        )
        .with_oauth_broker_service(Arc::new(DummyOAuthBrokerService))
    }

    fn make_token_metadata(index: usize, subject_id: Uuid) -> ApiTokenMetadata {
        let tenant_id = Uuid::parse_str("00000000-0000-0000-0000-000000000123")
            .expect("tenant uuid should parse");
        let session_id = Uuid::parse_str("00000000-0000-0000-0000-000000000789")
            .expect("session uuid should parse");
        let membership_id = Uuid::parse_str("00000000-0000-0000-0000-000000000321")
            .expect("membership uuid should parse");

        ApiTokenMetadata::new(
            format!("00000000-0000-0000-0000-{:012}", index + 1),
            ApiTokenType::UserAccessToken,
            ApiTokenSubjectType::User,
            subject_id,
            tenant_id,
            TOKEN_ISSUED_FROM_SESSION,
            chrono::Utc::now(),
        )
        .with_token_kind("user_access_token")
        .with_token_plane("management")
        .with_session_id(session_id)
        .with_membership_id(membership_id)
        .with_token_name(format!("token-{index}"))
        .with_scopes(vec!["tokens:read".to_string()])
    }

    #[tokio::test]
    async fn create_token_returns_paseto_for_subset_scopes() {
        let (state, credential_id) = seed_test_state();
        let response = create_token_handler(
            State(state),
            Extension(session_token(vec![
                TokenScope::TokensWrite,
                TokenScope::CredentialRead,
                TokenScope::AuditRead,
            ])),
            Json(create_token_request(
                &credential_id,
                &["credential:read"],
                Some(3600),
            )),
        )
        .await
        .expect("token creation should succeed")
        .0;

        assert!(response.access_token.starts_with("v4.local."));
        assert_eq!(response.token, response.access_token);
        assert_eq!(
            response.scope,
            "credential:read credential:decrypt sandbox:write sandbox:read sandbox:execute"
        );
        assert_eq!(
            response.granted_scopes,
            vec![
                "credential:read".to_string(),
                "credential:decrypt".to_string(),
                "sandbox:write".to_string(),
                "sandbox:read".to_string(),
                "sandbox:execute".to_string()
            ]
        );
        assert_eq!(response.expires_in, 3600);
        assert_eq!(response.subject_type, TOKEN_SUBJECT_TYPE_USER);
        assert_eq!(response.issued_from, TOKEN_ISSUED_FROM_ACCESS_TOKEN);
        assert_eq!(response.token_plane, "runtime");
        assert_eq!(response.credential_ids, vec![credential_id]);
        assert!(response.binding_handles.is_empty());
    }

    #[tokio::test]
    async fn create_token_response_includes_token_name() {
        let (state, credential_id) = seed_test_state();
        let mut request = create_token_request(&credential_id, &["credential:read"], Some(3600));
        request.token_name = Some("daily sync token".to_string());

        let response = create_token_handler(
            State(state),
            Extension(session_token(vec![
                TokenScope::TokensWrite,
                TokenScope::CredentialRead,
            ])),
            Json(request),
        )
        .await
        .expect("token creation should succeed")
        .0;

        assert_eq!(response.token_name.as_deref(), Some("daily sync token"));
        assert_eq!(response.display_name.as_deref(), Some("daily sync token"));
    }

    #[tokio::test]
    async fn create_token_succeeds_when_audit_write_fails_after_persist() {
        let (state, credential_id) = seed_test_state_with_failing_audit();
        let response = create_token_handler(
            State(state),
            Extension(session_token(vec![
                TokenScope::TokensWrite,
                TokenScope::CredentialRead,
            ])),
            Json(create_token_request(
                &credential_id,
                &["credential:read"],
                Some(3600),
            )),
        )
        .await
        .expect("token creation should still succeed")
        .0;

        assert!(!response.token_id.is_empty());
        assert_eq!(response.subject_type, TOKEN_SUBJECT_TYPE_USER);
    }

    #[tokio::test]
    async fn create_token_response_includes_token_alias_matching_access_token() {
        let (state, credential_id) = seed_test_state();
        let response = create_token_handler(
            State(state),
            Extension(session_token(vec![
                TokenScope::TokensWrite,
                TokenScope::CredentialRead,
            ])),
            Json(create_token_request(
                &credential_id,
                &["credential:read"],
                Some(3600),
            )),
        )
        .await
        .expect("token creation should succeed")
        .0;

        assert!(!response.token.is_empty());
        assert_eq!(response.token, response.access_token);
    }

    #[tokio::test]
    async fn create_token_respects_minimum_ttl_option() {
        let (state, credential_id) = seed_test_state();
        let response = create_token_handler(
            State(state),
            Extension(session_token(vec![
                TokenScope::TokensWrite,
                TokenScope::CredentialRead,
            ])),
            Json(create_token_request(
                &credential_id,
                &["credential:read"],
                Some(900),
            )),
        )
        .await
        .expect("token creation should succeed")
        .0;

        assert_eq!(response.expires_in, MIN_TOKEN_TTL_SECONDS);
    }

    #[tokio::test]
    async fn create_token_uses_default_ttl_when_not_provided() {
        let (state, credential_id) = seed_test_state();
        let response = create_token_handler(
            State(state),
            Extension(session_token(vec![
                TokenScope::TokensWrite,
                TokenScope::CredentialRead,
            ])),
            Json(create_token_request(
                &credential_id,
                &["credential:read"],
                None,
            )),
        )
        .await
        .expect("token creation should succeed")
        .0;

        assert_eq!(response.expires_in, DEFAULT_TOKEN_TTL_SECONDS);
    }

    #[tokio::test]
    async fn created_token_keeps_restricted_credential_ids_in_paseto_validation() {
        let (state, credential_id) = seed_test_state();
        let created = create_token_handler(
            State(state.clone()),
            Extension(session_token(vec![
                TokenScope::TokensWrite,
                TokenScope::TokensRead,
                TokenScope::CredentialRead,
            ])),
            Json(create_token_request(
                &credential_id,
                &["credential:read"],
                Some(120),
            )),
        )
        .await
        .expect("token creation should succeed")
        .0;

        assert_eq!(created.expires_in, MIN_TOKEN_TTL_SECONDS);
        assert_eq!(created.token_plane, "runtime");

        let validated = validate_paseto_token(
            &created.access_token,
            state.token_secret_key.as_slice(),
            "en",
        )
        .expect("created token should validate");

        assert_eq!(validated.token_id, created.token_id);
        assert_eq!(validated.token_plane(), "runtime");
        assert_eq!(validated.allowed_binding_handles(), None);
        assert!(validated.has_scope(&TokenScope::CredentialRead));
        assert!(validated.has_scope(&TokenScope::CredentialDecrypt));
        assert!(validated.has_scope(&TokenScope::SandboxWrite));
        assert!(validated.has_scope(&TokenScope::SandboxRead));
        assert!(validated.has_scope(&TokenScope::SandboxExecute));
    }

    #[tokio::test]
    async fn create_token_caps_ttl_at_maximum() {
        let (state, credential_id) = seed_test_state();
        let request_ttl = MAX_TOKEN_TTL_SECONDS + 1;
        let response = create_token_handler(
            State(state),
            Extension(session_token(vec![
                TokenScope::TokensWrite,
                TokenScope::CredentialRead,
            ])),
            Json(create_token_request(
                &credential_id,
                &["credential:read"],
                Some(request_ttl),
            )),
        )
        .await
        .expect("token creation should succeed")
        .0;

        assert_eq!(response.expires_in, MAX_TOKEN_TTL_SECONDS);
    }

    #[tokio::test]
    async fn create_token_supports_seven_day_expiry() {
        let (state, credential_id) = seed_test_state();
        let request_ttl = 7 * 24 * 60 * 60;
        let response = create_token_handler(
            State(state),
            Extension(session_token(vec![
                TokenScope::TokensWrite,
                TokenScope::CredentialRead,
            ])),
            Json(create_token_request(
                &credential_id,
                &["credential:read"],
                Some(request_ttl),
            )),
        )
        .await
        .expect("token creation should succeed")
        .0;

        assert_eq!(response.expires_in, request_ttl);
    }

    #[test]
    fn create_token_request_deserializes_without_scopes_as_empty() {
        let request: CreateTokenRequest = serde_json::from_value(json!({"expires_in": 300}))
            .expect("missing scopes should deserialize as empty");
        assert!(request.scopes.is_empty());
        assert_eq!(request.expires_in, Some(300));
        assert!(request.credential_ids.is_empty());
        assert!(request.binding_handles.is_empty());
        assert!(request.token_name.is_none());
    }

    #[tokio::test]
    async fn runtime_token_claims_are_explicitly_distinct_from_management_token() {
        let (state, credential_id) = seed_test_state();
        let management = session_token(vec![
            TokenScope::TokensWrite,
            TokenScope::CredentialRead,
            TokenScope::TenantAdmin,
        ]);

        let created = create_token_handler(
            State(state.clone()),
            Extension(management.clone()),
            Json(CreateTokenRequest {
                scopes: vec!["credential:read".to_string()],
                expires_in: Some(1800),
                credential_ids: vec![credential_id],
                token_name: None,
                binding_handles: vec!["binding_demo_runtime".to_string()],
            }),
        )
        .await
        .expect("token creation should succeed")
        .0;

        let runtime = validate_paseto_token(
            &created.access_token,
            state.token_secret_key.as_slice(),
            "en",
        )
        .expect("runtime token should validate");

        assert_eq!(management.issued_from(), TOKEN_ISSUED_FROM_SESSION);
        assert_eq!(management.subject_type(), TOKEN_SUBJECT_TYPE_USER);
        assert_eq!(management.token_plane(), "management");
        assert_eq!(management.allowed_binding_handles(), None);

        assert_eq!(runtime.issued_from(), TOKEN_ISSUED_FROM_ACCESS_TOKEN);
        assert_eq!(runtime.subject_type(), TOKEN_SUBJECT_TYPE_USER);
        assert_eq!(runtime.token_plane(), "runtime");
        assert_eq!(
            runtime.allowed_binding_handles(),
            Some(&["binding_demo_runtime".to_string()][..])
        );
        assert_eq!(
            created.binding_handles,
            vec!["binding_demo_runtime".to_string()]
        );
    }

    #[tokio::test]
    async fn create_token_accepts_binding_handles_without_credential_ids() {
        let (state, _credential_id) = seed_test_state();

        let created = create_token_handler(
            State(state.clone()),
            Extension(session_token(vec![
                TokenScope::TokensWrite,
                TokenScope::CredentialRead,
                TokenScope::TenantAdmin,
            ])),
            Json(CreateTokenRequest {
                scopes: vec!["credential:read".to_string()],
                expires_in: Some(1800),
                credential_ids: Vec::new(),
                token_name: None,
                binding_handles: vec!["binding_demo_runtime".to_string()],
            }),
        )
        .await
        .expect("token creation should succeed")
        .0;

        assert!(created.credential_ids.is_empty());
        assert_eq!(
            created.binding_handles,
            vec!["binding_demo_runtime".to_string()]
        );

        let runtime = validate_paseto_token(
            &created.access_token,
            state.token_secret_key.as_slice(),
            "en",
        )
        .expect("runtime token should validate");
        assert_eq!(
            runtime.allowed_binding_handles(),
            Some(&["binding_demo_runtime".to_string()][..])
        );
    }

    #[tokio::test]
    async fn create_token_accepts_runtime_approval_credentials_without_special_casing_issue_stage()
    {
        let (state, credential_id) = seed_test_state_with_runtime_approval_credential();

        let created = create_token_handler(
            State(state.clone()),
            Extension(session_token(vec![
                TokenScope::TokensWrite,
                TokenScope::CredentialRead,
            ])),
            Json(create_token_request(
                &credential_id,
                &["credential:read"],
                Some(1800),
            )),
        )
        .await
        .expect("runtime approval credentials should still issue tokens")
        .0;

        assert_eq!(created.credential_ids, vec![credential_id.clone()]);
        assert!(created.binding_handles.is_empty());
        assert_eq!(created.token_plane, "runtime");
        assert!(created.access_token.starts_with("v4.local."));

        let runtime = validate_paseto_token(
            &created.access_token,
            state.token_secret_key.as_slice(),
            "en",
        )
        .expect("runtime token should validate");
        assert_eq!(runtime.token_plane(), "runtime");
        assert!(runtime.has_scope(&TokenScope::CredentialRead));
    }

    #[tokio::test]
    async fn create_token_rejects_unknown_binding_handle() {
        let (state, credential_id) = seed_test_state();

        let err = create_token_handler(
            State(state),
            Extension(session_token(vec![
                TokenScope::TokensWrite,
                TokenScope::CredentialRead,
                TokenScope::TenantAdmin,
            ])),
            Json(CreateTokenRequest {
                scopes: vec!["credential:read".to_string()],
                expires_in: Some(1800),
                credential_ids: vec![credential_id],
                token_name: None,
                binding_handles: vec!["binding_not_found".to_string()],
            }),
        )
        .await
        .expect_err("unknown binding handle must be rejected");

        assert_eq!(err.error, "not_found");
        assert!(err.message.contains("Binding handle not found"));
    }

    #[tokio::test]
    async fn create_token_missing_scopes_returns_400_invalid_request() {
        let (state, _) = seed_test_state();
        let request: CreateTokenRequest =
            serde_json::from_value(json!({})).expect("missing scopes should deserialize");

        let err = create_token_handler(
            State(state),
            Extension(session_token(vec![
                TokenScope::TokensWrite,
                TokenScope::CredentialRead,
            ])),
            Json(request),
        )
        .await
        .expect_err("missing scopes must be rejected as invalid_request");

        assert_eq!(err.error, "invalid_request");
        assert!(err.message.contains("At least one scope is required"));
    }

    #[tokio::test]
    async fn create_token_empty_scopes_returns_400_invalid_request() {
        let (state, credential_id) = seed_test_state();
        let err = create_token_handler(
            State(state),
            Extension(session_token(vec![
                TokenScope::TokensWrite,
                TokenScope::CredentialRead,
            ])),
            Json(CreateTokenRequest {
                scopes: vec![],
                expires_in: None,
                credential_ids: vec![credential_id],
                token_name: None,
                binding_handles: Vec::new(),
            }),
        )
        .await
        .expect_err("empty scopes must be rejected as invalid_request");

        assert_eq!(err.error, "invalid_request");
        assert!(err.message.contains("At least one scope is required"));
    }

    #[tokio::test]
    async fn revoke_token_blacklists_current_access_token() {
        let (state, credential_id) = seed_test_state();
        let created = create_token_handler(
            State(state.clone()),
            Extension(session_token(vec![
                TokenScope::TokensWrite,
                TokenScope::TokensRevoke,
                TokenScope::CredentialRead,
            ])),
            Json(create_token_request(
                &credential_id,
                &["credential:read"],
                Some(120),
            )),
        )
        .await
        .expect("token creation should succeed")
        .0;

        let validated = validate_paseto_token(
            &created.access_token,
            state.token_secret_key.as_slice(),
            "en",
        )
        .expect("created token should validate");

        let response = revoke_token_handler(
            State(state.clone()),
            Extension(validated.clone()),
            Path(validated.token_id.clone()),
        )
        .await
        .expect("revoke should succeed")
        .0;

        assert!(response.revoked);
        assert!(
            state
                .token_store
                .is_blacklisted(&validated.token_id)
                .await
                .expect("blacklist lookup should succeed")
        );
    }

    #[tokio::test]
    async fn revoke_other_token_without_permission_returns_403() {
        let (state, _) = seed_test_state();
        let token = session_token(vec![TokenScope::CredentialRead]);
        let target_token_id = "00000000-0000-0000-0000-000000000999".to_string();

        let result = revoke_token_handler(
            State(state.clone()),
            Extension(token),
            Path(target_token_id.clone()),
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error, "forbidden");
        assert_eq!(
            err.message,
            "Can only revoke the current token unless you have admin revoke permissions"
        );
        assert!(
            !state
                .token_store
                .is_blacklisted(&target_token_id)
                .await
                .expect("blacklist lookup should succeed")
        );
    }

    // BUG-18204: 非法 UUID 应返回 404 而非 500
    #[test]
    fn parse_uuid_str_invalid_returns_not_found() {
        let result = parse_uuid_str("not-a-uuid", "service_account_id");
        assert!(result.is_err());

        let err = result.unwrap_err();
        // 验证错误代码是 "not_found" (404) 而非 "internal_error" (500)
        assert_eq!(err.error, "not_found");
    }

    #[test]
    fn parse_uuid_str_empty_returns_not_found() {
        let result = parse_uuid_str("", "token_id");
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.error, "not_found");
    }

    #[test]
    fn parse_uuid_str_partial_uuid_returns_not_found() {
        let result = parse_uuid_str("123e4567-e89b-12d3", "service_account_id");
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.error, "not_found");
    }

    #[test]
    fn parse_uuid_str_valid_uuid_succeeds() {
        let valid_uuid = Uuid::new_v4().to_string();
        let result = parse_uuid_str(&valid_uuid, "service_account_id");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), valid_uuid);
    }

    #[test]
    fn parse_uuid_str_uuid_v7_succeeds() {
        let uuid_v7 = Uuid::now_v7().to_string();
        let result = parse_uuid_str(&uuid_v7, "token_id");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), uuid_v7);
    }

    // BUG-18200: Session token 撤销不存在的 token 应返回 404 而非 400
    #[tokio::test]
    async fn revoke_nonexistent_token_with_revoke_scope_returns_404() {
        let (state, _) = seed_test_state();
        let nonexistent_token_id = "00000000-0000-0000-0000-000000000001";

        // 创建一个有 revoke 权限的 session token
        let token = session_token(vec![TokenScope::TokensRevoke, TokenScope::TenantAdmin]);

        let result = revoke_token_handler(
            State(state),
            Extension(token),
            Path(nonexistent_token_id.to_string()),
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        // 验证返回 404 而非 400 (invalid_request)
        assert_eq!(err.error, "not_found");
        assert_eq!(err.message, "Token metadata not found");
    }

    // BUG-18200: 确保 session token 撤销自己仍返回 400
    #[tokio::test]
    async fn revoke_self_session_token_returns_400() {
        let (state, _) = seed_test_state();
        let token = session_token(vec![TokenScope::TokensRevoke]);

        // session_token 的 session_id 是 "00000000-0000-0000-0000-000000000789"
        // token_id 是 "00000000-0000-0000-0000-000000000123"
        // 设置 token_id 与 session_id 相同来模拟撤销自己的 session token
        let mut self_token = token.clone();
        self_token.token_id = "00000000-0000-0000-0000-000000000789".to_string();

        let result = revoke_token_handler(
            State(state),
            Extension(self_token),
            Path("00000000-0000-0000-0000-000000000789".to_string()),
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        // session token 撤销自己应返回 400
        assert_eq!(err.error, "invalid_request");
    }

    #[tokio::test]
    async fn get_token_invalid_uuid_returns_400_invalid_request() {
        let (state, _) = seed_test_state();
        let result = get_token_handler(
            State(state),
            Extension(session_token(vec![TokenScope::TokensRead])),
            Path("not-a-valid-uuid".to_string()),
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error, "invalid_request");
        assert_eq!(err.message, "Invalid token_id format");
    }

    #[tokio::test]
    async fn get_token_valid_uuid_not_found_returns_404() {
        let (state, _) = seed_test_state();
        let result = get_token_handler(
            State(state),
            Extension(session_token(vec![
                TokenScope::TokensRead,
                TokenScope::TenantAdmin,
            ])),
            Path(Uuid::new_v4().to_string()),
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error, "not_found");
        assert_eq!(err.message, "Token metadata not found");
    }

    #[tokio::test]
    async fn list_tokens_returns_paginated_response() {
        let subject_id = Uuid::parse_str("00000000-0000-0000-0000-000000000456")
            .expect("subject uuid should parse");
        let tokens = (0..25)
            .map(|index| make_token_metadata(index, subject_id))
            .collect::<Vec<_>>();
        let response = list_tokens_handler(
            State(seed_token_list_state(tokens)),
            Extension(session_token(vec![TokenScope::TokensRead])),
            Query(ListTokensQuery {
                page: 2,
                page_size: 10,
            }),
        )
        .await
        .expect("list tokens should succeed")
        .0;

        assert_eq!(response.page, 2);
        assert_eq!(response.page_size, 10);
        assert_eq!(response.total, 25);
        assert_eq!(response.total_pages, 3);
        assert_eq!(response.items.len(), 10);
        assert_eq!(
            response
                .items
                .first()
                .and_then(|item| item.token_name.as_deref()),
            Some("token-10")
        );
        assert_eq!(
            response
                .items
                .last()
                .and_then(|item| item.token_name.as_deref()),
            Some("token-19")
        );
    }

    #[tokio::test]
    async fn list_tokens_page_zero_returns_400_invalid_request() {
        let result = list_tokens_handler(
            State(seed_token_list_state(Vec::new())),
            Extension(session_token(vec![TokenScope::TokensRead])),
            Query(ListTokensQuery {
                page: 0,
                page_size: 20,
            }),
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error, "invalid_request");
        assert_eq!(err.message, "page must be >= 1");
    }

    #[test]
    fn token_list_response_serializes_as_paginated_object() {
        let item = TokenMetadataResponse {
            token_id: Uuid::new_v4().to_string(),
            token_kind: "user_access_token".to_string(),
            token_name: Some("token-1".to_string()),
            token_prefix: Some("cb_".to_string()),
            token_type: "user_access_token".to_string(),
            subject_type: "user".to_string(),
            subject_id: Uuid::new_v4().to_string(),
            tenant_id: Uuid::new_v4().to_string(),
            issued_from: TOKEN_ISSUED_FROM_ACCESS_TOKEN.to_string(),
            token_plane: "runtime".to_string(),
            session_id: Some(Uuid::new_v4().to_string()),
            membership_id: Some(Uuid::new_v4().to_string()),
            display_name: Some("CredBridge CLI".to_string()),
            description: Some("regression fixture".to_string()),
            granted_scopes: vec!["tokens:read".to_string()],
            credential_ids: vec![Uuid::new_v4().to_string()],
            binding_handles: vec!["binding_demo_runtime".to_string()],
            issued_membership_role_snapshot: Some("tenant_admin".to_string()),
            permission_source: Some("membership".to_string()),
            created_via: Some("api".to_string()),
            revoked_reason: None,
            expires_at: "2026-01-01T00:00:00Z".to_string(),
            revoked_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            last_used_at: None,
        };

        let payload = serde_json::to_value(ListTokensResponse::new(vec![item], 1, 20, 1))
            .expect("token list payload should serialize");
        let items = payload["items"]
            .as_array()
            .expect("token list payload must include items");
        assert_eq!(items.len(), 1);
        assert_eq!(payload["page"].as_u64(), Some(1));
        assert_eq!(payload["page_size"].as_u64(), Some(20));
        assert_eq!(payload["total"].as_u64(), Some(1));
        assert_eq!(payload["total_pages"].as_u64(), Some(1));
    }

    #[test]
    fn empty_token_list_serializes_to_empty_page() {
        let payload = serde_json::to_value(ListTokensResponse::new(
            Vec::<TokenMetadataResponse>::new(),
            1,
            20,
            0,
        ))
        .expect("empty token list should serialize");
        let list = payload["items"]
            .as_array()
            .expect("empty token list payload must contain items");

        assert!(list.is_empty());
        assert_eq!(payload["total"].as_u64(), Some(0));
        assert_eq!(payload["total_pages"].as_u64(), Some(0));
    }
}
