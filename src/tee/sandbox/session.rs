//! 沙箱会话管理

use crate::crypto::hkdf::KeyHierarchy;
use crate::crypto::{CredentialCryptoContext, EncryptedBlob};
use crate::models::CredentialType;
use crate::tee::SharedEnclave;
use crate::tee::sandbox::{
    error::{SandboxError, SessionError},
    nsjail::{NsjailSandbox, SandboxProcessHealth},
    repository::{
        CompleteSandboxOperationRecord, NewSandboxOperationRecord, SandboxRepository, to_chrono_utc,
    },
    review::{OperationReviewer, ReviewContext, SuggestedAction},
    types::{
        ExecutionResult, OperationRequest, OperationStatus, OperationType, SessionContext,
        SessionId, SessionStatus,
    },
};
use crate::vault::models::{CredentialId, TenantId, UserId, VaultEntry};
use crate::vault::storage::CredentialVault;
use async_trait::async_trait;
use reqwest::{
    Client, Method, Url,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use time::OffsetDateTime;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;
use zeroize::Zeroizing;

#[async_trait]
pub trait SandboxSession: Send + Sync {
    fn id(&self) -> SessionId;
    async fn status(&self) -> SessionStatus;
    fn context(&self) -> &SessionContext;
    async fn execute_operation(
        &self,
        operation: OperationRequest,
    ) -> Result<ExecutionResult, SandboxError>;
    async fn pause(&self) -> Result<(), SandboxError>;
    async fn resume(&self) -> Result<(), SandboxError>;
    async fn close(&self) -> Result<(), SandboxError>;
    fn is_expired(&self) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveredSandboxResourceKind {
    WarmInstance,
    ActiveSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveredSandboxResource {
    pub kind: RecoveredSandboxResourceKind,
    pub sandbox_id: crate::tee::sandbox::types::SandboxId,
    pub session_id: Option<SessionId>,
}

#[async_trait]
pub trait SandboxResourceRecovery: Send + Sync {
    async fn recover_sandbox_resources(
        &self,
        current_session_id: SessionId,
    ) -> Result<Option<RecoveredSandboxResource>, SandboxError>;
}

#[async_trait]
trait SandboxOperationExecutor: Send + Sync {
    async fn execute(
        &self,
        session: &ActiveNsjailSession,
        operation: &OperationRequest,
    ) -> Result<SandboxExecutionOutput, SandboxError>;
}

struct DefaultSandboxOperationExecutor;

#[async_trait]
impl SandboxOperationExecutor for DefaultSandboxOperationExecutor {
    async fn execute(
        &self,
        session: &ActiveNsjailSession,
        operation: &OperationRequest,
    ) -> Result<SandboxExecutionOutput, SandboxError> {
        session.execute_in_sandbox(operation).await
    }
}

#[derive(Clone)]
pub struct ActiveNsjailSession {
    pub id: SessionId,
    context: SessionContext,
    status: Arc<RwLock<SessionStatus>>,
    sandbox: Arc<RwLock<Option<NsjailSandbox>>>,
    operation_history: Arc<RwLock<Vec<OperationRecord>>>,
    operation_reviewer: Option<Arc<OperationReviewer>>,
    repository: Option<Arc<dyn SandboxRepository>>,
    #[allow(dead_code)]
    vault: Option<Arc<CredentialVault>>,
    #[allow(dead_code)]
    key_hierarchy: Option<Arc<RwLock<KeyHierarchy>>>,
    #[allow(dead_code)]
    enclave: Option<SharedEnclave>,
    #[allow(dead_code)]
    credential_cache: Arc<RwLock<Option<Arc<SessionCredentialMaterial>>>>,
    resource_recovery: Option<Arc<dyn SandboxResourceRecovery>>,
    operation_executor: Arc<dyn SandboxOperationExecutor>,
}

#[derive(Debug, Clone)]
pub struct OperationRecord {
    pub operation_id: Uuid,
    pub operation_type: String,
    pub status: OperationStatus,
    pub started_at: OffsetDateTime,
    pub completed_at: Option<OffsetDateTime>,
    pub execution_time_ms: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug)]
struct SessionCredentialMaterial {
    values: HashMap<String, Zeroizing<String>>,
    provider: Option<crate::models::CredentialProvider>,
    allowed_domains: Vec<String>,
    custom_functions: Vec<crate::models::CredentialCustomFunction>,
}

#[derive(Debug)]
struct SandboxExecutionOutput {
    data: Option<Value>,
}

const DEFAULT_HTTP_REQUEST_TIMEOUT_MS: u64 = 30_000;
const MAX_HTTP_RESPONSE_BODY_BYTES: usize = 64 * 1024;
const MAX_RESOURCE_RECOVERY_RETRIES: usize = 2;

impl ActiveNsjailSession {
    pub fn new(id: SessionId, context: SessionContext, sandbox: NsjailSandbox) -> Self {
        Self::new_with_repository(id, context, sandbox, None)
    }

    pub fn new_with_repository(
        id: SessionId,
        context: SessionContext,
        sandbox: NsjailSandbox,
        repository: Option<Arc<dyn SandboxRepository>>,
    ) -> Self {
        Self::new_with_dependencies(id, context, sandbox, repository, None, None, None)
    }

    pub fn new_with_dependencies(
        id: SessionId,
        context: SessionContext,
        sandbox: NsjailSandbox,
        repository: Option<Arc<dyn SandboxRepository>>,
        vault: Option<Arc<CredentialVault>>,
        key_hierarchy: Option<Arc<RwLock<KeyHierarchy>>>,
        enclave: Option<SharedEnclave>,
    ) -> Self {
        Self {
            id,
            context,
            status: Arc::new(RwLock::new(SessionStatus::Ready)),
            sandbox: Arc::new(RwLock::new(Some(sandbox))),
            operation_history: Arc::new(RwLock::new(Vec::new())),
            operation_reviewer: None,
            repository,
            vault,
            key_hierarchy,
            enclave,
            credential_cache: Arc::new(RwLock::new(None)),
            resource_recovery: None,
            operation_executor: Arc::new(DefaultSandboxOperationExecutor),
        }
    }

    pub async fn take_sandbox(&self) -> Option<NsjailSandbox> {
        self.shutdown_runtime().await;
        self.sandbox.write().await.take()
    }

    pub async fn sandbox_process_health(&self) -> Option<SandboxProcessHealth> {
        self.sandbox
            .read()
            .await
            .as_ref()
            .and_then(NsjailSandbox::process_health)
    }

    pub fn with_reviewer(
        id: SessionId,
        context: SessionContext,
        sandbox: NsjailSandbox,
        reviewer: Arc<OperationReviewer>,
    ) -> Self {
        let mut session = Self::new(id, context, sandbox);
        session.operation_reviewer = Some(reviewer);
        session
    }

    pub fn set_operation_reviewer(&mut self, reviewer: Arc<OperationReviewer>) {
        self.operation_reviewer = Some(reviewer);
    }

    pub fn set_resource_recovery(&mut self, recovery: Arc<dyn SandboxResourceRecovery>) {
        self.resource_recovery = Some(recovery);
    }

    #[cfg(test)]
    fn set_operation_executor(&mut self, executor: Arc<dyn SandboxOperationExecutor>) {
        self.operation_executor = executor;
    }

    #[cfg(test)]
    pub(crate) async fn set_status_for_test(&self, status: SessionStatus) {
        *self.status.write().await = status;
    }

    pub async fn is_executing(&self) -> bool {
        *self.status.read().await == SessionStatus::Executing
    }

    pub async fn last_activity_at(&self) -> OffsetDateTime {
        self.context.last_activity().await
    }

    async fn can_execute(&self) -> Result<(), SessionError> {
        let status = *self.status.read().await;
        if !status.can_execute() {
            if status == SessionStatus::Executing
                && let Some(record) = self.current_executing_operation().await
            {
                return Err(SessionError::busy_with_operation(
                    self.id.into(),
                    record.operation_id,
                    record.started_at,
                ));
            }
            return Err(SessionError::invalid_state(
                self.id.into(),
                status,
                "ready or paused",
            ));
        }

        if self.is_expired() {
            return Err(SessionError::expired(self.id.into()));
        }

        let sandbox_guard = self.sandbox.read().await;
        if let Some(ref sandbox) = *sandbox_guard {
            if !sandbox.is_running().await {
                return Err(SessionError::CreationFailed {
                    reason: "Sandbox is not running".to_string(),
                });
            }
        } else {
            return Err(SessionError::CreationFailed {
                reason: "Sandbox not available".to_string(),
            });
        }

        Ok(())
    }

    async fn touch(&self) {
        self.context.touch().await;
    }

    async fn add_operation_record(&self, record: OperationRecord) {
        self.operation_history.write().await.push(record);
    }

    pub async fn get_operation_history(&self) -> Vec<OperationRecord> {
        self.operation_history.read().await.clone()
    }

    async fn current_executing_operation(&self) -> Option<OperationRecord> {
        self.operation_history
            .read()
            .await
            .iter()
            .rev()
            .find(|record| record.status == OperationStatus::Executing)
            .cloned()
    }

    async fn shutdown_runtime(&self) {
        *self.credential_cache.write().await = None;
    }

    #[allow(dead_code)]
    async fn credential_material(&self) -> Result<Arc<SessionCredentialMaterial>, SandboxError> {
        if let Some(material) = self.credential_cache.read().await.as_ref().cloned() {
            return Ok(material);
        }

        let vault = self.vault.as_ref().ok_or_else(|| {
            SandboxError::Config("sandbox credential vault is not configured".to_string())
        })?;
        let key_hierarchy = self.key_hierarchy.as_ref().ok_or_else(|| {
            SandboxError::Config("sandbox key hierarchy is not configured".to_string())
        })?;

        let credential_id = CredentialId::from_string(self.context.credential_id.to_string())
            .map_err(|error| SandboxError::Other(format!("invalid credential id: {error}")))?;
        let tenant_id = TenantId::new(self.context.tenant_id.to_string());
        let user_id = UserId::new(self.context.user_id.to_string());
        let entry = match vault.get_credential(&credential_id, &tenant_id, &user_id) {
            Ok(Some(entry)) => entry,
            Ok(None) => {
                return Err(SessionError::credential_not_found(self.context.credential_id).into());
            }
            Err(error) => {
                return Err(SandboxError::Other(format!(
                    "failed to load credential for sandbox session: {error}"
                )));
            }
        };

        let plaintext =
            decrypt_credential_material(self.enclave.as_ref(), key_hierarchy, &entry).await?;
        let plaintext_data: Value = serde_json::from_slice(&plaintext).map_err(|error| {
            SandboxError::Serialization(format!("failed to parse credential plaintext: {error}"))
        })?;

        let material = Arc::new(SessionCredentialMaterial {
            values: extract_supported_credential_fields(entry.credential_type, &plaintext_data)?,
            provider: entry.provider,
            allowed_domains: entry.allowed_domains.clone(),
            custom_functions: entry.custom_functions.clone(),
        });
        *self.credential_cache.write().await = Some(material.clone());
        Ok(material)
    }

    async fn execute_with_resource_retries(
        &self,
        operation: &OperationRequest,
    ) -> Result<SandboxExecutionOutput, SandboxError> {
        let mut retry_count = 0;

        loop {
            match self.operation_executor.execute(self, operation).await {
                Ok(output) => return Ok(output),
                Err(error)
                    if error.is_recoverable_resource_failure()
                        && retry_count < MAX_RESOURCE_RECOVERY_RETRIES =>
                {
                    let Some(recovery) = &self.resource_recovery else {
                        return Err(error);
                    };

                    self.shutdown_runtime().await;

                    let Some(recovered) = recovery.recover_sandbox_resources(self.id).await? else {
                        return Err(error);
                    };

                    retry_count += 1;
                    warn!(
                        session_id = %self.id,
                        operation_id = %operation.operation_id,
                        retry = retry_count,
                        original_error = %error,
                        recovered_kind = ?recovered.kind,
                        recovered_sandbox_id = %recovered.sandbox_id,
                        recovered_session_id = ?recovered.session_id,
                        "Recovered sandbox resources after operation failure; retrying"
                    );
                }
                Err(error) => {
                    self.shutdown_runtime().await;
                    return Err(error);
                }
            }
        }
    }

    async fn execute_operation_internal(
        &self,
        operation: OperationRequest,
        validate_session: bool,
    ) -> Result<ExecutionResult, SandboxError> {
        if validate_session {
            self.can_execute().await.map_err(SandboxError::Session)?;
        }

        *self.status.write().await = SessionStatus::Executing;
        self.touch().await;

        info!(
            "Executing operation {} ({}) in session {}",
            operation.operation_id, operation.operation_type, self.id
        );

        let start_time = OffsetDateTime::now_utc();
        self.add_operation_record(OperationRecord {
            operation_id: operation.operation_id,
            operation_type: operation.operation_type.to_string(),
            status: OperationStatus::Executing,
            started_at: start_time,
            completed_at: None,
            execution_time_ms: None,
        })
        .await;

        if let Some(repository) = &self.repository {
            if let Err(error) = repository
                .mark_session_operation_started(
                    self.context.session_id,
                    operation.operation_id,
                    to_chrono_utc(start_time),
                )
                .await
            {
                *self.status.write().await = SessionStatus::Ready;
                return Err(error);
            }
            if let Err(error) = repository
                .create_operation(NewSandboxOperationRecord {
                    operation_id: operation.operation_id,
                    session_id: self.context.session_id,
                    tenant_id: self.context.tenant_id,
                    credential_id: self.context.credential_id,
                    operation_type: operation.operation_type.to_string(),
                    input_params: Self::sanitized_operation_parameters(
                        operation.operation_type,
                        &operation.parameters,
                    ),
                    started_at: to_chrono_utc(start_time),
                })
                .await
            {
                let _ = repository
                    .mark_session_operation_finished(
                        self.context.session_id,
                        Some(error.to_string()),
                    )
                    .await;
                *self.status.write().await = SessionStatus::Ready;
                return Err(error);
            }
        }

        if let Some(ref reviewer) = self.operation_reviewer {
            let review_context = ReviewContext::new(
                self.context.session_id.0,
                self.context.tenant_id,
                self.context.user_id,
                self.context.credential_id,
                &self.context.original_intent,
            )
            .with_operation_type(operation.operation_type.to_string())
            .with_description(&operation.description);

            match reviewer.review_operation(&review_context, &operation).await {
                Ok(review_result) => match review_result.suggested_action {
                    SuggestedAction::Proceed | SuggestedAction::LogAndProceed => {}
                    _ => {
                        *self.status.write().await = SessionStatus::Ready;
                        let reason = review_result.reason;
                        if let Some(repository) = &self.repository {
                            repository
                                .complete_operation(CompleteSandboxOperationRecord {
                                    operation_id: operation.operation_id,
                                    status: "cancelled",
                                    output_result: None,
                                    error_message: Some(reason.clone()),
                                    completed_at: chrono::Utc::now(),
                                    execution_duration_ms: 0,
                                })
                                .await?;
                            repository
                                .mark_session_operation_finished(
                                    self.context.session_id,
                                    Some(reason.clone()),
                                )
                                .await?;
                        }
                        return Err(SessionError::OperationRejected { reason }.into());
                    }
                },
                Err(error) if reviewer.is_strict_mode() => {
                    *self.status.write().await = SessionStatus::Ready;
                    if let Some(repository) = &self.repository {
                        let reason = format!("AI review failed: {error}");
                        let _ = repository
                            .mark_session_operation_finished(
                                self.context.session_id,
                                Some(reason.clone()),
                            )
                            .await;
                    }
                    return Err(SessionError::OperationRejected {
                        reason: format!("AI review failed: {error}"),
                    }
                    .into());
                }
                Err(error) => {
                    warn!(
                        "AI review failed for operation {}: {}",
                        operation.operation_id, error
                    );
                }
            }
        }

        let result = self
            .build_execution_result(
                &operation,
                self.execute_with_resource_retries(&operation).await,
                start_time,
            )
            .await;

        let mut persistence_error = None;
        if let Some(repository) = &self.repository {
            if let Err(error) = repository
                .complete_operation(CompleteSandboxOperationRecord {
                    operation_id: operation.operation_id,
                    status: if result.success {
                        "completed"
                    } else {
                        "failed"
                    },
                    output_result: Self::sanitized_operation_output(
                        operation.operation_type,
                        result.data.as_ref(),
                        &operation.sensitive_output_values,
                    ),
                    error_message: result.error.clone(),
                    completed_at: chrono::Utc::now(),
                    execution_duration_ms: i32::try_from(result.execution_time_ms)
                        .unwrap_or(i32::MAX),
                })
                .await
            {
                persistence_error = Some(error);
            } else if let Err(error) = repository
                .mark_session_operation_finished(self.context.session_id, result.error.clone())
                .await
            {
                persistence_error = Some(error);
            }
        }

        *self.status.write().await = SessionStatus::Ready;
        if let Some(error) = persistence_error {
            return Err(error);
        }
        Ok(result)
    }

    async fn build_execution_result(
        &self,
        operation: &OperationRequest,
        operation_result: Result<SandboxExecutionOutput, SandboxError>,
        start_time: OffsetDateTime,
    ) -> ExecutionResult {
        let execution_time_ms =
            (OffsetDateTime::now_utc() - start_time).whole_milliseconds() as u64;
        let record_status = if operation_result.is_ok() {
            OperationStatus::Completed
        } else {
            OperationStatus::Failed
        };

        if let Some(record) = self.operation_history.write().await.last_mut() {
            record.status = record_status;
            record.completed_at = Some(OffsetDateTime::now_utc());
            record.execution_time_ms = Some(execution_time_ms);
        }

        match operation_result {
            Ok(output) => ExecutionResult {
                success: true,
                data: Self::sanitized_operation_output(
                    operation.operation_type,
                    output.data.as_ref(),
                    &operation.sensitive_output_values,
                ),
                error: None,
                execution_time_ms,
                audit_log: vec![],
            },
            Err(error) => ExecutionResult {
                success: false,
                data: None,
                error: Some(error.to_string()),
                execution_time_ms,
                audit_log: vec![],
            },
        }
    }

    #[cfg(test)]
    async fn execute_operation_for_test(
        &self,
        operation: OperationRequest,
    ) -> Result<ExecutionResult, SandboxError> {
        self.execute_operation_internal(operation, false).await
    }

    fn required_string(
        parameters: &HashMap<String, Value>,
        key: &str,
    ) -> Result<String, SandboxError> {
        parameters
            .get(key)
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| SandboxError::Other(format!("invalid_request: missing {key}")))
    }

    fn optional_u64(parameters: &HashMap<String, Value>, key: &str) -> Option<u64> {
        parameters.get(key).and_then(Value::as_u64)
    }

    #[allow(dead_code)]
    fn optional_bool(parameters: &HashMap<String, Value>, key: &str, default: bool) -> bool {
        parameters
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(default)
    }

    #[allow(dead_code)]
    fn optional_string(parameters: &HashMap<String, Value>, key: &str, default: &str) -> String {
        parameters
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| default.to_string())
    }

    #[allow(dead_code)]
    fn optional_string_array(
        parameters: &HashMap<String, Value>,
        key: &str,
    ) -> Result<Vec<String>, SandboxError> {
        let Some(value) = parameters.get(key) else {
            return Ok(Vec::new());
        };
        let items = value.as_array().ok_or_else(|| {
            SandboxError::Other(format!("invalid_request: {key} must be an array"))
        })?;
        items
            .iter()
            .map(|item| {
                item.as_str().map(ToOwned::to_owned).ok_or_else(|| {
                    SandboxError::Other(format!("invalid_request: {key} items must be strings"))
                })
            })
            .collect()
    }

    fn sanitized_operation_output(
        operation_type: OperationType,
        data: Option<&Value>,
        sensitive_output_values: &[String],
    ) -> Option<Value> {
        match operation_type {
            OperationType::HttpRequest => {
                data.map(|value| Self::redact_http_response_value(value, sensitive_output_values))
            }
            _ => data.cloned(),
        }
    }

    fn redact_http_response_value(value: &Value, sensitive_output_values: &[String]) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(key, nested)| {
                        (
                            key.clone(),
                            Self::redact_http_response_value(nested, sensitive_output_values),
                        )
                    })
                    .collect(),
            ),
            Value::Array(items) => Value::Array(
                items
                    .iter()
                    .map(|item| Self::redact_http_response_value(item, sensitive_output_values))
                    .collect(),
            ),
            Value::String(text) => Value::String(Self::redact_http_response_text(
                text,
                sensitive_output_values,
            )),
            _ => value.clone(),
        }
    }

    fn redact_http_response_text(text: &str, sensitive_output_values: &[String]) -> String {
        for sensitive in sensitive_output_values {
            if text == sensitive {
                return "[REDACTED]".to_string();
            }
        }

        let mut redacted = text.to_string();
        for sensitive in sensitive_output_values {
            redacted = if sensitive.len() >= "[REDACTED]".len() {
                redacted.replace(sensitive, "[REDACTED]")
            } else {
                Self::replace_sensitive_with_boundaries(&redacted, sensitive)
            };
        }

        redacted
    }

    fn replace_sensitive_with_boundaries(text: &str, sensitive: &str) -> String {
        if sensitive.is_empty() {
            return text.to_string();
        }

        let mut redacted = String::with_capacity(text.len());
        let mut last_end = 0usize;

        for (start, matched) in text.match_indices(sensitive) {
            let end = start + matched.len();
            if !Self::is_sensitive_match_boundary(text, start, end) {
                continue;
            }

            redacted.push_str(&text[last_end..start]);
            redacted.push_str("[REDACTED]");
            last_end = end;
        }

        if last_end == 0 {
            return text.to_string();
        }

        redacted.push_str(&text[last_end..]);
        redacted
    }

    fn is_sensitive_match_boundary(text: &str, start: usize, end: usize) -> bool {
        let previous = text[..start].chars().next_back();
        let next = text[end..].chars().next();

        previous.is_none_or(Self::is_sensitive_boundary_char)
            && next.is_none_or(Self::is_sensitive_boundary_char)
    }

    fn is_sensitive_boundary_char(ch: char) -> bool {
        !(ch.is_ascii_alphanumeric() || ch == '_')
    }

    fn required_http_method(parameters: &HashMap<String, Value>) -> Result<Method, SandboxError> {
        let method = Self::required_string(parameters, "method")?;
        Method::from_bytes(method.trim().to_ascii_uppercase().as_bytes()).map_err(|_| {
            SandboxError::Other(format!("invalid_request: invalid method {}", method.trim()))
        })
    }

    fn required_http_url(parameters: &HashMap<String, Value>) -> Result<Url, SandboxError> {
        let url = Self::required_string(parameters, "url")?;
        Url::parse(url.trim()).map_err(|_| {
            SandboxError::Other(format!("invalid_request: invalid url {}", url.trim()))
        })
    }

    fn optional_http_headers(
        parameters: &HashMap<String, Value>,
    ) -> Result<Option<HeaderMap>, SandboxError> {
        let Some(headers_value) = parameters.get("headers") else {
            return Ok(None);
        };
        let headers = headers_value.as_object().ok_or_else(|| {
            SandboxError::Other("invalid_request: headers must be an object".to_string())
        })?;

        let mut header_map = HeaderMap::with_capacity(headers.len());
        for (key, value) in headers {
            let text = value.as_str().ok_or_else(|| {
                SandboxError::Other(format!("invalid_request: headers.{key} must be a string"))
            })?;
            let name = HeaderName::try_from(key.as_str()).map_err(|_| {
                SandboxError::Other(format!("invalid_request: invalid header name {key}"))
            })?;
            let header_value = HeaderValue::from_str(text).map_err(|_| {
                SandboxError::Other(format!("invalid_request: invalid header value for {key}"))
            })?;
            header_map.append(name, header_value);
        }

        Ok(Some(header_map))
    }

    async fn execute_http_request(
        &self,
        parameters: &HashMap<String, Value>,
    ) -> Result<SandboxExecutionOutput, SandboxError> {
        let method = Self::required_http_method(parameters)?;
        let url = Self::required_http_url(parameters)?;
        let timeout_ms =
            Self::optional_u64(parameters, "timeout_ms").unwrap_or(DEFAULT_HTTP_REQUEST_TIMEOUT_MS);
        let client = Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|error| {
                SandboxError::Other(format!("failed to build http client: {error}"))
            })?;

        let mut request = client.request(method, url);
        if let Some(headers) = Self::optional_http_headers(parameters)? {
            request = request.headers(headers);
        }

        if let Some(body) = parameters.get("body") {
            request = match body {
                Value::String(text) => request.body(text.clone()),
                other => request.json(other),
            };
        }

        let mut response = request
            .send()
            .await
            .map_err(|error| SandboxError::Other(format!("http_request_failed: {error}")))?;
        let status = response.status().as_u16();
        let final_url = response.url().to_string();

        let headers = response
            .headers()
            .iter()
            .map(|(key, value)| {
                (
                    key.as_str().to_string(),
                    Value::String(value.to_str().unwrap_or_default().to_string()),
                )
            })
            .collect::<serde_json::Map<String, Value>>();

        let mut body = Vec::new();
        let mut truncated = false;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| SandboxError::Other(format!("http_request_failed: {error}")))?
        {
            if body.len() + chunk.len() > MAX_HTTP_RESPONSE_BODY_BYTES {
                let remaining = MAX_HTTP_RESPONSE_BODY_BYTES.saturating_sub(body.len());
                body.extend_from_slice(&chunk[..remaining]);
                truncated = true;
                break;
            }
            body.extend_from_slice(&chunk);
        }

        let response_body = match serde_json::from_slice::<Value>(&body) {
            Ok(json_body) => json_body,
            Err(_) => Value::String(String::from_utf8_lossy(&body).to_string()),
        };

        Ok(SandboxExecutionOutput {
            data: Some(json!({
                "status": status,
                "headers": Value::Object(headers),
                "body": response_body,
                "url": final_url,
                "truncated": truncated
            })),
        })
    }

    fn sanitized_operation_parameters(
        _operation_type: OperationType,
        parameters: &HashMap<String, Value>,
    ) -> Value {
        let sensitive = parameters
            .get("sensitive")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut sanitized = serde_json::Map::new();
        for (key, value) in parameters {
            sanitized.insert(key.clone(), sanitize_parameter_value(key, value, sensitive));
        }
        Value::Object(sanitized)
    }

    async fn execute_in_sandbox(
        &self,
        operation: &OperationRequest,
    ) -> Result<SandboxExecutionOutput, SandboxError> {
        let parameters = operation.effective_parameters();
        debug!(
            "Executing sandbox operation {} ({})",
            operation.operation_id, operation.operation_type
        );

        match operation.operation_type {
            OperationType::HttpRequest => self.execute_http_request(parameters).await,
            OperationType::Custom => Err(SandboxError::Other(
                "invalid_request: custom sandbox operations are not supported".to_string(),
            )),
        }
    }
}

