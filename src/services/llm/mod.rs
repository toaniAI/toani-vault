//! LLM 服务模块
//!
//! 提供多提供商 LLM 服务支持，包括：
//! - OpenAI Compatible API
//! - Azure OpenAI
//! - Mock Provider (零成本开发测试)
//!
//! # 使用示例
//!
//! ```rust
//! use credbridge::services::llm::{LlmService, ChatRequest, LlmProvider};
//!
//! // 创建服务
//! let service = LlmService::new(config);
//! service.initialize().await?;
//!
//! // 执行聊天补全
//! let request = ChatRequest::new("System prompt", "User message");
//! let response = service.chat_completion(request).await?;
//!
//! println!("Response: {}", response.content);
//! ```

pub mod azure;
pub mod claude;
pub mod cost;
pub mod mock;
pub mod openai;
pub mod provider;
pub mod service;
pub mod types;

// 公开导出 - 类型
pub use types::{
    ChatRequest, ChatRequestWithImage, ChatResponse, EmbeddingRequest, EmbeddingResponse,
    LlmServiceConfig, ProviderConfig, ProviderType, ResponseFormat, TokenUsage,
};

// 公开导出 - Provider
pub use provider::{LlmError, LlmProvider};

// 公开导出 - 服务
pub use service::{
    BudgetStatus, HealthCheckResult, LlmService, ProviderStatus, create_mock_service,
};

// 公开导出 - 成本
pub use cost::{CostController, CostTracker, PricingInfo, UsageStats, pricing};

// 公开导出 - Mock
pub use mock::{MockLlmProvider, MockProviderConfig, presets as mock_presets};

// 公开导出 - OpenAI
pub use openai::{OpenAiClientConfig, OpenAiCompatibleClient};

// 公开导出 - Azure OpenAI
pub use azure::{AzureOpenAiClient, AzureOpenAiConfig};

// 公开导出 - Claude
pub use claude::{ClaudeClient, ClaudeConfig};

/// LLM 服务版本
pub const LLM_SERVICE_VERSION: &str = "0.1.0";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert_eq!(LLM_SERVICE_VERSION, "0.1.0");
    }

    #[tokio::test]
    async fn test_end_to_end_mock() {
        // 使用 Mock 服务进行端到端测试
        let service = create_mock_service().await;

        let request =
            ChatRequest::new("You are a helpful assistant", "What is 2 + 2?").with_temperature(0.7);

        let response = service.chat_completion(request).await.unwrap();
        assert!(!response.content.is_empty());
        assert!(response.usage.total_tokens > 0);
    }
}
