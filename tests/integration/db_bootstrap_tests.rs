use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use uuid::Uuid;
use vault_service::services::db::{
    DatabaseConfig, DatabasePool, ensure_required_tables_on_startup,
};

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

const FIXED_SCHEMA_REQUIRED_TABLES: &[&str] = &["credentials", "credential_versions", "audit_logs"];
const TENANT_REQUIRED_TABLES: &[&str] = &[
    "credentials",
    "scope_tokens",
    "audit_logs",
    "tenant_roles",
    "user_roles",
];

fn test_database_url() -> Option<String> {
    std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok()
}

fn postgres_admin_url(database_url: &str) -> Option<String> {
    let (prefix, database_name) = database_url.rsplit_once('/')?;
    let query = database_name
        .find('?')
        .map(|idx| &database_name[idx..])
        .unwrap_or("");
    Some(format!("{prefix}/postgres{query}"))
}

fn temp_database_url(database_url: &str, database_name: &str) -> Option<String> {
    let (prefix, current_database) = database_url.rsplit_once('/')?;
    let query = current_database
        .find('?')
        .map(|idx| &current_database[idx..])
        .unwrap_or("");
    Some(format!("{prefix}/{database_name}{query}"))
}

async fn create_temp_database(admin_pool: &PgPool, database_name: &str) -> Result<(), sqlx::Error> {
    let create_sql = format!("CREATE DATABASE \"{database_name}\"");
    sqlx::query(&create_sql).execute(admin_pool).await?;
    Ok(())
}

async fn drop_temp_database(admin_pool: &PgPool, database_name: &str) {
    let terminate_sql = r#"
        SELECT pg_terminate_backend(pid)
        FROM pg_stat_activity
        WHERE datname = $1
          AND pid <> pg_backend_pid()
    "#;
    let _ = sqlx::query(terminate_sql)
        .bind(database_name)
        .execute(admin_pool)
        .await;

    let drop_sql = format!("DROP DATABASE IF EXISTS \"{database_name}\"");
    let _ = sqlx::query(&drop_sql).execute(admin_pool).await;
}

async fn fetch_schema_tables(pool: &PgPool, schema_name: &str) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT table_name
        FROM information_schema.tables
        WHERE table_schema = $1
        ORDER BY table_name
        "#,
    )
    .bind(schema_name)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("table_name"))
        .collect())
}

async fn fetch_table_columns(
    pool: &PgPool,
    schema_name: &str,
    table_name: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT column_name
        FROM information_schema.columns
        WHERE table_schema = $1
          AND table_name = $2
        ORDER BY column_name
        "#,
    )
    .bind(schema_name)
    .bind(table_name)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("column_name"))
        .collect())
}

fn assert_tables_present(existing_tables: &[String], required_tables: &[&str]) {
    for required_table in required_tables {
        assert!(
            existing_tables.contains(&required_table.to_string()),
            "expected table {required_table} to exist, got {existing_tables:?}"
        );
    }
}

fn assert_columns_present(existing_columns: &[String], required_columns: &[&str]) {
    for required_column in required_columns {
        assert!(
            existing_columns.contains(&required_column.to_string()),
            "expected column {required_column} to exist, got {existing_columns:?}"
        );
    }
}

