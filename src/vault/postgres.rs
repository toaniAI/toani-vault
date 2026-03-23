//! PostgreSQL 存储后端实现
//!
//! 为凭证创建、查询、更新和版本历史提供持久化存储。

use super::models::*;
use super::storage::StorageBackend;
use super::version::CredentialVersion;
use crate::models::{CredentialMetadata, CredentialType};
use crate::services::db::pool::DatabaseError;
use crate::services::db::{DatabaseConfig, DatabasePool};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::Row;
use std::env;
use tokio::runtime::Handle;

const DEFAULT_SCHEMA: &str = "credbridge_vault";

#[derive(Debug, thiserror::Error)]
pub enum PostgresBackendError {
    #[error("数据库错误: {0}")]
    Database(#[from] DatabaseError),

    #[error("配置错误: {0}")]
    Config(String),
}

pub struct PostgresStorageBackend {
    db: DatabasePool,
    schema: String,
    runtime_handle: Handle,
}

impl PostgresStorageBackend {
    pub async fn new(config: DatabaseConfig) -> Result<Self, PostgresBackendError> {
        let db = DatabasePool::new(config).await?;
        let schema = Self::schema_from_env()?;
        let backend = Self {
            db,
            schema,
            runtime_handle: Handle::current(),
        };
        backend.ensure_schema().await?;
        Ok(backend)
    }

    pub async fn from_env() -> Result<Self, PostgresBackendError> {
        Self::new(DatabaseConfig::from_env()?).await
    }

    pub fn schema(&self) -> &str {
        &self.schema
    }

    fn schema_from_env() -> Result<String, PostgresBackendError> {
        let schema =
            env::var("CREDBRIDGE_PG_SCHEMA").unwrap_or_else(|_| DEFAULT_SCHEMA.to_string());
        if schema.is_empty() {
            return Err(PostgresBackendError::Config(
                "CREDBRIDGE_PG_SCHEMA 不能为空".to_string(),
            ));
        }
        if !schema
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(PostgresBackendError::Config(format!(
                "非法 PostgreSQL schema 名称: {schema}"
            )));
        }
        Ok(schema)
    }

