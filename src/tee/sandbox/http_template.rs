use crate::models::{CredentialCustomFunction, CredentialProvider, CredentialType};
use crate::tee::sandbox::domain_policy::ensure_url_allowed;
use crate::tee::sandbox::error::SandboxError;
use crate::tee::sandbox::function_runtime::{
    FunctionExecutionContext, execute_custom_function, validate_custom_functions,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{SecondsFormat, Utc};
use hmac::{Hmac, Mac};
use regex::Regex;
use reqwest::Url;
use serde_json::{Map, Value};
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::OnceLock;
use url::form_urlencoded;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone)]
pub struct HttpTemplateCredentialMaterial {
    pub credential_type: CredentialType,
    pub values: HashMap<String, String>,
    pub provider: Option<CredentialProvider>,
    pub allowed_domains: Vec<String>,
    pub custom_functions: Vec<CredentialCustomFunction>,
}

#[derive(Debug, Clone)]
pub struct RenderedHttpRequestParameters {
    pub parameters: HashMap<String, Value>,
    pub sensitive_values: Vec<String>,
}

#[derive(Debug, Clone)]
struct DeferredFunctionCall {
    placeholder: String,
    function_name: String,
    args: Vec<Value>,
}

#[derive(Debug, Clone)]
struct RenderSnapshot {
    method: String,
    url: String,
    query: String,
    headers: Map<String, Value>,
    body: Option<Value>,
}

pub async fn render_http_request_parameters(
    parameters: &HashMap<String, Value>,
    material: &HttpTemplateCredentialMaterial,
) -> Result<RenderedHttpRequestParameters, SandboxError> {
    validate_custom_functions(&material.custom_functions)?;

    let method = required_string(parameters, "method")?
        .trim()
        .to_ascii_uppercase();
    let url_template = required_string(parameters, "url")?;
    let query_template = parameters.get("query");
    let headers_template = parameters.get("headers");
    let body_template = parameters.get("body");

    let mut sensitive_values = Vec::new();
    let mut deferred = Vec::new();

    let mut snapshot = RenderSnapshot {
        method: method.clone(),
        url: String::new(),
        query: String::new(),
        headers: Map::new(),
        body: None,
    };

    let rendered_url = render_template_string(
        url_template,
        material,
        &snapshot,
        &mut deferred,
        &mut sensitive_values,
    )
    .await?;
    snapshot.url = rendered_url.clone();

    let rendered_query = match query_template {
        Some(value) => Some(
            render_template_value(
                value,
                material,
                &snapshot,
                &mut deferred,
                &mut sensitive_values,
            )
            .await?,
        ),
        None => None,
    };
    if let Some(query_value) = &rendered_query {
        snapshot.query = query_string_from_value(query_value)?;
        snapshot.url = merge_query_into_url(&rendered_url, &snapshot.query)?;
    } else if let Ok(parsed) = Url::parse(&snapshot.url) {
        snapshot.query = parsed.query().unwrap_or_default().to_string();
    }

    let rendered_headers = match headers_template {
        Some(value) => match render_template_value(
            value,
            material,
            &snapshot,
            &mut deferred,
            &mut sensitive_values,
        )
        .await?
        {
            Value::Object(map) => map,
            _ => {
                return Err(SandboxError::Other(
                    "http_request headers must be an object".to_string(),
                ));
            }
        },
        None => Map::new(),
    };
    snapshot.headers = rendered_headers.clone();

    let rendered_body = match body_template {
        Some(value) => Some(
            render_template_value(
                value,
                material,
                &snapshot,
                &mut deferred,
                &mut sensitive_values,
            )
            .await?,
        ),
        None => None,
    };
    snapshot.body = rendered_body.clone();

    let final_url = replace_deferred_in_string(
        &snapshot.url,
        &deferred,
        material,
        &RenderSnapshot {
            body: rendered_body.clone(),
            ..snapshot.clone()
        },
        &mut sensitive_values,
    )
    .await?;
    let parsed_url = Url::parse(&final_url)
        .map_err(|_| SandboxError::Other(format!("invalid_request: invalid url {final_url}")))?;
    ensure_url_allowed(&parsed_url, &material.allowed_domains)?;

    let final_query = match rendered_query.as_ref() {
        Some(value) => Some(
            replace_deferred_in_value(value, &deferred, material, &snapshot, &mut sensitive_values)
                .await?,
        ),
        None => None,
    };
    let final_headers = replace_deferred_in_value(
        &Value::Object(rendered_headers.clone()),
        &deferred,
        material,
        &snapshot,
        &mut sensitive_values,
    )
    .await?;
    let final_body = match rendered_body.as_ref() {
        Some(value) => Some(
            replace_deferred_in_value(value, &deferred, material, &snapshot, &mut sensitive_values)
                .await?,
        ),
        None => None,
    };

    let mut rendered_parameters = parameters.clone();
    rendered_parameters.insert("method".to_string(), Value::String(method));
    rendered_parameters.insert("url".to_string(), Value::String(final_url));
    if let Some(query) = final_query {
        rendered_parameters.insert("query".to_string(), query);
    }
    if let Value::Object(headers) = final_headers {
        rendered_parameters.insert("headers".to_string(), Value::Object(headers));
    }
    if let Some(body) = final_body {
        rendered_parameters.insert("body".to_string(), body);
    }

    sensitive_values.sort();
    sensitive_values.dedup();

    Ok(RenderedHttpRequestParameters {
        parameters: rendered_parameters,
        sensitive_values,
    })
}