#[tokio::test]
async fn startup_bootstrap_repairs_public_fixed_and_tenant_schemas() {
    let Some(base_database_url) = test_database_url() else {
        eprintln!(
            "skip db bootstrap integration test: TEST_DATABASE_URL/DATABASE_URL not configured"
        );
        return;
    };
    let Some(admin_database_url) = postgres_admin_url(&base_database_url) else {
        eprintln!("skip db bootstrap integration test: unable to derive admin database url");
        return;
    };

    let admin_pool = match PgPoolOptions::new()
        .max_connections(2)
        .connect(&admin_database_url)
        .await
    {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("skip db bootstrap integration test: cannot connect admin database: {error}");
            return;
        }
    };

    let database_name = format!(
        "credbridge_bootstrap_{}",
        &Uuid::new_v4().simple().to_string()[..12]
    );
    if let Err(error) = create_temp_database(&admin_pool, &database_name).await {
        eprintln!("skip db bootstrap integration test: cannot create temp database: {error}");
        return;
    }

    let Some(temp_database_url) = temp_database_url(&base_database_url, &database_name) else {
        eprintln!("skip db bootstrap integration test: cannot derive temp database url");
        drop_temp_database(&admin_pool, &database_name).await;
        return;
    };

    let test_pool = match PgPoolOptions::new()
        .max_connections(5)
        .connect(&temp_database_url)
        .await
    {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("skip db bootstrap integration test: cannot connect temp database: {error}");
            drop_temp_database(&admin_pool, &database_name).await;
            return;
        }
    };

    let tenant_schema = "tenant_integration_bootstrap";
    sqlx::query(&format!("CREATE SCHEMA \"{tenant_schema}\""))
        .execute(&test_pool)
        .await
        .expect("should create tenant schema");

    let fixed_schema = "credbridge_bootstrap_vault";
    let original_fixed_schema = std::env::var("CREDBRIDGE_PG_SCHEMA").ok();
    unsafe {
        std::env::set_var("CREDBRIDGE_PG_SCHEMA", fixed_schema);
    }

    let config = DatabaseConfig {
        url: temp_database_url.clone(),
        max_connections: 5,
        min_connections: 1,
        connect_timeout: 10,
        idle_timeout: 60,
    };
    let database_pool = DatabasePool::new(config)
        .await
        .expect("should create DatabasePool for temp database");

    ensure_required_tables_on_startup(&database_pool)
        .await
        .expect("bootstrap should create all required tables");
    ensure_required_tables_on_startup(&database_pool)
        .await
        .expect("bootstrap should be idempotent");

    let public_tables = fetch_schema_tables(database_pool.pool(), "public")
        .await
        .expect("should list public tables");
    assert_tables_present(&public_tables, PUBLIC_REQUIRED_TABLES);

    let fixed_tables = fetch_schema_tables(database_pool.pool(), fixed_schema)
        .await
        .expect("should list fixed schema tables");
    assert_tables_present(&fixed_tables, FIXED_SCHEMA_REQUIRED_TABLES);

    let tenant_tables = fetch_schema_tables(database_pool.pool(), tenant_schema)
        .await
        .expect("should list tenant schema tables");
    assert_tables_present(&tenant_tables, TENANT_REQUIRED_TABLES);

    database_pool.close().await;
    test_pool.close().await;

    match original_fixed_schema {
        Some(value) => unsafe { std::env::set_var("CREDBRIDGE_PG_SCHEMA", value) },
        None => unsafe { std::env::remove_var("CREDBRIDGE_PG_SCHEMA") },
    }

    drop_temp_database(&admin_pool, &database_name).await;
    admin_pool.close().await;
}

#[tokio::test]
async fn startup_bootstrap_repairs_public_partial_migrations() {
    let Some(base_database_url) = test_database_url() else {
        eprintln!(
            "skip db bootstrap integration test: TEST_DATABASE_URL/DATABASE_URL not configured"
        );
        return;
    };
    let Some(admin_database_url) = postgres_admin_url(&base_database_url) else {
        eprintln!("skip db bootstrap integration test: unable to derive admin database url");
        return;
    };

    let admin_pool = match PgPoolOptions::new()
        .max_connections(2)
        .connect(&admin_database_url)
        .await
    {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("skip db bootstrap integration test: cannot connect admin database: {error}");
            return;
        }
    };

    let database_name = format!(
        "credbridge_bootstrap_partial_{}",
        &Uuid::new_v4().simple().to_string()[..12]
    );
    if let Err(error) = create_temp_database(&admin_pool, &database_name).await {
        eprintln!("skip db bootstrap integration test: cannot create temp database: {error}");
        return;
    }

    let Some(temp_database_url) = temp_database_url(&base_database_url, &database_name) else {
        eprintln!("skip db bootstrap integration test: cannot derive temp database url");
        drop_temp_database(&admin_pool, &database_name).await;
        return;
    };

    let config = DatabaseConfig {
        url: temp_database_url.clone(),
        max_connections: 5,
        min_connections: 1,
        connect_timeout: 10,
        idle_timeout: 60,
    };
    let database_pool = match DatabasePool::new(config).await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("skip db bootstrap integration test: cannot connect temp database: {error}");
            drop_temp_database(&admin_pool, &database_name).await;
            return;
        }
    };

    ensure_required_tables_on_startup(&database_pool)
        .await
        .expect("bootstrap should create all required tables");

    sqlx::query(
        r#"
        ALTER TABLE public.api_tokens
            DROP COLUMN IF EXISTS token_kind,
            DROP COLUMN IF EXISTS token_name,
            DROP COLUMN IF EXISTS token_prefix,
            DROP COLUMN IF EXISTS description,
            DROP COLUMN IF EXISTS issued_membership_role_snapshot,
            DROP COLUMN IF EXISTS permission_source,
            DROP COLUMN IF EXISTS created_via,
            DROP COLUMN IF EXISTS revoked_reason,
            DROP COLUMN IF EXISTS oauth_client_id,
            DROP COLUMN IF EXISTS oauth_grant_type,
            DROP COLUMN IF EXISTS oauth_subject_mode,
            DROP COLUMN IF EXISTS credential_ids
        "#,
    )
    .execute(database_pool.pool())
    .await
    .expect("should remove api_tokens migration columns");

    sqlx::query(
        r#"
        ALTER TABLE public.sandbox_sessions
            DROP COLUMN IF EXISTS credential_id,
            DROP COLUMN IF EXISTS original_intent
        "#,
    )
    .execute(database_pool.pool())
    .await
    .expect("should remove sandbox_sessions migration columns");

    ensure_required_tables_on_startup(&database_pool)
        .await
        .expect("bootstrap should repair public partial migrations");

    let api_token_columns = fetch_table_columns(database_pool.pool(), "public", "api_tokens")
        .await
        .expect("should list api_tokens columns");
    assert_columns_present(
        &api_token_columns,
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
    );

    let sandbox_session_columns =
        fetch_table_columns(database_pool.pool(), "public", "sandbox_sessions")
            .await
            .expect("should list sandbox_sessions columns");
    assert_columns_present(
        &sandbox_session_columns,
        &["credential_id", "original_intent"],
    );

    database_pool.close().await;
    drop_temp_database(&admin_pool, &database_name).await;
    admin_pool.close().await;
}

