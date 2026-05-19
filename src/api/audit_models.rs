//! 审计日志 API 模型
//!
//! 定义审计日志查询和导出相关的请求/响应结构
//!
//! # 模型结构
//!
//! ```text
//! AuditLogQueryRequest    - 审计日志查询请求参数
//! AuditLogListResponse    - 审计日志列表响应
//! AuditLogDetailResponse  - 审计日志详情响应
//! AuditExportRequest      - 审计日志导出请求
//! AuditVerifyRequest      - 审计日志验证请求
//! AuditVerifyResponse     - 审计日志验证响应
//! ```

use serde::{Deserialize, Serialize};

use crate::audit::events::{AuditAction, Outcome, RiskTier};
use crate::audit::recorder::SignedAuditEntry;

/// 支持多种时间格式的自定义类型
///
/// 可接受：
/// - Unix 时间戳毫秒（数字，如 `1735689600000`）
/// - ISO 8601 字符串（如 `2026-06-01T12:00:00.000Z`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FlexibleTime(u64);

impl FlexibleTime {
    /// 获取毫秒时间戳
    pub fn as_millis(&self) -> u64 {
        self.0
    }
}

impl From<u64> for FlexibleTime {
    fn from(ms: u64) -> Self {
        Self(ms)
    }
}

impl From<FlexibleTime> for u64 {
    fn from(time: FlexibleTime) -> u64 {
        time.0
    }
}

impl Serialize for FlexibleTime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u64(self.0)
    }
}

impl<'de> Deserialize<'de> for FlexibleTime {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        // 先尝试解析为通用值
        let value = serde_json::Value::deserialize(deserializer)?;

        match value {
            // 数字格式：直接作为毫秒时间戳
            serde_json::Value::Number(n) => {
                if let Some(ms) = n.as_u64() {
                    Ok(Self(ms))
                } else if let Some(f) = n.as_f64() {
                    // 处理可能传入的浮点数（截断为整数）
                    Ok(Self(f as u64))
                } else {
                    Err(Error::custom("时间戳必须是正整数或有效的 ISO 8601 字符串"))
                }
            }
            // 字符串格式：解析 ISO 8601
            serde_json::Value::String(s) => {
                // 尝试解析 ISO 8601 格式
                parse_iso8601_to_millis(&s).map(Self).map_err(Error::custom)
            }
            // 其他格式不支持
            _ => Err(Error::custom("时间必须是数字时间戳或 ISO 8601 字符串")),
        }
    }
}

/// 解析 ISO 8601 时间字符串为毫秒时间戳
///
/// 支持格式：
/// - `2026-06-01T12:00:00.000Z`
/// - `2026-06-01T12:00:00Z`
/// - `2026-06-01T12:00:00+08:00`
fn parse_iso8601_to_millis(s: &str) -> Result<u64, String> {
    // 尝试使用 chrono 解析 RFC3339 格式
    use chrono::DateTime;

    // 尝试解析为 RFC3339/ISO 8601
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        // 转换为 UTC 时间戳（毫秒）
        let ts = dt.timestamp_millis();
        if ts >= 0 {
            return Ok(ts as u64);
        } else {
            return Err("ISO 8601 时间不能为负数".to_string());
        }
    }

    // 尝试解析纯数字字符串（可能是毫秒时间戳）
    if let Ok(ms) = s.parse::<u64>() {
        return Ok(ms);
    }

    Err(format!(
        "无法解析时间格式: '{s}', 请使用毫秒时间戳或 ISO 8601 格式 (如 2026-06-01T12:00:00.000Z)"
    ))
}

