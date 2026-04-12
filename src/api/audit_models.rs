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

/// 审计日志查询请求参数
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AuditLogQueryRequest {
    /// 开始时间（Unix 时间戳毫秒）
    pub start_time: Option<u64>,
    /// 结束时间（Unix 时间戳毫秒）
    pub end_time: Option<u64>,
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
            && start > end
        {
            return Err("开始时间不能大于结束时间".to_string());
        }

        // 验证页码
        if self.page == 0 {
            return Err("页码必须从 1 开始".to_string());
        }

        // 验证分页大小
        if self.page_size == 0 || self.page_size > 1000 {
            return Err("分页大小必须在 1-1000 之间".to_string());
        }

        Ok(())
    }

    /// 获取偏移量
    pub fn offset(&self) -> usize {
        (self.page - 1) * self.page_size
    }

    /// 设置开始时间
    pub fn with_start_time(mut self, time: u64) -> Self {
        self.start_time = Some(time);
        self
    }

    /// 设置结束时间
    pub fn with_end_time(mut self, time: u64) -> Self {
        self.end_time = Some(time);
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
    /// 总页数
    pub total_pages: usize,
}

impl AuditLogListResponse {
    /// 创建成功响应
    pub fn success(
        items: Vec<AuditLogListItem>,
        total: u64,
        page: usize,
        page_size: usize,
    ) -> Self {
        let total_pages = ((total as f64) / (page_size as f64)).ceil() as usize;
        Self {
            success: true,
            data: AuditLogListData {
                items,
                total,
                page,
                page_size,
                total_pages,
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
                total_pages: 0,
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

/// Merkle Tree 证明响应
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct MerkleProofResponse {
    /// 包含证明
    pub inclusion_proof: Vec<String>,
    /// 一致性证明
    pub consistency_proof: Vec<String>,
    /// 树大小
    pub tree_size: u64,
    /// 根哈希
    pub root_hash: String,
    /// 事务 ID
    pub transaction_id: u64,
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
    /// Merkle 根哈希
    pub merkle_root: String,
    /// 签名者指纹
    pub signer_fingerprint: String,
    /// Merkle Tree 证明
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof: Option<MerkleProofResponse>,
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

impl From<(SignedAuditEntry, Option<MerkleProofResponse>)> for AuditLogDetailData {
    fn from((entry, proof): (SignedAuditEntry, Option<MerkleProofResponse>)) -> Self {
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
            merkle_root: hex::encode(entry.merkle_root),
            signer_fingerprint: entry.signer_fingerprint.clone(),
            proof,
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
    /// Merkle 证明验证
    pub merkle_proof_valid: bool,
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

        let req = AuditLogQueryRequest::new()
            .with_start_time(1000)
            .with_end_time(2000)
            .with_pagination(1, 50);
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

        let response = AuditLogListResponse::success(items, 1, 1, 20);
        assert!(response.success);
        assert_eq!(response.data.total, 1);
        assert_eq!(response.data.page, 1);
        assert_eq!(response.data.total_pages, 1);

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
}
