#![allow(clippy::field_reassign_with_default)]
#![allow(dead_code)]
#![allow(clippy::uninlined_format_args)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use sqlx::types::Json as SqlxJson;
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
    create_admin_token_for_tenant(
        Uuid::nil(),
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
    )
}

fn create_admin_token_for_tenant(tenant_id: Uuid, user_id: Uuid) -> ValidatedToken {
    ValidatedToken {
        token_id: Uuid::now_v7().to_string(),
        subject: format!("{tenant_id}:{user_id}"),
        tenant_id: tenant_id.to_string(),
        user_id: user_id.to_string(),
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

async fn create_pending_approval_request(app: &axum::Router) -> Value {
    create_pending_approval_request_for_business_id(app, "binding_lark_app_primary").await
}

async fn create_pending_approval_request_for_business_id(
    app: &axum::Router,
    business_id: &str,
) -> Value {
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/approvals")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "business_type": "oauth_binding_access",
                "business_id": business_id
            })
            .to_string(),
        ))
        .unwrap();

    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("pending approval request should be created");

    assert!(
        response.status() == StatusCode::OK || response.status() == StatusCode::CREATED,
        "expected 200 or 201, got {}",
        response.status()
    );

    read_json(response).await
}

async fn approval_request_snapshot(
    pool: &sqlx::PgPool,
    approval_id: &str,
) -> (String, Option<String>) {
    sqlx::query_as::<_, (String, Option<String>)>(
        r#"
        SELECT status, remark
        FROM approval_requests
        WHERE approval_id = $1
        "#,
    )
    .bind(Uuid::parse_str(approval_id).expect("approval id should be valid"))
    .fetch_one(pool)
    .await
    .expect("approval request snapshot query should succeed")
}

async fn approval_business_result_snapshot(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    business_type: &str,
    business_id: &str,
) -> (String, Option<String>) {
    sqlx::query_as::<_, (String, Option<String>)>(
        r#"
        SELECT status, result_code
        FROM approval_business_results
        WHERE tenant_id = $1 AND business_type = $2 AND business_id = $3
        "#,
    )
    .bind(tenant_id)
    .bind(business_type)
    .bind(business_id)
    .fetch_one(pool)
    .await
    .expect("approval business result query should succeed")
}