pub fn extract_supported_http_credential_fields(
    credential_type: CredentialType,
    plaintext_data: &Value,
) -> Result<HashMap<String, String>, SandboxError> {
    let object = plaintext_data.as_object().ok_or_else(|| {
        SandboxError::Other(
            "credential plaintext must be a JSON object for delegated sandbox use".to_string(),
        )
    })?;

    let mut values = HashMap::new();
    for (key, value) in object {
        match value {
            Value::String(raw) => {
                values.insert(key.clone(), raw.clone());
            }
            Value::Number(number) => {
                values.insert(key.clone(), number.to_string());
            }
            Value::Bool(boolean) => {
                values.insert(key.clone(), boolean.to_string());
            }
            _ => {}
        }
    }

    if credential_type == CredentialType::ApiKey {
        let api_key = ["api_key", "key", "apiKey"]
            .iter()
            .find_map(|field| values.get(*field).cloned())
            .ok_or_else(|| {
                SandboxError::Other("credential plaintext missing api_key".to_string())
            })?;
        values.insert("api_key".to_string(), api_key.clone());
        values.insert("key".to_string(), api_key.clone());
        values.insert("apiKey".to_string(), api_key);

        if let Some(secret_key) = ["secret_key", "secretKey", "secret"]
            .iter()
            .find_map(|field| values.get(*field).cloned())
        {
            values.insert("secret_key".to_string(), secret_key.clone());
            values.insert("secretKey".to_string(), secret_key.clone());
            values.insert("secret".to_string(), secret_key);
        }
    }

    if values.is_empty() {
        return Err(SandboxError::Other(format!(
            "sandbox credential delegation found no scalar fields for {}",
            credential_type.as_str()
        )));
    }

    Ok(values)
}

async fn render_template_value(
    value: &Value,
    material: &HttpTemplateCredentialMaterial,
    snapshot: &RenderSnapshot,
    deferred: &mut Vec<DeferredFunctionCall>,
    sensitive_values: &mut Vec<String>,
) -> Result<Value, SandboxError> {
    match value {
        Value::String(text) => Ok(Value::String(
            render_template_string(text, material, snapshot, deferred, sensitive_values).await?,
        )),
        Value::Array(items) => {
            let mut rendered = Vec::with_capacity(items.len());
            for item in items {
                rendered.push(
                    Box::pin(render_template_value(
                        item,
                        material,
                        snapshot,
                        deferred,
                        sensitive_values,
                    ))
                    .await?,
                );
            }
            Ok(Value::Array(rendered))
        }
        Value::Object(map) => {
            let mut rendered = Map::with_capacity(map.len());
            for (key, item) in map {
                rendered.insert(
                    key.clone(),
                    Box::pin(render_template_value(
                        item,
                        material,
                        snapshot,
                        deferred,
                        sensitive_values,
                    ))
                    .await?,
                );
            }
            Ok(Value::Object(rendered))
        }
        _ => Ok(value.clone()),
    }
}

