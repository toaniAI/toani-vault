#![allow(clippy::field_reassign_with_default)]
#![allow(dead_code)]
#![allow(clippy::uninlined_format_args)]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use tokio::sync::{Mutex, RwLock};
use tower::ServiceExt;
use uuid::Uuid;

use vault_service::api::audit::MemoryAuditStorageAdapter;
use vault_service::api::credentials::{
    AppState as CredentialAppState, AuditLogger, DefaultAuditLogger, routes as credential_routes,
};
use vault_service::api::middleware::{TokenScope, ValidatedToken};
use vault_service::api::oauth_broker::{
    OAuthBrokerApiState, oauth_broker_public_routes, oauth_broker_routes,
};
use vault_service::audit::MemoryAuditStorage;
use vault_service::crypto::hkdf::KeyHierarchy;
use vault_service::crypto::keys::HardwareRootKey;
use vault_service::services::db::{
    DatabaseConfig, DatabasePool, ensure_required_tables_on_startup,
};
use vault_service::tee::{Enclave, EnclaveConfig};
use vault_service::vault::storage::CredentialVault;

fn create_admin_token() -> ValidatedToken {
    create_admin_token_for(
        Uuid::nil(),
        "00000000-0000-0000-0000-000000000001".to_string(),
    )
}

fn create_admin_token_for(tenant_id: Uuid, user_id: String) -> ValidatedToken {
    let tenant_id = tenant_id.to_string();
    ValidatedToken {
        token_id: Uuid::now_v7().to_string(),
        subject: format!("{tenant_id}:{user_id}"),
        tenant_id,
        user_id,
        expires_at: u64::MAX,
        scopes: vec![
            TokenScope::TenantAdmin,
            TokenScope::CredentialRead,
            TokenScope::CredentialWrite,
        ],
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

fn create_runtime_token(binding_handle: &str) -> ValidatedToken {
    let user_id = "00000000-0000-0000-0000-000000000001";
    let tenant_id = Uuid::nil().to_string();
    ValidatedToken {
        token_id: Uuid::now_v7().to_string(),
        subject: format!("{tenant_id}:{user_id}"),
        tenant_id,
        user_id: user_id.to_string(),
        expires_at: u64::MAX,
        scopes: vec![TokenScope::CredentialRead, TokenScope::CredentialDecrypt],
        issued_at: 1000,
        membership_id: None,
        metadata: std::collections::HashMap::new(),
        subject_type: vault_service::token::TOKEN_SUBJECT_TYPE_USER.to_string(),
        issued_from: vault_service::token::TOKEN_ISSUED_FROM_ACCESS_TOKEN.to_string(),
        token_plane: "runtime".to_string(),
        allowed_credential_ids: None,
        allowed_binding_handles: Some(vec![binding_handle.to_string()]),
    }
}

fn create_credential_state() -> CredentialAppState {
    let vault = CredentialVault::new_in_memory();
    let mut hierarchy = KeyHierarchy::new();
    let l0 = HardwareRootKey::for_simulation().expect("failed to create simulation hardware root");
    hierarchy
        .initialize_master_key(&l0)
        .expect("failed to initialize test key hierarchy");

    let mut enclave = Enclave::new(EnclaveConfig {
        debug_mode: true,
        ..Default::default()
    });
    enclave
        .initialize()
        .expect("failed to initialize test enclave");

    let audit_logger: Arc<dyn AuditLogger> = Arc::new(DefaultAuditLogger);

    CredentialAppState {
        vault: Arc::new(vault),
        key_hierarchy: Arc::new(RwLock::new(hierarchy)),
        enclave: Arc::new(Mutex::new(enclave)),
        audit_logger,
    }
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

fn is_valid_pkce_code_verifier(value: &str) -> bool {
    (43..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~'))
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

async fn setup_postgres_backed_app() -> Option<(Router, sqlx::PgPool, sqlx::PgPool)> {
    let base_database_url = test_database_url()?;
    let admin_database_url = postgres_admin_url(&base_database_url)?;

    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&admin_database_url)
        .await
        .ok()?;

    let database_name = format!(
        "credbridge_lark_delegated_binding_{}",
        &Uuid::new_v4().simple().to_string()[..12]
    );
    create_temp_database(&admin_pool, &database_name)
        .await
        .ok()?;

    let temp_database_url = temp_database_url(&base_database_url, &database_name)?;
    let database_config = DatabaseConfig {
        url: temp_database_url,
        max_connections: 5,
        min_connections: 1,
        connect_timeout: 5,
        idle_timeout: 60,
    };

    let database_pool = DatabasePool::new(database_config).await.ok()?;
    ensure_required_tables_on_startup(&database_pool)
        .await
        .ok()?;

    let tenant_id = Uuid::nil();
    let user_id = Uuid::parse_str(create_admin_token().user_id.as_str()).ok()?;

    sqlx::query(
        r#"
        INSERT INTO public.tenants (id, name, description, status, config)
        VALUES ($1, $2, $3, 'active', '{}'::jsonb)
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(tenant_id)
    .bind("lark-delegated-binding-test")
    .bind("lark delegated binding test tenant")
    .execute(database_pool.pool())
    .await
    .ok()?;

    sqlx::query(
        r#"
        INSERT INTO public.users (id, status, display_name, default_tenant_id, onboarding_completed)
        VALUES ($1, 'active', $2, $3, true)
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(user_id)
    .bind("Lark Delegated Binding Test User")
    .bind(tenant_id)
    .execute(database_pool.pool())
    .await
    .ok()?;

    let credential_state = create_credential_state();
    let shared_audit_storage =
        Arc::new(tokio::sync::Mutex::new(MemoryAuditStorage::new(256).ok()?));
    let broker_state = OAuthBrokerApiState::new(Arc::new(
        vault_service::oauth_broker::PgOAuthBrokerService::new(database_pool.pool().clone()),
    ))
    .with_audit_storage(Arc::new(MemoryAuditStorageAdapter::from_shared_storage(
        shared_audit_storage,
    )))
    .with_credential_runtime(
        credential_state.vault.clone(),
        credential_state.key_hierarchy.clone(),
        credential_state.enclave.clone(),
    );

    let app = Router::new()
        .merge(credential_routes().with_state(credential_state))
        .merge(oauth_broker_routes(broker_state.clone()))
        .merge(oauth_broker_public_routes(broker_state));

    Some((app, database_pool.pool().clone(), admin_pool))
}

async fn read_json(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&bytes).expect("response body should be valid json")
}

async fn create_lark_app_secret_credential(
    app: Router,
    service_id: &str,
    app_id: &str,
    app_secret: &str,
) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/credentials")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "service_id": service_id,
                "credential_type": "api_key",
                "plaintext_data": {
                    "app_id": app_id,
                    "app_secret": app_secret
                }
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = read_json(response).await;
    body["credential_id"]
        .as_str()
        .expect("credential id should be present")
        .to_string()
}

async fn list_credentials_total(app: Router) -> u64 {
    let request = Request::builder()
        .method("GET")
        .uri("/credentials?page=1&page_size=100")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = read_json(response).await;
    body["total"]
        .as_u64()
        .expect("credential total should be present")
}

#[tokio::test]
async fn delegated_auth_transaction_start_persists_pkce_state_and_authorization_url() {
    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        eprintln!("skip lark delegated binding transaction test: database not available");
        return;
    };

    let app = app.layer(axum::Extension(create_admin_token()));
    let backing_credential_id = create_lark_app_secret_credential(
        app.clone(),
        "lark-delegated-client",
        "cli_delegated_123",
        "delegated_secret_mock",
    )
    .await;

    let create_provider_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "lark-delegated-user",
                "display_name": "Lark Delegated User",
                "provider_family": "lark_delegated",
                "binding_kind": "delegated_user",
                "grant_family": "authorization_code_pkce",
                "authorization_endpoint": "https://open.feishu.cn/open-apis/authen/v1/authorize",
                "token_endpoint": "https://open.feishu.cn/open-apis/authen/v1/oidc/access_token",
                "client_id": "cli_delegated_123",
                "client_auth_method": "none",
                "callback_mode": "platform",
                "adapter_key": "lark_openapi",
                "adapter_version": "v1",
                "scope_template": ["contact:user.id:readonly"],
                "runtime_config": {
                    "redirect_uri": "https://credbridge.dev/oauth/callback/lark",
                    "audience": "open.feishu.cn",
                    "token_exchange": {
                        "credential_client_id_field": "app_id",
                        "credential_client_secret_field": "app_secret",
                        "request_client_id_field": "app_id",
                        "request_client_secret_field": "app_secret"
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();
    let provider_response = app.clone().oneshot(create_provider_request).await.unwrap();
    assert_eq!(provider_response.status(), StatusCode::OK);
    let provider_created = read_json(provider_response).await;
    let provider_definition_id = provider_created["data"]["id"]
        .as_str()
        .expect("provider id should be present");

    let start_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/transactions/start")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "provider_definition_id": provider_definition_id,
                "alias": "lark-delegated-alice",
                "purpose": "Lark delegated runtime",
                "subject_ref_hint": "alice@example.com",
                "subject_display_name": "Alice",
                "backing_credential_id": backing_credential_id,
                "scopes": ["contact:user.id:readonly"]
            })
            .to_string(),
        ))
        .unwrap();
    let start_response = app.clone().oneshot(start_request).await.unwrap();

    assert_eq!(start_response.status(), StatusCode::OK);
    let started = read_json(start_response).await;
    assert_eq!(started["success"], true);
    assert_eq!(started["data"]["status"], "pending");
    assert_eq!(
        started["data"]["provider_definition_id"],
        provider_definition_id
    );
    assert!(
        started["data"]["authorization_url"]
            .as_str()
            .unwrap_or_default()
            .contains("code_challenge=")
    );
    assert!(
        started["data"]["authorization_url"]
            .as_str()
            .unwrap_or_default()
            .contains("state=")
    );
    assert_eq!(started["data"]["code_challenge_method"], "S256");
    assert!(
        !started["data"]["state"]
            .as_str()
            .unwrap_or_default()
            .is_empty()
    );
    assert!(
        !started["data"]["transaction_id"]
            .as_str()
            .unwrap_or_default()
            .is_empty()
    );

    let transaction_id = started["data"]["transaction_id"]
        .as_str()
        .expect("transaction id should be present");
    let status_query: (String, String, String, String) = sqlx::query_as(
        r#"
        SELECT status, state, pkce_code_challenge, redirect_uri
        FROM public.oauth_auth_transactions
        WHERE id = $1
        "#,
    )
    .bind(Uuid::parse_str(transaction_id).unwrap())
    .fetch_one(&pool)
    .await
    .expect("transaction should persist");
    assert_eq!(status_query.0, "pending");
    assert!(!status_query.1.is_empty());
    assert!(!status_query.2.is_empty());
    assert_eq!(status_query.3, "https://credbridge.dev/oauth/callback/lark");

    let inspect_request = Request::builder()
        .method("GET")
        .uri(format!("/oauth-broker/transactions/{transaction_id}"))
        .body(Body::empty())
        .unwrap();
    let inspect_response = app.oneshot(inspect_request).await.unwrap();
    assert_eq!(inspect_response.status(), StatusCode::OK);
    let inspected = read_json(inspect_response).await;
    assert_eq!(inspected["success"], true);
    assert_eq!(inspected["data"]["transaction_id"], transaction_id);
    assert_eq!(inspected["data"]["status"], "pending");

    let db_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .expect("should read temp database name");
    pool.close().await;
    drop_temp_database(&admin_pool, &db_name).await;
    admin_pool.close().await;
}

#[tokio::test]
async fn delegated_callback_rejects_state_mismatch_before_code_exchange() {
    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        eprintln!("skip lark delegated callback mismatch test: database not available");
        return;
    };

    let app = app.layer(axum::Extension(create_admin_token()));
    let backing_credential_id = create_lark_app_secret_credential(
        app.clone(),
        "lark-delegated-client",
        "cli_delegated_123",
        "delegated_secret_mock",
    )
    .await;

    let create_provider_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "lark-delegated-user-mismatch",
                "display_name": "Lark Delegated User Mismatch",
                "provider_family": "lark_delegated",
                "binding_kind": "delegated_user",
                "grant_family": "authorization_code_pkce",
                "authorization_endpoint": "https://open.feishu.cn/open-apis/authen/v1/authorize",
                "token_endpoint": "https://open.feishu.cn/open-apis/authen/v1/oidc/access_token",
                "client_id": "cli_delegated_123",
                "client_auth_method": "none",
                "callback_mode": "platform",
                "adapter_key": "lark_openapi",
                "adapter_version": "v1",
                "scope_template": ["contact:user.id:readonly"],
                "runtime_config": {
                    "redirect_uri": "https://credbridge.dev/oauth/callback/lark",
                    "token_exchange": {
                        "credential_client_id_field": "app_id",
                        "credential_client_secret_field": "app_secret",
                        "request_client_id_field": "app_id",
                        "request_client_secret_field": "app_secret"
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();
    let provider_response = app.clone().oneshot(create_provider_request).await.unwrap();
    let provider_created = read_json(provider_response).await;
    let provider_definition_id = provider_created["data"]["id"]
        .as_str()
        .expect("provider id should be present");

    let start_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/transactions/start")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "provider_definition_id": provider_definition_id,
                "alias": "lark-delegated-alice",
                "purpose": "Lark delegated runtime",
                "subject_ref_hint": "alice@example.com",
                "subject_display_name": "Alice",
                "backing_credential_id": backing_credential_id,
                "scopes": ["contact:user.id:readonly"]
            })
            .to_string(),
        ))
        .unwrap();
    let start_response = app.clone().oneshot(start_request).await.unwrap();
    let started = read_json(start_response).await;

    let callback_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/oauth-broker/callback/lark?code=mock_code_123&state=bad-{}",
            started["data"]["state"].as_str().unwrap()
        ))
        .body(Body::empty())
        .unwrap();
    let callback_response = app.clone().oneshot(callback_request).await.unwrap();
    assert_eq!(callback_response.status(), StatusCode::BAD_REQUEST);
    let callback_body = read_json(callback_response).await;
    assert_eq!(callback_body["success"], false);
    assert_eq!(callback_body["error"], "invalid_request");
    assert!(
        callback_body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("state")
    );

    let db_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .expect("should read temp database name");
    pool.close().await;
    drop_temp_database(&admin_pool, &db_name).await;
    admin_pool.close().await;
}

