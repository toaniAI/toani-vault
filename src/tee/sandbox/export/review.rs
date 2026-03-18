//! 截图内容审核模块
//!
//! 使用 Vision API (GPT-4o Vision) 审核截图内容，检测敏感信息。

use crate::services::llm::{LlmService, types::ChatRequestWithImage};
use crate::tee::sandbox::error::ExportError;
use std::sync::Arc;
use tracing::{debug, error, info};

/// 内容审核器
///
/// 使用 LLM Vision API 审核截图内容，检测敏感信息
pub struct ContentReviewer {
    llm_service: Arc<LlmService>,
    system_prompt: String,
}

/// 审核结果
#[derive(Debug, Clone)]
pub struct ReviewResult {
    /// 是否批准
    pub approved: bool,
    /// 置信度 (0.0 - 1.0)
    pub confidence: f32,
    /// 检测到的敏感项目
    pub detected_items: Vec<SensitiveItem>,
    /// 建议的脱敏区域
    pub redaction_regions: Vec<RedactionRegion>,
    /// 审核理由
    pub reason: String,
    /// 风险等级
    pub risk_level: RiskLevel,
}

/// 敏感项目
#[derive(Debug, Clone)]
pub struct SensitiveItem {
    /// 敏感类型
    pub item_type: SensitiveType,
    /// 描述
    pub description: String,
    /// 置信度
    pub confidence: f32,
    /// 建议操作
    pub recommended_action: RedactionAction,
}

/// 敏感类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensitiveType {
    /// 个人身份信息 (PII)
    Pii,
    /// 凭证/密码
    Credential,
    /// API 密钥
    ApiKey,
    /// 信用卡号
    CreditCard,
    /// 社会安全号
    Ssn,
    /// 电子邮件地址
    Email,
    /// 电话号码
    PhoneNumber,
    /// 地址
    Address,
    /// 账户余额/财务信息
    FinancialInfo,
    /// 内部系统信息
    InternalSystem,
    /// 其他敏感信息
    Other,
}

impl std::fmt::Display for SensitiveType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SensitiveType::Pii => write!(f, "pii"),
            SensitiveType::Credential => write!(f, "credential"),
            SensitiveType::ApiKey => write!(f, "api_key"),
            SensitiveType::CreditCard => write!(f, "credit_card"),
            SensitiveType::Ssn => write!(f, "ssn"),
            SensitiveType::Email => write!(f, "email"),
            SensitiveType::PhoneNumber => write!(f, "phone_number"),
            SensitiveType::Address => write!(f, "address"),
            SensitiveType::FinancialInfo => write!(f, "financial_info"),
            SensitiveType::InternalSystem => write!(f, "internal_system"),
            SensitiveType::Other => write!(f, "other"),
        }
    }
}

/// 脱敏区域
#[derive(Debug, Clone, Copy)]
pub struct RedactionRegion {
    /// 区域类型
    pub region_type: RegionType,
    /// 左上角 X 坐标（相对于图片宽度，0.0-1.0）
    pub x: f64,
    /// 左上角 Y 坐标（相对于图片高度，0.0-1.0）
    pub y: f64,
    /// 宽度（相对于图片宽度，0.0-1.0）
    pub width: f64,
    /// 高度（相对于图片高度，0.0-1.0）
    pub height: f64,
    /// 敏感类型
    pub sensitive_type: SensitiveType,
    /// 建议的脱敏动作
    pub action: RedactionAction,
    /// 置信度
    pub confidence: f32,
}

/// 区域类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionType {
    /// 矩形区域
    Rectangle,
    /// 文本块
    TextBlock,
    /// 图像/照片
    Image,
}

/// 脱敏动作
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RedactionAction {
    /// 模糊处理
    Blur,
    /// 黑色遮罩
    Blackout,
    /// 像素化
    Pixelate,
    /// 替换为占位符
    Replace,
    /// 裁剪移除
    Crop,
}

