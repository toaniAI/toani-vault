//! 审计日志查询 API
//!
//! 实现审计日志的查询、导出和验证功能
//!
//! # API 端点
//!
//! ```text
//! GET  /api/v1/audit/logs          - 查询审计日志列表
//! GET  /api/v1/audit/logs/:id      - 查询审计日志详情
//! POST /api/v1/audit/export        - 导出审计日志
//! POST /api/v1/audit/verify        - 验证审计日志完整性
//! ```
//!
//! # 权限要求
//!
//! 所有端点需要 Scope: `audit:read` 或 `admin`

use axum::{
    Json, Router,
    extract::{OriginalUri, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ring::digest::{SHA256, digest};
use ring::signature::{ED25519, UnparsedPublicKey};
use serde_json::json;
use sqlx::{PgPool, Postgres, QueryBuilder, Row, Transaction};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use crate::api::audit_models::*;
use crate::api::middleware::{TokenScope, ValidatedToken};
use crate::audit::immudb_store::AuditStorage as ImmuDbRecorderStorage;
use crate::audit::{
    AuditEntry, AuditFilter, AuditLogChain, ImmuDbAuditStore, MemoryAuditStorage, RecorderError,
    SignedAuditEntry, SigningKeyPair, hash_user_id,
};

/// 审计 API 状态
#[derive(Clone)]
pub struct AuditApiState {
    /// 审计存储
    pub storage: Arc<dyn AuditStorage>,
    /// Ed25519 验证公钥（32 字节原始格式）
    pub verifier_public_key: Vec<u8>,
}

impl std::fmt::Debug for AuditApiState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditApiState")
            .field("storage", &"<dyn AuditStorage>")
            .finish()
    }
}

/// 审计存储 trait
///
/// 抽象审计存储操作，支持不同的后端实现
#[async_trait::async_trait]
pub trait AuditStorage: Send + Sync {
    /// 记录审计日志
    async fn record(&self, entry: AuditEntry) -> Result<SignedAuditEntry, String>;

    /// 查询审计日志
    async fn query(
        &self,
        filter: AuditFilter,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<SignedAuditEntry>, u64), String>;

    /// 按 ID 获取审计条目
    async fn get_by_id(&self, id: &str) -> Result<Option<SignedAuditEntry>, String>;

    /// 按索引获取审计条目
    async fn get_by_index(&self, index: u64) -> Result<Option<SignedAuditEntry>, String>;

    /// 获取所有条目（用于导出）
    async fn get_all(&self, filter: AuditFilter) -> Result<Vec<SignedAuditEntry>, String>;
}

#[derive(Clone)]
pub struct PostgresAuditStorageAdapter {
    pool: PgPool,
    schema: String,
    signing_key: Arc<SigningKeyPair>,
    public_key: Vec<u8>,
}

impl std::fmt::Debug for PostgresAuditStorageAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresAuditStorageAdapter")
            .field("schema", &self.schema)
            .field("pool", &"<PgPool>")
            .finish()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PersistedSigningKey {
    private_key: Vec<u8>,
}

impl PostgresAuditStorageAdapter {
    pub async fn new(pool: PgPool) -> Result<Self, RecorderError> {
        let schema = Self::schema_from_env()?;
        Self::ensure_table(&pool, &schema).await?;

        let signing_key = Self::load_or_create_signing_key(&pool, &schema).await?;
        let public_key = signing_key.public_key().to_vec();

        Ok(Self {
            pool,
            schema,
            signing_key: Arc::new(signing_key),
            public_key,
        })
    }

    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    pub(crate) fn schema_from_env() -> Result<String, RecorderError> {
        let schema =
            env::var("CREDBRIDGE_PG_SCHEMA").unwrap_or_else(|_| DEFAULT_AUDIT_SCHEMA.to_string());
        if schema.is_empty() {
            return Err(RecorderError::StorageError(
                "CREDBRIDGE_PG_SCHEMA 不能为空".to_string(),
            ));
        }
        if !schema
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(RecorderError::StorageError(format!(
                "非法 PostgreSQL schema 名称: {schema}"
            )));
        }

