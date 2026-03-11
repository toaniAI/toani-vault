//! CredBridge SDK 核心客户端
//!
//! 实现 HTTP 请求、错误重试、Token 管理等功能

use crate::types::{
    ApiErrorResponse, CredBridgeConfig, CredBridgeError, CredBridgeErrorCode,
    CredBridgeErrorCode::*, RequestOptions, Result, TokenInfo, TokenScope,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use reqwest::{Client, ClientBuilder, Method, Response};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};
use tracing::{debug, error, info, warn};

/// 指数退避延迟计算
fn calculate_backoff_delay(attempt: u32, base_delay_ms: u64) -> u64 {
    base_delay_ms * 2_u64.pow(attempt)
}

/// 生成请求 ID
fn generate_request_id() -> String {
    format!(
        "req_{}_{}",
        chrono::Utc::now().timestamp_millis(),
        uuid::Uuid::now_v7()
    )
}

/// 解析 API 错误码
fn parse_error_code(status_code: u16, error_code: Option<&str>) -> CredBridgeErrorCode {
    if let Some(code) = error_code {
        match code {
            "not_found" => NotFound,
            "invalid_request" => InvalidRequest,
            "unauthorized" => Unauthorized,
            "forbidden" => Forbidden,
            "internal_error" => InternalError,
            "token_expired" => TokenExpired,
            "invalid_token" => InvalidToken,
            "token_revoked" => TokenRevoked,
            "insufficient_scope" => InsufficientScope,
            "tenant_isolation_violation" => TenantIsolationViolation,
            "credential_expired" => CredentialExpired,
            "decryption_failed" => DecryptionFailed,
            _ => Unknown,
        }
    } else {
        match status_code {
            400 => InvalidRequest,
            401 => Unauthorized,
            403 => Forbidden,
            404 => NotFound,
            408 => Timeout,
            429 => InternalError, // Rate limited, should retry
            500 | 502 | 503 | 504 => InternalError,
            _ => Unknown,
        }
    }
}

/// CredBridge HTTP 客户端
#[derive(Debug, Clone)]
pub struct CredBridgeClient {
    config: CredBridgeConfig,
    http_client: Client,
    token_info: Arc<RwLock<Option<TokenInfo>>>,
    token: Arc<RwLock<Option<String>>>,
}

