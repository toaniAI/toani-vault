use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use uuid::Uuid;
use vault_service::services::db::{DatabaseConfig, DatabasePool};
use vault_service::tenant::{PostgresTenantConfigStore, TenantConfig, TenantConfigStore, TenantId};

const SYSTEM_TENANT_ID: &str = "00000000-0000-0000-0000-000000000000";

async fn setup_test_pool() -> Option<PgPool> {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok()?;

    PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .ok()
}

async fn setup_database_pool() -> Option<DatabasePool> {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok()?;

    let config = DatabaseConfig {
        url: database_url,
        max_connections: 5,
        min_connections: 1,
        connect_timeout: 10,
        idle_timeout: 60,
    };
    DatabasePool::new(config).await.ok()
}

async fn ensure_tenants_table(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tenants (
            id UUID PRIMARY KEY,
            name VARCHAR(100) NOT NULL,
            description TEXT,
            status VARCHAR(50) DEFAULT 'active',
            config JSONB DEFAULT '{}'::jsonb,
            created_at TIMESTAMPTZ DEFAULT NOW(),
            updated_at TIMESTAMPTZ DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

fn tenant_config_json() -> serde_json::Value {
    serde_json::json!({
        "feature_flags": {
            "enable_credential_encryption": true,
            "enable_audit_logging": true,
            "enable_token_revocation": true,
            "enable_mfa": false,
            "enable_remote_attestation": false,
            "enable_auto_rotation": false,
            "allow_cors": false,
            "enable_ip_whitelist": false,
            "enable_webhooks": false,
            "enable_sso": false,
            "enable_custom_crypto": false,
            "enable_advanced_audit": false
        },
        "quota_limits": {
            "max_credentials": 1000,
            "max_tokens_per_user": 10,
            "max_requests_per_minute": 1000,
            "max_users": 100,
            "max_connectors": 20,
            "max_webhooks": 10,
            "storage_quota_mb": 1024,
            "audit_retention_days": 30,
            "max_token_ttl_seconds": 86400,
            "max_batch_size": 100
        },
        "settings": {
            "token_ttl_seconds": 900,
            "session_timeout_seconds": 3600,
            "max_login_attempts": 5,
            "lockout_duration_seconds": 900,
            "password_min_length": 8,
            "require_password_complexity": true,
            "require_mfa": false,
            "allowed_callback_urls": [],
            "timezone": "UTC",
            "language": "zh-CN",
            "metadata": {}
        },
        "version": 1,
        "updated_at": null,
        "updated_by": null
    })
}

#[tokio::test]
async fn postgres_tenant_config_survives_store_restart() {
    let Some(pool) = setup_test_pool().await else {
        eprintln!(
            "skip tenant config persistence test: TEST_DATABASE_URL/DATABASE_URL not configured"
        );
        return;
    };

    if ensure_tenants_table(&pool).await.is_err() {
        eprintln!("skip tenant config persistence test: unable to ensure tenants table");
        return;
    }

    let Some(db_pool) = setup_database_pool().await else {
        eprintln!("skip tenant config persistence test: cannot build DatabasePool");
        return;
    };

    let tenant_uuid = Uuid::new_v4();
    let tenant_id = TenantId::from_string(tenant_uuid.to_string());
    sqlx::query(
        "INSERT INTO tenants (id, name, description, status, config) VALUES ($1, $2, $3, $4, '{}'::jsonb)",
    )
    .bind(tenant_uuid)
    .bind("tenant-config-persistence")
    .bind("integration test tenant")
    .bind("active")
    .execute(&pool)
    .await
    .expect("should create tenant row");

    let mut config = TenantConfig::default();
    config.settings.language = "en-US".to_string();
    config.settings.timezone = "America/New_York".to_string();

    let store = PostgresTenantConfigStore::new(db_pool.clone());
    store
        .save_config(&tenant_id, &config)
        .await
        .expect("save config should persist");

    let restarted_store = PostgresTenantConfigStore::new(db_pool);
    let loaded = restarted_store
        .get_config(&tenant_id)
        .await
        .expect("get config after restart should succeed");

    assert_eq!(loaded.settings.language, "en-US");
    assert_eq!(loaded.settings.timezone, "America/New_York");
    assert_eq!(loaded.quota_limits.max_credentials, 1000);
}

#[tokio::test]
async fn system_tenant_bootstrap_config_roundtrips_from_postgres() {
    let Some(pool) = setup_test_pool().await else {
        eprintln!("skip tenant bootstrap test: TEST_DATABASE_URL/DATABASE_URL not configured");
        return;
    };

    if ensure_tenants_table(&pool).await.is_err() {
        eprintln!("skip tenant bootstrap test: unable to ensure tenants table");
        return;
    }

    let Some(db_pool) = setup_database_pool().await else {
        eprintln!("skip tenant bootstrap test: cannot build DatabasePool");
        return;
    };

    let system_tenant_uuid = Uuid::parse_str(SYSTEM_TENANT_ID).expect("system tenant id is valid");
    sqlx::query(
        r#"
        INSERT INTO tenants (id, name, description, status, config)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (id) DO UPDATE
        SET config = EXCLUDED.config,
            updated_at = NOW()
        "#,
    )
    .bind(system_tenant_uuid)
    .bind("system")
    .bind("System tenant for integration bootstrap verification")
    .bind("active")
    .bind(tenant_config_json())
    .execute(&pool)
    .await
    .expect("should upsert system tenant config");

    let tenant_id = TenantId::from_string(SYSTEM_TENANT_ID);
    let store = PostgresTenantConfigStore::new(db_pool);
    let config = store
        .get_config(&tenant_id)
        .await
        .expect("bootstrap config should be readable from postgres");

    assert_eq!(config.settings.language, "zh-CN");
    assert_eq!(config.settings.timezone, "UTC");
    assert_eq!(config.quota_limits.max_credentials, 1000);

    let row = sqlx::query("SELECT config FROM tenants WHERE id = $1")
        .bind(system_tenant_uuid)
        .fetch_one(&pool)
        .await
        .expect("system tenant row should exist");
    let raw_config: serde_json::Value = row
        .try_get("config")
        .expect("config column should deserialize as json");
    assert_eq!(raw_config["settings"]["language"], "zh-CN");
}