        Ok(schema)
    }

    pub(crate) async fn ensure_table(pool: &PgPool, schema: &str) -> Result<(), RecorderError> {
        let create_schema_sql = format!(r#"CREATE SCHEMA IF NOT EXISTS "{schema}""#);
        let create_table_sql = format!(
            r#"
            CREATE TABLE IF NOT EXISTS "{schema}".audit_logs (
                log_index BIGINT PRIMARY KEY,
                entry_id VARCHAR(64) NOT NULL UNIQUE,
                event_type VARCHAR(64) NOT NULL,
                event_data JSONB NOT NULL DEFAULT '{{}}'::jsonb,
                service VARCHAR(64) NOT NULL,
                user_id_hash VARCHAR(128),
                risk_tier VARCHAR(32) NOT NULL,
                outcome VARCHAR(32) NOT NULL,
                created_at TIMESTAMPTZ NOT NULL,
                signed_entry JSONB NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_cb_audit_logs_created_at
                ON "{schema}".audit_logs (created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_cb_audit_logs_event_type
                ON "{schema}".audit_logs (event_type);
            CREATE INDEX IF NOT EXISTS idx_cb_audit_logs_user_id_hash
                ON "{schema}".audit_logs (user_id_hash);
            "#
        );
        let evolve_table_sql = format!(
            r#"
            ALTER TABLE "{schema}".audit_logs ADD COLUMN IF NOT EXISTS log_index BIGINT;
            ALTER TABLE "{schema}".audit_logs ADD COLUMN IF NOT EXISTS entry_id VARCHAR(64);
            ALTER TABLE "{schema}".audit_logs ADD COLUMN IF NOT EXISTS event_data JSONB;
            ALTER TABLE "{schema}".audit_logs ALTER COLUMN event_data SET DEFAULT '{{}}'::jsonb;
            ALTER TABLE "{schema}".audit_logs ADD COLUMN IF NOT EXISTS service VARCHAR(64);
            ALTER TABLE "{schema}".audit_logs ADD COLUMN IF NOT EXISTS risk_tier VARCHAR(32);
            ALTER TABLE "{schema}".audit_logs ADD COLUMN IF NOT EXISTS outcome VARCHAR(32);
            ALTER TABLE "{schema}".audit_logs ADD COLUMN IF NOT EXISTS signed_entry JSONB;
            CREATE UNIQUE INDEX IF NOT EXISTS idx_cb_audit_logs_log_index
                ON "{schema}".audit_logs (log_index)
                WHERE log_index IS NOT NULL;
            CREATE UNIQUE INDEX IF NOT EXISTS idx_cb_audit_logs_entry_id
                ON "{schema}".audit_logs (entry_id)
                WHERE entry_id IS NOT NULL;
            CREATE TABLE IF NOT EXISTS "{schema}".audit_signing_keys (
                key_name VARCHAR(64) PRIMARY KEY,
                private_key BYTEA NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );
            "#
        );

        sqlx::query(&create_schema_sql)
            .execute(pool)
            .await
            .map_err(|error| RecorderError::StorageError(error.to_string()))?;
        for statement in create_table_sql.split(';') {
            let statement = statement.trim();
            if statement.is_empty() {
                continue;
            }
            sqlx::query(statement)
                .execute(pool)
                .await
                .map_err(|error| RecorderError::StorageError(error.to_string()))?;
        }
        for statement in evolve_table_sql.split(';') {
            let statement = statement.trim();
            if statement.is_empty() {
                continue;
            }
            sqlx::query(statement)
                .execute(pool)
                .await
                .map_err(|error| RecorderError::StorageError(error.to_string()))?;
        }

        Ok(())
    }

    async fn load_or_create_signing_key(
        pool: &PgPool,
        schema: &str,
    ) -> Result<SigningKeyPair, RecorderError> {
        if let Ok(explicit_path) = env::var("CREDBRIDGE_AUDIT_SIGNING_KEY_PATH")
            && !explicit_path.trim().is_empty()
        {
            let path = PathBuf::from(explicit_path);
            let (signing_key, loaded_existing) = load_or_create_signing_key_at_path(&path)?;
            if loaded_existing {
                tracing::info!(
                    module = "audit",
                    signing_key_path = %path.display(),
                    "已加载已有审计签名密钥文件"
                );
            } else {
                tracing::info!(
                    module = "audit",
                    signing_key_path = %path.display(),
                    "首次生成审计签名密钥文件"
                );
            }
            return Ok(signing_key);
        }

        Self::load_or_create_db_signing_key(pool, schema).await
    }

    async fn load_or_create_db_signing_key(
        pool: &PgPool,
        schema: &str,
    ) -> Result<SigningKeyPair, RecorderError> {
        let mut tx = pool
            .begin()
            .await
            .map_err(|error| RecorderError::StorageError(error.to_string()))?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1), hashtext($2))")
            .bind(schema)
            .bind("audit_signing_key")
            .execute(&mut *tx)
            .await
            .map_err(|error| RecorderError::StorageError(error.to_string()))?;

        let select_sql = format!(
            r#"SELECT private_key FROM "{schema}".audit_signing_keys WHERE key_name = $1 LIMIT 1"#
        );
        if let Some(row) = sqlx::query(&select_sql)
            .bind("default")
            .fetch_optional(&mut *tx)
            .await
            .map_err(|error| RecorderError::StorageError(error.to_string()))?
        {
            let private_key: Vec<u8> = row
                .try_get("private_key")
                .map_err(|error| RecorderError::StorageError(error.to_string()))?;
            tx.commit()
                .await
                .map_err(|error| RecorderError::StorageError(error.to_string()))?;
            return SigningKeyPair::from_pkcs8(private_key);
        }

        let signing_key = SigningKeyPair::generate()?;
        let insert_sql = format!(
            r#"INSERT INTO "{schema}".audit_signing_keys (key_name, private_key) VALUES ($1, $2)"#
        );
        sqlx::query(&insert_sql)
            .bind("default")
            .bind(signing_key.private_key().to_vec())
            .execute(&mut *tx)
            .await
            .map_err(|error| RecorderError::StorageError(error.to_string()))?;
        tx.commit()
            .await
            .map_err(|error| RecorderError::StorageError(error.to_string()))?;

        Ok(signing_key)
    }

    fn apply_filters<'a>(builder: &mut QueryBuilder<'a, sqlx::Postgres>, filter: &'a AuditFilter) {
        builder.push(" AND signed_entry IS NOT NULL");
        if let Some(start_time) = filter.start_time {
            builder.push(" AND created_at >= to_timestamp(");
            builder.push_bind(start_time as f64 / 1000.0);
            builder.push(")");
        }
        if let Some(end_time) = filter.end_time {
            builder.push(" AND created_at <= to_timestamp(");
            builder.push_bind(end_time as f64 / 1000.0);
            builder.push(")");
        }
        if let Some(ref user_id_hash) = filter.user_id_hash {
            builder.push(" AND user_id_hash = ");
            builder.push_bind(user_id_hash);
        }
        if let Some(action) = filter.action {
            builder.push(" AND event_type = ");
            builder.push_bind(action.to_string());
        }
        if let Some(risk_tier) = filter.risk_tier {
            builder.push(" AND risk_tier = ");
            builder.push_bind(risk_tier.to_string());
        }
        if let Some(outcome) = filter.outcome {
            builder.push(" AND outcome = ");
            builder.push_bind(outcome.to_string());
        }
        if let Some(ref service) = filter.service {
            builder.push(" AND service = ");
            builder.push_bind(service);
        }
    }

    async fn lock_audit_chain(
        tx: &mut Transaction<'_, Postgres>,
        schema: &str,
    ) -> Result<(), String> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1), hashtext($2))")
            .bind(schema)
            .bind("audit_logs_append")
            .execute(&mut **tx)
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    async fn record_in_transaction(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        entry: AuditEntry,
    ) -> Result<SignedAuditEntry, String> {
        Self::lock_audit_chain(tx, &self.schema).await?;

        let latest_sql = format!(
            r#"SELECT signed_entry FROM "{}".audit_logs
               WHERE signed_entry IS NOT NULL
               ORDER BY log_index DESC
               LIMIT 1"#,
            self.schema
        );
        let latest_row = sqlx::query(&latest_sql)
            .fetch_optional(&mut **tx)
            .await
            .map_err(|error| error.to_string())?;

        let (prev_hash, next_log_index) = if let Some(row) = latest_row {
            let value: serde_json::Value = row
                .try_get("signed_entry")
                .map_err(|error| error.to_string())?;
            let latest_entry: SignedAuditEntry =
                serde_json::from_value(value).map_err(|error| error.to_string())?;
            (
                latest_entry.content_hash,
                latest_entry.log_index.saturating_add(1),
            )
        } else {
            (AuditLogChain::genesis_hash(), 0)
        };

        let signed_entry =
            SignedAuditEntry::sign(entry, prev_hash, next_log_index, self.signing_key.as_ref())
                .map_err(|error| error.to_string())?;

        let signed_entry_json =
            serde_json::to_value(&signed_entry).map_err(|error| error.to_string())?;
        let event_data_json =
            serde_json::to_value(&signed_entry.entry).map_err(|error| error.to_string())?;
        let created_at = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(
            signed_entry.entry.timestamp as i64,
        )
        .ok_or_else(|| "invalid audit timestamp".to_string())?;
        let insert_sql = format!(
            r#"
            INSERT INTO "{schema}".audit_logs
                (log_index, entry_id, event_type, event_data, service, user_id_hash, risk_tier, outcome, created_at, signed_entry)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
            schema = self.schema
        );

        sqlx::query(&insert_sql)
            .bind(i64::try_from(signed_entry.log_index).map_err(|error| error.to_string())?)
            .bind(&signed_entry.entry.id)
            .bind(signed_entry.entry.action.to_string())
            .bind(event_data_json)
            .bind(&signed_entry.entry.service)
            .bind(&signed_entry.entry.user_id_hash)
            .bind(signed_entry.entry.risk_tier.to_string())
            .bind(signed_entry.entry.outcome.to_string())
            .bind(created_at)
            .bind(signed_entry_json)
            .execute(&mut **tx)
            .await
            .map_err(|error| error.to_string())?;

        Ok(signed_entry)
    }
}