    async fn ensure_schema(&self) -> Result<(), DatabaseError> {
        let create_schema_sql = format!("CREATE SCHEMA IF NOT EXISTS {}", self.schema);
        let credentials_sql = format!(
            r#"
            CREATE TABLE IF NOT EXISTS {schema}.credentials (
                credential_id VARCHAR(64) PRIMARY KEY,
                tenant_id VARCHAR(128) NOT NULL,
                user_id_hash VARCHAR(128) NOT NULL,
                service_id VARCHAR(64) NOT NULL,
                credential_type VARCHAR(32) NOT NULL,
                encrypted_payload JSONB NOT NULL,
                version INTEGER NOT NULL DEFAULT 1,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                expires_at TIMESTAMPTZ,
                is_deleted BOOLEAN NOT NULL DEFAULT FALSE,
                CONSTRAINT cb_credentials_version_check CHECK (version >= 1)
            );
            CREATE INDEX IF NOT EXISTS idx_cb_credentials_lookup
                ON {schema}.credentials (tenant_id, user_id_hash, is_deleted, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_cb_credentials_service
                ON {schema}.credentials (tenant_id, user_id_hash, service_id);
            CREATE INDEX IF NOT EXISTS idx_cb_credentials_type
                ON {schema}.credentials (tenant_id, user_id_hash, credential_type);
            "#,
            schema = self.schema
        );
        let versions_sql = format!(
            r#"
            CREATE TABLE IF NOT EXISTS {schema}.credential_versions (
                id BIGSERIAL PRIMARY KEY,
                credential_id VARCHAR(64) NOT NULL REFERENCES {schema}.credentials(credential_id) ON DELETE CASCADE,
                version INTEGER NOT NULL,
                encrypted_payload JSONB NOT NULL,
                change_reason TEXT,
                changed_by VARCHAR(128),
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                CONSTRAINT cb_unique_credential_version UNIQUE (credential_id, version),
                CONSTRAINT cb_credential_versions_version_check CHECK (version >= 1)
            );
            CREATE INDEX IF NOT EXISTS idx_cb_credential_versions_lookup
                ON {schema}.credential_versions (credential_id, version DESC);
            "#,
            schema = self.schema
        );

        sqlx::query(&create_schema_sql)
            .execute(self.db.pool())
            .await
            .map_err(|e| DatabaseError::SchemaError(e.to_string()))?;

        sqlx::query(&credentials_sql)
            .execute(self.db.pool())
            .await
            .map_err(|e| DatabaseError::SchemaError(e.to_string()))?;

        sqlx::query(&versions_sql)
            .execute(self.db.pool())
            .await
            .map_err(|e| DatabaseError::SchemaError(e.to_string()))?;

        Ok(())
    }

    fn block_on<T, F>(&self, future: F) -> Result<T, DatabaseError>
    where
        F: std::future::Future<Output = Result<T, DatabaseError>>,
    {
        tokio::task::block_in_place(|| self.runtime_handle.block_on(future))
    }

    fn payload_to_json(payload: &EncryptedPayload) -> Result<Value, VaultError> {
        serde_json::to_value(payload)
            .map_err(|e| VaultError::SerializationError(format!("payload serialize failed: {e}")))
    }

    fn payload_from_json(value: Value) -> Result<EncryptedPayload, VaultError> {
        serde_json::from_value(value)
            .map_err(|e| VaultError::SerializationError(format!("payload deserialize failed: {e}")))
    }

    fn credential_type_from_str(value: &str) -> CredentialType {
        match value {
            "username_password" => CredentialType::UsernamePassword,
            "oauth_refresh" => CredentialType::OAuthRefresh,
            "api_key" => CredentialType::ApiKey,
            "session_cookie" => CredentialType::SessionCookie,
            "kyc_document" => CredentialType::KycDocument,
            _ => CredentialType::ApiKey,
        }
    }

    fn entry_from_row(row: &sqlx::postgres::PgRow) -> Result<VaultEntry, VaultError> {
        let encrypted_payload =
            Self::payload_from_json(row.try_get("encrypted_payload").map_err(|e| {
                VaultError::StorageError(format!("读取 encrypted_payload 失败: {e}"))
            })?)?;
        let credential_type = row
            .try_get::<String, _>("credential_type")
            .map_err(|e| VaultError::StorageError(format!("读取 credential_type 失败: {e}")))?;
        let created_at: DateTime<Utc> = row
            .try_get("created_at")
            .map_err(|e| VaultError::StorageError(format!("读取 created_at 失败: {e}")))?;
        let updated_at: DateTime<Utc> = row
            .try_get("updated_at")
            .map_err(|e| VaultError::StorageError(format!("读取 updated_at 失败: {e}")))?;
        let expires_at: Option<DateTime<Utc>> = row
            .try_get("expires_at")
            .map_err(|e| VaultError::StorageError(format!("读取 expires_at 失败: {e}")))?;

        Ok(VaultEntry {
            credential_id: CredentialId::from_string(
                row.try_get("credential_id").map_err(|e| {
                    VaultError::StorageError(format!("读取 credential_id 失败: {e}"))
                })?,
            )?,
            version: row
                .try_get::<i32, _>("version")
                .map_err(|e| VaultError::StorageError(format!("读取 version 失败: {e}")))?
                as u32,
            tenant_id: TenantId::new(
                row.try_get::<String, _>("tenant_id")
                    .map_err(|e| VaultError::StorageError(format!("读取 tenant_id 失败: {e}")))?,
            ),
            user_id: UserId::from_hash(
                row.try_get::<String, _>("user_id_hash").map_err(|e| {
                    VaultError::StorageError(format!("读取 user_id_hash 失败: {e}"))
                })?,
            ),
            service_id: ServiceId::new(
                row.try_get::<String, _>("service_id")
                    .map_err(|e| VaultError::StorageError(format!("读取 service_id 失败: {e}")))?,
            ),
            credential_type: Self::credential_type_from_str(&credential_type),
            created_at: created_at.timestamp() as u64,
            updated_at: updated_at.timestamp() as u64,
            expires_at: expires_at.map(|ts| ts.timestamp() as u64),
            encrypted_payload,
            is_deleted: row
                .try_get("is_deleted")
                .map_err(|e| VaultError::StorageError(format!("读取 is_deleted 失败: {e}")))?,
        })
    }

    fn version_from_row(row: &sqlx::postgres::PgRow) -> Result<CredentialVersion, VaultError> {
        let encrypted_payload = Self::payload_from_json(
            row.try_get("encrypted_payload")
                .map_err(|e| VaultError::StorageError(format!("读取版本 payload 失败: {e}")))?,
        )?;
        let created_at: DateTime<Utc> = row
            .try_get("created_at")
            .map_err(|e| VaultError::StorageError(format!("读取版本 created_at 失败: {e}")))?;

        Ok(CredentialVersion {
            id: uuid::Uuid::now_v7(),
            credential_id: row.try_get("credential_id").map_err(|e| {
                VaultError::StorageError(format!("读取版本 credential_id 失败: {e}"))
            })?,
            version: row
                .try_get::<i32, _>("version")
                .map_err(|e| VaultError::StorageError(format!("读取版本号失败: {e}")))?
                as u32,
            encrypted_payload,
            change_reason: row
                .try_get("change_reason")
                .map_err(|e| VaultError::StorageError(format!("读取 change_reason 失败: {e}")))?,
            changed_by: row
                .try_get("changed_by")
                .map_err(|e| VaultError::StorageError(format!("读取 changed_by 失败: {e}")))?,
            created_at,
        })
    }

    fn timestamp_from_secs(seconds: u64) -> Result<DateTime<Utc>, VaultError> {
        DateTime::<Utc>::from_timestamp(seconds as i64, 0)
            .ok_or_else(|| VaultError::SerializationError(format!("无效时间戳: {seconds}")))
    }
}

impl StorageBackend for PostgresStorageBackend {
    fn store(&self, entry: &VaultEntry) -> Result<(), VaultError> {
        let sql = format!(
            r#"
            INSERT INTO {schema}.credentials (
                credential_id, tenant_id, user_id_hash, service_id, credential_type,
                encrypted_payload, version, created_at, updated_at, expires_at, is_deleted
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (credential_id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                user_id_hash = EXCLUDED.user_id_hash,
                service_id = EXCLUDED.service_id,
                credential_type = EXCLUDED.credential_type,
                encrypted_payload = EXCLUDED.encrypted_payload,
                version = EXCLUDED.version,
                created_at = EXCLUDED.created_at,
                updated_at = EXCLUDED.updated_at,
                expires_at = EXCLUDED.expires_at,
                is_deleted = EXCLUDED.is_deleted
            "#,
            schema = self.schema
        );
        let payload = Self::payload_to_json(&entry.encrypted_payload)?;
        let created_at = Self::timestamp_from_secs(entry.created_at)?;
        let updated_at = Self::timestamp_from_secs(entry.updated_at)?;
        let expires_at = entry
            .expires_at
            .map(Self::timestamp_from_secs)
            .transpose()?;

        self.block_on(async {
            sqlx::query(&sql)
                .bind(entry.credential_id.as_str())
                .bind(entry.tenant_id.as_str())
                .bind(entry.user_id.hash())
                .bind(entry.service_id.as_str())
                .bind(entry.credential_type.as_str())
                .bind(payload)
                .bind(entry.version as i32)
                .bind(created_at)
                .bind(updated_at)
                .bind(expires_at)
                .bind(entry.is_deleted)
                .execute(self.db.pool())
                .await
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
            Ok(())
        })
        .map_err(|e| VaultError::StorageError(e.to_string()))
    }

    fn get(&self, credential_id: &CredentialId) -> Result<Option<VaultEntry>, VaultError> {
        let sql = format!(
            "SELECT * FROM {}.credentials WHERE credential_id = $1",
            self.schema
        );
        self.block_on(async {
            let row = sqlx::query(&sql)
                .bind(credential_id.as_str())
                .fetch_optional(self.db.pool())
                .await
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
            Ok(row)
        })
        .map_err(|e| VaultError::StorageError(e.to_string()))?
        .map(|row| Self::entry_from_row(&row))
        .transpose()
    }

    fn get_metadata(
        &self,
        credential_id: &CredentialId,
    ) -> Result<Option<CredentialMetadata>, VaultError> {
        self.get(credential_id)
            .map(|entry| entry.map(|e| e.metadata()))
    }

    fn query(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        filter: &CredentialFilter,
    ) -> Result<CredentialQueryResult, VaultError> {
        let sql = format!(
            r#"
            SELECT * FROM {schema}.credentials
            WHERE tenant_id = $1
              AND user_id_hash = $2
              AND ($3 OR is_deleted = FALSE)
              AND (NOT $4 OR expires_at IS NULL OR expires_at > NOW())
              AND ($5::VARCHAR IS NULL OR service_id = $5)
              AND ($6::VARCHAR IS NULL OR credential_type = $6)
            ORDER BY created_at DESC
            "#,
            schema = self.schema
        );

        let service_id = filter.service_id.as_ref().map(|id| id.as_str().to_string());
        let credential_type = filter
            .credential_type
            .as_ref()
            .map(|kind| kind.as_str().to_string());

        let rows = self
            .block_on(async {
                sqlx::query(&sql)
                    .bind(tenant_id.as_str())
                    .bind(user_id.hash())
                    .bind(filter.include_deleted)
                    .bind(filter.only_valid)
                    .bind(service_id)
                    .bind(credential_type)
                    .fetch_all(self.db.pool())
                    .await
                    .map_err(|e| DatabaseError::QueryFailed(e.to_string()))
            })
            .map_err(|e| VaultError::StorageError(e.to_string()))?;

        let mut credentials = Vec::with_capacity(rows.len());
        for row in rows {
            credentials.push(Self::entry_from_row(&row)?.metadata());
        }
        let total = credentials.len();

        Ok(CredentialQueryResult { credentials, total })
    }

    fn delete(&self, credential_id: &CredentialId) -> Result<bool, VaultError> {
        let sql = format!(
            "UPDATE {}.credentials SET is_deleted = TRUE, updated_at = NOW() WHERE credential_id = $1",
            self.schema
        );
        let affected = self
            .block_on(async {
                let result = sqlx::query(&sql)
                    .bind(credential_id.as_str())
                    .execute(self.db.pool())
                    .await
                    .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
                Ok(result.rows_affected())
            })
            .map_err(|e| VaultError::StorageError(e.to_string()))?;
        Ok(affected > 0)
    }

    fn purge(&self, credential_id: &CredentialId) -> Result<bool, VaultError> {
        let sql = format!(
            "DELETE FROM {}.credentials WHERE credential_id = $1",
            self.schema
        );
        let affected = self
            .block_on(async {
                let result = sqlx::query(&sql)
                    .bind(credential_id.as_str())
                    .execute(self.db.pool())
                    .await
                    .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
                Ok(result.rows_affected())
            })
            .map_err(|e| VaultError::StorageError(e.to_string()))?;
        Ok(affected > 0)
    }

    fn exists(&self, credential_id: &CredentialId) -> Result<bool, VaultError> {
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM {}.credentials WHERE credential_id = $1)",
            self.schema
        );
        self.block_on(async {
            sqlx::query_scalar(&sql)
                .bind(credential_id.as_str())
                .fetch_one(self.db.pool())
                .await
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))
        })
        .map_err(|e| VaultError::StorageError(e.to_string()))
    }

    fn update(&self, entry: &VaultEntry) -> Result<(), VaultError> {
        let sql = format!(
            r#"
            UPDATE {schema}.credentials
            SET tenant_id = $2,
                user_id_hash = $3,
                service_id = $4,
                credential_type = $5,
                encrypted_payload = $6,
                version = $7,
                updated_at = $8,
                expires_at = $9,
                is_deleted = $10
            WHERE credential_id = $1
            "#,
            schema = self.schema
        );
        let payload = Self::payload_to_json(&entry.encrypted_payload)?;
        let updated_at = Self::timestamp_from_secs(entry.updated_at)?;
        let expires_at = entry
            .expires_at
            .map(Self::timestamp_from_secs)
            .transpose()?;

        let affected = self
            .block_on(async {
                let result = sqlx::query(&sql)
                    .bind(entry.credential_id.as_str())
                    .bind(entry.tenant_id.as_str())
                    .bind(entry.user_id.hash())
                    .bind(entry.service_id.as_str())
                    .bind(entry.credential_type.as_str())
                    .bind(payload)
                    .bind(entry.version as i32)
                    .bind(updated_at)
                    .bind(expires_at)
                    .bind(entry.is_deleted)
                    .execute(self.db.pool())
                    .await
                    .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
                Ok(result.rows_affected())
            })
            .map_err(|e| VaultError::StorageError(e.to_string()))?;

        if affected == 0 {
            return Err(VaultError::CredentialNotFound(
                entry.credential_id.as_str().to_string(),
            ));
        }

        Ok(())
    }

    fn create_version_record(&self, version: &CredentialVersion) -> Result<(), VaultError> {
        let sql = format!(
            r#"
            INSERT INTO {schema}.credential_versions (
                credential_id, version, encrypted_payload, change_reason, changed_by, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (credential_id, version) DO NOTHING
            "#,
            schema = self.schema
        );
        let payload = Self::payload_to_json(&version.encrypted_payload)?;

        self.block_on(async {
            sqlx::query(&sql)
                .bind(&version.credential_id)
                .bind(version.version as i32)
                .bind(payload)
                .bind(&version.change_reason)
                .bind(&version.changed_by)
                .bind(version.created_at)
                .execute(self.db.pool())
                .await
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
            Ok(())
        })
        .map_err(|e| VaultError::StorageError(e.to_string()))
    }

    fn get_version_history(
        &self,
        credential_id: &CredentialId,
    ) -> Result<Vec<CredentialVersion>, VaultError> {
        let sql = format!(
            r#"
            SELECT credential_id, version, encrypted_payload, change_reason, changed_by, created_at
            FROM {schema}.credential_versions
            WHERE credential_id = $1
            ORDER BY version DESC
            "#,
            schema = self.schema
        );
        let rows = self
            .block_on(async {
                sqlx::query(&sql)
                    .bind(credential_id.as_str())
                    .fetch_all(self.db.pool())
                    .await
                    .map_err(|e| DatabaseError::QueryFailed(e.to_string()))
            })
            .map_err(|e| VaultError::StorageError(e.to_string()))?;

        rows.iter().map(Self::version_from_row).collect()
    }

    fn get_version(
        &self,
        credential_id: &CredentialId,
        version: u32,
    ) -> Result<Option<CredentialVersion>, VaultError> {
        let sql = format!(
            r#"
            SELECT credential_id, version, encrypted_payload, change_reason, changed_by, created_at
            FROM {schema}.credential_versions
            WHERE credential_id = $1 AND version = $2
            LIMIT 1
            "#,
            schema = self.schema
        );
        self.block_on(async {
            let row = sqlx::query(&sql)
                .bind(credential_id.as_str())
                .bind(version as i32)
                .fetch_optional(self.db.pool())
                .await
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
            Ok(row)
        })
        .map_err(|e| VaultError::StorageError(e.to_string()))?
        .map(|row| Self::version_from_row(&row))
        .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::constants;

    fn test_payload() -> EncryptedPayload {
        EncryptedPayload::new(
            constants::PROTOCOL_VERSION,
            constants::ALGORITHM_AES_256_GCM,
            constants::KDF_HKDF_SHA256,
            vec![0u8; constants::NONCE_LENGTH],
            vec![0u8; constants::AUTH_TAG_LENGTH],
            vec![1, 2, 3, 4],
        )
    }

    #[test]
    fn schema_name_validation_rejects_invalid_chars() {
        unsafe {
            env::set_var("CREDBRIDGE_PG_SCHEMA", "bad-schema");
        }
        let result = PostgresStorageBackend::schema_from_env();
        assert!(result.is_err());
        unsafe {
            env::remove_var("CREDBRIDGE_PG_SCHEMA");
        }
    }

    #[test]
    fn payload_round_trip() {
        let payload = test_payload();
        let json = PostgresStorageBackend::payload_to_json(&payload).unwrap();
        let restored = PostgresStorageBackend::payload_from_json(json).unwrap();
        assert_eq!(payload.algorithm, restored.algorithm);
        assert_eq!(payload.ciphertext, restored.ciphertext);
    }
}