#[async_trait]
impl SandboxSession for ActiveNsjailSession {
    fn id(&self) -> SessionId {
        self.id
    }

    async fn status(&self) -> SessionStatus {
        *self.status.read().await
    }

    fn context(&self) -> &SessionContext {
        &self.context
    }

    async fn execute_operation(
        &self,
        operation: OperationRequest,
    ) -> Result<ExecutionResult, SandboxError> {
        self.execute_operation_internal(operation, true).await
    }

    async fn pause(&self) -> Result<(), SandboxError> {
        let mut status = self.status.write().await;
        if *status != SessionStatus::Ready {
            return Err(SessionError::invalid_state(self.id.into(), *status, "ready").into());
        }
        *status = SessionStatus::Paused;
        Ok(())
    }

    async fn resume(&self) -> Result<(), SandboxError> {
        let mut status = self.status.write().await;
        if *status != SessionStatus::Paused {
            return Err(SessionError::invalid_state(self.id.into(), *status, "paused").into());
        }
        *status = SessionStatus::Ready;
        Ok(())
    }

    async fn close(&self) -> Result<(), SandboxError> {
        let mut status = self.status.write().await;
        if *status == SessionStatus::Closed {
            return Ok(());
        }
        self.shutdown_runtime().await;
        if let Some(mut sandbox) = self.sandbox.write().await.take()
            && let Err(error) = sandbox.stop().await
        {
            warn!("Failed to stop sandbox for session {}: {}", self.id, error);
        }
        *status = SessionStatus::Closed;
        Ok(())
    }

