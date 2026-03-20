//! LLM 服务实现
//!
//! 提供高层的 LLM 服务接口，包括：
//! - 成本跟踪
//! - 降级策略
//! - 提供商路由
//! - 结果缓存

use crate::services::llm::{
    azure::{AzureOpenAiClient, AzureOpenAiConfig},
    claude::{ClaudeClient, ClaudeConfig},
    cost::{CostController, CostTracker, UsageStats},
    mock::{MockLlmProvider, presets as mock_presets},
    openai::{OpenAiClientConfig, OpenAiCompatibleClient},
    provider::{LlmError, LlmProvider},
    types::{
        ChatRequest, ChatRequestWithImage, ChatResponse, LlmServiceConfig, ProviderConfig,
        ProviderType,
    },
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// LLM 服务
pub struct LlmService {
    /// 提供商实例
    providers: Arc<RwLock<HashMap<String, Arc<dyn LlmProvider>>>>,
    /// 默认提供商名称
    default_provider: String,
    /// 成本控制器
    cost_controller: CostController,
    /// 配置
    config: LlmServiceConfig,
}

impl LlmService {
    /// 创建新的 LLM 服务
    pub fn new(config: LlmServiceConfig) -> Self {
        let cost_tracker = Arc::new(CostTracker::default());
        let cost_controller =
            CostController::new(cost_tracker, config.cost_control.max_monthly_cost_usd);

        Self {
            providers: Arc::new(RwLock::new(HashMap::new())),
            default_provider: config.default_provider.clone(),
            cost_controller,
            config,
        }
    }

    /// 初始化服务 (加载所有配置的提供商)
    pub async fn initialize(&mut self) -> Result<(), LlmError> {
        info!(
            "Initializing LLM service with {} providers",
            self.config.providers.len()
        );

        let mut providers = self.providers.write().await;

        for (name, provider_config) in &self.config.providers {
            match Self::create_provider(provider_config).await {
                Ok(provider) => {
                    providers.insert(name.clone(), provider);
                    info!("Loaded provider: {}", name);
                }
                Err(e) => {
                    warn!("Failed to load provider '{}': {}", name, e);
                }
            }
        }

        // 如果没有加载任何提供商，添加默认的 mock
        if providers.is_empty() {
            warn!("No providers loaded, using default mock provider");
            providers.insert("mock".to_string(), Arc::new(mock_presets::quick_review()));
            self.default_provider = "mock".to_string();
        }

        info!("LLM service initialized with {} providers", providers.len());
        Ok(())
    }

    /// 注册提供商
    pub async fn register_provider(&self, name: impl Into<String>, provider: Arc<dyn LlmProvider>) {
        let mut providers = self.providers.write().await;
        let name = name.into();
        providers.insert(name.clone(), provider);
        info!("Registered provider: {}", name);
    }

    /// 获取提供商
    pub async fn get_provider(&self, name: &str) -> Option<Arc<dyn LlmProvider>> {
        let providers = self.providers.read().await;
        providers.get(name).cloned()
    }

    /// 获取默认提供商
    pub async fn default_provider(&self) -> Option<Arc<dyn LlmProvider>> {
        self.get_provider(&self.default_provider).await
    }

    /// 执行聊天补全 (使用默认提供商)
    pub async fn chat_completion(&self, request: ChatRequest) -> Result<ChatResponse, LlmError> {
        // 检查成本限制
        if !self.cost_controller.can_proceed() {
            return Err(LlmError::Other("Monthly budget exceeded".to_string()));
        }

        let provider = self
            .default_provider()
            .await
            .ok_or_else(|| LlmError::Config("No default provider available".to_string()))?;

        let response = provider.chat_completion(request).await?;

        // 记录成本使用情况
        self.cost_controller.tracker().record_usage(
            response.usage.prompt_tokens,
            response.usage.completion_tokens,
        );

        Ok(response)
    }

    /// 执行聊天补全 (使用指定提供商)
    pub async fn chat_completion_with_provider(
        &self,
        provider_name: &str,
        request: ChatRequest,
    ) -> Result<ChatResponse, LlmError> {
        if !self.cost_controller.can_proceed() {
            return Err(LlmError::Other("Monthly budget exceeded".to_string()));
        }

        let provider = self
            .get_provider(provider_name)
            .await
            .ok_or_else(|| LlmError::Config(format!("Provider '{provider_name}' not found")))?;

        provider.chat_completion(request).await
    }

    /// 执行带图片的聊天补全
    pub async fn chat_completion_with_image(
        &self,
        request: ChatRequestWithImage,
    ) -> Result<ChatResponse, LlmError> {
        if !self.cost_controller.can_proceed() {
            return Err(LlmError::Other("Monthly budget exceeded".to_string()));
        }

        let provider = self
            .default_provider()
            .await
            .ok_or_else(|| LlmError::Config("No default provider available".to_string()))?;

        provider.chat_completion_with_image(request).await
    }

    /// 获取成本统计
    pub fn cost_stats(&self) -> UsageStats {
        self.cost_controller.tracker().stats()
    }

    /// 获取预算使用情况
    pub fn budget_status(&self) -> BudgetStatus {
        BudgetStatus {
            total_cost: self.cost_controller.tracker().total_cost_usd(),
            max_budget: self.config.cost_control.max_monthly_cost_usd,
            remaining: self.cost_controller.remaining_budget(),
            usage_ratio: self.cost_controller.budget_usage_ratio(),
        }
    }

    /// 健康检查
    pub async fn health_check(&self) -> HealthCheckResult {
        let providers = self.providers.read().await;
        let mut results = HashMap::new();

        for (name, provider) in providers.iter() {
            let status = match provider.health_check().await {
                Ok(()) => ProviderStatus::Healthy,
                Err(e) => ProviderStatus::Unhealthy(e.to_string()),
            };
            results.insert(name.clone(), status);
        }

        HealthCheckResult { providers: results }
    }

    /// 创建提供商实例
    async fn create_provider(config: &ProviderConfig) -> Result<Arc<dyn LlmProvider>, LlmError> {
        match config.provider_type {
            ProviderType::Mock => Ok(Arc::new(MockLlmProvider::new(&config.model, &config.model))),
            ProviderType::OpenAiCompatible => {
                let client_config = OpenAiClientConfig {
                    base_url: config.base_url.clone(),
                    api_key: config.api_key.clone(),
                    model: config.model.clone(),
                    timeout_secs: config.timeout_ms / 1000,
                    pricing: config.pricing.clone().into(),
                };
                Ok(Arc::new(OpenAiCompatibleClient::new(
                    &config.model,
                    client_config,
                )))
            }
            ProviderType::AzureOpenAi => {
                // Azure OpenAI 配置需要特定的字段
                // 从 base_url 解析 endpoint，从 extra 或 model 解析 deployment_id
                let endpoint = config.base_url.clone();
                let deployment_id = config.model.clone();
                let api_version = "2024-02-01".to_string(); // 默认 API 版本

                let client_config = AzureOpenAiConfig {
                    endpoint,
                    deployment_id,
                    api_key: config.api_key.clone(),
                    api_version,
                    timeout_secs: config.timeout_ms / 1000,
                    pricing: config.pricing.clone().into(),
                };

                Ok(Arc::new(AzureOpenAiClient::new(
                    &config.model,
                    client_config,
                )))
            }
            ProviderType::Claude => {
                let client_config = ClaudeConfig {
                    api_key: config.api_key.clone(),
                    model: config.model.clone(),
                    api_version: "2023-06-01".to_string(),
                    timeout_secs: config.timeout_ms / 1000,
                    pricing: config.pricing.clone().into(),
                };
                Ok(Arc::new(ClaudeClient::new(&config.model, client_config)))
            }
        }
    }
}

/// 预算状态
#[derive(Debug, Clone)]
pub struct BudgetStatus {
    /// 总成本
    pub total_cost: f64,
    /// 最大预算
    pub max_budget: f64,
    /// 剩余预算
    pub remaining: f64,
    /// 使用比例
    pub usage_ratio: f64,
}

impl BudgetStatus {
    /// 是否超出预算
    pub fn is_over_budget(&self) -> bool {
        self.total_cost >= self.max_budget
    }

    /// 是否需要警告 (使用超过 80%)
    pub fn should_warn(&self) -> bool {
        self.usage_ratio >= 0.8
    }
}

/// 健康检查结果
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    /// 各提供商状态
    pub providers: HashMap<String, ProviderStatus>,
}

