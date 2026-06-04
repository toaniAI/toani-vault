use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use reqwest::Url;
use ring::rand::SecureRandom;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::{FromRow, PgPool, Postgres};
use std::str::FromStr;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingKind {
    DelegatedUser,
    OAuthService,
    ProviderAppCredential,
}

impl BindingKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DelegatedUser => "delegated_user",
            Self::OAuthService => "oauth_service",
            Self::ProviderAppCredential => "provider_app_credential",
        }
    }
}

impl std::fmt::Display for BindingKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for BindingKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "delegated_user" => Ok(Self::DelegatedUser),
            "oauth_service" => Ok(Self::OAuthService),
            "provider_app_credential" => Ok(Self::ProviderAppCredential),
            _ => Err(format!("Unknown binding kind: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantFamily {
    AuthorizationCodePkce,
    ClientCredentials,
    None,
}

impl GrantFamily {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AuthorizationCodePkce => "authorization_code_pkce",
            Self::ClientCredentials => "client_credentials",
            Self::None => "none",
        }
    }
}

impl std::fmt::Display for GrantFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for GrantFamily {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "authorization_code_pkce" => Ok(Self::AuthorizationCodePkce),
            "client_credentials" => Ok(Self::ClientCredentials),
            "none" => Ok(Self::None),
            _ => Err(format!("Unknown grant family: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProviderDefinitionStatus {
    #[default]
    Active,
    Inactive,
}

impl ProviderDefinitionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Inactive => "inactive",
        }
    }
}