impl std::fmt::Display for RedactionAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RedactionAction::Blur => write!(f, "blur"),
            RedactionAction::Blackout => write!(f, "blackout"),
            RedactionAction::Pixelate => write!(f, "pixelate"),
            RedactionAction::Replace => write!(f, "replace"),
            RedactionAction::Crop => write!(f, "crop"),
        }
    }
}

/// 风险等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    /// 无风险
    None,
    /// 低风险
    Low,
    /// 中风险
    Medium,
    /// 高风险
    High,
    /// 严重风险
    Critical,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::None => write!(f, "none"),
            RiskLevel::Low => write!(f, "low"),
            RiskLevel::Medium => write!(f, "medium"),
            RiskLevel::High => write!(f, "high"),
            RiskLevel::Critical => write!(f, "critical"),
        }
    }
}

/// 审核配置
#[derive(Debug, Clone)]
pub struct ReviewConfig {
    /// 最低置信度阈值
    pub min_confidence: f32,
    /// 自动批准的风险等级
    pub auto_approve_level: RiskLevel,
    /// 要求人工审核的风险等级
    pub manual_review_level: RiskLevel,
    /// 启用的检测类型
    pub enabled_types: Vec<SensitiveType>,
    /// 自定义提示词
    pub custom_prompt: Option<String>,
}

impl Default for ReviewConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.7,
            auto_approve_level: RiskLevel::Low,
            manual_review_level: RiskLevel::High,
            enabled_types: vec![
                SensitiveType::Pii,
                SensitiveType::Credential,
                SensitiveType::ApiKey,
                SensitiveType::CreditCard,
                SensitiveType::Ssn,
                SensitiveType::Email,
                SensitiveType::PhoneNumber,
                SensitiveType::Address,
                SensitiveType::FinancialInfo,
            ],
            custom_prompt: None,
        }
    }
}

impl ContentReviewer {
    /// 创建新的内容审核器
    pub fn new(llm_service: Arc<LlmService>) -> Self {
        let system_prompt = Self::default_system_prompt();
        Self {
            llm_service,
            system_prompt,
        }
    }

    /// 创建带自定义配置的审核器
    pub fn with_config(llm_service: Arc<LlmService>, config: &ReviewConfig) -> Self {
        let system_prompt = config
            .custom_prompt
            .clone()
            .unwrap_or_else(Self::default_system_prompt);

        Self {
            llm_service,
            system_prompt,
        }
    }

    /// 审核截图内容
    ///
    /// # Arguments
    ///
    /// * `image_data` - 图片数据
    /// * `mime_type` - MIME 类型
    ///
    /// # Returns
    ///
    /// 返回审核结果，包含是否批准、检测到的敏感项目等信息
    pub async fn review(
        &self,
        image_data: &[u8],
        mime_type: &str,
    ) -> Result<ReviewResult, ExportError> {
        info!("开始审核截图内容, 大小: {} bytes", image_data.len());

        // 创建 Vision API 请求
        let request = ChatRequestWithImage::new(
            &self.system_prompt,
            "请分析这张截图，检测其中的敏感信息。",
            image_data,
            mime_type,
        )
        .with_json_response();

        // 调用 LLM 服务
        let response = self
            .llm_service
            .chat_completion_with_image(request)
            .await
            .map_err(|e| {
                error!("Vision API 调用失败: {}", e);
                ExportError::ContentReview(e.to_string())
            })?;

        debug!("Vision API 响应: {}", response.content);

        // 解析响应
        let result = self.parse_review_response(&response.content)?;

        info!(
            "审核完成: approved={}, risk_level={}, items={}",
            result.approved,
            result.risk_level,
            result.detected_items.len()
        );

        Ok(result)
    }

    /// 批量审核多张截图
    pub async fn review_batch(
        &self,
        images: &[(Vec<u8>, String)],
    ) -> Vec<Result<ReviewResult, ExportError>> {
        let mut results = Vec::with_capacity(images.len());

        for (data, mime_type) in images {
            let result = self.review(data, mime_type).await;
            results.push(result);
        }

        results
    }