async fn approval_audit_event_snapshots(
    pool: &sqlx::PgPool,
    approval_id: &str,
) -> Vec<(String, String, Value)> {
    sqlx::query_as::<_, (String, String, SqlxJson<Value>)>(
        r#"
        SELECT action, outcome, metadata
        FROM approval_audit_events
        WHERE approval_id = $1
        ORDER BY created_at ASC, audit_id ASC
        "#,
    )
    .bind(Uuid::parse_str(approval_id).expect("approval id should be valid"))
    .fetch_all(pool)
    .await
    .expect("approval audit event query should succeed")
    .into_iter()
    .map(|(action, outcome, metadata)| (action, outcome, metadata.0))
    .collect()
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
async fn approval_async_initiation_reuses_existing_pending_request_and_audits_replay() {
    let Some((app, pool, admin_pool, database_name)) = setup_postgres_backed_approval_app().await
    else {
        eprintln!("skip approval postgres test: TEST_DATABASE_URL/DATABASE_URL not configured");
        return;
    };

    let first = create_pending_approval_request_for_business_id(&app, "binding_retry_same").await;
    let second = create_pending_approval_request_for_business_id(&app, "binding_retry_same").await;

    let first_approval_id = first["data"]["approval_id"]
        .as_str()
        .expect("first approval id should be returned")
        .to_string();
    let second_approval_id = second["data"]["approval_id"]
        .as_str()
        .expect("second approval id should be returned")
        .to_string();

    assert_eq!(second["data"]["status"], "pending");
    assert_eq!(second_approval_id, first_approval_id);
    assert_eq!(approval_request_count(&pool).await, 1);

    let audit_events = approval_audit_event_snapshots(&pool, &first_approval_id).await;
    assert_eq!(audit_events.len(), 2);
    assert_eq!(audit_events[0].0, "initiated");
    assert_eq!(audit_events[0].1, "success");
    assert_eq!(audit_events[1].0, "initiation_replayed");
    assert_eq!(audit_events[1].1, "replayed");

    pool.close().await;
    admin_pool.close().await;
    drop_temp_database(&admin_pool, &database_name).await;
}

#[tokio::test]
async fn approval_reject_requires_remark_and_persists_rejection_reason() {
    let Some((app, pool, admin_pool, database_name)) = setup_postgres_backed_approval_app().await
    else {
        eprintln!("skip approval postgres test: TEST_DATABASE_URL/DATABASE_URL not configured");
        return;
    };

    let created = create_pending_approval_request(&app).await;
    let approval_id = created["data"]["approval_id"]
        .as_str()
        .expect("approval id should be returned")
        .to_string();

    let missing_reason_request = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/approvals/{approval_id}/reject"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({}).to_string()))
        .unwrap();
    let missing_reason_response = app.clone().oneshot(missing_reason_request).await.unwrap();

    assert_eq!(missing_reason_response.status(), StatusCode::BAD_REQUEST);

    let missing_reason_body = read_json(missing_reason_response).await;
    assert_eq!(missing_reason_body["success"], false);
    assert_eq!(missing_reason_body["error"], "invalid_request");

    let reject_request = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/approvals/{approval_id}/reject"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "remark": "binding owner denied access"
            })
            .to_string(),
        ))
        .unwrap();
    let reject_response = app.clone().oneshot(reject_request).await.unwrap();

    assert_eq!(reject_response.status(), StatusCode::OK);

    let reject_body = read_json(reject_response).await;
    assert_eq!(reject_body["data"]["status"], "rejected");
    assert_eq!(reject_body["data"]["remark"], "binding owner denied access");

    let request_snapshot = approval_request_snapshot(&pool, &approval_id).await;
    assert_eq!(request_snapshot.0, "rejected");
    assert_eq!(
        request_snapshot.1.as_deref(),
        Some("binding owner denied access")
    );

    let business_result = approval_business_result_snapshot(
        &pool,
        Uuid::nil(),
        "oauth_binding_access",
        "binding_lark_app_primary",
    )
    .await;
    assert_eq!(business_result.0, "rejected");
    assert_eq!(business_result.1.as_deref(), Some("rejected"));

    pool.close().await;
    admin_pool.close().await;
    drop_temp_database(&admin_pool, &database_name).await;
}

#[tokio::test]
async fn approval_repeat_same_terminal_is_idempotent_and_conflicting_terminal_is_rejected() {
    let Some((app, pool, admin_pool, database_name)) = setup_postgres_backed_approval_app().await
    else {
        eprintln!("skip approval postgres test: TEST_DATABASE_URL/DATABASE_URL not configured");
        return;
    };

    let created = create_pending_approval_request(&app).await;
    let approval_id = created["data"]["approval_id"]
        .as_str()
        .expect("approval id should be returned")
        .to_string();

    let first_approve = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/approvals/{approval_id}/approve"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "remark": "first approval wins"
            })
            .to_string(),
        ))
        .unwrap();
    let first_response = app.clone().oneshot(first_approve).await.unwrap();
    assert_eq!(first_response.status(), StatusCode::OK);

    let repeat_approve = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/approvals/{approval_id}/approve"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "remark": "second approval should be ignored"
            })
            .to_string(),
        ))
        .unwrap();
    let repeat_response = app.clone().oneshot(repeat_approve).await.unwrap();
    assert_eq!(repeat_response.status(), StatusCode::OK);

    let repeat_body = read_json(repeat_response).await;
    assert_eq!(repeat_body["data"]["status"], "approved");
    assert_eq!(repeat_body["data"]["remark"], "first approval wins");

    let conflicting_reject = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/approvals/{approval_id}/reject"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "remark": "too late to reject"
            })
            .to_string(),
        ))
        .unwrap();
    let conflicting_response = app.clone().oneshot(conflicting_reject).await.unwrap();
    assert_eq!(conflicting_response.status(), StatusCode::CONFLICT);

    let conflict_body = read_json(conflicting_response).await;
    assert_eq!(conflict_body["success"], false);
    assert_eq!(conflict_body["error"], "conflict");

    let request_snapshot = approval_request_snapshot(&pool, &approval_id).await;
    assert_eq!(request_snapshot.0, "approved");
    assert_eq!(request_snapshot.1.as_deref(), Some("first approval wins"));

    let audit_events = approval_audit_event_snapshots(&pool, &approval_id).await;
    assert_eq!(audit_events.len(), 3);
    assert_eq!(audit_events[0].0, "initiated");
    assert_eq!(audit_events[1].0, "approved");
    assert_eq!(audit_events[1].1, "success");
    assert_eq!(audit_events[2].0, "approved_replayed");
    assert_eq!(audit_events[2].1, "replayed");

    pool.close().await;
    admin_pool.close().await;
    drop_temp_database(&admin_pool, &database_name).await;
}