#[tokio::test]
async fn delegated_callback_alias_route_rejects_state_mismatch_before_code_exchange() {
    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        eprintln!("skip lark delegated callback alias mismatch test: database not available");
        return;
    };

    let app = app.layer(axum::Extension(create_admin_token()));
    let backing_credential_id = create_lark_app_secret_credential(
        app.clone(),
        "lark-delegated-client",
        "cli_delegated_123",
        "delegated_secret_mock",
    )
    .await;

    let create_provider_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "lark-delegated-user-alias-mismatch",
                "display_name": "Lark Delegated User Alias Mismatch",
                "provider_family": "lark_delegated",
                "binding_kind": "delegated_user",
                "grant_family": "authorization_code_pkce",
                "authorization_endpoint": "https://open.feishu.cn/open-apis/authen/v1/authorize",
                "token_endpoint": "https://open.feishu.cn/open-apis/authen/v1/oidc/access_token",
                "client_id": "cli_delegated_123",
                "client_auth_method": "none",
                "callback_mode": "platform",
                "adapter_key": "lark_openapi",
                "adapter_version": "v1",
                "scope_template": ["contact:user.id:readonly"],
                "runtime_config": {
                    "redirect_uri": "https://credbridge.dev/oauth/callback/lark",
                    "token_exchange": {
                        "credential_client_id_field": "app_id",
                        "credential_client_secret_field": "app_secret",
                        "request_client_id_field": "app_id",
                        "request_client_secret_field": "app_secret"
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();
    let provider_response = app.clone().oneshot(create_provider_request).await.unwrap();
    let provider_created = read_json(provider_response).await;
    let provider_definition_id = provider_created["data"]["id"]
        .as_str()
        .expect("provider id should be present");

    let start_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/transactions/start")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "provider_definition_id": provider_definition_id,
                "alias": "lark-delegated-alias-route",
                "purpose": "Lark delegated runtime alias route",
                "subject_ref_hint": "alice@example.com",
                "subject_display_name": "Alice",
                "backing_credential_id": backing_credential_id,
                "scopes": ["contact:user.id:readonly"]
            })
            .to_string(),
        ))
        .unwrap();
    let start_response = app.clone().oneshot(start_request).await.unwrap();
    let started = read_json(start_response).await;

    let callback_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/oauth/callback/lark?code=mock_code_123&state=bad-{}",
            started["data"]["state"].as_str().unwrap()
        ))
        .body(Body::empty())
        .unwrap();
    let callback_response = app.clone().oneshot(callback_request).await.unwrap();
    assert_eq!(callback_response.status(), StatusCode::BAD_REQUEST);
    let callback_body = read_json(callback_response).await;
    assert_eq!(callback_body["success"], false);
    assert_eq!(callback_body["error"], "invalid_request");
    assert!(
        callback_body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("state")
    );

    let db_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .expect("should read temp database name");
    pool.close().await;
    drop_temp_database(&admin_pool, &db_name).await;
    admin_pool.close().await;
}

