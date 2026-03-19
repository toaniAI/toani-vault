//! HTTP Connector 实现
//!
//! 提供基于 HTTP/HTTPS 的外部服务连接器，支持 REST API 调用。

use crate::connector::error::{ConnectorError, ConnectorResult, ValidationError};
use crate::connector::trait_def::{Connector, ConnectorConfig, ValidatedParams};
use async_trait::async_trait;
use reqwest::{Client, Method, Response};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Duration;

// ============================================================================
// HTTPConnector 配置
// ============================================================================

/// HTTP 连接器配置
#[derive(Debug, Clone)]
pub struct HttpConnectorConfig {
    /// 基础 URL
    pub base_url: String,
    /// 默认请求头
    pub default_headers: HashMap<String, String>,
    /// 请求超时（秒）
    pub timeout_secs: u64,
    /// 连接超时（秒）
    pub connect_timeout_secs: u64,
    /// 是否验证 SSL 证书
    pub verify_ssl: bool,
    /// 重试次数
    pub retry_count: u32,
    /// 重试延迟（毫秒）
    pub retry_delay_ms: u64,
}

impl Default for HttpConnectorConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            default_headers: HashMap::new(),
            timeout_secs: 30,
            connect_timeout_secs: 10,
            verify_ssl: true,
            retry_count: 3,
            retry_delay_ms: 100,
        }
    }
}

impl HttpConnectorConfig {
    /// 创建新配置
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            ..Default::default()
        }
    }

    /// 添加默认请求头
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.default_headers.insert(key.into(), value.into());
        self
    }

    /// 设置超时
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    /// 设置连接超时
    pub fn with_connect_timeout(mut self, connect_timeout_secs: u64) -> Self {
        self.connect_timeout_secs = connect_timeout_secs;
        self
    }

    /// 设置重试次数
    pub fn with_retry(mut self, retry_count: u32) -> Self {
        self.retry_count = retry_count;
        self
    }

    /// 禁用 SSL 验证（仅用于测试）
    pub fn disable_ssl_verification(mut self) -> Self {
        self.verify_ssl = false;
        self
    }

    /// 从 ConnectorConfig 构建
    pub fn from_connector_config(config: &ConnectorConfig) -> ConnectorResult<Self> {
        let base_url = config
            .get_sync("base_url")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .ok_or_else(|| ConnectorError::config("缺少 base_url 配置"))?;

        let mut http_config = Self::new(base_url);

        // 读取可选配置
        if let Some(timeout) = config.get_sync("timeout_secs").and_then(|v| v.as_u64()) {
            http_config = http_config.with_timeout(timeout);
        }

        if let Some(connect_timeout) = config
            .get_sync("connect_timeout_secs")
            .and_then(|v| v.as_u64())
        {
            http_config = http_config.with_connect_timeout(connect_timeout);
        }

        if let Some(retry_count) = config.get_sync("retry_count").and_then(|v| v.as_u64()) {
            http_config = http_config.with_retry(retry_count as u32);
        }

        // 读取默认请求头
        if let Some(headers) = config.get_sync("headers")
            && let Some(headers_map) = headers.as_object()
        {
            for (key, value) in headers_map {
                if let Some(value_str) = value.as_str() {
                    http_config = http_config.with_header(key.clone(), value_str.to_string());
                }
            }
        }

        Ok(http_config)
    }
}

// ============================================================================
// HTTPConnector
// ============================================================================

/// HTTP 连接器
///
/// 用于调用外部 HTTP/HTTPS API 的连接器实现。
///
/// # 支持的操作
///
/// - GET、POST、PUT、DELETE、PATCH 请求
/// - 自定义请求头
/// - 请求体（JSON）
/// - 响应解析
/// - 重试机制
///
/// # 示例
///
/// ```rust,no_run
/// use vault_service::connector::http::{HttpConnector, HttpConnectorConfig};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let config = HttpConnectorConfig::new("https://api.example.com")
///         .with_header("Authorization", "Bearer token")
///         .with_timeout(60);
///
///     let connector = HttpConnector::new("my-http", config);
///     Ok(())
/// }
/// ```
pub struct HttpConnector {
    /// 连接器名称
    name: String,
    /// 连接器描述
    description: String,
    /// 配置
    config: HttpConnectorConfig,
    /// HTTP 客户端
    client: Option<Client>,
    /// 是否已初始化
    initialized: bool,
}

impl HttpConnector {
    /// 创建新的 HTTP 连接器
    pub fn new(name: impl Into<String>, config: HttpConnectorConfig) -> Self {
        Self {
            name: name.into(),
            description: "HTTP 连接器".to_string(),
            config,
            client: None,
            initialized: false,
        }
    }

    /// 设置描述
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// 获取 HTTP 客户端
    fn client(&self) -> ConnectorResult<&Client> {
        self.client
            .as_ref()
            .ok_or_else(|| ConnectorError::internal("连接器未初始化"))
    }

    /// 构建完整 URL
    fn build_url(&self, path: &str) -> String {
        let base = self.config.base_url.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        format!("{}/{}", base, path)
    }

