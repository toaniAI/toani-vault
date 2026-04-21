//! 启动期数据库表完整性检查与自修复。
//!
//! 除表名存在性外，还会检查关键迁移列是否已经到位；发现缺表或缺关键列时，
//! 按 schema 类型执行对应的补建逻辑。

use std::collections::HashSet;

use sqlx::{PgPool, Row};

use crate::api::audit::PostgresAuditStorageAdapter;
use crate::services::db::pool::{DatabaseError, execute_pg_raw_sql_conn};
use crate::services::db::{DatabasePool, SchemaManager};
use crate::vault::postgres::PostgresStorageBackend;

const PUBLIC_REQUIRED_TABLES: &[&str] = &[
    "tenants",
    "users",
    "external_identities",
    "tenant_memberships",
    "tenant_invitations",
    "auth_sessions",
    "auth_audit_logs",
    "credentials",
    "scope_tokens",
    "audit_logs",
    "tenant_roles",
    "user_roles",
    "credential_versions",
    "sandbox_sessions",
    "sandbox_operations",
    "service_accounts",
    "api_tokens",
];

const TENANT_REQUIRED_TABLES: &[&str] = &[
    "credentials",
    "scope_tokens",
    "audit_logs",
    "tenant_roles",
    "user_roles",
];

const FIXED_SCHEMA_REQUIRED_TABLES: &[&str] = &["credentials", "credential_versions", "audit_logs"];

const PUBLIC_VERSIONING_AND_AUDIT_SQL: &str = r#"
BEGIN;