    /// 默认系统提示词
    fn default_system_prompt() -> String {
        r#"你是一个专业的截图内容安全审核助手。你的任务是分析截图并检测其中的敏感信息。

请检测以下类型的敏感信息：
1. 个人身份信息 (PII): 姓名、身份证号、生日等
2. 凭证信息: 密码、API密钥、访问令牌
3. 财务信息: 信用卡号、银行账户、余额
4. 联系方式: 邮箱、电话号码、地址
5. 内部系统信息: 服务器地址、数据库连接字符串

请按以下 JSON 格式返回结果：
{
    "approved": false,  // 是否批准导出
    "confidence": 0.95,  // 整体置信度 0.0-1.0
    "risk_level": "high",  // none/low/medium/high/critical
    "reason": "检测到API密钥",  // 审核理由
    "detected_items": [
        {
            "type": "api_key",
            "description": "OpenAI API密钥",
            "confidence": 0.98,
            "recommended_action": "blackout"
        }
    ],
    "redaction_regions": [
        {
            "type": "rectangle",
            "x": 0.1,
            "y": 0.2,
            "width": 0.3,
            "height": 0.1,
            "sensitive_type": "api_key",
            "action": "blackout",
            "confidence": 0.98
        }
    ]
}

脱敏动作说明：
- blur: 模糊处理，适合人脸、背景
- blackout: 黑色遮罩，适合敏感文本
- pixelate: 像素化，适合小区域
- replace: 替换为占位符如 [REDACTED]
- crop: 裁剪移除，适合边缘区域

注意：坐标和尺寸都是相对于图片宽高的比例值 (0.0-1.0)。"#
            .to_string()
    }

    /// 解析审核响应
    fn parse_review_response(&self, content: &str) -> Result<ReviewResult, ExportError> {
        // 尝试解析 JSON 响应
        let json_str = if content.starts_with("```json") {
            content
                .trim_start_matches("```json")
                .trim_end_matches("```")
                .trim()
        } else if content.starts_with("```") {
            content
                .trim_start_matches("```")
                .trim_end_matches("```")
                .trim()
        } else {
            content
        };

        #[derive(Debug, serde::Deserialize)]
        struct ReviewResponse {
            approved: bool,
            confidence: f32,
            risk_level: String,
            reason: String,
            #[serde(default)]
            detected_items: Vec<DetectedItemJson>,
            #[serde(default)]
            redaction_regions: Vec<RegionJson>,
        }

        #[derive(Debug, serde::Deserialize)]
        struct DetectedItemJson {
            #[serde(rename = "type")]
            item_type: String,
            description: String,
            confidence: f32,
            #[serde(rename = "recommended_action")]
            action: String,
        }

        #[derive(Debug, serde::Deserialize)]
        struct RegionJson {
            #[serde(rename = "type")]
            region_type: String,
            x: f64,
            y: f64,
            width: f64,
            height: f64,
            sensitive_type: String,
            action: String,
            confidence: f32,
        }

        let response: ReviewResponse = serde_json::from_str(json_str)
            .map_err(|e| ExportError::ContentReview(format!("解析审核响应失败: {}", e)))?;

        // 转换检测到的项目
        let detected_items: Vec<SensitiveItem> = response
            .detected_items
            .into_iter()
            .map(|item| SensitiveItem {
                item_type: Self::parse_sensitive_type(&item.item_type),
                description: item.description,
                confidence: item.confidence,
                recommended_action: Self::parse_action(&item.action),
            })
            .collect();

        // 转换脱敏区域
        let redaction_regions: Vec<RedactionRegion> = response
            .redaction_regions
            .into_iter()
            .map(|region| RedactionRegion {
                region_type: Self::parse_region_type(&region.region_type),
                x: region.x.clamp(0.0, 1.0),
                y: region.y.clamp(0.0, 1.0),
                width: region.width.clamp(0.0, 1.0),
                height: region.height.clamp(0.0, 1.0),
                sensitive_type: Self::parse_sensitive_type(&region.sensitive_type),
                action: Self::parse_action(&region.action),
                confidence: region.confidence,
            })
            .collect();

        Ok(ReviewResult {
            approved: response.approved,
            confidence: response.confidence,
            detected_items,
            redaction_regions,
            reason: response.reason,
            risk_level: Self::parse_risk_level(&response.risk_level),
        })
    }

