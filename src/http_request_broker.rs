use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::{
    Client, Method, Url,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use serde_json::Value;
use sqlx::Row;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::VaultEntry;
use crate::crypto::hkdf::KeyHierarchy;
use crate::crypto::{CredentialCryptoContext, EncryptedBlob};
use crate::tee::SharedEnclave;
use crate::tee::sandbox::domain_policy::ensure_url_allowed;
use crate::tee::sandbox::error::SandboxError;
use crate::tee::sandbox::error::SessionError;
use crate::tee::sandbox::http_template::{
    HttpTemplateCredentialMaterial, render_http_request_parameters_with_policy,
};
use crate::vault::models::{CredentialId, TenantId, UserId, VaultError};
use crate::vault::storage::CredentialVault;

const DEFAULT_HTTP_REQUEST_TIMEOUT_MS: u64 = 30_000;
const MAX_HTTP_RESPONSE_BODY_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct HttpRequestBrokerSubmitInput {
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub credential_id: Uuid,
    pub description: String,
    pub parameters: HashMap<String, Value>,
}

#[derive(Debug, Clone)]
pub struct HttpRequestBrokerSubmitResult {
    pub operation_id: Uuid,
    pub success: bool,
    pub data: Option<Value>,
    pub error: Option<String>,
    pub execution_time_ms: u64,
}

#[derive(Debug, Clone)]
pub struct HttpRequestOperationRecord {
    pub operation_id: Uuid,
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub credential_id: Uuid,
    pub description: String,
    pub operation_type: String,
    pub status: String,
    pub request_parameters: Value,
    pub response_data: Option<Value>,
    pub error_message: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub execution_duration_ms: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct NewHttpRequestOperationRecord {
    pub operation_id: Uuid,
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub credential_id: Uuid,
    pub description: String,
    pub request_parameters: Value,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CompleteHttpRequestOperationRecord {
    pub operation_id: Uuid,
    pub status: &'static str,
    pub response_data: Option<Value>,
    pub error_message: Option<String>,
    pub completed_at: DateTime<Utc>,
    pub execution_duration_ms: i32,
}

#[async_trait]
pub trait HttpRequestCredentialResolver: Send + Sync {
    async fn resolve(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        credential_id: Uuid,
    ) -> Result<HttpTemplateCredentialMaterial, SandboxError>;
}

#[async_trait]
pub trait HttpRequestTargetResolver: Send + Sync {
    async fn resolve(&self, host: &str, port: u16) -> Result<Vec<IpAddr>, SandboxError>;
}

struct TokioHttpRequestTargetResolver;

#[async_trait]
impl HttpRequestTargetResolver for TokioHttpRequestTargetResolver {
    async fn resolve(&self, host: &str, port: u16) -> Result<Vec<IpAddr>, SandboxError> {
        let mut resolved_ips = Vec::new();
        for resolved in tokio::net::lookup_host((host, port))
            .await
            .map_err(|error| {
                SandboxError::Other(format!("dns_resolution_failed: {host}:{port}: {error}"))
            })?
        {
            let ip = resolved.ip();
            if !resolved_ips.contains(&ip) {
                resolved_ips.push(ip);
            }
        }

        Ok(resolved_ips)
    }
}

#[async_trait]
pub trait HttpRequestTransport: Send + Sync {
    async fn execute(
        &self,
        parameters: &HashMap<String, Value>,
        sensitive_output_values: &[String],
    ) -> Result<Value, SandboxError>;
}

#[async_trait]
pub trait HttpRequestOperationStore: Send + Sync {
    async fn create_operation(
        &self,
        record: NewHttpRequestOperationRecord,
    ) -> Result<(), SandboxError>;

    async fn complete_operation(
        &self,
        record: CompleteHttpRequestOperationRecord,
    ) -> Result<(), SandboxError>;

    async fn get_operation_by_id(
        &self,
        tenant_id: Uuid,
        operation_id: Uuid,
    ) -> Result<Option<HttpRequestOperationRecord>, SandboxError>;
}

#[async_trait]
pub trait HttpRequestBroker: Send + Sync {
    async fn submit(
        &self,
        input: HttpRequestBrokerSubmitInput,
    ) -> Result<HttpRequestBrokerSubmitResult, SandboxError>;

    async fn get_operation(
        &self,
        tenant_id: Uuid,
        operation_id: Uuid,
    ) -> Result<Option<HttpRequestOperationRecord>, SandboxError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct HttpRequestValidationPolicy {
    pub allow_insecure_http: bool,
}

pub struct PersistentHttpRequestBroker {
    resolver: Arc<dyn HttpRequestCredentialResolver>,
    target_resolver: Arc<dyn HttpRequestTargetResolver>,
    validation_policy: HttpRequestValidationPolicy,
    transport: Arc<dyn HttpRequestTransport>,
    store: Arc<dyn HttpRequestOperationStore>,
}

impl PersistentHttpRequestBroker {
    pub fn new(
        resolver: Arc<dyn HttpRequestCredentialResolver>,
        transport: Arc<dyn HttpRequestTransport>,
        store: Arc<dyn HttpRequestOperationStore>,
    ) -> Self {
        Self::new_with_target_resolver_and_policy(
            resolver,
            Arc::new(TokioHttpRequestTargetResolver),
            HttpRequestValidationPolicy::default(),
            transport,
            store,
        )
    }

    pub fn new_with_target_resolver(
        resolver: Arc<dyn HttpRequestCredentialResolver>,
        target_resolver: Arc<dyn HttpRequestTargetResolver>,
        transport: Arc<dyn HttpRequestTransport>,
        store: Arc<dyn HttpRequestOperationStore>,
    ) -> Self {
        Self::new_with_target_resolver_and_policy(
            resolver,
            target_resolver,
            HttpRequestValidationPolicy::default(),
            transport,
            store,
        )
    }

    pub fn new_with_target_resolver_and_policy(
        resolver: Arc<dyn HttpRequestCredentialResolver>,
        target_resolver: Arc<dyn HttpRequestTargetResolver>,
        validation_policy: HttpRequestValidationPolicy,
        transport: Arc<dyn HttpRequestTransport>,
        store: Arc<dyn HttpRequestOperationStore>,
    ) -> Self {
        Self {
            resolver,
            target_resolver,
            validation_policy,
            transport,
            store,
        }
    }
}

#[derive(Clone)]
pub struct PgHttpRequestOperationStore {
    pool: sqlx::PgPool,
}

impl PgHttpRequestOperationStore {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HttpRequestOperationStore for PgHttpRequestOperationStore {
    async fn create_operation(
        &self,
        record: NewHttpRequestOperationRecord,
    ) -> Result<(), SandboxError> {
        sqlx::query(
            r#"
            INSERT INTO http_request_operations (
                id,
                tenant_id,
                created_by,
                credential_id,
                description,
                operation_type,
                status,
                request_parameters,
                started_at
            )
            VALUES ($1, $2, $3, $4, $5, 'http_request', 'running', $6, $7)
            "#,
        )
        .bind(record.operation_id)
        .bind(record.tenant_id)
        .bind(record.user_id)
        .bind(record.credential_id)
        .bind(record.description)
        .bind(record.request_parameters)
        .bind(record.started_at)
        .execute(&self.pool)
        .await
        .map_err(|error| {
            SandboxError::Pool(format!("persist http request operation failed: {error}"))
        })?;
        Ok(())
    }

    async fn complete_operation(
        &self,
        record: CompleteHttpRequestOperationRecord,
    ) -> Result<(), SandboxError> {
        sqlx::query(
            r#"
            UPDATE http_request_operations
            SET status = $2,
                response_data = COALESCE(response_data, $3),
                error_message = COALESCE(error_message, $4),
                completed_at = COALESCE(completed_at, $5),
                execution_duration_ms = COALESCE(execution_duration_ms, $6),
                updated_at = NOW()
            WHERE id = $1
              AND status = 'running'
            "#,
        )
        .bind(record.operation_id)
        .bind(record.status)
        .bind(record.response_data)
        .bind(record.error_message)
        .bind(record.completed_at)
        .bind(record.execution_duration_ms)
        .execute(&self.pool)
        .await
        .map_err(|error| {
            SandboxError::Pool(format!("update http request operation failed: {error}"))
        })?;
        Ok(())
    }

    async fn get_operation_by_id(
        &self,
        tenant_id: Uuid,
        operation_id: Uuid,
    ) -> Result<Option<HttpRequestOperationRecord>, SandboxError> {
        let row = sqlx::query(
            r#"
            SELECT
                id,
                tenant_id,
                created_by,
                credential_id,
                description,
                operation_type,
                status,
                request_parameters,
                response_data,
                error_message,
                started_at,
                completed_at,
                execution_duration_ms
            FROM http_request_operations
            WHERE tenant_id = $1
              AND id = $2
            "#,
        )
        .bind(tenant_id)
        .bind(operation_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| {
            SandboxError::Pool(format!("load http request operation failed: {error}"))
        })?;

        row.map(map_http_request_operation_row).transpose()
    }
}

pub struct VaultHttpRequestCredentialResolver {
    vault: Arc<CredentialVault>,
    key_hierarchy: Arc<RwLock<KeyHierarchy>>,
    enclave: Option<SharedEnclave>,
}

impl VaultHttpRequestCredentialResolver {
    pub fn new(
        vault: Arc<CredentialVault>,
        key_hierarchy: Arc<RwLock<KeyHierarchy>>,
        enclave: Option<SharedEnclave>,
    ) -> Self {
        Self {
            vault,
            key_hierarchy,
            enclave,
        }
    }
}

#[async_trait]
impl HttpRequestCredentialResolver for VaultHttpRequestCredentialResolver {
    async fn resolve(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        credential_id: Uuid,
    ) -> Result<HttpTemplateCredentialMaterial, SandboxError> {
        let credential_id_model = CredentialId::from_string(credential_id.to_string())
            .map_err(|error| SandboxError::Other(format!("invalid credential id: {error}")))?;
        let tenant_id = TenantId::new(tenant_id.to_string());
        let user_id = UserId::new(user_id.to_string());

        let entry = match self
            .vault
            .get_credential(&credential_id_model, &tenant_id, &user_id)
        {
            Ok(Some(entry)) => entry,
            Ok(None) | Err(VaultError::TenantIsolationViolation { .. }) => {
                return Err(SandboxError::Session(SessionError::credential_not_found(
                    credential_id,
                )));
            }
            Err(error) => {
                return Err(SandboxError::Other(format!(
                    "failed to load credential for http request broker: {error}"
                )));
            }
        };

        decrypt_credential_material(&self.key_hierarchy, self.enclave.as_ref(), &entry).await
    }
}

#[derive(Clone, Default)]
pub struct ReqwestHttpRequestTransport;

impl ReqwestHttpRequestTransport {
    pub fn new() -> Self {
        Self
    }

    fn required_string<'a>(
        parameters: &'a HashMap<String, Value>,
        key: &str,
    ) -> Result<&'a str, SandboxError> {
        parameters
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| SandboxError::Other(format!("invalid_request: missing {key}")))
    }

    fn optional_u64(parameters: &HashMap<String, Value>, key: &str) -> Option<u64> {
        parameters.get(key).and_then(Value::as_u64)
    }

    fn required_http_method(parameters: &HashMap<String, Value>) -> Result<Method, SandboxError> {
        let method = Self::required_string(parameters, "method")?;
        Method::from_bytes(method.to_ascii_uppercase().as_bytes()).map_err(|_| {
            SandboxError::Other(format!("invalid_request: invalid method {}", method.trim()))
        })
    }

    fn required_http_url(parameters: &HashMap<String, Value>) -> Result<Url, SandboxError> {
        let url = Self::required_string(parameters, "url")?;
        Url::parse(url)
            .map_err(|_| SandboxError::Other(format!("invalid_request: invalid url {url}")))
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
}

#[async_trait]
impl HttpRequestTransport for ReqwestHttpRequestTransport {
    async fn execute(
        &self,
        parameters: &HashMap<String, Value>,
        sensitive_output_values: &[String],
    ) -> Result<Value, SandboxError> {
        let method = Self::required_http_method(parameters)?;
        let url = Self::required_http_url(parameters)?;
        let timeout_ms =
            Self::optional_u64(parameters, "timeout_ms").unwrap_or(DEFAULT_HTTP_REQUEST_TIMEOUT_MS);
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| {
                SandboxError::Other(format!(
                    "http_request_failed: failed to build client: {error}"
                ))
            })?;

        let mut request = client
            .request(method, url)
            .timeout(Duration::from_millis(timeout_ms));
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
            .map_err(|error| classify_transport_error(error, timeout_ms))?;
        if response.status().is_redirection() {
            return Err(SandboxError::Other(format!(
                "redirect_not_allowed: upstream responded with HTTP {}",
                response.status()
            )));
        }
        let status = response.status().as_u16();
        let final_url = response.url().to_string();

        let headers = response
            .headers()
            .iter()
            .map(|(key, value)| {
                let header_text = if is_sensitive_http_key(key.as_str()) {
                    "[REDACTED]".to_string()
                } else {
                    redact_http_response_text(
                        value.to_str().unwrap_or_default(),
                        sensitive_output_values,
                    )
                };
                (key.as_str().to_string(), Value::String(header_text))
            })
            .collect::<serde_json::Map<String, Value>>();

        let mut body = Vec::new();
        let mut truncated = false;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| classify_transport_error(error, timeout_ms))?
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

        Ok(serde_json::json!({
            "status": status,
            "headers": Value::Object(headers),
            "body": response_body,
            "url": final_url,
            "truncated": truncated
        }))
    }
}

#[async_trait]
impl HttpRequestBroker for PersistentHttpRequestBroker {
    async fn submit(
        &self,
        input: HttpRequestBrokerSubmitInput,
    ) -> Result<HttpRequestBrokerSubmitResult, SandboxError> {
        let material = self
            .resolver
            .resolve(input.tenant_id, input.user_id, input.credential_id)
            .await?;
        let rendered =
            render_http_request_parameters_with_policy(&input.parameters, &material, false).await?;
        let operation_id = Uuid::new_v4();
        let started_at = Utc::now();
        let persisted_request = redact_http_request_parameters(
            &input.parameters,
            &rendered.parameters,
            &rendered.sensitive_values,
        );

        self.store
            .create_operation(NewHttpRequestOperationRecord {
                operation_id,
                tenant_id: input.tenant_id,
                user_id: input.user_id,
                credential_id: input.credential_id,
                description: input.description,
                request_parameters: persisted_request,
                started_at,
            })
            .await?;

        if let Err(error) = validate_broker_http_request(
            &rendered.parameters,
            &material.allowed_domains,
            self.validation_policy,
            self.target_resolver.as_ref(),
        )
        .await
        {
            let completed_at = Utc::now();
            let execution_duration_ms =
                saturating_duration_ms(completed_at.signed_duration_since(started_at));
            let error_message = broker_error_string(&error);

            self.store
                .complete_operation(CompleteHttpRequestOperationRecord {
                    operation_id,
                    status: "failed",
                    response_data: None,
                    error_message: Some(error_message.clone()),
                    completed_at,
                    execution_duration_ms,
                })
                .await?;

            return Ok(HttpRequestBrokerSubmitResult {
                operation_id,
                success: false,
                data: None,
                error: Some(error_message),
                execution_time_ms: execution_duration_ms as u64,
            });
        }

        match self
            .transport
            .execute(&rendered.parameters, &rendered.sensitive_values)
            .await
        {
            Ok(data) => {
                let completed_at = Utc::now();
                let execution_duration_ms =
                    saturating_duration_ms(completed_at.signed_duration_since(started_at));
                let sanitized_data = redact_http_response_value(&data, &rendered.sensitive_values);

                self.store
                    .complete_operation(CompleteHttpRequestOperationRecord {
                        operation_id,
                        status: "completed",
                        response_data: Some(sanitized_data.clone()),
                        error_message: None,
                        completed_at,
                        execution_duration_ms,
                    })
                    .await?;

                Ok(HttpRequestBrokerSubmitResult {
                    operation_id,
                    success: true,
                    data: Some(sanitized_data),
                    error: None,
                    execution_time_ms: execution_duration_ms as u64,
                })
            }
            Err(error) => {
                let completed_at = Utc::now();
                let execution_duration_ms =
                    saturating_duration_ms(completed_at.signed_duration_since(started_at));
                let error_message = broker_error_string(&error);

                self.store
                    .complete_operation(CompleteHttpRequestOperationRecord {
                        operation_id,
                        status: "failed",
                        response_data: None,
                        error_message: Some(error_message.clone()),
                        completed_at,
                        execution_duration_ms,
                    })
                    .await?;

                Ok(HttpRequestBrokerSubmitResult {
                    operation_id,
                    success: false,
                    data: None,
                    error: Some(error_message),
                    execution_time_ms: execution_duration_ms as u64,
                })
            }
        }
    }

    async fn get_operation(
        &self,
        tenant_id: Uuid,
        operation_id: Uuid,
    ) -> Result<Option<HttpRequestOperationRecord>, SandboxError> {
        self.store
            .get_operation_by_id(tenant_id, operation_id)
            .await
    }
}

async fn validate_broker_http_request(
    parameters: &HashMap<String, Value>,
    allowed_domains: &[String],
    validation_policy: HttpRequestValidationPolicy,
    target_resolver: &dyn HttpRequestTargetResolver,
) -> Result<(), SandboxError> {
    if allowed_domains.is_empty() {
        return Err(SandboxError::Other(
            "forbidden: credential allowed_domains must not be empty".to_string(),
        ));
    }

    let url = ReqwestHttpRequestTransport::required_http_url(parameters)?;
    validate_http_request_target(&url, allowed_domains, validation_policy, target_resolver).await
}

async fn validate_http_request_target(
    url: &Url,
    allowed_domains: &[String],
    validation_policy: HttpRequestValidationPolicy,
    target_resolver: &dyn HttpRequestTargetResolver,
) -> Result<(), SandboxError> {
    if url.scheme() != "https" && !(validation_policy.allow_insecure_http && url.scheme() == "http")
    {
        return Err(SandboxError::Other(format!(
            "forbidden: url scheme is not allowed: {}",
            url.scheme()
        )));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(SandboxError::Other(
            "invalid_request: url must not contain embedded credentials".to_string(),
        ));
    }

    let host = url.host_str().ok_or_else(|| {
        SandboxError::Other("invalid_request: url must include a host".to_string())
    })?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| SandboxError::Other(format!("invalid_request: url missing port: {url}")))?;

    if host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .ok()
            .is_some_and(|ip| is_local_or_private_ip(&ip))
    {
        return Err(SandboxError::Other(
            "forbidden: local or private IP targets are not allowed".to_string(),
        ));
    }

    ensure_url_allowed(url, allowed_domains).map_err(|error| match error {
        SandboxError::Other(message) if message.contains("allowed_domains policy") => {
            SandboxError::Other(format!(
                "forbidden: url target is not in credential allowed_domains: {url}"
            ))
        }
        other => other,
    })?;

    if target_resolver
        .resolve(host, port)
        .await?
        .iter()
        .any(is_local_or_private_ip)
    {
        return Err(SandboxError::Other(
            "forbidden: local or private IP targets are not allowed".to_string(),
        ));
    }

    Ok(())
}