ALTER TABLE public.credentials
    ADD COLUMN IF NOT EXISTS version INTEGER NOT NULL DEFAULT 1;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint c
        JOIN pg_class t ON t.oid = c.conrelid
        JOIN pg_namespace n ON n.oid = t.relnamespace
        WHERE c.conname = 'credentials_version_check'
          AND t.relname = 'credentials'
          AND n.nspname = 'public'
    ) THEN
        ALTER TABLE public.credentials
        ADD CONSTRAINT credentials_version_check CHECK (version >= 1);
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS public.credential_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    credential_id VARCHAR(64) NOT NULL,
    version INTEGER NOT NULL,
    encrypted_payload JSONB NOT NULL,
    change_reason TEXT,
    changed_by UUID,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    CONSTRAINT unique_credential_version UNIQUE (credential_id, version),
    CONSTRAINT credential_versions_version_check CHECK (version >= 1),
    CONSTRAINT fk_credential_versions_credential
        FOREIGN KEY (credential_id) REFERENCES public.credentials(credential_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_credentials_version
    ON public.credentials(version);
CREATE INDEX IF NOT EXISTS idx_credential_versions_credential_id
    ON public.credential_versions(credential_id);
CREATE INDEX IF NOT EXISTS idx_credential_versions_created_at
    ON public.credential_versions(created_at);
CREATE INDEX IF NOT EXISTS idx_credential_versions_changed_by
    ON public.credential_versions(changed_by);
CREATE INDEX IF NOT EXISTS idx_credential_versions_lookup
    ON public.credential_versions(credential_id, version DESC);

ALTER TABLE public.audit_logs
    ADD COLUMN IF NOT EXISTS event_category VARCHAR(32);
ALTER TABLE public.audit_logs
    ADD COLUMN IF NOT EXISTS metadata JSONB;

CREATE INDEX IF NOT EXISTS idx_audit_logs_category
    ON public.audit_logs(event_category);
CREATE INDEX IF NOT EXISTS idx_audit_logs_metadata
    ON public.audit_logs USING GIN (metadata);

COMMIT;
"#;

const PUBLIC_COMPATIBILITY_VIEW_RESET_SQL: &str = r#"
DROP VIEW IF EXISTS public.sandbox_active_sessions;
DROP VIEW IF EXISTS public.sandbox_session_stats;
"#;

const PUBLIC_SANDBOX_VIEW_REFRESH_SQL: &str = r#"
DROP VIEW IF EXISTS public.sandbox_active_sessions;
DROP VIEW IF EXISTS public.sandbox_session_stats;

CREATE VIEW public.sandbox_active_sessions AS
SELECT
    s.id,
    s.tenant_id,
    s.created_by,
    s.status,
    s.started_at,
    s.expires_at,
    s.terminated_at,
    s.termination_reason,
    s.tee_context_id,
    s.security_policy,
    s.metadata,
    s.created_at,
    s.updated_at,
    s.credential_id,
    s.original_intent,
    s.active_operation_id,
    s.active_operation_started_at,
    s.last_error_summary,
    COALESCE(stats.operation_count, 0) AS operation_count,
    stats.last_operation_at
FROM public.sandbox_sessions s
LEFT JOIN (
    SELECT
        o.session_id,
        COUNT(o.id) AS operation_count,
        MAX(o.started_at) AS last_operation_at
    FROM public.sandbox_operations o
    GROUP BY o.session_id
) stats ON stats.session_id = s.id
WHERE s.status = 'active';

COMMENT ON VIEW public.sandbox_active_sessions IS '活跃沙箱会话视图 - 包含操作统计信息';

CREATE VIEW public.sandbox_session_stats AS
SELECT
    s.id AS session_id,
    s.tenant_id,
    s.status,
    COUNT(o.id) AS total_operations,
    COUNT(o.id) FILTER (WHERE o.status = 'completed') AS completed_operations,
    COUNT(o.id) FILTER (WHERE o.status = 'failed') AS failed_operations,
    AVG(o.execution_duration_ms) FILTER (WHERE o.status = 'completed') AS avg_execution_time_ms,
    MAX(o.started_at) AS last_operation_at
FROM public.sandbox_sessions s
LEFT JOIN public.sandbox_operations o ON s.id = o.session_id
GROUP BY s.id, s.tenant_id, s.status;

COMMENT ON VIEW public.sandbox_session_stats IS '沙箱会话统计视图 - 包含操作统计信息';
"#;

const PUBLIC_BASELINE_SCRIPTS: &[&str] = &[
    include_str!("../../../scripts/init-database.sql"),
    PUBLIC_VERSIONING_AND_AUDIT_SQL,
    PUBLIC_COMPATIBILITY_VIEW_RESET_SQL,
    include_str!("../../../migrations/20260317000001_create_sandbox_tables.sql"),
    include_str!("../../../migrations/20260403093000_add_sandbox_session_identity_columns.sql"),
    include_str!("../../../migrations/20260409143000_add_service_accounts_and_api_tokens.sql"),
    include_str!("../../../migrations/20260410110000_extend_api_tokens_for_automation_tokens.sql"),
    include_str!("../../../migrations/20260413102000_add_api_token_credential_ids.sql"),
    include_str!(
        "../../../migrations/20260421090000_align_sandbox_session_status_and_diagnostics.sql"
    ),
    PUBLIC_SANDBOX_VIEW_REFRESH_SQL,
];

const PUBLIC_REQUIRED_COLUMNS: &[(&str, &[&str])] = &[
    ("credentials", &["version"]),
    ("audit_logs", &["event_category", "metadata"]),
    (
        "sandbox_sessions",
        &[
            "credential_id",
            "original_intent",
            "active_operation_id",
            "active_operation_started_at",
            "last_error_summary",
        ],
    ),
    (
        "api_tokens",
        &[
            "token_kind",
            "token_name",
            "token_prefix",
            "description",
            "issued_membership_role_snapshot",
            "permission_source",
            "created_via",
            "revoked_reason",
            "oauth_client_id",
            "oauth_grant_type",
            "oauth_subject_mode",
            "credential_ids",
        ],
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequiredSchemaKind {
    Public,
    Fixed,
    Tenant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequiredSchemaSet {
    pub(crate) schema_name: String,
    pub(crate) required_tables: &'static [&'static str],
    kind: RequiredSchemaKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MissingTableReport {
    pub(crate) schema_name: String,
    pub(crate) missing_tables: Vec<String>,
    pub(crate) missing_columns: Vec<String>,
    kind: RequiredSchemaKind,
}

pub async fn ensure_required_tables_on_startup(db: &DatabasePool) -> Result<(), DatabaseError> {
    let fixed_schema = PostgresStorageBackend::schema_from_env()
        .map_err(|error| DatabaseError::ConfigError(error.to_string()))?;
    ensure_required_tables_on_startup_with_schema(db, &fixed_schema).await
}

pub(crate) async fn ensure_required_tables_on_startup_with_schema(
    db: &DatabasePool,
    fixed_schema: &str,
) -> Result<(), DatabaseError> {
    let required_sets = load_required_schema_sets(db.pool(), fixed_schema).await?;
    let reports = collect_missing_tables(db.pool(), &required_sets).await?;

    if reports.is_empty() {
        tracing::info!(
            module = "database_bootstrap",
            status = "verified",
            checked_schemas = required_sets.len(),
            "数据库表完整性检查通过"
        );
        return Ok(());
    }

    for report in &reports {
        tracing::warn!(
            module = "database_bootstrap",
            schema = %report.schema_name,
            missing_tables = ?report.missing_tables,
            missing_columns = ?report.missing_columns,
            "检测到缺失表或关键迁移列，开始补建"
        );
        repair_missing_tables(db, report).await?;
    }

    tracing::info!(
        module = "database_bootstrap",
        status = "repaired",
        repaired_schemas = reports.len(),
        "数据库表完整性检查已完成并修复缺表"
    );
    Ok(())
}

async fn load_required_schema_sets(
    pool: &PgPool,
    fixed_schema: &str,
) -> Result<Vec<RequiredSchemaSet>, DatabaseError> {
    let tenant_schemas = list_existing_tenant_schemas(pool).await?;
    let mut sets = Vec::with_capacity(tenant_schemas.len() + 2);
    sets.push(RequiredSchemaSet {
        schema_name: "public".to_string(),
        required_tables: PUBLIC_REQUIRED_TABLES,
        kind: RequiredSchemaKind::Public,
    });

    if fixed_schema != "public" {
        sets.push(RequiredSchemaSet {
            schema_name: fixed_schema.to_string(),
            required_tables: FIXED_SCHEMA_REQUIRED_TABLES,
            kind: RequiredSchemaKind::Fixed,
        });
    }

    for schema_name in tenant_schemas {
        sets.push(RequiredSchemaSet {
            schema_name,
            required_tables: TENANT_REQUIRED_TABLES,
            kind: RequiredSchemaKind::Tenant,
        });
    }

    Ok(sets)
}

async fn collect_missing_tables(
    pool: &PgPool,
    required_sets: &[RequiredSchemaSet],
) -> Result<Vec<MissingTableReport>, DatabaseError> {
    let mut reports = Vec::new();

    for required in required_sets {
        let existing = fetch_existing_tables(pool, &required.schema_name).await?;
        let missing_tables = diff_required_tables(&existing, required.required_tables);
        let missing_columns = match required.kind {
            RequiredSchemaKind::Public => {
                let existing_columns = fetch_existing_columns(pool, &required.schema_name).await?;
                diff_required_columns(&existing_columns, PUBLIC_REQUIRED_COLUMNS)
            }
            RequiredSchemaKind::Fixed | RequiredSchemaKind::Tenant => Vec::new(),
        };

        if !missing_tables.is_empty() || !missing_columns.is_empty() {
            reports.push(MissingTableReport {
                schema_name: required.schema_name.clone(),
                missing_tables,
                missing_columns,
                kind: required.kind,
            });
        }
    }

    Ok(reports)
}

async fn fetch_existing_tables(
    pool: &PgPool,
    schema_name: &str,
) -> Result<HashSet<String>, DatabaseError> {
    let rows = sqlx::query(
        r#"
        SELECT table_name
        FROM information_schema.tables
        WHERE table_schema = $1
        "#,
    )
    .bind(schema_name)
    .fetch_all(pool)
    .await
    .map_err(|error| DatabaseError::QueryFailed(error.to_string()))?;

    Ok(rows
        .into_iter()
        .filter_map(|row| row.try_get::<String, _>("table_name").ok())
        .collect())
}

async fn fetch_existing_columns(
    pool: &PgPool,
    schema_name: &str,
) -> Result<HashSet<(String, String)>, DatabaseError> {
    let rows = sqlx::query(
        r#"
        SELECT table_name, column_name
        FROM information_schema.columns
        WHERE table_schema = $1
        "#,
    )
    .bind(schema_name)
    .fetch_all(pool)
    .await
    .map_err(|error| DatabaseError::QueryFailed(error.to_string()))?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let table_name = row.try_get::<String, _>("table_name").ok()?;
            let column_name = row.try_get::<String, _>("column_name").ok()?;
            Some((table_name, column_name))
        })
        .collect())
}

async fn list_existing_tenant_schemas(pool: &PgPool) -> Result<Vec<String>, DatabaseError> {
    let rows = sqlx::query(
        r#"
        SELECT schema_name
        FROM information_schema.schemata
        WHERE schema_name LIKE 'tenant\_%' ESCAPE '\'
        ORDER BY schema_name
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| DatabaseError::QueryFailed(error.to_string()))?;

    Ok(rows
        .into_iter()
        .filter_map(|row| row.try_get::<String, _>("schema_name").ok())
        .collect())
}

fn diff_required_tables(
    existing_tables: &HashSet<String>,
    required_tables: &[&str],
) -> Vec<String> {
    let mut missing = required_tables
        .iter()
        .filter(|table_name| !existing_tables.contains(**table_name))
        .map(|table_name| (*table_name).to_string())
        .collect::<Vec<_>>();
    missing.sort();
    missing
}

fn diff_required_columns(
    existing_columns: &HashSet<(String, String)>,
    required_columns: &[(&str, &[&str])],
) -> Vec<String> {
    let mut missing = required_columns
        .iter()
        .flat_map(|(table_name, columns)| {
            columns.iter().filter_map(move |column_name| {
                if existing_columns.contains(&(table_name.to_string(), (*column_name).to_string()))
                {
                    None
                } else {
                    Some(format!("{table_name}.{column_name}"))
                }
            })
        })
        .collect::<Vec<_>>();
    missing.sort();
    missing
}

async fn repair_missing_tables(
    db: &DatabasePool,
    report: &MissingTableReport,
) -> Result<(), DatabaseError> {
    match report.kind {
        RequiredSchemaKind::Public => ensure_public_schema_baseline(db.pool()).await,
        RequiredSchemaKind::Fixed => {
            ensure_fixed_schema_baseline(db.pool(), &report.schema_name).await
        }
        RequiredSchemaKind::Tenant => ensure_tenant_schema_baseline(db, &report.schema_name).await,
    }
}

async fn ensure_public_schema_baseline(pool: &PgPool) -> Result<(), DatabaseError> {
    let mut conn = pool
        .acquire()
        .await
        .map_err(|error| DatabaseError::SchemaError(error.to_string()))?;

    sqlx::query("SET search_path TO public")
        .execute(&mut *conn)
        .await
        .map_err(|error| DatabaseError::SchemaError(error.to_string()))?;

    let apply_result = async {
        for script in PUBLIC_BASELINE_SCRIPTS {
            execute_pg_raw_sql_conn(&mut conn, script)
                .await
                .map_err(|error| DatabaseError::SchemaError(error.to_string()))?;
        }
        Ok::<(), DatabaseError>(())
    }
    .await;

    let reset_result = sqlx::query("RESET search_path")
        .execute(&mut *conn)
        .await
        .map_err(|error| DatabaseError::SchemaError(error.to_string()));

    apply_result?;
    reset_result?;
    Ok(())
}

async fn ensure_fixed_schema_baseline(
    pool: &PgPool,
    schema_name: &str,
) -> Result<(), DatabaseError> {
    PostgresStorageBackend::ensure_schema_in_pool(pool, schema_name).await?;
    PostgresAuditStorageAdapter::ensure_table(pool, schema_name)
        .await
        .map_err(|error| DatabaseError::SchemaError(error.to_string()))?;
    Ok(())
}

async fn ensure_tenant_schema_baseline(
    db: &DatabasePool,
    schema_name: &str,
) -> Result<(), DatabaseError> {
    let manager = SchemaManager::new(db.clone());
    manager.ensure_schema_objects(schema_name).await?;
    manager.ensure_default_roles_in_schema(schema_name).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_required_tables_reports_missing_public_tables() {
        let existing = HashSet::from([
            "tenants".to_string(),
            "users".to_string(),
            "credentials".to_string(),
        ]);

        let missing = diff_required_tables(&existing, PUBLIC_REQUIRED_TABLES);

        assert!(missing.contains(&"external_identities".to_string()));
        assert!(missing.contains(&"sandbox_sessions".to_string()));
        assert!(!missing.contains(&"tenants".to_string()));
    }

    #[test]
    fn diff_required_tables_reports_missing_fixed_schema_tables() {
        let existing = HashSet::from(["credentials".to_string()]);

        let missing = diff_required_tables(&existing, FIXED_SCHEMA_REQUIRED_TABLES);

        assert_eq!(
            missing,
            vec!["audit_logs".to_string(), "credential_versions".to_string()]
        );
    }

    #[test]
    fn diff_required_tables_reports_missing_tenant_tables() {
        let existing = HashSet::from([
            "credentials".to_string(),
            "scope_tokens".to_string(),
            "tenant_roles".to_string(),
        ]);

        let missing = diff_required_tables(&existing, TENANT_REQUIRED_TABLES);

        assert_eq!(
            missing,
            vec!["audit_logs".to_string(), "user_roles".to_string()]
        );
    }

    #[test]
    fn diff_required_columns_reports_missing_public_migration_columns() {
        let existing = HashSet::from([
            ("credentials".to_string(), "version".to_string()),
            ("audit_logs".to_string(), "event_category".to_string()),
            ("sandbox_sessions".to_string(), "credential_id".to_string()),
            ("api_tokens".to_string(), "token_kind".to_string()),
        ]);

        let missing = diff_required_columns(&existing, PUBLIC_REQUIRED_COLUMNS);

        assert!(missing.contains(&"audit_logs.metadata".to_string()));
        assert!(missing.contains(&"sandbox_sessions.original_intent".to_string()));
        assert!(missing.contains(&"api_tokens.credential_ids".to_string()));
        assert!(!missing.contains(&"credentials.version".to_string()));
    }
}
