//! Azure OpenAI API 客户端
//!
//! 支持 Azure OpenAI Service 的 API 调用
//! API 格式: {endpoint}/openai/deployments/{deployment_id}/chat/completions?api-version={api_version}

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info};

use crate::services::llm::{
    cost::{CostTracker, PricingInfo},
    provider::{LlmError, LlmProvider},
    types::{
        ChatRequest, ChatRequestWithImage, ChatResponse, EmbeddingRequest, EmbeddingResponse,
        ResponseFormat, TokenUsage,
    },
};

/// Azure OpenAI 客户端配置
#[derive(Debug, Clone)]
pub struct AzureOpenAiConfig {
    /// Azure OpenAI 端点 (e.g., https://{resource-name}.openai.azure.com)
    pub endpoint: String,
    /// 部署 ID (模型部署名称)
    pub deployment_id: String,
    /// API 密钥
    pub api_key: String,
    /// API 版本 (e.g., "2024-02-01")
    pub api_version: String,
    /// 请求超时 (秒)
    pub timeout_secs: u64,
    /// 价格配置
    pub pricing: PricingInfo,
}

impl AzureOpenAiConfig {
    /// 创建新的 Azure OpenAI 配置
    pub fn new(
        endpoint: impl Into<String>,
        deployment_id: impl Into<String>,
        api_key: impl Into<String>,
        api_version: impl Into<String>,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            deployment_id: deployment_id.into(),
            api_key: api_key.into(),
            api_version: api_version.into(),
            timeout_secs: 30,
            pricing: PricingInfo::default(),
        }
    }

    /// 设置价格配置
    pub fn with_pricing(mut self, pricing: PricingInfo) -> Self {
        self.pricing = pricing;
        self
    }

    /// 设置超时
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// 构建完整的 API URL
    fn build_url(&self, operation: &str) -> String {
        format!(
            "{}/openai/deployments/{}/{}?api-version={}",
            self.endpoint.trim_end_matches('/'),
            self.deployment_id,
            operation,
            self.api_version
        )
    }
}

/// Azure OpenAI 客户端
pub struct AzureOpenAiClient {
    name: String,
    config: AzureOpenAiConfig,
    client: Client,
    cost_tracker: Arc<CostTracker>,
}

impl AzureOpenAiClient {
    /// 创建新的 Azure OpenAI 客户端
    pub fn new(name: impl Into<String>, config: AzureOpenAiConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            name: name.into(),
            config: config.clone(),
            client,
            cost_tracker: Arc::new(CostTracker::new(config.pricing.clone())),
        }
    }

    /// 构建请求头
    fn build_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        // Azure OpenAI 使用 api-key 头而非 Authorization
        headers.insert(
            "api-key",
            self.config.api_key.parse().expect("Invalid API key format"),
        );
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        headers
    }

    /// 转换请求格式
    fn convert_request(&self, request: &ChatRequest) -> AzureChatRequest {
        let messages = vec![
            AzureMessage {
                role: "system".to_string(),
                content: AzureMessageContent::Text(request.system_prompt.clone()),
            },
            AzureMessage {
                role: "user".to_string(),
                content: AzureMessageContent::Text(request.user_message.clone()),
            },
        ];

        AzureChatRequest {
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            response_format: request.response_format.as_ref().map(|rf| match rf {
                ResponseFormat::Text => AzureResponseFormat::Text,
                ResponseFormat::JsonObject => AzureResponseFormat::JsonObject {
                    r#type: "json_object".to_string(),
                },
            }),
        }
    }

    /// 转换带图片的请求
    fn convert_image_request(&self, request: &ChatRequestWithImage) -> AzureChatRequest {
        let system_message = AzureMessage {
            role: "system".to_string(),
            content: AzureMessageContent::Text(request.base.system_prompt.clone()),
        };

        let image_url = format!(
            "data:{};base64,{}",
            request.image_mime_type, request.image_base64
        );

        // 构建 content array：文本 + 图片
        let user_message = AzureMessage {
            role: "user".to_string(),
            content: AzureMessageContent::Parts(vec![
                AzureContentPart::Text {
                    text: request.base.user_message.clone(),
                },
                AzureContentPart::ImageUrl {
                    image_url: AzureImageUrlContent { url: image_url },
                },
            ]),
        };

        AzureChatRequest {
            messages: vec![system_message, user_message],
            temperature: request.base.temperature,
            max_tokens: request.base.max_tokens,
            response_format: None,
        }
    }

    /// 处理 API 错误
    fn handle_error(&self, status: StatusCode, body: &str) -> LlmError {
        match status {
            StatusCode::UNAUTHORIZED => LlmError::Authentication("Invalid API key".to_string()),
            StatusCode::TOO_MANY_REQUESTS => LlmError::RateLimited,
            StatusCode::BAD_REQUEST => LlmError::InvalidRequest(body.to_string()),
            StatusCode::REQUEST_TIMEOUT => LlmError::Timeout,
            _ => LlmError::api(status.as_u16(), body),
        }
    }
}