/// 审计日志查询请求参数
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AuditLogQueryRequest {
    /// 开始时间（支持 Unix 时间戳毫秒或 ISO 8601 字符串）
    pub start_time: Option<FlexibleTime>,
    /// 结束时间（支持 Unix 时间戳毫秒或 ISO 8601 字符串）
    pub end_time: Option<FlexibleTime>,
    /// 用户 ID 哈希
    pub user_id_hash: Option<String>,
    /// 操作类型
    pub action: Option<AuditAction>,
    /// 风险等级
    pub risk_tier: Option<RiskTier>,
    /// 操作结果
    pub outcome: Option<Outcome>,
    /// 服务标识
    pub service: Option<String>,
    /// 页码（从 1 开始）
    #[serde(default = "default_page")]
    pub page: usize,
    /// 每页数量
    #[serde(default = "default_page_size")]
    pub page_size: usize,
    /// 偏移量（与 `limit` 配对，兼容旧分页风格）
    #[serde(default)]
    pub offset: Option<usize>,
    /// 返回条数限制（兼容旧分页风格）
    #[serde(default)]
    pub limit: Option<usize>,
}

fn default_page() -> usize {
    1
}

fn default_page_size() -> usize {
    20
}

impl AuditLogQueryRequest {
    /// 创建新的查询请求
    pub fn new() -> Self {
        Self::default()
    }

    /// 验证请求参数
    pub fn validate(&self) -> Result<(), String> {
        // 验证时间范围
        if let (Some(start), Some(end)) = (self.start_time, self.end_time)
            && start.as_millis() > end.as_millis()
        {
            return Err("开始时间不能大于结束时间".to_string());
        }

        // 验证页码
        if self.page == 0 {
            return Err("页码必须从 1 开始".to_string());
        }

        // 验证分页大小
        let page_size = self.effective_page_size();
        if page_size == 0 || page_size > 1000 {
            return Err("分页大小必须在 1-1000 之间".to_string());
        }

        Ok(())
    }

    /// 获取偏移量
    pub fn offset(&self) -> usize {
        self.offset
            .unwrap_or_else(|| (self.page - 1) * self.effective_page_size())
    }

    /// 获取实际分页大小
    pub fn effective_page_size(&self) -> usize {
        self.limit.unwrap_or(self.page_size)
    }

    /// 获取当前页码
    pub fn current_page(&self) -> usize {
        match self.offset {
            Some(offset) => (offset / self.effective_page_size()) + 1,
            None => self.page,
        }
    }

    /// 设置开始时间
    pub fn with_start_time(mut self, time: u64) -> Self {
        self.start_time = Some(FlexibleTime::from(time));
        self
    }

    /// 设置结束时间
    pub fn with_end_time(mut self, time: u64) -> Self {
        self.end_time = Some(FlexibleTime::from(time));
        self
    }

    /// 设置用户 ID 哈希
    pub fn with_user_id_hash(mut self, hash: impl Into<String>) -> Self {
        self.user_id_hash = Some(hash.into());
        self
    }

    /// 设置操作类型
    pub fn with_action(mut self, action: AuditAction) -> Self {
        self.action = Some(action);
        self
    }

    /// 设置风险等级
    pub fn with_risk_tier(mut self, tier: RiskTier) -> Self {
        self.risk_tier = Some(tier);
        self
    }

    /// 设置操作结果
    pub fn with_outcome(mut self, outcome: Outcome) -> Self {
        self.outcome = Some(outcome);
        self
    }

    /// 设置服务
    pub fn with_service(mut self, service: impl Into<String>) -> Self {
        self.service = Some(service.into());
        self
    }

    /// 设置分页
    pub fn with_pagination(mut self, page: usize, page_size: usize) -> Self {
        self.page = page;
        self.page_size = page_size;
        self
    }

    /// 设置 offset/limit 分页
    pub fn with_offset_limit(mut self, offset: usize, limit: usize) -> Self {
        self.offset = Some(offset);
        self.limit = Some(limit);
        self
    }
}

impl Default for AuditLogQueryRequest {
    fn default() -> Self {
        Self {
            start_time: None,
            end_time: None,
            user_id_hash: None,
            action: None,
            risk_tier: None,
            outcome: None,
            service: None,
            page: default_page(),
            page_size: default_page_size(),
            offset: None,
            limit: None,
        }
    }
}