#[tokio::test]
async fn approval_cancel_updates_terminal_state_and_business_writeback() {
    let Some((app, pool, admin_pool, database_name)) = setup_postgres_backed_approval_app().await
    else {
        eprintln!("skip approval postgres test: TEST_DATABASE_URL/DATABASE_URL not configured");
        return;
    };

    let created = create_pending_approval_request(&app).await;
    let approval_id = created["data"]["approval_id"]
        .as_str()
        .expect("approval id should be returned")
        .to_string();

    let cancel_request = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/approvals/{approval_id}/cancel"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "remark": "request withdrawn by operator"
            })
            .to_string(),
        ))
        .unwrap();
    let cancel_response = app.clone().oneshot(cancel_request).await.unwrap();

    assert_eq!(cancel_response.status(), StatusCode::OK);

    let cancel_body = read_json(cancel_response).await;
    assert_eq!(cancel_body["data"]["status"], "cancelled");
    assert_eq!(
        cancel_body["data"]["remark"],
        "request withdrawn by operator"
    );

    let request_snapshot = approval_request_snapshot(&pool, &approval_id).await;
    assert_eq!(request_snapshot.0, "cancelled");
    assert_eq!(
        request_snapshot.1.as_deref(),
        Some("request withdrawn by operator")
    );

    let business_result = approval_business_result_snapshot(
        &pool,
        Uuid::nil(),
        "oauth_binding_access",
        "binding_lark_app_primary",
    )
    .await;
    assert_eq!(business_result.0, "cancelled");
    assert_eq!(business_result.1.as_deref(), Some("cancelled"));

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

#[tokio::test]
async fn approval_list_returns_pending_and_terminal_requests_with_pending_first() {
    let Some((app, pool, admin_pool, database_name)) = setup_postgres_backed_approval_app().await
    else {
        eprintln!("skip approval postgres test: TEST_DATABASE_URL/DATABASE_URL not configured");
        return;
    };

    let pending = create_pending_approval_request_for_business_id(&app, "binding_pending").await;
    let pending_id = pending["data"]["approval_id"]
        .as_str()
        .expect("approval id should be returned")
        .to_string();

    let terminal = create_pending_approval_request_for_business_id(&app, "binding_terminal").await;
    let terminal_id = terminal["data"]["approval_id"]
        .as_str()
        .expect("approval id should be returned")
        .to_string();

    let approve_request = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/approvals/{terminal_id}/approve"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "remark": "approved for dashboard list"
            })
            .to_string(),
        ))
        .unwrap();
    let approve_response = app.clone().oneshot(approve_request).await.unwrap();
    assert_eq!(approve_response.status(), StatusCode::OK);

    let list_request = Request::builder()
        .method("GET")
        .uri("/api/v1/approvals")
        .body(Body::empty())
        .unwrap();
    let list_response = app.clone().oneshot(list_request).await.unwrap();

    assert_eq!(list_response.status(), StatusCode::OK);

    let list_body = read_json(list_response).await;
    assert_eq!(list_body["success"], true);
    assert_eq!(list_body["data"]["total"], 2);
    assert_eq!(list_body["data"]["page"], 1);
    assert_eq!(list_body["data"]["page_size"], 20);
    assert_eq!(list_body["data"]["total_pages"], 1);

    let items = list_body["data"]["items"]
        .as_array()
        .expect("approval list items should be an array");
    assert_eq!(items.len(), 2);

    assert_eq!(items[0]["approval_id"], pending_id);
    assert_eq!(items[0]["business_id"], "binding_pending");
    assert_eq!(items[0]["status"], "pending");

    assert_eq!(items[1]["approval_id"], terminal_id);
    assert_eq!(items[1]["business_id"], "binding_terminal");
    assert_eq!(items[1]["status"], "approved");
    assert_eq!(items[1]["remark"], "approved for dashboard list");

    pool.close().await;
    admin_pool.close().await;
    drop_temp_database(&admin_pool, &database_name).await;
}