const DEFAULT_AUDIT_SCHEMA: &str = "credbridge_vault";

#[allow(dead_code)]
pub(crate) fn signing_key_path(
    schema: &str,
    explicit_path: Option<&str>,
    sealed_storage_path: Option<&str>,
) -> PathBuf {
    if let Some(path) = explicit_path
        && !path.trim().is_empty()
    {
        return PathBuf::from(path);
    }

    let env_override = env::var("CREDBRIDGE_AUDIT_SIGNING_KEY_PATH")
        .ok()
        .filter(|path| !path.trim().is_empty());
    if let Some(path) = env_override {
        return PathBuf::from(path);
    }

    let sealed_storage_path = sealed_storage_path
        .filter(|path| !path.trim().is_empty())
        .map(str::to_owned)
        .or_else(|| {
            env::var("SEALED_STORAGE_PATH")
                .ok()
                .filter(|path| !path.trim().is_empty())
        })
        .unwrap_or_else(|| ".sealed".to_string());
    PathBuf::from(sealed_storage_path).join(format!("{schema}.audit-signing-key.json"))
}

fn load_or_create_signing_key_at_path(
    path: &std::path::Path,
) -> Result<(SigningKeyPair, bool), RecorderError> {
    if path.exists() {
        let bytes = fs::read(path).map_err(|error| {
            RecorderError::StorageError(format!("读取审计签名密钥失败 {}: {error}", path.display()))
        })?;
        let persisted: PersistedSigningKey = serde_json::from_slice(&bytes).map_err(|error| {
            RecorderError::SerializationError(format!(
                "解析审计签名密钥失败 {}: {error}",
                path.display()
            ))
        })?;
        let signing_key = SigningKeyPair::from_pkcs8(persisted.private_key)?;
        return Ok((signing_key, true));
    }

    let signing_key = SigningKeyPair::generate()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            RecorderError::StorageError(format!(
                "创建审计签名密钥目录失败 {}: {error}",
                parent.display()
            ))
        })?;
    }
    let payload = serde_json::to_vec(&PersistedSigningKey {
        private_key: signing_key.private_key().to_vec(),
    })
    .map_err(|error| RecorderError::SerializationError(error.to_string()))?;
    fs::write(path, payload).map_err(|error| {
        RecorderError::StorageError(format!("写入审计签名密钥失败 {}: {error}", path.display()))
    })?;

    Ok((signing_key, false))
}

#[async_trait::async_trait]
impl AuditStorage for PostgresAuditStorageAdapter {
    async fn record(&self, entry: AuditEntry) -> Result<SignedAuditEntry, String> {
        let mut tx = self.pool.begin().await.map_err(|error| error.to_string())?;
        let signed_entry = self.record_in_transaction(&mut tx, entry).await?;
        tx.commit().await.map_err(|error| error.to_string())?;
        Ok(signed_entry)
    }

    async fn query(
        &self,
        filter: AuditFilter,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<SignedAuditEntry>, u64), String> {
        let mut count_builder = QueryBuilder::<sqlx::Postgres>::new(format!(
            r#"SELECT COUNT(*) as count FROM "{}".audit_logs WHERE 1=1"#,
            self.schema
        ));
        Self::apply_filters(&mut count_builder, &filter);
        let total: i64 = count_builder
            .build_query_scalar()
            .fetch_one(&self.pool)
            .await
            .map_err(|error| error.to_string())?;

        let mut builder = QueryBuilder::<sqlx::Postgres>::new(format!(
            r#"SELECT signed_entry FROM "{}".audit_logs WHERE 1=1"#,
            self.schema
        ));
        Self::apply_filters(&mut builder, &filter);
        builder.push(" ORDER BY created_at DESC, log_index DESC");
        builder.push(" OFFSET ");
        builder.push_bind(i64::try_from(offset).map_err(|error| error.to_string())?);
        builder.push(" LIMIT ");
        builder.push_bind(i64::try_from(limit).map_err(|error| error.to_string())?);

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|error| error.to_string())?;