impl std::fmt::Display for ProviderDefinitionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ProviderDefinitionStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(Self::Active),
            "inactive" => Ok(Self::Inactive),
            _ => Err(format!("Unknown provider definition status: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProviderDefinitionInput {
    pub tenant_id: Uuid,
    pub created_by: Uuid,
    pub alias: String,
    pub display_name: String,
    pub provider_family: String,
    pub binding_kind: BindingKind,
    pub grant_family: GrantFamily,
    pub authorization_endpoint: Option<String>,
    pub token_endpoint: Option<String>,
    pub client_id: Option<String>,
    pub client_auth_method: Option<String>,
    pub callback_mode: Option<String>,
    pub adapter_key: Option<String>,
    pub adapter_version: Option<String>,
    pub scope_template: Vec<String>,
    pub runtime_config: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateProviderDefinitionInput {
    pub updated_by: Uuid,
    pub display_name: Option<String>,
    pub authorization_endpoint: Option<String>,
    pub token_endpoint: Option<String>,
    pub client_id: Option<String>,
    pub client_auth_method: Option<String>,
    pub callback_mode: Option<String>,
    pub adapter_key: Option<String>,
    pub adapter_version: Option<String>,
    pub scope_template: Option<Vec<String>>,
    pub runtime_config: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProviderDefinition {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub alias: String,
    pub display_name: String,
    pub version: i32,
    pub provider_family: String,
    pub binding_kind: BindingKind,
    pub grant_family: GrantFamily,
    pub authorization_endpoint: Option<String>,
    pub token_endpoint: Option<String>,
    pub client_id: Option<String>,
    pub client_auth_method: Option<String>,
    pub callback_mode: Option<String>,
    pub adapter_key: Option<String>,
    pub adapter_version: Option<String>,
    pub scope_template: Vec<String>,
    pub runtime_config: JsonValue,
    pub status: ProviderDefinitionStatus,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProviderDefinition {
    pub fn new(
        id: Uuid,
        tenant_id: Uuid,
        alias: String,
        display_name: String,
        provider_family: String,
        binding_kind: BindingKind,
        grant_family: GrantFamily,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            tenant_id,
            alias,
            display_name,
            version: 1,
            provider_family,
            binding_kind,
            grant_family,
            authorization_endpoint: None,
            token_endpoint: None,
            client_id: None,
            client_auth_method: None,
            callback_mode: None,
            adapter_key: None,
            adapter_version: None,
            scope_template: Vec::new(),
            runtime_config: JsonValue::Object(Default::default()),
            status: ProviderDefinitionStatus::Active,
            created_by: Uuid::nil(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn from_input(input: CreateProviderDefinitionInput) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::now_v7(),
            tenant_id: input.tenant_id,
            alias: input.alias,
            display_name: input.display_name,
            version: 1,
            provider_family: input.provider_family,
            binding_kind: input.binding_kind,
            grant_family: input.grant_family,
            authorization_endpoint: input.authorization_endpoint,
            token_endpoint: input.token_endpoint,
            client_id: input.client_id,
            client_auth_method: input.client_auth_method,
            callback_mode: input.callback_mode,
            adapter_key: input.adapter_key,
            adapter_version: input.adapter_version,
            scope_template: input.scope_template,
            runtime_config: input.runtime_config,
            status: ProviderDefinitionStatus::Active,
            created_by: input.created_by,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingStatus {
    Draft,
    Ready,
    Revoked,
}

impl BindingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Ready => "ready",
            Self::Revoked => "revoked",
        }
    }
}

impl std::fmt::Display for BindingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for BindingStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "draft" => Ok(Self::Draft),
            "ready" => Ok(Self::Ready),
            "revoked" => Ok(Self::Revoked),
            _ => Err(format!("Unknown binding status: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBindingInput {
    pub tenant_id: Uuid,
    pub created_by: Uuid,
    pub provider_definition_id: Uuid,
    pub alias: String,
    pub purpose: String,
    pub subject_ref: String,
    pub subject_display_name: Option<String>,
    pub backing_credential_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Binding {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub provider_definition_id: Uuid,
    pub binding_handle: String,
    pub alias: String,
    pub purpose: String,
    pub binding_kind: BindingKind,
    pub subject_ref: String,
    pub subject_display_name: Option<String>,
    pub backing_credential_id: Option<String>,
    pub status: BindingStatus,
    pub health_status: Option<String>,
    pub last_error_code: Option<String>,
    pub last_error_message: Option<String>,
    pub cooldown_until: Option<DateTime<Utc>>,
    pub rebind_required: bool,
    pub last_validated_at: Option<DateTime<Utc>>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BindingPolicySnapshot {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub binding_id: Uuid,
    pub version: i32,
    pub granted_scopes: Vec<String>,
    pub effective_scopes: Vec<String>,
    pub allowed_domains: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub allowed_path_prefixes: Vec<String>,
    pub validation_status: String,
    pub last_validation_error_code: Option<String>,
    pub last_validation_error_message: Option<String>,
    pub validated_at: Option<DateTime<Utc>>,
    pub applied_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageBindingPolicyInput {
    pub granted_scopes: Vec<String>,
    pub effective_scopes: Vec<String>,
    pub allowed_domains: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub allowed_path_prefixes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BindingRuntimeStateUpdate {
    pub binding_status: Option<BindingStatus>,
    pub health_status: String,
    pub last_error_code: Option<String>,
    pub last_error_message: Option<String>,
    pub cooldown_until: Option<DateTime<Utc>>,
    pub rebind_required: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum OAuthBrokerError {
    #[error("provider definition not found: {0}")]
    ProviderDefinitionNotFound(Uuid),

    #[error("binding not found: {0}")]
    BindingNotFound(Uuid),

    #[error("provider definition already exists: tenant={tenant_id}, alias={alias}")]
    ProviderDefinitionAlreadyExists { tenant_id: Uuid, alias: String },

    #[error("binding already exists: tenant={tenant_id}, alias={alias}")]
    BindingAlreadyExists { tenant_id: Uuid, alias: String },

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("database error: {0}")]
    DatabaseError(String),

    #[error("serialization error: {0}")]
    SerializationError(String),

    #[error("internal error: {0}")]
    InternalError(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderValidationCheckStatus {
    Passed,
    Failed,
}

impl ProviderValidationCheckStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderValidationCheck {
    pub name: String,
    pub status: ProviderValidationCheckStatus,
    pub detail: String,
}

impl ProviderValidationCheck {
    pub fn passed(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: ProviderValidationCheckStatus::Passed,
            detail: detail.into(),
        }
    }

    pub fn failed(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: ProviderValidationCheckStatus::Failed,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderValidationResult {
    pub provider_definition_id: Uuid,
    pub valid: bool,
    pub checks: Vec<ProviderValidationCheck>,
    pub validated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingValidationResult {
    pub binding_id: Uuid,
    pub binding_handle: String,
    pub valid: bool,
    pub status: String,
    pub health_status: String,
    pub checks: Vec<ProviderValidationCheck>,
    pub validated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthTransactionStatus {
    Pending,
    Consumed,
    Expired,
    Failed,
}

impl AuthTransactionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Consumed => "consumed",
            Self::Expired => "expired",
            Self::Failed => "failed",
        }
    }
}

impl std::fmt::Display for AuthTransactionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AuthTransactionStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "consumed" => Ok(Self::Consumed),
            "expired" => Ok(Self::Expired),
            "failed" => Ok(Self::Failed),
            _ => Err(format!("Unknown auth transaction status: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartAuthTransactionInput {
    pub tenant_id: Uuid,
    pub created_by: Uuid,
    pub provider_definition_id: Uuid,
    pub alias: String,
    pub purpose: String,
    pub subject_ref_hint: Option<String>,
    pub subject_display_name: Option<String>,
    pub backing_credential_id: Option<String>,
    pub requested_scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AuthTransaction {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub provider_definition_id: Uuid,
    pub created_by: Uuid,
    pub alias: String,
    pub purpose: String,
    pub subject_ref_hint: Option<String>,
    pub subject_display_name: Option<String>,
    pub backing_credential_id: Option<String>,
    pub state: String,
    pub nonce: String,
    pub pkce_code_verifier: String,
    pub pkce_code_challenge: String,
    pub code_challenge_method: String,
    pub redirect_uri: String,
    pub requested_scopes: Vec<String>,
    pub status: AuthTransactionStatus,
    pub authorization_url: String,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallbackCoreGateFailure {
    TransactionMissing,
    StateMismatch,
    ReplayDetected,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallbackCoreGate {
    Passed,
    Failed(CallbackCoreGateFailure),
}

#[derive(Debug, Error)]
pub enum CallbackAdapterInvocationError {
    #[error("core callback gate failed: {0:?}")]
    CoreGateFailed(CallbackCoreGateFailure),

    #[error("adapter binding is not supported: key={key}, version={version}")]
    UnsupportedAdapterBinding { key: String, version: String },

    #[error("adapter execution failed: {0}")]
    AdapterExecutionFailed(String),
}

pub trait CallbackAdapterExecutor {
    fn execute(
        &self,
        payload: &serde_json::Value,
    ) -> Result<serde_json::Value, CallbackAdapterInvocationError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackAdapterDefinition {
    pub key: String,
    pub version: String,
    pub grant_family: GrantFamily,
}

impl CallbackAdapterDefinition {
    pub fn new(
        key: impl Into<String>,
        version: impl Into<String>,
        grant_family: GrantFamily,
    ) -> Self {
        Self {
            key: key.into(),
            version: version.into(),
            grant_family,
        }
    }

    pub fn execute_after_core_gate(
        &self,
        gate: CallbackCoreGate,
        adapter: &dyn CallbackAdapterExecutor,
        payload: &serde_json::Value,
    ) -> Result<serde_json::Value, CallbackAdapterInvocationError> {
        match gate {
            CallbackCoreGate::Passed => {}
            CallbackCoreGate::Failed(reason) => {
                return Err(CallbackAdapterInvocationError::CoreGateFailed(reason));
            }
        }

        if !adapter_binding_is_supported(Some(self.key.as_str()), Some(self.version.as_str())) {
            return Err(CallbackAdapterInvocationError::UnsupportedAdapterBinding {
                key: self.key.clone(),
                version: self.version.clone(),
            });
        }

        adapter.execute(payload)
    }
}

#[async_trait]
pub trait OAuthBrokerService: Send + Sync {
    async fn create_provider_definition(
        &self,
        input: CreateProviderDefinitionInput,
    ) -> Result<ProviderDefinition, OAuthBrokerError>;

    async fn list_provider_definitions(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ProviderDefinition>, OAuthBrokerError>;

    async fn get_provider_definition(
        &self,
        provider_definition_id: Uuid,
    ) -> Result<Option<ProviderDefinition>, OAuthBrokerError>;

    async fn update_provider_definition(
        &self,
        tenant_id: Uuid,
        provider_definition_id: Uuid,
        input: UpdateProviderDefinitionInput,
    ) -> Result<ProviderDefinition, OAuthBrokerError>;

    async fn validate_provider_definition(
        &self,
        tenant_id: Uuid,
        provider_definition_id: Uuid,
    ) -> Result<ProviderValidationResult, OAuthBrokerError>;

    async fn create_binding(&self, input: CreateBindingInput) -> Result<Binding, OAuthBrokerError>;

    async fn list_bindings(&self, tenant_id: Uuid) -> Result<Vec<Binding>, OAuthBrokerError>;

    async fn get_binding(&self, binding_id: Uuid) -> Result<Option<Binding>, OAuthBrokerError>;

    async fn get_binding_by_handle(
        &self,
        tenant_id: Uuid,
        binding_handle: &str,
    ) -> Result<Option<Binding>, OAuthBrokerError>;

    async fn get_latest_binding_policy_snapshot(
        &self,
        binding_id: Uuid,
    ) -> Result<Option<BindingPolicySnapshot>, OAuthBrokerError>;

    async fn stage_binding_policy(
        &self,
        binding_id: Uuid,
        input: StageBindingPolicyInput,
        staged_at: DateTime<Utc>,
    ) -> Result<BindingPolicySnapshot, OAuthBrokerError> {
        let _ = (binding_id, input, staged_at);
        Err(OAuthBrokerError::InvalidRequest(
            "binding policy staging is not implemented".to_string(),
        ))
    }

    async fn get_staged_binding_policy(
        &self,
        binding_id: Uuid,
    ) -> Result<Option<BindingPolicySnapshot>, OAuthBrokerError> {
        let _ = binding_id;
        Err(OAuthBrokerError::InvalidRequest(
            "binding policy staging is not implemented".to_string(),
        ))
    }

    async fn set_staged_binding_policy_validation(
        &self,
        binding_id: Uuid,
        validation_status: &str,
        last_validation_error_code: Option<&str>,
        last_validation_error_message: Option<&str>,
        validated_at: DateTime<Utc>,
    ) -> Result<BindingPolicySnapshot, OAuthBrokerError> {
        let _ = (
            binding_id,
            validation_status,
            last_validation_error_code,
            last_validation_error_message,
            validated_at,
        );
        Err(OAuthBrokerError::InvalidRequest(
            "binding policy staging is not implemented".to_string(),
        ))
    }

    async fn apply_staged_binding_policy(
        &self,
        binding_id: Uuid,
        applied_at: DateTime<Utc>,
    ) -> Result<BindingPolicySnapshot, OAuthBrokerError> {
        let _ = (binding_id, applied_at);
        Err(OAuthBrokerError::InvalidRequest(
            "binding policy staging is not implemented".to_string(),
        ))
    }

    async fn set_binding_validation_state(
        &self,
        binding_id: Uuid,
        binding_status: BindingStatus,
        health_status: &str,
        last_error_code: Option<&str>,
        last_error_message: Option<&str>,
        validated_at: DateTime<Utc>,
    ) -> Result<Binding, OAuthBrokerError>;

    async fn set_binding_runtime_state(
        &self,
        binding_id: Uuid,
        update: BindingRuntimeStateUpdate,
    ) -> Result<Binding, OAuthBrokerError> {
        let _ = (binding_id, update);
        Err(OAuthBrokerError::InvalidRequest(
            "binding runtime state updates are not implemented".to_string(),
        ))
    }

    async fn revoke_binding(
        &self,
        binding_id: Uuid,
        revoked_at: DateTime<Utc>,
    ) -> Result<Binding, OAuthBrokerError> {
        let _ = (binding_id, revoked_at);
        Err(OAuthBrokerError::InvalidRequest(
            "binding revoke is not implemented".to_string(),
        ))
    }

    async fn delete_binding(&self, binding_id: Uuid) -> Result<(), OAuthBrokerError> {
        let _ = binding_id;
        Err(OAuthBrokerError::InvalidRequest(
            "binding delete is not implemented".to_string(),
        ))
    }

    async fn start_auth_transaction(
        &self,
        input: StartAuthTransactionInput,
    ) -> Result<AuthTransaction, OAuthBrokerError>;

    async fn get_auth_transaction(
        &self,
        transaction_id: Uuid,
    ) -> Result<Option<AuthTransaction>, OAuthBrokerError>;

    async fn get_auth_transaction_by_state(
        &self,
        state: &str,
    ) -> Result<Option<AuthTransaction>, OAuthBrokerError>;

    async fn set_auth_transaction_status(
        &self,
        transaction_id: Uuid,
        status: AuthTransactionStatus,
        consumed_at: Option<DateTime<Utc>>,
    ) -> Result<AuthTransaction, OAuthBrokerError>;
}

#[derive(Clone)]
pub struct PgOAuthBrokerService {
    pool: PgPool,
}

impl PgOAuthBrokerService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn lookup_provider_definition(
        &self,
        provider_definition_id: Uuid,
    ) -> Result<ProviderDefinition, OAuthBrokerError> {
        self.get_provider_definition(provider_definition_id)
            .await?
            .ok_or(OAuthBrokerError::ProviderDefinitionNotFound(
                provider_definition_id,
            ))
    }

    fn build_auth_transaction(
        provider: &ProviderDefinition,
        input: StartAuthTransactionInput,
    ) -> Result<AuthTransaction, OAuthBrokerError> {
        if provider.binding_kind != BindingKind::DelegatedUser {
            return Err(OAuthBrokerError::InvalidRequest(
                "auth transactions require delegated_user binding kind".to_string(),
            ));
        }
        if provider.grant_family != GrantFamily::AuthorizationCodePkce {
            return Err(OAuthBrokerError::InvalidRequest(
                "auth transactions require authorization_code_pkce grant family".to_string(),
            ));
        }

        let authorization_endpoint =
            provider.authorization_endpoint.as_deref().ok_or_else(|| {
                OAuthBrokerError::InvalidRequest(
                    "provider definition is missing authorization_endpoint".to_string(),
                )
            })?;
        let redirect_uri = provider
            .runtime_config
            .get("redirect_uri")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| {
                OAuthBrokerError::InvalidRequest(
                    "provider definition runtime_config.redirect_uri is required".to_string(),
                )
            })?;

        let state = random_urlsafe_token(32);
        let nonce = random_urlsafe_token(32);
        let pkce_code_verifier = random_urlsafe_token(48);
        let pkce_code_challenge = pkce_s256_challenge(&pkce_code_verifier);
        let ttl_minutes = provider
            .runtime_config
            .get("transaction_ttl_minutes")
            .and_then(JsonValue::as_i64)
            .filter(|value| *value > 0)
            .unwrap_or(10);
        let expires_at = Utc::now() + chrono::Duration::minutes(ttl_minutes);
        let requested_scopes = if input.requested_scopes.is_empty() {
            provider.scope_template.clone()
        } else {
            input.requested_scopes.clone()
        };
        let authorization_url = build_authorization_url(AuthorizationUrlInput {
            authorization_endpoint,
            client_id: provider.client_id.as_deref(),
            redirect_uri,
            state: &state,
            nonce: &nonce,
            code_challenge: &pkce_code_challenge,
            scopes: &requested_scopes,
            extra_query: provider.runtime_config.get("authorization_query"),
        })?;

        Ok(AuthTransaction {
            id: Uuid::now_v7(),
            tenant_id: input.tenant_id,
            provider_definition_id: input.provider_definition_id,
            created_by: input.created_by,
            alias: input.alias,
            purpose: input.purpose,
            subject_ref_hint: input.subject_ref_hint,
            subject_display_name: input.subject_display_name,
            backing_credential_id: input.backing_credential_id,
            state,
            nonce,
            pkce_code_verifier,
            pkce_code_challenge,
            code_challenge_method: "S256".to_string(),
            redirect_uri: redirect_uri.to_string(),
            requested_scopes,
            status: AuthTransactionStatus::Pending,
            authorization_url,
            expires_at,
            consumed_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }
}

#[async_trait]
impl OAuthBrokerService for PgOAuthBrokerService {
    async fn create_provider_definition(
        &self,
        input: CreateProviderDefinitionInput,
    ) -> Result<ProviderDefinition, OAuthBrokerError> {
        let scope_template = serde_json::to_value(&input.scope_template)
            .map_err(|error| OAuthBrokerError::SerializationError(error.to_string()))?;

        let row = sqlx::query_as::<_, ProviderDefinition>(
            r#"
            INSERT INTO oauth_provider_definitions (
                id, tenant_id, alias, display_name, version, provider_family, binding_kind, grant_family,
                authorization_endpoint, token_endpoint, client_id, client_auth_method, callback_mode, adapter_key,
                adapter_version, scope_template, runtime_config, status, created_by, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21)
            RETURNING id, tenant_id, alias, display_name, version, provider_family, binding_kind, grant_family,
                      authorization_endpoint, token_endpoint, client_id, client_auth_method, callback_mode, adapter_key,
                      adapter_version, ARRAY(SELECT jsonb_array_elements_text(scope_template)) AS scope_template,
                      runtime_config, status, created_by, created_at, updated_at
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(input.tenant_id)
        .bind(&input.alias)
        .bind(&input.display_name)
        .bind(1_i32)
        .bind(&input.provider_family)
        .bind(input.binding_kind)
        .bind(input.grant_family)
        .bind(&input.authorization_endpoint)
        .bind(&input.token_endpoint)
        .bind(&input.client_id)
        .bind(&input.client_auth_method)
        .bind(&input.callback_mode)
        .bind(&input.adapter_key)
        .bind(&input.adapter_version)
        .bind(&scope_template)
        .bind(&input.runtime_config)
        .bind(ProviderDefinitionStatus::Active)
        .bind(input.created_by)
        .bind(Utc::now())
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await
        .map_err(|error| match error {
            sqlx::Error::Database(db_error)
                if db_error.constraint()
                    == Some("uq_oauth_provider_definitions_tenant_alias") =>
            {
                OAuthBrokerError::ProviderDefinitionAlreadyExists {
                    tenant_id: input.tenant_id,
                    alias: input.alias.clone(),
                }
            }
            other => OAuthBrokerError::DatabaseError(other.to_string()),
        })?;

        Ok(row)
    }

    async fn list_provider_definitions(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<ProviderDefinition>, OAuthBrokerError> {
        let rows = sqlx::query_as::<_, ProviderDefinition>(
            r#"
            SELECT id, tenant_id, alias, display_name, version, provider_family, binding_kind, grant_family,
                   authorization_endpoint, token_endpoint, client_id, client_auth_method, callback_mode, adapter_key,
                   adapter_version, ARRAY(SELECT jsonb_array_elements_text(scope_template)) AS scope_template,
                   runtime_config, status, created_by, created_at, updated_at
            FROM oauth_provider_definitions
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        Ok(rows)
    }

    async fn get_provider_definition(
        &self,
        provider_definition_id: Uuid,
    ) -> Result<Option<ProviderDefinition>, OAuthBrokerError> {
        let row = sqlx::query_as::<_, ProviderDefinition>(
            r#"
            SELECT id, tenant_id, alias, display_name, version, provider_family, binding_kind, grant_family,
                   authorization_endpoint, token_endpoint, client_id, client_auth_method, callback_mode, adapter_key,
                   adapter_version, ARRAY(SELECT jsonb_array_elements_text(scope_template)) AS scope_template,
                   runtime_config, status, created_by, created_at, updated_at
            FROM oauth_provider_definitions
            WHERE id = $1
            "#,
        )
        .bind(provider_definition_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        Ok(row)
    }

    async fn update_provider_definition(
        &self,
        tenant_id: Uuid,
        provider_definition_id: Uuid,
        input: UpdateProviderDefinitionInput,
    ) -> Result<ProviderDefinition, OAuthBrokerError> {
        let current = self
            .get_provider_definition(provider_definition_id)
            .await?
            .ok_or(OAuthBrokerError::ProviderDefinitionNotFound(
                provider_definition_id,
            ))?;
        if current.tenant_id != tenant_id {
            return Err(OAuthBrokerError::InvalidRequest(
                "provider definition tenant mismatch".to_string(),
            ));
        }

        let scope_template =
            serde_json::to_value(input.scope_template.unwrap_or(current.scope_template))
                .map_err(|error| OAuthBrokerError::SerializationError(error.to_string()))?;
        let runtime_config = input.runtime_config.unwrap_or(current.runtime_config);

        let row = sqlx::query_as::<_, ProviderDefinition>(
            r#"
            UPDATE oauth_provider_definitions
            SET display_name = $2,
                version = version + 1,
                authorization_endpoint = $3,
                token_endpoint = $4,
                client_id = $5,
                client_auth_method = $6,
                callback_mode = $7,
                adapter_key = $8,
                adapter_version = $9,
                scope_template = $10,
                runtime_config = $11,
                updated_at = $12
            WHERE id = $1
            RETURNING id, tenant_id, alias, display_name, version, provider_family, binding_kind, grant_family,
                      authorization_endpoint, token_endpoint, client_id, client_auth_method, callback_mode, adapter_key,
                      adapter_version, ARRAY(SELECT jsonb_array_elements_text(scope_template)) AS scope_template,
                      runtime_config, status, created_by, created_at, updated_at
            "#,
        )
        .bind(provider_definition_id)
        .bind(input.display_name.unwrap_or(current.display_name))
        .bind(input.authorization_endpoint.or(current.authorization_endpoint))
        .bind(input.token_endpoint.or(current.token_endpoint))
        .bind(input.client_id.or(current.client_id))
        .bind(input.client_auth_method.or(current.client_auth_method))
        .bind(input.callback_mode.or(current.callback_mode))
        .bind(input.adapter_key.or(current.adapter_key))
        .bind(input.adapter_version.or(current.adapter_version))
        .bind(scope_template)
        .bind(runtime_config)
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        let _ = input.updated_by;
        Ok(row)
    }

    async fn validate_provider_definition(
        &self,
        tenant_id: Uuid,
        provider_definition_id: Uuid,
    ) -> Result<ProviderValidationResult, OAuthBrokerError> {
        let provider = self
            .get_provider_definition(provider_definition_id)
            .await?
            .ok_or(OAuthBrokerError::ProviderDefinitionNotFound(
                provider_definition_id,
            ))?;
        if provider.tenant_id != tenant_id {
            return Err(OAuthBrokerError::InvalidRequest(
                "provider definition tenant mismatch".to_string(),
            ));
        }

        let mut checks = Vec::new();

        let endpoint_ok = match provider.grant_family {
            GrantFamily::AuthorizationCodePkce => {
                provider.authorization_endpoint.is_some() && provider.token_endpoint.is_some()
            }
            GrantFamily::ClientCredentials => provider.token_endpoint.is_some(),
            GrantFamily::None => true,
        };
        checks.push(if endpoint_ok {
            ProviderValidationCheck::passed("endpoint", "required endpoints are present")
        } else {
            ProviderValidationCheck::failed("endpoint", "required endpoints are missing")
        });

        let client_auth_ok = provider.client_id.is_some()
            && provider
                .client_auth_method
                .as_deref()
                .map(is_supported_client_auth_method)
                .unwrap_or(false);
        checks.push(if client_auth_ok {
            ProviderValidationCheck::passed("client_auth", "client auth method is supported")
        } else {
            ProviderValidationCheck::failed(
                "client_auth",
                "client_id or client_auth_method is missing/unsupported",
            )
        });

        let callback_adapter_ok = match provider.grant_family {
            GrantFamily::AuthorizationCodePkce => provider.callback_mode.is_some(),
            _ => provider.callback_mode.as_deref().unwrap_or("none") == "none",
        } && adapter_binding_is_supported(
            provider.adapter_key.as_deref(),
            provider.adapter_version.as_deref(),
        );
        checks.push(if callback_adapter_ok {
            ProviderValidationCheck::passed(
                "callback_adapter",
                "callback mode and adapter binding are valid",
            )
        } else {
            ProviderValidationCheck::failed(
                "callback_adapter",
                "callback mode and adapter binding are inconsistent",
            )
        });

        let connectivity_ok = provider
            .runtime_config
            .get("connectivity_probe_url")
            .and_then(|value| value.as_str())
            .or(provider.token_endpoint.as_deref())
            .or(provider.authorization_endpoint.as_deref());
        let connectivity_check = match connectivity_ok {
            Some(target) => probe_connectivity_target(target).await,
            None => ProviderValidationCheck::failed(
                "connectivity",
                "no connectivity probe target configured",
            ),
        };
        checks.push(connectivity_check);

        let valid = checks
            .iter()
            .all(|check| check.status == ProviderValidationCheckStatus::Passed);

        Ok(ProviderValidationResult {
            provider_definition_id,
            valid,
            checks,
            validated_at: Utc::now(),
        })
    }

    async fn create_binding(&self, input: CreateBindingInput) -> Result<Binding, OAuthBrokerError> {
        let provider = self
            .lookup_provider_definition(input.provider_definition_id)
            .await?;

        if provider.tenant_id != input.tenant_id {
            return Err(OAuthBrokerError::InvalidRequest(
                "provider definition tenant mismatch".to_string(),
            ));
        }

        let binding_id = Uuid::now_v7();
        let binding_handle = format!("binding_{}", binding_id.simple());
        let now = Utc::now();
        let allowed_domains = json_string_array(provider.runtime_config.get("allowed_domains"));
        let allowed_methods = json_string_array(provider.runtime_config.get("allowed_methods"));
        let allowed_path_prefixes =
            json_string_array(provider.runtime_config.get("allowed_path_prefixes"));

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO oauth_bindings (
                id, tenant_id, provider_definition_id, binding_handle, alias, purpose,
                binding_kind, subject_ref, subject_display_name, backing_credential_id, status,
                created_by, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            "#,
        )
        .bind(binding_id)
        .bind(input.tenant_id)
        .bind(input.provider_definition_id)
        .bind(&binding_handle)
        .bind(&input.alias)
        .bind(&input.purpose)
        .bind(provider.binding_kind)
        .bind(&input.subject_ref)
        .bind(&input.subject_display_name)
        .bind(&input.backing_credential_id)
        .bind(BindingStatus::Draft)
        .bind(input.created_by)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|error| match error {
            sqlx::Error::Database(db_error)
                if db_error.constraint() == Some("uq_oauth_bindings_tenant_alias") =>
            {
                OAuthBrokerError::BindingAlreadyExists {
                    tenant_id: input.tenant_id,
                    alias: input.alias.clone(),
                }
            }
            other => OAuthBrokerError::DatabaseError(other.to_string()),
        })?;

        let provider_scope_template = serde_json::to_value(&provider.scope_template)
            .map_err(|error| OAuthBrokerError::SerializationError(error.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO oauth_binding_policy_snapshots (
                id, tenant_id, binding_id, version, granted_scopes, effective_scopes,
                allowed_domains, allowed_methods, allowed_path_prefixes, validation_status,
                validated_at, applied_at, created_at
            )
            VALUES ($1, $2, $3, 1, $4, $4, $5, $6, $7, 'validated', $8, $8, $8)
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(input.tenant_id)
        .bind(binding_id)
        .bind(&provider_scope_template)
        .bind(
            serde_json::to_value(&allowed_domains)
                .map_err(|error| OAuthBrokerError::SerializationError(error.to_string()))?,
        )
        .bind(
            serde_json::to_value(&allowed_methods)
                .map_err(|error| OAuthBrokerError::SerializationError(error.to_string()))?,
        )
        .bind(
            serde_json::to_value(&allowed_path_prefixes)
                .map_err(|error| OAuthBrokerError::SerializationError(error.to_string()))?,
        )
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO oauth_binding_runtime_states (
                binding_id, tenant_id, health_status, last_error_code, last_error_message,
                cooldown_until, rebind_required, last_validated_at, updated_at
            )
            VALUES ($1, $2, 'draft', NULL, NULL, NULL, FALSE, NULL, $3)
            "#,
        )
        .bind(binding_id)
        .bind(input.tenant_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        tx.commit()
            .await
            .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        self.get_binding(binding_id)
            .await?
            .ok_or(OAuthBrokerError::BindingNotFound(binding_id))
    }

    async fn list_bindings(&self, tenant_id: Uuid) -> Result<Vec<Binding>, OAuthBrokerError> {
        let rows = sqlx::query_as::<_, Binding>(
            r#"
            SELECT b.id, b.tenant_id, b.provider_definition_id, b.binding_handle, b.alias, b.purpose,
                   b.binding_kind, b.subject_ref, b.subject_display_name, b.backing_credential_id,
                   b.status, rs.health_status, rs.last_error_code, rs.last_error_message, rs.cooldown_until,
                   rs.rebind_required, rs.last_validated_at, b.created_by, b.created_at, b.updated_at
            FROM oauth_bindings b
            LEFT JOIN oauth_binding_runtime_states rs ON rs.binding_id = b.id
            WHERE b.tenant_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        Ok(rows)
    }

    async fn get_binding(&self, binding_id: Uuid) -> Result<Option<Binding>, OAuthBrokerError> {
        let row = sqlx::query_as::<_, Binding>(
            r#"
            SELECT b.id, b.tenant_id, b.provider_definition_id, b.binding_handle, b.alias, b.purpose,
                   b.binding_kind, b.subject_ref, b.subject_display_name, b.backing_credential_id,
                   b.status, rs.health_status, rs.last_error_code, rs.last_error_message, rs.cooldown_until,
                   rs.rebind_required, rs.last_validated_at, b.created_by, b.created_at, b.updated_at
            FROM oauth_bindings b
            LEFT JOIN oauth_binding_runtime_states rs ON rs.binding_id = b.id
            WHERE b.id = $1
            "#,
        )
        .bind(binding_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        Ok(row)
    }

    async fn get_binding_by_handle(
        &self,
        tenant_id: Uuid,
        binding_handle: &str,
    ) -> Result<Option<Binding>, OAuthBrokerError> {
        let row = sqlx::query_as::<_, Binding>(
            r#"
            SELECT b.id, b.tenant_id, b.provider_definition_id, b.binding_handle, b.alias, b.purpose,
                   b.binding_kind, b.subject_ref, b.subject_display_name, b.backing_credential_id,
                   b.status, rs.health_status, rs.last_error_code, rs.last_error_message, rs.cooldown_until,
                   rs.rebind_required, rs.last_validated_at, b.created_by, b.created_at, b.updated_at
            FROM oauth_bindings b
            LEFT JOIN oauth_binding_runtime_states rs ON rs.binding_id = b.id
            WHERE b.tenant_id = $1
              AND b.binding_handle = $2
            "#,
        )
        .bind(tenant_id)
        .bind(binding_handle)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        Ok(row)
    }

    async fn get_latest_binding_policy_snapshot(
        &self,
        binding_id: Uuid,
    ) -> Result<Option<BindingPolicySnapshot>, OAuthBrokerError> {
        let row = sqlx::query_as::<_, BindingPolicySnapshot>(
            r#"
            SELECT id, tenant_id, binding_id, version,
                   ARRAY(SELECT jsonb_array_elements_text(granted_scopes)) AS granted_scopes,
                   ARRAY(SELECT jsonb_array_elements_text(effective_scopes)) AS effective_scopes,
                   ARRAY(SELECT jsonb_array_elements_text(allowed_domains)) AS allowed_domains,
                   ARRAY(SELECT jsonb_array_elements_text(allowed_methods)) AS allowed_methods,
                   ARRAY(SELECT jsonb_array_elements_text(allowed_path_prefixes)) AS allowed_path_prefixes,
                   validation_status, last_validation_error_code, last_validation_error_message,
                   validated_at, applied_at,
                   created_at
            FROM oauth_binding_policy_snapshots
            WHERE binding_id = $1
              AND applied_at IS NOT NULL
            ORDER BY version DESC
            LIMIT 1
            "#,
        )
        .bind(binding_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        Ok(row)
    }

    async fn stage_binding_policy(
        &self,
        binding_id: Uuid,
        input: StageBindingPolicyInput,
        staged_at: DateTime<Utc>,
    ) -> Result<BindingPolicySnapshot, OAuthBrokerError> {
        let binding = self
            .get_binding(binding_id)
            .await?
            .ok_or(OAuthBrokerError::BindingNotFound(binding_id))?;
        let latest_active = self
            .get_latest_binding_policy_snapshot(binding_id)
            .await?
            .ok_or(OAuthBrokerError::InvalidRequest(
                "binding is missing an active policy snapshot".to_string(),
            ))?;
        let next_version = latest_active.version + 1;

        sqlx::query(
            r#"
            DELETE FROM oauth_binding_policy_snapshots
            WHERE binding_id = $1
              AND applied_at IS NULL
            "#,
        )
        .bind(binding_id)
        .execute(&self.pool)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        let row = sqlx::query_as::<_, BindingPolicySnapshot>(
            r#"
            INSERT INTO oauth_binding_policy_snapshots (
                id, tenant_id, binding_id, version, granted_scopes, effective_scopes,
                allowed_domains, allowed_methods, allowed_path_prefixes, validation_status,
                last_validation_error_code, last_validation_error_message, validated_at, applied_at, created_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8, $9, 'pending_validation',
                NULL, NULL, NULL, NULL, $10
            )
            RETURNING id, tenant_id, binding_id, version,
                      ARRAY(SELECT jsonb_array_elements_text(granted_scopes)) AS granted_scopes,
                      ARRAY(SELECT jsonb_array_elements_text(effective_scopes)) AS effective_scopes,
                      ARRAY(SELECT jsonb_array_elements_text(allowed_domains)) AS allowed_domains,
                      ARRAY(SELECT jsonb_array_elements_text(allowed_methods)) AS allowed_methods,
                      ARRAY(SELECT jsonb_array_elements_text(allowed_path_prefixes)) AS allowed_path_prefixes,
                      validation_status, last_validation_error_code, last_validation_error_message,
                      validated_at, applied_at, created_at
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(binding.tenant_id)
        .bind(binding_id)
        .bind(next_version)
        .bind(
            serde_json::to_value(&input.granted_scopes)
                .map_err(|error| OAuthBrokerError::SerializationError(error.to_string()))?,
        )
        .bind(
            serde_json::to_value(&input.effective_scopes)
                .map_err(|error| OAuthBrokerError::SerializationError(error.to_string()))?,
        )
        .bind(
            serde_json::to_value(&input.allowed_domains)
                .map_err(|error| OAuthBrokerError::SerializationError(error.to_string()))?,
        )
        .bind(
            serde_json::to_value(&input.allowed_methods)
                .map_err(|error| OAuthBrokerError::SerializationError(error.to_string()))?,
        )
        .bind(
            serde_json::to_value(&input.allowed_path_prefixes)
                .map_err(|error| OAuthBrokerError::SerializationError(error.to_string()))?,
        )
        .bind(staged_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        Ok(row)
    }

    async fn get_staged_binding_policy(
        &self,
        binding_id: Uuid,
    ) -> Result<Option<BindingPolicySnapshot>, OAuthBrokerError> {
        let row = sqlx::query_as::<_, BindingPolicySnapshot>(
            r#"
            SELECT id, tenant_id, binding_id, version,
                   ARRAY(SELECT jsonb_array_elements_text(granted_scopes)) AS granted_scopes,
                   ARRAY(SELECT jsonb_array_elements_text(effective_scopes)) AS effective_scopes,
                   ARRAY(SELECT jsonb_array_elements_text(allowed_domains)) AS allowed_domains,
                   ARRAY(SELECT jsonb_array_elements_text(allowed_methods)) AS allowed_methods,
                   ARRAY(SELECT jsonb_array_elements_text(allowed_path_prefixes)) AS allowed_path_prefixes,
                   validation_status, last_validation_error_code, last_validation_error_message,
                   validated_at, applied_at, created_at
            FROM oauth_binding_policy_snapshots
            WHERE binding_id = $1
              AND applied_at IS NULL
            ORDER BY version DESC
            LIMIT 1
            "#,
        )
        .bind(binding_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        Ok(row)
    }

    async fn set_staged_binding_policy_validation(
        &self,
        binding_id: Uuid,
        validation_status: &str,
        last_validation_error_code: Option<&str>,
        last_validation_error_message: Option<&str>,
        validated_at: DateTime<Utc>,
    ) -> Result<BindingPolicySnapshot, OAuthBrokerError> {
        let row = sqlx::query_as::<_, BindingPolicySnapshot>(
            r#"
            UPDATE oauth_binding_policy_snapshots
            SET validation_status = $2,
                last_validation_error_code = $3,
                last_validation_error_message = $4,
                validated_at = $5
            WHERE id = (
                SELECT id
                FROM oauth_binding_policy_snapshots
                WHERE binding_id = $1
                  AND applied_at IS NULL
                ORDER BY version DESC
                LIMIT 1
            )
            RETURNING id, tenant_id, binding_id, version,
                      ARRAY(SELECT jsonb_array_elements_text(granted_scopes)) AS granted_scopes,
                      ARRAY(SELECT jsonb_array_elements_text(effective_scopes)) AS effective_scopes,
                      ARRAY(SELECT jsonb_array_elements_text(allowed_domains)) AS allowed_domains,
                      ARRAY(SELECT jsonb_array_elements_text(allowed_methods)) AS allowed_methods,
                      ARRAY(SELECT jsonb_array_elements_text(allowed_path_prefixes)) AS allowed_path_prefixes,
                      validation_status, last_validation_error_code, last_validation_error_message,
                      validated_at, applied_at, created_at
            "#,
        )
        .bind(binding_id)
        .bind(validation_status)
        .bind(last_validation_error_code)
        .bind(last_validation_error_message)
        .bind(validated_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        Ok(row)
    }

    async fn apply_staged_binding_policy(
        &self,
        binding_id: Uuid,
        applied_at: DateTime<Utc>,
    ) -> Result<BindingPolicySnapshot, OAuthBrokerError> {
        let staged = self.get_staged_binding_policy(binding_id).await?.ok_or(
            OAuthBrokerError::InvalidRequest(
                "binding does not have a staged policy change".to_string(),
            ),
        )?;

        if staged.validation_status != "validated" {
            return Err(OAuthBrokerError::InvalidRequest(
                "staged policy change must be validated before apply".to_string(),
            ));
        }

        let row = sqlx::query_as::<_, BindingPolicySnapshot>(
            r#"
            UPDATE oauth_binding_policy_snapshots
            SET applied_at = $2
            WHERE id = $1
            RETURNING id, tenant_id, binding_id, version,
                      ARRAY(SELECT jsonb_array_elements_text(granted_scopes)) AS granted_scopes,
                      ARRAY(SELECT jsonb_array_elements_text(effective_scopes)) AS effective_scopes,
                      ARRAY(SELECT jsonb_array_elements_text(allowed_domains)) AS allowed_domains,
                      ARRAY(SELECT jsonb_array_elements_text(allowed_methods)) AS allowed_methods,
                      ARRAY(SELECT jsonb_array_elements_text(allowed_path_prefixes)) AS allowed_path_prefixes,
                      validation_status, last_validation_error_code, last_validation_error_message,
                      validated_at, applied_at, created_at
            "#,
        )
        .bind(staged.id)
        .bind(applied_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        Ok(row)
    }

    async fn set_binding_validation_state(
        &self,
        binding_id: Uuid,
        binding_status: BindingStatus,
        health_status: &str,
        last_error_code: Option<&str>,
        last_error_message: Option<&str>,
        validated_at: DateTime<Utc>,
    ) -> Result<Binding, OAuthBrokerError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        sqlx::query(
            r#"
            UPDATE oauth_bindings
            SET status = $2,
                updated_at = $3
            WHERE id = $1
            "#,
        )
        .bind(binding_id)
        .bind(binding_status)
        .bind(validated_at)
        .execute(&mut *tx)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        sqlx::query(
            r#"
            UPDATE oauth_binding_runtime_states
            SET health_status = $2,
                last_error_code = $3,
                last_error_message = $4,
                cooldown_until = NULL,
                rebind_required = FALSE,
                last_validated_at = $5,
                updated_at = $5
            WHERE binding_id = $1
            "#,
        )
        .bind(binding_id)
        .bind(health_status)
        .bind(last_error_code)
        .bind(last_error_message)
        .bind(validated_at)
        .execute(&mut *tx)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        tx.commit()
            .await
            .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        self.get_binding(binding_id)
            .await?
            .ok_or(OAuthBrokerError::BindingNotFound(binding_id))
    }

    async fn set_binding_runtime_state(
        &self,
        binding_id: Uuid,
        update: BindingRuntimeStateUpdate,
    ) -> Result<Binding, OAuthBrokerError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        if let Some(binding_status) = update.binding_status {
            sqlx::query(
                r#"
                UPDATE oauth_bindings
                SET status = $2,
                    updated_at = $3
                WHERE id = $1
                "#,
            )
            .bind(binding_id)
            .bind(binding_status)
            .bind(update.updated_at)
            .execute(&mut *tx)
            .await
            .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;
        }

        sqlx::query(
            r#"
            UPDATE oauth_binding_runtime_states
            SET health_status = $2,
                last_error_code = $3,
                last_error_message = $4,
                cooldown_until = $5,
                rebind_required = $6,
                updated_at = $7
            WHERE binding_id = $1
            "#,
        )
        .bind(binding_id)
        .bind(&update.health_status)
        .bind(update.last_error_code.as_deref())
        .bind(update.last_error_message.as_deref())
        .bind(update.cooldown_until)
        .bind(update.rebind_required)
        .bind(update.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        tx.commit()
            .await
            .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        self.get_binding(binding_id)
            .await?
            .ok_or(OAuthBrokerError::BindingNotFound(binding_id))
    }

    async fn revoke_binding(
        &self,
        binding_id: Uuid,
        revoked_at: DateTime<Utc>,
    ) -> Result<Binding, OAuthBrokerError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        sqlx::query(
            r#"
            UPDATE oauth_bindings
            SET status = 'revoked',
                revoked_at = $2,
                updated_at = $2
            WHERE id = $1
            "#,
        )
        .bind(binding_id)
        .bind(revoked_at)
        .execute(&mut *tx)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        sqlx::query(
            r#"
            UPDATE oauth_binding_runtime_states
            SET health_status = 'revoked',
                last_error_code = 'binding_revoked',
                last_error_message = NULL,
                cooldown_until = NULL,
                rebind_required = FALSE,
                updated_at = $2
            WHERE binding_id = $1
            "#,
        )
        .bind(binding_id)
        .bind(revoked_at)
        .execute(&mut *tx)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        tx.commit()
            .await
            .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        self.get_binding(binding_id)
            .await?
            .ok_or(OAuthBrokerError::BindingNotFound(binding_id))
    }

    async fn delete_binding(&self, binding_id: Uuid) -> Result<(), OAuthBrokerError> {
        let binding = self
            .get_binding(binding_id)
            .await?
            .ok_or(OAuthBrokerError::BindingNotFound(binding_id))?;
        if binding.status != BindingStatus::Revoked {
            return Err(OAuthBrokerError::InvalidRequest(
                "binding must be revoked before delete".to_string(),
            ));
        }

        sqlx::query(
            r#"
            DELETE FROM oauth_bindings
            WHERE id = $1
            "#,
        )
        .bind(binding_id)
        .execute(&self.pool)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        Ok(())
    }

    async fn start_auth_transaction(
        &self,
        input: StartAuthTransactionInput,
    ) -> Result<AuthTransaction, OAuthBrokerError> {
        let provider = self
            .lookup_provider_definition(input.provider_definition_id)
            .await?;
        if provider.tenant_id != input.tenant_id {
            return Err(OAuthBrokerError::InvalidRequest(
                "provider definition tenant mismatch".to_string(),
            ));
        }

        let transaction = Self::build_auth_transaction(&provider, input)?;
        let requested_scopes = serde_json::to_value(&transaction.requested_scopes)
            .map_err(|error| OAuthBrokerError::SerializationError(error.to_string()))?;

        let row = sqlx::query_as::<_, AuthTransaction>(
            r#"
            INSERT INTO oauth_auth_transactions (
                id, tenant_id, provider_definition_id, created_by, alias, purpose,
                subject_ref_hint, subject_display_name, backing_credential_id, state, nonce, pkce_code_verifier,
                pkce_code_challenge, code_challenge_method, redirect_uri, requested_scopes,
                status, authorization_url, expires_at, consumed_at, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16,
                    $17, $18, $19, NULL, $20, $21)
            RETURNING id, tenant_id, provider_definition_id, created_by, alias, purpose,
                      subject_ref_hint, subject_display_name, backing_credential_id, state, nonce, pkce_code_verifier,
                      pkce_code_challenge, code_challenge_method, redirect_uri,
                      ARRAY(SELECT jsonb_array_elements_text(requested_scopes)) AS requested_scopes,
                      status, authorization_url, expires_at, consumed_at, created_at, updated_at
            "#,
        )
        .bind(transaction.id)
        .bind(transaction.tenant_id)
        .bind(transaction.provider_definition_id)
        .bind(transaction.created_by)
        .bind(&transaction.alias)
        .bind(&transaction.purpose)
        .bind(&transaction.subject_ref_hint)
        .bind(&transaction.subject_display_name)
        .bind(&transaction.backing_credential_id)
        .bind(&transaction.state)
        .bind(&transaction.nonce)
        .bind(&transaction.pkce_code_verifier)
        .bind(&transaction.pkce_code_challenge)
        .bind(&transaction.code_challenge_method)
        .bind(&transaction.redirect_uri)
        .bind(&requested_scopes)
        .bind(transaction.status)
        .bind(&transaction.authorization_url)
        .bind(transaction.expires_at)
        .bind(transaction.created_at)
        .bind(transaction.updated_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        Ok(row)
    }

    async fn get_auth_transaction(
        &self,
        transaction_id: Uuid,
    ) -> Result<Option<AuthTransaction>, OAuthBrokerError> {
        let row = sqlx::query_as::<_, AuthTransaction>(
            r#"
            SELECT id, tenant_id, provider_definition_id, created_by, alias, purpose,
                   subject_ref_hint, subject_display_name, backing_credential_id, state, nonce, pkce_code_verifier,
                   pkce_code_challenge, code_challenge_method, redirect_uri,
                   ARRAY(SELECT jsonb_array_elements_text(requested_scopes)) AS requested_scopes,
                   status, authorization_url, expires_at, consumed_at, created_at, updated_at
            FROM oauth_auth_transactions
            WHERE id = $1
            "#,
        )
        .bind(transaction_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        Ok(row)
    }

    async fn get_auth_transaction_by_state(
        &self,
        state: &str,
    ) -> Result<Option<AuthTransaction>, OAuthBrokerError> {
        let row = sqlx::query_as::<_, AuthTransaction>(
            r#"
            SELECT id, tenant_id, provider_definition_id, created_by, alias, purpose,
                   subject_ref_hint, subject_display_name, backing_credential_id, state, nonce, pkce_code_verifier,
                   pkce_code_challenge, code_challenge_method, redirect_uri,
                   ARRAY(SELECT jsonb_array_elements_text(requested_scopes)) AS requested_scopes,
                   status, authorization_url, expires_at, consumed_at, created_at, updated_at
            FROM oauth_auth_transactions
            WHERE state = $1
            "#,
        )
        .bind(state)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        Ok(row)
    }

    async fn set_auth_transaction_status(
        &self,
        transaction_id: Uuid,
        status: AuthTransactionStatus,
        consumed_at: Option<DateTime<Utc>>,
    ) -> Result<AuthTransaction, OAuthBrokerError> {
        let row = sqlx::query_as::<_, AuthTransaction>(
            r#"
            UPDATE oauth_auth_transactions
            SET status = $2,
                consumed_at = $3,
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, tenant_id, provider_definition_id, created_by, alias, purpose,
                      subject_ref_hint, subject_display_name, backing_credential_id, state, nonce, pkce_code_verifier,
                      pkce_code_challenge, code_challenge_method, redirect_uri,
                      ARRAY(SELECT jsonb_array_elements_text(requested_scopes)) AS requested_scopes,
                      status, authorization_url, expires_at, consumed_at, created_at, updated_at
            "#,
        )
        .bind(transaction_id)
        .bind(status)
        .bind(consumed_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| OAuthBrokerError::DatabaseError(error.to_string()))?;

        Ok(row)
    }
}

macro_rules! impl_text_sqlx_codec {
    ($ty:ty) => {
        impl sqlx::Type<Postgres> for $ty {
            fn type_info() -> sqlx::postgres::PgTypeInfo {
                sqlx::postgres::PgTypeInfo::with_name("text")
            }

            fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
                *ty == sqlx::postgres::PgTypeInfo::with_name("text")
                    || *ty == sqlx::postgres::PgTypeInfo::with_name("varchar")
            }
        }

        impl<'r> sqlx::Decode<'r, Postgres> for $ty {
            fn decode(
                value: sqlx::postgres::PgValueRef<'r>,
            ) -> Result<Self, sqlx::error::BoxDynError> {
                let s = <&str as sqlx::Decode<Postgres>>::decode(value)?;
                <$ty>::from_str(s).map_err(|error| {
                    Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
                        as sqlx::error::BoxDynError
                })
            }
        }

        impl<'q> sqlx::Encode<'q, Postgres> for $ty {
            fn encode_by_ref(
                &self,
                buf: &mut sqlx::postgres::PgArgumentBuffer,
            ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync + 'static>>
            {
                <&str as sqlx::Encode<Postgres>>::encode_by_ref(&self.as_str(), buf)
            }
        }
    };
}

impl_text_sqlx_codec!(BindingKind);
impl_text_sqlx_codec!(GrantFamily);
impl_text_sqlx_codec!(ProviderDefinitionStatus);
impl_text_sqlx_codec!(BindingStatus);
impl_text_sqlx_codec!(AuthTransactionStatus);

fn is_supported_client_auth_method(value: &str) -> bool {
    matches!(
        value,
        "client_secret_basic" | "client_secret_post" | "private_key_jwt" | "app_secret" | "none"
    )
}

fn adapter_binding_is_supported(adapter_key: Option<&str>, adapter_version: Option<&str>) -> bool {
    matches!(
        (adapter_key, adapter_version),
        (None, None)
            | (Some("lark_openapi"), Some("v1"))
            | (Some("oauth_openid"), Some("v1"))
            | (Some("google_service_account"), Some("v1"))
            | (Some("generic_passthrough"), Some("v1"))
    )
}

fn json_string_array(value: Option<&JsonValue>) -> Vec<String> {
    value
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn random_urlsafe_token(num_bytes: usize) -> String {
    let mut bytes = vec![0u8; num_bytes];
    ring::rand::SystemRandom::new()
        .fill(&mut bytes)
        .expect("system randomness should be available");
    URL_SAFE_NO_PAD.encode(bytes)
}

fn pkce_s256_challenge(verifier: &str) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest.as_ref())
}

struct AuthorizationUrlInput<'a> {
    authorization_endpoint: &'a str,
    client_id: Option<&'a str>,
    redirect_uri: &'a str,
    state: &'a str,
    nonce: &'a str,
    code_challenge: &'a str,
    scopes: &'a [String],
    extra_query: Option<&'a JsonValue>,
}

fn build_authorization_url(input: AuthorizationUrlInput<'_>) -> Result<String, OAuthBrokerError> {
    let client_id = input.client_id.ok_or_else(|| {
        OAuthBrokerError::InvalidRequest(
            "provider definition client_id is required for delegated auth".to_string(),
        )
    })?;

    let mut url = Url::parse(input.authorization_endpoint)
        .map_err(|error| OAuthBrokerError::InvalidRequest(error.to_string()))?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("response_type", "code");
        query.append_pair("client_id", client_id);
        query.append_pair("redirect_uri", input.redirect_uri);
        query.append_pair("state", input.state);
        query.append_pair("nonce", input.nonce);
        query.append_pair("code_challenge", input.code_challenge);
        query.append_pair("code_challenge_method", "S256");
        if !input.scopes.is_empty() {
            query.append_pair("scope", &input.scopes.join(" "));
        }
        if let Some(extra_query) = input.extra_query.and_then(JsonValue::as_object) {
            for (key, value) in extra_query {
                match value {
                    JsonValue::String(value) => {
                        query.append_pair(key, value);
                    }
                    JsonValue::Bool(value) => {
                        query.append_pair(key, &value.to_string());
                    }
                    JsonValue::Number(value) => {
                        query.append_pair(key, &value.to_string());
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(url.to_string())
}

async fn probe_connectivity_target(target: &str) -> ProviderValidationCheck {
    let url = match Url::parse(target) {
        Ok(url) => url,
        Err(error) => {
            return ProviderValidationCheck::failed(
                "connectivity",
                format!("invalid connectivity target: {error}"),
            );
        }
    };

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return ProviderValidationCheck::failed(
                "connectivity",
                format!("failed to build validation client: {error}"),
            );
        }
    };

    match client.get(url).send().await {
        Ok(response) if response.status().is_server_error() => ProviderValidationCheck::failed(
            "connectivity",
            format!("probe returned server error {}", response.status()),
        ),
        Ok(response) => ProviderValidationCheck::passed(
            "connectivity",
            format!("probe succeeded with status {}", response.status()),
        ),
        Err(error) => {
            ProviderValidationCheck::failed("connectivity", format!("probe failed: {error}"))
        }
    }
}