#[tokio::test]
async fn approval_list_supports_status_filter_and_pagination() {
    let Some((app, pool, admin_pool, database_name)) = setup_postgres_backed_approval_app().await
    else {
        eprintln!("skip approval postgres test: TEST_DATABASE_URL/DATABASE_URL not configured");
        return;
    };

    let pending = create_pending_approval_request_for_business_id(&app, "binding_pending").await;
    let pending_id = pending["data"]["approval_id"]
        .as_str()
        .expect("approval id should be returned")
        .to_string();

    let approved = create_pending_approval_request_for_business_id(&app, "binding_approved").await;
    let approved_id = approved["data"]["approval_id"]
        .as_str()
        .expect("approval id should be returned")
        .to_string();

    let rejected = create_pending_approval_request_for_business_id(&app, "binding_rejected").await;
    let rejected_id = rejected["data"]["approval_id"]
        .as_str()
        .expect("approval id should be returned")
        .to_string();

    let cancelled =
        create_pending_approval_request_for_business_id(&app, "binding_cancelled").await;
    let cancelled_id = cancelled["data"]["approval_id"]
        .as_str()
        .expect("approval id should be returned")
        .to_string();

    let approve_request = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/approvals/{approved_id}/approve"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "remark": "approved for list filter"
            })
            .to_string(),
        ))
        .unwrap();
    let approve_response = app.clone().oneshot(approve_request).await.unwrap();
    assert_eq!(approve_response.status(), StatusCode::OK);

    let reject_request = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/approvals/{rejected_id}/reject"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "remark": "rejected for list filter"
            })
            .to_string(),
        ))
        .unwrap();
    let reject_response = app.clone().oneshot(reject_request).await.unwrap();
    assert_eq!(reject_response.status(), StatusCode::OK);

    let cancel_request = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/approvals/{cancelled_id}/cancel"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "remark": "cancelled for list filter"
            })
            .to_string(),
        ))
        .unwrap();
    let cancel_response = app.clone().oneshot(cancel_request).await.unwrap();
    assert_eq!(cancel_response.status(), StatusCode::OK);

    let filtered_request = Request::builder()
        .method("GET")
        .uri("/api/v1/approvals?status=approved&page=1&page_size=1")
        .body(Body::empty())
        .unwrap();
    let filtered_response = app.clone().oneshot(filtered_request).await.unwrap();
    assert_eq!(filtered_response.status(), StatusCode::OK);

    let filtered_body = read_json(filtered_response).await;
    assert_eq!(filtered_body["success"], true);
    assert_eq!(filtered_body["data"]["total"], 1);
    assert_eq!(filtered_body["data"]["page"], 1);
    assert_eq!(filtered_body["data"]["page_size"], 1);
    assert_eq!(filtered_body["data"]["total_pages"], 1);
    assert_eq!(filtered_body["data"]["counts"]["pending"], 1);
    assert_eq!(filtered_body["data"]["counts"]["approved"], 1);
    assert_eq!(filtered_body["data"]["counts"]["rejected"], 1);
    assert_eq!(filtered_body["data"]["counts"]["cancelled"], 1);
    assert_eq!(filtered_body["data"]["counts"]["completed"], 3);

    let filtered_items = filtered_body["data"]["items"]
        .as_array()
        .expect("filtered approval list items should be an array");
    assert_eq!(filtered_items.len(), 1);
    assert_eq!(filtered_items[0]["approval_id"], approved_id);
    assert_eq!(filtered_items[0]["status"], "approved");

    let paged_request = Request::builder()
        .method("GET")
        .uri("/api/v1/approvals?page=2&page_size=1")
        .body(Body::empty())
        .unwrap();
    let paged_response = app.clone().oneshot(paged_request).await.unwrap();
    assert_eq!(paged_response.status(), StatusCode::OK);

    let paged_body = read_json(paged_response).await;
    assert_eq!(paged_body["success"], true);
    assert_eq!(paged_body["data"]["total"], 4);
    assert_eq!(paged_body["data"]["page"], 2);
    assert_eq!(paged_body["data"]["page_size"], 1);
    assert_eq!(paged_body["data"]["total_pages"], 4);

    let paged_items = paged_body["data"]["items"]
        .as_array()
        .expect("paged approval list items should be an array");
    assert_eq!(paged_items.len(), 1);
    assert_ne!(paged_items[0]["status"], "pending");

    let pending_request = Request::builder()
        .method("GET")
        .uri("/api/v1/approvals?status=pending")
        .body(Body::empty())
        .unwrap();
    let pending_response = app.clone().oneshot(pending_request).await.unwrap();
    assert_eq!(pending_response.status(), StatusCode::OK);

    let pending_body = read_json(pending_response).await;
    let pending_items = pending_body["data"]["items"]
        .as_array()
        .expect("pending approval list items should be an array");
    assert_eq!(pending_body["data"]["total"], 1);
    assert_eq!(pending_items.len(), 1);
    assert_eq!(pending_items[0]["approval_id"], pending_id);
    assert_eq!(pending_items[0]["status"], "pending");

    let completed_request = Request::builder()
        .method("GET")
        .uri("/api/v1/approvals?status=completed&page=1&page_size=10")
        .body(Body::empty())
        .unwrap();
    let completed_response = app.clone().oneshot(completed_request).await.unwrap();
    assert_eq!(completed_response.status(), StatusCode::OK);

    let completed_body = read_json(completed_response).await;
    assert_eq!(completed_body["success"], true);
    assert_eq!(completed_body["data"]["total"], 3);
    assert_eq!(completed_body["data"]["page"], 1);
    assert_eq!(completed_body["data"]["page_size"], 10);
    assert_eq!(completed_body["data"]["total_pages"], 1);
    assert_eq!(completed_body["data"]["counts"]["pending"], 1);
    assert_eq!(completed_body["data"]["counts"]["approved"], 1);
    assert_eq!(completed_body["data"]["counts"]["rejected"], 1);
    assert_eq!(completed_body["data"]["counts"]["cancelled"], 1);
    assert_eq!(completed_body["data"]["counts"]["completed"], 3);

    let completed_items = completed_body["data"]["items"]
        .as_array()
        .expect("completed approval list items should be an array");
    assert_eq!(completed_items.len(), 3);
    let completed_statuses = completed_items
        .iter()
        .map(|item| {
            item["status"]
                .as_str()
                .expect("completed approval item status should be a string")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert!(completed_statuses.iter().any(|status| status == "approved"));
    assert!(completed_statuses.iter().any(|status| status == "rejected"));
    assert!(
        completed_statuses
            .iter()
            .any(|status| status == "cancelled")
    );

    pool.close().await;
    admin_pool.close().await;
    drop_temp_database(&admin_pool, &database_name).await;
}

#[tokio::test]
async fn approval_list_rejects_invalid_status_filter() {
    let Some((app, pool, admin_pool, database_name)) = setup_postgres_backed_approval_app().await
    else {
        eprintln!("skip approval postgres test: TEST_DATABASE_URL/DATABASE_URL not configured");
        return;
    };

    let request = Request::builder()
        .method("GET")
        .uri("/api/v1/approvals?status=unknown")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = read_json(response).await;
    assert_eq!(body["success"], false);
    assert_eq!(body["error"], "invalid_request");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("status must be one of")
    );

    pool.close().await;
    admin_pool.close().await;
    drop_temp_database(&admin_pool, &database_name).await;
}

