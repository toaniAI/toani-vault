//! LLM 服务类型定义

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 聊天请求
#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    /// 系统提示词
    pub system_prompt: String,
    /// 用户消息
    pub user_message: String,
    /// 温度参数 (0.0 - 2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// 最大生成 token 数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// 响应格式
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    /// 额外参数
    #[serde(skip_serializing_if = "HashMap::is_empty", flatten)]
    pub extra_params: HashMap<String, serde_json::Value>,
}

impl ChatRequest {
    /// 创建新的聊天请求
    pub fn new(system_prompt: impl Into<String>, user_message: impl Into<String>) -> Self {
        Self {
            system_prompt: system_prompt.into(),
            user_message: user_message.into(),
            temperature: None,
            max_tokens: None,
            response_format: None,
            extra_params: HashMap::new(),
        }
    }

    /// 设置温度参数
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature.clamp(0.0, 2.0));
        self
    }

    /// 设置最大 token 数
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// 设置 JSON 响应格式
    pub fn with_json_response(mut self) -> Self {
        self.response_format = Some(ResponseFormat::JsonObject);
        self
    }
}

/// 支持图片的聊天请求
#[derive(Debug, Clone, Serialize)]
pub struct ChatRequestWithImage {
    /// 基础请求
    #[serde(flatten)]
    pub base: ChatRequest,
    /// 图片数据 (base64)
    pub image_base64: String,
    /// 图片 MIME 类型
    pub image_mime_type: String,
}

impl ChatRequestWithImage {
    /// 创建新的带图片聊天请求
    pub fn new(
        system_prompt: impl Into<String>,
        user_message: impl Into<String>,
        image_data: &[u8],
        mime_type: impl Into<String>,
    ) -> Self {
        use base64::Engine;
        let image_base64 = base64::engine::general_purpose::STANDARD.encode(image_data);

        Self {
            base: ChatRequest::new(system_prompt, user_message),
            image_base64,
            image_mime_type: mime_type.into(),
        }
    }

    /// 设置 JSON 响应格式
    pub fn with_json_response(mut self) -> Self {
        self.base.response_format = Some(ResponseFormat::JsonObject);
        self
    }
}

/// 响应格式
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseFormat {
    /// 文本格式
    Text,
    /// JSON 对象格式
    JsonObject,
}

/// 聊天响应
#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponse {
    /// 生成的内容
    pub content: String,
    /// 使用的模型
    pub model: String,
    /// 使用的 token 数
    pub usage: TokenUsage,
    /// 完成原因
    pub finish_reason: Option<String>,
    /// 原始响应 (用于调试)
    #[serde(skip)]
    pub raw_response: Option<String>,
}

/// Token 使用情况
#[derive(Debug, Clone, Deserialize, Default)]
pub struct TokenUsage {
    /// 输入 token 数
    pub prompt_tokens: u32,
    /// 输出 token 数
    pub completion_tokens: u32,
    /// 总 token 数
    pub total_tokens: u32,
}

impl TokenUsage {
    /// 计算成本 (美元)
    pub fn calculate_cost(&self, input_price_per_1k: f64, output_price_per_1k: f64) -> f64 {
        let input_cost = (self.prompt_tokens as f64 / 1000.0) * input_price_per_1k;
        let output_cost = (self.completion_tokens as f64 / 1000.0) * output_price_per_1k;
        input_cost + output_cost
    }
}

/// 嵌入请求
#[derive(Debug, Clone, Serialize)]
pub struct EmbeddingRequest {
    /// 输入文本
    pub input: String,
    /// 模型名称
    pub model: String,
}

/// 嵌入响应
#[derive(Debug, Clone, Deserialize)]
pub struct EmbeddingResponse {
    /// 嵌入向量
    pub embedding: Vec<f32>,
    /// 使用的模型
    pub model: String,
    /// 使用的 token 数
    pub usage: TokenUsage,
}

/// LLM 提供商配置
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    /// 提供商类型
    pub provider_type: ProviderType,
    /// API 基础 URL
    pub base_url: String,
    /// API 密钥
    pub api_key: String,
    /// 默认模型
    pub model: String,
    /// 超时时间 (毫秒)
    pub timeout_ms: u64,
    /// 价格配置
    pub pricing: PricingConfig,
}

/// 提供商类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    /// OpenAI 兼容 API
    OpenAiCompatible,
    /// Azure OpenAI
    AzureOpenAi,
    /// Anthropic Claude
    Claude,
    /// 模拟提供商 (开发测试)
    Mock,
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderType::OpenAiCompatible => write!(f, "openai_compatible"),
            ProviderType::AzureOpenAi => write!(f, "azure_openai"),
            ProviderType::Claude => write!(f, "claude"),
            ProviderType::Mock => write!(f, "mock"),
        }
    }
}

/// 价格配置
#[derive(Debug, Clone, Deserialize)]
pub struct PricingConfig {
    /// 输入价格 (每 1000 tokens，美元)
    pub input_price_per_1k: f64,
    /// 输出价格 (每 1000 tokens，美元)
    pub output_price_per_1k: f64,
}

impl Default for PricingConfig {
    fn default() -> Self {
        Self {
            input_price_per_1k: 0.0,
            output_price_per_1k: 0.0,
        }
    }
}

/// LLM 服务配置
#[derive(Debug, Clone, Deserialize)]
pub struct LlmServiceConfig {
    /// 默认提供商
    pub default_provider: String,
    /// 提供商配置列表
    pub providers: HashMap<String, ProviderConfig>,
    /// 路由配置
    pub routing: RoutingConfig,
    /// 成本控制配置
    pub cost_control: CostControlConfig,
}

/// 路由配置
#[derive(Debug, Clone, Deserialize)]
pub struct RoutingConfig {
    /// 操作审核使用的提供商
    pub operation_review: String,
    /// 内容审核使用的提供商
    pub content_review: String,
    /// 代码生成使用的提供商
    pub code_generation: String,
}

/// 成本控制配置
#[derive(Debug, Clone, Deserialize)]
pub struct CostControlConfig {
    /// 每月最大成本 (美元)
    pub max_monthly_cost_usd: f64,
    /// 成本阈值触发的降级链
    pub fallback_chain: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_request_builder() {
        let request = ChatRequest::new("You are a helpful assistant", "Hello")
            .with_temperature(0.7)
            .with_max_tokens(100)
            .with_json_response();

        assert_eq!(request.system_prompt, "You are a helpful assistant");
        assert_eq!(request.user_message, "Hello");
        assert_eq!(request.temperature, Some(0.7));
        assert_eq!(request.max_tokens, Some(100));
        assert!(matches!(
            request.response_format,
            Some(ResponseFormat::JsonObject)
        ));
    }

    #[test]
    fn test_token_usage_cost_calculation() {
        let usage = TokenUsage {
            prompt_tokens: 1000,
            completion_tokens: 500,
            total_tokens: 1500,
        };

        // OpenAI GPT-4o pricing
        let cost = usage.calculate_cost(0.005, 0.015);
        assert!((cost - 0.0125).abs() < 0.0001);
    }

    #[test]
    fn test_provider_type_display() {
        assert_eq!(
            ProviderType::OpenAiCompatible.to_string(),
            "openai_compatible"
        );
        assert_eq!(ProviderType::Mock.to_string(), "mock");
    }
}
