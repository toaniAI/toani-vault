use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    Extension, Json, Router,
    extract::State,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use super::{
    auth::AuthApiState,
    middleware::{TokenScope, ValidatedToken, validate_paseto_token},
    response::ApiErrorResponse,
};
use crate::token::{PasetoToken, TokenClaims};

const TOKEN_SECRET_KEY: [u8; 32] = [0u8; 32];

#[derive(Debug, Deserialize)]
pub struct CreateTokenRequest {
    pub scopes: Vec<String>,
    pub expires_in: u64,
}

#[derive(Debug, Serialize)]
pub struct CreatedTokenResponse {
    pub access_token: String,
    pub token_id: String,
    pub token_type: String,
    pub expires_in: u64,
    pub scope: String,
    pub issued_at: u64,
    pub expires_at: u64,
}

#[derive(Debug, Deserialize)]
pub struct VerifyTokenRequest {
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct VerifyTokenResponse {
    pub valid: bool,
    pub token_id: Option<String>,
    pub user_id: Option<String>,
    pub tenant_id: Option<String>,
    pub scopes: Option<Vec<String>>,
    pub expires_at: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct TokenStatsResponse {
    pub active_tokens: u64,
}

pub fn token_routes(state: AuthApiState) -> Router {
    Router::new()
        .route("/tokens", post(create_token_handler))
        .route("/tokens/verify", post(verify_token_handler))
        .route("/tokens/stats", get(get_token_stats_handler))
        .with_state(state)
}

async fn create_token_handler(
    State(_state): State<AuthApiState>,
    Extension(token): Extension<ValidatedToken>,
    Json(request): Json<CreateTokenRequest>,
) -> Result<Json<CreatedTokenResponse>, ApiErrorResponse> {
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

    let requested_scopes = request
        .scopes
        .iter()
        .map(|scope| {
            TokenScope::parse(scope)
                .ok_or_else(|| ApiErrorResponse::invalid_request(format!("Invalid scope: {scope}")))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if requested_scopes.iter().any(|scope| !token.has_scope(scope)) {
        return Err(ApiErrorResponse::forbidden(
            "Requested scopes must be a subset of the current session scopes",
        ));
    }

    let ttl_seconds = request.expires_in.max(1);
    let scope_string = requested_scopes
        .iter()
        .map(TokenScope::as_str)
        .collect::<Vec<_>>()
        .join(" ");
    let claims = TokenClaims::new(
        format!("{}:{}", token.tenant_id, token.user_id),
        token.tenant_id.clone(),
        scope_string.clone(),
        false,
        ttl_seconds,
    );

    let paseto_key = PasetoToken::key_from_bytes(&TOKEN_SECRET_KEY)
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;
    let access_token = PasetoToken::sign(&claims, &paseto_key)
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

    Ok(Json(CreatedTokenResponse {
        access_token,
        token_id: claims.jti.clone(),
        token_type: "Bearer".to_string(),
        expires_in: ttl_seconds,
        scope: scope_string,
        issued_at: claims.iat.unwrap_or_else(unix_now),
        expires_at: claims.exp,
    }))
}

async fn verify_token_handler(
    State(state): State<AuthApiState>,
    Extension(session_token): Extension<ValidatedToken>,
    Json(request): Json<VerifyTokenRequest>,
) -> Result<Json<VerifyTokenResponse>, ApiErrorResponse> {
    if !session_token.has_any_scope(&[
        TokenScope::TokensRead,
        TokenScope::TokensWrite,
        TokenScope::Admin,
    ]) {
        return Err(ApiErrorResponse::forbidden(
            "Missing required scope: tokens:read",
        ));
    }

    let validated = match validate_paseto_token(&request.token, &TOKEN_SECRET_KEY, "en") {
        Ok(token) => token,
        Err(_) => return Ok(Json(invalid_token_response())),
    };

    let is_blacklisted = state
        .token_store
        .is_blacklisted(&validated.token_id)
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;

    if is_blacklisted || validated.is_expired() || validated.tenant_id != session_token.tenant_id {
        return Ok(Json(VerifyTokenResponse {
            valid: false,
            token_id: Some(validated.token_id),
            user_id: Some(validated.user_id),
            tenant_id: Some(validated.tenant_id),
            scopes: Some(
                validated
                    .scopes
                    .into_iter()
                    .map(|scope| scope.as_str().to_string())
                    .collect(),
            ),
            expires_at: Some(validated.expires_at),
        }));
    }

    Ok(Json(VerifyTokenResponse {
        valid: true,
        token_id: Some(validated.token_id),
        user_id: Some(validated.user_id),
        tenant_id: Some(validated.tenant_id),
        scopes: Some(
            validated
                .scopes
                .into_iter()
                .map(|scope| scope.as_str().to_string())
                .collect(),
        ),
        expires_at: Some(validated.expires_at),
    }))
}

async fn get_token_stats_handler() -> Json<TokenStatsResponse> {
    Json(TokenStatsResponse { active_tokens: 0 })
}

fn invalid_token_response() -> VerifyTokenResponse {
    VerifyTokenResponse {
        valid: false,
        token_id: None,
        user_id: None,
        tenant_id: None,
        scopes: None,
        expires_at: None,
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
        middleware::{TokenScope, ValidatedToken},
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
    use async_trait::async_trait;
    use serde_json::Value as JsonValue;
    use uuid::Uuid;

    struct DummyAuthService;

    #[async_trait]
    impl AuthService for DummyAuthService {
        async fn create_user_from_privy(&self, _privy_token: &str) -> Result<User, AuthError> {
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

    fn test_state() -> AuthApiState {
        AuthApiState::new_with_token_store(
            std::sync::Arc::new(DummyAuthService),
            create_token_store(),
        )
    }

    fn session_token(scopes: Vec<TokenScope>) -> ValidatedToken {
        ValidatedToken::mock("tenant_123", "user_123", scopes)
    }

    #[tokio::test]
    async fn create_token_returns_paseto_for_subset_scopes() {
        let response = create_token_handler(
            State(test_state()),
            Extension(session_token(vec![
                TokenScope::TokensWrite,
                TokenScope::CredentialRead,
                TokenScope::AuditRead,
            ])),
            Json(CreateTokenRequest {
                scopes: vec!["credential:read".to_string(), "audit:read".to_string()],
                expires_in: 3600,
            }),
        )
        .await
        .expect("token creation should succeed")
        .0;

        assert!(response.access_token.starts_with("v4.local."));
        assert_eq!(response.scope, "credential:read audit:read");
        assert_eq!(response.expires_in, 3600);
    }

    #[tokio::test]
    async fn verify_token_round_trip_succeeds_for_same_tenant() {
        let created = create_token_handler(
            State(test_state()),
            Extension(session_token(vec![
                TokenScope::TokensWrite,
                TokenScope::TokensRead,
                TokenScope::CredentialRead,
            ])),
            Json(CreateTokenRequest {
                scopes: vec!["credential:read".to_string()],
                expires_in: 120,
            }),
        )
        .await
        .expect("token creation should succeed")
        .0;

        let verified = verify_token_handler(
            State(test_state()),
            Extension(session_token(vec![TokenScope::TokensRead])),
            Json(VerifyTokenRequest {
                token: created.access_token,
            }),
        )
        .await
        .expect("token verify should succeed")
        .0;

        assert!(verified.valid);
        assert_eq!(verified.tenant_id.as_deref(), Some("tenant_123"));
        assert_eq!(
            verified.scopes.unwrap_or_default(),
            vec!["credential:read".to_string()]
        );
    }
}
