use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;
use vault_service::http_request_broker::{
    HttpRequestBroker, HttpRequestBrokerSubmitInput, HttpRequestCredentialResolver,
    HttpRequestOperationStore, HttpRequestTransport, PersistentHttpRequestBroker,
    PgHttpRequestOperationStore,
};
use vault_service::models::CredentialType;
use vault_service::tee::sandbox::error::SandboxError;
use vault_service::tee::sandbox::http_template::HttpTemplateCredentialMaterial;

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

async fn ensure_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sandbox_sessions (
            id UUID PRIMARY KEY,
            tenant_id UUID NOT NULL,
            created_by UUID NOT NULL,
            credential_id UUID,
            original_intent TEXT,
            status VARCHAR(32) NOT NULL DEFAULT 'ready',
            started_at TIMESTAMPTZ NOT NULL,
            expires_at TIMESTAMPTZ NOT NULL,
            terminated_at TIMESTAMPTZ,
            termination_reason TEXT,
            active_operation_id UUID,
            active_operation_started_at TIMESTAMPTZ,
            last_error_summary TEXT,
            tee_context_id VARCHAR(128),
            metadata JSONB,
            created_at TIMESTAMPTZ DEFAULT NOW(),
            updated_at TIMESTAMPTZ DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(include_str!(
        "../../migrations/20260515111500_add_http_request_operations.sql"
    ))
    .execute(pool)
    .await?;

    Ok(())
}

struct FakeResolver;

#[async_trait]
impl HttpRequestCredentialResolver for FakeResolver {
    async fn resolve(
        &self,
        _tenant_id: Uuid,
        _user_id: Uuid,
        _credential_id: Uuid,
    ) -> Result<HttpTemplateCredentialMaterial, SandboxError> {
        Ok(HttpTemplateCredentialMaterial {
            credential_type: CredentialType::ApiKey,
            values: HashMap::from([("api_key".to_string(), "secret-api-key".to_string())]),
            provider: None,
            allowed_domains: vec!["api.example.com".to_string()],
            custom_functions: Vec::new(),
        })
    }
}

struct FakeTransport;

#[async_trait]
impl HttpRequestTransport for FakeTransport {
    async fn execute(
        &self,
        _parameters: &HashMap<String, Value>,
        _sensitive_output_values: &[String],
    ) -> Result<Value, SandboxError> {
        Ok(json!({
            "status": 200,
            "body": {
                "echo": "Bearer secret-api-key"
            }
        }))
    }
}

#[tokio::test]
async fn test_http_request_broker_persists_operation_without_creating_session_rows() {
    let Some(pool) = setup_test_pool().await else {
        eprintln!(
            "skip http request broker persistence test: TEST_DATABASE_URL/DATABASE_URL not set"
        );
        return;
    };
    if ensure_schema(&pool).await.is_err() {
        eprintln!("skip http request broker persistence test: unable to ensure schema");
        return;
    }

    let tenant_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let credential_id = Uuid::new_v4();
    let store: Arc<dyn HttpRequestOperationStore> =
        Arc::new(PgHttpRequestOperationStore::new(pool.clone()));
    let broker = PersistentHttpRequestBroker::new(
        Arc::new(FakeResolver),
        Arc::new(FakeTransport),
        store.clone(),
    );

    let submit = broker
        .submit(HttpRequestBrokerSubmitInput {
            tenant_id,
            user_id,
            credential_id,
            description: "Fetch broker payload".to_string(),
            parameters: HashMap::from([
                ("method".to_string(), json!("GET")),
                ("url".to_string(), json!("https://api.example.com/balance")),
                (
                    "headers".to_string(),
                    json!({
                        "Authorization": "Bearer ${credential.api_key}"
                    }),
                ),
            ]),
        })
        .await
        .expect("submit should succeed");

    assert!(submit.success);
    assert_eq!(
        submit.data,
        Some(json!({
            "status": 200,
            "body": {
                "echo": "Bearer [REDACTED]"
            }
        }))
    );

    let operation = broker
        .get_operation(tenant_id, submit.operation_id)
        .await
        .expect("get should succeed")
        .expect("persisted operation should exist");

    assert_eq!(operation.status, "completed");
    assert_eq!(
        operation.request_parameters["headers"]["Authorization"],
        json!("[REDACTED]")
    );
    assert_eq!(
        operation.response_data,
        Some(json!({
            "status": 200,
            "body": {
                "echo": "Bearer [REDACTED]"
            }
        }))
    );
    assert!(operation.completed_at.unwrap_or_else(Utc::now) >= operation.started_at);

    let session_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sandbox_sessions WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .expect("count sandbox sessions");
    assert_eq!(
        session_count, 0,
        "broker flow must not create sandbox session rows"
    );

    let operation_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM http_request_operations WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .expect("count http request operations");
    assert_eq!(operation_count, 1);

    let _ = sqlx::query("DELETE FROM http_request_operations WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await;
}