#[tokio::test]
async fn approval_approve_updates_terminal_state_and_business_writeback() {
    let Some((app, pool, admin_pool, database_name)) = setup_postgres_backed_approval_app().await
    else {
        eprintln!("skip approval postgres test: TEST_DATABASE_URL/DATABASE_URL not configured");
        return;
    };

    let created = create_pending_approval_request(&app).await;
    let approval_id = created["data"]["approval_id"]
        .as_str()
        .expect("approval id should be returned")
        .to_string();

    let approve_request = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/approvals/{approval_id}/approve"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "remark": "approved by integration test"
            })
            .to_string(),
        ))
        .unwrap();

    let approve_response = app.clone().oneshot(approve_request).await.unwrap();

    assert_eq!(approve_response.status(), StatusCode::OK);

    let approve_body = read_json(approve_response).await;
    assert_eq!(approve_body["success"], true);
    assert_eq!(approve_body["data"]["approval_id"], approval_id);
    assert_eq!(approve_body["data"]["status"], "approved");
    assert_eq!(
        approve_body["data"]["remark"],
        "approved by integration test"
    );

    let status_request = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/approvals/{approval_id}"))
        .body(Body::empty())
        .unwrap();
    let status_response = app.clone().oneshot(status_request).await.unwrap();

    assert_eq!(status_response.status(), StatusCode::OK);

    let status_body = read_json(status_response).await;
    assert_eq!(status_body["success"], true);
    assert_eq!(status_body["data"]["approval_id"], approval_id);
    assert_eq!(status_body["data"]["status"], "approved");
    assert_eq!(
        status_body["data"]["remark"],
        "approved by integration test"
    );

    let request_snapshot = approval_request_snapshot(&pool, &approval_id).await;
    assert_eq!(request_snapshot.0, "approved");
    assert_eq!(
        request_snapshot.1.as_deref(),
        Some("approved by integration test")
    );

    let business_result = approval_business_result_snapshot(
        &pool,
        Uuid::nil(),
        "oauth_binding_access",
        "binding_lark_app_primary",
    )
    .await;
    assert_eq!(business_result.0, "approved");
    assert_eq!(business_result.1.as_deref(), Some("approved"));

    let audit_events = approval_audit_event_snapshots(&pool, &approval_id).await;
    assert_eq!(audit_events.len(), 2);
    assert_eq!(audit_events[0].0, "initiated");
    assert_eq!(audit_events[1].0, "approved");
    assert_eq!(audit_events[1].1, "success");
    assert_eq!(audit_events[1].2["status"], "approved");
    assert_eq!(audit_events[1].2["remark_present"], true);
    assert_eq!(audit_events[1].2["remark_redacted"], true);
    assert!(
        !audit_events[1]
            .2
            .to_string()
            .contains("approved by integration test"),
        "audit metadata should not expose raw approval remarks"
    );

    pool.close().await;
    admin_pool.close().await;
    drop_temp_database(&admin_pool, &database_name).await;
}