/// 审计日志列表项
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AuditLogListItem {
    /// 审计条目 ID
    pub id: String,
    /// 时间戳（Unix 时间戳毫秒）
    pub timestamp: u64,
    /// 用户 ID 哈希
    pub user_id_hash: String,
    /// 会话 ID
    pub session_id: String,
    /// 服务标识
    pub service: String,
    /// 操作类型
    pub action: AuditAction,
    /// 风险等级
    pub risk_tier: RiskTier,
    /// 操作结果
    pub outcome: Outcome,
    /// 日志索引
    pub log_index: u64,
}

impl From<&SignedAuditEntry> for AuditLogListItem {
    fn from(entry: &SignedAuditEntry) -> Self {
        Self {
            id: entry.entry.id.clone(),
            timestamp: entry.entry.timestamp,
            user_id_hash: entry.entry.user_id_hash.clone(),
            session_id: entry.entry.session_id.clone(),
            service: entry.entry.service.clone(),
            action: entry.entry.action,
            risk_tier: entry.entry.risk_tier,
            outcome: entry.entry.outcome,
            log_index: entry.log_index,
        }
    }
}

/// 审计日志列表响应
#[derive(Debug, Clone, Serialize)]
pub struct AuditLogListResponse {
    /// 是否成功
    pub success: bool,
    /// 数据
    pub data: AuditLogListData,
    /// 错误信息（失败时）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 审计日志列表数据
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AuditLogListData {
    /// 审计日志列表
    pub items: Vec<AuditLogListItem>,
    /// 总数
    pub total: u64,
    /// 页码
    pub page: usize,
    /// 每页数量
    pub page_size: usize,
    /// 分页偏移量
    pub offset: usize,
    /// 分页限制
    pub limit: usize,
    /// 总页数
    pub total_pages: usize,
    /// 是否存在下一页
    pub has_more: bool,
}

impl AuditLogListResponse {
    /// 创建成功响应
    pub fn success(
        items: Vec<AuditLogListItem>,
        total: u64,
        page: usize,
        page_size: usize,
        offset: usize,
    ) -> Self {
        let total_pages = if total == 0 {
            0
        } else {
            total.div_ceil(page_size as u64) as usize
        };
        let has_more = (offset as u64 + items.len() as u64) < total;
        Self {
            success: true,
            data: AuditLogListData {
                items,
                total,
                page,
                page_size,
                offset,
                limit: page_size,
                total_pages,
                has_more,
            },
            error: None,
        }
    }

    /// 创建错误响应
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: AuditLogListData {
                items: vec![],
                total: 0,
                page: 1,
                page_size: 20,
                offset: 0,
                limit: 20,
                total_pages: 0,
                has_more: false,
            },
            error: Some(message.into()),
        }
    }
}

/// 审计日志参数响应
#[derive(Debug, Clone, Serialize)]
pub struct AuditLogParamItem {
    /// 参数名
    pub key: String,
    /// 参数值
    pub value: String,
}