#[tokio::test]
async fn startup_bootstrap_repairs_public_schema_even_when_search_path_is_stale() {
    let Some(base_database_url) = test_database_url() else {
        eprintln!(
            "skip db bootstrap integration test: TEST_DATABASE_URL/DATABASE_URL not configured"
        );
        return;
    };
    let Some(admin_database_url) = postgres_admin_url(&base_database_url) else {
        eprintln!("skip db bootstrap integration test: unable to derive admin database url");
        return;
    };

    let admin_pool = match PgPoolOptions::new()
        .max_connections(2)
        .connect(&admin_database_url)
        .await
    {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("skip db bootstrap integration test: cannot connect admin database: {error}");
            return;
        }
    };

    let database_name = format!(
        "credbridge_bootstrap_search_path_{}",
        &Uuid::new_v4().simple().to_string()[..12]
    );
    if let Err(error) = create_temp_database(&admin_pool, &database_name).await {
        eprintln!("skip db bootstrap integration test: cannot create temp database: {error}");
        return;
    }

    let Some(temp_database_url) = temp_database_url(&base_database_url, &database_name) else {
        eprintln!("skip db bootstrap integration test: cannot derive temp database url");
        drop_temp_database(&admin_pool, &database_name).await;
        return;
    };

    let fixed_schema = "credbridge_bootstrap_vault";
    let original_fixed_schema = std::env::var("CREDBRIDGE_PG_SCHEMA").ok();
    unsafe {
        std::env::set_var("CREDBRIDGE_PG_SCHEMA", fixed_schema);
    }

    let config = DatabaseConfig {
        url: temp_database_url.clone(),
        max_connections: 1,
        min_connections: 1,
        connect_timeout: 10,
        idle_timeout: 60,
    };
    let database_pool = match DatabasePool::new(config).await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("skip db bootstrap integration test: cannot connect temp database: {error}");
            drop_temp_database(&admin_pool, &database_name).await;
            return;
        }
    };

    sqlx::query(&format!("CREATE SCHEMA \"{fixed_schema}\""))
        .execute(database_pool.pool())
        .await
        .expect("should create fixed schema");

    sqlx::query(&format!(
        r#"
        CREATE TABLE "{fixed_schema}".credentials (
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
            CONSTRAINT credentials_version_check CHECK (version >= 1)
        )
        "#
    ))
    .execute(database_pool.pool())
    .await
    .expect("should create fixed schema credential table with conflicting constraint name");

    sqlx::query(&format!("SET search_path TO \"{fixed_schema}\""))
        .execute(database_pool.pool())
        .await
        .expect("should contaminate pooled connection search_path");

    ensure_required_tables_on_startup(&database_pool)
        .await
        .expect("bootstrap should still repair public schema");

    let public_tables = fetch_schema_tables(database_pool.pool(), "public")
        .await
        .expect("should list public tables");
    assert_tables_present(&public_tables, PUBLIC_REQUIRED_TABLES);

    database_pool.close().await;

    match original_fixed_schema {
        Some(value) => unsafe { std::env::set_var("CREDBRIDGE_PG_SCHEMA", value) },
        None => unsafe { std::env::remove_var("CREDBRIDGE_PG_SCHEMA") },
    }

    drop_temp_database(&admin_pool, &database_name).await;
    admin_pool.close().await;
}

