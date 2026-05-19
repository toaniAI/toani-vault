use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    routing::{get, post, put},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::{Method, Url};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::{
    audit::AuditStorage,
    middleware::{TokenScope, ValidatedToken},
    response::{ApiErrorResponse, ApiSuccessResponse},
};
use crate::audit::{AuditAction, AuditEntry, Outcome, RedactedParam};
use crate::crypto::hkdf::KeyHierarchy;
use crate::crypto::{CredentialCryptoContext, EncryptedBlob};
use crate::models::CredentialType;
use crate::oauth_broker::{
    AuthTransaction, AuthTransactionStatus, Binding, BindingKind, BindingPolicySnapshot,
    BindingRuntimeStateUpdate, BindingStatus, BindingValidationResult, CreateBindingInput,
    CreateProviderDefinitionInput, GrantFamily, OAuthBrokerError, OAuthBrokerService,
    ProviderDefinition, ProviderValidationCheck, ProviderValidationCheckStatus,
    ProviderValidationResult, StageBindingPolicyInput, StartAuthTransactionInput,
    UpdateProviderDefinitionInput,
};
use crate::tee::SharedEnclave;
use crate::vault::models::ServiceId;
use crate::vault::models::{CreateCredentialRequest, CredentialId, TenantId, UserId};
use crate::vault::storage::CredentialVault;
use jsonwebtoken::{Algorithm, EncodingKey, Header};

#[derive(Clone)]
pub struct OAuthBrokerCredentialRuntime {
    pub vault: Arc<CredentialVault>,
    pub key_hierarchy: Arc<RwLock<KeyHierarchy>>,
    pub enclave: SharedEnclave,
}

#[derive(Clone)]
pub struct OAuthBrokerApiState {
    service: Arc<dyn OAuthBrokerService>,
    audit_storage: Option<Arc<dyn AuditStorage>>,
    credential_runtime: Option<OAuthBrokerCredentialRuntime>,
}

impl OAuthBrokerApiState {
    pub fn new(service: Arc<dyn OAuthBrokerService>) -> Self {
        Self {
            service,
            audit_storage: None,
            credential_runtime: None,
        }
    }

    pub fn service(&self) -> Arc<dyn OAuthBrokerService> {
        Arc::clone(&self.service)
    }

    pub fn with_audit_storage(mut self, audit_storage: Arc<dyn AuditStorage>) -> Self {
        self.audit_storage = Some(audit_storage);
        self
    }

    pub fn with_credential_runtime(
        mut self,
        vault: Arc<CredentialVault>,
        key_hierarchy: Arc<RwLock<KeyHierarchy>>,
        enclave: SharedEnclave,
    ) -> Self {
        self.credential_runtime = Some(OAuthBrokerCredentialRuntime {
            vault,
            key_hierarchy,
            enclave,
        });
        self
    }