impl CredBridgeClient {
    /// 创建新的客户端
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use credbridge_sdk::{CredBridgeConfig, CredBridgeClient};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = CredBridgeConfig::new("https://api.credbridge.io")
    ///     .with_token("your-api-token")
    ///     .with_timeout_ms(30000);
    ///
    /// let client = CredBridgeClient::new(config)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(config: CredBridgeConfig) -> Result<Self> {
        let http_client = ClientBuilder::new()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|e| {
                CredBridgeError::new(
                    CredBridgeErrorCode::NetworkError,
                    format!("Failed to create HTTP client: {}", e),
                )
            })?;

        let token_info = Arc::new(RwLock::new(None));
        let token = Arc::new(RwLock::new(config.token.clone()));
        let client = Self {
            config,
            http_client,
            token_info,
            token,
        };

        // 如果提供了 token，解析并存储
        if let Some(ref token_str) = client.config.token {
            client.parse_and_store_token(token_str);
        }

        Ok(client)
    }

    /// 获取当前配置
    pub fn get_config(&self) -> &CredBridgeConfig {
        &self.config
    }

    /// 更新 Token
    pub fn set_token(&self, new_token: impl Into<String>) {
        let new_token = new_token.into();
        if let Ok(mut guard) = self.token.write() {
            *guard = Some(new_token.clone());
        }
        self.parse_and_store_token(&new_token);
        info!("Token updated successfully");
    }

    /// 获取当前 Token
    pub fn get_token(&self) -> Option<String> {
        self.token.read().ok().and_then(|guard| guard.clone())
    }

    /// 获取 Token 信息
    pub fn get_token_info(&self) -> Option<TokenInfo> {
        self.token_info.read().ok().and_then(|guard| guard.clone())
    }

    /// 检查 Token 是否即将过期
    pub fn is_token_expiring_soon(&self) -> bool {
        let token_info = match self.get_token_info() {
            Some(info) => info,
            None => return true,
        };

        let now = chrono::Utc::now().timestamp();
        let buffer_seconds = (self.config.token_refresh_buffer_ms / 1000) as i64;

        now >= token_info.expires_at - buffer_seconds
    }

    /// 检查 Token 是否已过期
    pub fn is_token_expired(&self) -> bool {
        let token_info = match self.get_token_info() {
            Some(info) => info,
            None => return true,
        };

        let now = chrono::Utc::now().timestamp();
        now >= token_info.expires_at
    }

    /// 发送 HTTP 请求
    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        options: Option<RequestOptions>,
    ) -> Result<T> {
        let options = options.unwrap_or_default();
        let request_id = options.request_id.unwrap_or_else(generate_request_id);
        let url = self.build_url(path)?;
        let timeout_ms = options.timeout_ms.unwrap_or(self.config.timeout_ms);
        let max_retries = if options.skip_retry {
            0
        } else {
            options.retries.unwrap_or(self.config.max_retries)
        };

        debug!(
            request_id = %request_id,
            method = %method,
            url = %url,
            "Starting request"
        );

        // 检查是否需要刷新 Token
        if self.config.auto_refresh_token
            && self.is_token_expiring_soon()
            && !path.contains("/tokens")
        {
            warn!("Token is expiring soon, consider refreshing");
        }

        let mut last_error: Option<CredBridgeError> = None;

        for attempt in 0..=max_retries {
            match self
                .execute_request::<T>(
                    method.clone(),
                    &url,
                    body.clone(),
                    timeout_ms,
                    &request_id,
                    &options.headers,
                )
                .await
            {
                Ok(result) => {
                    info!(request_id = %request_id, "Request successful");
                    return Ok(result);
                }
                Err(error) => {
                    last_error = Some(error.clone());

                    // 如果是认证错误，尝试刷新 Token 后重试
                    if error.is_auth_error()
                        && self.config.auto_refresh_token
                        && attempt == 0
                    {
                        warn!(request_id = %request_id, "Auth error, attempting token refresh");
                        // Token 刷新逻辑可以在这里实现
                        // 目前只是继续重试流程
                    }

                    // 如果不是可重试的错误，或者已经是最后一次尝试，抛出错误
                    if !error.is_retryable() || attempt == max_retries {
                        error!(
                            request_id = %request_id,
                            error = %error,
                            "Request failed"
                        );
                        return Err(error);
                    }

                    // 计算退避延迟
                    let delay_ms = calculate_backoff_delay(attempt, 1000);
                    warn!(
                        request_id = %request_id,
                        attempt = attempt + 1,
                        max_retries = max_retries,
                        delay_ms = delay_ms,
                        "Retrying request"
                    );
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }

        // 所有重试都失败了
        Err(last_error.unwrap_or_else(|| {
            CredBridgeError::new(CredBridgeErrorCode::Unknown, "Request failed after retries")
        }))
    }

    /// 执行单次 HTTP 请求
    async fn execute_request<T: DeserializeOwned>(
        &self,
        method: Method,
        url: &str,
        body: Option<Value>,
        timeout_ms: u64,
        request_id: &str,
        custom_headers: &HashMap<String, String>,
    ) -> Result<T> {
        let mut request_builder = self
            .http_client
            .request(method.clone(), url)
            .timeout(Duration::from_millis(timeout_ms))
            .header("Content-Type", "application/json")
            .header("X-Request-ID", request_id);

        // 添加自定义请求头
        for (key, value) in &self.config.headers {
            request_builder = request_builder.header(key, value);
        }
        for (key, value) in custom_headers {
            request_builder = request_builder.header(key, value);
        }

        // 添加 Authorization 头
        if let Some(token) = self.get_token() {
            request_builder = request_builder.header("Authorization", format!("Bearer {}", token));
        }

        // 添加请求签名（如果配置了签名密钥）
        if let Some(ref _signing_key) = self.config.signing_key {
            let signature = self.sign_request(&method, url, body.as_ref());
            request_builder = request_builder.header("X-CredBridge-Signature", signature);
        }

        // 添加请求体
        if let Some(body) = body {
            request_builder = request_builder.json(&body);
        }

        // 发送请求
        let response = request_builder.send().await.map_err(|e| {
            if e.is_timeout() {
                CredBridgeError::new(CredBridgeErrorCode::Timeout, format!("Request timeout: {}", e))
                    .with_request_id(request_id)
            } else if e.is_connect() {
                CredBridgeError::new(
                    CredBridgeErrorCode::NetworkError,
                    format!("Network error: {}", e),
                )
                .with_request_id(request_id)
            } else {
                CredBridgeError::new(CredBridgeErrorCode::Unknown, format!("Request error: {}", e))
                    .with_request_id(request_id)
            }
        })?;

        self.handle_response(response, request_id).await
    }

    /// 处理 HTTP 响应
    async fn handle_response<T: DeserializeOwned>(
        &self,
        response: Response,
        request_id: &str,
    ) -> Result<T> {
        let status = response.status();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok());

        let is_json = content_type
            .map(|ct| ct.contains("application/json"))
            .unwrap_or(false);

        if status.is_success() {
            if is_json {
                response.json::<T>().await.map_err(|e| {
                    CredBridgeError::new(
                        CredBridgeErrorCode::InternalError,
                        format!("Failed to parse JSON response: {}", e),
                    )
                    .with_request_id(request_id)
                })
            } else {
                // 对于非 JSON 响应，尝试解析为空类型
                // 这里简化处理，实际可能需要更复杂的逻辑
                Err(CredBridgeError::new(
                    CredBridgeErrorCode::InternalError,
                    "Unexpected non-JSON response",
                ))
            }
        } else {
            // 处理错误响应
            let error_response: ApiErrorResponse = if is_json {
                response.json().await.map_err(|e| {
                    CredBridgeError::new(
                        CredBridgeErrorCode::InternalError,
                        format!("Failed to parse error response: {}", e),
                    )
                })?
            } else {
                let text = response.text().await.unwrap_or_default();
                return Err(CredBridgeError::new(
                    parse_error_code(status.as_u16(), None),
                    text,
                )
                .with_status_code(status.as_u16())
                .with_request_id(request_id));
            };

            let error_code =
                parse_error_code(status.as_u16(), Some(&error_response.error.code));

            Err(CredBridgeError::new(error_code, error_response.error.message)
                .with_status_code(status.as_u16())
                .with_request_id(request_id)
                .with_details(error_response.error.details.unwrap_or_default()))
        }
    }

    /// 构建完整 URL
    fn build_url(&self, path: &str) -> Result<String> {
        let base_url = self.config.base_url.trim_end_matches('/');
        let path = if path.starts_with('/') {
            &path[1..]
        } else {
            path
        };

        let url = format!("{}/api/v1/{}", base_url, path);
        Ok(url)
    }

    /// 解析并存储 Token 信息
    fn parse_and_store_token(&self, token: &str) {
        match self.do_parse_token(token) {
            Ok(token_info) => {
                if let Ok(mut guard) = self.token_info.write() {
                    *guard = Some(token_info);
                }
            }
            Err(e) => {
                warn!("Failed to parse token: {}", e);
            }
        }
    }

    /// 执行 Token 解析
    fn do_parse_token(&self, token: &str) -> Result<TokenInfo> {
        // 解析 PASETO token 的 payload 部分
        // PASETO 格式: version.purpose.payload.signature
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() < 3 {
            return Err(CredBridgeError::new(
                CredBridgeErrorCode::InvalidToken,
                "Invalid PASETO token format",
            ));
        }

        let payload_base64 = parts[2];
        let payload_json = URL_SAFE_NO_PAD
            .decode(payload_base64)
            .map_err(|e| {
                CredBridgeError::new(
                    CredBridgeErrorCode::InvalidToken,
                    format!("Failed to decode token payload: {}", e),
                )
            })?;

        let payload: Value = serde_json::from_slice(&payload_json).map_err(|e| {
            CredBridgeError::new(
                CredBridgeErrorCode::InvalidToken,
                format!("Failed to parse token payload: {}", e),
            )
        })?;

        // 解析 scope 字符串为数组
        let scope_str = payload["scope"].as_str().unwrap_or("");
        let scopes: Vec<TokenScope> = scope_str
            .split_whitespace()
            .filter_map(|s| match s {
                "credential:read" => Some(TokenScope::CredentialRead),
                "credential:decrypt" => Some(TokenScope::CredentialDecrypt),
                "credential:write" => Some(TokenScope::CredentialWrite),
                "audit:read" => Some(TokenScope::AuditRead),
                "admin" => Some(TokenScope::Admin),
                _ => None,
            })
            .collect();

        // 解析 subject (tenant_id:user_id)
        let subject = payload["sub"].as_str().unwrap_or("").to_string();
        let (tenant_id, user_id) = subject
            .split_once(':')
            .map(|(t, u)| (t.to_string(), u.to_string()))
            .unwrap_or_else(|| {
                (
                    payload["tenant_id"].as_str().unwrap_or("").to_string(),
                    String::new(),
                )
            });

        Ok(TokenInfo {
            token_id: payload["jti"].as_str().unwrap_or("").to_string(),
            subject,
            tenant_id,
            user_id,
            expires_at: payload["exp"].as_i64().unwrap_or(0),
            scopes,
            issued_at: payload["iat"].as_i64().unwrap_or(0),
        })
    }

    /// 签名请求
    fn sign_request(&self, _method: &Method, _url: &str, _body: Option<&Value>) -> String {
        // 请求签名实现
        // 这里应该使用配置的 signingKey 对请求进行签名
        // 实际实现取决于服务端验证签名的方式
        let timestamp = chrono::Utc::now().timestamp();
        format!("t={},v1=placeholder", timestamp)
    }

    // ============================================================================
    // HTTP 方法快捷方式
    // ============================================================================

    /// GET 请求
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.request(Method::GET, path, None, None).await
    }

    /// GET 请求（带选项）
    pub async fn get_with_options<T: DeserializeOwned>(
        &self,
        path: &str,
        options: Option<RequestOptions>,
    ) -> Result<T> {
        self.request(Method::GET, path, None, options).await
    }

    /// POST 请求
    pub async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: impl serde::Serialize,
    ) -> Result<T> {
        let body = serde_json::to_value(body).map_err(|e| {
            CredBridgeError::new(
                CredBridgeErrorCode::InvalidRequest,
                format!("Failed to serialize request body: {}", e),
            )
        })?;
        self.request(Method::POST, path, Some(body), None).await
    }

    /// POST 请求（带选项）
    pub async fn post_with_options<T: DeserializeOwned>(
        &self,
        path: &str,
        body: impl serde::Serialize,
        options: Option<RequestOptions>,
    ) -> Result<T> {
        let body = serde_json::to_value(body).map_err(|e| {
            CredBridgeError::new(
                CredBridgeErrorCode::InvalidRequest,
                format!("Failed to serialize request body: {}", e),
            )
        })?;
        self.request(Method::POST, path, Some(body), options).await
    }

    /// PUT 请求
    pub async fn put<T: DeserializeOwned>(
        &self,
        path: &str,
        body: impl serde::Serialize,
    ) -> Result<T> {
        let body = serde_json::to_value(body).map_err(|e| {
            CredBridgeError::new(
                CredBridgeErrorCode::InvalidRequest,
                format!("Failed to serialize request body: {}", e),
            )
        })?;
        self.request(Method::PUT, path, Some(body), None).await
    }

    /// DELETE 请求
    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.request(Method::DELETE, path, None, None).await
    }

    /// DELETE 请求（带选项）
    pub async fn delete_with_options<T: DeserializeOwned>(
        &self,
        path: &str,
        options: Option<RequestOptions>,
    ) -> Result<T> {
        self.request(Method::DELETE, path, None, options).await
    }

    /// PATCH 请求
    pub async fn patch<T: DeserializeOwned>(
        &self,
        path: &str,
        body: impl serde::Serialize,
    ) -> Result<T> {
        let body = serde_json::to_value(body).map_err(|e| {
            CredBridgeError::new(
                CredBridgeErrorCode::InvalidRequest,
                format!("Failed to serialize request body: {}", e),
            )
        })?;
        self.request(Method::PATCH, path, Some(body), None).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_request_id() {
        let id1 = generate_request_id();
        let id2 = generate_request_id();
        assert_ne!(id1, id2);
        assert!(id1.starts_with("req_"));
    }

    #[test]
    fn test_calculate_backoff_delay() {
        assert_eq!(calculate_backoff_delay(0, 1000), 1000);
        assert_eq!(calculate_backoff_delay(1, 1000), 2000);
        assert_eq!(calculate_backoff_delay(2, 1000), 4000);
    }

    #[test]
    fn test_build_url() {
        let config = CredBridgeConfig::new("https://api.credbridge.io");
        let client = CredBridgeClient::new(config).unwrap();

        assert_eq!(
            client.build_url("/credentials").unwrap(),
            "https://api.credbridge.io/api/v1/credentials"
        );
        assert_eq!(
            client.build_url("credentials").unwrap(),
            "https://api.credbridge.io/api/v1/credentials"
        );

        let config = CredBridgeConfig::new("https://api.credbridge.io/");
        let client = CredBridgeClient::new(config).unwrap();
        assert_eq!(
            client.build_url("/credentials").unwrap(),
            "https://api.credbridge.io/api/v1/credentials"
        );
    }
}
