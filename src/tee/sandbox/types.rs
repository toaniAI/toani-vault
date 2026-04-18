//! 沙箱核心类型定义

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::sync::RwLock;
use uuid::Uuid;

/// 沙箱 ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SandboxId(pub Uuid);

impl SandboxId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SandboxId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SandboxId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for SandboxId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<SandboxId> for Uuid {
    fn from(id: SandboxId) -> Self {
        id.0
    }
}

/// 会话 ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for SessionId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<SessionId> for Uuid {
    fn from(id: SessionId) -> Self {
        id.0
    }
}

/// 沙箱状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxStatus {
    /// 正在创建
    Creating,
    /// 已就绪（热实例）
    Ready,
    /// 运行中
    Running,
    /// 已暂停
    Paused,
    /// 已关闭
    Closed,
    /// 错误状态
    Error,
}

impl SandboxStatus {
    /// 检查是否为活跃状态
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            SandboxStatus::Ready | SandboxStatus::Running | SandboxStatus::Paused
        )
    }

    /// 检查是否可以执行操作
    pub fn can_execute(&self) -> bool {
        matches!(self, SandboxStatus::Running)
    }
}

impl std::fmt::Display for SandboxStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SandboxStatus::Creating => write!(f, "creating"),
            SandboxStatus::Ready => write!(f, "ready"),
            SandboxStatus::Running => write!(f, "running"),
            SandboxStatus::Paused => write!(f, "paused"),
            SandboxStatus::Closed => write!(f, "closed"),
            SandboxStatus::Error => write!(f, "error"),
        }
    }
}

/// 会话状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// 正在创建
    Creating,
    /// 已就绪
    Ready,
    /// 正在执行操作
    Executing,
    /// 已暂停
    Paused,
    /// 已关闭
    Closed,
}

impl SessionStatus {
    /// 检查是否可以执行操作
    pub fn can_execute(&self) -> bool {
        matches!(self, SessionStatus::Ready | SessionStatus::Paused)
    }

    /// 检查是否可以关闭
    pub fn can_close(&self) -> bool {
        !matches!(self, SessionStatus::Closed | SessionStatus::Creating)
    }
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionStatus::Creating => write!(f, "creating"),
            SessionStatus::Ready => write!(f, "ready"),
            SessionStatus::Executing => write!(f, "executing"),
            SessionStatus::Paused => write!(f, "paused"),
            SessionStatus::Closed => write!(f, "closed"),
        }
    }
}

/// 会话请求
#[derive(Debug, Clone)]
pub struct SessionRequest {
    /// 租户 ID
    pub tenant_id: Uuid,
    /// 用户 ID
    pub user_id: Uuid,
    /// 凭证 ID
    pub credential_id: Uuid,
    /// 原始意图描述
    pub original_intent: String,
    /// 会话元数据
    pub metadata: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialReference {
    pub field: String,
}

impl CredentialReference {
    pub fn from_value(value: &serde_json::Value) -> Option<Self> {
        let object = value.as_object()?;
        if object.len() != 1 {
            return None;
        }

        let field = object.get("$credential")?.as_str()?.trim();
        if field.is_empty() {
            return None;
        }

        Some(Self {
            field: field.to_string(),
        })
    }
}

/// 会话上下文
#[derive(Debug, Clone)]
pub struct SessionContext {
    /// 会话 ID
    pub session_id: SessionId,
    /// 沙箱 ID
    pub sandbox_id: SandboxId,
    /// 租户 ID
    pub tenant_id: Uuid,
    /// 用户 ID
    pub user_id: Uuid,
    /// 凭证 ID
    pub credential_id: Uuid,
    /// 原始意图
    pub original_intent: String,
    /// 创建时间
    pub created_at: OffsetDateTime,
    /// 过期时间
    pub expires_at: OffsetDateTime,
    /// 最后活动时间 - 使用 RwLock 实现内部可变性
    pub last_activity_at: Arc<RwLock<OffsetDateTime>>,
}

impl SessionContext {
    /// 检查会话是否已过期
    pub fn is_expired(&self) -> bool {
        OffsetDateTime::now_utc() > self.expires_at
    }

    /// 更新最后活动时间
    pub async fn touch(&self) {
        let mut activity = self.last_activity_at.write().await;
        *activity = OffsetDateTime::now_utc();
    }

    /// 获取最后活动时间
    pub async fn last_activity(&self) -> OffsetDateTime {
        *self.last_activity_at.read().await
    }
}

/// 热实例信息
#[derive(Debug, Clone)]
pub struct WarmInstanceInfo {
    /// 实例 ID
    pub instance_id: SandboxId,
    /// 创建时间
    pub created_at: OffsetDateTime,
    /// 最后使用时间
    pub last_used_at: Option<OffsetDateTime>,
    /// PID
    pub pid: u32,
}

/// 操作类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationType {
    /// 页面导航
    Navigate,
    /// 点击元素
    Click,
    /// 填写表单
    Fill,
    /// 获取文本
    GetText,
    /// 导出数据
    Export,
    /// 导出 DOM
    DomExport,
    /// 执行脚本
    ExecuteScript,
    /// 受控页面启动脚本注入
    BootstrapPage,
    /// 等待元素
    Wait,
    /// 直接发起 HTTP 请求
    HttpRequest,
    /// 自定义操作
    Custom,
}

impl OperationType {
    pub fn requires_browser(self) -> bool {
        !matches!(self, OperationType::HttpRequest | OperationType::Custom)
    }
}