async fn render_template_string(
    template: &str,
    material: &HttpTemplateCredentialMaterial,
    snapshot: &RenderSnapshot,
    deferred: &mut Vec<DeferredFunctionCall>,
    sensitive_values: &mut Vec<String>,
) -> Result<String, SandboxError> {
    let mut rendered = String::new();
    let mut last_index = 0usize;

    for captures in template_expression_regex().captures_iter(template) {
        let full = captures.get(0).expect("match should include full capture");
        let expression = captures
            .get(1)
            .expect("match should include expression")
            .as_str()
            .trim();

        rendered.push_str(&template[last_index..full.start()]);
        rendered.push_str(
            &resolve_template_expression(
                expression,
                material,
                snapshot,
                deferred,
                sensitive_values,
            )
            .await?,
        );
        last_index = full.end();
    }

    rendered.push_str(&template[last_index..]);
    Ok(rendered)
}

async fn resolve_template_expression(
    expression: &str,
    material: &HttpTemplateCredentialMaterial,
    snapshot: &RenderSnapshot,
    deferred: &mut Vec<DeferredFunctionCall>,
    sensitive_values: &mut Vec<String>,
) -> Result<String, SandboxError> {
    if let Some(field) = expression.strip_prefix("credential.") {
        let value = material.values.get(field).cloned().ok_or_else(|| {
            SandboxError::Other(format!("unsupported credential template field: {field}"))
        })?;
        sensitive_values.push(value.clone());
        return Ok(value);
    }

    let Some(function_expression) = expression.strip_prefix("functions.") else {
        return Err(SandboxError::Other(format!(
            "unsupported template expression: {expression}"
        )));
    };

    let Some((function_name, args_text)) = function_expression.split_once('(') else {
        return Err(SandboxError::Other(format!(
            "invalid function expression: {expression}"
        )));
    };
    let args_text = args_text.trim_end_matches(')').trim();
    let args = parse_function_args(args_text)?;
    let function_name = function_name.trim();

    if matches!(function_name, "okx_sign" | "binance_sign") {
        let placeholder = format!("CBTPL{}TOKEN", deferred.len());
        deferred.push(DeferredFunctionCall {
            placeholder: placeholder.clone(),
            function_name: function_name.to_string(),
            args,
        });
        return Ok(placeholder);
    }

    execute_function(function_name, args.as_slice(), material, snapshot).await
}

async fn replace_deferred_in_string(
    value: &str,
    deferred: &[DeferredFunctionCall],
    material: &HttpTemplateCredentialMaterial,
    snapshot: &RenderSnapshot,
    sensitive_values: &mut Vec<String>,
) -> Result<String, SandboxError> {
    let mut rendered = value.to_string();
    for call in deferred {
        if !rendered.contains(&call.placeholder) {
            continue;
        }
        let replacement = execute_function(
            &call.function_name,
            call.args.as_slice(),
            material,
            snapshot,
        )
        .await?;
        sensitive_values.push(replacement.clone());
        rendered = rendered.replace(&call.placeholder, &replacement);
    }
    Ok(rendered)
}

async fn replace_deferred_in_value(
    value: &Value,
    deferred: &[DeferredFunctionCall],
    material: &HttpTemplateCredentialMaterial,
    snapshot: &RenderSnapshot,
    sensitive_values: &mut Vec<String>,
) -> Result<Value, SandboxError> {
    match value {
        Value::String(text) => Ok(Value::String(
            replace_deferred_in_string(text, deferred, material, snapshot, sensitive_values)
                .await?,
        )),
        Value::Array(items) => {
            let mut rendered = Vec::with_capacity(items.len());
            for item in items {
                rendered.push(
                    Box::pin(replace_deferred_in_value(
                        item,
                        deferred,
                        material,
                        snapshot,
                        sensitive_values,
                    ))
                    .await?,
                );
            }
            Ok(Value::Array(rendered))
        }
        Value::Object(map) => {
            let mut rendered = Map::with_capacity(map.len());
            for (key, item) in map {
                rendered.insert(
                    key.clone(),
                    Box::pin(replace_deferred_in_value(
                        item,
                        deferred,
                        material,
                        snapshot,
                        sensitive_values,
                    ))
                    .await?,
                );
            }
            Ok(Value::Object(rendered))
        }
        _ => Ok(value.clone()),
    }
}