#[tokio::test]
async fn delegated_callback_rejects_replay_after_transaction_is_consumed() {
    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        eprintln!("skip lark delegated callback replay test: database not available");
        return;
    };

    let app = app.layer(axum::Extension(create_admin_token()));
    let backing_credential_id = create_lark_app_secret_credential(
        app.clone(),
        "lark-delegated-client",
        "cli_delegated_123",
        "delegated_secret_mock",
    )
    .await;

    let create_provider_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "lark-delegated-user-replay",
                "display_name": "Lark Delegated User Replay",
                "provider_family": "lark_delegated",
                "binding_kind": "delegated_user",
                "grant_family": "authorization_code_pkce",
                "authorization_endpoint": "https://open.feishu.cn/open-apis/authen/v1/authorize",
                "token_endpoint": "https://open.feishu.cn/open-apis/authen/v1/oidc/access_token",
                "client_id": "cli_delegated_123",
                "client_auth_method": "none",
                "callback_mode": "platform",
                "adapter_key": "lark_openapi",
                "adapter_version": "v1",
                "scope_template": ["contact:user.id:readonly"],
                "runtime_config": {
                    "redirect_uri": "https://credbridge.dev/oauth/callback/lark",
                    "token_exchange": {
                        "credential_client_id_field": "app_id",
                        "credential_client_secret_field": "app_secret",
                        "request_client_id_field": "app_id",
                        "request_client_secret_field": "app_secret"
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();
    let provider_response = app.clone().oneshot(create_provider_request).await.unwrap();
    let provider_created = read_json(provider_response).await;
    let provider_definition_id = provider_created["data"]["id"]
        .as_str()
        .expect("provider id should be present");

    let start_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/transactions/start")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "provider_definition_id": provider_definition_id,
                "alias": "lark-delegated-bob",
                "purpose": "Lark delegated replay test",
                "subject_ref_hint": "bob@example.com",
                "subject_display_name": "Bob",
                "backing_credential_id": backing_credential_id,
                "scopes": ["contact:user.id:readonly"]
            })
            .to_string(),
        ))
        .unwrap();
    let start_response = app.clone().oneshot(start_request).await.unwrap();
    let started = read_json(start_response).await;
    let transaction_id = started["data"]["transaction_id"]
        .as_str()
        .expect("transaction id should be present");

    sqlx::query(
        r#"
        UPDATE public.oauth_auth_transactions
        SET status = 'consumed',
            consumed_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(Uuid::parse_str(transaction_id).unwrap())
    .execute(&pool)
    .await
    .expect("should mark transaction consumed");

    let callback_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/oauth-broker/callback/lark?code=mock_code_123&state={}",
            started["data"]["state"].as_str().unwrap()
        ))
        .body(Body::empty())
        .unwrap();
    let callback_response = app.clone().oneshot(callback_request).await.unwrap();
    assert_eq!(callback_response.status(), StatusCode::CONFLICT);
    let callback_body = read_json(callback_response).await;
    assert_eq!(callback_body["success"], false);
    assert_eq!(callback_body["error"], "conflict");
    assert!(
        callback_body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("consumed")
    );

    let db_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .expect("should read temp database name");
    pool.close().await;
    drop_temp_database(&admin_pool, &db_name).await;
    admin_pool.close().await;
}