/// 审计日志详情响应
#[derive(Debug, Clone, Serialize)]
pub struct AuditLogDetailResponse {
    /// 是否成功
    pub success: bool,
    /// 数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<AuditLogDetailData>,
    /// 错误信息（失败时）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 审计日志详情数据
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AuditLogDetailData {
    /// 审计条目 ID
    pub id: String,
    /// 时间戳
    pub timestamp: u64,
    /// 用户 ID 哈希
    pub user_id_hash: String,
    /// 会话 ID
    pub session_id: String,
    /// 服务标识
    pub service: String,
    /// 操作类型
    pub action: AuditAction,
    /// 风险等级
    pub risk_tier: RiskTier,
    /// 操作结果
    pub outcome: Outcome,
    /// TEE MRENCLAVE
    pub tee_mrenclave: String,
    /// Action Token JTI
    pub action_token_jti: String,
    /// 操作参数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Vec<AuditLogParamItem>>,
    /// 错误信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// 日志索引
    pub log_index: u64,
    /// 内容哈希
    pub content_hash: String,
    /// 前一哈希
    pub previous_hash: String,
    /// 签名者指纹
    pub signer_fingerprint: String,
}

impl AuditLogDetailResponse {
    /// 创建成功响应
    pub fn success(data: AuditLogDetailData) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    /// 创建错误响应
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

impl From<SignedAuditEntry> for AuditLogDetailData {
    fn from(entry: SignedAuditEntry) -> Self {
        let params = entry.entry.params.map(|p| {
            p.into_iter()
                .map(|(k, v)| AuditLogParamItem {
                    key: k,
                    value: v.to_string(),
                })
                .collect()
        });

        Self {
            id: entry.entry.id.clone(),
            timestamp: entry.entry.timestamp,
            user_id_hash: entry.entry.user_id_hash.clone(),
            session_id: entry.entry.session_id.clone(),
            service: entry.entry.service.clone(),
            action: entry.entry.action,
            risk_tier: entry.entry.risk_tier,
            outcome: entry.entry.outcome,
            tee_mrenclave: entry.entry.tee_mrenclave.clone(),
            action_token_jti: entry.entry.action_token_jti.clone(),
            params,
            error_message: entry.entry.error_message.clone(),
            log_index: entry.log_index,
            content_hash: hex::encode(entry.content_hash),
            previous_hash: hex::encode(entry.prev_hash),
            signer_fingerprint: entry.signer_fingerprint.clone(),
        }
    }
}

/// 导出格式
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ExportFormat {
    /// JSON 格式
    #[default]
    Json,
    /// CSV 格式
    Csv,
}

/// 审计日志导出请求
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AuditExportRequest {
    /// 开始时间（Unix 时间戳毫秒）
    pub start_time: Option<u64>,
    /// 结束时间（Unix 时间戳毫秒）
    pub end_time: Option<u64>,
    /// 导出格式
    #[serde(default)]
    pub format: ExportFormat,
    /// 用户 ID 哈希过滤
    pub user_id_hash: Option<String>,
    /// 操作类型过滤
    pub action: Option<AuditAction>,
}

impl AuditExportRequest {
    /// 创建新的导出请求
    pub fn new() -> Self {
        Self::default()
    }

    /// 验证请求参数
    pub fn validate(&self) -> Result<(), String> {
        // 验证必填字段：start_time 和 end_time 都必须提供
        if self.start_time.is_none() {
            return Err("start_time 是必填字段".to_string());
        }
        if self.end_time.is_none() {
            return Err("end_time 是必填字段".to_string());
        }

        // 验证时间范围
        if let (Some(start), Some(end)) = (self.start_time, self.end_time) {
            if start > end {
                return Err("开始时间不能大于结束时间".to_string());
            }

            // 限制导出时间范围不超过 90 天
            let max_range = 90 * 24 * 60 * 60 * 1000; // 90 天毫秒
            if end - start > max_range {
                return Err("导出时间范围不能超过 90 天".to_string());
            }
        }

        Ok(())
    }

    /// 设置时间范围
    pub fn with_time_range(mut self, start: u64, end: u64) -> Self {
        self.start_time = Some(start);
        self.end_time = Some(end);
        self
    }

    /// 设置导出格式
    pub fn with_format(mut self, format: ExportFormat) -> Self {
        self.format = format;
        self
    }

    /// 设置用户 ID 哈希过滤
    pub fn with_user_id_hash(mut self, hash: impl Into<String>) -> Self {
        self.user_id_hash = Some(hash.into());
        self
    }