fn is_local_or_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_multicast()
                || ip.is_broadcast()
                || is_ipv4_documentation(ip)
                || ip.is_unspecified()
        }
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_multicast()
                || is_ipv6_documentation(ip)
        }
    }
}

fn is_ipv4_documentation(ip: &std::net::Ipv4Addr) -> bool {
    matches!(
        ip.octets(),
        [192, 0, 2, _] | [198, 51, 100, _] | [203, 0, 113, _]
    )
}

fn is_ipv6_documentation(ip: &std::net::Ipv6Addr) -> bool {
    let segments = ip.segments();
    segments[0] == 0x2001 && segments[1] == 0x0db8
}

fn classify_transport_error(error: reqwest::Error, timeout_ms: u64) -> SandboxError {
    if error.is_timeout() {
        return SandboxError::Other(format!(
            "timeout: request exceeded configured timeout of {timeout_ms}ms"
        ));
    }

    SandboxError::Other(format!("http_request_failed: {error}"))
}

fn broker_error_string(error: &SandboxError) -> String {
    match error {
        SandboxError::Config(message)
        | SandboxError::Process(message)
        | SandboxError::Pool(message)
        | SandboxError::Serialization(message)
        | SandboxError::Other(message) => message.clone(),
        SandboxError::Timeout { operation } => format!("timeout: {operation}"),
        _ => error.to_string(),
    }
}

