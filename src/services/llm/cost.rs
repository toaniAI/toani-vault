//! LLM 成本跟踪

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, info, warn};

/// 成本跟踪器
#[derive(Debug, Default)]
pub struct CostTracker {
    /// 本月累计成本 (美元，以微美元为单位存储)
    monthly_cost_micro_usd: AtomicU64,
    /// 本月累计输入 tokens
    total_input_tokens: AtomicU64,
    /// 本月累计输出 tokens
    total_output_tokens: AtomicU64,
    /// 本月累计请求数
    total_requests: AtomicU64,
    /// 价格配置
    pricing: PricingInfo,
}

/// 价格信息
#[derive(Debug, Clone)]
pub struct PricingInfo {
    /// 输入价格 (每 1000 tokens，美元)
    pub input_price_per_1k: f64,
    /// 输出价格 (每 1000 tokens，美元)
    pub output_price_per_1k: f64,
}

impl Default for PricingInfo {
    fn default() -> Self {
        Self {
            input_price_per_1k: 0.0,
            output_price_per_1k: 0.0,
        }
    }
}

impl From<crate::services::llm::types::PricingConfig> for PricingInfo {
    fn from(config: crate::services::llm::types::PricingConfig) -> Self {
        Self {
            input_price_per_1k: config.input_price_per_1k,
            output_price_per_1k: config.output_price_per_1k,
        }
    }
}

/// 使用统计
#[derive(Debug, Clone)]
pub struct UsageStats {
    /// 累计成本 (美元)
    pub total_cost_usd: f64,
    /// 输入 tokens
    pub input_tokens: u64,
    /// 输出 tokens
    pub output_tokens: u64,
    /// 总 tokens
    pub total_tokens: u64,
    /// 请求数
    pub request_count: u64,
    /// 平均每次请求成本
    pub avg_cost_per_request: f64,
}

impl CostTracker {
    /// 创建新的成本跟踪器
    pub fn new(pricing: PricingInfo) -> Self {
        Self {
            monthly_cost_micro_usd: AtomicU64::new(0),
            total_input_tokens: AtomicU64::new(0),
            total_output_tokens: AtomicU64::new(0),
            total_requests: AtomicU64::new(0),
            pricing,
        }
    }

    /// 记录一次 API 调用
    pub fn record_usage(&self, input_tokens: u32, output_tokens: u32) -> f64 {
        // 计算成本
        let input_cost = (input_tokens as f64 / 1000.0) * self.pricing.input_price_per_1k;
        let output_cost = (output_tokens as f64 / 1000.0) * self.pricing.output_price_per_1k;
        let total_cost = input_cost + output_cost;

        // 转换为微美元并累加
        let cost_micro_usd = (total_cost * 1_000_000.0) as u64;
        self.monthly_cost_micro_usd
            .fetch_add(cost_micro_usd, Ordering::SeqCst);

        // 累加 tokens
        self.total_input_tokens
            .fetch_add(input_tokens as u64, Ordering::SeqCst);
        self.total_output_tokens
            .fetch_add(output_tokens as u64, Ordering::SeqCst);

        // 累加请求数
        self.total_requests.fetch_add(1, Ordering::SeqCst);

        debug!(
            "LLM usage recorded: input={}, output={}, cost=${:.6}",
            input_tokens, output_tokens, total_cost
        );

        total_cost
    }

    /// 获取累计成本 (美元)
    pub fn total_cost_usd(&self) -> f64 {
        let micro_usd = self.monthly_cost_micro_usd.load(Ordering::SeqCst);
        micro_usd as f64 / 1_000_000.0
    }

    /// 检查是否超出预算
    pub fn is_over_budget(&self, budget_usd: f64) -> bool {
        self.total_cost_usd() >= budget_usd
    }

    /// 获取使用统计
    pub fn stats(&self) -> UsageStats {
        let input = self.total_input_tokens.load(Ordering::SeqCst);
        let output = self.total_output_tokens.load(Ordering::SeqCst);
        let requests = self.total_requests.load(Ordering::SeqCst);
        let cost = self.total_cost_usd();

        UsageStats {
            total_cost_usd: cost,
            input_tokens: input,
            output_tokens: output,
            total_tokens: input + output,
            request_count: requests,
            avg_cost_per_request: if requests > 0 {
                cost / requests as f64
            } else {
                0.0
            },
        }
    }