    async fn record_audit(
        &self,
        action: AuditAction,
        user_id: &str,
        details: serde_json::Value,
    ) -> Result<(), String> {
        let Some(storage) = &self.audit_storage else {
            return Ok(());
        };

        let entry = AuditEntry::new(
            crate::audit::events::hash_user_id(user_id),
            "session",
            "oauth_broker",
            action,
            Outcome::Success,
            "software_mode",
            Uuid::now_v7().to_string(),
        )
        .with_param("details", RedactedParam::Plain(details.to_string()));

        storage
            .record(entry)
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    async fn record_audit_best_effort(
        &self,
        action: AuditAction,
        user_id: &str,
        details: serde_json::Value,
    ) {
        if let Err(error) = self.record_audit(action, user_id, details).await {
            tracing::warn!(
                user_id,
                "oauth broker mutation succeeded but audit write failed: {error}"
            );
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateProviderDefinitionRequest {
    pub alias: String,
    pub display_name: String,
    pub provider_family: String,
    pub binding_kind: String,
    pub grant_family: String,
    #[serde(default)]
    pub authorization_endpoint: Option<String>,
    #[serde(default)]
    pub token_endpoint: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_auth_method: Option<String>,
    #[serde(default)]
    pub callback_mode: Option<String>,
    #[serde(default)]
    pub adapter_key: Option<String>,
    #[serde(default)]
    pub adapter_version: Option<String>,
    #[serde(default)]
    pub scope_template: Vec<String>,
    #[serde(default)]
    pub runtime_config: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct CreateBindingRequest {
    pub provider_definition_id: String,
    pub alias: String,
    pub purpose: String,
    pub subject_ref: String,
    #[serde(default)]
    pub subject_display_name: Option<String>,
    #[serde(default)]
    pub backing_credential_id: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateProviderDefinitionRequest {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub authorization_endpoint: Option<String>,
    #[serde(default)]
    pub token_endpoint: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_auth_method: Option<String>,
    #[serde(default)]
    pub callback_mode: Option<String>,
    #[serde(default)]
    pub adapter_key: Option<String>,
    #[serde(default)]
    pub adapter_version: Option<String>,
    #[serde(default)]
    pub scope_template: Option<Vec<String>>,
    #[serde(default)]
    pub runtime_config: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct RuntimeInvokeRequest {
    pub binding_handle: String,
    pub request: GovernedHttpRequestSpec,
}

#[derive(Debug, Deserialize)]
pub struct GovernedHttpRequestSpec {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub query: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct RuntimeInvokeResponse {
    pub binding_handle: String,
    pub status_code: u16,
    pub provider_request_id: Option<String>,
    pub body: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct StageBindingPolicyRequest {
    #[serde(default)]
    pub granted_scopes: Vec<String>,
    #[serde(default)]
    pub effective_scopes: Vec<String>,
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    #[serde(default)]
    pub allowed_methods: Vec<String>,
    #[serde(default)]
    pub allowed_path_prefixes: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct StagedBindingPolicyValidationResponse {
    pub binding_id: Uuid,
    pub version: i32,
    pub valid: bool,
    pub validation_status: String,
    pub checks: Vec<ProviderValidationCheck>,
    pub validated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct DeleteBindingResponse {
    pub binding_id: Uuid,
    pub deleted: bool,
}

#[derive(Debug, Deserialize)]
pub struct StartAuthTransactionRequest {
    pub provider_definition_id: String,
    pub alias: String,
    pub purpose: String,
    #[serde(default)]
    pub subject_ref_hint: Option<String>,
    #[serde(default)]
    pub subject_display_name: Option<String>,
    #[serde(default)]
    pub backing_credential_id: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct AuthTransactionResponse {
    pub transaction_id: String,
    pub provider_definition_id: String,
    pub alias: String,
    pub purpose: String,
    pub subject_ref_hint: Option<String>,
    pub subject_display_name: Option<String>,
    pub state: String,
    pub code_challenge_method: String,
    pub authorization_url: String,
    pub redirect_uri: String,
    pub requested_scopes: Vec<String>,
    pub status: String,
    pub expires_at: String,
    pub created_at: String,
    pub consumed_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

pub fn oauth_broker_routes(state: OAuthBrokerApiState) -> Router {
    Router::new()
        .route(
            "/oauth-broker/providers",
            post(create_provider_definition_handler).get(list_provider_definitions_handler),
        )
        .route(
            "/oauth-broker/bindings",
            post(create_binding_handler).get(list_bindings_handler),
        )
        .route(
            "/oauth-broker/providers/:provider_definition_id",
            get(get_provider_definition_handler).patch(update_provider_definition_handler),
        )
        .route(
            "/oauth-broker/providers/:provider_definition_id/validate",
            post(validate_provider_definition_handler),
        )
        .route(
            "/oauth-broker/bindings/:binding_id",
            get(get_binding_handler).delete(delete_binding_handler),
        )
        .route(
            "/oauth-broker/bindings/:binding_id/validate",
            post(validate_binding_handler),
        )
        .route(
            "/oauth-broker/bindings/:binding_id/revoke",
            post(revoke_binding_handler),
        )
        .route(
            "/oauth-broker/bindings/:binding_id/staged-policy",
            put(stage_binding_policy_handler).get(get_staged_binding_policy_handler),
        )
        .route(
            "/oauth-broker/bindings/:binding_id/staged-policy/revalidate",
            post(revalidate_staged_binding_policy_handler),
        )
        .route(
            "/oauth-broker/bindings/:binding_id/staged-policy/apply",
            post(apply_staged_binding_policy_handler),
        )
        .route(
            "/oauth-broker/transactions/start",
            post(start_auth_transaction_handler),
        )
        .route(
            "/oauth-broker/transactions/:transaction_id",
            get(get_auth_transaction_handler),
        )
        .route("/oauth-broker/runtime/invoke", post(runtime_invoke_handler))
        .with_state(state)
}

pub fn oauth_broker_public_routes(state: OAuthBrokerApiState) -> Router {
    Router::new()
        .route("/oauth-broker/callback/lark", get(lark_callback_handler))
        .route("/oauth/callback/lark", get(lark_callback_handler))
        .route("/oauth-broker/callback/google", get(lark_callback_handler))
        .route("/oauth/callback/google", get(lark_callback_handler))
        .with_state(state)
}

async fn create_provider_definition_handler(
    State(state): State<OAuthBrokerApiState>,
    Extension(token): Extension<ValidatedToken>,
    Json(request): Json<CreateProviderDefinitionRequest>,
) -> Result<ApiSuccessResponse<ProviderDefinition>, ApiErrorResponse> {
    require_broker_admin(&token)?;

    if request.alias.trim().is_empty() {
        return Err(ApiErrorResponse::invalid_request(
            "Provider alias is required",
        ));
    }
    if request.display_name.trim().is_empty() {
        return Err(ApiErrorResponse::invalid_request(
            "Provider display_name is required",
        ));
    }
    if request.provider_family.trim().is_empty() {
        return Err(ApiErrorResponse::invalid_request(
            "Provider family is required",
        ));
    }

    let binding_kind = request
        .binding_kind
        .parse::<BindingKind>()
        .map_err(ApiErrorResponse::invalid_request)?;
    let grant_family = request
        .grant_family
        .parse::<GrantFamily>()
        .map_err(ApiErrorResponse::invalid_request)?;

    let input = CreateProviderDefinitionInput {
        tenant_id: parse_uuid_str(&token.tenant_id, "tenant_id")?,
        created_by: parse_uuid_str(&token.user_id, "user_id")?,
        alias: request.alias.trim().to_string(),
        display_name: request.display_name.trim().to_string(),
        provider_family: request.provider_family.trim().to_string(),
        binding_kind,
        grant_family,
        authorization_endpoint: request.authorization_endpoint,
        token_endpoint: request.token_endpoint,
        client_id: request.client_id,
        client_auth_method: request.client_auth_method,
        callback_mode: request.callback_mode,
        adapter_key: request.adapter_key,
        adapter_version: request.adapter_version,
        scope_template: request.scope_template,
        runtime_config: request.runtime_config,
    };

    let created = state
        .service
        .create_provider_definition(input)
        .await
        .map_err(map_oauth_broker_error)?;

    state
        .record_audit_best_effort(
            AuditAction::SystemConfigChange,
            &token.user_id,
            serde_json::json!({
                "audit_event": "oauth_provider_definition_created",
                "provider_definition_id": created.id,
                "alias": created.alias,
                "binding_kind": created.binding_kind.as_str(),
                "grant_family": created.grant_family.as_str(),
            }),
        )
        .await;

    Ok(ApiSuccessResponse::new(created))
}

async fn list_provider_definitions_handler(
    State(state): State<OAuthBrokerApiState>,
    Extension(token): Extension<ValidatedToken>,
) -> Result<ApiSuccessResponse<Vec<ProviderDefinition>>, ApiErrorResponse> {
    require_broker_admin(&token)?;

    let items = state
        .service
        .list_provider_definitions(parse_uuid_str(&token.tenant_id, "tenant_id")?)
        .await
        .map_err(map_oauth_broker_error)?;

    Ok(ApiSuccessResponse::new(items))
}

async fn get_provider_definition_handler(
    State(state): State<OAuthBrokerApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(provider_definition_id): Path<String>,
) -> Result<ApiSuccessResponse<ProviderDefinition>, ApiErrorResponse> {
    require_broker_admin(&token)?;

    let item = state
        .service
        .get_provider_definition(parse_uuid_str(
            &provider_definition_id,
            "provider_definition_id",
        )?)
        .await
        .map_err(map_oauth_broker_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Provider definition not found"))?;

    if item.tenant_id.to_string() != token.tenant_id {
        return Err(ApiErrorResponse::forbidden(
            "Cannot access a provider definition from another tenant",
        ));
    }

    Ok(ApiSuccessResponse::new(item))
}

async fn update_provider_definition_handler(
    State(state): State<OAuthBrokerApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(provider_definition_id): Path<String>,
    Json(request): Json<UpdateProviderDefinitionRequest>,
) -> Result<ApiSuccessResponse<ProviderDefinition>, ApiErrorResponse> {
    require_broker_admin(&token)?;
    let tenant_id = parse_uuid_str(&token.tenant_id, "tenant_id")?;
    let provider_definition_id = parse_uuid_str(&provider_definition_id, "provider_definition_id")?;

    let current = state
        .service
        .get_provider_definition(provider_definition_id)
        .await
        .map_err(map_oauth_broker_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Provider definition not found"))?;
    if current.tenant_id != tenant_id {
        return Err(ApiErrorResponse::forbidden(
            "Cannot update a provider definition from another tenant",
        ));
    }

    let updated = state
        .service
        .update_provider_definition(
            tenant_id,
            provider_definition_id,
            UpdateProviderDefinitionInput {
                updated_by: parse_uuid_str(&token.user_id, "user_id")?,
                display_name: request.display_name,
                authorization_endpoint: request.authorization_endpoint,
                token_endpoint: request.token_endpoint,
                client_id: request.client_id,
                client_auth_method: request.client_auth_method,
                callback_mode: request.callback_mode,
                adapter_key: request.adapter_key,
                adapter_version: request.adapter_version,
                scope_template: request.scope_template,
                runtime_config: request.runtime_config,
            },
        )
        .await
        .map_err(map_oauth_broker_error)?;

    state
        .record_audit_best_effort(
            AuditAction::SystemConfigChange,
            &token.user_id,
            serde_json::json!({
                "audit_event": "oauth_provider_definition_updated",
                "provider_definition_id": updated.id,
                "alias": updated.alias,
                "version": updated.version,
                "adapter_key": updated.adapter_key,
                "adapter_version": updated.adapter_version,
            }),
        )
        .await;

    Ok(ApiSuccessResponse::new(updated))
}

async fn validate_provider_definition_handler(
    State(state): State<OAuthBrokerApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(provider_definition_id): Path<String>,
) -> Result<ApiSuccessResponse<ProviderValidationResult>, ApiErrorResponse> {
    require_broker_admin(&token)?;
    let tenant_id = parse_uuid_str(&token.tenant_id, "tenant_id")?;
    let provider_definition_id = parse_uuid_str(&provider_definition_id, "provider_definition_id")?;

    let current = state
        .service
        .get_provider_definition(provider_definition_id)
        .await
        .map_err(map_oauth_broker_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Provider definition not found"))?;
    if current.tenant_id != tenant_id {
        return Err(ApiErrorResponse::forbidden(
            "Cannot validate a provider definition from another tenant",
        ));
    }

    let result = state
        .service
        .validate_provider_definition(tenant_id, provider_definition_id)
        .await
        .map_err(map_oauth_broker_error)?;

    Ok(ApiSuccessResponse::new(result))
}

async fn create_binding_handler(
    State(state): State<OAuthBrokerApiState>,
    Extension(token): Extension<ValidatedToken>,
    Json(request): Json<CreateBindingRequest>,
) -> Result<ApiSuccessResponse<Binding>, ApiErrorResponse> {
    require_broker_admin(&token)?;

    if request.alias.trim().is_empty() {
        return Err(ApiErrorResponse::invalid_request(
            "Binding alias is required",
        ));
    }
    if request.purpose.trim().is_empty() {
        return Err(ApiErrorResponse::invalid_request(
            "Binding purpose is required",
        ));
    }
    if request.subject_ref.trim().is_empty() {
        return Err(ApiErrorResponse::invalid_request(
            "Binding subject_ref is required",
        ));
    }
    ensure_backing_credential_access(
        &state,
        parse_uuid_str(&token.tenant_id, "tenant_id")?,
        parse_uuid_str(&token.user_id, "user_id")?,
        request.backing_credential_id.as_deref(),
        "backing_credential_id",
    )?;

    let input = CreateBindingInput {
        tenant_id: parse_uuid_str(&token.tenant_id, "tenant_id")?,
        created_by: parse_uuid_str(&token.user_id, "user_id")?,
        provider_definition_id: parse_uuid_str(
            &request.provider_definition_id,
            "provider_definition_id",
        )?,
        alias: request.alias.trim().to_string(),
        purpose: request.purpose.trim().to_string(),
        subject_ref: request.subject_ref.trim().to_string(),
        subject_display_name: request.subject_display_name,
        backing_credential_id: request.backing_credential_id,
    };

    let created = state
        .service
        .create_binding(input)
        .await
        .map_err(map_oauth_broker_error)?;

    state
        .record_audit_best_effort(
            AuditAction::SystemConfigChange,
            &token.user_id,
            serde_json::json!({
                "audit_event": "oauth_binding_created",
                "binding_id": created.id,
                "binding_handle": created.binding_handle,
                "alias": created.alias,
                "binding_kind": created.binding_kind.as_str(),
                "status": created.status.as_str(),
            }),
        )
        .await;

    Ok(ApiSuccessResponse::new(created))
}

async fn list_bindings_handler(
    State(state): State<OAuthBrokerApiState>,
    Extension(token): Extension<ValidatedToken>,
) -> Result<ApiSuccessResponse<Vec<Binding>>, ApiErrorResponse> {
    require_broker_admin(&token)?;

    let items = state
        .service
        .list_bindings(parse_uuid_str(&token.tenant_id, "tenant_id")?)
        .await
        .map_err(map_oauth_broker_error)?;

    Ok(ApiSuccessResponse::new(items))
}

async fn get_binding_handler(
    State(state): State<OAuthBrokerApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(binding_id): Path<String>,
) -> Result<ApiSuccessResponse<Binding>, ApiErrorResponse> {
    require_broker_admin(&token)?;

    let item = state
        .service
        .get_binding(parse_uuid_str(&binding_id, "binding_id")?)
        .await
        .map_err(map_oauth_broker_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Binding not found"))?;

    if item.tenant_id.to_string() != token.tenant_id {
        return Err(ApiErrorResponse::forbidden(
            "Cannot access a binding from another tenant",
        ));
    }

    Ok(ApiSuccessResponse::new(item))
}

async fn validate_binding_handler(
    State(state): State<OAuthBrokerApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(binding_id): Path<String>,
) -> Result<ApiSuccessResponse<BindingValidationResult>, ApiErrorResponse> {
    require_broker_admin(&token)?;

    let binding_id = parse_uuid_str(&binding_id, "binding_id")?;
    let binding = state
        .service
        .get_binding(binding_id)
        .await
        .map_err(map_oauth_broker_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Binding not found"))?;

    if binding.tenant_id.to_string() != token.tenant_id {
        return Err(ApiErrorResponse::forbidden(
            "Cannot validate a binding from another tenant",
        ));
    }

    let provider = state
        .service
        .get_provider_definition(binding.provider_definition_id)
        .await
        .map_err(map_oauth_broker_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Provider definition not found"))?;

    let validated_at = chrono::Utc::now();
    let mut checks = Vec::new();
    let credential_payload = match load_binding_secret_payload(&state, &binding).await {
        Ok(payload) => {
            checks.push(ProviderValidationCheck::passed(
                "backing_credential",
                "backing credential is present and decryptable",
            ));
            payload
        }
        Err(error) => {
            checks.push(ProviderValidationCheck::failed(
                "backing_credential",
                error.to_string(),
            ));
            let updated = state
                .service
                .set_binding_validation_state(
                    binding.id,
                    BindingStatus::Draft,
                    "validation_failed",
                    Some("backing_credential_invalid"),
                    Some(error.to_string().as_str()),
                    validated_at,
                )
                .await
                .map_err(map_oauth_broker_error)?;

            return Ok(ApiSuccessResponse::new(BindingValidationResult {
                binding_id: updated.id,
                binding_handle: updated.binding_handle,
                valid: false,
                status: updated.status.as_str().to_string(),
                health_status: updated
                    .health_status
                    .unwrap_or_else(|| "validation_failed".to_string()),
                checks,
                validated_at,
            }));
        }
    };

    checks.push(probe_binding_access_token(&provider, &credential_payload).await);
    let valid = checks
        .iter()
        .all(|check| check.status == ProviderValidationCheckStatus::Passed);
    let updated = if valid {
        state
            .service
            .set_binding_validation_state(
                binding.id,
                BindingStatus::Ready,
                "ready",
                None,
                None,
                validated_at,
            )
            .await
            .map_err(map_oauth_broker_error)?
    } else {
        let failure_detail = checks
            .iter()
            .find(|check| check.status == ProviderValidationCheckStatus::Failed)
            .map(|check| check.detail.clone())
            .unwrap_or_else(|| "binding validation failed".to_string());
        state
            .service
            .set_binding_validation_state(
                binding.id,
                BindingStatus::Draft,
                "validation_failed",
                Some("token_probe_failed"),
                Some(failure_detail.as_str()),
                validated_at,
            )
            .await
            .map_err(map_oauth_broker_error)?
    };

    state
        .record_audit_best_effort(
            AuditAction::SystemConfigChange,
            &token.user_id,
            json!({
                "audit_event": "oauth_binding_validated",
                "binding_id": updated.id,
                "binding_handle": updated.binding_handle,
                "valid": valid,
                "status": updated.status.as_str(),
                "health_status": updated.health_status,
            }),
        )
        .await;

    Ok(ApiSuccessResponse::new(BindingValidationResult {
        binding_id: updated.id,
        binding_handle: updated.binding_handle,
        valid,
        status: updated.status.as_str().to_string(),
        health_status: updated.health_status.unwrap_or_else(|| {
            if valid {
                "ready".to_string()
            } else {
                "validation_failed".to_string()
            }
        }),
        checks,
        validated_at,
    }))
}

async fn stage_binding_policy_handler(
    State(state): State<OAuthBrokerApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(binding_id): Path<String>,
    Json(request): Json<StageBindingPolicyRequest>,
) -> Result<ApiSuccessResponse<BindingPolicySnapshot>, ApiErrorResponse> {
    require_broker_admin(&token)?;

    let binding_id = parse_uuid_str(&binding_id, "binding_id")?;
    let binding = state
        .service
        .get_binding(binding_id)
        .await
        .map_err(map_oauth_broker_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Binding not found"))?;
    if binding.tenant_id.to_string() != token.tenant_id {
        return Err(ApiErrorResponse::forbidden(
            "Cannot stage policy for a binding from another tenant",
        ));
    }

    let staged = state
        .service
        .stage_binding_policy(
            binding_id,
            StageBindingPolicyInput {
                granted_scopes: request.granted_scopes,
                effective_scopes: request.effective_scopes,
                allowed_domains: request.allowed_domains,
                allowed_methods: request.allowed_methods,
                allowed_path_prefixes: request.allowed_path_prefixes,
            },
            chrono::Utc::now(),
        )
        .await
        .map_err(map_oauth_broker_error)?;

    state
        .record_audit_best_effort(
            AuditAction::SystemConfigChange,
            &token.user_id,
            json!({
                "audit_event": "oauth_binding_policy_staged",
                "binding_id": binding_id,
                "version": staged.version,
            }),
        )
        .await;

    Ok(ApiSuccessResponse::new(staged))
}

async fn get_staged_binding_policy_handler(
    State(state): State<OAuthBrokerApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(binding_id): Path<String>,
) -> Result<ApiSuccessResponse<BindingPolicySnapshot>, ApiErrorResponse> {
    require_broker_admin(&token)?;

    let binding_id = parse_uuid_str(&binding_id, "binding_id")?;
    let binding = state
        .service
        .get_binding(binding_id)
        .await
        .map_err(map_oauth_broker_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Binding not found"))?;
    if binding.tenant_id.to_string() != token.tenant_id {
        return Err(ApiErrorResponse::forbidden(
            "Cannot access staged policy from another tenant",
        ));
    }

    let staged = state
        .service
        .get_staged_binding_policy(binding_id)
        .await
        .map_err(map_oauth_broker_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Staged binding policy not found"))?;

    Ok(ApiSuccessResponse::new(staged))
}

async fn revalidate_staged_binding_policy_handler(
    State(state): State<OAuthBrokerApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(binding_id): Path<String>,
) -> Result<ApiSuccessResponse<StagedBindingPolicyValidationResponse>, ApiErrorResponse> {
    require_broker_admin(&token)?;

    let binding_id = parse_uuid_str(&binding_id, "binding_id")?;
    let binding = state
        .service
        .get_binding(binding_id)
        .await
        .map_err(map_oauth_broker_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Binding not found"))?;
    if binding.tenant_id.to_string() != token.tenant_id {
        return Err(ApiErrorResponse::forbidden(
            "Cannot revalidate staged policy from another tenant",
        ));
    }

    let staged = state
        .service
        .get_staged_binding_policy(binding_id)
        .await
        .map_err(map_oauth_broker_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Staged binding policy not found"))?;

    let provider = state
        .service
        .get_provider_definition(binding.provider_definition_id)
        .await
        .map_err(map_oauth_broker_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Provider definition not found"))?;
    let credential_payload = load_binding_secret_payload(&state, &binding)
        .await
        .map_err(map_oauth_broker_error)?;

    let mut checks = Vec::new();
    let scopes_ok = staged
        .effective_scopes
        .iter()
        .all(|scope| staged.granted_scopes.iter().any(|granted| granted == scope));
    if scopes_ok {
        checks.push(ProviderValidationCheck::passed(
            "scope_subset",
            "effective scopes stay within granted scopes",
        ));
    } else {
        checks.push(ProviderValidationCheck::failed(
            "scope_subset",
            "effective scopes must be a subset of granted scopes",
        ));
    }

    let methods_ok = staged
        .allowed_methods
        .iter()
        .all(|method| Method::from_bytes(method.as_bytes()).is_ok());
    if methods_ok {
        checks.push(ProviderValidationCheck::passed(
            "allowed_methods",
            "all staged methods are valid HTTP verbs",
        ));
    } else {
        checks.push(ProviderValidationCheck::failed(
            "allowed_methods",
            "staged policy contains an invalid HTTP method",
        ));
    }

    let prefixes_ok = staged
        .allowed_path_prefixes
        .iter()
        .all(|prefix| prefix.starts_with('/'));
    if prefixes_ok {
        checks.push(ProviderValidationCheck::passed(
            "allowed_path_prefixes",
            "all staged path prefixes are absolute paths",
        ));
    } else {
        checks.push(ProviderValidationCheck::failed(
            "allowed_path_prefixes",
            "staged path prefixes must start with '/'",
        ));
    }

    checks.push(probe_binding_access_token(&provider, &credential_payload).await);

    let validated_at = chrono::Utc::now();
    let valid = checks
        .iter()
        .all(|check| check.status == ProviderValidationCheckStatus::Passed);
    let updated = if valid {
        state
            .service
            .set_staged_binding_policy_validation(binding_id, "validated", None, None, validated_at)
            .await
            .map_err(map_oauth_broker_error)?
    } else {
        let failure_detail = checks
            .iter()
            .find(|check| check.status == ProviderValidationCheckStatus::Failed)
            .map(|check| check.detail.clone())
            .unwrap_or_else(|| "staged binding policy validation failed".to_string());
        state
            .service
            .set_staged_binding_policy_validation(
                binding_id,
                "validation_failed",
                Some("staged_policy_invalid"),
                Some(failure_detail.as_str()),
                validated_at,
            )
            .await
            .map_err(map_oauth_broker_error)?
    };

    Ok(ApiSuccessResponse::new(
        StagedBindingPolicyValidationResponse {
            binding_id,
            version: updated.version,
            valid,
            validation_status: updated.validation_status,
            checks,
            validated_at,
        },
    ))
}

async fn apply_staged_binding_policy_handler(
    State(state): State<OAuthBrokerApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(binding_id): Path<String>,
) -> Result<ApiSuccessResponse<BindingPolicySnapshot>, ApiErrorResponse> {
    require_broker_admin(&token)?;

    let binding_id = parse_uuid_str(&binding_id, "binding_id")?;
    let binding = state
        .service
        .get_binding(binding_id)
        .await
        .map_err(map_oauth_broker_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Binding not found"))?;
    if binding.tenant_id.to_string() != token.tenant_id {
        return Err(ApiErrorResponse::forbidden(
            "Cannot apply staged policy from another tenant",
        ));
    }

    let applied = state
        .service
        .apply_staged_binding_policy(binding_id, chrono::Utc::now())
        .await
        .map_err(map_oauth_broker_error)?;

    state
        .record_audit_best_effort(
            AuditAction::SystemConfigChange,
            &token.user_id,
            json!({
                "audit_event": "oauth_binding_policy_applied",
                "binding_id": binding_id,
                "version": applied.version,
            }),
        )
        .await;

    Ok(ApiSuccessResponse::new(applied))
}

async fn revoke_binding_handler(
    State(state): State<OAuthBrokerApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(binding_id): Path<String>,
) -> Result<ApiSuccessResponse<Binding>, ApiErrorResponse> {
    require_broker_admin(&token)?;

    let binding_id = parse_uuid_str(&binding_id, "binding_id")?;
    let binding = state
        .service
        .get_binding(binding_id)
        .await
        .map_err(map_oauth_broker_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Binding not found"))?;
    if binding.tenant_id.to_string() != token.tenant_id {
        return Err(ApiErrorResponse::forbidden(
            "Cannot revoke a binding from another tenant",
        ));
    }

    let revoked = state
        .service
        .revoke_binding(binding_id, chrono::Utc::now())
        .await
        .map_err(map_oauth_broker_error)?;

    state
        .record_audit_best_effort(
            AuditAction::SystemConfigChange,
            &token.user_id,
            json!({
                "audit_event": "oauth_binding_revoked",
                "binding_id": binding_id,
                "binding_handle": revoked.binding_handle,
            }),
        )
        .await;

    Ok(ApiSuccessResponse::new(revoked))
}

async fn delete_binding_handler(
    State(state): State<OAuthBrokerApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(binding_id): Path<String>,
) -> Result<ApiSuccessResponse<DeleteBindingResponse>, ApiErrorResponse> {
    require_broker_admin(&token)?;

    let binding_id = parse_uuid_str(&binding_id, "binding_id")?;
    let binding = state
        .service
        .get_binding(binding_id)
        .await
        .map_err(map_oauth_broker_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Binding not found"))?;
    if binding.tenant_id.to_string() != token.tenant_id {
        return Err(ApiErrorResponse::forbidden(
            "Cannot delete a binding from another tenant",
        ));
    }

    state
        .service
        .delete_binding(binding_id)
        .await
        .map_err(map_oauth_broker_error)?;

    state
        .record_audit_best_effort(
            AuditAction::SystemConfigChange,
            &token.user_id,
            json!({
                "audit_event": "oauth_binding_deleted",
                "binding_id": binding_id,
                "binding_handle": binding.binding_handle,
            }),
        )
        .await;

    Ok(ApiSuccessResponse::new(DeleteBindingResponse {
        binding_id,
        deleted: true,
    }))
}

async fn start_auth_transaction_handler(
    State(state): State<OAuthBrokerApiState>,
    Extension(token): Extension<ValidatedToken>,
    Json(request): Json<StartAuthTransactionRequest>,
) -> Result<ApiSuccessResponse<AuthTransactionResponse>, ApiErrorResponse> {
    require_broker_admin(&token)?;

    if request.alias.trim().is_empty() {
        return Err(ApiErrorResponse::invalid_request(
            "Transaction alias is required",
        ));
    }
    if request.purpose.trim().is_empty() {
        return Err(ApiErrorResponse::invalid_request(
            "Transaction purpose is required",
        ));
    }
    ensure_backing_credential_access(
        &state,
        parse_uuid_str(&token.tenant_id, "tenant_id")?,
        parse_uuid_str(&token.user_id, "user_id")?,
        request.backing_credential_id.as_deref(),
        "backing_credential_id",
    )?;

    let transaction = state
        .service
        .start_auth_transaction(StartAuthTransactionInput {
            tenant_id: parse_uuid_str(&token.tenant_id, "tenant_id")?,
            created_by: parse_uuid_str(&token.user_id, "user_id")?,
            provider_definition_id: parse_uuid_str(
                &request.provider_definition_id,
                "provider_definition_id",
            )?,
            alias: request.alias.trim().to_string(),
            purpose: request.purpose.trim().to_string(),
            subject_ref_hint: request.subject_ref_hint,
            subject_display_name: request.subject_display_name,
            backing_credential_id: request.backing_credential_id,
            requested_scopes: request.scopes,
        })
        .await
        .map_err(map_oauth_broker_error)?;

    state
        .record_audit_best_effort(
            AuditAction::SystemConfigChange,
            &token.user_id,
            json!({
                "audit_event": "oauth_auth_transaction_started",
                "transaction_id": transaction.id,
                "provider_definition_id": transaction.provider_definition_id,
                "state": transaction.state,
                "status": transaction.status.as_str(),
            }),
        )
        .await;

    Ok(ApiSuccessResponse::new(map_auth_transaction(transaction)))
}

async fn get_auth_transaction_handler(
    State(state): State<OAuthBrokerApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(transaction_id): Path<String>,
) -> Result<ApiSuccessResponse<AuthTransactionResponse>, ApiErrorResponse> {
    require_broker_admin(&token)?;

    let transaction = state
        .service
        .get_auth_transaction(parse_uuid_str(&transaction_id, "transaction_id")?)
        .await
        .map_err(map_oauth_broker_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Auth transaction not found"))?;

    if transaction.tenant_id.to_string() != token.tenant_id {
        return Err(ApiErrorResponse::forbidden(
            "Cannot access an auth transaction from another tenant",
        ));
    }

    Ok(ApiSuccessResponse::new(map_auth_transaction(transaction)))
}

async fn lark_callback_handler(
    State(state): State<OAuthBrokerApiState>,
    axum::extract::Query(query): axum::extract::Query<CallbackQuery>,
) -> Result<ApiSuccessResponse<serde_json::Value>, ApiErrorResponse> {
    let state_value = query
        .state
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ApiErrorResponse::invalid_request("callback state is required"))?;

    let transaction = state
        .service
        .get_auth_transaction_by_state(state_value)
        .await
        .map_err(map_oauth_broker_error)?
        .ok_or_else(|| {
            ApiErrorResponse::invalid_request("callback state does not match any transaction")
        })?;

    if transaction.status == crate::oauth_broker::AuthTransactionStatus::Consumed
        || transaction.consumed_at.is_some()
    {
        return Err(ApiErrorResponse::conflict(
            "callback transaction has already been consumed",
        ));
    }

    if transaction.expires_at <= chrono::Utc::now() {
        return Err(ApiErrorResponse::invalid_request(
            "callback transaction has expired",
        ));
    }

    if query.code.as_deref().unwrap_or_default().trim().is_empty() {
        return Err(ApiErrorResponse::invalid_request(
            "callback code is required",
        ));
    }

    if let Some(error) = query.error.as_deref()
        && !error.trim().is_empty()
    {
        return Err(ApiErrorResponse::invalid_request(format!(
            "callback returned provider error: {error}"
        )));
    }

    let provider = state
        .service
        .get_provider_definition(transaction.provider_definition_id)
        .await
        .map_err(map_oauth_broker_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Provider definition not found"))?;
    let callback_exchange = exchange_delegated_binding_payload(
        &provider,
        &state,
        &transaction,
        query.code.as_deref().unwrap_or_default(),
    )
    .await
    .map_err(ApiErrorResponse::invalid_request)?;

    let backing_credential_id = store_binding_secret_payload(
        &state,
        transaction.tenant_id,
        transaction.created_by,
        &transaction.alias,
        &callback_exchange.credential_payload,
    )
    .await
    .map_err(map_oauth_broker_error)?;

    let binding = match state
        .service
        .create_binding(CreateBindingInput {
            tenant_id: transaction.tenant_id,
            created_by: transaction.created_by,
            provider_definition_id: transaction.provider_definition_id,
            alias: transaction.alias.clone(),
            purpose: transaction.purpose.clone(),
            subject_ref: callback_exchange.subject_ref.clone(),
            subject_display_name: callback_exchange
                .subject_display_name
                .clone()
                .or(transaction.subject_display_name.clone()),
            backing_credential_id: Some(backing_credential_id.clone()),
        })
        .await
    {
        Ok(binding) => binding,
        Err(error) => {
            cleanup_stored_binding_secret(
                &state,
                transaction.tenant_id,
                transaction.created_by,
                &backing_credential_id,
            );
            return Err(map_oauth_broker_error(error));
        }
    };

    let validated_at = chrono::Utc::now();
    let binding = state
        .service
        .set_binding_validation_state(
            binding.id,
            BindingStatus::Ready,
            "ready",
            None,
            None,
            validated_at,
        )
        .await
        .map_err(map_oauth_broker_error)?;

    state
        .service
        .set_auth_transaction_status(
            transaction.id,
            AuthTransactionStatus::Consumed,
            Some(validated_at),
        )
        .await
        .map_err(map_oauth_broker_error)?;

    state
        .record_audit_best_effort(
            AuditAction::SystemConfigChange,
            "oauth_callback",
            json!({
                "audit_event": "oauth_callback_consumed",
                "transaction_id": transaction.id,
                "binding_id": binding.id,
                "binding_handle": binding.binding_handle,
                "subject_ref": callback_exchange.subject_ref,
            }),
        )
        .await;

    Ok(ApiSuccessResponse::new(json!({
        "transaction_id": transaction.id,
        "binding_id": binding.id,
        "binding_handle": binding.binding_handle,
        "status": binding.status.as_str(),
        "health_status": binding.health_status,
    })))
}

async fn runtime_invoke_handler(
    State(state): State<OAuthBrokerApiState>,
    Extension(token): Extension<ValidatedToken>,
    Json(request): Json<RuntimeInvokeRequest>,
) -> Result<ApiSuccessResponse<RuntimeInvokeResponse>, ApiErrorResponse> {
    if token.token_plane() != "runtime" {
        return Err(ApiErrorResponse::forbidden(
            "OAuth broker runtime invocation requires a runtime token",
        ));
    }
    if !token.has_scope(&TokenScope::CredentialRead) {
        return Err(ApiErrorResponse::forbidden(
            "OAuth broker runtime invocation requires credential:read scope",
        ));
    }

    let tenant_id = parse_uuid_str(&token.tenant_id, "tenant_id")?;
    let binding = state
        .service
        .get_binding_by_handle(tenant_id, &request.binding_handle)
        .await
        .map_err(map_oauth_broker_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Binding not found"))?;

    if !token.can_access_binding_handle(&binding.binding_handle) {
        return Err(ApiErrorResponse::forbidden(
            "Requested binding_handle is not allowed by the current token",
        ));
    }
    if binding.rebind_required {
        return Err(ApiErrorResponse::conflict(
            "Binding requires rebind before runtime invocation",
        ));
    }
    if binding
        .cooldown_until
        .is_some_and(|until| until > chrono::Utc::now())
    {
        return Err(ApiErrorResponse::conflict(
            "Binding is currently in cooldown after token refresh failure",
        ));
    }
    if binding.status != BindingStatus::Ready {
        return Err(ApiErrorResponse::conflict(
            "Binding is not ready for runtime invocation",
        ));
    }

    let provider = state
        .service
        .get_provider_definition(binding.provider_definition_id)
        .await
        .map_err(map_oauth_broker_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Provider definition not found"))?;
    let policy = state
        .service
        .get_latest_binding_policy_snapshot(binding.id)
        .await
        .map_err(map_oauth_broker_error)?
        .ok_or_else(|| ApiErrorResponse::not_found("Binding policy snapshot not found"))?;

    let method = Method::from_bytes(request.request.method.as_bytes())
        .map_err(|_| ApiErrorResponse::invalid_request("request.method is invalid"))?;
    let mut url = Url::parse(&request.request.url)
        .map_err(|_| ApiErrorResponse::invalid_request("request.url must be an absolute URL"))?;
    authorize_runtime_request(&policy, &method, &url, &request.request.headers)?;

    if targets_provider_token_endpoint(&request.request.url, provider.token_endpoint.as_deref()) {
        return Err(ApiErrorResponse::forbidden(
            "Runtime invocation cannot target the provider token endpoint",
        ));
    }

    for (key, value) in &request.request.query {
        url.query_pairs_mut().append_pair(key, value);
    }

    let credential_payload = load_binding_secret_payload(&state, &binding)
        .await
        .map_err(map_oauth_broker_error)?;
    let token_response = match exchange_provider_access_token(&provider, &credential_payload).await
    {
        Ok(token_response) => token_response,
        Err(error) => {
            let now = chrono::Utc::now();
            let runtime_state = classify_runtime_exchange_failure(&provider, &error, now);
            state
                .service
                .set_binding_runtime_state(
                    binding.id,
                    BindingRuntimeStateUpdate {
                        binding_status: None,
                        health_status: runtime_state.health_status.to_string(),
                        last_error_code: Some(runtime_state.last_error_code.to_string()),
                        last_error_message: Some(error.clone()),
                        cooldown_until: runtime_state.cooldown_until,
                        rebind_required: runtime_state.rebind_required,
                        updated_at: now,
                    },
                )
                .await
                .map_err(map_oauth_broker_error)?;
            return Err(ApiErrorResponse::conflict(runtime_state.user_message));
        }
    };

    let mut outbound = reqwest::Client::new()
        .request(method.clone(), url)
        .bearer_auth(token_response.access_token);

    for (key, value) in &request.request.headers {
        let normalized = key.to_ascii_lowercase();
        if matches!(normalized.as_str(), "authorization" | "host" | "cookie") {
            return Err(ApiErrorResponse::forbidden(format!(
                "request.headers may not override reserved header: {key}"
            )));
        }
        outbound = outbound.header(key, value);
    }

    if let Some(body) = &request.request.body {
        outbound = outbound.json(body);
    }

    let response = outbound
        .send()
        .await
        .map_err(|error| ApiErrorResponse::internal_error(error.to_string()))?;
    let status_code = response.status().as_u16();
    let provider_request_id = response
        .headers()
        .get("x-tt-logid")
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string)
        .or_else(|| {
            response
                .headers()
                .get("x-request-id")
                .and_then(|value| value.to_str().ok())
                .map(ToString::to_string)
        });

    let body = match response.json::<serde_json::Value>().await {
        Ok(json_body) => json_body,
        Err(error) => {
            return Err(ApiErrorResponse::internal_error(format!(
                "provider response body is not valid JSON: {error}"
            )));
        }
    };
    let body = redact_sensitive_json(body);

    state
        .record_audit_best_effort(
            AuditAction::CredentialAccess,
            &token.user_id,
            json!({
                "audit_event": "oauth_binding_runtime_invoked",
                "binding_id": binding.id,
                "binding_handle": binding.binding_handle,
                "provider_definition_id": provider.id,
                "method": method.as_str(),
                "url": request.request.url,
                "status_code": status_code,
                "provider_request_id": provider_request_id,
            }),
        )
        .await;

    Ok(ApiSuccessResponse::new(RuntimeInvokeResponse {
        binding_handle: binding.binding_handle,
        status_code,
        provider_request_id,
        body,
    }))
}

#[derive(Debug, Deserialize, Serialize)]
struct LarkTokenProbeResponse {
    code: i32,
    msg: Option<String>,
    tenant_access_token: Option<String>,
    app_access_token: Option<String>,
}

async fn load_binding_secret_payload(
    state: &OAuthBrokerApiState,
    binding: &Binding,
) -> Result<serde_json::Value, OAuthBrokerError> {
    let runtime = state.credential_runtime.as_ref().ok_or_else(|| {
        OAuthBrokerError::InternalError(
            "OAuth broker credential runtime is not configured".to_string(),
        )
    })?;
    let backing_credential_id = binding.backing_credential_id.as_ref().ok_or_else(|| {
        OAuthBrokerError::InvalidRequest("binding is missing backing_credential_id".to_string())
    })?;

    let credential_id = CredentialId::from_string(backing_credential_id.clone())
        .map_err(|error| OAuthBrokerError::InvalidRequest(error.to_string()))?;
    let tenant_id = TenantId::new(binding.tenant_id.to_string());
    let user_id = UserId::new(binding.created_by.to_string());
    let entry = runtime
        .vault
        .get_credential(&credential_id, &tenant_id, &user_id)
        .map_err(|error| OAuthBrokerError::InternalError(error.to_string()))?
        .ok_or_else(|| {
            OAuthBrokerError::InvalidRequest(format!(
                "backing credential not found: {}",
                credential_id.as_str()
            ))
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

    let plaintext = {
        let mut enclave = runtime.enclave.lock().await;
        if enclave.is_running() {
            enclave
                .decrypt_credential(
                    tenant_id.as_str(),
                    user_id.hash(),
                    credential_id.as_str(),
                    &blob,
                )
                .map_err(|error| OAuthBrokerError::InternalError(error.to_string()))?
        } else {
            let hierarchy = runtime.key_hierarchy.read().await;
            let context = CredentialCryptoContext::new(
                tenant_id.as_str(),
                user_id.hash(),
                credential_id.as_str(),
            );
            context
                .decrypt_with_hierarchy(&hierarchy, &blob)
                .map_err(|error| OAuthBrokerError::InternalError(error.to_string()))?
        }
    };

    serde_json::from_slice(&plaintext).map_err(|error| {
        OAuthBrokerError::InvalidRequest(format!("credential plaintext is not valid JSON: {error}"))
    })
}

async fn probe_binding_access_token(
    provider: &ProviderDefinition,
    credential_payload: &serde_json::Value,
) -> ProviderValidationCheck {
    match exchange_provider_access_token(provider, credential_payload).await {
        Ok(result) => ProviderValidationCheck::passed("token_probe", result.probe_detail),
        Err(error) => ProviderValidationCheck::failed("token_probe", error),
    }
}

#[derive(Debug, Clone)]
struct ProviderAccessTokenExchangeResult {
    access_token: String,
    probe_detail: String,
}

struct RuntimeRefreshFailureState<'a> {
    health_status: &'a str,
    last_error_code: &'a str,
    cooldown_until: Option<chrono::DateTime<chrono::Utc>>,
    rebind_required: bool,
    user_message: &'a str,
}

fn classify_runtime_exchange_failure<'a>(
    provider: &ProviderDefinition,
    error: &'a str,
    now: chrono::DateTime<chrono::Utc>,
) -> RuntimeRefreshFailureState<'a> {
    let normalized = error.to_ascii_lowercase();
    if normalized.contains("invalid_grant")
        || normalized.contains("missing refresh token")
        || normalized.contains("refresh token was revoked")
        || normalized.contains("refresh token revoked")
        || normalized.contains("refresh token expired")
        || normalized.contains("requires rebind")
    {
        return RuntimeRefreshFailureState {
            health_status: "rebind_required",
            last_error_code: "token_rebind_required",
            cooldown_until: None,
            rebind_required: true,
            user_message: "Binding requires rebind after token refresh failure",
        };
    }

    let cooldown_seconds = provider
        .runtime_config
        .get("cooldown_seconds")
        .and_then(serde_json::Value::as_i64)
        .filter(|value| *value > 0)
        .unwrap_or(300);
    RuntimeRefreshFailureState {
        health_status: "cooldown",
        last_error_code: "token_refresh_failed",
        cooldown_until: Some(now + chrono::Duration::seconds(cooldown_seconds)),
        rebind_required: false,
        user_message: "Binding entered cooldown after token refresh failure",
    }
}

struct DelegatedBindingExchangeResult {
    subject_ref: String,
    subject_display_name: Option<String>,
    credential_payload: serde_json::Value,
}

async fn exchange_provider_access_token(
    provider: &ProviderDefinition,
    credential_payload: &serde_json::Value,
) -> Result<ProviderAccessTokenExchangeResult, String> {
    match provider.adapter_key.as_deref() {
        Some("lark_openapi") => exchange_lark_access_token(provider, credential_payload).await,
        Some("oauth_openid") => {
            exchange_standard_oauth_access_token(provider, credential_payload).await
        }
        Some("google_service_account") => {
            exchange_google_service_account_access_token(provider, credential_payload).await
        }
        Some(other) => Err(format!(
            "unsupported adapter_key for token exchange: {other}"
        )),
        None => Err("provider adapter_key is required for token exchange".to_string()),
    }
}

#[derive(Debug, Deserialize)]
struct OidcIdTokenClaims {
    sub: Option<String>,
    email: Option<String>,
    email_verified: Option<bool>,
    name: Option<String>,
    picture: Option<String>,
    hd: Option<String>,
}

fn parse_oidc_id_token_claims(id_token: &str) -> Result<OidcIdTokenClaims, String> {
    let payload = id_token
        .split('.')
        .nth(1)
        .ok_or_else(|| "oidc id_token is missing JWT payload segment".to_string())?;
    let decoded = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|error| format!("oidc id_token payload decode failed: {error}"))?;
    serde_json::from_slice::<OidcIdTokenClaims>(&decoded)
        .map_err(|error| format!("oidc id_token payload JSON decode failed: {error}"))
}

fn parse_access_token_expires_at(
    value: &serde_json::Value,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, String> {
    let Some(raw) = value.as_str() else {
        return Ok(None);
    };
    let parsed = chrono::DateTime::parse_from_rfc3339(raw)
        .map_err(|error| format!("access_token_expires_at is not valid RFC3339: {error}"))?;
    Ok(Some(parsed.with_timezone(&chrono::Utc)))
}

#[derive(Debug, Deserialize)]
struct StandardOAuthTokenResponse {
    access_token: Option<String>,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    scope: Option<String>,
    token_type: Option<String>,
    id_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

async fn exchange_standard_oauth_access_token(
    provider: &ProviderDefinition,
    credential_payload: &serde_json::Value,
) -> Result<ProviderAccessTokenExchangeResult, String> {
    if provider.adapter_key.as_deref() != Some("oauth_openid") {
        return Err("oauth delegated token exchange requires adapter_key=oauth_openid".to_string());
    }

    let object = match credential_payload.as_object() {
        Some(object) => object,
        None => return Err("backing credential plaintext must be a JSON object".to_string()),
    };

    if let Some(access_token) = object
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        let cached_expires_at = object
            .get("access_token_expires_at")
            .map(parse_access_token_expires_at)
            .transpose()?
            .flatten();
        let still_valid = cached_expires_at
            .map(|expires_at| expires_at > chrono::Utc::now() + chrono::Duration::seconds(30))
            .unwrap_or(true);
        if still_valid {
            return Ok(ProviderAccessTokenExchangeResult {
                access_token: access_token.to_string(),
                probe_detail: "google delegated access token is present in backing credential"
                    .to_string(),
            });
        }
    }

    let refresh_token = object
        .get("refreshToken")
        .and_then(serde_json::Value::as_str)
        .or(object
            .get("refresh_token")
            .and_then(serde_json::Value::as_str))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "oauth delegated credential is missing refresh token".to_string())?;
    let client_id = object
        .get("client_id")
        .and_then(serde_json::Value::as_str)
        .or(provider.client_id.as_deref())
        .ok_or_else(|| "oauth delegated credential is missing client_id".to_string())?;
    let client_secret = object
        .get("client_secret")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "oauth delegated credential is missing client_secret".to_string())?;
    let token_uri = object
        .get("token_uri")
        .and_then(serde_json::Value::as_str)
        .or(provider.token_endpoint.as_deref())
        .ok_or_else(|| "oauth delegated credential is missing token_uri".to_string())?;

    let response = reqwest::Client::new()
        .post(token_uri)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id),
            ("client_secret", client_secret),
        ])
        .send()
        .await
        .map_err(|error| error.to_string())?;

    let status = response.status();
    let raw_body = response.text().await.map_err(|error| error.to_string())?;
    let payload = serde_json::from_str::<StandardOAuthTokenResponse>(&raw_body)
        .map_err(|error| format!("oauth token response decode failed: {error}; body={raw_body}"))?;

    if !status.is_success() {
        if let Some(error) = payload.error.clone() {
            return Err(payload.error_description.clone().unwrap_or_else(|| {
                format!("oauth token exchange failed: {error}; status={status}")
            }));
        }
        return Err(format!(
            "oauth token endpoint returned HTTP {status}; body={raw_body}"
        ));
    }

    if let Some(error) = payload.error {
        return Err(payload
            .error_description
            .unwrap_or_else(|| format!("oauth token exchange failed: {error}")));
    }

    let access_token = payload
        .access_token
        .ok_or_else(|| "google token response did not include access_token".to_string())?;

    Ok(ProviderAccessTokenExchangeResult {
        access_token,
        probe_detail: "oauth delegated token refresh succeeded".to_string(),
    })
}

async fn exchange_lark_access_token(
    provider: &ProviderDefinition,
    credential_payload: &serde_json::Value,
) -> Result<ProviderAccessTokenExchangeResult, String> {
    if provider.adapter_key.as_deref() != Some("lark_openapi") {
        return Err("binding validation currently requires adapter_key=lark_openapi".to_string());
    }

    let Some(token_endpoint) = provider.token_endpoint.as_deref() else {
        return Err("provider token_endpoint is required".to_string());
    };

    let object = match credential_payload.as_object() {
        Some(object) => object,
        None => return Err("backing credential plaintext must be a JSON object".to_string()),
    };

    if let Some(access_token) = object
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        return Ok(ProviderAccessTokenExchangeResult {
            access_token: access_token.to_string(),
            probe_detail: "lark delegated access token is present in backing credential"
                .to_string(),
        });
    }

    let app_id = object
        .get("app_id")
        .and_then(serde_json::Value::as_str)
        .or(provider.client_id.as_deref());
    let app_secret = object.get("app_secret").and_then(serde_json::Value::as_str);

    let (Some(app_id), Some(app_secret)) = (app_id, app_secret) else {
        return Err(
            "backing credential must provide app_id/app_secret (client_id may also come from provider definition)"
                .to_string(),
        );
    };

    let response = match reqwest::Client::new()
        .post(token_endpoint)
        .json(&json!({
            "app_id": app_id,
            "app_secret": app_secret,
        }))
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => return Err(error.to_string()),
    };

    let status = response.status();
    let payload = match response.json::<LarkTokenProbeResponse>().await {
        Ok(payload) => payload,
        Err(error) => return Err(error.to_string()),
    };

    if !status.is_success() {
        return Err(format!("token endpoint returned HTTP {status}"));
    }

    if payload.code != 0 {
        return Err(payload
            .msg
            .unwrap_or_else(|| format!("lark token exchange failed with code {}", payload.code)));
    }

    let token_mode = provider
        .runtime_config
        .get("token_mode")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("tenant_access_token");

    let access_token = match token_mode {
        "app_access_token" => payload.app_access_token,
        _ => payload.tenant_access_token,
    }
    .ok_or_else(|| format!("lark token response did not include expected {token_mode}"))?;

    Ok(ProviderAccessTokenExchangeResult {
        access_token,
        probe_detail: "lark app/tenant token probe succeeded".to_string(),
    })
}

#[derive(Debug, Deserialize)]
struct LarkDelegatedAccessTokenEnvelope {
    code: i32,
    msg: Option<String>,
    access_token: Option<String>,
    refresh_token: Option<String>,
    open_id: Option<String>,
    union_id: Option<String>,
    expires_in: Option<u64>,
    data: Option<LarkDelegatedAccessTokenData>,
}

#[derive(Debug, Deserialize)]
struct LarkDelegatedAccessTokenData {
    access_token: String,
    refresh_token: Option<String>,
    open_id: Option<String>,
    union_id: Option<String>,
    expires_in: Option<u64>,
}

fn parse_lark_delegated_access_token_envelope(
    payload: LarkDelegatedAccessTokenEnvelope,
) -> Result<LarkDelegatedAccessTokenData, String> {
    if payload.code != 0 {
        return Err(payload.msg.unwrap_or_else(|| {
            format!("delegated token exchange failed with code {}", payload.code)
        }));
    }

    if let Some(data) = payload.data {
        return Ok(data);
    }

    let access_token = payload
        .access_token
        .ok_or_else(|| "delegated token response missing data".to_string())?;

    Ok(LarkDelegatedAccessTokenData {
        access_token,
        refresh_token: payload.refresh_token,
        open_id: payload.open_id,
        union_id: payload.union_id,
        expires_in: payload.expires_in,
    })
}

async fn exchange_delegated_binding_payload(
    provider: &ProviderDefinition,
    state: &OAuthBrokerApiState,
    transaction: &AuthTransaction,
    code: &str,
) -> Result<DelegatedBindingExchangeResult, String> {
    match provider.adapter_key.as_deref() {
        Some("lark_openapi") => {
            exchange_lark_delegated_binding_payload(provider, state, transaction, code).await
        }
        Some("oauth_openid") => {
            exchange_standard_oauth_binding_payload(provider, state, transaction, code).await
        }
        Some(other) => Err(format!(
            "delegated callback currently does not support adapter_key={other}"
        )),
        None => Err("provider adapter_key is required for delegated callback".to_string()),
    }
}

async fn exchange_lark_delegated_binding_payload(
    provider: &ProviderDefinition,
    state: &OAuthBrokerApiState,
    transaction: &AuthTransaction,
    code: &str,
) -> Result<DelegatedBindingExchangeResult, String> {
    let token_endpoint = provider
        .token_endpoint
        .as_deref()
        .ok_or_else(|| "provider token_endpoint is required".to_string())?;
    let client_id = provider
        .client_id
        .as_deref()
        .ok_or_else(|| "provider client_id is required".to_string())?;
    let backing_payload = load_transaction_backing_credential_payload(state, transaction).await?;
    let token_exchange = provider
        .runtime_config
        .get("token_exchange")
        .and_then(serde_json::Value::as_object);
    let credential_client_id_field = token_exchange
        .and_then(|config| config.get("credential_client_id_field"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("client_id");
    let credential_client_secret_field = token_exchange
        .and_then(|config| config.get("credential_client_secret_field"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("client_secret");
    let request_client_id_field = token_exchange
        .and_then(|config| config.get("request_client_id_field"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("client_id");
    let request_client_secret_field = token_exchange
        .and_then(|config| config.get("request_client_secret_field"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("client_secret");
    let resolved_client_id = backing_payload
        .get(credential_client_id_field)
        .and_then(serde_json::Value::as_str)
        .or(Some(client_id))
        .ok_or_else(|| {
            format!("transaction backing credential is missing {credential_client_id_field}")
        })?;
    let resolved_client_secret = backing_payload
        .get(credential_client_secret_field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            format!("transaction backing credential is missing {credential_client_secret_field}")
        })?;
    let mut body = serde_json::Map::new();
    body.insert(
        request_client_id_field.to_string(),
        serde_json::Value::String(resolved_client_id.to_string()),
    );
    body.insert(
        request_client_secret_field.to_string(),
        serde_json::Value::String(resolved_client_secret.to_string()),
    );
    body.insert(
        "grant_type".to_string(),
        serde_json::Value::String("authorization_code".to_string()),
    );
    body.insert(
        "code".to_string(),
        serde_json::Value::String(code.to_string()),
    );
    body.insert(
        "redirect_uri".to_string(),
        serde_json::Value::String(transaction.redirect_uri.clone()),
    );
    body.insert(
        "code_verifier".to_string(),
        serde_json::Value::String(transaction.pkce_code_verifier.clone()),
    );

    let response = reqwest::Client::new()
        .post(token_endpoint)
        .json(&serde_json::Value::Object(body))
        .send()
        .await
        .map_err(|error| error.to_string())?;

    let status = response.status();
    let raw_body = response.text().await.map_err(|error| error.to_string())?;
    let payload =
        serde_json::from_str::<LarkDelegatedAccessTokenEnvelope>(&raw_body).map_err(|error| {
            format!("delegated token response decode failed: {error}; body={raw_body}")
        })?;

    if !status.is_success() {
        return Err(format!(
            "delegated token endpoint returned HTTP {status}; body={raw_body}"
        ));
    }
    let data = parse_lark_delegated_access_token_envelope(payload)
        .map_err(|error| format!("{error}; body={raw_body}"))?;
    let subject_ref = data
        .open_id
        .clone()
        .or(data.union_id.clone())
        .or(transaction.subject_ref_hint.clone())
        .ok_or_else(|| "delegated token response missing stable subject identifier".to_string())?;

    let credential_payload = json!({
        "access_token": data.access_token,
        "refreshToken": data.refresh_token,
        "open_id": data.open_id,
        "union_id": data.union_id,
        "expires_in": data.expires_in,
    });

    Ok(DelegatedBindingExchangeResult {
        subject_ref,
        subject_display_name: transaction.subject_display_name.clone(),
        credential_payload,
    })
}

async fn exchange_standard_oauth_binding_payload(
    provider: &ProviderDefinition,
    state: &OAuthBrokerApiState,
    transaction: &AuthTransaction,
    code: &str,
) -> Result<DelegatedBindingExchangeResult, String> {
    let token_endpoint = provider
        .token_endpoint
        .as_deref()
        .ok_or_else(|| "provider token_endpoint is required".to_string())?;
    let client_id = provider
        .client_id
        .as_deref()
        .ok_or_else(|| "provider client_id is required".to_string())?;
    let client_secret = load_transaction_client_secret(state, transaction).await?;

    let response = reqwest::Client::new()
        .post(token_endpoint)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", client_id),
            ("client_secret", client_secret.as_str()),
            ("redirect_uri", transaction.redirect_uri.as_str()),
            ("code_verifier", transaction.pkce_code_verifier.as_str()),
        ])
        .send()
        .await
        .map_err(|error| error.to_string())?;

    let status = response.status();
    let raw_body = response.text().await.map_err(|error| error.to_string())?;
    let payload =
        serde_json::from_str::<StandardOAuthTokenResponse>(&raw_body).map_err(|error| {
            format!("delegated token response decode failed: {error}; body={raw_body}")
        })?;

    if !status.is_success() {
        return Err(format!(
            "delegated token endpoint returned HTTP {status}; body={raw_body}"
        ));
    }

    if let Some(error) = payload.error {
        return Err(payload
            .error_description
            .unwrap_or_else(|| format!("oauth delegated token exchange failed: {error}")));
    }

    let access_token = payload
        .access_token
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "oauth delegated token response missing access_token".to_string())?;
    let id_token_claims = payload
        .id_token
        .as_deref()
        .map(parse_oidc_id_token_claims)
        .transpose()?;
    let subject_ref = id_token_claims
        .as_ref()
        .and_then(|claims| claims.sub.clone())
        .or_else(|| {
            id_token_claims
                .as_ref()
                .and_then(|claims| claims.email.clone())
        })
        .or_else(|| transaction.subject_ref_hint.clone())
        .ok_or_else(|| {
            "oauth delegated token response missing stable subject identifier".to_string()
        })?;
    let subject_display_name = id_token_claims
        .as_ref()
        .and_then(|claims| claims.name.clone())
        .or_else(|| {
            id_token_claims
                .as_ref()
                .and_then(|claims| claims.email.clone())
        })
        .or_else(|| transaction.subject_display_name.clone());

    let credential_payload = json!({
        "access_token": access_token,
        "access_token_expires_at": payload.expires_in.map(|seconds| {
            (chrono::Utc::now() + chrono::Duration::seconds(seconds as i64)).to_rfc3339()
        }),
        "refreshToken": payload.refresh_token,
        "expires_in": payload.expires_in,
        "scope": payload.scope,
        "token_type": payload.token_type,
        "id_token": payload.id_token,
        "client_id": client_id,
        "client_secret": client_secret,
        "token_uri": token_endpoint,
        "subject": {
            "sub": id_token_claims.as_ref().and_then(|claims| claims.sub.clone()),
            "email": id_token_claims.as_ref().and_then(|claims| claims.email.clone()),
            "email_verified": id_token_claims.as_ref().and_then(|claims| claims.email_verified),
            "name": id_token_claims.as_ref().and_then(|claims| claims.name.clone()),
            "picture": id_token_claims.as_ref().and_then(|claims| claims.picture.clone()),
            "hd": id_token_claims.as_ref().and_then(|claims| claims.hd.clone()),
        }
    });

    Ok(DelegatedBindingExchangeResult {
        subject_ref,
        subject_display_name,
        credential_payload,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        LarkDelegatedAccessTokenData, LarkDelegatedAccessTokenEnvelope,
        parse_lark_delegated_access_token_envelope,
    };

    #[test]
    fn parse_lark_delegated_access_token_supports_legacy_data_envelope() {
        let parsed = parse_lark_delegated_access_token_envelope(LarkDelegatedAccessTokenEnvelope {
            code: 0,
            msg: Some("ok".to_string()),
            access_token: None,
            refresh_token: None,
            open_id: None,
            union_id: None,
            expires_in: None,
            data: Some(LarkDelegatedAccessTokenData {
                access_token: "uat_legacy".to_string(),
                refresh_token: Some("urt_legacy".to_string()),
                open_id: Some("ou_legacy".to_string()),
                union_id: None,
                expires_in: Some(7200),
            }),
        })
        .expect("legacy envelope should parse");

        assert_eq!(parsed.access_token, "uat_legacy");
        assert_eq!(parsed.refresh_token.as_deref(), Some("urt_legacy"));
        assert_eq!(parsed.open_id.as_deref(), Some("ou_legacy"));
    }

    #[test]
    fn parse_lark_delegated_access_token_supports_top_level_v2_fields() {
        let parsed = parse_lark_delegated_access_token_envelope(LarkDelegatedAccessTokenEnvelope {
            code: 0,
            msg: Some("success".to_string()),
            access_token: Some("uat_v2".to_string()),
            refresh_token: Some("urt_v2".to_string()),
            open_id: Some("ou_v2".to_string()),
            union_id: Some("on_v2".to_string()),
            expires_in: Some(7200),
            data: None,
        })
        .expect("top-level v2 envelope should parse");

        assert_eq!(parsed.access_token, "uat_v2");
        assert_eq!(parsed.refresh_token.as_deref(), Some("urt_v2"));
        assert_eq!(parsed.open_id.as_deref(), Some("ou_v2"));
        assert_eq!(parsed.union_id.as_deref(), Some("on_v2"));
    }
}

async fn load_transaction_backing_credential_payload(
    state: &OAuthBrokerApiState,
    transaction: &AuthTransaction,
) -> Result<serde_json::Value, String> {
    let backing_credential_id = transaction
        .backing_credential_id
        .as_ref()
        .ok_or_else(|| "auth transaction is missing backing_credential_id".to_string())?;
    let binding = Binding {
        id: Uuid::nil(),
        tenant_id: transaction.tenant_id,
        provider_definition_id: transaction.provider_definition_id,
        binding_handle: String::new(),
        alias: transaction.alias.clone(),
        purpose: transaction.purpose.clone(),
        binding_kind: BindingKind::DelegatedUser,
        subject_ref: transaction.subject_ref_hint.clone().unwrap_or_default(),
        subject_display_name: transaction.subject_display_name.clone(),
        backing_credential_id: Some(backing_credential_id.clone()),
        status: BindingStatus::Draft,
        health_status: None,
        last_error_code: None,
        last_error_message: None,
        cooldown_until: None,
        rebind_required: false,
        last_validated_at: None,
        created_by: transaction.created_by,
        created_at: transaction.created_at,
        updated_at: transaction.updated_at,
    };
    load_binding_secret_payload(state, &binding)
        .await
        .map_err(|error| error.to_string())
}

async fn load_transaction_client_secret(
    state: &OAuthBrokerApiState,
    transaction: &AuthTransaction,
) -> Result<String, String> {
    let payload = load_transaction_backing_credential_payload(state, transaction).await?;
    let secret = payload
        .get("app_secret")
        .and_then(serde_json::Value::as_str)
        .or(payload
            .get("client_secret")
            .and_then(serde_json::Value::as_str))
        .ok_or_else(|| "transaction backing credential is missing app_secret".to_string())?;
    Ok(secret.to_string())
}

#[derive(Debug, Serialize)]
struct GoogleServiceAccountAssertionClaims {
    iss: String,
    scope: String,
    aud: String,
    exp: usize,
    iat: usize,
}

#[derive(Debug, Deserialize)]
struct GoogleAccessTokenResponse {
    access_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

async fn exchange_google_service_account_access_token(
    provider: &ProviderDefinition,
    credential_payload: &serde_json::Value,
) -> Result<ProviderAccessTokenExchangeResult, String> {
    if provider.adapter_key.as_deref() != Some("google_service_account") {
        return Err(
            "google service account token exchange requires adapter_key=google_service_account"
                .to_string(),
        );
    }

    if provider.scope_template.is_empty() {
        return Err("google service account binding requires at least one scope".to_string());
    }

    let object = match credential_payload.as_object() {
        Some(object) => object,
        None => return Err("backing credential plaintext must be a JSON object".to_string()),
    };

    let client_email = object
        .get("client_email")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "service account credential is missing client_email".to_string())?;
    let private_key = object
        .get("private_key")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "service account credential is missing private_key".to_string())?;
    let token_uri = object
        .get("token_uri")
        .and_then(serde_json::Value::as_str)
        .or(provider.token_endpoint.as_deref())
        .ok_or_else(|| "service account credential is missing token_uri".to_string())?;

    let now = chrono::Utc::now().timestamp() as usize;
    let claims = GoogleServiceAccountAssertionClaims {
        iss: client_email.to_string(),
        scope: provider.scope_template.join(" "),
        aud: token_uri.to_string(),
        iat: now,
        exp: now + 3600,
    };

    let mut header = Header::new(Algorithm::RS256);
    if let Some(key_id) = object
        .get("private_key_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        header.kid = Some(key_id.to_string());
    }
    let assertion = jsonwebtoken::encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_pem(private_key.as_bytes()).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    let response = reqwest::Client::new()
        .post(token_uri)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", assertion.as_str()),
        ])
        .send()
        .await
        .map_err(|error| error.to_string())?;

    let status = response.status();
    let payload = response
        .json::<GoogleAccessTokenResponse>()
        .await
        .map_err(|error| error.to_string())?;

    if !status.is_success() {
        return Err(format!("google token endpoint returned HTTP {status}"));
    }

    if let Some(error) = payload.error {
        return Err(payload
            .error_description
            .unwrap_or_else(|| format!("google token exchange failed: {error}")));
    }

    let access_token = payload
        .access_token
        .ok_or_else(|| "google token response did not include access_token".to_string())?;

    Ok(ProviderAccessTokenExchangeResult {
        access_token,
        probe_detail: "google service account token probe succeeded".to_string(),
    })
}

#[allow(clippy::result_large_err)]
fn authorize_runtime_request(
    policy: &crate::oauth_broker::BindingPolicySnapshot,
    method: &Method,
    url: &Url,
    _headers: &HashMap<String, String>,
) -> Result<(), ApiErrorResponse> {
    let host = url
        .host_str()
        .ok_or_else(|| ApiErrorResponse::invalid_request("request.url must include a host"))?;

    if !policy.allowed_domains.is_empty() && !policy.allowed_domains.iter().any(|item| item == host)
    {
        return Err(ApiErrorResponse::forbidden(format!(
            "request.url host is not allowed by binding policy: {host}"
        )));
    }

    if !policy.allowed_methods.is_empty()
        && !policy
            .allowed_methods
            .iter()
            .any(|item| item.eq_ignore_ascii_case(method.as_str()))
    {
        return Err(ApiErrorResponse::forbidden(format!(
            "request.method is not allowed by binding policy: {}",
            method.as_str()
        )));
    }

    if !policy.allowed_path_prefixes.is_empty()
        && !policy
            .allowed_path_prefixes
            .iter()
            .any(|prefix| url.path().starts_with(prefix))
    {
        return Err(ApiErrorResponse::forbidden(format!(
            "request.url path is not allowed by binding policy: {}",
            url.path()
        )));
    }

    Ok(())
}

fn redact_sensitive_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let redacted = map
                .into_iter()
                .map(|(key, value)| {
                    let normalized = key.to_ascii_lowercase();
                    let value = if is_sensitive_runtime_key(normalized.as_str()) {
                        serde_json::Value::String("[REDACTED]".to_string())
                    } else {
                        redact_sensitive_json(value)
                    };
                    (key, value)
                })
                .collect::<serde_json::Map<String, serde_json::Value>>();
            serde_json::Value::Object(redacted)
        }
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .into_iter()
                .map(redact_sensitive_json)
                .collect::<Vec<_>>(),
        ),
        other => other,
    }
}

fn is_sensitive_runtime_key(key: &str) -> bool {
    matches!(
        key,
        "access_token"
            | "tenant_access_token"
            | "app_access_token"
            | "refresh_token"
            | "app_secret"
            | "client_secret"
            | "authorization"
    )
}

fn targets_provider_token_endpoint(request_url: &str, token_endpoint: Option<&str>) -> bool {
    let Some(token_endpoint) = token_endpoint else {
        return false;
    };
    let (Ok(request), Ok(endpoint)) = (Url::parse(request_url), Url::parse(token_endpoint)) else {
        return false;
    };

    request.scheme() == endpoint.scheme()
        && request.host_str() == endpoint.host_str()
        && request.port_or_known_default() == endpoint.port_or_known_default()
        && request.path() == endpoint.path()
}

async fn store_binding_secret_payload(
    state: &OAuthBrokerApiState,
    tenant_id: Uuid,
    user_id: Uuid,
    alias: &str,
    plaintext: &serde_json::Value,
) -> Result<String, OAuthBrokerError> {
    let runtime = state.credential_runtime.as_ref().ok_or_else(|| {
        OAuthBrokerError::InternalError(
            "OAuth broker credential runtime is not configured".to_string(),
        )
    })?;

    let credential_id = CredentialId::new();
    let tenant = TenantId::new(tenant_id.to_string());
    let user = UserId::new(user_id.to_string());
    let plaintext_bytes = serde_json::to_vec(plaintext)
        .map_err(|error| OAuthBrokerError::SerializationError(error.to_string()))?;
    let context =
        CredentialCryptoContext::new(tenant.as_str(), user.hash(), credential_id.as_str());
    let blob = {
        let mut enclave = runtime.enclave.lock().await;
        if enclave.is_running() {
            enclave
                .encrypt_credential(
                    tenant.as_str(),
                    user.hash(),
                    credential_id.as_str(),
                    &plaintext_bytes,
                )
                .map_err(|error| OAuthBrokerError::InternalError(error.to_string()))?
        } else {
            let hierarchy = runtime.key_hierarchy.read().await;
            context
                .encrypt_with_hierarchy(&hierarchy, &plaintext_bytes)
                .map_err(|error| OAuthBrokerError::InternalError(error.to_string()))?
        }
    };

    let request = CreateCredentialRequest {
        tenant_id: tenant,
        user_id: user,
        service_id: ServiceId::new(format!("oauth_broker:{alias}")),
        credential_type: CredentialType::OAuthRefresh,
        expires_at: None,
        provider: None,
        allowed_domains: Vec::new(),
        custom_functions: Vec::new(),
    };

    runtime
        .vault
        .create_credential_with_id(
            request,
            crate::vault::models::EncryptedPayload::from_blob(&blob),
            credential_id.clone(),
        )
        .map_err(|error| OAuthBrokerError::InternalError(error.to_string()))?;

    Ok(credential_id.as_str().to_string())
}

#[allow(clippy::result_large_err)]
fn ensure_backing_credential_access(
    state: &OAuthBrokerApiState,
    tenant_id: Uuid,
    user_id: Uuid,
    backing_credential_id: Option<&str>,
    field_name: &str,
) -> Result<(), ApiErrorResponse> {
    let Some(backing_credential_id) = backing_credential_id.map(str::trim) else {
        return Ok(());
    };
    if backing_credential_id.is_empty() {
        return Err(ApiErrorResponse::invalid_request(format!(
            "{field_name} must not be empty"
        )));
    }

    let runtime = state.credential_runtime.as_ref().ok_or_else(|| {
        ApiErrorResponse::internal_error("OAuth broker credential runtime is not configured")
    })?;
    let credential_id = CredentialId::from_string(backing_credential_id.to_string())
        .map_err(|error| ApiErrorResponse::invalid_request(error.to_string()))?;
    let tenant_id = TenantId::new(tenant_id.to_string());
    let user_id = UserId::new(user_id.to_string());

    match runtime
        .vault
        .get_credential_metadata(&credential_id, &tenant_id, &user_id)
    {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(ApiErrorResponse::not_found(format!(
            "{field_name} was not found"
        ))),
        Err(error) => Err(ApiErrorResponse::forbidden(format!(
            "{field_name} is not accessible to the current user: {error}"
        ))),
    }
}

fn cleanup_stored_binding_secret(
    state: &OAuthBrokerApiState,
    tenant_id: Uuid,
    user_id: Uuid,
    backing_credential_id: &str,
) {
    let Some(runtime) = state.credential_runtime.as_ref() else {
        return;
    };
    let Ok(credential_id) = CredentialId::from_string(backing_credential_id.to_string()) else {
        return;
    };
    let tenant_id = TenantId::new(tenant_id.to_string());
    let user_id = UserId::new(user_id.to_string());

    if let Err(error) = runtime
        .vault
        .delete_credential(&credential_id, &tenant_id, &user_id)
    {
        tracing::warn!(
            "[OAUTH_BROKER] failed to cleanup stored binding secret {}: {}",
            credential_id.as_str(),
            error
        );
    }
}

#[allow(clippy::result_large_err)]
fn require_broker_admin(token: &ValidatedToken) -> Result<(), ApiErrorResponse> {
    if !token.has_any_scope(&[TokenScope::TenantAdmin, TokenScope::Admin]) {
        return Err(ApiErrorResponse::forbidden(
            "Broker management requires tenant admin scope",
        ));
    }
    Ok(())
}

fn map_auth_transaction(transaction: AuthTransaction) -> AuthTransactionResponse {
    AuthTransactionResponse {
        transaction_id: transaction.id.to_string(),
        provider_definition_id: transaction.provider_definition_id.to_string(),
        alias: transaction.alias,
        purpose: transaction.purpose,
        subject_ref_hint: transaction.subject_ref_hint,
        subject_display_name: transaction.subject_display_name,
        state: transaction.state,
        code_challenge_method: transaction.code_challenge_method,
        authorization_url: transaction.authorization_url,
        redirect_uri: transaction.redirect_uri,
        requested_scopes: transaction.requested_scopes,
        status: transaction.status.as_str().to_string(),
        expires_at: transaction.expires_at.to_rfc3339(),
        created_at: transaction.created_at.to_rfc3339(),
        consumed_at: transaction.consumed_at.map(|value| value.to_rfc3339()),
    }
}

#[allow(clippy::result_large_err)]
fn parse_uuid_str(value: &str, field_name: &str) -> Result<Uuid, ApiErrorResponse> {
    Uuid::parse_str(value)
        .map_err(|_| ApiErrorResponse::invalid_request(format!("{field_name} must be a UUID")))
}

fn map_oauth_broker_error(error: OAuthBrokerError) -> ApiErrorResponse {
    match error {
        OAuthBrokerError::ProviderDefinitionNotFound(_) | OAuthBrokerError::BindingNotFound(_) => {
            ApiErrorResponse::not_found(error.to_string())
        }
        OAuthBrokerError::ProviderDefinitionAlreadyExists { .. }
        | OAuthBrokerError::BindingAlreadyExists { .. } => {
            ApiErrorResponse::conflict(error.to_string())
        }
        OAuthBrokerError::InvalidRequest(_) => ApiErrorResponse::invalid_request(error.to_string()),
        OAuthBrokerError::DatabaseError(_)
        | OAuthBrokerError::SerializationError(_)
        | OAuthBrokerError::InternalError(_) => ApiErrorResponse::internal_error(error.to_string()),
    }
}