async fn execute_function(
    function_name: &str,
    args: &[Value],
    material: &HttpTemplateCredentialMaterial,
    snapshot: &RenderSnapshot,
) -> Result<String, SandboxError> {
    match function_name {
        "okx_timestamp" => Ok(Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)),
        "binance_timestamp" => Ok(Utc::now().timestamp_millis().to_string()),
        "okx_sign" => okx_sign(material, snapshot),
        "binance_sign" => binance_sign(material, snapshot),
        custom_name => {
            let context = FunctionExecutionContext {
                credential: material.values.clone(),
                provider: material.provider,
                method: snapshot.method.clone(),
                url: snapshot.url.clone(),
                query: snapshot.query.clone(),
                headers: snapshot.headers.clone(),
                body: snapshot.body.clone(),
            };
            execute_custom_function(
                material.custom_functions.as_slice(),
                custom_name,
                args,
                &context,
            )
            .await
        }
    }
}

fn okx_sign(
    material: &HttpTemplateCredentialMaterial,
    snapshot: &RenderSnapshot,
) -> Result<String, SandboxError> {
    let secret = material.values.get("secret_key").ok_or_else(|| {
        SandboxError::Other("okx_sign requires credential.secret_key".to_string())
    })?;
    let timestamp = snapshot
        .headers
        .get("OK-ACCESS-TIMESTAMP")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            SandboxError::Other("okx_sign requires OK-ACCESS-TIMESTAMP header".to_string())
        })?;
    let url = Url::parse(&snapshot.url).map_err(|_| {
        SandboxError::Other(format!("invalid_request: invalid url {}", snapshot.url))
    })?;
    let request_path = match url.query() {
        Some(query) if !query.is_empty() => format!("{}?{}", url.path(), query),
        _ => url.path().to_string(),
    };
    let body = normalized_request_body(snapshot.body.as_ref())?;

    let payload = format!("{timestamp}{}{}{}", snapshot.method, request_path, body);
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|error| SandboxError::Other(format!("failed to initialize okx hmac: {error}")))?;
    mac.update(payload.as_bytes());
    Ok(STANDARD.encode(mac.finalize().into_bytes()))
}

fn binance_sign(
    material: &HttpTemplateCredentialMaterial,
    snapshot: &RenderSnapshot,
) -> Result<String, SandboxError> {
    let secret = material.values.get("secret_key").ok_or_else(|| {
        SandboxError::Other("binance_sign requires credential.secret_key".to_string())
    })?;

    let url = Url::parse(&snapshot.url).map_err(|_| {
        SandboxError::Other(format!("invalid_request: invalid url {}", snapshot.url))
    })?;
    let mut source_parts = Vec::new();
    let query = remove_signature_parameter(url.query().unwrap_or_default());
    if !query.is_empty() {
        source_parts.push(query);
    }
    if let Some(body) = snapshot.body.as_ref() {
        let body_source = normalized_request_body_for_signature(body)?;
        let body_source = remove_signature_parameter(&body_source);
        if !body_source.is_empty() {
            source_parts.push(body_source);
        }
    }
    let signing_payload = source_parts.join("&");

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).map_err(|error| {
        SandboxError::Other(format!("failed to initialize binance hmac: {error}"))
    })?;
    mac.update(signing_payload.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn normalized_request_body(body: Option<&Value>) -> Result<String, SandboxError> {
    match body {
        None => Ok(String::new()),
        Some(Value::String(text)) => Ok(text.clone()),
        Some(other) => serde_json::to_string(other).map_err(|error| {
            SandboxError::Serialization(format!("failed to serialize request body: {error}"))
        }),
    }
}

fn normalized_request_body_for_signature(body: &Value) -> Result<String, SandboxError> {
    match body {
        Value::String(text) => Ok(text.clone()),
        Value::Object(_) => query_string_from_value(body),
        Value::Array(_) => serde_json::to_string(body).map_err(|error| {
            SandboxError::Serialization(format!("failed to serialize request body: {error}"))
        }),
        _ => Ok(body.to_string()),
    }
}

fn remove_signature_parameter(raw: &str) -> String {
    form_urlencoded::parse(raw.as_bytes())
        .filter(|(key, _)| key != "signature")
        .fold(
            form_urlencoded::Serializer::new(String::new()),
            |mut serializer, (key, value)| {
                serializer.append_pair(&key, &value);
                serializer
            },
        )
        .finish()
}

fn parse_function_args(args_text: &str) -> Result<Vec<Value>, SandboxError> {
    if args_text.is_empty() {
        return Ok(Vec::new());
    }

    serde_json::from_str::<Vec<Value>>(&format!("[{args_text}]")).map_err(|error| {
        SandboxError::Other(format!("failed to parse function arguments: {error}"))
    })
}

fn required_string<'a>(
    parameters: &'a HashMap<String, Value>,
    field: &str,
) -> Result<&'a str, SandboxError> {
    parameters
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| SandboxError::Other(format!("invalid_request: missing {field}")))
}