#[tokio::test]
async fn approval_business_writeback_is_scoped_by_tenant() {
    let Some((_, pool, admin_pool, database_name)) = setup_postgres_backed_approval_app().await
    else {
        eprintln!(
            "skip approval tenant-scoped writeback test: TEST_DATABASE_URL/DATABASE_URL not configured"
        );
        return;
    };

    let tenant_one = Uuid::nil();
    let tenant_two = Uuid::new_v4();
    let user_one = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let user_two = Uuid::new_v4();
    let tenant_one_app = axum::Router::new()
        .nest(
            "/api/v1",
            approval_routes(ApprovalApiState::new(pool.clone())),
        )
        .layer(axum::Extension(create_admin_token_for_tenant(
            tenant_one, user_one,
        )));
    let tenant_two_app = axum::Router::new()
        .nest(
            "/api/v1",
            approval_routes(ApprovalApiState::new(pool.clone())),
        )
        .layer(axum::Extension(create_admin_token_for_tenant(
            tenant_two, user_two,
        )));

    let create_request = |business_id: &str| {
        Request::builder()
            .method("POST")
            .uri("/api/v1/approvals")
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({
                    "business_type": "credential_runtime_access",
                    "business_id": business_id
                })
                .to_string(),
            ))
            .unwrap()
    };

    let tenant_one_created = read_json(
        tenant_one_app
            .clone()
            .oneshot(create_request("runtime-shared-request"))
            .await
            .unwrap(),
    )
    .await;
    let tenant_two_created = read_json(
        tenant_two_app
            .clone()
            .oneshot(create_request("runtime-shared-request"))
            .await
            .unwrap(),
    )
    .await;

    let tenant_one_approval_id = tenant_one_created["data"]["approval_id"]
        .as_str()
        .expect("tenant one approval id should be present");
    let tenant_two_approval_id = tenant_two_created["data"]["approval_id"]
        .as_str()
        .expect("tenant two approval id should be present");

    let tenant_one_approve = Request::builder()
        .method("POST")
        .uri(format!(
            "/api/v1/approvals/{tenant_one_approval_id}/approve"
        ))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "remark": "tenant one approved" }).to_string(),
        ))
        .unwrap();
    let tenant_two_reject = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/approvals/{tenant_two_approval_id}/reject"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "remark": "tenant two rejected" }).to_string(),
        ))
        .unwrap();

    let tenant_one_response = tenant_one_app
        .clone()
        .oneshot(tenant_one_approve)
        .await
        .unwrap();
    let tenant_two_response = tenant_two_app
        .clone()
        .oneshot(tenant_two_reject)
        .await
        .unwrap();
    assert_eq!(tenant_one_response.status(), StatusCode::OK);
    assert_eq!(tenant_two_response.status(), StatusCode::OK);

    let tenant_one_result = approval_business_result_snapshot(
        &pool,
        tenant_one,
        "credential_runtime_access",
        "runtime-shared-request",
    )
    .await;
    let tenant_two_result = approval_business_result_snapshot(
        &pool,
        tenant_two,
        "credential_runtime_access",
        "runtime-shared-request",
    )
    .await;
    assert_eq!(tenant_one_result.0, "approved");
    assert_eq!(tenant_two_result.0, "rejected");

    let result_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM approval_business_results
        WHERE business_type = 'credential_runtime_access'
          AND business_id = $1
        "#,
    )
    .bind("runtime-shared-request")
    .fetch_one(&pool)
    .await
    .expect("tenant-scoped approval business result count query should succeed");
    assert_eq!(result_count, 2);

    pool.close().await;
    admin_pool.close().await;
    drop_temp_database(&admin_pool, &database_name).await;
}