fn redact_http_request_parameters(
    original: &HashMap<String, Value>,
    rendered: &HashMap<String, Value>,
    sensitive_values: &[String],
) -> Value {
    Value::Object(
        original
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    redact_http_request_value(
                        key,
                        Some(value),
                        rendered.get(key),
                        sensitive_values,
                    ),
                )
            })
            .collect(),
    )
}

fn redact_http_request_value(
    key: &str,
    original: Option<&Value>,
    resolved: Option<&Value>,
    sensitive_values: &[String],
) -> Value {
    let sensitive_key = is_sensitive_http_key(key);
    let credential_backed = original.is_some_and(is_credential_reference_value);

    match original {
        Some(Value::Object(original_map)) => Value::Object(
            original_map
                .iter()
                .map(|(nested_key, nested_value)| {
                    let resolved_nested = resolved
                        .and_then(Value::as_object)
                        .and_then(|map| map.get(nested_key));
                    (
                        nested_key.clone(),
                        redact_http_request_value(
                            nested_key,
                            Some(nested_value),
                            resolved_nested,
                            sensitive_values,
                        ),
                    )
                })
                .collect(),
        ),
        Some(Value::Array(values)) => Value::Array(
            values
                .iter()
                .enumerate()
                .map(|(index, nested_value)| {
                    let resolved_nested = resolved
                        .and_then(Value::as_array)
                        .and_then(|items| items.get(index));
                    redact_http_request_value(
                        key,
                        Some(nested_value),
                        resolved_nested,
                        sensitive_values,
                    )
                })
                .collect(),
        ),
        Some(Value::String(text)) if sensitive_key || credential_backed => {
            let redacted = redact_http_response_text(text, sensitive_values);
            Value::String(if redacted == text.as_str() {
                "[REDACTED]".to_string()
            } else {
                redacted
            })
        }
        Some(value) => value.clone(),
        None => Value::Null,
    }
}

