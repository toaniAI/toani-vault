//! 审核引擎类型定义

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// 审核结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewResult {
    /// 是否批准
    pub approved: bool,
    /// 风险等级
    pub risk_level: RiskLevel,
    /// 审核原因
    pub reason: String,
    /// 建议操作
    pub suggested_action: SuggestedAction,
    /// 是否需要二次确认
    #[serde(default)]
    pub requires_confirmation: bool,
    /// 是否需要 2FA
    #[serde(default)]
    pub requires_2fa: bool,
    /// 元数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl ReviewResult {
    /// 创建批准结果
    pub fn approved(reason: impl Into<String>) -> Self {
        Self {
            approved: true,
            risk_level: RiskLevel::Low,
            reason: reason.into(),
            suggested_action: SuggestedAction::Proceed,
            requires_confirmation: false,
            requires_2fa: false,
            metadata: None,
        }
    }

    /// 创建拒绝结果
    pub fn rejected(reason: impl Into<String>) -> Self {
        Self {
            approved: false,
            risk_level: RiskLevel::High,
            reason: reason.into(),
            suggested_action: SuggestedAction::Reject,
            requires_confirmation: false,
            requires_2fa: false,
            metadata: None,
        }
    }

    /// 创建需要确认的结果
    pub fn requires_confirmation(reason: impl Into<String>) -> Self {
        Self {
            approved: true,
            risk_level: RiskLevel::Medium,
            reason: reason.into(),
            suggested_action: SuggestedAction::RequireConfirmation,
            requires_confirmation: true,
            requires_2fa: false,
            metadata: None,
        }
    }

    /// 检查是否已批准
    pub fn is_approved(&self) -> bool {
        self.approved
    }

    /// 检查是否需要二次确认
    pub fn needs_confirmation(&self) -> bool {
        self.requires_confirmation
    }
}

/// 风险等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    /// 低风险
    Low,
    /// 中等风险
    Medium,
    /// 高风险
    High,
    /// 严重风险
    Critical,
}

impl RiskLevel {
    /// 转换为数值（用于比较）
    pub fn as_u8(&self) -> u8 {
        match self {
            RiskLevel::Low => 1,
            RiskLevel::Medium => 2,
            RiskLevel::High => 3,
            RiskLevel::Critical => 4,
        }
    }

    /// 检查是否高于阈值
    pub fn is_above(&self, threshold: RiskLevel) -> bool {
        self.as_u8() > threshold.as_u8()
    }
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::Low => write!(f, "low"),
            RiskLevel::Medium => write!(f, "medium"),
            RiskLevel::High => write!(f, "high"),
            RiskLevel::Critical => write!(f, "critical"),
        }
    }
}

/// 建议操作
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestedAction {
    /// 继续执行
    Proceed,
    /// 需要确认
    RequireConfirmation,
    /// 需要二次认证
    RequireAdditionalAuth,
    /// 拒绝执行
    Reject,
    /// 记录审计日志
    LogAndProceed,
}

/// 审核上下文
#[derive(Debug, Clone)]
pub struct ReviewContext {
    /// 会话 ID
    pub session_id: Uuid,
    /// 租户 ID
    pub tenant_id: Uuid,
    /// 用户 ID
    pub user_id: Uuid,
    /// 凭证 ID
    pub credential_id: Uuid,
    /// 原始意图
    pub original_intent: String,
    /// 操作类型
    pub operation_type: String,
    /// 操作描述
    pub operation_description: String,
    /// 时间戳
    pub timestamp: OffsetDateTime,
}

impl ReviewContext {
    /// 创建新的审核上下文
    pub fn new(
        session_id: Uuid,
        tenant_id: Uuid,
        user_id: Uuid,
        credential_id: Uuid,
        original_intent: impl Into<String>,
    ) -> Self {
        Self {
            session_id,
            tenant_id,
            user_id,
            credential_id,
            original_intent: original_intent.into(),
            operation_type: String::new(),
            operation_description: String::new(),
            timestamp: OffsetDateTime::now_utc(),
        }
    }

    /// 设置操作类型
    pub fn with_operation_type(mut self, operation_type: impl Into<String>) -> Self {
        self.operation_type = operation_type.into();
        self
    }

    /// 设置操作描述
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.operation_description = description.into();
        self
    }
}

/// 审核记录
#[derive(Debug, Clone)]
pub struct ReviewRecord {
    /// 记录 ID
    pub record_id: Uuid,
    /// 审核请求
    pub context: ReviewContext,
    /// 审核结果
    pub result: ReviewResult,
    /// 使用的提供商
    pub provider: String,
    /// 使用的模型
    pub model: String,
    /// 审核耗时 (毫秒)
    pub review_time_ms: u64,
    /// 是否命中缓存
    pub cached: bool,
    /// 创建时间
    pub created_at: OffsetDateTime,
}