#[async_trait]
impl LlmProvider for AzureOpenAiClient {
    fn name(&self) -> &str {
        &self.name
    }

    fn model(&self) -> &str {
        &self.config.deployment_id
    }

    async fn chat_completion(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        let url = self.config.build_url("chat/completions");
        let azure_request = self.convert_request(&request);

        debug!("Sending request to Azure OpenAI API: {}", url);

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers())
            .json(&azure_request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout
                } else {
                    LlmError::Network(e.to_string())
                }
            })?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| LlmError::Network(e.to_string()))?;

        if !status.is_success() {
            error!("Azure OpenAI API error: {} - {}", status, body);
            return Err(self.handle_error(status, &body));
        }

        let azure_response: AzureChatResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::ParseError(e.to_string()))?;

        // 记录成本
        self.cost_tracker.record_usage(
            azure_response.usage.prompt_tokens,
            azure_response.usage.completion_tokens,
        );

        info!(
            "Azure OpenAI API call successful: {} tokens",
            azure_response.usage.total_tokens
        );

        // 检查 choices 是否为空，区分正常空结果和错误情况
        let choice = azure_response
            .choices
            .first()
            .ok_or_else(|| LlmError::InvalidRequest("API returned empty choices (possible rate limiting, content filtering, or model error)".to_string()))?;

        let content = match &choice.message.content {
            AzureMessageContent::Text(s) => s.clone(),
            AzureMessageContent::Parts(parts) => parts
                .iter()
                .filter_map(|p| {
                    if let AzureContentPart::Text { text } = p {
                        Some(text.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(""),
        };

        Ok(ChatResponse {
            content,
            model: azure_response.model,
            usage: TokenUsage {
                prompt_tokens: azure_response.usage.prompt_tokens,
                completion_tokens: azure_response.usage.completion_tokens,
                total_tokens: azure_response.usage.total_tokens,
            },
            finish_reason: choice.finish_reason.clone(),
            raw_response: Some(body),
        })
    }

    async fn chat_completion_with_image(
        &self,
        request: ChatRequestWithImage,
    ) -> Result<ChatResponse, LlmError> {
        let url = self.config.build_url("chat/completions");
        let azure_request = self.convert_image_request(&request);

        debug!("Sending image request to Azure OpenAI API: {}", url);

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers())
            .json(&azure_request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout
                } else {
                    LlmError::Network(e.to_string())
                }
            })?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| LlmError::Network(e.to_string()))?;

        if !status.is_success() {
            error!("Azure OpenAI API error: {} - {}", status, body);
            return Err(self.handle_error(status, &body));
        }

        let azure_response: AzureChatResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::ParseError(e.to_string()))?;

        self.cost_tracker.record_usage(
            azure_response.usage.prompt_tokens,
            azure_response.usage.completion_tokens,
        );

        // 检查 choices 是否为空，区分正常空结果和错误情况
        let choice = azure_response
            .choices
            .first()
            .ok_or_else(|| LlmError::InvalidRequest("API returned empty choices (possible rate limiting, content filtering, or model error)".to_string()))?;

        let content = match &choice.message.content {
            AzureMessageContent::Text(s) => s.clone(),
            AzureMessageContent::Parts(parts) => parts
                .iter()
                .filter_map(|p| {
                    if let AzureContentPart::Text { text } = p {
                        Some(text.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(""),
        };

        Ok(ChatResponse {
            content,
            model: azure_response.model,
            usage: TokenUsage {
                prompt_tokens: azure_response.usage.prompt_tokens,
                completion_tokens: azure_response.usage.completion_tokens,
                total_tokens: azure_response.usage.total_tokens,
            },
            finish_reason: choice.finish_reason.clone(),
            raw_response: Some(body),
        })
    }

    async fn embedding(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, LlmError> {
        let url = self.config.build_url("embeddings");

        let azure_request = AzureEmbeddingRequest {
            input: request.input,
        };

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers())
            .json(&azure_request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout
                } else {
                    LlmError::Network(e.to_string())
                }
            })?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| LlmError::Network(e.to_string()))?;

        if !status.is_success() {
            return Err(self.handle_error(status, &body));
        }

        let azure_response: AzureEmbeddingResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::ParseError(e.to_string()))?;

        self.cost_tracker
            .record_usage(azure_response.usage.prompt_tokens, 0);

        let embedding = azure_response
            .data
            .first()
            .map(|d| d.embedding.clone())
            .ok_or_else(|| LlmError::ParseError("No embedding data".to_string()))?;

        Ok(EmbeddingResponse {
            embedding,
            model: azure_response.model,
            usage: TokenUsage {
                prompt_tokens: azure_response.usage.prompt_tokens,
                completion_tokens: 0,
                total_tokens: azure_response.usage.total_tokens,
            },
        })
    }

    fn cost_tracker(&self) -> Option<&CostTracker> {
        Some(&self.cost_tracker)
    }

    async fn health_check(&self) -> Result<(), LlmError> {
        // Azure OpenAI 没有直接的模型列表端点，使用一个简单的请求来检查
        let url = self.config.build_url("chat/completions");

        // 构建一个最小化的健康检查请求
        let health_request = AzureChatRequest {
            messages: vec![AzureMessage {
                role: "user".to_string(),
                content: AzureMessageContent::Text("hi".to_string()),
            }],
            temperature: Some(0.0),
            max_tokens: Some(1),
            response_format: None,
        };

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers())
            .json(&health_request)
            .send()
            .await
            .map_err(|e| LlmError::ProviderUnavailable(e.to_string()))?;

        match response.status() {
            status if status.is_success() => Ok(()),
            StatusCode::UNAUTHORIZED => Err(LlmError::Authentication(
                "Invalid Azure API key".to_string(),
            )),
            status => Err(LlmError::ProviderUnavailable(format!(
                "Health check failed: {status}"
            ))),
        }
    }
}