fn query_string_from_value(value: &Value) -> Result<String, SandboxError> {
    let object = value
        .as_object()
        .ok_or_else(|| SandboxError::Other("http_request query must be an object".to_string()))?;

    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for (key, value) in object {
        match value {
            Value::Null => {}
            Value::String(text) => {
                serializer.append_pair(key, text);
            }
            Value::Number(number) => {
                serializer.append_pair(key, &number.to_string());
            }
            Value::Bool(boolean) => {
                serializer.append_pair(key, &boolean.to_string());
            }
            Value::Array(items) => {
                for item in items {
                    match item {
                        Value::String(text) => serializer.append_pair(key, text),
                        Value::Number(number) => serializer.append_pair(key, &number.to_string()),
                        Value::Bool(boolean) => serializer.append_pair(key, &boolean.to_string()),
                        _ => {
                            return Err(SandboxError::Other(format!(
                                "http_request query field {key} must contain only scalar values"
                            )));
                        }
                    };
                }
            }
            _ => {
                return Err(SandboxError::Other(format!(
                    "http_request query field {key} must be scalar or array of scalars"
                )));
            }
        }
    }
    Ok(serializer.finish())
}

fn merge_query_into_url(url: &str, query: &str) -> Result<String, SandboxError> {
    if query.is_empty() {
        return Ok(url.to_string());
    }

    let mut parsed = Url::parse(url)
        .map_err(|_| SandboxError::Other(format!("invalid_request: invalid url {url}")))?;
    match parsed.query() {
        Some(existing) if !existing.is_empty() => {
            parsed.set_query(Some(&format!("{existing}&{query}")));
        }
        _ => parsed.set_query(Some(query)),
    }
    Ok(parsed.to_string())
}

fn template_expression_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\$\{([^}]+)\}").expect("template regex should compile"))
}

#[cfg(test)]
mod tests {
    use super::{
        HttpTemplateCredentialMaterial, RenderSnapshot, extract_supported_http_credential_fields,
        render_http_request_parameters,
    };
    use crate::models::CredentialType;
    use serde_json::{Value, json};
    use std::collections::HashMap;

    #[tokio::test]
    async fn renders_okx_headers_and_templates() {
        let mut values = HashMap::new();
        values.insert("api_key".to_string(), "api-key".to_string());
        values.insert("secret_key".to_string(), "secret-key".to_string());
        values.insert("passphrase".to_string(), "passphrase".to_string());

        let material = HttpTemplateCredentialMaterial {
            credential_type: CredentialType::ApiKey,
            values,
            provider: None,
            allowed_domains: vec!["www.okx.com:443".to_string()],
            custom_functions: Vec::new(),
        };

        let parameters = HashMap::from([
            ("method".to_string(), json!("GET")),
            (
                "url".to_string(),
                json!("https://www.okx.com/api/v5/account/balance"),
            ),
            (
                "headers".to_string(),
                json!({
                    "OK-ACCESS-KEY": "${credential.api_key}",
                    "OK-ACCESS-TIMESTAMP": "${functions.okx_timestamp()}",
                    "OK-ACCESS-PASSPHRASE": "${credential.passphrase}",
                    "OK-ACCESS-SIGN": "${functions.okx_sign()}"
                }),
            ),
        ]);

        let rendered = render_http_request_parameters(&parameters, &material)
            .await
            .expect("render should succeed");

        let headers = rendered
            .parameters
            .get("headers")
            .and_then(Value::as_object)
            .expect("headers should be an object");
        assert_eq!(headers["OK-ACCESS-KEY"], "api-key");
        assert_eq!(headers["OK-ACCESS-PASSPHRASE"], "passphrase");
        assert!(
            headers["OK-ACCESS-SIGN"]
                .as_str()
                .expect("signature should be a string")
                .len()
                > 10
        );
    }

