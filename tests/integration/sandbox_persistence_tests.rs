use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use time::OffsetDateTime;
use uuid::Uuid;
use vault_service::tee::sandbox::SessionId;
use vault_service::tee::sandbox::repository::{
    CompleteSandboxOperationRecord, NewSandboxOperationRecord, NewSandboxSessionRecord,
    PostgresSandboxRepository, SandboxRepository, metadata_to_json, to_chrono_utc,
};

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

async fn ensure_sandbox_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
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

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sandbox_operations (
            id UUID PRIMARY KEY,
            session_id UUID NOT NULL,
            tenant_id UUID NOT NULL,
            operation_type VARCHAR(64) NOT NULL,
            status VARCHAR(32) NOT NULL DEFAULT 'pending',
            input_params JSONB,
            output_result JSONB,
            error_message TEXT,
            started_at TIMESTAMPTZ NOT NULL,
            completed_at TIMESTAMPTZ,
            execution_duration_ms INTEGER,
            credential_id UUID,
            created_at TIMESTAMPTZ DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

#[tokio::test]
async fn test_sandbox_repository_persists_across_restart_and_reconciles_orphans() {
    let Some(pool) = setup_test_pool().await else {
        eprintln!("skip sandbox persistence test: TEST_DATABASE_URL/DATABASE_URL not set");
        return;
    };
    if ensure_sandbox_schema(&pool).await.is_err() {
        eprintln!("skip sandbox persistence test: unable to ensure schema");
        return;
    }

    let tenant_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let session_id = SessionId::new();
    let operation_id = Uuid::new_v4();
    let credential_id = Uuid::new_v4();
    let sandbox_id = Uuid::new_v4();
    let now = OffsetDateTime::now_utc();

    let repo = PostgresSandboxRepository::new(pool.clone());

    repo.create_session(NewSandboxSessionRecord {
        session_id,
        sandbox_id,
        tenant_id,
        created_by: user_id,
        credential_id,
        original_intent: "restart durability".to_string(),
        started_at: to_chrono_utc(now),
        expires_at: to_chrono_utc(now + time::Duration::minutes(30)),
        metadata: metadata_to_json(None),
    })
    .await
    .expect("create session should persist");

    repo.create_operation(NewSandboxOperationRecord {
        operation_id,
        session_id,
        tenant_id,
        credential_id,
        operation_type: "navigate".to_string(),
        input_params: serde_json::json!({"url":"https://example.com"}),
        started_at: to_chrono_utc(now),
    })
    .await
    .expect("create operation should persist");

    repo.mark_session_operation_started(session_id, operation_id, chrono::Utc::now())
        .await
        .expect("mark executing should persist");
    let executing = repo
        .get_session_by_id(tenant_id, session_id)
        .await
        .expect("query executing session should succeed")
        .expect("executing session should exist");
    assert_eq!(executing.status, "executing");
    assert_eq!(executing.active_operation_id, Some(operation_id));
    assert!(executing.active_operation_started_at.is_some());

    repo.complete_operation(CompleteSandboxOperationRecord {
        operation_id,
        status: "completed",
        output_result: Some(serde_json::json!({"ok":true})),
        error_message: None,
        completed_at: chrono::Utc::now(),
        execution_duration_ms: 12,
    })
    .await
    .expect("complete operation should persist");
    repo.mark_session_operation_finished(session_id, Some("navigate failed".to_string()))
        .await
        .expect("mark ready should persist");

    // Simulate process restart by re-creating repository from same pool.
    let repo_after_restart = PostgresSandboxRepository::new(pool.clone());
    let loaded_session = repo_after_restart
        .get_session_by_id(tenant_id, session_id)
        .await
        .expect("session query should succeed")
        .expect("session should exist after restart");
    assert_eq!(loaded_session.tenant_id, tenant_id);
    assert_eq!(loaded_session.credential_id, credential_id);
    assert_eq!(loaded_session.status, "ready");
    assert_eq!(loaded_session.active_operation_id, None);
    assert_eq!(
        loaded_session.last_error_summary.as_deref(),
        Some("navigate failed")
    );

    let loaded_operation = repo_after_restart
        .get_operation_by_id(tenant_id, operation_id)
        .await
        .expect("operation query should succeed")
        .expect("operation should exist after restart");
    assert_eq!(loaded_operation.status, "completed");
    assert_eq!(loaded_operation.session_id, session_id);

    // Pause/Resume persistence path
    repo_after_restart
        .update_session_status(session_id, "paused")
        .await
        .expect("pause should persist");
    let paused = repo_after_restart
        .get_session_by_id(tenant_id, session_id)
        .await
        .expect("query paused session should succeed")
        .expect("paused session should exist");
    assert_eq!(paused.status, "paused");

    repo_after_restart
        .update_session_status(session_id, "ready")
        .await
        .expect("resume should persist");

    // Orphan recovery path
    let orphan_session_id = SessionId::new();
    repo_after_restart
        .create_session(NewSandboxSessionRecord {
            session_id: orphan_session_id,
            sandbox_id: Uuid::new_v4(),
            tenant_id,
            created_by: user_id,
            credential_id: Uuid::new_v4(),
            original_intent: "orphan recovery".to_string(),
            started_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(30),
            metadata: metadata_to_json(None),
        })
        .await
        .expect("orphan session should persist");

    let affected = repo_after_restart
        .reconcile_orphaned_active_sessions("integration_test_recovery")
        .await
        .expect("reconcile should succeed");
    assert!(affected >= 1);

    let reconciled = repo_after_restart
        .get_session_by_id(tenant_id, orphan_session_id)
        .await
        .expect("query reconciled session should succeed")
        .expect("reconciled session should exist");
    assert_eq!(reconciled.status, "expired");
    assert!(reconciled.terminated_at.is_some());

    // Cleanup
    let _ = sqlx::query("DELETE FROM sandbox_operations WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM sandbox_sessions WHERE tenant_id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await;
}