/// Azure OpenAI API 请求结构
#[derive(Debug, Serialize)]
struct AzureChatRequest {
    messages: Vec<AzureMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<AzureResponseFormat>,
}

/// Azure OpenAI 消息 content 枚举，支持纯文本和多部分数组
///
/// Azure OpenAI API 与 OpenAI API 兼容，支持相同的 content array 格式
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum AzureMessageContent {
    /// 纯文本内容
    Text(String),
    /// 多部分内容（文本 + 图片等）
    Parts(Vec<AzureContentPart>),
}

/// Content 数组中的单个部分
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum AzureContentPart {
    /// 文本部分
    #[serde(rename = "text")]
    Text { text: String },
    /// 图片 URL 部分
    #[serde(rename = "image_url")]
    ImageUrl { image_url: AzureImageUrlContent },
}

/// 图片 URL 内容
#[derive(Debug, Serialize, Deserialize)]
struct AzureImageUrlContent {
    /// 图片 URL（支持 data: URI）
    url: String,
}

/// Azure OpenAI 消息
#[derive(Debug, Serialize, Deserialize)]
struct AzureMessage {
    role: String,
    content: AzureMessageContent,
}

/// Azure OpenAI 响应格式
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum AzureResponseFormat {
    Text,
    JsonObject { r#type: String },
}

/// Azure OpenAI API 响应结构
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AzureChatResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<AzureChoice>,
    usage: AzureUsage,
}

/// Azure OpenAI 选择
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AzureChoice {
    index: u32,
    message: AzureMessage,
    finish_reason: Option<String>,
}

/// Azure OpenAI 使用情况
#[derive(Debug, Deserialize)]
struct AzureUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