#[tokio::test]
async fn startup_bootstrap_repairs_fixed_schema_legacy_sandbox_columns() {
    let Some(base_database_url) = test_database_url() else {
        eprintln!(
            "skip db bootstrap integration test: TEST_DATABASE_URL/DATABASE_URL not configured"
        );
        return;
    };
    let Some(admin_database_url) = postgres_admin_url(&base_database_url) else {
        eprintln!("skip db bootstrap integration test: unable to derive admin database url");
        return;
    };

    let admin_pool = match PgPoolOptions::new()
        .max_connections(2)
        .connect(&admin_database_url)
        .await
    {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("skip db bootstrap integration test: cannot connect admin database: {error}");
            return;
        }
    };

    let database_name = format!(
        "credbridge_bootstrap_fixed_sandbox_{}",
        &Uuid::new_v4().simple().to_string()[..12]
    );
    if let Err(error) = create_temp_database(&admin_pool, &database_name).await {
        eprintln!("skip db bootstrap integration test: cannot create temp database: {error}");
        return;
    }

    let Some(temp_database_url) = temp_database_url(&base_database_url, &database_name) else {
        eprintln!("skip db bootstrap integration test: cannot derive temp database url");
        drop_temp_database(&admin_pool, &database_name).await;
        return;
    };

    let fixed_schema = "credbridge_bootstrap_fixed_sandbox";
    let original_fixed_schema = std::env::var("CREDBRIDGE_PG_SCHEMA").ok();
    unsafe {
        std::env::set_var("CREDBRIDGE_PG_SCHEMA", fixed_schema);
    }

    let config = DatabaseConfig {
        url: temp_database_url.clone(),
        max_connections: 5,
        min_connections: 1,
        connect_timeout: 10,
        idle_timeout: 60,
    };
    let database_pool = match DatabasePool::new(config).await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("skip db bootstrap integration test: cannot connect temp database: {error}");
            drop_temp_database(&admin_pool, &database_name).await;
            return;
        }
    };

    sqlx::query(&format!("CREATE SCHEMA \"{fixed_schema}\""))
        .execute(database_pool.pool())
        .await
        .expect("should create fixed schema");

    let mut conn = database_pool
        .acquire()
        .await
        .expect("should acquire connection for fixed-schema bootstrap setup");
    sqlx::raw_sql(&format!("SET search_path TO \"{fixed_schema}\", public;"))
        .execute(&mut *conn)
        .await
        .expect("should set search_path to fixed schema");

    sqlx::raw_sql(include_str!(
        "../../migrations/20260317000001_create_sandbox_tables.sql"
    ))
    .execute(&mut *conn)
    .await
    .expect("should create legacy fixed-schema sandbox tables");
    sqlx::raw_sql(include_str!(
        "../../migrations/20260403093000_add_sandbox_session_identity_columns.sql"
    ))
    .execute(&mut *conn)
    .await
    .expect("should add fixed-schema sandbox identity columns");
    drop(conn);

    sqlx::query(&format!(
        r#"
        CREATE TABLE "{fixed_schema}".credentials (
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
            CONSTRAINT credentials_version_check CHECK (version >= 1)
        )
        "#
    ))
    .execute(database_pool.pool())
    .await
    .expect("should create fixed schema credential table");

    sqlx::query(&format!(
        r#"
        ALTER TABLE "{fixed_schema}".sandbox_sessions
        DROP COLUMN IF EXISTS active_operation_id,
        DROP COLUMN IF EXISTS active_operation_started_at,
        DROP COLUMN IF EXISTS last_error_summary
        "#
    ))
    .execute(database_pool.pool())
    .await
    .expect("should remove latest sandbox session columns from fixed schema");

    sqlx::query(&format!(
        r#"
        CREATE OR REPLACE VIEW "{fixed_schema}".sandbox_active_sessions AS
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
            COUNT(o.id) AS operation_count,
            MAX(o.started_at) AS last_operation_at
        FROM "{fixed_schema}".sandbox_sessions s
        LEFT JOIN "{fixed_schema}".sandbox_operations o ON s.id = o.session_id
        WHERE s.status = 'active'
        GROUP BY
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
            s.updated_at
        "#
    ))
    .execute(database_pool.pool())
    .await
    .expect("should create legacy fixed-schema sandbox view shape");

    ensure_required_tables_on_startup(&database_pool)
        .await
        .expect("bootstrap should repair fixed schema sandbox compatibility columns");

    let fixed_sandbox_session_columns =
        fetch_table_columns(database_pool.pool(), fixed_schema, "sandbox_sessions")
            .await
            .expect("should list fixed-schema sandbox_sessions columns");
    assert_columns_present(
        &fixed_sandbox_session_columns,
        &[
            "credential_id",
            "original_intent",
            "active_operation_id",
            "active_operation_started_at",
            "last_error_summary",
        ],
    );

    let fixed_sandbox_active_view_columns = fetch_table_columns(
        database_pool.pool(),
        fixed_schema,
        "sandbox_active_sessions",
    )
    .await
    .expect("should list fixed-schema sandbox_active_sessions view columns");
    assert_columns_present(
        &fixed_sandbox_active_view_columns,
        &[
            "credential_id",
            "original_intent",
            "active_operation_id",
            "active_operation_started_at",
            "last_error_summary",
            "operation_count",
            "last_operation_at",
        ],
    );

    database_pool.close().await;

    match original_fixed_schema {
        Some(value) => unsafe { std::env::set_var("CREDBRIDGE_PG_SCHEMA", value) },
        None => unsafe { std::env::remove_var("CREDBRIDGE_PG_SCHEMA") },
    }

    drop_temp_database(&admin_pool, &database_name).await;
    admin_pool.close().await;
}