    /// 设置操作类型过滤
    pub fn with_action(mut self, action: AuditAction) -> Self {
        self.action = Some(action);
        self
    }
}

impl Default for AuditExportRequest {
    fn default() -> Self {
        Self {
            start_time: None,
            end_time: None,
            format: ExportFormat::Json,
            user_id_hash: None,
            action: None,
        }
    }
}

/// 审计日志导出响应
#[derive(Debug, Clone, Serialize)]
pub struct AuditExportResponse {
    /// 是否成功
    pub success: bool,
    /// 数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<AuditExportData>,
    /// 错误信息（失败时）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 审计日志导出数据
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AuditExportData {
    /// 导出 ID
    pub export_id: String,
    /// 导出格式
    pub format: ExportFormat,
    /// 导出内容（Base64 编码）
    pub content: String,
    /// 完整性哈希
    pub integrity_hash: String,
    /// 条目数量
    pub count: u64,
    /// 生成时间
    pub generated_at: u64,
}

impl AuditExportResponse {
    /// 创建成功响应
    pub fn success(data: AuditExportData) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    /// 创建错误响应
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

/// 审计日志验证请求
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AuditVerifyRequest {
    /// 审计条目 ID（可选，与 log_index 二选一）
    #[serde(default)]
    pub id: String,
    /// 日志索引（可选，优先于 ID）
    #[serde(default)]
    pub log_index: Option<u64>,
}

/// 审计日志验证结果
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    /// 验证通过
    Valid,
    /// 验证失败
    Invalid,
    /// 未找到条目
    NotFound,
    /// 验证错误
    Error,
}

/// 审计日志验证详情
#[derive(Debug, Clone, Serialize)]
pub struct VerificationDetail {
    /// 验证步骤
    pub step: String,
    /// 是否通过
    pub passed: bool,
    /// 详情信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// 审计日志验证响应
#[derive(Debug, Clone, Serialize)]
pub struct AuditVerifyResponse {
    /// 是否成功
    pub success: bool,
    /// 验证状态
    pub status: VerificationStatus,
    /// 数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<AuditVerifyData>,
    /// 错误信息（失败时）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 审计日志验证数据
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AuditVerifyData {
    /// 审计条目 ID
    pub id: String,
    /// 日志索引
    pub log_index: u64,
    /// 是否通过验证
    pub verified: bool,
    /// 内容哈希匹配
    pub content_hash_match: bool,
    /// 签名验证
    pub signature_valid: bool,
    /// 验证详情
    pub details: Vec<VerificationDetail>,
    /// 验证时间戳
    pub verified_at: u64,
}

impl AuditVerifyResponse {
    /// 创建验证成功响应
    pub fn verified(data: AuditVerifyData) -> Self {
        Self {
            success: true,
            status: VerificationStatus::Valid,
            data: Some(data),
            error: None,
        }
    }

    /// 创建验证失败响应
    pub fn invalid(data: AuditVerifyData) -> Self {
        Self {
            success: true,
            status: VerificationStatus::Invalid,
            data: Some(data),
            error: None,
        }
    }

    /// 创建未找到响应
    pub fn not_found(id: impl Into<String>) -> Self {
        Self {
            success: false,
            status: VerificationStatus::NotFound,
            data: None,
            error: Some(format!("审计条目未找到: {}", id.into())),
        }
    }