/// Azure OpenAI 嵌入请求
#[derive(Debug, Serialize)]
struct AzureEmbeddingRequest {
    input: String,
}

/// Azure OpenAI 嵌入响应
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AzureEmbeddingResponse {
    object: String,
    data: Vec<AzureEmbeddingData>,
    model: String,
    usage: AzureUsage,
}

/// Azure OpenAI 嵌入数据
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AzureEmbeddingData {
    object: String,
    embedding: Vec<f32>,
    index: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_azure_config() {
        let config = AzureOpenAiConfig::new(
            "https://test.openai.azure.com",
            "gpt-4o",
            "test-api-key",
            "2024-02-01",
        );

        assert_eq!(config.endpoint, "https://test.openai.azure.com");
        assert_eq!(config.deployment_id, "gpt-4o");
        assert_eq!(config.api_version, "2024-02-01");
        assert_eq!(config.timeout_secs, 30);
    }

    #[test]
    fn test_azure_config_with_pricing() {
        let config = AzureOpenAiConfig::new(
            "https://test.openai.azure.com",
            "gpt-4o",
            "test-api-key",
            "2024-02-01",
        )
        .with_pricing(PricingInfo {
            input_price_per_1k: 0.005,
            output_price_per_1k: 0.015,
        })
        .with_timeout(60);

        assert_eq!(config.pricing.input_price_per_1k, 0.005);
        assert_eq!(config.pricing.output_price_per_1k, 0.015);
        assert_eq!(config.timeout_secs, 60);
    }

    #[test]
    fn test_build_url() {
        let config = AzureOpenAiConfig::new(
            "https://test.openai.azure.com",
            "gpt-4o",
            "test-api-key",
            "2024-02-01",
        );

        let url = config.build_url("chat/completions");
        assert_eq!(
            url,
            "https://test.openai.azure.com/openai/deployments/gpt-4o/chat/completions?api-version=2024-02-01"
        );
    }

    #[test]
    fn test_request_conversion() {
        let config = AzureOpenAiConfig::new(
            "https://test.openai.azure.com",
            "gpt-4o",
            "test-api-key",
            "2024-02-01",
        );
        let client = AzureOpenAiClient::new("azure", config);

        let request = ChatRequest::new("System prompt", "User message")
            .with_temperature(0.7)
            .with_max_tokens(100);

        let azure_request = client.convert_request(&request);

        assert_eq!(azure_request.temperature, Some(0.7));
        assert_eq!(azure_request.max_tokens, Some(100));
        assert_eq!(azure_request.messages.len(), 2);
        assert_eq!(azure_request.messages[0].role, "system");
        assert!(
            matches!(&azure_request.messages[0].content, AzureMessageContent::Text(s) if s == "System prompt")
        );
        assert_eq!(azure_request.messages[1].role, "user");
        assert!(
            matches!(&azure_request.messages[1].content, AzureMessageContent::Text(s) if s == "User message")
        );
    }

    #[test]
    fn test_build_headers() {
        let config = AzureOpenAiConfig::new(
            "https://test.openai.azure.com",
            "gpt-4o",
            "test-api-key",
            "2024-02-01",
        );
        let client = AzureOpenAiClient::new("azure", config);

        let headers = client.build_headers();

        assert_eq!(
            headers.get("api-key").unwrap().to_str().unwrap(),
            "test-api-key"
        );
        assert_eq!(
            headers.get("content-type").unwrap().to_str().unwrap(),
            "application/json"
        );
    }

    #[test]
    fn test_handle_error() {
        let config = AzureOpenAiConfig::new(
            "https://test.openai.azure.com",
            "gpt-4o",
            "test-api-key",
            "2024-02-01",
        );
        let client = AzureOpenAiClient::new("azure", config);

        let err = client.handle_error(StatusCode::UNAUTHORIZED, "");
        assert!(matches!(err, LlmError::Authentication(_)));

        let err = client.handle_error(StatusCode::TOO_MANY_REQUESTS, "");
        assert!(matches!(err, LlmError::RateLimited));

        let err = client.handle_error(StatusCode::BAD_REQUEST, "Invalid request");
        assert!(matches!(err, LlmError::InvalidRequest(_)));
    }
}