/// 注入检测结果
#[derive(Debug, Clone)]
pub struct DetectionResult {
    /// 是否检测到注入
    pub detected: bool,
    /// 检测到的攻击类型
    pub attack_types: Vec<AttackType>,
    /// 置信度 (0.0 - 1.0)
    pub confidence: f32,
    /// 匹配的模式
    pub matched_patterns: Vec<String>,
    /// 清理后的输入
    pub sanitized_input: Option<String>,
}

impl DetectionResult {
    /// 创建干净的结果
    pub fn clean() -> Self {
        Self {
            detected: false,
            attack_types: Vec::new(),
            confidence: 0.0,
            matched_patterns: Vec::new(),
            sanitized_input: None,
        }
    }

    /// 创建检测到注入的结果
    pub fn detected(attack_types: Vec<AttackType>, confidence: f32) -> Self {
        Self {
            detected: true,
            attack_types,
            confidence,
            matched_patterns: Vec::new(),
            sanitized_input: None,
        }
    }

    /// 检查是否被拒绝
    pub fn is_rejected(&self) -> bool {
        self.detected && self.confidence > 0.7
    }

    /// 检查是否需要警告
    pub fn needs_warning(&self) -> bool {
        self.detected && self.confidence > 0.4
    }
}

/// 攻击类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackType {
    /// 指令覆盖
    InstructionOverride,
    /// 角色冒充
    RoleImpersonation,
    /// 上下文操纵
    ContextManipulation,
    /// 编码混淆
    EncodingObfuscation,
    /// 零宽字符
    ZeroWidthChars,
    /// 提示词泄露
    PromptLeakage,
    /// 越狱攻击
    JailbreakAttempt,
}

impl std::fmt::Display for AttackType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttackType::InstructionOverride => write!(f, "instruction_override"),
            AttackType::RoleImpersonation => write!(f, "role_impersonation"),
            AttackType::ContextManipulation => write!(f, "context_manipulation"),
            AttackType::EncodingObfuscation => write!(f, "encoding_obfuscation"),
            AttackType::ZeroWidthChars => write!(f, "zero_width_chars"),
            AttackType::PromptLeakage => write!(f, "prompt_leakage"),
            AttackType::JailbreakAttempt => write!(f, "jailbreak_attempt"),
        }
    }
}

/// 审核配置
#[derive(Debug, Clone)]
pub struct ReviewConfig {
    /// 是否启用严格模式
    pub strict_mode: bool,
    /// 最大描述长度
    pub max_description_length: usize,
    /// 缓存 TTL (分钟)
    pub cache_ttl_minutes: u64,
    /// 高风险阈值
    pub high_risk_threshold: RiskLevel,
    /// 是否启用注入检测
    pub enable_injection_detection: bool,
    /// 是否启用 LLM 审核
    pub enable_llm_review: bool,
}

impl Default for ReviewConfig {
    fn default() -> Self {
        Self {
            strict_mode: true,
            max_description_length: 2000,
            cache_ttl_minutes: 30,
            high_risk_threshold: RiskLevel::High,
            enable_injection_detection: true,
            enable_llm_review: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_result_approved() {
        let result = ReviewResult::approved("Safe operation");
        assert!(result.is_approved());
        assert!(!result.needs_confirmation());
        assert_eq!(result.risk_level, RiskLevel::Low);
    }

    #[test]
    fn test_review_result_rejected() {
        let result = ReviewResult::rejected("Injection detected");
        assert!(!result.is_approved());
        assert_eq!(result.risk_level, RiskLevel::High);
    }

    #[test]
    fn test_risk_level_comparison() {
        assert!(RiskLevel::High.as_u8() > RiskLevel::Medium.as_u8());
        assert!(RiskLevel::Critical.is_above(RiskLevel::High));
        assert!(!RiskLevel::Low.is_above(RiskLevel::Medium));
    }

    #[test]
    fn test_detection_result() {
        let clean = DetectionResult::clean();
        assert!(!clean.is_rejected());
        assert!(!clean.needs_warning());

        let detected = DetectionResult::detected(
            vec![AttackType::InstructionOverride],
            0.9,
        );
        assert!(detected.is_rejected());
    }

    #[test]
    fn test_attack_type_display() {
        assert_eq!(
            AttackType::InstructionOverride.to_string(),
            "instruction_override"
        );
        assert_eq!(
            AttackType::ZeroWidthChars.to_string(),
            "zero_width_chars"
        );
    }
}