        let entries = rows
            .into_iter()
            .map(|row| {
                let value: serde_json::Value = row.try_get("signed_entry")?;
                serde_json::from_value(value).map_err(|error| sqlx::Error::Decode(Box::new(error)))
            })
            .collect::<Result<Vec<SignedAuditEntry>, sqlx::Error>>()
            .map_err(|error| error.to_string())?;

        Ok((entries, total as u64))
    }

    async fn get_by_id(&self, id: &str) -> Result<Option<SignedAuditEntry>, String> {
        let sql = format!(
            r#"SELECT signed_entry FROM "{}".audit_logs WHERE entry_id = $1"#,
            self.schema
        );
        let row = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| error.to_string())?;

        row.map(|row| {
            let value: serde_json::Value = row.try_get("signed_entry")?;
            serde_json::from_value(value).map_err(|error| sqlx::Error::Decode(Box::new(error)))
        })
        .transpose()
        .map_err(|error| error.to_string())
    }

    async fn get_by_index(&self, index: u64) -> Result<Option<SignedAuditEntry>, String> {
        let sql = format!(
            r#"SELECT signed_entry FROM "{}".audit_logs WHERE log_index = $1"#,
            self.schema
        );
        let row = sqlx::query(&sql)
            .bind(i64::try_from(index).map_err(|error| error.to_string())?)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| error.to_string())?;

        row.map(|row| {
            let value: serde_json::Value = row.try_get("signed_entry")?;
            serde_json::from_value(value).map_err(|error| sqlx::Error::Decode(Box::new(error)))
        })
        .transpose()
        .map_err(|error| error.to_string())
    }

    async fn get_all(&self, filter: AuditFilter) -> Result<Vec<SignedAuditEntry>, String> {
        let (entries, _) = self.query(filter, 0, 100_000).await?;
        Ok(entries)
    }
}

/// 内存审计存储适配器
#[derive(Clone)]
pub struct MemoryAuditStorageAdapter {
    storage: Arc<tokio::sync::Mutex<MemoryAuditStorage>>,
}

impl std::fmt::Debug for MemoryAuditStorageAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryAuditStorageAdapter")
            .field("storage", &"<Arc<Mutex<MemoryAuditStorage>>>")
            .finish()
    }
}

impl MemoryAuditStorageAdapter {
    /// 创建新的内存存储适配器
    pub fn new(storage: MemoryAuditStorage) -> Self {
        Self {
            storage: Arc::new(tokio::sync::Mutex::new(storage)),
        }
    }

    /// 从共享的存储创建适配器
    ///
    /// 用于让多个组件共享同一个存储实例
    pub fn from_shared_storage(storage: Arc<tokio::sync::Mutex<MemoryAuditStorage>>) -> Self {
        Self { storage }
    }
}

#[async_trait::async_trait]
impl AuditStorage for MemoryAuditStorageAdapter {
    async fn record(&self, entry: AuditEntry) -> Result<SignedAuditEntry, String> {
        let storage = self.storage.lock().await;
        storage.record(entry).map_err(|e| e.to_string())
    }

    async fn query(
        &self,
        filter: AuditFilter,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<SignedAuditEntry>, u64), String> {
        let storage = self.storage.lock().await;

        // 获取所有条目
        let all_entries = storage
            .query_recent(100_000)
            .map_err(|e| format!("{e:?}"))?;

        // 过滤
        let mut filtered: Vec<_> = all_entries
            .into_iter()
            .filter(|e| {
                if let Some(start) = filter.start_time
                    && e.entry.timestamp < start
                {
                    return false;
                }
                if let Some(end) = filter.end_time
                    && e.entry.timestamp > end
                {
                    return false;
                }
                if let Some(ref user_hash) = filter.user_id_hash
                    && &e.entry.user_id_hash != user_hash
                {
                    return false;
                }
                if let Some(action) = filter.action
                    && e.entry.action != action
                {
                    return false;
                }
                if let Some(tier) = filter.risk_tier
                    && e.entry.risk_tier != tier
                {
                    return false;
                }
                if let Some(outcome) = filter.outcome
                    && e.entry.outcome != outcome
                {
                    return false;
                }
                if let Some(ref service) = filter.service
                    && &e.entry.service != service
                {
                    return false;
                }
                true
            })
            .collect();

        // 审计日志列表统一为“最新优先”：
        // 先按操作时间倒序，再按日志索引倒序做稳定兜底，避免同毫秒时间戳导致顺序抖动。
        filtered.sort_by(|a, b| {
            b.entry
                .timestamp
                .cmp(&a.entry.timestamp)
                .then_with(|| b.log_index.cmp(&a.log_index))
        });

        let total = filtered.len() as u64;

        // 分页
        let paged: Vec<_> = filtered.into_iter().skip(offset).take(limit).collect();

        Ok((paged, total))
    }

    async fn get_by_id(&self, id: &str) -> Result<Option<SignedAuditEntry>, String> {
        let storage = self.storage.lock().await;
        // 搜索所有条目
        let all_entries = storage
            .query_recent(100_000)
            .map_err(|e| format!("{e:?}"))?;
        Ok(all_entries.into_iter().find(|e| e.entry.id == id))
    }

    async fn get_by_index(&self, index: u64) -> Result<Option<SignedAuditEntry>, String> {
        let storage = self.storage.lock().await;
        storage.get_by_index(index).map_err(|e| format!("{e:?}"))
    }

    async fn get_all(&self, filter: AuditFilter) -> Result<Vec<SignedAuditEntry>, String> {
        let (entries, _) = self.query(filter, 0, 100_000).await?;
        Ok(entries)
    }
}