#[tokio::test]
async fn delegated_callback_cleans_up_stored_secret_when_binding_creation_fails() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock lark listener should bind");
    let addr = listener.local_addr().unwrap();
    let mock_app = Router::new().route(
        "/open-apis/authen/v1/oidc/access_token",
        axum::routing::post(|| async {
            axum::Json(json!({
                "code": 0,
                "msg": "ok",
                "data": {
                    "access_token": "uat_mock_123",
                    "refresh_token": "urt_mock_123",
                    "open_id": "ou_mock_alice",
                    "token_type": "Bearer",
                    "expires_in": 7200
                }
            }))
        }),
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        server.abort();
        eprintln!("skip lark delegated cleanup test: database not available");
        return;
    };

    let app = app.layer(axum::Extension(create_admin_token()));
    let backing_credential_id = create_lark_app_secret_credential(
        app.clone(),
        "lark-delegated-client-cleanup",
        "cli_delegated_cleanup",
        "delegated_secret_cleanup",
    )
    .await;

    let create_provider_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "lark-delegated-user-cleanup",
                "display_name": "Lark Delegated User Cleanup",
                "provider_family": "lark_delegated",
                "binding_kind": "delegated_user",
                "grant_family": "authorization_code_pkce",
                "authorization_endpoint": "https://open.feishu.cn/open-apis/authen/v1/authorize",
                "token_endpoint": format!("http://127.0.0.1:{}/open-apis/authen/v1/oidc/access_token", addr.port()),
                "client_id": "cli_delegated_cleanup",
                "client_auth_method": "none",
                "callback_mode": "platform",
                "adapter_key": "lark_openapi",
                "adapter_version": "v1",
                "scope_template": ["contact:user.id:readonly"],
                "runtime_config": {
                    "redirect_uri": "https://credbridge.dev/oauth/callback/lark",
                    "token_exchange": {
                        "credential_client_id_field": "app_id",
                        "credential_client_secret_field": "app_secret",
                        "request_client_id_field": "app_id",
                        "request_client_secret_field": "app_secret"
                    },
                    "audience": "open.feishu.cn"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let provider_response = app.clone().oneshot(create_provider_request).await.unwrap();
    let provider_created = read_json(provider_response).await;
    let provider_definition_id = provider_created["data"]["id"]
        .as_str()
        .expect("provider id should be present");

    let create_binding_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/bindings")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "provider_definition_id": provider_definition_id,
                "alias": "lark-delegated-cleanup",
                "purpose": "Existing binding to force alias collision",
                "subject_ref": "existing@example.com",
                "subject_display_name": "Existing",
                "backing_credential_id": backing_credential_id
            })
            .to_string(),
        ))
        .unwrap();
    let binding_response = app.clone().oneshot(create_binding_request).await.unwrap();
    assert_eq!(binding_response.status(), StatusCode::OK);

    let start_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/transactions/start")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "provider_definition_id": provider_definition_id,
                "alias": "lark-delegated-cleanup",
                "purpose": "Lark delegated cleanup",
                "subject_ref_hint": "alice@example.com",
                "subject_display_name": "Alice",
                "backing_credential_id": backing_credential_id,
                "scopes": ["contact:user.id:readonly"]
            })
            .to_string(),
        ))
        .unwrap();
    let start_response = app.clone().oneshot(start_request).await.unwrap();
    let started = read_json(start_response).await;
    let transaction_id = started["data"]["transaction_id"]
        .as_str()
        .expect("transaction id should be present")
        .to_string();
    let state = started["data"]["state"]
        .as_str()
        .expect("callback state should be present")
        .to_string();

    let credentials_before = list_credentials_total(app.clone()).await;

    let callback_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/oauth-broker/callback/lark?code=mock_code_123&state={state}"
        ))
        .body(Body::empty())
        .unwrap();
    let callback_response = app.clone().oneshot(callback_request).await.unwrap();
    assert_eq!(callback_response.status(), StatusCode::CONFLICT);
    let callback_body = read_json(callback_response).await;
    assert_eq!(callback_body["error"], "conflict");

    let credentials_after = list_credentials_total(app.clone()).await;
    assert_eq!(credentials_after, credentials_before);

    let status: String =
        sqlx::query_scalar(r#"SELECT status FROM public.oauth_auth_transactions WHERE id = $1"#)
            .bind(Uuid::parse_str(&transaction_id).unwrap())
            .fetch_one(&pool)
            .await
            .expect("transaction should remain available for retry");
    assert_eq!(status, "pending");

    let db_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .expect("should read temp database name");
    pool.close().await;
    drop_temp_database(&admin_pool, &db_name).await;
    admin_pool.close().await;
    server.abort();
}