fn redact_http_response_value(value: &Value, sensitive_output_values: &[String]) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, nested)| {
                    (
                        key.clone(),
                        redact_http_response_value(nested, sensitive_output_values),
                    )
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| redact_http_response_value(item, sensitive_output_values))
                .collect(),
        ),
        Value::String(text) => {
            Value::String(redact_http_response_text(text, sensitive_output_values))
        }
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
            replace_sensitive_with_boundaries(&redacted, sensitive)
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
        if !is_sensitive_match_boundary(text, start, end) {
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

    previous.is_none_or(is_sensitive_boundary_char) && next.is_none_or(is_sensitive_boundary_char)
}

fn is_sensitive_boundary_char(ch: char) -> bool {
    !(ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_sensitive_http_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase();
    normalized == "authorization"
        || normalized == "cookie"
        || normalized == "set-cookie"
        || normalized == "api_key"
        || normalized == "api-key"
        || normalized == "signature"
        || normalized == "ok-access-sign"
        || normalized == "secret"
        || normalized.contains("token")
}

fn is_credential_reference_value(value: &Value) -> bool {
    let Value::Object(map) = value else {
        return false;
    };
    map.get("$credential")
        .and_then(Value::as_str)
        .is_some_and(|field| !field.trim().is_empty())
}

fn saturating_duration_ms(duration: chrono::Duration) -> i32 {
    duration.num_milliseconds().clamp(0, i64::from(i32::MAX)) as i32
}

fn map_http_request_operation_row(
    row: sqlx::postgres::PgRow,
) -> Result<HttpRequestOperationRecord, SandboxError> {
    Ok(HttpRequestOperationRecord {
        operation_id: row
            .try_get("id")
            .map_err(|error| SandboxError::Pool(format!("decode operation id failed: {error}")))?,
        tenant_id: row
            .try_get("tenant_id")
            .map_err(|error| SandboxError::Pool(format!("decode tenant id failed: {error}")))?,
        user_id: row
            .try_get("created_by")
            .map_err(|error| SandboxError::Pool(format!("decode created_by failed: {error}")))?,
        credential_id: row
            .try_get("credential_id")
            .map_err(|error| SandboxError::Pool(format!("decode credential id failed: {error}")))?,
        description: row
            .try_get("description")
            .map_err(|error| SandboxError::Pool(format!("decode description failed: {error}")))?,
        operation_type: row.try_get("operation_type").map_err(|error| {
            SandboxError::Pool(format!("decode operation type failed: {error}"))
        })?,
        status: row
            .try_get("status")
            .map_err(|error| SandboxError::Pool(format!("decode status failed: {error}")))?,
        request_parameters: row.try_get("request_parameters").map_err(|error| {
            SandboxError::Pool(format!("decode request parameters failed: {error}"))
        })?,
        response_data: row
            .try_get("response_data")
            .map_err(|error| SandboxError::Pool(format!("decode response_data failed: {error}")))?,
        error_message: row
            .try_get("error_message")
            .map_err(|error| SandboxError::Pool(format!("decode error_message failed: {error}")))?,
        started_at: row
            .try_get("started_at")
            .map_err(|error| SandboxError::Pool(format!("decode started_at failed: {error}")))?,
        completed_at: row
            .try_get("completed_at")
            .map_err(|error| SandboxError::Pool(format!("decode completed_at failed: {error}")))?,
        execution_duration_ms: row.try_get("execution_duration_ms").map_err(|error| {
            SandboxError::Pool(format!("decode execution_duration_ms failed: {error}"))
        })?,
    })
}

async fn decrypt_credential_material(
    key_hierarchy: &Arc<RwLock<KeyHierarchy>>,
    enclave: Option<&SharedEnclave>,
    entry: &VaultEntry,
) -> Result<HttpTemplateCredentialMaterial, SandboxError> {
    let plaintext = decrypt_vault_entry(key_hierarchy, enclave, entry).await?;
    let plaintext_data: serde_json::Value =
        serde_json::from_slice(&plaintext).map_err(|error| {
            SandboxError::Other(format!("credential plaintext is not valid JSON: {error}"))
        })?;
    let values = crate::tee::sandbox::http_template::extract_supported_http_credential_fields(
        entry.credential_type,
        &plaintext_data,
    )?;

    Ok(HttpTemplateCredentialMaterial {
        credential_type: entry.credential_type,
        values,
        provider: entry.provider,
        allowed_domains: entry.allowed_domains.clone(),
        custom_functions: entry.custom_functions.clone(),
    })
}

async fn decrypt_vault_entry(
    key_hierarchy: &Arc<RwLock<KeyHierarchy>>,
    enclave: Option<&SharedEnclave>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CredentialType;
    use axum::Router;
    use axum::http::StatusCode;
    use axum::routing::get;
    use chrono::Utc;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::RwLock;

    struct FakeResolver;

    #[async_trait]
    impl HttpRequestCredentialResolver for FakeResolver {
        async fn resolve(
            &self,
            _tenant_id: Uuid,
            _user_id: Uuid,
            _credential_id: Uuid,
        ) -> Result<HttpTemplateCredentialMaterial, SandboxError> {
            Ok(HttpTemplateCredentialMaterial {
                credential_type: CredentialType::ApiKey,
                values: HashMap::from([
                    ("api_key".to_string(), "secret-api-key".to_string()),
                    ("api_secret".to_string(), "secret-api-secret".to_string()),
                ]),
                provider: None,
                allowed_domains: vec!["api.example.com".to_string()],
                custom_functions: Vec::new(),
            })
        }
    }

    struct ResolverWithAllowedDomains {
        allowed_domains: Vec<String>,
    }

    #[async_trait]
    impl HttpRequestCredentialResolver for ResolverWithAllowedDomains {
        async fn resolve(
            &self,
            _tenant_id: Uuid,
            _user_id: Uuid,
            _credential_id: Uuid,
        ) -> Result<HttpTemplateCredentialMaterial, SandboxError> {
            Ok(HttpTemplateCredentialMaterial {
                credential_type: CredentialType::ApiKey,
                values: HashMap::from([
                    ("api_key".to_string(), "secret-api-key".to_string()),
                    ("api_secret".to_string(), "secret-api-secret".to_string()),
                ]),
                provider: None,
                allowed_domains: self.allowed_domains.clone(),
                custom_functions: Vec::new(),
            })
        }
    }

    struct StaticTargetResolver {
        resolved_ips: Vec<IpAddr>,
    }

    #[async_trait]
    impl HttpRequestTargetResolver for StaticTargetResolver {
        async fn resolve(&self, _host: &str, _port: u16) -> Result<Vec<IpAddr>, SandboxError> {
            Ok(self.resolved_ips.clone())
        }
    }

    struct FakeTransport;

    #[async_trait]
    impl HttpRequestTransport for FakeTransport {
        async fn execute(
            &self,
            _parameters: &HashMap<String, Value>,
            _sensitive_output_values: &[String],
        ) -> Result<Value, SandboxError> {
            Ok(json!({
                "status": 200,
                "body": {
                    "balance": "42"
                }
            }))
        }
    }

    struct CountingTransport {
        call_count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl HttpRequestTransport for CountingTransport {
        async fn execute(
            &self,
            _parameters: &HashMap<String, Value>,
            _sensitive_output_values: &[String],
        ) -> Result<Value, SandboxError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(json!({
                "status": 200,
                "body": {
                    "balance": "42"
                }
            }))
        }
    }

    #[derive(Default)]
    struct InMemoryStore {
        operations: Arc<RwLock<HashMap<Uuid, HttpRequestOperationRecord>>>,
    }

    #[async_trait]
    impl HttpRequestOperationStore for InMemoryStore {
        async fn create_operation(
            &self,
            record: NewHttpRequestOperationRecord,
        ) -> Result<(), SandboxError> {
            self.operations.write().await.insert(
                record.operation_id,
                HttpRequestOperationRecord {
                    operation_id: record.operation_id,
                    tenant_id: record.tenant_id,
                    user_id: record.user_id,
                    credential_id: record.credential_id,
                    description: record.description,
                    operation_type: "http_request".to_string(),
                    status: "running".to_string(),
                    request_parameters: record.request_parameters,
                    response_data: None,
                    error_message: None,
                    started_at: record.started_at,
                    completed_at: None,
                    execution_duration_ms: None,
                },
            );
            Ok(())
        }

        async fn complete_operation(
            &self,
            record: CompleteHttpRequestOperationRecord,
        ) -> Result<(), SandboxError> {
            let mut operations = self.operations.write().await;
            if let Some(operation) = operations.get_mut(&record.operation_id) {
                operation.status = record.status.to_string();
                operation.response_data = record.response_data;
                operation.error_message = record.error_message;
                operation.completed_at = Some(record.completed_at);
                operation.execution_duration_ms = Some(record.execution_duration_ms);
            }
            Ok(())
        }

        async fn get_operation_by_id(
            &self,
            _tenant_id: Uuid,
            operation_id: Uuid,
        ) -> Result<Option<HttpRequestOperationRecord>, SandboxError> {
            Ok(self.operations.read().await.get(&operation_id).cloned())
        }
    }

    #[tokio::test]
    async fn test_submit_and_get_operation_minimal_closed_loop() {
        let broker = PersistentHttpRequestBroker::new_with_target_resolver(
            Arc::new(FakeResolver),
            Arc::new(StaticTargetResolver {
                resolved_ips: vec!["93.184.216.34".parse().unwrap()],
            }),
            Arc::new(FakeTransport),
            Arc::new(InMemoryStore::default()),
        );
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let credential_id = Uuid::new_v4();

        let submit = broker
            .submit(HttpRequestBrokerSubmitInput {
                tenant_id,
                user_id,
                credential_id,
                description: "Fetch broker payload".to_string(),
                parameters: HashMap::from([
                    ("method".to_string(), json!("GET")),
                    ("url".to_string(), json!("https://api.example.com/balance")),
                    (
                        "headers".to_string(),
                        json!({
                            "Authorization": "Bearer ${credential.api_key}"
                        }),
                    ),
                ]),
            })
            .await
            .expect("submit broker request should succeed");

        assert!(submit.success);
        assert_eq!(submit.error, None);
        assert_eq!(
            submit.data,
            Some(json!({"status": 200, "body": {"balance": "42"}}))
        );

        let operation = broker
            .get_operation(tenant_id, submit.operation_id)
            .await
            .expect("query should succeed")
            .expect("persisted operation should exist");

        assert_eq!(operation.tenant_id, tenant_id);
        assert_eq!(operation.user_id, user_id);
        assert_eq!(operation.credential_id, credential_id);
        assert_eq!(operation.status, "completed");
        assert_eq!(
            operation.response_data,
            Some(json!({"status": 200, "body": {"balance": "42"}}))
        );
        assert!(operation.completed_at.unwrap_or_else(Utc::now) >= operation.started_at);
    }

    #[tokio::test]
    async fn test_submit_fail_closes_when_credential_allowlist_is_empty() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let broker = PersistentHttpRequestBroker::new_with_target_resolver(
            Arc::new(ResolverWithAllowedDomains {
                allowed_domains: Vec::new(),
            }),
            Arc::new(StaticTargetResolver {
                resolved_ips: vec!["93.184.216.34".parse().unwrap()],
            }),
            Arc::new(CountingTransport {
                call_count: call_count.clone(),
            }),
            Arc::new(InMemoryStore::default()),
        );

        let submit = broker
            .submit(HttpRequestBrokerSubmitInput {
                tenant_id: Uuid::new_v4(),
                user_id: Uuid::new_v4(),
                credential_id: Uuid::new_v4(),
                description: "Should fail closed".to_string(),
                parameters: HashMap::from([
                    ("method".to_string(), json!("GET")),
                    ("url".to_string(), json!("https://api.example.com/balance")),
                ]),
            })
            .await
            .expect("submit should return a completed broker result");

        assert!(!submit.success);
        assert_eq!(
            submit.error,
            Some("forbidden: credential allowed_domains must not be empty".to_string())
        );
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_submit_rejects_http_targets_by_default_even_if_allowlisted() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let broker = PersistentHttpRequestBroker::new_with_target_resolver(
            Arc::new(ResolverWithAllowedDomains {
                allowed_domains: vec!["http://api.example.com".to_string()],
            }),
            Arc::new(StaticTargetResolver {
                resolved_ips: vec!["93.184.216.34".parse().unwrap()],
            }),
            Arc::new(CountingTransport {
                call_count: call_count.clone(),
            }),
            Arc::new(InMemoryStore::default()),
        );

        let submit = broker
            .submit(HttpRequestBrokerSubmitInput {
                tenant_id: Uuid::new_v4(),
                user_id: Uuid::new_v4(),
                credential_id: Uuid::new_v4(),
                description: "Default policy rejects HTTP".to_string(),
                parameters: HashMap::from([
                    ("method".to_string(), json!("GET")),
                    ("url".to_string(), json!("http://api.example.com/balance")),
                ]),
            })
            .await
            .expect("submit should return a completed broker result");

        assert!(!submit.success);
        assert_eq!(
            submit.error,
            Some("forbidden: url scheme is not allowed: http".to_string())
        );
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_submit_allows_http_targets_only_with_explicit_test_policy() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let broker = PersistentHttpRequestBroker::new_with_target_resolver_and_policy(
            Arc::new(ResolverWithAllowedDomains {
                allowed_domains: vec!["http://api.example.com".to_string()],
            }),
            Arc::new(StaticTargetResolver {
                resolved_ips: vec!["93.184.216.34".parse().unwrap()],
            }),
            HttpRequestValidationPolicy {
                allow_insecure_http: true,
            },
            Arc::new(CountingTransport {
                call_count: call_count.clone(),
            }),
            Arc::new(InMemoryStore::default()),
        );

        let submit = broker
            .submit(HttpRequestBrokerSubmitInput {
                tenant_id: Uuid::new_v4(),
                user_id: Uuid::new_v4(),
                credential_id: Uuid::new_v4(),
                description: "Explicit test policy allows HTTP".to_string(),
                parameters: HashMap::from([
                    ("method".to_string(), json!("GET")),
                    ("url".to_string(), json!("http://api.example.com/balance")),
                ]),
            })
            .await
            .expect("submit should return a completed broker result");

        assert!(submit.success);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_submit_rejects_local_ip_targets_even_if_allowlisted() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let broker = PersistentHttpRequestBroker::new_with_target_resolver_and_policy(
            Arc::new(ResolverWithAllowedDomains {
                allowed_domains: vec!["http://127.0.0.1".to_string()],
            }),
            Arc::new(StaticTargetResolver {
                resolved_ips: vec!["127.0.0.1".parse().unwrap()],
            }),
            HttpRequestValidationPolicy {
                allow_insecure_http: true,
            },
            Arc::new(CountingTransport {
                call_count: call_count.clone(),
            }),
            Arc::new(InMemoryStore::default()),
        );

        let submit = broker
            .submit(HttpRequestBrokerSubmitInput {
                tenant_id: Uuid::new_v4(),
                user_id: Uuid::new_v4(),
                credential_id: Uuid::new_v4(),
                description: "Should reject local IP".to_string(),
                parameters: HashMap::from([
                    ("method".to_string(), json!("GET")),
                    ("url".to_string(), json!("http://127.0.0.1/internal")),
                ]),
            })
            .await
            .expect("submit should return a completed broker result");

        assert!(!submit.success);
        assert_eq!(
            submit.error,
            Some("forbidden: local or private IP targets are not allowed".to_string())
        );
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_submit_rejects_hostname_resolving_to_private_ip_even_if_allowlisted() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let broker = PersistentHttpRequestBroker::new_with_target_resolver(
            Arc::new(ResolverWithAllowedDomains {
                allowed_domains: vec!["api.example.com".to_string()],
            }),
            Arc::new(StaticTargetResolver {
                resolved_ips: vec!["127.0.0.1".parse().unwrap()],
            }),
            Arc::new(CountingTransport {
                call_count: call_count.clone(),
            }),
            Arc::new(InMemoryStore::default()),
        );

        let submit = broker
            .submit(HttpRequestBrokerSubmitInput {
                tenant_id: Uuid::new_v4(),
                user_id: Uuid::new_v4(),
                credential_id: Uuid::new_v4(),
                description: "Should reject local DNS target".to_string(),
                parameters: HashMap::from([
                    ("method".to_string(), json!("GET")),
                    ("url".to_string(), json!("https://api.example.com/balance")),
                ]),
            })
            .await
            .expect("submit should return a completed broker result");

        assert!(!submit.success);
        assert_eq!(
            submit.error,
            Some("forbidden: local or private IP targets are not allowed".to_string())
        );
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_submit_rejects_allowlisted_host_when_port_does_not_match() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let broker = PersistentHttpRequestBroker::new_with_target_resolver(
            Arc::new(ResolverWithAllowedDomains {
                allowed_domains: vec!["https://api.example.com:8443".to_string()],
            }),
            Arc::new(StaticTargetResolver {
                resolved_ips: vec!["93.184.216.34".parse().unwrap()],
            }),
            Arc::new(CountingTransport {
                call_count: call_count.clone(),
            }),
            Arc::new(InMemoryStore::default()),
        );
        let tenant_id = Uuid::new_v4();

        let submit = broker
            .submit(HttpRequestBrokerSubmitInput {
                tenant_id,
                user_id: Uuid::new_v4(),
                credential_id: Uuid::new_v4(),
                description: "Should reject port mismatch".to_string(),
                parameters: HashMap::from([
                    ("method".to_string(), json!("GET")),
                    ("url".to_string(), json!("https://api.example.com/balance")),
                ]),
            })
            .await
            .expect("submit should return a completed broker result");

        assert!(!submit.success);
        assert_eq!(
            submit.error,
            Some(
                "forbidden: url target is not in credential allowed_domains: https://api.example.com/balance"
                    .to_string()
            )
        );
        assert_eq!(call_count.load(Ordering::SeqCst), 0);

        let operation = broker
            .get_operation(tenant_id, submit.operation_id)
            .await
            .expect("query should succeed")
            .expect("failed operation should be persisted");
        assert_eq!(operation.status, "failed");
        assert_eq!(
            operation.error_message,
            Some(
                "forbidden: url target is not in credential allowed_domains: https://api.example.com/balance"
                    .to_string()
            )
        );
    }

    #[tokio::test]
    async fn test_submit_rejects_allowlisted_port_when_host_does_not_match() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let broker = PersistentHttpRequestBroker::new_with_target_resolver(
            Arc::new(ResolverWithAllowedDomains {
                allowed_domains: vec!["https://api.okx.com".to_string()],
            }),
            Arc::new(StaticTargetResolver {
                resolved_ips: vec!["93.184.216.34".parse().unwrap()],
            }),
            Arc::new(CountingTransport {
                call_count: call_count.clone(),
            }),
            Arc::new(InMemoryStore::default()),
        );
        let tenant_id = Uuid::new_v4();

        let error = broker
            .submit(HttpRequestBrokerSubmitInput {
                tenant_id,
                user_id: Uuid::new_v4(),
                credential_id: Uuid::new_v4(),
                description: "Should reject host mismatch".to_string(),
                parameters: HashMap::from([
                    ("method".to_string(), json!("GET")),
                    ("url".to_string(), json!("https://api.example.com/balance")),
                ]),
            })
            .await
            .expect("submit should return a completed broker result");

        assert!(!error.success);
        assert_eq!(
            error.error,
            Some(
                "forbidden: url target is not in credential allowed_domains: https://api.example.com/balance"
                    .to_string()
            )
        );
        assert_eq!(call_count.load(Ordering::SeqCst), 0);

        let operation = broker
            .get_operation(tenant_id, error.operation_id)
            .await
            .expect("query should succeed")
            .expect("failed operation should be persisted");
        assert_eq!(operation.status, "failed");
        assert_eq!(
            operation.error_message,
            Some(
                "forbidden: url target is not in credential allowed_domains: https://api.example.com/balance"
                    .to_string()
            )
        );
    }

    #[tokio::test]
    async fn test_submit_rejects_hostname_resolving_to_ipv4_multicast_even_if_allowlisted() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let broker = PersistentHttpRequestBroker::new_with_target_resolver(
            Arc::new(ResolverWithAllowedDomains {
                allowed_domains: vec!["api.example.com".to_string()],
            }),
            Arc::new(StaticTargetResolver {
                resolved_ips: vec!["224.0.0.1".parse().unwrap()],
            }),
            Arc::new(CountingTransport {
                call_count: call_count.clone(),
            }),
            Arc::new(InMemoryStore::default()),
        );

        let submit = broker
            .submit(HttpRequestBrokerSubmitInput {
                tenant_id: Uuid::new_v4(),
                user_id: Uuid::new_v4(),
                credential_id: Uuid::new_v4(),
                description: "Should reject IPv4 multicast DNS target".to_string(),
                parameters: HashMap::from([
                    ("method".to_string(), json!("GET")),
                    ("url".to_string(), json!("https://api.example.com/balance")),
                ]),
            })
            .await
            .expect("submit should return a completed broker result");

        assert!(!submit.success);
        assert_eq!(
            submit.error,
            Some("forbidden: local or private IP targets are not allowed".to_string())
        );
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_submit_rejects_hostname_resolving_to_ipv6_multicast_even_if_allowlisted() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let broker = PersistentHttpRequestBroker::new_with_target_resolver(
            Arc::new(ResolverWithAllowedDomains {
                allowed_domains: vec!["api.example.com".to_string()],
            }),
            Arc::new(StaticTargetResolver {
                resolved_ips: vec!["ff02::1".parse().unwrap()],
            }),
            Arc::new(CountingTransport {
                call_count: call_count.clone(),
            }),
            Arc::new(InMemoryStore::default()),
        );

        let submit = broker
            .submit(HttpRequestBrokerSubmitInput {
                tenant_id: Uuid::new_v4(),
                user_id: Uuid::new_v4(),
                credential_id: Uuid::new_v4(),
                description: "Should reject IPv6 multicast DNS target".to_string(),
                parameters: HashMap::from([
                    ("method".to_string(), json!("GET")),
                    ("url".to_string(), json!("https://api.example.com/balance")),
                ]),
            })
            .await
            .expect("submit should return a completed broker result");

        assert!(!submit.success);
        assert_eq!(
            submit.error,
            Some("forbidden: local or private IP targets are not allowed".to_string())
        );
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_reqwest_transport_rejects_redirect_responses() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener should expose addr");
        let app = Router::new()
            .route(
                "/redirect",
                get(|| async { (StatusCode::TEMPORARY_REDIRECT, [("location", "/final")], "") }),
            )
            .route("/final", get(|| async { "ok" }));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("server should run");
        });

        let transport = ReqwestHttpRequestTransport::new();
        let result = transport
            .execute(
                &HashMap::from([
                    ("method".to_string(), json!("GET")),
                    (
                        "url".to_string(),
                        json!(format!("http://127.0.0.1:{}/redirect", addr.port())),
                    ),
                ]),
                &[],
            )
            .await;

        server.abort();

        match result {
            Err(SandboxError::Other(message)) => {
                assert_eq!(
                    message,
                    "redirect_not_allowed: upstream responded with HTTP 307 Temporary Redirect"
                );
            }
            other => panic!("expected redirect_not_allowed error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_reqwest_transport_redacts_sensitive_response_headers() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener should expose addr");
        let app = Router::new().route(
            "/headers",
            get(|| async {
                (
                    [
                        ("set-cookie", "session=super-secret"),
                        ("authorization", "Bearer response-secret"),
                        ("x-trace-id", "trace-visible"),
                    ],
                    axum::Json(json!({ "ok": true })),
                )
            }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("server should run");
        });

        let transport = ReqwestHttpRequestTransport::new();
        let result = transport
            .execute(
                &HashMap::from([
                    ("method".to_string(), json!("GET")),
                    (
                        "url".to_string(),
                        json!(format!("http://127.0.0.1:{}/headers", addr.port())),
                    ),
                ]),
                &[],
            )
            .await
            .expect("transport should return response");

        server.abort();

        assert_eq!(result["headers"]["set-cookie"], "[REDACTED]");
        assert_eq!(result["headers"]["authorization"], "[REDACTED]");
        assert_eq!(result["headers"]["x-trace-id"], "trace-visible");
    }

    #[tokio::test]
    async fn test_reqwest_transport_surfaces_timeout_with_stable_code() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener should expose addr");
        let app = Router::new().route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_millis(50)).await;
                axum::Json(json!({ "ok": true }))
            }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("server should run");
        });

        let transport = ReqwestHttpRequestTransport::new();
        let result = transport
            .execute(
                &HashMap::from([
                    ("method".to_string(), json!("GET")),
                    (
                        "url".to_string(),
                        json!(format!("http://127.0.0.1:{}/slow", addr.port())),
                    ),
                    ("timeout_ms".to_string(), json!(5)),
                ]),
                &[],
            )
            .await;

        server.abort();

        match result {
            Err(SandboxError::Other(message)) => {
                assert_eq!(
                    message,
                    "timeout: request exceeded configured timeout of 5ms"
                );
            }
            other => panic!("expected timeout error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_reqwest_transport_marks_truncated_response_body() {
        let oversized_body = "a".repeat(MAX_HTTP_RESPONSE_BODY_BYTES + 32);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener should expose addr");
        let app = Router::new().route(
            "/large",
            get(move || {
                let body = oversized_body.clone();
                async move { body }
            }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("server should run");
        });

        let transport = ReqwestHttpRequestTransport::new();
        let result = transport
            .execute(
                &HashMap::from([
                    ("method".to_string(), json!("GET")),
                    (
                        "url".to_string(),
                        json!(format!("http://127.0.0.1:{}/large", addr.port())),
                    ),
                ]),
                &[],
            )
            .await
            .expect("transport should return response");

        server.abort();

        assert_eq!(result["truncated"], json!(true));
        assert_eq!(
            result["body"].as_str().map(str::len),
            Some(MAX_HTTP_RESPONSE_BODY_BYTES)
        );
    }
}