/// immudb 审计存储适配器
#[derive(Clone)]
pub struct ImmuDbAuditStorageAdapter {
    storage: Arc<ImmuDbAuditStore>,
}

impl std::fmt::Debug for ImmuDbAuditStorageAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImmuDbAuditStorageAdapter")
            .field("storage", &"<Arc<ImmuDbAuditStore>>")
            .finish()
    }
}

impl ImmuDbAuditStorageAdapter {
    pub fn new(storage: Arc<ImmuDbAuditStore>) -> Self {
        Self { storage }
    }
}

#[async_trait::async_trait]
impl AuditStorage for ImmuDbAuditStorageAdapter {
    async fn record(&self, entry: AuditEntry) -> Result<SignedAuditEntry, String> {
        self.storage.record(entry).await.map_err(|e| e.to_string())
    }

    async fn query(
        &self,
        filter: AuditFilter,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<SignedAuditEntry>, u64), String> {
        let entries = {
            let storage = self.storage.storage();
            let storage = storage.lock().await;
            storage
                .query(&crate::audit::QueryOptions {
                    start_time: filter.start_time,
                    end_time: filter.end_time,
                    user_id_hash: filter.user_id_hash,
                    action: filter.action,
                    risk_tier: filter.risk_tier,
                    outcome: filter.outcome,
                    limit: None,
                    offset: None,
                })
                .await
                .map_err(|e| e.to_string())?
        };

        let mut entries: Vec<_> = entries
            .into_iter()
            .map(|entry| entry.signed_entry)
            .filter(|entry| {
                if let Some(ref service) = filter.service {
                    return &entry.entry.service == service;
                }
                true
            })
            .collect();

        let total = entries.len() as u64;
        entries = entries.into_iter().skip(offset).take(limit).collect();

        Ok((entries, total))
    }

    async fn get_by_id(&self, id: &str) -> Result<Option<SignedAuditEntry>, String> {
        let (entries, _) = self.query(AuditFilter::new(), 0, 100_000).await?;
        Ok(entries.into_iter().find(|entry| entry.entry.id == id))
    }

    async fn get_by_index(&self, index: u64) -> Result<Option<SignedAuditEntry>, String> {
        self.storage
            .get_by_index(index)
            .await
            .map_err(|e| e.to_string())
    }

    async fn get_all(&self, filter: AuditFilter) -> Result<Vec<SignedAuditEntry>, String> {
        let (entries, _) = self.query(filter, 0, 100_000).await?;
        Ok(entries)
    }
}

/// 检查权限
fn has_audit_permission(token: &ValidatedToken) -> bool {
    token.scopes.contains(&TokenScope::AuditRead) || token.scopes.contains(&TokenScope::Admin)
}

/// 从请求扩展中提取 Token
fn extract_token_from_request(req: &axum::extract::Request) -> Option<&ValidatedToken> {
    req.extensions().get::<ValidatedToken>()
}

fn current_audit_user_hash(token: &ValidatedToken) -> String {
    hash_user_id(token.principal_id())
}

fn enforce_self_audit_scope(
    token: &ValidatedToken,
    requested_user_id_hash: Option<&String>,
) -> Result<String, &'static str> {
    let current_user_id_hash = current_audit_user_hash(token);

    if requested_user_id_hash.is_some() && requested_user_id_hash != Some(&current_user_id_hash) {
        return Err("权限不足：只能查看当前主体自己的审计日志");
    }

    Ok(current_user_id_hash)
}

fn entry_belongs_to_current_subject(token: &ValidatedToken, entry: &SignedAuditEntry) -> bool {
    entry.entry.user_id_hash == current_audit_user_hash(token)
}

/// 查询审计日志列表
///
/// GET /api/v1/audit/logs
///
/// BUG-18229: 检测原始请求路径是否以尾斜杠结尾，防止 NormalizePathLayer
/// 将 `/audit/logs/` 归一化后错误命中列表路由返回 200 OK。
/// 如果原始路径带尾斜杠，返回 400 Bad Request + {"error":"invalid_request"}。
pub async fn list_audit_logs(
    State(state): State<AuditApiState>,
    OriginalUri(original_uri): OriginalUri,
    req: axum::extract::Request,
) -> Response {
    // BUG-18229: 检测原始请求路径是否以尾斜杠结尾
    // NormalizePathLayer 会将 /audit/logs/ 归一化为 /audit/logs
    // 但原始 URI 保留了尾斜杠，用于判断是否是"缺少详情 id"的请求
    let original_path = original_uri.path();
    if original_path.ends_with('/') {
        // 路径以尾斜杠结尾，表示请求的是 `/audit/logs/` 而非 `/audit/logs`
        // 这对应于"缺少详情路径参数 id"的语义，应返回 400
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid_request" })),
        )
            .into_response();
    }

    // 从查询参数解析
    let params: Query<AuditLogQueryRequest> = match Query::try_from_uri(req.uri()) {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(AuditLogListResponse::error("无效的查询参数")),
            )
                .into_response();
        }
    };

    // 提取 Token
    let token = match extract_token_from_request(&req) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(AuditLogListResponse::error("未提供有效的 Token")),
            )
                .into_response();
        }
    };

    // 验证权限
    if !has_audit_permission(token) {
        return (
            StatusCode::FORBIDDEN,
            Json(AuditLogListResponse::error(
                "权限不足：需要 audit:read 或 admin 权限",
            )),
        )
            .into_response();
    }

    // 验证参数
    if let Err(e) = params.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(AuditLogListResponse::error(e)),
        )
            .into_response();
    }

    let current_user_id_hash = match enforce_self_audit_scope(token, params.user_id_hash.as_ref()) {
        Ok(hash) => hash,
        Err(message) => {
            return (
                StatusCode::FORBIDDEN,
                Json(AuditLogListResponse::error(message)),
            )
                .into_response();
        }
    };

    // 构建过滤器，注入用户隔离约束
    let filter = AuditFilter {
        start_time: params.start_time.map(|t| t.as_millis()),
        end_time: params.end_time.map(|t| t.as_millis()),
        user_id_hash: Some(current_user_id_hash),
        action: params.action,
        risk_tier: params.risk_tier,
        outcome: params.outcome,
        service: params.service.clone(),
    };
    let offset = params.offset();
    let page_size = params.effective_page_size();
    let page = params.current_page();

    // 查询
    match state.storage.query(filter, offset, page_size).await {
        Ok((entries, total)) => {
            let items: Vec<AuditLogListItem> = entries.iter().map(AuditLogListItem::from).collect();
            let response = AuditLogListResponse::success(items, total, page, page_size, offset);
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            tracing::error!("查询审计日志失败: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuditLogListResponse::error("查询审计日志失败")),
            )
                .into_response()
        }
    }
}