    /// 创建错误响应
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            status: VerificationStatus::Error,
            data: None,
            error: Some(message.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_log_query_request_default() {
        let req = AuditLogQueryRequest::default();
        assert_eq!(req.page, 1);
        assert_eq!(req.page_size, 20);
        assert_eq!(req.offset, None);
        assert_eq!(req.limit, None);
        assert!(req.start_time.is_none());
        assert!(req.end_time.is_none());
    }

    #[test]
    fn test_audit_log_query_request_validation() {
        let req = AuditLogQueryRequest::new()
            .with_start_time(2000)
            .with_end_time(1000);
        assert!(req.validate().is_err());

        let req = AuditLogQueryRequest::new().with_pagination(0, 20);
        assert!(req.validate().is_err());

        let req = AuditLogQueryRequest::new().with_pagination(1, 0);
        assert!(req.validate().is_err());

        let req = AuditLogQueryRequest::new().with_pagination(1, 1001);
        assert!(req.validate().is_err());

        let req = AuditLogQueryRequest::new().with_offset_limit(0, 1001);
        assert!(req.validate().is_err());

        let req = AuditLogQueryRequest::new()
            .with_start_time(1000)
            .with_end_time(2000)
            .with_pagination(1, 50);
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_audit_log_query_request_supports_offset_limit() {
        let req = AuditLogQueryRequest::new()
            .with_pagination(2, 20)
            .with_offset_limit(6, 3);

        assert_eq!(req.effective_page_size(), 3);
        assert_eq!(req.offset(), 6);
        assert_eq!(req.current_page(), 3);
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_audit_log_list_response() {
        let items = vec![AuditLogListItem {
            id: "test-id".to_string(),
            timestamp: 1234567890,
            user_id_hash: "user_hash".to_string(),
            session_id: "session".to_string(),
            service: "vault".to_string(),
            action: AuditAction::CredentialDecrypt,
            risk_tier: RiskTier::High,
            outcome: Outcome::Success,
            log_index: 0,
        }];

        let response = AuditLogListResponse::success(items, 1, 1, 20, 0);
        assert!(response.success);
        assert_eq!(response.data.total, 1);
        assert_eq!(response.data.page, 1);
        assert_eq!(response.data.offset, 0);
        assert_eq!(response.data.limit, 20);
        assert_eq!(response.data.total_pages, 1);
        assert!(!response.data.has_more);

        let error_response = AuditLogListResponse::error("测试错误");
        assert!(!error_response.success);
        assert_eq!(error_response.error, Some("测试错误".to_string()));
    }

    #[test]
    fn test_audit_export_request_validation() {
        // 测试缺少 end_time
        let req = AuditExportRequest {
            start_time: Some(1000),
            end_time: None,
            format: ExportFormat::Csv,
            user_id_hash: None,
            action: None,
        };
        assert!(req.validate().is_err());

        // 测试缺少 start_time
        let req = AuditExportRequest {
            start_time: None,
            end_time: Some(2000),
            format: ExportFormat::Csv,
            user_id_hash: None,
            action: None,
        };
        assert!(req.validate().is_err());

        // 测试缺少两个时间字段
        let req = AuditExportRequest::new();
        assert!(req.validate().is_err());

        // 测试开始时间大于结束时间
        let req = AuditExportRequest::new().with_time_range(2000, 1000);
        assert!(req.validate().is_err());

        // 测试超过 90 天的范围
        let start = 1000u64;
        let end = start + 91 * 24 * 60 * 60 * 1000;
        let req = AuditExportRequest::new().with_time_range(start, end);
        assert!(req.validate().is_err());

        // 测试合法时间范围
        let req = AuditExportRequest::new().with_time_range(1000, 2000);
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_verification_status_serialization() {
        assert_eq!(
            serde_json::to_string(&VerificationStatus::Valid).unwrap(),
            "\"valid\""
        );
        assert_eq!(
            serde_json::to_string(&VerificationStatus::Invalid).unwrap(),
            "\"invalid\""
        );
    }

    #[test]
    fn test_export_format_default() {
        assert_eq!(ExportFormat::default(), ExportFormat::Json);
    }

    // BUG-18228: 测试 FlexibleTime 支持多种时间格式
    #[test]
    fn test_flexible_time_from_u64() {
        let time = FlexibleTime::from(1735689600000u64);
        assert_eq!(time.as_millis(), 1735689600000);
    }

    #[test]
    fn test_flexible_time_deserialize_number() {
        // 数字格式：直接作为毫秒时间戳
        let json = r#"1735689600000"#;
        let time: FlexibleTime = serde_json::from_str(json).unwrap();
        assert_eq!(time.as_millis(), 1735689600000);
    }

    #[test]
    fn test_flexible_time_deserialize_iso8601_string() {
        // ISO 8601 字符串格式 - JSON 字符串需要双引号包裹
        let json = r#""2026-06-01T12:00:00.000Z""#;
        let time: FlexibleTime = serde_json::from_str(json).unwrap();
        // 验证解析成功，时间戳应为正数
        assert!(time.as_millis() > 0);
        // 验证可以通过 chrono 反向验证
        use chrono::TimeZone;
        let dt = chrono::Utc
            .timestamp_millis_opt(time.as_millis() as i64)
            .single()
            .unwrap();
        assert_eq!(
            dt.format("%Y-%m-%dT%H:%M:%S").to_string(),
            "2026-06-01T12:00:00"
        );
    }

    #[test]
    fn test_flexible_time_deserialize_iso8601_with_timezone() {
        // ISO 8601 带时区偏移 - JSON 字符串需要双引号包裹
        let json = r#""2026-06-01T12:00:00+08:00""#;
        let time: FlexibleTime = serde_json::from_str(json).unwrap();
        // 验证解析成功
        assert!(time.as_millis() > 0);
        // UTC 时间应该是 2026-06-01T04:00:00Z (减去8小时偏移)
        use chrono::TimeZone;
        let dt = chrono::Utc
            .timestamp_millis_opt(time.as_millis() as i64)
            .single()
            .unwrap();
        assert_eq!(
            dt.format("%Y-%m-%dT%H:%M:%S").to_string(),
            "2026-06-01T04:00:00"
        );
    }

    #[test]
    fn test_flexible_time_deserialize_number_string() {
        // 纯数字字符串（作为毫秒时间戳）
        let json = r#"1735689600000"#;
        let time: FlexibleTime = serde_json::from_str(json).unwrap();
        assert_eq!(time.as_millis(), 1735689600000);
    }

    #[test]
    fn test_flexible_time_serialize() {
        let time = FlexibleTime::from(1735689600000u64);
        let json = serde_json::to_string(&time).unwrap();
        assert_eq!(json, "1735689600000");
    }

    #[test]
    fn test_flexible_time_invalid_format() {
        // 无效格式应返回错误
        let json = r#"invalid-time"#;
        let result: Result<FlexibleTime, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_audit_log_query_request_iso8601_same_time() {
        // BUG-18228: 测试 ISO 8601 同瞬时间（start_time == end_time）应被视为合法零宽窗口
        // 模拟 URL 查询参数解析场景
        let query_json = r#"{"start_time":"2026-06-01T12:00:00.000Z","end_time":"2026-06-01T12:00:00.000Z","page":1,"page_size":20}"#;
        let req: AuditLogQueryRequest = serde_json::from_str(query_json).unwrap();

        // 验证两个时间相等
        assert_eq!(
            req.start_time.unwrap().as_millis(),
            req.end_time.unwrap().as_millis()
        );

        // 验证参数校验通过（同瞬时间应视为合法）
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_audit_log_query_request_mixed_formats() {
        // 测试混合格式：毫秒时间戳 + ISO 8601
        let query_json = r#"{"start_time":1735689600000,"end_time":"2026-06-01T12:00:00.000Z","page":1,"page_size":20}"#;
        let req: AuditLogQueryRequest = serde_json::from_str(query_json).unwrap();

        // 1735689600000 = 2025-01-01T00:00:00Z
        // 2026-06-01T12:00:00Z = 1751366400000
        // start < end，校验应通过
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_audit_log_query_request_start_greater_than_end() {
        // 起始时间大于结束时间应校验失败
        let query_json = r#"{"start_time":"2026-06-01T12:00:00.000Z","end_time":"2026-01-01T00:00:00.000Z","page":1,"page_size":20}"#;
        let req: AuditLogQueryRequest = serde_json::from_str(query_json).unwrap();
        assert!(req.validate().is_err());
    }
}