impl std::fmt::Display for OperationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationType::Navigate => write!(f, "navigate"),
            OperationType::Click => write!(f, "click"),
            OperationType::Fill => write!(f, "fill"),
            OperationType::GetText => write!(f, "get_text"),
            OperationType::Export => write!(f, "export"),
            OperationType::DomExport => write!(f, "dom_export"),
            OperationType::ExecuteScript => write!(f, "execute_script"),
            OperationType::BootstrapPage => write!(f, "bootstrap_page"),
            OperationType::Wait => write!(f, "wait"),
            OperationType::HttpRequest => write!(f, "http_request"),
            OperationType::Custom => write!(f, "custom"),
        }
    }
}

/// 操作请求
#[derive(Debug, Clone)]
pub struct OperationRequest {
    /// 操作 ID
    pub operation_id: Uuid,
    /// 操作类型
    pub operation_type: OperationType,
    /// 操作描述
    pub description: String,
    /// 操作参数
    pub parameters: HashMap<String, serde_json::Value>,
    /// 运行时解析后的参数
    pub resolved_parameters: HashMap<String, serde_json::Value>,
    /// 创建时间
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct PreparedOperation {
    pub parameters: HashMap<String, serde_json::Value>,
    pub persisted_parameters: serde_json::Value,
}

impl OperationRequest {
    pub fn effective_parameters(&self) -> &HashMap<String, serde_json::Value> {
        if self.resolved_parameters.is_empty() {
            &self.parameters
        } else {
            &self.resolved_parameters
        }
    }
}

/// 操作状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    /// 待审核
    Pending,
    /// 审核中
    Reviewing,
    /// 审核通过
    Approved,
    /// 审核拒绝
    Rejected,
    /// 执行中
    Executing,
    /// 执行成功
    Completed,
    /// 执行失败
    Failed,
}

impl std::fmt::Display for OperationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationStatus::Pending => write!(f, "pending"),
            OperationStatus::Reviewing => write!(f, "reviewing"),
            OperationStatus::Approved => write!(f, "approved"),
            OperationStatus::Rejected => write!(f, "rejected"),
            OperationStatus::Executing => write!(f, "executing"),
            OperationStatus::Completed => write!(f, "completed"),
            OperationStatus::Failed => write!(f, "failed"),
        }
    }
}

/// 执行结果
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// 是否成功
    pub success: bool,
    /// 返回数据
    pub data: Option<serde_json::Value>,
    /// 错误信息
    pub error: Option<String>,
    /// 执行时间（毫秒）
    pub execution_time_ms: u64,
    /// 审计日志
    pub audit_log: Vec<AuditLogEntry>,
}

/// 审计日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    /// 时间戳
    pub timestamp: OffsetDateTime,
    /// 事件类型
    pub event_type: String,
    /// 严重级别
    pub severity: AuditSeverity,
    /// 详细信息
    pub details: serde_json::Value,
}

/// 审计严重级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

impl std::fmt::Display for AuditSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditSeverity::Info => write!(f, "info"),
            AuditSeverity::Warning => write!(f, "warning"),
            AuditSeverity::Error => write!(f, "error"),
            AuditSeverity::Critical => write!(f, "critical"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_id() {
        let id1 = SandboxId::new();
        let id2 = SandboxId::new();
        assert_ne!(id1.0, id2.0);
    }

    #[test]
    fn test_sandbox_status() {
        assert!(SandboxStatus::Running.is_active());
        assert!(SandboxStatus::Ready.is_active());
        assert!(!SandboxStatus::Closed.is_active());

        assert!(SandboxStatus::Running.can_execute());
        assert!(!SandboxStatus::Ready.can_execute());
    }

    #[test]
    fn test_session_status() {
        assert!(SessionStatus::Ready.can_execute());
        assert!(!SessionStatus::Executing.can_execute());

        assert!(SessionStatus::Ready.can_close());
        assert!(!SessionStatus::Closed.can_close());
    }

    #[test]
    fn test_session_context_expired() {
        let now = OffsetDateTime::now_utc();
        let context = SessionContext {
            session_id: SessionId::new(),
            sandbox_id: SandboxId::new(),
            tenant_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            credential_id: Uuid::new_v4(),
            original_intent: "test".to_string(),
            created_at: now,
            expires_at: now - time::Duration::seconds(1),
            last_activity_at: Arc::new(RwLock::new(now)),
        };
        assert!(context.is_expired());
    }

    #[tokio::test]
    async fn test_session_context_touch() {
        let now = OffsetDateTime::now_utc();
        let context = SessionContext {
            session_id: SessionId::new(),
            sandbox_id: SandboxId::new(),
            tenant_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            credential_id: Uuid::new_v4(),
            original_intent: "test".to_string(),
            created_at: now,
            expires_at: now + time::Duration::minutes(30),
            last_activity_at: Arc::new(RwLock::new(now)),
        };

        // 记录初始活动时间
        let initial_activity = context.last_activity().await;

        // 等待一小段时间
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // 更新活动时间
        context.touch().await;

        // 验证时间已更新
        let new_activity = context.last_activity().await;
        assert!(new_activity > initial_activity);
    }

    #[test]
    fn test_operation_type_display() {
        assert_eq!(OperationType::Navigate.to_string(), "navigate");
        assert_eq!(OperationType::DomExport.to_string(), "dom_export");
        assert_eq!(OperationType::BootstrapPage.to_string(), "bootstrap_page");
        assert_eq!(OperationType::HttpRequest.to_string(), "http_request");
    }
}
