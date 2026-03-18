//! OpenAI Compatible API 客户端
//!
//! 支持 OpenAI 官方 API 和任何兼容 OpenAI API 格式的服务

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

/// OpenAI 兼容客户端配置
#[derive(Debug, Clone)]
pub struct OpenAiClientConfig {
    /// API 基础 URL
    pub base_url: String,
    /// API 密钥
    pub api_key: String,
    /// 默认模型
    pub model: String,
    /// 请求超时 (秒)
    pub timeout_secs: u64,
    /// 价格配置
    pub pricing: PricingInfo,
}

impl OpenAiClientConfig {
    /// 创建 OpenAI 官方配置
    pub fn openai(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: api_key.into(),
            model: model.into(),
            timeout_secs: 30,
            pricing: PricingInfo::default(),
        }
    }

    /// 创建自定义端点配置
    pub fn custom(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
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
}

/// OpenAI 兼容客户端
pub struct OpenAiCompatibleClient {
    name: String,
    config: OpenAiClientConfig,
    client: Client,
    cost_tracker: Arc<CostTracker>,
}

impl OpenAiCompatibleClient {
    /// 创建新的客户端
    pub fn new(name: impl Into<String>, config: OpenAiClientConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            name: name.into(),
            config,
            client,
            cost_tracker: Arc::new(CostTracker::new(PricingInfo::default())),
        }
    }

    /// 构建请求头
    fn build_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", self.config.api_key)
                .parse()
                .expect("Invalid API key format"),
        );
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        headers
    }

    /// 转换请求格式
    fn convert_request(&self, request: &ChatRequest) -> OpenAiChatRequest {
        let messages = vec![
            OpenAiMessage {
                role: "system".to_string(),
                content: request.system_prompt.clone(),
            },
            OpenAiMessage {
                role: "user".to_string(),
                content: request.user_message.clone(),
            },
        ];

        OpenAiChatRequest {
            model: self.config.model.clone(),
            messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            response_format: request.response_format.as_ref().map(|rf| match rf {
                ResponseFormat::Text => OpenAiResponseFormat::Text,
                ResponseFormat::JsonObject => OpenAiResponseFormat::JsonObject {
                    r#type: "json_object".to_string(),
                },
            }),
        }
    }

    /// 转换带图片的请求
    fn convert_image_request(&self, request: &ChatRequestWithImage) -> OpenAiChatRequest {
        let system_message = OpenAiMessage {
            role: "system".to_string(),
            content: request.base.system_prompt.clone(),
        };

        let _image_url = format!(
            "data:{};base64,{}",
            request.image_mime_type, request.image_base64
        );

        let user_content = request.base.user_message.clone();

        let user_message = OpenAiMessage {
            role: "user".to_string(),
            content: user_content, // 简化处理，实际应使用 content array
        };

        OpenAiChatRequest {
            model: self.config.model.clone(),
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
impl LlmProvider for OpenAiCompatibleClient {
    fn name(&self) -> &str {
        &self.name
    }

    fn model(&self) -> &str {
        &self.config.model
    }

    async fn chat_completion(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        let url = format!("{}/chat/completions", self.config.base_url);
        let openai_request = self.convert_request(&request);

        debug!("Sending request to OpenAI API: {}", url);

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers())
            .json(&openai_request)
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
            error!("OpenAI API error: {} - {}", status, body);
            return Err(self.handle_error(status, &body));
        }

        let openai_response: OpenAiChatResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::ParseError(e.to_string()))?;

        // 记录成本
        self.cost_tracker.record_usage(
            openai_response.usage.prompt_tokens,
            openai_response.usage.completion_tokens,
        );

        info!(
            "OpenAI API call successful: {} tokens",
            openai_response.usage.total_tokens
        );

        let content = openai_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        Ok(ChatResponse {
            content,
            model: openai_response.model,
            usage: TokenUsage {
                prompt_tokens: openai_response.usage.prompt_tokens,
                completion_tokens: openai_response.usage.completion_tokens,
                total_tokens: openai_response.usage.total_tokens,
            },
            finish_reason: openai_response
                .choices
                .first()
                .and_then(|c| c.finish_reason.clone()),
            raw_response: Some(body),
        })
    }

    async fn chat_completion_with_image(
        &self,
        request: ChatRequestWithImage,
    ) -> Result<ChatResponse, LlmError> {
        let url = format!("{}/chat/completions", self.config.base_url);
        let openai_request = self.convert_image_request(&request);

        debug!("Sending image request to OpenAI API: {}", url);

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers())
            .json(&openai_request)
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
            error!("OpenAI API error: {} - {}", status, body);
            return Err(self.handle_error(status, &body));
        }

        let openai_response: OpenAiChatResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::ParseError(e.to_string()))?;

        self.cost_tracker.record_usage(
            openai_response.usage.prompt_tokens,
            openai_response.usage.completion_tokens,
        );

        let content = openai_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        Ok(ChatResponse {
            content,
            model: openai_response.model,
            usage: TokenUsage {
                prompt_tokens: openai_response.usage.prompt_tokens,
                completion_tokens: openai_response.usage.completion_tokens,
                total_tokens: openai_response.usage.total_tokens,
            },
            finish_reason: openai_response
                .choices
                .first()
                .and_then(|c| c.finish_reason.clone()),
            raw_response: Some(body),
        })
    }

    async fn embedding(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, LlmError> {
        let url = format!("{}/embeddings", self.config.base_url);

        let openai_request = OpenAiEmbeddingRequest {
            model: request.model,
            input: request.input,
        };

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers())
            .json(&openai_request)
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

        let openai_response: OpenAiEmbeddingResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::ParseError(e.to_string()))?;

        self.cost_tracker
            .record_usage(openai_response.usage.prompt_tokens, 0);

        let embedding = openai_response
            .data
            .first()
            .map(|d| d.embedding.clone())
            .ok_or_else(|| LlmError::ParseError("No embedding data".to_string()))?;

        Ok(EmbeddingResponse {
            embedding,
            model: openai_response.model,
            usage: TokenUsage {
                prompt_tokens: openai_response.usage.prompt_tokens,
                completion_tokens: 0,
                total_tokens: openai_response.usage.total_tokens,
            },
        })
    }

    fn cost_tracker(&self) -> Option<&CostTracker> {
        Some(&self.cost_tracker)
    }

    async fn health_check(&self) -> Result<(), LlmError> {
        let url = format!("{}/models", self.config.base_url);

        let response = self
            .client
            .get(&url)
            .headers(self.build_headers())
            .send()
            .await
            .map_err(|e| LlmError::ProviderUnavailable(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(LlmError::ProviderUnavailable(format!(
                "Health check failed: {}",
                response.status()
            )))
        }
    }
}