impl HealthCheckResult {
    /// 是否全部健康
    pub fn all_healthy(&self) -> bool {
        self.providers
            .values()
            .all(|s| matches!(s, ProviderStatus::Healthy))
    }

    /// 获取不健康的提供商
    pub fn unhealthy_providers(&self) -> Vec<&String> {
        self.providers
            .iter()
            .filter(|(_, s)| matches!(s, ProviderStatus::Unhealthy(_)))
            .map(|(n, _)| n)
            .collect()
    }
}

/// 提供商状态
#[derive(Debug, Clone)]
pub enum ProviderStatus {
    /// 健康
    Healthy,
    /// 不健康
    Unhealthy(String),
}

/// 便捷函数：创建默认的 Mock 服务
pub async fn create_mock_service() -> LlmService {
    use crate::services::llm::types::{CostControlConfig, LlmServiceConfig, RoutingConfig};

    let config = LlmServiceConfig {
        default_provider: "mock".to_string(),
        providers: HashMap::new(),
        routing: RoutingConfig {
            operation_review: "mock".to_string(),
            content_review: "mock".to_string(),
            code_generation: "mock".to_string(),
        },
        cost_control: CostControlConfig {
            max_monthly_cost_usd: 1000.0, // 测试用充足预算
            fallback_chain: vec!["mock".to_string()],
        },
    };

    let mut service = LlmService::new(config);
    service.initialize().await.unwrap();
    service
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::llm::types::{CostControlConfig, RoutingConfig};

    fn create_test_config() -> LlmServiceConfig {
        LlmServiceConfig {
            default_provider: "mock".to_string(),
            providers: HashMap::new(),
            routing: RoutingConfig {
                operation_review: "mock".to_string(),
                content_review: "mock".to_string(),
                code_generation: "mock".to_string(),
            },
            cost_control: CostControlConfig {
                max_monthly_cost_usd: 100.0,
                fallback_chain: vec!["mock".to_string()],
            },
        }
    }

    #[tokio::test]
    async fn test_service_initialization() {
        let config = create_test_config();
        let mut service = LlmService::new(config);

        // 初始化应该创建默认的 mock provider
        service.initialize().await.unwrap();

        let provider = service.default_provider().await;
        assert!(provider.is_some());
    }

    #[tokio::test]
    async fn test_register_provider() {
        let config = create_test_config();
        let service = LlmService::new(config);

        let mock = Arc::new(MockLlmProvider::new("test", "test-model"));
        service.register_provider("test", mock).await;

        let provider = service.get_provider("test").await;
        assert!(provider.is_some());
    }

    #[tokio::test]
    async fn test_cost_tracking() {
        let config = create_test_config();
        let mut service = LlmService::new(config);
        service.initialize().await.unwrap();

        // 执行一次调用
        let request = ChatRequest::new("Test", "Hello");
        let _ = service.chat_completion(request).await;

        // 检查成本统计
        let stats = service.cost_stats();
        assert_eq!(stats.request_count, 1);
    }

    #[test]
    fn test_budget_status() {
        let status = BudgetStatus {
            total_cost: 80.0,
            max_budget: 100.0,
            remaining: 20.0,
            usage_ratio: 0.8,
        };

        assert!(!status.is_over_budget());
        assert!(status.should_warn());
    }

    #[tokio::test]
    async fn test_health_check() {
        let config = create_test_config();
        let mut service = LlmService::new(config);
        service.initialize().await.unwrap();

        let health = service.health_check().await;
        assert!(health.all_healthy());
    }
}