#[tokio::test]
async fn delegated_callback_success_creates_ready_binding_and_replay_is_rejected() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock lark listener should bind");
    let addr = listener.local_addr().unwrap();
    let mock_app = Router::new()
        .route(
            "/open-apis/authen/v1/oidc/access_token",
            axum::routing::post(|axum::Json(payload): axum::Json<Value>| async move {
                assert_eq!(payload["app_id"], "cli_delegated_123");
                assert_eq!(payload["app_secret"], "delegated_secret_mock");
                assert_eq!(payload["grant_type"], "authorization_code");
                assert_eq!(payload["code"], "mock_code_123");
                assert_eq!(
                    payload["redirect_uri"],
                    "https://credbridge.dev/oauth/callback/lark"
                );
                assert!(
                    is_valid_pkce_code_verifier(
                        payload["code_verifier"]
                            .as_str()
                            .expect("code_verifier should be present")
                    ),
                    "code_verifier should satisfy RFC 7636"
                );
                axum::Json(json!({
                    "code": 0,
                    "msg": "ok",
                    "data": {
                        "access_token": "uat_mock_123",
                        "refresh_token": "urt_mock_123",
                        "open_id": "ou_mock_alice",
                        "token_type": "Bearer",
                        "expires_in": 7200
                    }
                }))
            }),
        )
        .route(
            "/open-apis/contact/v3/scopes",
            axum::routing::get(|| async {
                (
                    [(
                        axum::http::header::HeaderName::from_static("x-tt-logid"),
                        "lark_delegated_req_123",
                    )],
                    axum::Json(json!({
                        "code": 0,
                        "data": {
                            "scopes": ["contact:user.id:readonly"]
                        }
                    })),
                )
            }),
        );
    let server = tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        server.abort();
        eprintln!("skip lark delegated callback success test: database not available");
        return;
    };

    let app = app.layer(axum::Extension(create_admin_token()));
    let backing_credential_id = create_lark_app_secret_credential(
        app.clone(),
        "lark-delegated-client",
        "cli_delegated_123",
        "delegated_secret_mock",
    )
    .await;

    let create_provider_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "lark-delegated-user-success",
                "display_name": "Lark Delegated User Success",
                "provider_family": "lark_delegated",
                "binding_kind": "delegated_user",
                "grant_family": "authorization_code_pkce",
                "authorization_endpoint": format!("http://127.0.0.1:{}/open-apis/authen/v1/authorize", addr.port()),
                "token_endpoint": format!("http://127.0.0.1:{}/open-apis/authen/v1/oidc/access_token", addr.port()),
                "client_id": "cli_delegated_123",
                "client_auth_method": "none",
                "callback_mode": "platform",
                "adapter_key": "lark_openapi",
                "adapter_version": "v1",
                "scope_template": ["contact:user.id:readonly"],
                "runtime_config": {
                    "redirect_uri": "https://credbridge.dev/oauth/callback/lark",
                    "token_exchange": {
                        "credential_client_id_field": "app_id",
                        "credential_client_secret_field": "app_secret",
                        "request_client_id_field": "app_id",
                        "request_client_secret_field": "app_secret"
                    },
                    "base_url": format!("http://127.0.0.1:{}", addr.port()),
                    "allowed_domains": ["127.0.0.1"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": ["/open-apis/contact/v3/scopes"]
                }
            })
            .to_string(),
        ))
        .unwrap();
    let provider_response = app.clone().oneshot(create_provider_request).await.unwrap();
    let provider_created = read_json(provider_response).await;
    let provider_definition_id = provider_created["data"]["id"]
        .as_str()
        .expect("provider id should be present");

    let start_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/transactions/start")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "provider_definition_id": provider_definition_id,
                "alias": "lark-delegated-alice",
                "purpose": "Lark delegated runtime",
                "subject_ref_hint": "alice@example.com",
                "subject_display_name": "Alice",
                "backing_credential_id": backing_credential_id,
                "scopes": ["contact:user.id:readonly"]
            })
            .to_string(),
        ))
        .unwrap();
    let start_response = app.clone().oneshot(start_request).await.unwrap();
    let started = read_json(start_response).await;
    let state = started["data"]["state"].as_str().unwrap().to_string();

    let callback_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/oauth-broker/callback/lark?code=mock_code_123&state={state}"
        ))
        .body(Body::empty())
        .unwrap();
    let callback_response = app.clone().oneshot(callback_request).await.unwrap();

    assert_eq!(callback_response.status(), StatusCode::OK);
    let callback_body = read_json(callback_response).await;
    assert_eq!(callback_body["success"], true);
    assert_eq!(callback_body["data"]["status"], "ready");
    assert!(callback_body["data"]["binding_handle"].as_str().is_some());

    let binding_handle = callback_body["data"]["binding_handle"]
        .as_str()
        .expect("binding handle should be present");
    let stored: (String, String, String) = sqlx::query_as(
        r#"
        SELECT b.status, rs.health_status, b.subject_ref
        FROM public.oauth_bindings b
        JOIN public.oauth_binding_runtime_states rs ON rs.binding_id = b.id
        WHERE b.binding_handle = $1
        "#,
    )
    .bind(binding_handle)
    .fetch_one(&pool)
    .await
    .expect("binding should persist after successful callback");
    assert_eq!(stored.0, "ready");
    assert_eq!(stored.1, "ready");
    assert_eq!(stored.2, "ou_mock_alice");

    let replay_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/oauth-broker/callback/lark?code=mock_code_456&state={state}"
        ))
        .body(Body::empty())
        .unwrap();
    let replay_response = app.clone().oneshot(replay_request).await.unwrap();
    assert_eq!(replay_response.status(), StatusCode::CONFLICT);

    let db_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .expect("should read temp database name");
    pool.close().await;
    drop_temp_database(&admin_pool, &db_name).await;
    admin_pool.close().await;
    server.abort();
}

