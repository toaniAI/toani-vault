//! 沙箱会话管理

use crate::tee::sandbox::{
    error::{SandboxError, SessionError},
    nsjail::NsjailSandbox,
    review::{OperationReviewer, ReviewContext, SuggestedAction},
    types::{
        ExecutionResult, OperationRequest, OperationStatus, SessionContext, SessionId,
        SessionStatus,
    },
};
use async_trait::async_trait;
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// 沙箱会话 trait
#[async_trait]
pub trait SandboxSession: Send + Sync {
    /// 获取会话 ID
    fn id(&self) -> SessionId;
    /// 获取会话状态
    async fn status(&self) -> SessionStatus;
    /// 获取会话上下文
    fn context(&self) -> &SessionContext;
    /// 执行操作
    async fn execute_operation(
        &self,
        operation: OperationRequest,
    ) -> Result<ExecutionResult, SandboxError>;
    /// 暂停会话
    async fn pause(&self) -> Result<(), SandboxError>;
    /// 恢复会话
    async fn resume(&self) -> Result<(), SandboxError>;
    /// 关闭会话
    async fn close(&self) -> Result<(), SandboxError>;
    /// 检查是否过期
    fn is_expired(&self) -> bool;
}

/// 活跃的 nsjail 会话
#[derive(Clone)]
pub struct ActiveNsjailSession {
    /// 会话 ID
    pub id: SessionId,
    /// 会话上下文
    context: SessionContext,
    /// 会话状态
    status: Arc<RwLock<SessionStatus>>,
    /// 沙箱实例
    sandbox: Arc<RwLock<Option<NsjailSandbox>>>,
    /// 操作历史
    operation_history: Arc<RwLock<Vec<OperationRecord>>>,
    /// 操作审核器
    operation_reviewer: Option<Arc<OperationReviewer>>,
}

/// 操作记录
#[derive(Debug, Clone)]
pub struct OperationRecord {
    /// 操作 ID
    pub operation_id: Uuid,
    /// 操作类型
    pub operation_type: String,
    /// 状态
    pub status: OperationStatus,
    /// 开始时间
    pub started_at: OffsetDateTime,
    /// 完成时间
    pub completed_at: Option<OffsetDateTime>,
    /// 执行时间（毫秒）
    pub execution_time_ms: Option<u64>,
}

impl ActiveNsjailSession {
    /// 创建新的活跃会话
    pub fn new(id: SessionId, context: SessionContext, sandbox: NsjailSandbox) -> Self {
        Self {
            id,
            context,
            status: Arc::new(RwLock::new(SessionStatus::Ready)),
            sandbox: Arc::new(RwLock::new(Some(sandbox))),
            operation_history: Arc::new(RwLock::new(Vec::new())),
            operation_reviewer: None,
        }
    }

    /// 提取 sandbox（用于回收）
    /// 注意：调用此方法后，session 将不再拥有 sandbox
    pub async fn take_sandbox(&self) -> Option<NsjailSandbox> {
        let mut sandbox_guard = self.sandbox.write().await;
        sandbox_guard.take()
    }

    /// 创建带审核器的活跃会话
    pub fn with_reviewer(
        id: SessionId,
        context: SessionContext,
        sandbox: NsjailSandbox,
        reviewer: Arc<OperationReviewer>,
    ) -> Self {
        Self {
            id,
            context,
            status: Arc::new(RwLock::new(SessionStatus::Ready)),
            sandbox: Arc::new(RwLock::new(Some(sandbox))),
            operation_history: Arc::new(RwLock::new(Vec::new())),
            operation_reviewer: Some(reviewer),
        }
    }

    /// 设置操作审核器
    pub fn set_operation_reviewer(&mut self, reviewer: Arc<OperationReviewer>) {
        self.operation_reviewer = Some(reviewer);
    }