    /// 执行 HTTP 请求
    async fn do_request(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        headers: Option<HashMap<String, String>>,
    ) -> ConnectorResult<Response> {
        let client = self.client()?;
        let url = self.build_url(path);

        // 构建请求
        let mut request_builder = client.request(method.clone(), &url);

        // 添加额外请求头
        if let Some(extra_headers) = headers {
            for (key, value) in extra_headers {
                request_builder = request_builder.header(&key, &value);
            }
        }

        // 添加请求体
        if let Some(body_value) = body {
            request_builder = request_builder.json(body_value);
        }

        // 发送请求（带重试）
        let mut last_error = None;
        for attempt in 0..=self.config.retry_count {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(self.config.retry_delay_ms)).await;
            }

            match request_builder.try_clone().unwrap().send().await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    // 只有网络错误才重试，其他错误直接返回
                    if e.is_timeout() || e.is_connect() || e.is_request() {
                        last_error = Some(e);
                        continue;
                    }
                    return Err(ConnectorError::execution_with_source(
                        format!("HTTP 请求失败: {}", e),
                        e,
                    ));
                }
            }
        }

        Err(ConnectorError::execution(format!(
            "HTTP 请求失败，重试 {} 次后仍失败: {}",
            self.config.retry_count,
            last_error.unwrap()
        )))
    }

    /// GET 请求
    pub async fn get(&self, path: &str) -> ConnectorResult<Value> {
        self.execute_request(Method::GET, path, None, None).await
    }

    /// GET 请求带请求头
    pub async fn get_with_headers(
        &self,
        path: &str,
        headers: HashMap<String, String>,
    ) -> ConnectorResult<Value> {
        self.execute_request(Method::GET, path, None, Some(headers))
            .await
    }

    /// POST 请求
    pub async fn post(&self, path: &str, body: &Value) -> ConnectorResult<Value> {
        self.execute_request(Method::POST, path, Some(body), None)
            .await
    }

    /// POST 请求带请求头
    pub async fn post_with_headers(
        &self,
        path: &str,
        body: &Value,
        headers: HashMap<String, String>,
    ) -> ConnectorResult<Value> {
        self.execute_request(Method::POST, path, Some(body), Some(headers))
            .await
    }

    /// PUT 请求
    pub async fn put(&self, path: &str, body: &Value) -> ConnectorResult<Value> {
        self.execute_request(Method::PUT, path, Some(body), None)
            .await
    }

    /// DELETE 请求
    pub async fn delete(&self, path: &str) -> ConnectorResult<Value> {
        self.execute_request(Method::DELETE, path, None, None).await
    }

    /// 执行请求并解析响应
    async fn execute_request(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        headers: Option<HashMap<String, String>>,
    ) -> ConnectorResult<Value> {
        let response = self.do_request(method, path, body, headers).await?;

        // 检查状态码
        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            return Err(ConnectorError::execution(format!(
                "HTTP 请求失败: 状态码 {}, 响应: {}",
                status, error_body
            )));
        }

        // 解析 JSON 响应
        let json = response
            .json::<Value>()
            .await
            .map_err(|e| ConnectorError::execution_with_source("解析 JSON 响应失败", e))?;

        Ok(json)
    }
}