#[tokio::test]
async fn approval_transition_rolls_back_when_business_writeback_fails() {
    let Some((app, pool, admin_pool, database_name)) = setup_postgres_backed_approval_app().await
    else {
        eprintln!("skip approval postgres test: TEST_DATABASE_URL/DATABASE_URL not configured");
        return;
    };

    let created =
        create_pending_approval_request_for_business_id(&app, "binding_writeback_fail").await;
    let approval_id = created["data"]["approval_id"]
        .as_str()
        .expect("approval id should be returned")
        .to_string();

    sqlx::query("DROP TABLE approval_business_results")
        .execute(&pool)
        .await
        .expect("dropping approval_business_results should succeed");

    let approve_request = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/approvals/{approval_id}/approve"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "remark": "retryable writeback failure"
            })
            .to_string(),
        ))
        .unwrap();

    let approve_response = app.clone().oneshot(approve_request).await.unwrap();
    assert_eq!(approve_response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let approve_body = read_json(approve_response).await;
    assert_eq!(approve_body["success"], false);
    assert_eq!(approve_body["error"], "internal_error");

    let request_snapshot = approval_request_snapshot(&pool, &approval_id).await;
    assert_eq!(request_snapshot.0, "pending");
    assert_eq!(request_snapshot.1, None);

    let audit_events = approval_audit_event_snapshots(&pool, &approval_id).await;
    assert_eq!(audit_events.len(), 1);
    assert_eq!(audit_events[0].0, "initiated");
    assert_eq!(audit_events[0].1, "success");

    pool.close().await;
    admin_pool.close().await;
    drop_temp_database(&admin_pool, &database_name).await;
}