    fn is_expired(&self) -> bool {
        OffsetDateTime::now_utc() > self.context.expires_at
    }
}

#[allow(dead_code)]
async fn decrypt_credential_material(
    enclave: Option<&SharedEnclave>,
    key_hierarchy: &Arc<RwLock<KeyHierarchy>>,
    entry: &VaultEntry,
) -> Result<Vec<u8>, SandboxError> {
    let blob = EncryptedBlob {
        version: entry.encrypted_payload.version,
        algorithm: entry.encrypted_payload.algorithm.clone(),
        kdf: entry.encrypted_payload.kdf.clone(),
        nonce: entry.encrypted_payload.nonce.clone(),
        auth_tag: entry.encrypted_payload.auth_tag.clone(),
        ciphertext: entry.encrypted_payload.ciphertext.clone(),
        aad_hash: None,
    };
    let context = CredentialCryptoContext::new(
        entry.tenant_id.as_str(),
        entry.user_id.hash(),
        entry.credential_id.as_str(),
    );

    if let Some(enclave) = enclave {
        let mut enclave = enclave.lock().await;
        if enclave.is_running() {
            return enclave
                .decrypt_credential(
                    entry.tenant_id.as_str(),
                    entry.user_id.hash(),
                    entry.credential_id.as_str(),
                    &blob,
                )
                .map_err(|error| SandboxError::Other(format!("TEE decrypt failed: {error}")));
        }
    }

    let hierarchy = key_hierarchy.read().await;
    context
        .decrypt_with_hierarchy(&hierarchy, &blob)
        .map_err(|error| SandboxError::Other(format!("software decrypt failed: {error}")))
}