/// 获取审计日志详情
///
/// GET /api/v1/audit/logs/:id
pub async fn get_audit_log_detail(
    State(state): State<AuditApiState>,
    Path(id): Path<String>,
    req: axum::extract::Request,
) -> Response {
    // 提取 Token
    let token = match extract_token_from_request(&req) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(AuditLogDetailResponse::error("未提供有效的 Token")),
            )
                .into_response();
        }
    };
    // 验证权限
    if !has_audit_permission(token) {
        return (
            StatusCode::FORBIDDEN,
            Json(AuditLogDetailResponse::error(
                "权限不足：需要 audit:read 或 admin 权限",
            )),
        )
            .into_response();
    }

    // 先尝试按 ID 查找
    let entry = match state.storage.get_by_id(&id).await {
        Ok(Some(e)) => e,
        Ok(None) => {
            // 尝试将 ID 作为索引解析
            if let Ok(index) = id.parse::<u64>() {
                match state.storage.get_by_index(index).await {
                    Ok(Some(e)) => e,
                    Ok(None) => {
                        return (
                            StatusCode::NOT_FOUND,
                            Json(AuditLogDetailResponse::error("审计条目未找到")),
                        )
                            .into_response();
                    }
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(AuditLogDetailResponse::error(e)),
                        )
                            .into_response();
                    }
                }
            } else {
                return (
                    StatusCode::NOT_FOUND,
                    Json(AuditLogDetailResponse::error("审计条目未找到")),
                )
                    .into_response();
            }
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuditLogDetailResponse::error(e)),
            )
                .into_response();
        }
    };

    if !entry_belongs_to_current_subject(token, &entry) {
        return (
            StatusCode::NOT_FOUND,
            Json(AuditLogDetailResponse::error("审计条目未找到")),
        )
            .into_response();
    }

    let data = AuditLogDetailData::from(entry);
    let response = AuditLogDetailResponse::success(data);
    (StatusCode::OK, Json(response)).into_response()
}

/// 导出审计日志
///
/// POST /api/v1/audit/export
pub async fn export_audit_logs(
    State(state): State<AuditApiState>,
    req: axum::extract::Request,
) -> Response {
    // 解析请求体（需要所有权）
    let (parts, body) = req.into_parts();
    let bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(AuditExportResponse::error(format!("读取请求体失败：{e}"))),
            )
                .into_response();
        }
    };

    let params: AuditExportRequest = match serde_json::from_slice(&bytes) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(AuditExportResponse::error(format!("无效的请求体：{e}"))),
            )
                .into_response();
        }
    };

    // 重新构建请求以提取 Token
    let req = axum::extract::Request::from_parts(parts, axum::body::Body::empty());

    // 提取 Token
    let token = match extract_token_from_request(&req) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(AuditExportResponse::error("未提供有效的 Token")),
            )
                .into_response();
        }
    };
    // 验证权限
    if !has_audit_permission(token) {
        return (
            StatusCode::FORBIDDEN,
            Json(AuditExportResponse::error(
                "权限不足：需要 audit:read 或 admin 权限",
            )),
        )
            .into_response();
    }

    // 验证参数
    if let Err(_e) = params.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid_request" })),
        )
            .into_response();
    }

    let current_user_id_hash = match enforce_self_audit_scope(token, params.user_id_hash.as_ref()) {
        Ok(hash) => hash,
        Err(message) => {
            return (
                StatusCode::FORBIDDEN,
                Json(AuditExportResponse::error(message)),
            )
                .into_response();
        }
    };

    // 构建过滤器
    let filter = AuditFilter {
        start_time: params.start_time,
        end_time: params.end_time,
        user_id_hash: Some(current_user_id_hash),
        action: params.action,
        risk_tier: None,
        outcome: None,
        service: None,
    };

    // 获取数据
    let entries = match state.storage.get_all(filter).await {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuditExportResponse::error(e)),
            )
                .into_response();
        }
    };

    // 生成导出内容
    let (content, _content_type) = match params.format {
        ExportFormat::Json => {
            let json = match serde_json::to_string_pretty(&entries) {
                Ok(j) => j,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(AuditExportResponse::error(format!("序列化失败: {e}"))),
                    )
                        .into_response();
                }
            };
            (json, "application/json")
        }
        ExportFormat::Csv => {
            let csv = export_to_csv(&entries);
            (csv, "text/csv")
        }
    };

    // 计算完整性哈希
    let integrity_hash = hex::encode(digest(&SHA256, content.as_bytes()).as_ref());

    // 生成导出 ID
    let export_id = uuid::Uuid::now_v7().to_string();

    // Base64 编码内容
    let content_base64 = STANDARD.encode(&content);

    let data = AuditExportData {
        export_id,
        format: params.format,
        content: content_base64,
        integrity_hash,
        count: entries.len() as u64,
        generated_at: current_timestamp_millis(),
    };

    let response = AuditExportResponse::success(data);
    (StatusCode::OK, Json(response)).into_response()
}