#[tokio::test]
async fn delegated_binding_runtime_invoke_uses_binding_handle_after_callback_success() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock lark listener should bind");
    let addr = listener.local_addr().unwrap();
    let mock_app = Router::new()
        .route(
            "/open-apis/authen/v1/oidc/access_token",
            axum::routing::post(|| async {
                axum::Json(json!({
                    "code": 0,
                    "msg": "ok",
                    "data": {
                        "access_token": "uat_mock_123",
                        "refresh_token": "urt_mock_123",
                        "open_id": "ou_mock_alice",
                        "token_type": "Bearer",
                        "expires_in": 7200
                    }
                }))
            }),
        )
        .route(
            "/open-apis/contact/v3/scopes",
            axum::routing::get(|| async {
                (
                    [(
                        axum::http::header::HeaderName::from_static("x-tt-logid"),
                        "lark_delegated_runtime_req_123",
                    )],
                    axum::Json(json!({
                        "code": 0,
                        "data": {
                            "scopes": ["contact:user.id:readonly"]
                        }
                    })),
                )
            }),
        );
    let server = tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        server.abort();
        eprintln!("skip lark delegated runtime invoke test: database not available");
        return;
    };

    let admin_app = app.clone().layer(axum::Extension(create_admin_token()));
    let backing_credential_id = create_lark_app_secret_credential(
        admin_app.clone(),
        "lark-delegated-client",
        "cli_delegated_123",
        "delegated_secret_mock",
    )
    .await;

    let create_provider_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "lark-delegated-user-runtime",
                "display_name": "Lark Delegated User Runtime",
                "provider_family": "lark_delegated",
                "binding_kind": "delegated_user",
                "grant_family": "authorization_code_pkce",
                "authorization_endpoint": format!("http://127.0.0.1:{}/open-apis/authen/v1/authorize", addr.port()),
                "token_endpoint": format!("http://127.0.0.1:{}/open-apis/authen/v1/oidc/access_token", addr.port()),
                "client_id": "cli_delegated_123",
                "client_auth_method": "none",
                "callback_mode": "platform",
                "adapter_key": "lark_openapi",
                "adapter_version": "v1",
                "scope_template": ["contact:user.id:readonly"],
                "runtime_config": {
                    "redirect_uri": "https://credbridge.dev/oauth/callback/lark",
                    "token_exchange": {
                        "credential_client_id_field": "app_id",
                        "credential_client_secret_field": "app_secret",
                        "request_client_id_field": "app_id",
                        "request_client_secret_field": "app_secret"
                    },
                    "base_url": format!("http://127.0.0.1:{}", addr.port()),
                    "allowed_domains": ["127.0.0.1"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": ["/open-apis/contact/v3/scopes"]
                }
            })
            .to_string(),
        ))
        .unwrap();
    let provider_response = admin_app
        .clone()
        .oneshot(create_provider_request)
        .await
        .unwrap();
    let provider_created = read_json(provider_response).await;
    let provider_definition_id = provider_created["data"]["id"]
        .as_str()
        .expect("provider id should be present");

    let start_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/transactions/start")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "provider_definition_id": provider_definition_id,
                "alias": "lark-delegated-alice-runtime",
                "purpose": "Lark delegated runtime",
                "subject_ref_hint": "alice@example.com",
                "subject_display_name": "Alice",
                "backing_credential_id": backing_credential_id,
                "scopes": ["contact:user.id:readonly"]
            })
            .to_string(),
        ))
        .unwrap();
    let start_response = admin_app.clone().oneshot(start_request).await.unwrap();
    let started = read_json(start_response).await;
    let state = started["data"]["state"].as_str().unwrap().to_string();

    let callback_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/oauth-broker/callback/lark?code=mock_code_123&state={state}"
        ))
        .body(Body::empty())
        .unwrap();
    let callback_response = admin_app.clone().oneshot(callback_request).await.unwrap();
    let callback_body = read_json(callback_response).await;
    let binding_handle = callback_body["data"]["binding_handle"]
        .as_str()
        .expect("binding handle should be present")
        .to_string();

    let runtime_app = app.layer(axum::Extension(create_runtime_token(&binding_handle)));
    let invoke_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/runtime/invoke")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "binding_handle": binding_handle,
                "request": {
                    "method": "GET",
                    "url": format!("http://127.0.0.1:{}/open-apis/contact/v3/scopes", addr.port())
                }
            })
            .to_string(),
        ))
        .unwrap();
    let invoke_response = runtime_app.oneshot(invoke_request).await.unwrap();
    assert_eq!(invoke_response.status(), StatusCode::OK);
    let invoked = read_json(invoke_response).await;
    assert_eq!(invoked["success"], true);
    assert_eq!(invoked["data"]["binding_handle"], binding_handle);
    assert_eq!(invoked["data"]["status_code"], 200);
    assert_eq!(
        invoked["data"]["provider_request_id"],
        "lark_delegated_runtime_req_123"
    );
    assert_eq!(
        invoked["data"]["body"]["data"]["scopes"][0],
        "contact:user.id:readonly"
    );

    let db_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .expect("should read temp database name");
    pool.close().await;
    drop_temp_database(&admin_pool, &db_name).await;
    admin_pool.close().await;
    server.abort();
}