fn sanitize_parameter_value(key: &str, value: &Value, sensitive: bool) -> Value {
    if value.get("$credential").and_then(Value::as_str).is_some() {
        return json!({
            "$credential": value.get("$credential").and_then(Value::as_str).unwrap_or_default()
        });
    }

    let normalized = key.to_ascii_lowercase();
    let sensitive_key = normalized == "authorization"
        || normalized == "cookie"
        || normalized == "set-cookie"
        || normalized == "bindings"
        || normalized == "credentialbindings"
        || normalized == "credential_bindings"
        || normalized == "api_key"
        || normalized == "api-key"
        || normalized == "secret"
        || normalized.contains("token");
    if (sensitive || sensitive_key) && value.is_string() {
        return Value::String("[REDACTED]".to_string());
    }

    match value {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| sanitize_parameter_value(key, item, sensitive))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(nested_key, nested_value)| {
                    (
                        nested_key.clone(),
                        sanitize_parameter_value(nested_key, nested_value, sensitive),
                    )
                })
                .collect(),
        ),
        _ => value.clone(),
    }
}

#[allow(dead_code)]
fn redact_sensitive_text(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let looks_secret = trimmed.starts_with("Bearer ")
        || trimmed.starts_with("sk_")
        || trimmed.starts_with("ghp_")
        || trimmed.starts_with("eyJ")
        || (trimmed.len() >= 24
            && !trimmed.contains(char::is_whitespace)
            && trimmed.chars().any(|c| c.is_ascii_digit())
            && trimmed.chars().any(|c| c.is_ascii_alphabetic()));

    if looks_secret {
        "********".to_string()
    } else {
        trimmed.to_string()
    }
}