    fn parse_sensitive_type(s: &str) -> SensitiveType {
        match s.to_lowercase().as_str() {
            "pii" | "personal_info" => SensitiveType::Pii,
            "credential" | "password" => SensitiveType::Credential,
            "api_key" | "apikey" | "api-key" => SensitiveType::ApiKey,
            "credit_card" | "creditcard" | "credit-card" => SensitiveType::CreditCard,
            "ssn" | "social_security" => SensitiveType::Ssn,
            "email" => SensitiveType::Email,
            "phone" | "phone_number" | "phonenumber" => SensitiveType::PhoneNumber,
            "address" => SensitiveType::Address,
            "financial" | "financial_info" => SensitiveType::FinancialInfo,
            "internal" | "internal_system" => SensitiveType::InternalSystem,
            _ => SensitiveType::Other,
        }
    }

    fn parse_action(s: &str) -> RedactionAction {
        match s.to_lowercase().as_str() {
            "blur" => RedactionAction::Blur,
            "blackout" => RedactionAction::Blackout,
            "pixelate" => RedactionAction::Pixelate,
            "replace" => RedactionAction::Replace,
            "crop" => RedactionAction::Crop,
            _ => RedactionAction::Blackout,
        }
    }

    fn parse_region_type(s: &str) -> RegionType {
        match s.to_lowercase().as_str() {
            "text" | "textblock" => RegionType::TextBlock,
            "image" => RegionType::Image,
            _ => RegionType::Rectangle,
        }
    }

    fn parse_risk_level(s: &str) -> RiskLevel {
        match s.to_lowercase().as_str() {
            "none" => RiskLevel::None,
            "low" => RiskLevel::Low,
            "medium" => RiskLevel::Medium,
            "high" => RiskLevel::High,
            "critical" => RiskLevel::Critical,
            _ => RiskLevel::Medium,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::None < RiskLevel::Low);
        assert!(RiskLevel::Low < RiskLevel::Medium);
        assert!(RiskLevel::Medium < RiskLevel::High);
        assert!(RiskLevel::High < RiskLevel::Critical);
    }

    #[test]
    fn test_parse_sensitive_type() {
        assert_eq!(
            ContentReviewer::parse_sensitive_type("api_key"),
            SensitiveType::ApiKey
        );
        assert_eq!(
            ContentReviewer::parse_sensitive_type("PII"),
            SensitiveType::Pii
        );
        assert_eq!(
            ContentReviewer::parse_sensitive_type("unknown"),
            SensitiveType::Other
        );
    }

    #[test]
    fn test_parse_action() {
        assert_eq!(ContentReviewer::parse_action("blur"), RedactionAction::Blur);
        assert_eq!(
            ContentReviewer::parse_action("BLACKOUT"),
            RedactionAction::Blackout
        );
        assert_eq!(
            ContentReviewer::parse_action("unknown"),
            RedactionAction::Blackout
        );
    }

    #[test]
    fn test_parse_risk_level() {
        assert_eq!(ContentReviewer::parse_risk_level("high"), RiskLevel::High);
        assert_eq!(
            ContentReviewer::parse_risk_level("CRITICAL"),
            RiskLevel::Critical
        );
        assert_eq!(
            ContentReviewer::parse_risk_level("unknown"),
            RiskLevel::Medium
        );
    }

    #[test]
    fn test_review_config_default() {
        let config = ReviewConfig::default();
        assert_eq!(config.min_confidence, 0.7);
        assert_eq!(config.auto_approve_level, RiskLevel::Low);
        assert_eq!(config.manual_review_level, RiskLevel::High);
        assert!(!config.enabled_types.is_empty());
    }
}
