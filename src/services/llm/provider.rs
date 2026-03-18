//! LLM Provider trait 定义

use crate::services::llm::cost::CostTracker;
use crate::services::llm::types::{
    ChatRequest, ChatRequestWithImage, ChatResponse, EmbeddingRequest, EmbeddingResponse,
};
use async_trait::async_trait;
use std::sync::Arc;

/// LLM 提供商 trait
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// 获取提供商名称
    fn name(&self) -> &str;

    /// 获取模型名称
    fn model(&self) -> &str;

    /// 执行聊天补全
    async fn chat_completion(&self, request: ChatRequest) -> Result<ChatResponse, LlmError>;

    /// 执行带图片的聊天补全
    async fn chat_completion_with_image(
        &self,
        request: ChatRequestWithImage,
    ) -> Result<ChatResponse, LlmError>;

    /// 执行嵌入请求 (可选)
    async fn embedding(&self, _request: EmbeddingRequest) -> Result<EmbeddingResponse, LlmError> {
        Err(LlmError::NotSupported("embedding".to_string()))
    }

    /// 获取成本跟踪器
    fn cost_tracker(&self) -> Option<&CostTracker> {
        None
    }

    /// 检查提供商是否健康
    async fn health_check(&self) -> Result<(), LlmError> {
        Ok(())
    }
}

/// LLM 错误类型
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    /// 网络错误
    #[error("网络错误: {0}")]
    Network(String),

    /// API 错误
    #[error("API 错误 (状态码 {status_code}): {message}")]
    Api { status_code: u16, message: String },

    /// 认证错误
    #[error("认证失败: {0}")]
    Authentication(String),

    /// 速率限制
    #[error("速率限制，请稍后重试")]
    RateLimited,

    /// 超时
    #[error("请求超时")]
    Timeout,

    /// 无效的请求
    #[error("无效的请求: {0}")]
    InvalidRequest(String),

    /// 响应解析错误
    #[error("响应解析错误: {0}")]
    ParseError(String),

    /// 不支持的操作
    #[error("不支持的操作: {0}")]
    NotSupported(String),

    /// 配置错误
    #[error("配置错误: {0}")]
    Config(String),

    /// 提供商不可用
    #[error("提供商不可用: {0}")]
    ProviderUnavailable(String),

    /// 其他错误
    #[error("LLM 错误: {0}")]
    Other(String),
}

impl LlmError {
    /// 创建 API 错误
    pub fn api(status_code: u16, message: impl Into<String>) -> Self {
        Self::Api {
            status_code,
            message: message.into(),
        }
    }

    /// 检查是否为可重试错误
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Network(_) | Self::RateLimited | Self::Timeout | Self::ProviderUnavailable(_)
        )
    }

    /// 获取 HTTP 状态码 (如果有)
    pub fn status_code(&self) -> Option<u16> {
        match self {
            Self::Api { status_code, .. } => Some(*status_code),
            _ => None,
        }
    }
}

/// 提供商工厂
#[allow(dead_code)]
trait ProviderFactory {
    /// 根据配置创建提供商
    fn create(config: &ProviderConfig) -> Result<Arc<dyn LlmProvider>, LlmError>;
}

use crate::services::llm::types::ProviderConfig;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_error_api() {
        let err = LlmError::api(401, "Invalid API key");
        assert!(matches!(
            err,
            LlmError::Api {
                status_code: 401,
                ..
            }
        ));
        assert_eq!(err.status_code(), Some(401));
    }

    #[test]
    fn test_llm_error_is_retryable() {
        assert!(LlmError::Timeout.is_retryable());
        assert!(LlmError::RateLimited.is_retryable());
        assert!(LlmError::Network("connection reset".to_string()).is_retryable());
        assert!(!LlmError::api(400, "bad request").is_retryable());
        assert!(!LlmError::Authentication("invalid key".to_string()).is_retryable());
    }
}