#[allow(dead_code)]
fn validate_dom_export_format(format: &str) -> Result<(), SandboxError> {
    match format {
        "html" | "text" | "json" => Ok(()),
        _ => Err(SandboxError::Other(format!(
            "invalid_request: unsupported dom export format {format}"
        ))),
    }
}

#[allow(dead_code)]
fn redact_dom_export_result(value: Value) -> Value {
    match value {
        Value::String(text) => Value::String(redact_sensitive_text(&text)),
        Value::Array(items) => {
            Value::Array(items.into_iter().map(redact_dom_export_result).collect())
        }
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, item)| (key, redact_dom_export_result(item)))
                .collect(),
        ),
        other => other,
    }
}

#[allow(dead_code)]
fn truncate_dom_export_data(mut data: Value, max_bytes: u64) -> Value {
    let max_bytes = usize::try_from(max_bytes).unwrap_or(usize::MAX);
    let mut truncated = false;

    if serialized_len(&data) > max_bytes
        && let Some(object) = data.as_object_mut()
        && let Some(content) = object.get_mut("content")
    {
        let content_text = match &*content {
            Value::String(text) => text.clone(),
            other => serde_json::to_string(other).unwrap_or_default(),
        };
        let mut budget = max_bytes.saturating_sub(512);
        if budget == 0 {
            budget = max_bytes;
        }
        *content = Value::String(truncate_utf8(&content_text, budget));
        truncated = true;

        while serialized_len(&data) > max_bytes {
            let Some(object) = data.as_object_mut() else {
                break;
            };
            let Some(Value::String(content_text)) = object.get_mut("content") else {
                break;
            };
            if content_text.is_empty() {
                break;
            }
            let next_len = content_text.len() / 2;
            *content_text = truncate_utf8(content_text, next_len);
        }
    }

    if let Some(object) = data.as_object_mut() {
        object.insert("truncated".to_string(), Value::Bool(truncated));
    }
    data
}

#[allow(dead_code)]
fn serialized_len(value: &Value) -> usize {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
}

#[allow(dead_code)]
fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }

    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

