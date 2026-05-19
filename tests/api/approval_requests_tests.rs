#![allow(clippy::field_reassign_with_default)]
#![allow(dead_code)]
#![allow(clippy::uninlined_format_args)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use uuid::Uuid;

use vault_service::api::{
    approvals::{ApprovalApiState, approval_routes},
    middleware::{TokenScope, ValidatedToken},
};
use vault_service::services::db::{
    DatabaseConfig, DatabasePool, ensure_required_tables_on_startup,
};

fn create_admin_token() -> ValidatedToken {
    let tenant_id = Uuid::nil().to_string();
    let user_id = "00000000-0000-0000-0000-000000000001".to_string();

    ValidatedToken {
        token_id: Uuid::now_v7().to_string(),
        subject: format!("{tenant_id}:{user_id}"),
        tenant_id,
        user_id,
        expires_at: u64::MAX,
        scopes: vec![TokenScope::TenantAdmin],
        issued_at: 1000,
        membership_id: None,
        metadata: std::collections::HashMap::new(),
        subject_type: vault_service::token::TOKEN_SUBJECT_TYPE_USER.to_string(),
        issued_from: vault_service::token::TOKEN_ISSUED_FROM_SESSION.to_string(),
        token_plane: "management".to_string(),
        allowed_credential_ids: None,
        allowed_binding_handles: None,
    }
}

async fn read_json(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&bytes).expect("response body should be valid json")
}

async fn approval_request_count(pool: &sqlx::PgPool) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM approval_requests")
        .fetch_one(pool)
        .await
        .expect("approval request count query should succeed")
}

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

async fn create_temp_database(
    admin_pool: &sqlx::PgPool,
    database_name: &str,
) -> Result<(), sqlx::Error> {
    let create_sql = format!("CREATE DATABASE \"{database_name}\"");
    sqlx::query(&create_sql).execute(admin_pool).await?;
    Ok(())
}

async fn drop_temp_database(admin_pool: &sqlx::PgPool, database_name: &str) {
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

async fn setup_postgres_backed_approval_app()
-> Option<(axum::Router, sqlx::PgPool, sqlx::PgPool, String)> {
    let base_database_url = test_database_url()?;
    let admin_database_url = postgres_admin_url(&base_database_url)?;

    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&admin_database_url)
        .await
        .ok()?;

    let database_name = format!(
        "credbridge_approvals_{}",
        &Uuid::new_v4().simple().to_string()[..12]
    );
    create_temp_database(&admin_pool, &database_name)
        .await
        .ok()?;

    let temp_url = temp_database_url(&base_database_url, &database_name)?;
    let config = DatabaseConfig {
        url: temp_url,
        max_connections: 5,
        min_connections: 1,
        connect_timeout: 10,
        idle_timeout: 60,
    };
    let database_pool = DatabasePool::new(config).await.ok()?;
    ensure_required_tables_on_startup(&database_pool)
        .await
        .ok()?;

    let app = axum::Router::new()
        .nest(
            "/api/v1",
            approval_routes(ApprovalApiState::new(database_pool.pool().clone())),
        )
        .layer(axum::Extension(create_admin_token()));

    Some((app, database_pool.pool().clone(), admin_pool, database_name))
}

#[tokio::test]
async fn approval_async_initiation_creates_pending_request_and_returns_approval_id() {
    let Some((app, pool, admin_pool, database_name)) = setup_postgres_backed_approval_app().await
    else {
        eprintln!("skip approval postgres test: TEST_DATABASE_URL/DATABASE_URL not configured");
        return;
    };

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/approvals")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "business_type": "oauth_binding_access",
                "business_id": "binding_lark_app_primary"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert!(
        response.status() == StatusCode::OK || response.status() == StatusCode::CREATED,
        "expected 200 or 201, got {}",
        response.status()
    );

    let body = read_json(response).await;
    assert_eq!(body["success"], true);
    assert!(
        body["data"]["approval_id"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    assert_eq!(body["data"]["status"], "pending");

    pool.close().await;
    admin_pool.close().await;
    drop_temp_database(&admin_pool, &database_name).await;
}

#[tokio::test]
async fn approval_async_initiation_rejects_invalid_business_type_without_persisting() {
    let Some((app, pool, admin_pool, database_name)) = setup_postgres_backed_approval_app().await
    else {
        eprintln!("skip approval postgres test: TEST_DATABASE_URL/DATABASE_URL not configured");
        return;
    };

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/approvals")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "business_type": "invalid_business_type",
                "business_id": "binding_lark_app_primary"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = read_json(response).await;
    assert_eq!(body["success"], false);
    assert_eq!(body["error"], "invalid_request");
    assert!(
        body["message"]
            .as_str()
            .is_some_and(|message| message.contains("business_type"))
    );

    assert_eq!(approval_request_count(&pool).await, 0);

    pool.close().await;
    admin_pool.close().await;
    drop_temp_database(&admin_pool, &database_name).await;
}