#[async_trait]
impl Connector for HttpConnector {
    fn name(&self) -> &'static str {
        // 返回静态字符串引用
        // 由于 name 是 String，我们需要使用 Box::leak 来获取 'static 生命周期
        // 但这样会内存泄漏，更好的方式是使用 &'static str
        // 这里我们使用一个临时方案
        Box::leak(self.name.clone().into_boxed_str())
    }

    fn description(&self) -> &'static str {
        Box::leak(self.description.clone().into_boxed_str())
    }

    async fn init(&mut self, config: ConnectorConfig) -> ConnectorResult<()> {
        // 从配置构建 HttpConnectorConfig
        let http_config = HttpConnectorConfig::from_connector_config(&config)?;

        // 更新配置
        self.config = http_config;

        // 构建 HTTP 客户端
        let mut client_builder = Client::builder()
            .timeout(Duration::from_secs(self.config.timeout_secs))
            .connect_timeout(Duration::from_secs(self.config.connect_timeout_secs));

        // 禁用 SSL 验证（仅用于测试）
        if !self.config.verify_ssl {
            client_builder = client_builder.danger_accept_invalid_certs(true);
        }

        // 添加默认请求头
        for (key, value) in &self.config.default_headers {
            client_builder = client_builder.default_headers(
                std::iter::once((
                    reqwest::header::HeaderName::try_from(key.as_str())
                        .map_err(|e| ConnectorError::config(format!("无效的请求头名称: {}", e)))?,
                    reqwest::header::HeaderValue::from_str(value)
                        .map_err(|e| ConnectorError::config(format!("无效的请求头值: {}", e)))?,
                ))
                .collect(),
            );
        }

        let client = client_builder
            .build()
            .map_err(|e| ConnectorError::execution_with_source("创建 HTTP 客户端失败", e))?;

        self.client = Some(client);
        self.initialized = true;

        Ok(())
    }

    async fn validate(&self, params: &Value) -> Result<ValidatedParams, ValidationError> {
        // 验证必需字段
        let method = params.get("method").and_then(|v| v.as_str());
        let path = params.get("path").and_then(|v| v.as_str());

        // 检查 method
        let method = method.ok_or_else(|| ValidationError::field("method", "缺少 HTTP 方法"))?;

        // 验证 HTTP 方法有效性
        let valid_methods = ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];
        if !valid_methods.contains(&method.to_uppercase().as_str()) {
            return Err(ValidationError::with_code(
                Some("method".to_string()),
                format!("无效的 HTTP 方法: {}", method),
                "INVALID_METHOD",
            ));
        }

        // 检查 path
        let path = path.ok_or_else(|| ValidationError::field("path", "缺少请求路径"))?;

        // 验证路径不以 // 开头
        if path.starts_with("//") {
            return Err(ValidationError::with_code(
                Some("path".to_string()),
                "路径格式无效",
                "INVALID_PATH",
            ));
        }

        // 创建验证后的参数
        let mut validated = ValidatedParams::new(params.clone());
        validated.set_metadata("validated_method", json!(method.to_uppercase()));
        validated.set_metadata("validated_path", json!(path));

        Ok(validated)
    }

    async fn execute(&self, params: ValidatedParams) -> ConnectorResult<Value> {
        let raw = params.raw();

        // 解析参数
        let method_str = raw
            .get("method")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ConnectorError::validation("缺少 method 参数"))?;

        let path = raw
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ConnectorError::validation("缺少 path 参数"))?;

        let body = raw.get("body");
        let headers = raw.get("headers").and_then(|v| v.as_object()).map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect::<HashMap<String, String>>()
        });

        // 解析 HTTP 方法
        let method: Method = method_str
            .parse()
            .map_err(|_| ConnectorError::validation_field("无效的 HTTP 方法", "method"))?;

        // 执行请求
        self.execute_request(method, path, body, headers).await
    }

    async fn cleanup(&self) -> ConnectorResult<()> {
        // HTTP 客户端不需要显式清理
        Ok(())
    }

    fn timeout_seconds(&self) -> u64 {
        self.config.timeout_secs
    }

    fn supports_concurrent(&self) -> bool {
        true
    }

    fn metadata(&self) -> HashMap<&str, &str> {
        let mut meta = HashMap::new();
        meta.insert("type", "http");
        meta.insert("base_url", &self.config.base_url);
        meta
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connector::Connector;
    use serde_json::json;

    fn create_test_connector() -> HttpConnector {
        let config = HttpConnectorConfig::new("https://httpbin.org").with_timeout(30);
        HttpConnector::new("test-http", config)
    }

    #[test]
    fn test_http_connector_config() {
        let config = HttpConnectorConfig::new("https://api.example.com")
            .with_header("Authorization", "Bearer token")
            .with_timeout(60)
            .with_connect_timeout(20)
            .with_retry(5);

        assert_eq!(config.base_url, "https://api.example.com");
        assert_eq!(
            config.default_headers.get("Authorization"),
            Some(&"Bearer token".to_string())
        );
        assert_eq!(config.timeout_secs, 60);
        assert_eq!(config.connect_timeout_secs, 20);
        assert_eq!(config.retry_count, 5);
    }

    #[tokio::test]
    async fn test_validate_params() {
        let connector = create_test_connector();

        // 有效参数
        let params = json!({
            "method": "GET",
            "path": "/users"
        });
        let result = connector.validate(&params).await;
        assert!(result.is_ok());

        // 缺少 method
        let params = json!({
            "path": "/users"
        });
        let result = connector.validate(&params).await;
        assert!(result.is_err());

        // 缺少 path
        let params = json!({
            "method": "GET"
        });
        let result = connector.validate(&params).await;
        assert!(result.is_err());

        // 无效的 HTTP 方法
        let params = json!({
            "method": "INVALID",
            "path": "/users"
        });
        let result = connector.validate(&params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_build_url() {
        let connector = create_test_connector();

        assert_eq!(connector.build_url("/users"), "https://httpbin.org/users");
        assert_eq!(connector.build_url("users"), "https://httpbin.org/users");
        assert_eq!(
            connector.build_url("/api/v1/users"),
            "https://httpbin.org/api/v1/users"
        );
    }

    #[tokio::test]
    async fn test_init_connector() {
        let mut connector = create_test_connector();

        let config = ConnectorConfig::new();
        let config = config
            .with_sync("base_url", "https://api.example.com")
            .with_sync("timeout_secs", 60u64);

        let result = connector.init(config).await;
        assert!(result.is_ok());
        assert!(connector.initialized);
    }

    #[tokio::test]
    #[ignore] // 需要网络连接
    async fn test_http_get_request() {
        let mut connector = create_test_connector();

        // 初始化
        let config = ConnectorConfig::new();
        connector.init(config).await.unwrap();

        // 执行 GET 请求
        let params = json!({
            "method": "GET",
            "path": "/get",
            "headers": {
                "X-Test-Header": "test-value"
            }
        });

        let validated = connector.validate(&params).await.unwrap();
        let result = connector.execute(validated).await;

        assert!(result.is_ok());
    }
}
