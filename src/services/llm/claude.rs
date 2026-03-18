//! Anthropic Claude API 客户端
//!
//! 支持 Claude Messages API，包括文本和多模态（图片）功能
//! API 文档: https://docs.anthropic.com/claude/reference/messages_post

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info};

use crate::services::llm::{
    cost::{CostTracker, PricingInfo},
    provider::{LlmError, LlmProvider},
    types::{ChatRequest, ChatRequestWithImage, ChatResponse, TokenUsage},
};

/// Claude API 版本
const CLAUDE_API_VERSION: &str = "2023-06-01";

/// Claude API 基础 URL
const CLAUDE_API_BASE_URL: &str = "https://api.anthropic.com/v1";

/// Claude 客户端配置
#[derive(Debug, Clone)]
pub struct ClaudeConfig {
    /// API 密钥
    pub api_key: String,
    /// 模型名称 (如 claude-3-5-sonnet-20241022, claude-3-opus-20240229)
    pub model: String,
    /// API 版本
    pub api_version: String,
    /// 请求超时 (秒)
    pub timeout_secs: u64,
    /// 价格配置
    pub pricing: PricingInfo,
}

impl ClaudeConfig {
    /// 创建 Claude 配置
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            api_version: CLAUDE_API_VERSION.to_string(),
            timeout_secs: 30,
            pricing: PricingInfo::default(),
        }
    }

    /// 创建 Claude 3.5 Sonnet 配置
    pub fn sonnet(api_key: impl Into<String>) -> Self {
        Self::new(api_key, "claude-3-5-sonnet-20241022")
    }

    /// 创建 Claude 3 Opus 配置
    pub fn opus(api_key: impl Into<String>) -> Self {
        Self::new(api_key, "claude-3-opus-20240229")
    }

    /// 创建 Claude 3 Haiku 配置
    pub fn haiku(api_key: impl Into<String>) -> Self {
        Self::new(api_key, "claude-3-haiku-20240307")
    }

    /// 设置 API 版本
    pub fn with_api_version(mut self, version: impl Into<String>) -> Self {
        self.api_version = version.into();
        self
    }

    /// 设置超时
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// 设置价格配置
    pub fn with_pricing(mut self, pricing: PricingInfo) -> Self {
        self.pricing = pricing;
        self
    }
}

/// Claude 客户端
pub struct ClaudeClient {
    name: String,
    config: ClaudeConfig,
    client: Client,
    cost_tracker: Arc<CostTracker>,
}