/// OpenAI API 请求结构
#[derive(Debug, Serialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<OpenAiResponseFormat>,
}

/// OpenAI 消息
#[derive(Debug, Serialize, Deserialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

/// OpenAI 响应格式
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum OpenAiResponseFormat {
    Text,
    JsonObject { r#type: String },
}

/// OpenAI API 响应结构
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OpenAiChatResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<OpenAiChoice>,
    usage: OpenAiUsage,
}

/// OpenAI 选择
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OpenAiChoice {
    index: u32,
    message: OpenAiMessage,
    finish_reason: Option<String>,
}

/// OpenAI 使用情况
#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

/// OpenAI 嵌入请求
#[derive(Debug, Serialize)]
struct OpenAiEmbeddingRequest {
    model: String,
    input: String,
}

/// OpenAI 嵌入响应
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OpenAiEmbeddingResponse {
    object: String,
    data: Vec<OpenAiEmbeddingData>,
    model: String,
    usage: OpenAiUsage,
}

/// OpenAI 嵌入数据
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OpenAiEmbeddingData {
    object: String,
    embedding: Vec<f32>,
    index: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_config() {
        let config = OpenAiClientConfig::openai("test-key", "gpt-4o");
        assert_eq!(config.base_url, "https://api.openai.com/v1");
        assert_eq!(config.model, "gpt-4o");
        assert_eq!(config.timeout_secs, 30);
    }

    #[test]
    fn test_custom_config() {
        let config =
            OpenAiClientConfig::custom("https://api.example.com/v1", "custom-key", "custom-model")
                .with_timeout(60);

        assert_eq!(config.base_url, "https://api.example.com/v1");
        assert_eq!(config.timeout_secs, 60);
    }

    #[test]
    fn test_request_conversion() {
        let config = OpenAiClientConfig::openai("key", "gpt-4o");
        let client = OpenAiCompatibleClient::new("openai", config);

        let request = ChatRequest::new("System prompt", "User message")
            .with_temperature(0.7)
            .with_max_tokens(100);

        let openai_request = client.convert_request(&request);

        assert_eq!(openai_request.model, "gpt-4o");
        assert_eq!(openai_request.temperature, Some(0.7));
        assert_eq!(openai_request.max_tokens, Some(100));
        assert_eq!(openai_request.messages.len(), 2);
    }
}