    /// 重置统计 (通常在新月份调用)
    pub fn reset(&self) {
        self.monthly_cost_micro_usd.store(0, Ordering::SeqCst);
        self.total_input_tokens.store(0, Ordering::SeqCst);
        self.total_output_tokens.store(0, Ordering::SeqCst);
        self.total_requests.store(0, Ordering::SeqCst);

        info!("Cost tracker reset");
    }
}

/// 成本控制器
pub struct CostController {
    /// 成本跟踪器
    tracker: Arc<CostTracker>,
    /// 最大月预算 (美元)
    max_monthly_budget: f64,
    /// 是否启用成本控制
    enabled: bool,
}

impl CostController {
    /// 创建新的成本控制器
    pub fn new(tracker: Arc<CostTracker>, max_monthly_budget: f64) -> Self {
        Self {
            tracker,
            max_monthly_budget,
            enabled: true,
        }
    }

    /// 检查是否允许继续调用
    pub fn can_proceed(&self) -> bool {
        if !self.enabled {
            return true;
        }

        let can = !self.tracker.is_over_budget(self.max_monthly_budget);
        if !can {
            warn!(
                "Monthly LLM budget exceeded: ${:.2} / ${:.2}",
                self.tracker.total_cost_usd(),
                self.max_monthly_budget
            );
        }
        can
    }

    /// 获取剩余预算
    pub fn remaining_budget(&self) -> f64 {
        (self.max_monthly_budget - self.tracker.total_cost_usd()).max(0.0)
    }

    /// 获取预算使用比例
    pub fn budget_usage_ratio(&self) -> f64 {
        if self.max_monthly_budget <= 0.0 {
            return 0.0;
        }
        (self.tracker.total_cost_usd() / self.max_monthly_budget).min(1.0)
    }

    /// 禁用成本控制 (用于测试)
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// 启用成本控制
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// 获取成本跟踪器引用
    pub fn tracker(&self) -> &CostTracker {
        &self.tracker
    }
}

/// 常用模型的默认价格配置
pub mod pricing {
    use super::PricingInfo;

    /// OpenAI GPT-4o
    pub fn gpt_4o() -> PricingInfo {
        PricingInfo {
            input_price_per_1k: 0.005,
            output_price_per_1k: 0.015,
        }
    }

    /// OpenAI GPT-4o-mini
    pub fn gpt_4o_mini() -> PricingInfo {
        PricingInfo {
            input_price_per_1k: 0.00015,
            output_price_per_1k: 0.0006,
        }
    }

    /// OpenAI GPT-4
    pub fn gpt_4() -> PricingInfo {
        PricingInfo {
            input_price_per_1k: 0.03,
            output_price_per_1k: 0.06,
        }
    }

    /// Claude 3.5 Sonnet
    pub fn claude_3_5_sonnet() -> PricingInfo {
        PricingInfo {
            input_price_per_1k: 0.003,
            output_price_per_1k: 0.015,
        }
    }

    /// 免费/模拟
    pub fn free() -> PricingInfo {
        PricingInfo {
            input_price_per_1k: 0.0,
            output_price_per_1k: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_tracker() {
        let tracker = CostTracker::new(pricing::gpt_4o());

        // 记录一次调用
        let cost = tracker.record_usage(1000, 500);
        assert!(cost > 0.0);

        // 检查统计
        let stats = tracker.stats();
        assert_eq!(stats.input_tokens, 1000);
        assert_eq!(stats.output_tokens, 500);
        assert_eq!(stats.request_count, 1);
        assert!((stats.total_cost_usd - cost).abs() < 0.0001);
    }

    #[test]
    fn test_cost_controller() {
        let tracker = Arc::new(CostTracker::new(pricing::gpt_4o()));
        let controller = CostController::new(tracker.clone(), 0.01); // $0.01 budget

        // 记录大量调用以超过预算
        for _ in 0..10 {
            tracker.record_usage(1000, 500);
        }

        assert!(!controller.can_proceed());
        assert_eq!(controller.remaining_budget(), 0.0);
        assert!(controller.budget_usage_ratio() >= 1.0);
    }

    #[test]
    fn test_pricing_configs() {
        let gpt4 = pricing::gpt_4();
        assert!(gpt4.input_price_per_1k > 0.0);

        let free = pricing::free();
        assert_eq!(free.input_price_per_1k, 0.0);
    }
}