    /// 检查是否可以执行操作
    async fn can_execute(&self) -> Result<(), SessionError> {
        // 检查会话状态
        let status = *self.status.read().await;
        if !status.can_execute() {
            return Err(SessionError::invalid_state(
                self.id.into(),
                status,
                "ready or paused",
            ));
        }

        // 检查是否过期
        if self.is_expired() {
            return Err(SessionError::expired(self.id.into()));
        }

        // 检查沙箱是否运行
        let sandbox_guard = self.sandbox.read().await;
        if let Some(ref sandbox) = *sandbox_guard {
            if !sandbox.is_running().await {
                return Err(SessionError::CreationFailed {
                    reason: "Sandbox is not running".to_string(),
                });
            }
        } else {
            return Err(SessionError::CreationFailed {
                reason: "Sandbox not available".to_string(),
            });
        }

        Ok(())
    }

    /// 更新最后活动时间
    async fn touch(&self) {
        self.context.touch().await;
    }

    /// 添加操作记录
    async fn add_operation_record(&self, record: OperationRecord) {
        let mut history = self.operation_history.write().await;
        history.push(record);
    }

    /// 获取操作历史
    pub async fn get_operation_history(&self) -> Vec<OperationRecord> {
        self.operation_history.read().await.clone()
    }
}

#[async_trait]
impl SandboxSession for ActiveNsjailSession {
    fn id(&self) -> SessionId {
        self.id
    }

    async fn status(&self) -> SessionStatus {
        *self.status.read().await
    }

    fn context(&self) -> &SessionContext {
        &self.context
    }