#[allow(dead_code)]
fn extract_supported_credential_fields(
    credential_type: CredentialType,
    plaintext_data: &Value,
) -> Result<HashMap<String, Zeroizing<String>>, SandboxError> {
    let object = plaintext_data.as_object().ok_or_else(|| {
        SandboxError::Other(
            "invalid_request: delegated credential payload must be a JSON object".to_string(),
        )
    })?;
    let mut values = HashMap::new();
    if credential_type == CredentialType::ApiKey {
        let api_key = ["api_key", "key", "apiKey"]
            .into_iter()
            .find_map(|field| object.get(field))
            .and_then(|value| match value {
                Value::String(text) => Some(text.clone()),
                Value::Number(number) => Some(number.to_string()),
                Value::Bool(boolean) => Some(boolean.to_string()),
                _ => None,
            })
            .ok_or_else(|| {
                SandboxError::Other(
                    "invalid_request: delegated credential payload missing api_key".to_string(),
                )
            })?;
        for field in ["api_key", "key", "apiKey"] {
            values.insert(field.to_string(), Zeroizing::new(api_key.clone()));
        }
        return Ok(values);
    }

    let allowed_fields: &[&str] = match credential_type {
        CredentialType::UsernamePassword => &["username", "password"],
        CredentialType::SessionCookie => &["cookie", "name"],
        CredentialType::OAuthRefresh => &["refresh_token", "refreshToken"],
        _ => {
            return Err(SandboxError::Other(format!(
                "invalid_request: delegated sandbox credentials do not support {}",
                credential_type.as_str()
            )));
        }
    };

    for field in allowed_fields {
        let Some(value) = object.get(*field) else {
            continue;
        };
        match value {
            Value::String(text) => {
                let normalized_field = if *field == "refreshToken" {
                    "refresh_token"
                } else {
                    field
                };
                values.insert(normalized_field.to_string(), Zeroizing::new(text.clone()));
            }
            Value::Number(number) => {
                values.insert(field.to_string(), Zeroizing::new(number.to_string()));
            }
            Value::Bool(boolean) => {
                values.insert(field.to_string(), Zeroizing::new(boolean.to_string()));
            }
            _ => {}
        }
    }

    if values.is_empty() {
        return Err(SandboxError::Other(
            "invalid_request: delegated credential payload must contain at least one scalar field"
                .to_string(),
        ));
    }

    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tee::sandbox::config::SandboxConfig;
    use crate::tee::sandbox::repository::{
        CompleteSandboxOperationRecord, NewSandboxOperationRecord, NewSandboxSessionRecord,
        SandboxOperationRecord, SandboxRepository, SandboxSessionRecord,
    };
    use crate::tee::sandbox::types::SandboxId;
    use async_trait::async_trait;
    use chrono::Utc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        sync::Mutex as TokioMutex,
    };

    fn create_test_session() -> ActiveNsjailSession {
        let sandbox = NsjailSandbox::new(crate::tee::sandbox::config::NsjailConfig {
            sandbox: SandboxConfig::default(),
            command: vec!["sleep".to_string(), "60".to_string()],
            cwd: std::path::PathBuf::from("/"),
            env: HashMap::new(),
            enable_user_namespace: true,
            uid_map: Default::default(),
            gid_map: Default::default(),
        });
        let context = SessionContext {
            session_id: SessionId::new(),
            sandbox_id: SandboxId::new(),
            tenant_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            credential_id: Uuid::new_v4(),
            original_intent: "test".to_string(),
            created_at: OffsetDateTime::now_utc(),
            expires_at: OffsetDateTime::now_utc() + time::Duration::minutes(30),
            last_activity_at: Arc::new(RwLock::new(OffsetDateTime::now_utc())),
        };

        ActiveNsjailSession::new(SessionId::new(), context, sandbox)
    }

    #[derive(Default)]
    struct MockSessionRepository {
        create_operation_calls: AtomicUsize,
        complete_operation_calls: AtomicUsize,
    }

    #[async_trait]
    impl SandboxRepository for MockSessionRepository {
        async fn create_session(
            &self,
            _record: NewSandboxSessionRecord,
        ) -> Result<(), SandboxError> {
            Ok(())
        }

        async fn mark_session_terminated(
            &self,
            _session_id: SessionId,
            _status: &'static str,
            _reason: Option<String>,
            _terminated_at: chrono::DateTime<Utc>,
        ) -> Result<(), SandboxError> {
            Ok(())
        }

        async fn update_session_status(
            &self,
            _session_id: SessionId,
            _status: &'static str,
        ) -> Result<(), SandboxError> {
            Ok(())
        }

        async fn mark_session_operation_started(
            &self,
            _session_id: SessionId,
            _operation_id: Uuid,
            _started_at: chrono::DateTime<Utc>,
        ) -> Result<(), SandboxError> {
            Ok(())
        }

        async fn mark_session_operation_finished(
            &self,
            _session_id: SessionId,
            _last_error_summary: Option<String>,
        ) -> Result<(), SandboxError> {
            Ok(())
        }

        async fn create_operation(
            &self,
            _record: NewSandboxOperationRecord,
        ) -> Result<(), SandboxError> {
            self.create_operation_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn complete_operation(
            &self,
            _record: CompleteSandboxOperationRecord,
        ) -> Result<(), SandboxError> {
            self.complete_operation_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn reconcile_orphaned_active_sessions(
            &self,
            _recovery_reason: &str,
        ) -> Result<u64, SandboxError> {
            Ok(0)
        }

        async fn list_sessions_by_tenant(
            &self,
            _tenant_id: Uuid,
        ) -> Result<Vec<SandboxSessionRecord>, SandboxError> {
            Ok(Vec::new())
        }

        async fn get_session_by_id(
            &self,
            _tenant_id: Uuid,
            _session_id: SessionId,
        ) -> Result<Option<SandboxSessionRecord>, SandboxError> {
            Ok(None)
        }

        async fn get_operation_by_id(
            &self,
            _tenant_id: Uuid,
            _operation_id: Uuid,
        ) -> Result<Option<SandboxOperationRecord>, SandboxError> {
            Ok(None)
        }
    }

    #[derive(Default)]
    struct MockRecoveryCoordinator {
        calls: AtomicUsize,
        results: TokioMutex<Vec<Option<RecoveredSandboxResource>>>,
    }

    #[async_trait]
    impl SandboxResourceRecovery for MockRecoveryCoordinator {
        async fn recover_sandbox_resources(
            &self,
            _current_session_id: SessionId,
        ) -> Result<Option<RecoveredSandboxResource>, SandboxError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.results.lock().await.remove(0))
        }
    }

    struct MockOperationExecutor {
        attempts: AtomicUsize,
        results: TokioMutex<Vec<Result<SandboxExecutionOutput, SandboxError>>>,
    }

    #[async_trait]
    impl SandboxOperationExecutor for MockOperationExecutor {
        async fn execute(
            &self,
            _session: &ActiveNsjailSession,
            _operation: &OperationRequest,
        ) -> Result<SandboxExecutionOutput, SandboxError> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            self.results.lock().await.remove(0)
        }
    }

    fn create_test_operation() -> OperationRequest {
        OperationRequest {
            operation_id: Uuid::new_v4(),
            operation_type: OperationType::HttpRequest,
            description: "test operation".to_string(),
            parameters: HashMap::from([
                ("method".to_string(), Value::String("GET".to_string())),
                (
                    "url".to_string(),
                    Value::String("http://127.0.0.1/test".to_string()),
                ),
            ]),
            resolved_parameters: HashMap::new(),
            sensitive_output_values: Vec::new(),
            created_at: OffsetDateTime::now_utc(),
        }
    }

    #[test]
    fn test_sanitize_parameter_value_preserves_credential_reference() {
        let value = json!({ "$credential": "password", "ignored": true });
        assert_eq!(
            sanitize_parameter_value("value", &value, true),
            json!({ "$credential": "password" })
        );
    }

    #[test]
    fn test_redact_sensitive_text_masks_secret_like_values() {
        assert_eq!(
            redact_sensitive_text("sk_1234567890abcdefghijklmnop"),
            "********"
        );
        assert_eq!(redact_sensitive_text("visible text"), "visible text");
    }

    #[test]
    fn test_dom_export_format_validation() {
        assert!(validate_dom_export_format("html").is_ok());
        assert!(validate_dom_export_format("text").is_ok());
        assert!(validate_dom_export_format("json").is_ok());
        assert!(validate_dom_export_format("png").is_err());
    }

    #[test]
    fn test_redact_dom_export_result_masks_secret_like_values() {
        let result = redact_dom_export_result(json!({
            "content": "Bearer secret-token",
            "nested": ["visible", "sk_1234567890abcdefghijklmnop"]
        }));

        assert_eq!(result["content"], "********");
        assert_eq!(result["nested"][0], "visible");
        assert_eq!(result["nested"][1], "********");
    }

    #[test]
    fn test_truncate_dom_export_data_sets_truncated_flag() {
        let result = truncate_dom_export_data(
            json!({
                "format": "html",
                "content": "x".repeat(1024),
                "truncated": false
            }),
            128,
        );

        assert_eq!(result["truncated"], true);
        assert!(result["content"].as_str().expect("content").len() < 1024);
    }

    #[test]
    fn test_extract_supported_credential_fields_supports_api_key_and_cookie() {
        let api_key = extract_supported_credential_fields(
            CredentialType::ApiKey,
            &json!({ "api_key": "secret-key" }),
        )
        .expect("api key fields");
        assert_eq!(
            api_key.get("api_key").map(|value| value.as_str()),
            Some("secret-key")
        );

        let cookie = extract_supported_credential_fields(
            CredentialType::SessionCookie,
            &json!({ "cookie": "abc", "name": "sid" }),
        )
        .expect("cookie fields");
        assert_eq!(
            cookie.get("cookie").map(|value| value.as_str()),
            Some("abc")
        );
        assert_eq!(cookie.get("name").map(|value| value.as_str()), Some("sid"));
    }

    #[test]
    fn test_extract_supported_credential_fields_supports_api_key() {
        let values = extract_supported_credential_fields(
            CredentialType::ApiKey,
            &json!({ "api_key": "sk_live_123" }),
        )
        .expect("api key fields should be supported");

        assert_eq!(
            values.get("api_key").map(|value| value.as_str()),
            Some("sk_live_123")
        );
        assert_eq!(
            values.get("key").map(|value| value.as_str()),
            Some("sk_live_123")
        );
        assert_eq!(
            values.get("apiKey").map(|value| value.as_str()),
            Some("sk_live_123")
        );
    }

    #[test]
    fn test_extract_supported_credential_fields_supports_legacy_api_key_aliases() {
        let values = extract_supported_credential_fields(
            CredentialType::ApiKey,
            &json!({ "apiKey": "sk_live_legacy" }),
        )
        .expect("legacy api key field should be supported");

        assert_eq!(
            values.get("api_key").map(|value| value.as_str()),
            Some("sk_live_legacy")
        );
        assert_eq!(
            values.get("key").map(|value| value.as_str()),
            Some("sk_live_legacy")
        );
        assert_eq!(
            values.get("apiKey").map(|value| value.as_str()),
            Some("sk_live_legacy")
        );
    }

    #[test]
    fn test_extract_supported_credential_fields_supports_cookie_and_name() {
        let values = extract_supported_credential_fields(
            CredentialType::SessionCookie,
            &json!({ "cookie": "session=abc", "name": "session" }),
        )
        .expect("session cookie fields should be supported");

        assert_eq!(
            values.get("cookie").map(|value| value.as_str()),
            Some("session=abc")
        );
        assert_eq!(
            values.get("name").map(|value| value.as_str()),
            Some("session")
        );
    }

    #[test]
    fn test_sanitized_operation_output_redacts_http_request_response_values() {
        let output = ActiveNsjailSession::sanitized_operation_output(
            OperationType::HttpRequest,
            Some(&json!({
                "headers": {
                    "X-Cred": "sk_live_123"
                },
                "body": {
                    "token": "sk_live_123",
                    "nested": [
                        "prefix sk_live_123 suffix",
                        { "secret": "Bearer sk_live_123" }
                    ]
                }
            })),
            &["sk_live_123".to_string(), "Bearer sk_live_123".to_string()],
        )
        .expect("http_request output should be redacted");

        assert_eq!(output["headers"]["X-Cred"], "[REDACTED]");
        assert_eq!(output["body"]["token"], "[REDACTED]");
        assert_eq!(output["body"]["nested"][0], "prefix [REDACTED] suffix");
        assert_eq!(output["body"]["nested"][1]["secret"], "[REDACTED]");
    }

    #[test]
    fn test_sanitized_operation_output_redacts_short_http_secrets_with_boundaries() {
        let output = ActiveNsjailSession::sanitized_operation_output(
            OperationType::HttpRequest,
            Some(&json!({
                "body": {
                    "otp_query": "code=12345",
                    "otp_bearer": "Bearer 12345",
                    "embedded_word": "abc12345def"
                }
            })),
            &["12345".to_string()],
        )
        .expect("http_request output should be redacted");

        assert_eq!(output["body"]["otp_query"], "code=[REDACTED]");
        assert_eq!(output["body"]["otp_bearer"], "Bearer [REDACTED]");
        assert_eq!(output["body"]["embedded_word"], "abc12345def");
    }

    #[test]
    fn test_sanitized_operation_output_leaves_non_http_request_data_untouched() {
        let original = json!({ "text": "prefix sk_live_123 suffix" });
        let output = ActiveNsjailSession::sanitized_operation_output(
            OperationType::Custom,
            Some(&original),
            &["sk_live_123".to_string()],
        )
        .expect("non-http output should stay intact");

        assert_eq!(output, original);
    }

    async fn spawn_test_http_server(body: String, content_type: &str) -> Option<String> {
        let listener = match TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return None,
            Err(error) => panic!("bind test server: {error}"),
        };
        let address = listener.local_addr().expect("local addr");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nX-Test: sandbox\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept request");
            let mut buffer = [0_u8; 2048];
            let _ = stream.read(&mut buffer).await;
            stream
                .write_all(response.as_bytes())
                .await
                .expect("write response");
        });

        Some(format!("http://{address}/"))
    }

    #[tokio::test]
    async fn test_http_request_executes_without_browser_runtime() {
        let session = create_test_session();
        let Some(url) =
            spawn_test_http_server("{\"ok\":true}".to_string(), "application/json").await
        else {
            return;
        };
        let operation = OperationRequest {
            operation_id: Uuid::new_v4(),
            operation_type: OperationType::HttpRequest,
            description: "http request".to_string(),
            parameters: HashMap::from([
                ("method".to_string(), Value::String("GET".to_string())),
                ("url".to_string(), Value::String(url.clone())),
            ]),
            resolved_parameters: HashMap::new(),
            sensitive_output_values: Vec::new(),
            created_at: OffsetDateTime::now_utc(),
        };

        let output = session
            .execute_in_sandbox(&operation)
            .await
            .expect("http request should succeed");

        assert_eq!(
            output.data.as_ref().and_then(|data| data.get("status")),
            Some(&json!(200))
        );
        assert_eq!(
            output
                .data
                .as_ref()
                .and_then(|data| data.get("body"))
                .and_then(|body| body.get("ok")),
            Some(&json!(true))
        );
        assert_eq!(
            output
                .data
                .as_ref()
                .and_then(|data| data.get("url"))
                .and_then(Value::as_str),
            Some(url.as_str())
        );
    }

    #[tokio::test]
    async fn test_execute_operation_redacts_http_request_result_before_return() {
        let executor = Arc::new(MockOperationExecutor {
            attempts: AtomicUsize::new(0),
            results: TokioMutex::new(vec![Ok(SandboxExecutionOutput {
                data: Some(json!({
                    "headers": {
                        "X-Cred": "sk_live_123"
                    },
                    "body": {
                        "message": "Bearer sk_live_123",
                        "nested": ["before sk_live_123 after"]
                    }
                })),
            })]),
        });
        let mut session = create_test_session();
        session.operation_executor = executor;

        let mut operation = create_test_operation();
        operation.sensitive_output_values =
            vec!["sk_live_123".to_string(), "Bearer sk_live_123".to_string()];

        let result = session
            .execute_operation_for_test(operation)
            .await
            .expect("http_request operation should succeed");

        assert_eq!(
            result.data.as_ref().unwrap()["headers"]["X-Cred"],
            "[REDACTED]"
        );
        assert_eq!(
            result.data.as_ref().unwrap()["body"]["message"],
            "[REDACTED]"
        );
        assert_eq!(
            result.data.as_ref().unwrap()["body"]["nested"][0],
            "before [REDACTED] after"
        );
    }

    #[tokio::test]
    async fn test_http_request_rejects_missing_method() {
        let session = create_test_session();
        let operation = OperationRequest {
            operation_id: Uuid::new_v4(),
            operation_type: OperationType::HttpRequest,
            description: "missing method".to_string(),
            parameters: HashMap::from([(
                "url".to_string(),
                Value::String("http://127.0.0.1".to_string()),
            )]),
            resolved_parameters: HashMap::new(),
            sensitive_output_values: Vec::new(),
            created_at: OffsetDateTime::now_utc(),
        };

        let error = session
            .execute_in_sandbox(&operation)
            .await
            .expect_err("missing method should fail");
        assert!(
            error
                .to_string()
                .contains("invalid_request: missing method")
        );
    }

    #[tokio::test]
    async fn test_http_request_rejects_invalid_method_and_url() {
        let session = create_test_session();
        let invalid_method = OperationRequest {
            operation_id: Uuid::new_v4(),
            operation_type: OperationType::HttpRequest,
            description: "invalid method".to_string(),
            parameters: HashMap::from([
                (
                    "method".to_string(),
                    Value::String("NOT A METHOD".to_string()),
                ),
                (
                    "url".to_string(),
                    Value::String("http://127.0.0.1".to_string()),
                ),
            ]),
            resolved_parameters: HashMap::new(),
            sensitive_output_values: Vec::new(),
            created_at: OffsetDateTime::now_utc(),
        };
        let invalid_url = OperationRequest {
            operation_id: Uuid::new_v4(),
            operation_type: OperationType::HttpRequest,
            description: "invalid url".to_string(),
            parameters: HashMap::from([
                ("method".to_string(), Value::String("GET".to_string())),
                ("url".to_string(), Value::String("://bad url".to_string())),
            ]),
            resolved_parameters: HashMap::new(),
            sensitive_output_values: Vec::new(),
            created_at: OffsetDateTime::now_utc(),
        };

        let method_error = session
            .execute_in_sandbox(&invalid_method)
            .await
            .expect_err("invalid method should fail");
        let url_error = session
            .execute_in_sandbox(&invalid_url)
            .await
            .expect_err("invalid url should fail");

        assert!(
            method_error
                .to_string()
                .contains("invalid_request: invalid method")
        );
        assert!(
            url_error
                .to_string()
                .contains("invalid_request: invalid url")
        );
    }

    #[tokio::test]
    async fn test_execute_operation_retries_after_recoverable_resource_failure() {
        let repository = Arc::new(MockSessionRepository::default());
        let recovery = Arc::new(MockRecoveryCoordinator {
            calls: AtomicUsize::new(0),
            results: TokioMutex::new(vec![Some(RecoveredSandboxResource {
                kind: RecoveredSandboxResourceKind::WarmInstance,
                sandbox_id: SandboxId::new(),
                session_id: None,
            })]),
        });
        let executor = Arc::new(MockOperationExecutor {
            attempts: AtomicUsize::new(0),
            results: TokioMutex::new(vec![
                Err(SandboxError::Other(
                    "spawn /usr/local/bin/lightpanda EAGAIN".to_string(),
                )),
                Ok(SandboxExecutionOutput {
                    data: Some(json!({ "ok": true })),
                }),
            ]),
        });
        let mut session = create_test_session();
        session.repository = Some(repository.clone());
        session.set_resource_recovery(recovery.clone());
        session.set_operation_executor(executor.clone());

        let result = session
            .execute_operation_for_test(create_test_operation())
            .await
            .expect("operation execution should complete");

        assert!(result.success);
        assert_eq!(result.data, Some(json!({ "ok": true })));
        assert_eq!(executor.attempts.load(Ordering::SeqCst), 2);
        assert_eq!(recovery.calls.load(Ordering::SeqCst), 1);
        assert_eq!(repository.create_operation_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            repository.complete_operation_calls.load(Ordering::SeqCst),
            1
        );
        let history = session.get_operation_history().await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, OperationStatus::Completed);
    }

    #[tokio::test]
    async fn test_execute_operation_stops_retry_when_recovery_reclaims_nothing() {
        let recovery = Arc::new(MockRecoveryCoordinator {
            calls: AtomicUsize::new(0),
            results: TokioMutex::new(vec![None]),
        });
        let executor = Arc::new(MockOperationExecutor {
            attempts: AtomicUsize::new(0),
            results: TokioMutex::new(vec![Err(SandboxError::Other(
                "lightpanda failed with SystemResources".to_string(),
            ))]),
        });
        let mut session = create_test_session();
        session.set_resource_recovery(recovery.clone());
        session.set_operation_executor(executor.clone());

        let result = session
            .execute_operation_for_test(create_test_operation())
            .await
            .expect("operation execution should complete");

        assert!(!result.success);
        assert!(
            result
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("SystemResources")
        );
        assert_eq!(executor.attempts.load(Ordering::SeqCst), 1);
        assert_eq!(recovery.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_execute_operation_does_not_retry_non_resource_failures() {
        let recovery = Arc::new(MockRecoveryCoordinator {
            calls: AtomicUsize::new(0),
            results: TokioMutex::new(Vec::new()),
        });
        let executor = Arc::new(MockOperationExecutor {
            attempts: AtomicUsize::new(0),
            results: TokioMutex::new(vec![Err(SandboxError::Other(
                "selector not found".to_string(),
            ))]),
        });
        let mut session = create_test_session();
        session.set_resource_recovery(recovery.clone());
        session.set_operation_executor(executor.clone());

        let result = session
            .execute_operation_for_test(create_test_operation())
            .await
            .expect("operation execution should complete");

        assert!(!result.success);
        assert_eq!(executor.attempts.load(Ordering::SeqCst), 1);
        assert_eq!(recovery.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_execute_operation_timeout_resets_session_status() {
        let executor = Arc::new(MockOperationExecutor {
            attempts: AtomicUsize::new(0),
            results: TokioMutex::new(vec![Err(SandboxError::Timeout {
                operation: "navigate".to_string(),
            })]),
        });
        let mut session = create_test_session();
        session.set_operation_executor(executor);

        let result = session
            .execute_operation_for_test(create_test_operation())
            .await
            .expect("operation timeout should be recorded as a failed result");

        assert!(!result.success);
        assert!(matches!(session.status().await, SessionStatus::Ready));
        let history = session.get_operation_history().await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, OperationStatus::Failed);
    }

    #[test]
    fn test_session_creation() {
        let session = create_test_session();
        assert!(!session.is_expired());
    }
}