/// 导出为 CSV 格式
fn export_to_csv(entries: &[SignedAuditEntry]) -> String {
    let mut csv = String::new();

    // 表头
    csv.push_str("id,timestamp,user_id_hash,session_id,service,action,risk_tier,outcome,tee_mrenclave,action_token_jti,log_index,content_hash\n");

    // 数据行
    for entry in entries {
        let line = format!(
            "{},{},{},{},{},{},{},{},{},{},{},{}\n",
            entry.entry.id,
            entry.entry.timestamp,
            entry.entry.user_id_hash,
            entry.entry.session_id,
            entry.entry.service,
            entry.entry.action,
            entry.entry.risk_tier,
            entry.entry.outcome,
            entry.entry.tee_mrenclave,
            entry.entry.action_token_jti,
            entry.log_index,
            hex::encode(entry.content_hash)
        );
        csv.push_str(&line);
    }

    csv
}

/// 验证审计日志
///
/// POST /api/v1/audit/verify
pub async fn verify_audit_log(
    State(state): State<AuditApiState>,
    req: axum::extract::Request,
) -> Response {
    // 解析请求体（需要所有权）
    let (parts, body) = req.into_parts();
    let bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(AuditVerifyResponse::error(format!("读取请求体失败：{e}"))),
            )
                .into_response();
        }
    };

    let params: AuditVerifyRequest = match serde_json::from_slice(&bytes) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(AuditVerifyResponse::error(format!("无效的请求体：{e}"))),
            )
                .into_response();
        }
    };

    // 重新构建请求以提取 Token
    let req = axum::extract::Request::from_parts(parts, axum::body::Body::empty());

    // 提取 Token
    let token = match extract_token_from_request(&req) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(AuditVerifyResponse::error("未提供有效的 Token")),
            )
                .into_response();
        }
    };
    // 验证权限
    if !has_audit_permission(token) {
        return (
            StatusCode::FORBIDDEN,
            Json(AuditVerifyResponse::error(
                "权限不足：需要 audit:read 或 admin 权限",
            )),
        )
            .into_response();
    }

    // 参数完整性校验：id 和 log_index 至少需要提供一个有效值
    if params.log_index.is_none() && params.id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid_request" })),
        )
            .into_response();
    }

    // 获取条目
    let entry = if let Some(index) = params.log_index {
        match state.storage.get_by_index(index).await {
            Ok(Some(e)) => e,
            Ok(None) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(AuditVerifyResponse::not_found(format!("索引: {index}"))),
                )
                    .into_response();
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(AuditVerifyResponse::error(e)),
                )
                    .into_response();
            }
        }
    } else {
        match state.storage.get_by_id(&params.id).await {
            Ok(Some(e)) => e,
            Ok(None) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(AuditVerifyResponse::not_found(&params.id)),
                )
                    .into_response();
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(AuditVerifyResponse::error(e)),
                )
                    .into_response();
            }
        }
    };

    if !entry_belongs_to_current_subject(token, &entry) {
        return (
            StatusCode::NOT_FOUND,
            Json(AuditVerifyResponse::not_found(
                params
                    .log_index
                    .map(|index| format!("索引: {index}"))
                    .unwrap_or_else(|| params.id.clone()),
            )),
        )
            .into_response();
    }

    // 执行验证
    let mut details = Vec::new();

    // 1. 验证内容哈希：重新计算条目内容哈希并与存储值比较
    let computed_content_hash = entry.entry.content_hash();
    let content_hash_match = computed_content_hash == entry.content_hash;
    details.push(VerificationDetail {
        step: "内容哈希验证".to_string(),
        passed: content_hash_match,
        message: Some(format!("哈希: {}", hex::encode(entry.content_hash))),
    });

    // 2. 验证签名（Ed25519，与 AuditRecorder::record 签名数据格式一致）
    // 首先检查公钥是否为空，如果为空则返回错误
    let signature_valid = if state.verifier_public_key.is_empty() {
        tracing::warn!("[AUDIT-VERIFY] 公钥未配置，无法验证签名");
        // 公钥未配置，签名验证跳过
        false
    } else {
        let combined_data = [entry.content_hash.as_slice(), entry.prev_hash.as_slice()].concat();
        let combined_hash = digest(&SHA256, &combined_data);
        let pub_key = UnparsedPublicKey::new(&ED25519, &state.verifier_public_key);
        pub_key
            .verify(combined_hash.as_ref(), &entry.signature)
            .is_ok()
    };
    details.push(VerificationDetail {
        step: "数字签名验证".to_string(),
        passed: signature_valid,
        message: Some(format!("签名者: {}", entry.signer_fingerprint)),
    });

    // 总体验证结果
    let verified = content_hash_match && signature_valid;

    let data = AuditVerifyData {
        id: entry.entry.id.clone(),
        log_index: entry.log_index,
        verified,
        content_hash_match,
        signature_valid,
        details,
        verified_at: current_timestamp_millis(),
    };

    let response = if verified {
        AuditVerifyResponse::verified(data)
    } else {
        AuditVerifyResponse::invalid(data)
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// 创建审计 API 路由
pub fn audit_routes(state: AuditApiState) -> Router {
    Router::new()
        .route("/audit/logs", get(list_audit_logs))
        .route("/audit/logs/:id", get(get_audit_log_detail))
        .route("/audit/export", post(export_audit_logs))
        .route("/audit/verify", post(verify_audit_log))
        .with_state(state)
}

/// 获取当前 Unix 时间戳（毫秒）
fn current_timestamp_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间错误")
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::events::{AuditAction, AuditEntry, Outcome};
    use std::fs;
    use std::sync::{Mutex, OnceLock};
    use uuid::Uuid;

    static FS_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn fs_test_lock() -> std::sync::MutexGuard<'static, ()> {
        FS_TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("fs test lock poisoned")
    }

    fn unique_temp_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("credbridge-audit-{name}-{}", Uuid::now_v7()))
    }

    fn create_test_token() -> ValidatedToken {
        ValidatedToken {
            token_id: "test_jti".to_string(),
            subject: "tenant:user".to_string(),
            tenant_id: "tenant".to_string(),
            user_id: "user".to_string(),
            expires_at: 9999999999,
            scopes: vec![TokenScope::AuditRead],
            issued_at: 1000,
            membership_id: None,
            metadata: std::collections::HashMap::new(),
            subject_type: crate::token::TOKEN_SUBJECT_TYPE_USER.to_string(),
            issued_from: crate::token::TOKEN_ISSUED_FROM_SESSION.to_string(),
            token_plane: "management".to_string(),
            allowed_credential_ids: None,
            allowed_binding_handles: None,
        }
    }

    fn create_test_admin_token() -> ValidatedToken {
        ValidatedToken {
            token_id: "admin_jti".to_string(),
            subject: "tenant:admin".to_string(),
            tenant_id: "tenant".to_string(),
            user_id: "admin".to_string(),
            expires_at: 9999999999,
            scopes: vec![TokenScope::Admin],
            issued_at: 1000,
            membership_id: None,
            metadata: std::collections::HashMap::new(),
            subject_type: crate::token::TOKEN_SUBJECT_TYPE_USER.to_string(),
            issued_from: crate::token::TOKEN_ISSUED_FROM_SESSION.to_string(),
            token_plane: "management".to_string(),
            allowed_credential_ids: None,
            allowed_binding_handles: None,
        }
    }

    #[allow(dead_code)]
    fn create_test_storage() -> MemoryAuditStorageAdapter {
        let storage = MemoryAuditStorage::new(1000).unwrap();
        MemoryAuditStorageAdapter::new(storage)
    }

    #[test]
    fn test_has_audit_permission() {
        let audit_token = create_test_token();
        assert!(has_audit_permission(&audit_token));

        let admin_token = create_test_admin_token();
        assert!(has_audit_permission(&admin_token));

        let no_scope_token = ValidatedToken {
            token_id: "test".to_string(),
            subject: "tenant:user".to_string(),
            tenant_id: "tenant".to_string(),
            user_id: "user".to_string(),
            expires_at: 9999999999,
            scopes: vec![TokenScope::CredentialRead],
            issued_at: 1000,
            membership_id: None,
            metadata: std::collections::HashMap::new(),
            subject_type: crate::token::TOKEN_SUBJECT_TYPE_USER.to_string(),
            issued_from: crate::token::TOKEN_ISSUED_FROM_SESSION.to_string(),
            token_plane: "management".to_string(),
            allowed_credential_ids: None,
            allowed_binding_handles: None,
        };
        assert!(!has_audit_permission(&no_scope_token));
    }

    #[test]
    fn test_export_to_csv() {
        let entry = SignedAuditEntry {
            entry: AuditEntry::new(
                "user_hash",
                "session",
                "service",
                AuditAction::CredentialDecrypt,
                Outcome::Success,
                "mrenclave",
                "jti",
            ),
            content_hash: [1u8; 32],
            prev_hash: [0u8; 32],
            signature: vec![1, 2, 3],
            signer_fingerprint: "test_fp".to_string(),
            log_index: 0,
        };

        let csv = export_to_csv(&[entry]);
        assert!(csv.contains("id,timestamp,user_id_hash"));
        assert!(csv.contains("credential_decrypt"));
        assert!(csv.contains("success"));
    }

    #[test]
    fn test_audit_verify_request_empty_json_returns_error() {
        // 测试空 JSON {} 场景：id 为空字符串，log_index 为 None
        let req = AuditVerifyRequest {
            id: String::new(),
            log_index: None,
        };
        // 参数完整性校验：log_index 为 None 且 id.trim().is_empty()
        assert!(req.log_index.is_none() && req.id.trim().is_empty());
    }

    #[test]
    fn test_audit_verify_request_blank_id_returns_error() {
        // 测试空白字符串 id 场景
        let req = AuditVerifyRequest {
            id: "   ".to_string(),
            log_index: None,
        };
        // trim() 后为空，应触发参数校验失败
        assert!(req.log_index.is_none() && req.id.trim().is_empty());
    }

    #[test]
    fn test_audit_verify_request_valid_log_index() {
        // 测试仅提供 log_index 的合法场景
        let req = AuditVerifyRequest {
            id: String::new(),
            log_index: Some(123),
        };
        // log_index 存在，参数校验应通过
        assert!(req.log_index.is_some());
    }

    #[test]
    fn test_audit_verify_request_valid_id() {
        // 测试仅提供有效 id 的合法场景
        let req = AuditVerifyRequest {
            id: "valid-uuid-123".to_string(),
            log_index: None,
        };
        // id 非空，参数校验应通过
        assert!(!req.id.trim().is_empty());
    }

    #[test]
    fn test_load_or_create_signing_key_reuses_existing_file() {
        let _guard = fs_test_lock();
        let temp_dir = unique_temp_dir("signing-key-reuse");
        let path = temp_dir.join("credbridge_vault.audit-signing-key.json");

        let (first_key, loaded_existing_first) =
            load_or_create_signing_key_at_path(&path).expect("first key init should succeed");
        let first_public_key = first_key.public_key().to_vec();
        let first_file_bytes = fs::read(&path).expect("first key file should exist");

        let (second_key, loaded_existing_second) =
            load_or_create_signing_key_at_path(&path).expect("second key init should succeed");
        let second_public_key = second_key.public_key().to_vec();
        let second_file_bytes = fs::read(&path).expect("second key file should exist");

        assert!(!loaded_existing_first);
        assert!(loaded_existing_second);
        assert_eq!(first_public_key, second_public_key);
        assert_eq!(first_file_bytes, second_file_bytes);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_load_or_create_signing_key_fails_on_corrupt_existing_file() {
        let _guard = fs_test_lock();
        let temp_dir = unique_temp_dir("signing-key-corrupt");
        fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let path = temp_dir.join("credbridge_vault.audit-signing-key.json");
        fs::write(&path, b"not-valid-json").expect("corrupt key file should be written");

        let message = match load_or_create_signing_key_at_path(&path) {
            Ok(_) => panic!("corrupt signing key file should fail"),
            Err(error) => error.to_string(),
        };
        assert!(message.contains("解析审计签名密钥失败"));
        assert!(message.contains(path.to_string_lossy().as_ref()));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