impl ClaudeClient {
    /// 创建新的 Claude 客户端
    pub fn new(name: impl Into<String>, config: ClaudeConfig) -> Self {
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
            "x-api-key",
            self.config
                .api_key
                .parse()
                .expect("Invalid API key format"),
        );
        headers.insert(
            "anthropic-version",
            self.config
                .api_version
                .parse()
                .expect("Invalid API version format"),
        );
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        headers
    }

    /// 转换请求格式为 Claude Messages API 格式
    fn convert_request(&self, request: &ChatRequest) -> ClaudeRequest {
        let messages = vec![ClaudeMessage {
            role: "user".to_string(),
            content: request.user_message.clone(),
        }];

        ClaudeRequest {
            model: self.config.model.clone(),
            messages,
            max_tokens: request.max_tokens.unwrap_or(1024),
            temperature: request.temperature,
            system: Some(request.system_prompt.clone()),
        }
    }

    /// 转换带图片的请求为 Claude Messages API 格式
    /// 使用多模态 content blocks 格式，支持图片和文本的组合
    fn convert_image_request(&self, request: &ChatRequestWithImage) -> ClaudeRequestWithContent {
        let messages = vec![ClaudeMessageWithContent {
            role: "user".to_string(),
            content: vec![
                ClaudeContentBlock::Image {
                    source: ClaudeImageSource {
                        type_: "base64".to_string(),
                        media_type: request.image_mime_type.clone(),
                        data: request.image_base64.clone(),
                    },
                },
                ClaudeContentBlock::Text {
                    text: request.base.user_message.clone(),
                },
            ],
        }];

        ClaudeRequestWithContent {
            model: self.config.model.clone(),
            messages,
            max_tokens: request.base.max_tokens.unwrap_or(1024),
            temperature: request.base.temperature,
            system: Some(request.base.system_prompt.clone()),
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

    /// 估算成本 (基于 token 数量)
    pub fn cost_estimate(&self, prompt_tokens: u32, completion_tokens: u32) -> f64 {
        let input_cost = (prompt_tokens as f64 / 1000.0) * self.config.pricing.input_price_per_1k;
        let output_cost = (completion_tokens as f64 / 1000.0) * self.config.pricing.output_price_per_1k;
        input_cost + output_cost
    }
}

#[async_trait]
impl LlmProvider for ClaudeClient {
    fn name(&self) -> &str {
        &self.name
    }

    fn model(&self) -> &str {
        &self.config.model
    }

    async fn chat_completion(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        let url = format!("{}/messages", CLAUDE_API_BASE_URL);
        let claude_request = self.convert_request(&request);

        debug!("Sending request to Claude API: {}", url);

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers())
            .json(&claude_request)
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
            error!("Claude API error: {} - {}", status, body);
            return Err(self.handle_error(status, &body));
        }

        let claude_response: ClaudeResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::ParseError(e.to_string()))?;

        // 记录成本
        self.cost_tracker.record_usage(
            claude_response.usage.input_tokens,
            claude_response.usage.output_tokens,
        );

        info!(
            "Claude API call successful: {} input tokens, {} output tokens",
            claude_response.usage.input_tokens, claude_response.usage.output_tokens
        );

        let content = claude_response
            .content
            .first()
            .map(|c| match c {
                ClaudeContent::Text { text } => text.clone(),
                ClaudeContent::Image { .. } => "[Image content]".to_string(),
            })
            .unwrap_or_default();

        Ok(ChatResponse {
            content,
            model: claude_response.model,
            usage: TokenUsage {
                prompt_tokens: claude_response.usage.input_tokens,
                completion_tokens: claude_response.usage.output_tokens,
                total_tokens: claude_response.usage.input_tokens + claude_response.usage.output_tokens,
            },
            finish_reason: claude_response.stop_reason.clone(),
            raw_response: Some(body),
        })
    }

    async fn chat_completion_with_image(
        &self,
        request: ChatRequestWithImage,
    ) -> Result<ChatResponse, LlmError> {
        let url = format!("{}/messages", CLAUDE_API_BASE_URL);

        // Claude 支持多模态输入，使用 content blocks 格式
        let messages = vec![ClaudeMessageWithContent {
            role: "user".to_string(),
            content: vec![
                ClaudeContentBlock::Image {
                    source: ClaudeImageSource {
                        type_: "base64".to_string(),
                        media_type: request.image_mime_type.clone(),
                        data: request.image_base64.clone(),
                    },
                },
                ClaudeContentBlock::Text {
                    text: request.base.user_message.clone(),
                },
            ],
        }];

        let claude_request = ClaudeRequestWithContent {
            model: self.config.model.clone(),
            messages,
            max_tokens: request.base.max_tokens.unwrap_or(1024),
            temperature: request.base.temperature,
            system: Some(request.base.system_prompt.clone()),
        };

        debug!("Sending image request to Claude API: {}", url);

        let response = self
            .client
            .post(&url)
            .headers(self.build_headers())
            .json(&claude_request)
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
            error!("Claude API error: {} - {}", status, body);
            return Err(self.handle_error(status, &body));
        }

        let claude_response: ClaudeResponse =
            serde_json::from_str(&body).map_err(|e| LlmError::ParseError(e.to_string()))?;

        self.cost_tracker.record_usage(
            claude_response.usage.input_tokens,
            claude_response.usage.output_tokens,
        );

        let content = claude_response
            .content
            .first()
            .map(|c| match c {
                ClaudeContent::Text { text } => text.clone(),
                ClaudeContent::Image { .. } => "[Image content]".to_string(),
            })
            .unwrap_or_default();

        Ok(ChatResponse {
            content,
            model: claude_response.model,
            usage: TokenUsage {
                prompt_tokens: claude_response.usage.input_tokens,
                completion_tokens: claude_response.usage.output_tokens,
                total_tokens: claude_response.usage.input_tokens + claude_response.usage.output_tokens,
            },
            finish_reason: claude_response.stop_reason.clone(),
            raw_response: Some(body),
        })
    }

    fn cost_tracker(&self) -> Option<&CostTracker> {
        Some(&self.cost_tracker)
    }

    async fn health_check(&self) -> Result<(), LlmError> {
        // Claude API 没有专门的健康检查端点，使用一个简单的请求来验证
        let url = format!("{}/models", CLAUDE_API_BASE_URL);

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

/// Claude API 请求结构 (简单文本消息)
#[derive(Debug, Serialize)]
struct ClaudeRequest {
    model: String,
    messages: Vec<ClaudeMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
}

/// Claude 消息 (简单文本版本)
#[derive(Debug, Serialize)]
struct ClaudeMessage {
    role: String,
    content: String,
}

/// Claude API 请求结构 (带 content blocks 的多模态消息)
#[derive(Debug, Serialize)]
struct ClaudeRequestWithContent {
    model: String,
    messages: Vec<ClaudeMessageWithContent>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
}

/// Claude 消息 (content blocks 版本)
#[derive(Debug, Serialize)]
struct ClaudeMessageWithContent {
    role: String,
    content: Vec<ClaudeContentBlock>,
}

/// Claude Content Block (请求用)
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ClaudeContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: ClaudeImageSource },
}