    #[tokio::test]
    async fn blocks_disallowed_binance_domain_after_rendering_templates() {
        let mut values = HashMap::new();
        values.insert("api_key".to_string(), "api-key".to_string());
        values.insert("secret_key".to_string(), "secret-key".to_string());

        let material = HttpTemplateCredentialMaterial {
            credential_type: CredentialType::ApiKey,
            values,
            provider: None,
            allowed_domains: vec!["www.okx.com:443".to_string()],
            custom_functions: Vec::new(),
        };

        let parameters = HashMap::from([
            ("method".to_string(), json!("GET")),
            (
                "url".to_string(),
                json!("https://api.binance.com/api/v3/account"),
            ),
            (
                "query".to_string(),
                json!({
                    "recvWindow": "5000",
                    "timestamp": "${functions.binance_timestamp()}",
                    "signature": "${functions.binance_sign()}"
                }),
            ),
            (
                "headers".to_string(),
                json!({
                    "X-MBX-APIKEY": "${credential.api_key}"
                }),
            ),
        ]);

        let error = render_http_request_parameters(&parameters, &material)
            .await
            .expect_err("disallowed domains should be blocked");

        assert!(error.to_string().contains("allowed_domains policy"));
        assert!(error.to_string().contains("api.binance.com:443"));
    }

    #[tokio::test]
    async fn tracks_deferred_signature_values_in_query_and_headers() {
        let mut values = HashMap::new();
        values.insert("api_key".to_string(), "api-key".to_string());
        values.insert("secret_key".to_string(), "secret-key".to_string());

        let material = HttpTemplateCredentialMaterial {
            credential_type: CredentialType::ApiKey,
            values,
            provider: None,
            allowed_domains: vec!["api.binance.com:443".to_string()],
            custom_functions: Vec::new(),
        };

        let parameters = HashMap::from([
            ("method".to_string(), json!("GET")),
            (
                "url".to_string(),
                json!("https://api.binance.com/api/v3/account"),
            ),
            (
                "query".to_string(),
                json!({
                    "recvWindow": "5000",
                    "timestamp": "${functions.binance_timestamp()}",
                    "signature": "${functions.binance_sign()}"
                }),
            ),
            (
                "headers".to_string(),
                json!({
                    "X-MBX-APIKEY": "${credential.api_key}",
                    "X-SIGNATURE-COPY": "${functions.binance_sign()}"
                }),
            ),
        ]);

        let rendered = render_http_request_parameters(&parameters, &material)
            .await
            .expect("render should succeed");

        let query_signature = rendered
            .parameters
            .get("query")
            .and_then(Value::as_object)
            .and_then(|query| query.get("signature"))
            .and_then(Value::as_str)
            .expect("query signature should be rendered")
            .to_string();
        let header_signature = rendered
            .parameters
            .get("headers")
            .and_then(Value::as_object)
            .and_then(|headers| headers.get("X-SIGNATURE-COPY"))
            .and_then(Value::as_str)
            .expect("header signature should be rendered")
            .to_string();

        assert!(rendered.sensitive_values.contains(&query_signature));
        assert!(rendered.sensitive_values.contains(&header_signature));
    }

    #[test]
    fn extracts_api_key_secret_key_and_custom_scalars() {
        let fields = extract_supported_http_credential_fields(
            CredentialType::ApiKey,
            &json!({
                "api_key": "ak",
                "secret_key": "sk",
                "passphrase": "pp",
                "label": "sandbox"
            }),
        )
        .expect("field extraction should succeed");

        assert_eq!(fields.get("api_key").map(String::as_str), Some("ak"));
        assert_eq!(fields.get("secret_key").map(String::as_str), Some("sk"));
        assert_eq!(fields.get("passphrase").map(String::as_str), Some("pp"));
        assert_eq!(fields.get("label").map(String::as_str), Some("sandbox"));
    }

    #[test]
    fn render_snapshot_is_cloneable_for_runtime_contexts() {
        let snapshot = RenderSnapshot {
            method: "GET".to_string(),
            url: "https://api.binance.com/api/v3/account".to_string(),
            query: "timestamp=1".to_string(),
            headers: serde_json::Map::new(),
            body: None,
        };
        assert_eq!(snapshot.clone().method, "GET");
    }
}