    async fn execute_operation(
        &self,
        operation: OperationRequest,
    ) -> Result<ExecutionResult, SandboxError> {
        // 检查是否可以执行
        self.can_execute().await.map_err(SandboxError::Session)?;

        // 更新状态为执行中
        *self.status.write().await = SessionStatus::Executing;

        // 更新活动时间
        self.touch().await;

        info!(
            "Executing operation {} ({}) in session {}",
            operation.operation_id, operation.operation_type, self.id
        );

        let start_time = OffsetDateTime::now_utc();

        // 创建操作记录
        let record = OperationRecord {
            operation_id: operation.operation_id,
            operation_type: operation.operation_type.to_string(),
            status: OperationStatus::Executing,
            started_at: start_time,
            completed_at: None,
            execution_time_ms: None,
        };
        self.add_operation_record(record).await;

        // AI 审核流程
        if let Some(ref reviewer) = self.operation_reviewer {
            let review_context = ReviewContext::new(
                self.context.session_id.0,
                self.context.tenant_id,
                self.context.user_id,
                self.context.credential_id,
                &self.context.original_intent,
            )
            .with_operation_type(operation.operation_type.to_string())
            .with_description(&operation.description);

            match reviewer.review_operation(&review_context, &operation).await {
                Ok(review_result) => {
                    // 根据审核结果处理
                    match review_result.suggested_action {
                        SuggestedAction::Proceed => {
                            debug!("Operation {} approved, proceeding", operation.operation_id);
                        }
                        SuggestedAction::LogAndProceed => {
                            info!(
                                "Operation {} approved with logging: {}",
                                operation.operation_id, review_result.reason
                            );
                        }
                        SuggestedAction::RequireConfirmation => {
                            warn!(
                                "Operation {} requires confirmation: {}",
                                operation.operation_id, review_result.reason
                            );
                            // 合并锁获取：只获取一次写锁并更新状态
                            {
                                let mut history = self.operation_history.write().await;
                                if let Some(record) = history.last_mut() {
                                    record.status = OperationStatus::Pending;
                                }
                            }
                            // 恢复状态为就绪
                            *self.status.write().await = SessionStatus::Ready;
                            return Err(SessionError::OperationRejected {
                                reason: format!(
                                    "Operation requires confirmation: {}",
                                    review_result.reason
                                ),
                            }
                            .into());
                        }
                        SuggestedAction::RequireAdditionalAuth => {
                            warn!(
                                "Operation {} requires additional authentication: {}",
                                operation.operation_id, review_result.reason
                            );
                            // 合并锁获取：只获取一次写锁并更新状态
                            {
                                let mut history = self.operation_history.write().await;
                                if let Some(record) = history.last_mut() {
                                    record.status = OperationStatus::Pending;
                                }
                            }
                            // 恢复状态为就绪
                            *self.status.write().await = SessionStatus::Ready;
                            return Err(SessionError::OperationRejected {
                                reason: format!(
                                    "Operation requires additional authentication: {}",
                                    review_result.reason
                                ),
                            }
                            .into());
                        }
                        SuggestedAction::Reject => {
                            warn!(
                                "Operation {} rejected: {}",
                                operation.operation_id, review_result.reason
                            );
                            // 更新操作记录状态为拒绝
                            if let Some(record) = self.operation_history.write().await.last_mut() {
                                record.status = OperationStatus::Rejected;
                                record.completed_at = Some(OffsetDateTime::now_utc());
                            }
                            // 恢复状态为就绪
                            *self.status.write().await = SessionStatus::Ready;
                            return Err(SessionError::OperationRejected {
                                reason: format!("Operation rejected: {}", review_result.reason),
                            }
                            .into());
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        "AI review failed for operation {}: {}",
                        operation.operation_id, e
                    );
                    // 审核失败时，如果审核器配置为严格模式，则拒绝操作
                    if reviewer.is_strict_mode() {
                        if let Some(record) = self.operation_history.write().await.last_mut() {
                            record.status = OperationStatus::Rejected;
                            record.completed_at = Some(OffsetDateTime::now_utc());
                        }
                        *self.status.write().await = SessionStatus::Ready;
                        return Err(SessionError::OperationRejected {
                            reason: format!("AI review failed: {}", e),
                        }
                        .into());
                    }
                    // 非严格模式下，记录警告但继续执行
                    info!(
                        "AI review failed but strict mode is off, proceeding with operation {}",
                        operation.operation_id
                    );
                }
            }
        } else {
            debug!("No operation reviewer configured, skipping AI review");
        }

        // 执行操作
        let result = match self.execute_in_sandbox(&operation).await {
            Ok(data) => {
                let execution_time_ms =
                    (OffsetDateTime::now_utc() - start_time).whole_milliseconds() as u64;

                // 更新操作记录
                if let Some(record) = self.operation_history.write().await.last_mut() {
                    record.status = OperationStatus::Completed;
                    record.completed_at = Some(OffsetDateTime::now_utc());
                    record.execution_time_ms = Some(execution_time_ms);
                }

                ExecutionResult {
                    success: true,
                    data: Some(serde_json::json!({"result": data})),
                    error: None,
                    execution_time_ms,
                    screenshot: None,
                    audit_log: vec![],
                }
            }
            Err(e) => {
                let execution_time_ms =
                    (OffsetDateTime::now_utc() - start_time).whole_milliseconds() as u64;

                // 更新操作记录
                if let Some(record) = self.operation_history.write().await.last_mut() {
                    record.status = OperationStatus::Failed;
                    record.completed_at = Some(OffsetDateTime::now_utc());
                    record.execution_time_ms = Some(execution_time_ms);
                }

                ExecutionResult {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                    execution_time_ms,
                    screenshot: None,
                    audit_log: vec![],
                }
            }
        };

        // 恢复状态为就绪
        *self.status.write().await = SessionStatus::Ready;

        info!(
            "Operation {} completed in {}ms, success={}",
            operation.operation_id, result.execution_time_ms, result.success
        );

        Ok(result)
    }

    async fn pause(&self) -> Result<(), SandboxError> {
        let mut status = self.status.write().await;

        if *status != SessionStatus::Ready {
            return Err(SessionError::invalid_state(self.id.into(), *status, "ready").into());
        }

        *status = SessionStatus::Paused;
        info!("Session {} paused", self.id);

        Ok(())
    }

    async fn resume(&self) -> Result<(), SandboxError> {
        let mut status = self.status.write().await;

        if *status != SessionStatus::Paused {
            return Err(SessionError::invalid_state(self.id.into(), *status, "paused").into());
        }

        *status = SessionStatus::Ready;
        info!("Session {} resumed", self.id);

        Ok(())
    }

    async fn close(&self) -> Result<(), SandboxError> {
        let mut status = self.status.write().await;

        if *status == SessionStatus::Closed {
            return Ok(());
        }

        info!("Closing session {}", self.id);

        // 停止沙箱
        let mut sandbox_guard = self.sandbox.write().await;
        if let Some(mut sandbox) = sandbox_guard.take() {
            if let Err(e) = sandbox.stop().await {
                warn!("Failed to stop sandbox for session {}: {}", self.id, e);
            }
        }

        *status = SessionStatus::Closed;
        info!("Session {} closed", self.id);

        Ok(())
    }

    fn is_expired(&self) -> bool {
        OffsetDateTime::now_utc() > self.context.expires_at
    }
}

impl ActiveNsjailSession {
    /// 在沙箱中执行操作（模拟实现）
    async fn execute_in_sandbox(
        &self,
        operation: &OperationRequest,
    ) -> Result<String, SandboxError> {
        // 这里是实际的浏览器自动化逻辑
        // 使用 Playwright 或其他浏览器自动化工具

        debug!("Executing operation in sandbox: {:?}", operation);

        // 模拟延迟
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // 根据操作类型返回不同结果
        match operation.operation_type {
            crate::tee::sandbox::types::OperationType::Navigate => Ok(format!(
                "Navigated to {:?}",
                operation.parameters.get("url")
            )),
            crate::tee::sandbox::types::OperationType::Click => Ok("Element clicked".to_string()),
            crate::tee::sandbox::types::OperationType::GetText => {
                Ok("Sample text content".to_string())
            }
            _ => Ok(format!("Operation {:?} executed", operation.operation_type)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tee::sandbox::config::SandboxConfig;
    use crate::tee::sandbox::nsjail::NsjailSandbox;
    use crate::tee::sandbox::types::{OperationType, SandboxId};
    use std::collections::HashMap;

    fn create_test_session() -> ActiveNsjailSession {
        let config = SandboxConfig::default();
        let nsjail_config = crate::tee::sandbox::config::NsjailConfig {
            sandbox: config,
            command: vec!["sleep".to_string(), "100".to_string()],
            cwd: std::path::PathBuf::from("/"),
            env: HashMap::new(),
            uid_map: Default::default(),
            gid_map: Default::default(),
        };

        let sandbox = NsjailSandbox::new(nsjail_config);
        let context = SessionContext {
            session_id: SessionId::new(),
            sandbox_id: SandboxId::new(),
            tenant_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            credential_id: Uuid::new_v4(),
            original_intent: "test".to_string(),
            created_at: OffsetDateTime::now_utc(),
            expires_at: OffsetDateTime::now_utc() + time::Duration::minutes(30),
            last_activity_at: Arc::new(RwLock::new(OffsetDateTime::now_utc())),
        };

        ActiveNsjailSession::new(SessionId::new(), context, sandbox)
    }

    #[test]
    fn test_session_creation() {
        let session = create_test_session();
        assert!(session.is_expired() == false);
    }

    #[tokio::test]
    async fn test_session_status() {
        let session = create_test_session();
        let status = session.status().await;
        assert!(matches!(status, SessionStatus::Ready));
    }

    #[tokio::test]
    async fn test_session_pause_resume() {
        let session = create_test_session();

        // 暂停
        session.pause().await.unwrap();
        assert!(matches!(session.status().await, SessionStatus::Paused));

        // 恢复
        session.resume().await.unwrap();
        assert!(matches!(session.status().await, SessionStatus::Ready));
    }

    #[tokio::test]
    async fn test_session_close() {
        let session = create_test_session();

        session.close().await.unwrap();
        assert!(matches!(session.status().await, SessionStatus::Closed));
    }
}
