//! 沙箱会话管理

use crate::crypto::hkdf::KeyHierarchy;
use crate::crypto::{CredentialCryptoContext, EncryptedBlob};
use crate::models::CredentialType;
use crate::tee::SharedEnclave;
use crate::tee::sandbox::{
    browser_runtime::SandboxBrowserRuntime,
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
use std::collections::{HashMap, HashSet};
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

#[derive(Clone)]
pub struct ActiveNsjailSession {
    pub id: SessionId,
    context: SessionContext,
    status: Arc<RwLock<SessionStatus>>,
    sandbox: Arc<RwLock<Option<NsjailSandbox>>>,
    operation_history: Arc<RwLock<Vec<OperationRecord>>>,
    operation_reviewer: Option<Arc<OperationReviewer>>,
    repository: Option<Arc<dyn SandboxRepository>>,
    vault: Option<Arc<CredentialVault>>,
    key_hierarchy: Option<Arc<RwLock<KeyHierarchy>>>,
    enclave: Option<SharedEnclave>,
    browser_runtime: Arc<RwLock<Option<SandboxBrowserRuntime>>>,
    credential_cache: Arc<RwLock<Option<Arc<SessionCredentialMaterial>>>>,
    sensitive_selectors: Arc<RwLock<HashSet<String>>>,
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

#[derive(Debug)]
struct SessionCredentialMaterial {
    values: HashMap<String, Zeroizing<String>>,
}

#[derive(Debug)]
struct SandboxExecutionOutput {
    data: Option<Value>,
}

const DEFAULT_HTTP_REQUEST_TIMEOUT_MS: u64 = 30_000;
const MAX_HTTP_RESPONSE_BODY_BYTES: usize = 64 * 1024;
const DEFAULT_DOM_EXPORT_MAX_BYTES: u64 = 262_144;
const EXECUTE_SCRIPT_BINDINGS_ERROR: &str =
    "invalid_request: execute_script bindings must be plain strings";

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
            browser_runtime: Arc::new(RwLock::new(None)),
            credential_cache: Arc::new(RwLock::new(None)),
            sensitive_selectors: Arc::new(RwLock::new(HashSet::new())),
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

    async fn can_execute(&self) -> Result<(), SessionError> {
        let status = *self.status.read().await;
        if !status.can_execute() {
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

    async fn shutdown_runtime(&self) {
        if let Some(runtime) = self.browser_runtime.write().await.take() {
            runtime.close().await;
        }
        *self.credential_cache.write().await = None;
        self.sensitive_selectors.write().await.clear();
    }

    async fn browser_runtime(&self) -> Result<SandboxBrowserRuntime, SandboxError> {
        if let Some(runtime) = self.browser_runtime.read().await.as_ref().cloned() {
            return Ok(runtime);
        }

        let runtime = {
            let sandbox_guard = self.sandbox.read().await;
            let sandbox = sandbox_guard
                .as_ref()
                .ok_or_else(|| SessionError::CreationFailed {
                    reason: "Sandbox not available".to_string(),
                })?;
            let work_dir = sandbox.working_dir();
            SandboxBrowserRuntime::launch(work_dir, sandbox).await?
        };
        let mut guard = self.browser_runtime.write().await;
        if guard.is_none() {
            *guard = Some(runtime.clone());
        }
        Ok(guard.as_ref().cloned().unwrap_or(runtime))
    }

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
        });
        *self.credential_cache.write().await = Some(material.clone());
        Ok(material)
    }

    async fn mark_sensitive_selector(&self, selector: String) {
        self.sensitive_selectors.write().await.insert(selector);
    }

    async fn sensitive_selectors(&self) -> Vec<String> {
        self.sensitive_selectors
            .read()
            .await
            .iter()
            .cloned()
            .collect()
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

    fn optional_bool(parameters: &HashMap<String, Value>, key: &str, default: bool) -> bool {
        parameters
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(default)
    }

    fn optional_string(parameters: &HashMap<String, Value>, key: &str, default: &str) -> String {
        parameters
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| default.to_string())
    }

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

    async fn resolve_fill_value(&self, value: &Value) -> Result<(String, bool), SandboxError> {
        if let Some(raw) = value.as_str() {
            return Ok((raw.to_string(), false));
        }

        let field = value
            .get("$credential")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                SandboxError::Other(
                    "invalid_request: fill.value must be a string or {$credential: <field>}"
                        .to_string(),
                )
            })?;

        let material = self.credential_material().await?;
        material
            .values
            .get(field)
            .map(|value| (value.as_str().to_string(), true))
            .ok_or_else(|| {
                SandboxError::Other(format!(
                    "invalid_request: unsupported credential field {field}"
                ))
            })
    }

    async fn selector_is_sensitive(&self, selector: &str) -> bool {
        if self.sensitive_selectors.read().await.contains(selector) {
            return true;
        }

        let normalized = selector.to_ascii_lowercase();
        [
            "password", "passwd", "token", "secret", "api-key", "apikey", "cookie",
        ]
        .iter()
        .any(|needle| normalized.contains(needle))
    }

    async fn safe_get_text(
        &self,
        browser: &SandboxBrowserRuntime,
        selector: &str,
    ) -> Result<(String, bool), SandboxError> {
        if self.selector_is_sensitive(selector).await {
            return Err(SandboxError::Other(format!(
                "invalid_request: get_text is blocked for sensitive selector {selector}"
            )));
        }

        let meta = browser.selector_text_metadata(selector).await?;
        if !meta.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            return Err(SandboxError::Other(format!(
                "selector_not_found: {selector}"
            )));
        }

        if meta
            .get("type")
            .and_then(Value::as_str)
            .map(|kind| kind.eq_ignore_ascii_case("password"))
            .unwrap_or(false)
        {
            return Err(SandboxError::Other(format!(
                "invalid_request: get_text is blocked for password field {selector}"
            )));
        }

        let raw = meta
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let redacted = redact_sensitive_text(raw);
        Ok((redacted.clone(), redacted != raw))
    }

    fn sanitized_operation_parameters(parameters: &HashMap<String, Value>) -> Value {
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
            OperationType::Navigate => {
                let browser = self.browser_runtime().await?;
                let final_url = browser
                    .navigate(&Self::required_string(parameters, "url")?)
                    .await?;
                Ok(SandboxExecutionOutput {
                    data: Some(json!({ "final_url": final_url })),
                })
            }
            OperationType::Click => {
                let browser = self.browser_runtime().await?;
                let selector = Self::required_string(parameters, "selector")?;
                browser.click(&selector).await?;
                Ok(SandboxExecutionOutput {
                    data: Some(json!({ "clicked": true, "selector": selector })),
                })
            }
            OperationType::Fill => {
                let browser = self.browser_runtime().await?;
                let selector = Self::required_string(parameters, "selector")?;
                let raw_value = parameters.get("value").ok_or_else(|| {
                    SandboxError::Other("invalid_request: fill requires value".to_string())
                })?;
                let (resolved, credential_backed) = self.resolve_fill_value(raw_value).await?;
                let sensitive = parameters
                    .get("sensitive")
                    .and_then(Value::as_bool)
                    .unwrap_or_else(|| {
                        credential_backed
                            || operation
                                .parameters
                                .get("value")
                                .and_then(|value| value.get("$credential"))
                                .is_some()
                    });
                if sensitive {
                    self.mark_sensitive_selector(selector.clone()).await;
                }
                browser.fill(&selector, &resolved).await?;
                Ok(SandboxExecutionOutput {
                    data: Some(json!({
                        "filled": true,
                        "selector": selector,
                        "sensitive": sensitive
                    })),
                })
            }
            OperationType::Wait => {
                let browser = self.browser_runtime().await?;
                let selector = parameters
                    .get("selector")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                let timeout_ms = Self::optional_u64(parameters, "timeout_ms")
                    .or_else(|| Self::optional_u64(parameters, "milliseconds"))
                    .unwrap_or(5_000);
                browser
                    .wait_for_selector(selector.as_deref(), timeout_ms)
                    .await?;
                Ok(SandboxExecutionOutput {
                    data: Some(json!({
                        "waited": true,
                        "selector": selector,
                        "timeout_ms": timeout_ms
                    })),
                })
            }
            OperationType::GetText => {
                let browser = self.browser_runtime().await?;
                let selector = Self::required_string(parameters, "selector")?;
                let (text, redacted) = self.safe_get_text(&browser, &selector).await?;
                Ok(SandboxExecutionOutput {
                    data: Some(json!({
                        "selector": selector,
                        "text": text,
                        "redacted": redacted
                    })),
                })
            }
            OperationType::Export => {
                let browser = self.browser_runtime().await?;
                let selectors = parameters
                    .get("selectors")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(ToOwned::to_owned)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let mut items = Vec::new();
                for selector in selectors {
                    match self.safe_get_text(&browser, &selector).await {
                        Ok((text, redacted)) => items.push(json!({
                            "selector": selector,
                            "text": text,
                            "redacted": redacted
                        })),
                        Err(_) => items.push(json!({
                            "selector": selector,
                            "text": "********",
                            "redacted": true
                        })),
                    }
                }
                Ok(SandboxExecutionOutput {
                    data: Some(json!({
                        "url": browser.current_url().await?,
                        "items": items
                    })),
                })
            }
            OperationType::DomExport => {
                let browser = self.browser_runtime().await?;
                let root_selector = Self::optional_string(parameters, "root_selector", "html");
                let format = Self::optional_string(parameters, "format", "html");
                validate_dom_export_format(&format)?;
                let include_text = Self::optional_bool(parameters, "include_text", true);
                let include_metadata = Self::optional_bool(parameters, "include_metadata", false);
                let max_bytes = Self::optional_u64(parameters, "max_bytes")
                    .unwrap_or(DEFAULT_DOM_EXPORT_MAX_BYTES);
                if max_bytes == 0 {
                    return Err(SandboxError::Other(
                        "invalid_request: max_bytes must be greater than 0".to_string(),
                    ));
                }

                let mut sensitive_selectors = self.sensitive_selectors().await;
                sensitive_selectors.extend(Self::optional_string_array(
                    parameters,
                    "extra_sensitive_selectors",
                )?);

                let data = browser
                    .dom_export(
                        &root_selector,
                        &format,
                        include_text,
                        include_metadata,
                        &sensitive_selectors,
                    )
                    .await?;
                let data = redact_dom_export_result(data);
                Ok(SandboxExecutionOutput {
                    data: Some(truncate_dom_export_data(data, max_bytes)),
                })
            }
            OperationType::ExecuteScript => {
                let browser = self.browser_runtime().await?;
                let script = Self::required_string(parameters, "script")?;
                let bindings = Self::resolve_execute_script_bindings(parameters)?;
                let result = browser.execute_script(&script, &bindings).await?;
                Ok(SandboxExecutionOutput { data: Some(result) })
            }
            OperationType::HttpRequest => self.execute_http_request(parameters).await,
            OperationType::Custom => Err(SandboxError::Other(
                "invalid_request: custom sandbox operations are not supported".to_string(),
            )),
        }
    }

    fn resolve_execute_script_bindings(
        parameters: &HashMap<String, Value>,
    ) -> Result<HashMap<String, String>, SandboxError> {
        let Some(raw_bindings) = parameters
            .get("bindings")
            .or_else(|| parameters.get("credential_bindings"))
        else {
            return Ok(HashMap::new());
        };

        let Some(bindings) = raw_bindings.as_object() else {
            return Err(SandboxError::Other(
                EXECUTE_SCRIPT_BINDINGS_ERROR.to_string(),
            ));
        };

        let mut resolved = HashMap::with_capacity(bindings.len());

        for (name, raw_value) in bindings {
            let Some(raw) = raw_value.as_str() else {
                return Err(SandboxError::Other(
                    EXECUTE_SCRIPT_BINDINGS_ERROR.to_string(),
                ));
            };
            resolved.insert(name.clone(), raw.to_string());
        }

        Ok(resolved)
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
        self.can_execute().await.map_err(SandboxError::Session)?;
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
            repository
                .create_operation(NewSandboxOperationRecord {
                    operation_id: operation.operation_id,
                    session_id: self.context.session_id,
                    tenant_id: self.context.tenant_id,
                    credential_id: self.context.credential_id,
                    operation_type: operation.operation_type.to_string(),
                    input_params: Self::sanitized_operation_parameters(&operation.parameters),
                    started_at: to_chrono_utc(start_time),
                })
                .await?;
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
                        }
                        return Err(SessionError::OperationRejected { reason }.into());
                    }
                },
                Err(error) if reviewer.is_strict_mode() => {
                    *self.status.write().await = SessionStatus::Ready;
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

        let result = match self.execute_in_sandbox(&operation).await {
            Ok(output) => {
                let execution_time_ms =
                    (OffsetDateTime::now_utc() - start_time).whole_milliseconds() as u64;
                if let Some(record) = self.operation_history.write().await.last_mut() {
                    record.status = OperationStatus::Completed;
                    record.completed_at = Some(OffsetDateTime::now_utc());
                    record.execution_time_ms = Some(execution_time_ms);
                }
                ExecutionResult {
                    success: true,
                    data: output.data,
                    error: None,
                    execution_time_ms,
                    audit_log: vec![],
                }
            }
            Err(error) => {
                let execution_time_ms =
                    (OffsetDateTime::now_utc() - start_time).whole_milliseconds() as u64;
                if let Some(record) = self.operation_history.write().await.last_mut() {
                    record.status = OperationStatus::Failed;
                    record.completed_at = Some(OffsetDateTime::now_utc());
                    record.execution_time_ms = Some(execution_time_ms);
                }
                ExecutionResult {
                    success: false,
                    data: None,
                    error: Some(error.to_string()),
                    execution_time_ms,
                    audit_log: vec![],
                }
            }
        };

        if let Some(repository) = &self.repository {
            repository
                .complete_operation(CompleteSandboxOperationRecord {
                    operation_id: operation.operation_id,
                    status: if result.success {
                        "completed"
                    } else {
                        "failed"
                    },
                    output_result: result.data.clone(),
                    error_message: result.error.clone(),
                    completed_at: chrono::Utc::now(),
                    execution_duration_ms: i32::try_from(result.execution_time_ms)
                        .unwrap_or(i32::MAX),
                })
                .await?;
        }

        *self.status.write().await = SessionStatus::Ready;
        Ok(result)
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

fn validate_dom_export_format(format: &str) -> Result<(), SandboxError> {
    match format {
        "html" | "text" | "json" => Ok(()),
        _ => Err(SandboxError::Other(format!(
            "invalid_request: unsupported dom export format {format}"
        ))),
    }
}

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

fn serialized_len(value: &Value) -> usize {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
}

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
    let allowed_fields: &[&str] = match credential_type {
        CredentialType::UsernamePassword => &["username", "password"],
        CredentialType::ApiKey => &["api_key"],
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
    use crate::tee::sandbox::types::SandboxId;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    fn create_test_session() -> ActiveNsjailSession {
        let sandbox = NsjailSandbox::new(crate::tee::sandbox::config::NsjailConfig {
            sandbox: SandboxConfig::default(),
            command: vec!["sleep".to_string(), "60".to_string()],
            cwd: std::path::PathBuf::from("/"),
            env: HashMap::new(),
            disable_seccomp_for_browser_runtime: false,
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
    fn test_resolve_execute_script_bindings_accepts_plain_strings() {
        let parameters = HashMap::from([(
            "bindings".to_string(),
            json!({
                "username": "alice",
                "otp": "123456"
            }),
        )]);

        let bindings = ActiveNsjailSession::resolve_execute_script_bindings(&parameters)
            .expect("plain string bindings should be accepted");

        assert_eq!(bindings.get("username").map(String::as_str), Some("alice"));
        assert_eq!(bindings.get("otp").map(String::as_str), Some("123456"));
    }

    #[test]
    fn test_resolve_execute_script_bindings_rejects_credential_reference_objects() {
        let parameters = HashMap::from([(
            "bindings".to_string(),
            json!({
                "password": { "$credential": "password" }
            }),
        )]);

        let error = ActiveNsjailSession::resolve_execute_script_bindings(&parameters)
            .expect_err("credential reference bindings must be rejected");

        match error {
            SandboxError::Other(message) => assert_eq!(message, EXECUTE_SCRIPT_BINDINGS_ERROR),
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[test]
    fn test_resolve_execute_script_bindings_rejects_non_string_objects() {
        let parameters = HashMap::from([(
            "bindings".to_string(),
            json!({
                "payload": { "nested": "value" }
            }),
        )]);

        let error = ActiveNsjailSession::resolve_execute_script_bindings(&parameters)
            .expect_err("non-string bindings must be rejected");

        match error {
            SandboxError::Other(message) => assert_eq!(message, EXECUTE_SCRIPT_BINDINGS_ERROR),
            other => panic!("unexpected error variant: {other}"),
        }
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
    async fn test_http_request_does_not_initialize_browser_runtime() {
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
        assert!(session.browser_runtime.read().await.is_none());
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
        assert!(session.browser_runtime.read().await.is_none());
    }

    #[test]
    fn test_session_creation() {
        let session = create_test_session();
        assert!(!session.is_expired());
    }
}
