//! 沙箱持久化仓储

use crate::tee::sandbox::{error::SandboxError, types::SessionId};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NewSandboxSessionRecord {
    pub session_id: SessionId,
    pub sandbox_id: Uuid,
    pub tenant_id: Uuid,
    pub created_by: Uuid,
    pub credential_id: Uuid,
    pub original_intent: String,
    pub started_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub metadata: Value,
}

#[derive(Debug, Clone)]
pub struct NewSandboxOperationRecord {
    pub operation_id: Uuid,
    pub session_id: SessionId,
    pub tenant_id: Uuid,
    pub credential_id: Uuid,
    pub operation_type: String,
    pub input_params: Value,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SandboxSessionRecord {
    pub session_id: SessionId,
    pub sandbox_id: Uuid,
    pub tenant_id: Uuid,
    pub created_by: Uuid,
    pub credential_id: Uuid,
    pub original_intent: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub terminated_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SandboxOperationRecord {
    pub operation_id: Uuid,
    pub session_id: SessionId,
    pub operation_type: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub execution_duration_ms: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct CompleteSandboxOperationRecord {
    pub operation_id: Uuid,
    pub status: &'static str,
    pub output_result: Option<Value>,
    pub error_message: Option<String>,
    pub completed_at: DateTime<Utc>,
    pub execution_duration_ms: i32,
}

#[async_trait]
pub trait SandboxRepository: Send + Sync {
    async fn create_session(&self, record: NewSandboxSessionRecord) -> Result<(), SandboxError>;

    async fn mark_session_terminated(
        &self,
        session_id: SessionId,
        status: &'static str,
        reason: Option<String>,
        terminated_at: DateTime<Utc>,
    ) -> Result<(), SandboxError>;

    async fn update_session_status(
        &self,
        session_id: SessionId,
        status: &'static str,
    ) -> Result<(), SandboxError>;

    async fn create_operation(&self, record: NewSandboxOperationRecord)
    -> Result<(), SandboxError>;

    async fn complete_operation(
        &self,
        record: CompleteSandboxOperationRecord,
    ) -> Result<(), SandboxError>;

    async fn reconcile_orphaned_active_sessions(
        &self,
        recovery_reason: &str,
    ) -> Result<u64, SandboxError>;

    async fn list_sessions_by_tenant(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<SandboxSessionRecord>, SandboxError>;

    async fn get_session_by_id(
        &self,
        tenant_id: Uuid,
        session_id: SessionId,
    ) -> Result<Option<SandboxSessionRecord>, SandboxError>;

    async fn get_operation_by_id(
        &self,
        tenant_id: Uuid,
        operation_id: Uuid,
    ) -> Result<Option<SandboxOperationRecord>, SandboxError>;
}

#[derive(Clone)]
pub struct PostgresSandboxRepository {
    pool: PgPool,
}

impl PostgresSandboxRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn with_intent_metadata(metadata: Value, credential_id: Uuid, original_intent: &str) -> Value {
        let mut merged = metadata;
        if !merged.is_object() {
            merged = Value::Object(Default::default());
        }
        if let Some(map) = merged.as_object_mut() {
            map.insert(
                "credential_id".to_string(),
                Value::String(credential_id.to_string()),
            );
            map.insert(
                "original_intent".to_string(),
                Value::String(original_intent.to_string()),
            );
        }
        merged
    }

    fn parse_sandbox_id(raw: Option<String>) -> Uuid {
        raw.and_then(|value| Uuid::parse_str(&value).ok())
            .unwrap_or_else(Uuid::nil)
    }

    fn parse_credential_id(typed: Option<Uuid>, metadata: &Value) -> Uuid {
        if let Some(value) = typed {
            return value;
        }
        metadata
            .get("credential_id")
            .and_then(Value::as_str)
            .and_then(|raw| Uuid::parse_str(raw).ok())
            .unwrap_or_else(Uuid::nil)
    }

    fn parse_original_intent(typed: Option<String>, metadata: &Value) -> String {
        typed
            .or_else(|| {
                metadata
                    .get("original_intent")
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
            })
            .unwrap_or_default()
    }
}

#[async_trait]
impl SandboxRepository for PostgresSandboxRepository {
    async fn create_session(&self, record: NewSandboxSessionRecord) -> Result<(), SandboxError> {
        let metadata = Self::with_intent_metadata(
            record.metadata,
            record.credential_id,
            &record.original_intent,
        );
        let sandbox_context_id = record.sandbox_id.to_string();
        sqlx::query(
            r#"
            INSERT INTO sandbox_sessions (
                id,
                tenant_id,
                created_by,
                credential_id,
                original_intent,
                status,
                started_at,
                expires_at,
                tee_context_id,
                metadata
            )
            VALUES ($1, $2, $3, $4, $5, 'active', $6, $7, $8, $9)
            "#,
        )
        .bind(Uuid::from(record.session_id))
        .bind(record.tenant_id)
        .bind(record.created_by)
        .bind(record.credential_id)
        .bind(record.original_intent)
        .bind(record.started_at)
        .bind(record.expires_at)
        .bind(sandbox_context_id)
        .bind(metadata)
        .execute(&self.pool)
        .await
        .map_err(|error| SandboxError::Pool(format!("持久化沙箱会话失败: {error}")))?;
        Ok(())
    }

    async fn mark_session_terminated(
        &self,
        session_id: SessionId,
        status: &'static str,
        reason: Option<String>,
        terminated_at: DateTime<Utc>,
    ) -> Result<(), SandboxError> {
        sqlx::query(
            r#"
            UPDATE sandbox_sessions
            SET status = $2,
                terminated_at = COALESCE(terminated_at, $3),
                termination_reason = $4,
                updated_at = NOW()
            WHERE id = $1
              AND status IN ('active', 'paused')
            "#,
        )
        .bind(Uuid::from(session_id))
        .bind(status)
        .bind(terminated_at)
        .bind(reason)
        .execute(&self.pool)
        .await
        .map_err(|error| SandboxError::Pool(format!("更新沙箱会话状态失败: {error}")))?;
        Ok(())
    }

    async fn update_session_status(
        &self,
        session_id: SessionId,
        status: &'static str,
    ) -> Result<(), SandboxError> {
        sqlx::query(
            r#"
            UPDATE sandbox_sessions
            SET status = $2,
                updated_at = NOW()
            WHERE id = $1
              AND status IN ('active', 'paused')
            "#,
        )
        .bind(Uuid::from(session_id))
        .bind(status)
        .execute(&self.pool)
        .await
        .map_err(|error| SandboxError::Pool(format!("更新沙箱会话状态失败: {error}")))?;
        Ok(())
    }

    async fn create_operation(
        &self,
        record: NewSandboxOperationRecord,
    ) -> Result<(), SandboxError> {
        sqlx::query(
            r#"
            INSERT INTO sandbox_operations (
                id,
                session_id,
                tenant_id,
                operation_type,
                status,
                input_params,
                started_at,
                credential_id
            )
            VALUES ($1, $2, $3, $4, 'running', $5, $6, $7)
            "#,
        )
        .bind(record.operation_id)
        .bind(Uuid::from(record.session_id))
        .bind(record.tenant_id)
        .bind(record.operation_type)
        .bind(record.input_params)
        .bind(record.started_at)
        .bind(record.credential_id)
        .execute(&self.pool)
        .await
        .map_err(|error| SandboxError::Pool(format!("持久化沙箱操作失败: {error}")))?;
        Ok(())
    }

    async fn complete_operation(
        &self,
        record: CompleteSandboxOperationRecord,
    ) -> Result<(), SandboxError> {
        sqlx::query(
            r#"
            UPDATE sandbox_operations
            SET status = $2,
                output_result = COALESCE(output_result, $3),
                error_message = COALESCE(error_message, $4),
                completed_at = COALESCE(completed_at, $5),
                execution_duration_ms = COALESCE(execution_duration_ms, $6)
            WHERE id = $1
              AND status = 'running'
            "#,
        )
        .bind(record.operation_id)
        .bind(record.status)
        .bind(record.output_result)
        .bind(record.error_message)
        .bind(record.completed_at)
        .bind(record.execution_duration_ms)
        .execute(&self.pool)
        .await
        .map_err(|error| SandboxError::Pool(format!("更新沙箱操作状态失败: {error}")))?;
        Ok(())
    }

    async fn reconcile_orphaned_active_sessions(
        &self,
        recovery_reason: &str,
    ) -> Result<u64, SandboxError> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            UPDATE sandbox_sessions
            SET status = 'expired',
                terminated_at = $1,
                termination_reason = $2,
                updated_at = NOW()
            WHERE status = 'active'
            "#,
        )
        .bind(now)
        .bind(recovery_reason)
        .execute(&self.pool)
        .await
        .map_err(|error| SandboxError::Pool(format!("恢复孤儿沙箱会话失败: {error}")))?;
        Ok(result.rows_affected())
    }

    async fn list_sessions_by_tenant(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<SandboxSessionRecord>, SandboxError> {
        let rows = sqlx::query(
            r#"
            SELECT id,
                   tenant_id,
                   created_by,
                   credential_id,
                   original_intent,
                   status,
                   started_at,
                   expires_at,
                   terminated_at,
                   updated_at,
                   tee_context_id,
                   metadata
            FROM sandbox_sessions
            WHERE tenant_id = $1
            ORDER BY started_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| SandboxError::Pool(format!("查询沙箱会话失败: {error}")))?;

        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            let metadata: Value = row
                .try_get("metadata")
                .unwrap_or_else(|_| Value::Object(Default::default()));
            records.push(SandboxSessionRecord {
                session_id: SessionId(
                    row.try_get("id")
                        .map_err(|error| SandboxError::Pool(format!("读取会话ID失败: {error}")))?,
                ),
                sandbox_id: Self::parse_sandbox_id(row.try_get("tee_context_id").ok()),
                tenant_id: row
                    .try_get("tenant_id")
                    .map_err(|error| SandboxError::Pool(format!("读取租户ID失败: {error}")))?,
                created_by: row
                    .try_get("created_by")
                    .map_err(|error| SandboxError::Pool(format!("读取创建者ID失败: {error}")))?,
                credential_id: Self::parse_credential_id(
                    row.try_get("credential_id").ok(),
                    &metadata,
                ),
                original_intent: Self::parse_original_intent(
                    row.try_get("original_intent").ok(),
                    &metadata,
                ),
                status: row
                    .try_get("status")
                    .map_err(|error| SandboxError::Pool(format!("读取会话状态失败: {error}")))?,
                started_at: row.try_get("started_at").map_err(|error| {
                    SandboxError::Pool(format!("读取会话开始时间失败: {error}"))
                })?,
                expires_at: row.try_get("expires_at").map_err(|error| {
                    SandboxError::Pool(format!("读取会话过期时间失败: {error}"))
                })?,
                terminated_at: row.try_get("terminated_at").ok(),
                updated_at: row.try_get("updated_at").map_err(|error| {
                    SandboxError::Pool(format!("读取会话更新时间失败: {error}"))
                })?,
            });
        }
        Ok(records)
    }

    async fn get_session_by_id(
        &self,
        tenant_id: Uuid,
        session_id: SessionId,
    ) -> Result<Option<SandboxSessionRecord>, SandboxError> {
        let row = sqlx::query(
            r#"
            SELECT id,
                   tenant_id,
                   created_by,
                   credential_id,
                   original_intent,
                   status,
                   started_at,
                   expires_at,
                   terminated_at,
                   updated_at,
                   tee_context_id,
                   metadata
            FROM sandbox_sessions
            WHERE tenant_id = $1
              AND id = $2
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(Uuid::from(session_id))
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| SandboxError::Pool(format!("查询沙箱会话详情失败: {error}")))?;

        let Some(row) = row else {
            return Ok(None);
        };
        let metadata: Value = row
            .try_get("metadata")
            .unwrap_or_else(|_| Value::Object(Default::default()));

        Ok(Some(SandboxSessionRecord {
            session_id: SessionId(
                row.try_get("id")
                    .map_err(|error| SandboxError::Pool(format!("读取会话ID失败: {error}")))?,
            ),
            sandbox_id: Self::parse_sandbox_id(row.try_get("tee_context_id").ok()),
            tenant_id: row
                .try_get("tenant_id")
                .map_err(|error| SandboxError::Pool(format!("读取租户ID失败: {error}")))?,
            created_by: row
                .try_get("created_by")
                .map_err(|error| SandboxError::Pool(format!("读取创建者ID失败: {error}")))?,
            credential_id: Self::parse_credential_id(row.try_get("credential_id").ok(), &metadata),
            original_intent: Self::parse_original_intent(
                row.try_get("original_intent").ok(),
                &metadata,
            ),
            status: row
                .try_get("status")
                .map_err(|error| SandboxError::Pool(format!("读取会话状态失败: {error}")))?,
            started_at: row
                .try_get("started_at")
                .map_err(|error| SandboxError::Pool(format!("读取会话开始时间失败: {error}")))?,
            expires_at: row
                .try_get("expires_at")
                .map_err(|error| SandboxError::Pool(format!("读取会话过期时间失败: {error}")))?,
            terminated_at: row.try_get("terminated_at").ok(),
            updated_at: row
                .try_get("updated_at")
                .map_err(|error| SandboxError::Pool(format!("读取会话更新时间失败: {error}")))?,
        }))
    }

    async fn get_operation_by_id(
        &self,
        tenant_id: Uuid,
        operation_id: Uuid,
    ) -> Result<Option<SandboxOperationRecord>, SandboxError> {
        let row = sqlx::query(
            r#"
            SELECT id,
                   session_id,
                   operation_type,
                   status,
                   started_at,
                   completed_at,
                   execution_duration_ms
            FROM sandbox_operations
            WHERE tenant_id = $1
              AND id = $2
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(operation_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| SandboxError::Pool(format!("查询沙箱操作详情失败: {error}")))?;

        Ok(row.map(|record| SandboxOperationRecord {
            operation_id: record.try_get("id").unwrap_or(operation_id),
            session_id: SessionId(record.try_get("session_id").unwrap_or_else(|_| Uuid::nil())),
            operation_type: record
                .try_get("operation_type")
                .unwrap_or_else(|_| "unknown".to_string()),
            status: record
                .try_get("status")
                .unwrap_or_else(|_| "unknown".to_string()),
            started_at: record.try_get("started_at").unwrap_or_else(|_| Utc::now()),
            completed_at: record.try_get("completed_at").ok(),
            execution_duration_ms: record.try_get("execution_duration_ms").ok(),
        }))
    }
}

pub fn to_chrono_utc(datetime: time::OffsetDateTime) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(datetime.unix_timestamp(), datetime.nanosecond())
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
}

pub fn metadata_to_json(metadata: Option<std::collections::HashMap<String, String>>) -> Value {
    match metadata {
        Some(map) => {
            serde_json::to_value(map).unwrap_or_else(|_| Value::Object(Default::default()))
        }
        None => Value::Object(Default::default()),
    }
}