#[tokio::test]
async fn startup_bootstrap_repairs_partial_migrations_with_legacy_sandbox_view_shape() {
    let Some(base_database_url) = test_database_url() else {
        eprintln!(
            "skip db bootstrap integration test: TEST_DATABASE_URL/DATABASE_URL not configured"
        );
        return;
    };
    let Some(admin_database_url) = postgres_admin_url(&base_database_url) else {
        eprintln!("skip db bootstrap integration test: unable to derive admin database url");
        return;
    };

    let admin_pool = match PgPoolOptions::new()
        .max_connections(2)
        .connect(&admin_database_url)
        .await
    {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("skip db bootstrap integration test: cannot connect admin database: {error}");
            return;
        }
    };

    let database_name = format!(
        "credbridge_bootstrap_legacy_view_{}",
        &Uuid::new_v4().simple().to_string()[..12]
    );
    if let Err(error) = create_temp_database(&admin_pool, &database_name).await {
        eprintln!("skip db bootstrap integration test: cannot create temp database: {error}");
        return;
    }

    let Some(temp_database_url) = temp_database_url(&base_database_url, &database_name) else {
        eprintln!("skip db bootstrap integration test: cannot derive temp database url");
        drop_temp_database(&admin_pool, &database_name).await;
        return;
    };

    let config = DatabaseConfig {
        url: temp_database_url.clone(),
        max_connections: 5,
        min_connections: 1,
        connect_timeout: 10,
        idle_timeout: 60,
    };
    let database_pool = match DatabasePool::new(config).await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("skip db bootstrap integration test: cannot connect temp database: {error}");
            drop_temp_database(&admin_pool, &database_name).await;
            return;
        }
    };

    ensure_required_tables_on_startup(&database_pool)
        .await
        .expect("bootstrap should create all required tables");

    sqlx::query("ALTER TABLE public.sandbox_sessions DROP COLUMN IF EXISTS original_intent")
        .execute(database_pool.pool())
        .await
        .expect("should remove sandbox_sessions.original_intent");

    sqlx::query(
        r#"
        CREATE OR REPLACE VIEW public.sandbox_active_sessions AS
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
            COUNT(o.id) AS operation_count,
            MAX(o.started_at) AS last_operation_at
        FROM public.sandbox_sessions s
        LEFT JOIN public.sandbox_operations o ON s.id = o.session_id
        WHERE s.status = 'active'
        GROUP BY
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
            s.updated_at
        "#,
    )
    .execute(database_pool.pool())
    .await
    .expect("should create legacy sandbox_active_sessions view shape");

    ensure_required_tables_on_startup(&database_pool)
        .await
        .expect("bootstrap should repair partial migrations even with legacy sandbox view");

    let sandbox_session_columns =
        fetch_table_columns(database_pool.pool(), "public", "sandbox_sessions")
            .await
            .expect("should list sandbox_sessions columns");
    assert_columns_present(&sandbox_session_columns, &["original_intent"]);

    let sandbox_active_view_columns =
        fetch_table_columns(database_pool.pool(), "public", "sandbox_active_sessions")
            .await
            .expect("should list sandbox_active_sessions view columns");
    assert_columns_present(
        &sandbox_active_view_columns,
        &[
            "credential_id",
            "original_intent",
            "active_operation_id",
            "active_operation_started_at",
            "last_error_summary",
            "operation_count",
            "last_operation_at",
        ],
    );

    database_pool.close().await;
    drop_temp_database(&admin_pool, &database_name).await;
    admin_pool.close().await;
}
