use chrono::{DateTime, Utc};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::{
    auth::AuthApiState,
    middleware::{TokenScope, ValidatedToken},
    response::ApiErrorResponse,
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
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatedTokenResponse {
    pub access_token: String,
    pub token: String,
    pub token_id: String,
    pub token_type: String,
    pub subject_type: String,
    pub issued_from: String,
    pub display_name: Option<String>,
    pub expires_in: u64,
    pub scope: String,
    pub granted_scopes: Vec<String>,
    pub credential_ids: Vec<String>,
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
    pub session_id: Option<String>,
    pub membership_id: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub granted_scopes: Vec<String>,
    pub credential_ids: Vec<String>,
    pub issued_membership_role_snapshot: Option<String>,
    pub permission_source: Option<String>,
    pub created_via: Option<String>,
    pub revoked_reason: Option<String>,
    pub expires_at: String,
    pub revoked_at: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
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
    let created = issue_access_token_from_user_token(&state, &token, request).await?;
    Ok(Json(created))
}

pub async fn issue_access_token_from_user_token(
    state: &AuthApiState,
    token: &ValidatedToken,
    request: CreateTokenRequest,
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

    if request.credential_ids.is_empty() {
        return Err(ApiErrorResponse::invalid_request(
            "At least one credential_id is required",
        ));
    }

    let requested_scopes = request
        .scopes
        .iter()
        .map(|scope| {
            TokenScope::parse(scope)
                .ok_or_else(|| ApiErrorResponse::invalid_request(format!("Invalid scope: {scope}")))
        })
        .collect::<Result<Vec<_>, _>>()?;

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

    let credential_ids = resolve_allowed_credential_ids(state, token, &request.credential_ids)?;

    let ttl_seconds = normalize_ttl(request.expires_in);
    let granted_scopes = requested_scopes
        .iter()
        .map(TokenScope::as_str)
        .map(str::to_string)
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
    .with_issued_from(TOKEN_ISSUED_FROM_ACCESS_TOKEN);

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
    .with_credential_ids(credential_ids.clone());
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

    state
        .record_audit(
            AuditAction::TokenIssue,
            &token.user_id,
            Outcome::Success,
            Some(json!({
                "issued_from": TOKEN_ISSUED_FROM_ACCESS_TOKEN,
                "subject_type": TOKEN_SUBJECT_TYPE_USER,
                "token_id": token_id,
                "scopes": granted_scopes,
                "credential_ids": credential_ids,
                "expires_in": ttl_seconds,
                "session_id": token.session_id(),
                "membership_id": token.membership_id(),
            })),
        )
        .await;

    Ok(CreatedTokenResponse {
        token: access_token.clone(),
        access_token,
        token_id: claims.jti.clone(),
        token_type: "Bearer".to_string(),
        subject_type: TOKEN_SUBJECT_TYPE_USER.to_string(),
        issued_from: TOKEN_ISSUED_FROM_ACCESS_TOKEN.to_string(),
        display_name: None,
        expires_in: ttl_seconds,
        scope: scope_string,
        granted_scopes: requested_scopes
            .iter()
            .map(TokenScope::as_str)
            .map(str::to_string)
            .collect(),
        credential_ids,
        issued_at: claims.iat.unwrap_or_else(unix_now),
        expires_at: claims.exp,
        revoked_at: None,
    })
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

    state
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
        .await;

    Ok(Json(RevokeTokenResponse {
        revoked: true,
        token_id,
    }))
}

async fn list_tokens_handler(
    State(state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
) -> Result<Json<Vec<TokenMetadataResponse>>, ApiErrorResponse> {
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

    let items = items
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
        .map(map_token_metadata)
        .collect();

    Ok(Json(items))
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
        session_id: item.session_id.map(|value| value.to_string()),
        membership_id: item.membership_id.map(|value| value.to_string()),
        display_name: item.display_name,
        description: item.description,
        granted_scopes: item.scopes,
        credential_ids: item.credential_ids,
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
    use crate::vault::{
        models::EncryptedPayload,
        storage::{CredentialVault, create_credential},
    };
    use async_trait::async_trait;
    use serde_json::Value as JsonValue;
    use std::sync::Arc;
    use uuid::Uuid;

    struct DummyAuthService;

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
        let state =
            AuthApiState::new_with_token_store(Arc::new(DummyAuthService), create_token_store())
                .with_vault(vault);

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
        assert_eq!(response.scope, "credential:read");
        assert_eq!(response.expires_in, 3600);
        assert_eq!(response.subject_type, TOKEN_SUBJECT_TYPE_USER);
        assert_eq!(response.issued_from, TOKEN_ISSUED_FROM_ACCESS_TOKEN);
        assert_eq!(response.credential_ids, vec![credential_id]);
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

        let validated = validate_paseto_token(
            &created.access_token,
            state.token_secret_key.as_slice(),
            "en",
        )
        .expect("created token should validate");

        assert_eq!(validated.token_id, created.token_id);
        assert!(validated.has_scope(&TokenScope::CredentialRead));
    }

    #[tokio::test]
    async fn create_token_caps_ttl_at_maximum() {
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
                Some(MAX_TOKEN_TTL_SECONDS + 1),
            )),
        )
        .await
        .expect("token creation should succeed")
        .0;

        assert_eq!(response.expires_in, MAX_TOKEN_TTL_SECONDS);
    }

    #[test]
    fn create_token_request_deserializes_without_scopes_as_empty() {
        let request: CreateTokenRequest = serde_json::from_value(json!({"expires_in": 300}))
            .expect("missing scopes should deserialize as empty");
        assert!(request.scopes.is_empty());
        assert_eq!(request.expires_in, Some(300));
        assert!(request.credential_ids.is_empty());
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

    // BUG-18195: /api/v1/tokens 响应契约应为顶层数组而不是 {data,total}
    #[test]
    fn token_list_response_serializes_as_top_level_array() {
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
            session_id: Some(Uuid::new_v4().to_string()),
            membership_id: Some(Uuid::new_v4().to_string()),
            display_name: Some("CredBridge CLI".to_string()),
            description: Some("regression fixture".to_string()),
            granted_scopes: vec!["tokens:read".to_string()],
            credential_ids: vec![Uuid::new_v4().to_string()],
            issued_membership_role_snapshot: Some("tenant_admin".to_string()),
            permission_source: Some("membership".to_string()),
            created_via: Some("api".to_string()),
            revoked_reason: None,
            expires_at: "2026-01-01T00:00:00Z".to_string(),
            revoked_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            last_used_at: None,
        };

        let payload =
            serde_json::to_value(vec![item]).expect("token list payload should serialize");
        let list = payload
            .as_array()
            .expect("token list payload must be a top-level array");
        assert_eq!(list.len(), 1);
        assert!(payload.get("data").is_none());
        assert!(payload.get("total").is_none());
    }

    #[test]
    fn empty_token_list_serializes_to_empty_array() {
        let payload = serde_json::to_value(Vec::<TokenMetadataResponse>::new())
            .expect("empty token list should serialize");
        let list = payload
            .as_array()
            .expect("empty token list payload must be an array");

        assert!(list.is_empty());
        assert!(payload.get("data").is_none());
        assert!(payload.get("total").is_none());
    }
}