/// Claude 图片源
#[derive(Debug, Serialize, Deserialize)]
struct ClaudeImageSource {
    #[serde(rename = "type")]
    type_: String,
    #[serde(rename = "media_type")]
    media_type: String,
    data: String,
}

/// Claude API 响应结构
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ClaudeResponse {
    id: String,
    #[serde(rename = "type")]
    type_: String,
    role: String,
    model: String,
    content: Vec<ClaudeContent>,
    stop_reason: Option<String>,
    stop_sequence: Option<String>,
    usage: ClaudeUsage,
}

/// Claude Content (响应用)
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ClaudeContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: ClaudeImageSource },
}

/// Claude Token 使用情况
#[derive(Debug, Deserialize)]
struct ClaudeUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_config() {
        let config = ClaudeConfig::new("test-key", "claude-3-5-sonnet-20241022");
        assert_eq!(config.api_key, "test-key");
        assert_eq!(config.model, "claude-3-5-sonnet-20241022");
        assert_eq!(config.api_version, "2023-06-01");
        assert_eq!(config.timeout_secs, 30);
    }

    #[test]
    fn test_claude_preset_configs() {
        let sonnet = ClaudeConfig::sonnet("test-key");
        assert_eq!(sonnet.model, "claude-3-5-sonnet-20241022");

        let opus = ClaudeConfig::opus("test-key");
        assert_eq!(opus.model, "claude-3-opus-20240229");

        let haiku = ClaudeConfig::haiku("test-key");
        assert_eq!(haiku.model, "claude-3-haiku-20240307");
    }

    #[test]
    fn test_request_conversion() {
        let config = ClaudeConfig::new("test-key", "claude-3-5-sonnet-20241022");
        let client = ClaudeClient::new("claude", config);

        let request = ChatRequest::new("You are a helpful assistant", "Hello")
            .with_temperature(0.7)
            .with_max_tokens(100);

        let claude_request = client.convert_request(&request);

        assert_eq!(claude_request.model, "claude-3-5-sonnet-20241022");
        assert_eq!(claude_request.temperature, Some(0.7));
        assert_eq!(claude_request.max_tokens, 100);
        assert_eq!(claude_request.system, Some("You are a helpful assistant".to_string()));
        assert_eq!(claude_request.messages.len(), 1);
        assert_eq!(claude_request.messages[0].role, "user");
        assert_eq!(claude_request.messages[0].content, "Hello");
    }

    #[test]
    fn test_headers_building() {
        let config = ClaudeConfig::new("test-api-key", "claude-3-5-sonnet-20241022");
        let client = ClaudeClient::new("claude", config);

        let headers = client.build_headers();

        assert_eq!(
            headers.get("x-api-key").unwrap().to_str().unwrap(),
            "test-api-key"
        );
        assert_eq!(
            headers.get("anthropic-version").unwrap().to_str().unwrap(),
            "2023-06-01"
        );
        assert_eq!(
            headers.get("content-type").unwrap().to_str().unwrap(),
            "application/json"
        );
    }

    #[test]
    fn test_error_handling() {
        let config = ClaudeConfig::new("test-key", "claude-3-5-sonnet-20241022");
        let client = ClaudeClient::new("claude", config);

        let err = client.handle_error(StatusCode::UNAUTHORIZED, "Invalid API key");
        assert!(matches!(err, LlmError::Authentication(_)));

        let err = client.handle_error(StatusCode::TOO_MANY_REQUESTS, "Rate limited");
        assert!(matches!(err, LlmError::RateLimited));

        let err = client.handle_error(StatusCode::BAD_REQUEST, "Bad request");
        assert!(matches!(err, LlmError::InvalidRequest(_)));

        let err = client.handle_error(StatusCode::REQUEST_TIMEOUT, "Timeout");
        assert!(matches!(err, LlmError::Timeout));
    }
}
