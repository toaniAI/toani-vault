#![allow(clippy::field_reassign_with_default)]
#![allow(dead_code)]
#![allow(clippy::uninlined_format_args)]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
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
use vault_service::api::oauth_broker::{OAuthBrokerApiState, oauth_broker_routes};
use vault_service::audit::{AuditAction, MemoryAuditStorage};
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

async fn setup_postgres_backed_app() -> Option<(
    Router,
    sqlx::PgPool,
    sqlx::PgPool,
    Arc<tokio::sync::Mutex<MemoryAuditStorage>>,
)> {
    let base_database_url = test_database_url()?;
    let admin_database_url = postgres_admin_url(&base_database_url)?;

    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&admin_database_url)
        .await
        .ok()?;

    let database_name = format!(
        "credbridge_lark_binding_{}",
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
    .bind("lark-binding-test")
    .bind("lark binding test tenant")
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
    .bind("Lark Binding Test User")
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
        shared_audit_storage.clone(),
    )))
    .with_credential_runtime(
        credential_state.vault.clone(),
        credential_state.key_hierarchy.clone(),
        credential_state.enclave.clone(),
    );

    let app = Router::new()
        .merge(credential_routes().with_state(credential_state))
        .merge(oauth_broker_routes(broker_state));

    Some((
        app,
        database_pool.pool().clone(),
        admin_pool,
        shared_audit_storage,
    ))
}

async fn read_json(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&bytes).expect("response body should be valid json")
}

#[tokio::test]
async fn lark_binding_validate_promotes_binding_to_ready_after_real_token_probe() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock lark listener should bind");
    let addr = listener.local_addr().unwrap();
    let mock_app = Router::new().route(
        "/open-apis/auth/v3/tenant_access_token/internal",
        post(|| async {
            axum::Json(json!({
                "code": 0,
                "msg": "ok",
                "tenant_access_token": "tenant_access_token_mock",
                "expire": 7200
            }))
        }),
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let Some((app, pool, admin_pool, audit_storage)) = setup_postgres_backed_app().await else {
        server.abort();
        eprintln!("skip lark binding test: database not available");
        return;
    };

    let app = app.layer(axum::Extension(create_admin_token()));

    let create_credential_request = Request::builder()
        .method("POST")
        .uri("/credentials")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "service_id": "lark-app-secret",
                "credential_type": "api_key",
                "plaintext_data": {
                    "app_id": "cli_mock",
                    "app_secret": "mock_secret"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let credential_response = app
        .clone()
        .oneshot(create_credential_request)
        .await
        .unwrap();
    assert_eq!(credential_response.status(), StatusCode::CREATED);
    let credential_created = read_json(credential_response).await;
    let backing_credential_id = credential_created["credential_id"]
        .as_str()
        .expect("credential id should be present")
        .to_string();

    let create_provider_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "lark-tenant-token",
                "display_name": "Lark Tenant Token",
                "provider_family": "lark_app",
                "binding_kind": "provider_app_credential",
                "grant_family": "client_credentials",
                "token_endpoint": format!("http://127.0.0.1:{}/open-apis/auth/v3/tenant_access_token/internal", addr.port()),
                "client_auth_method": "client_secret_post",
                "callback_mode": "none",
                "adapter_key": "lark_openapi",
                "adapter_version": "v1",
                "scope_template": ["contact:scope:readonly"],
                "runtime_config": {
                    "base_url": format!("http://127.0.0.1:{}", addr.port()),
                    "token_mode": "tenant_access_token",
                    "allowed_domains": ["127.0.0.1"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": ["/open-apis/contact/v3/scopes"],
                    "connectivity_probe_url": format!("http://127.0.0.1:{}/open-apis/auth/v3/tenant_access_token/internal", addr.port())
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

    let create_binding_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/bindings")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "provider_definition_id": provider_definition_id,
                "alias": "lark-primary",
                "purpose": "Lark tenant token runtime",
                "subject_ref": "cli_mock",
                "subject_display_name": "Lark Tenant Token",
                "backing_credential_id": backing_credential_id
            })
            .to_string(),
        ))
        .unwrap();
    let binding_response = app.clone().oneshot(create_binding_request).await.unwrap();
    assert_eq!(binding_response.status(), StatusCode::OK);
    let binding_created = read_json(binding_response).await;
    let binding_id = binding_created["data"]["id"]
        .as_str()
        .expect("binding id should be present");

    let validate_binding_request = Request::builder()
        .method("POST")
        .uri(format!("/oauth-broker/bindings/{binding_id}/validate"))
        .body(Body::empty())
        .unwrap();
    let validate_binding_response = app.clone().oneshot(validate_binding_request).await.unwrap();

    assert_eq!(validate_binding_response.status(), StatusCode::OK);
    let validated = read_json(validate_binding_response).await;
    assert_eq!(validated["success"], true);
    assert_eq!(validated["data"]["valid"], true);
    assert_eq!(validated["data"]["status"], "ready");

    let get_binding_request = Request::builder()
        .method("GET")
        .uri(format!("/oauth-broker/bindings/{binding_id}"))
        .body(Body::empty())
        .unwrap();
    let get_binding_response = app.oneshot(get_binding_request).await.unwrap();
    assert_eq!(get_binding_response.status(), StatusCode::OK);
    let binding_detail = read_json(get_binding_response).await;
    assert_eq!(binding_detail["data"]["status"], "ready");
    assert_eq!(binding_detail["data"]["health_status"], "ready");

    let stored_health: String = sqlx::query_scalar(
        r#"SELECT health_status FROM public.oauth_binding_runtime_states WHERE binding_id = $1"#,
    )
    .bind(Uuid::parse_str(binding_id).unwrap())
    .fetch_one(&pool)
    .await
    .expect("should read runtime state health");
    assert_eq!(stored_health, "ready");

    let audit_entries = audit_storage
        .lock()
        .await
        .query_recent(10)
        .expect("audit query should succeed");
    assert!(audit_entries.iter().any(|item| {
        item.entry.action == AuditAction::SystemConfigChange
            && item
                .entry
                .params
                .as_ref()
                .and_then(|params| {
                    params
                        .iter()
                        .find(|(key, _)| key == "details")
                        .map(|(_, value)| value.to_string())
                })
                .map(|value| value.contains("oauth_binding_validated"))
                .unwrap_or(false)
    }));

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
async fn lark_binding_create_rejects_backing_credential_owned_by_another_user() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock lark listener should bind");
    let addr = listener.local_addr().unwrap();
    let mock_app = Router::new().route(
        "/open-apis/auth/v3/tenant_access_token/internal",
        post(|| async {
            axum::Json(json!({
                "code": 0,
                "msg": "ok",
                "tenant_access_token": "tenant_access_token_mock",
                "expire": 7200
            }))
        }),
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let Some((app, pool, admin_pool, _audit_storage)) = setup_postgres_backed_app().await else {
        server.abort();
        eprintln!("skip lark binding ownership test: database not available");
        return;
    };

    let outsider_app = app.clone().layer(axum::Extension(create_admin_token_for(
        Uuid::nil(),
        "00000000-0000-0000-0000-000000000099".to_string(),
    )));
    let create_credential_request = Request::builder()
        .method("POST")
        .uri("/credentials")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "service_id": "lark-app-secret-other-user",
                "credential_type": "api_key",
                "plaintext_data": {
                    "app_id": "cli_mock_other",
                    "app_secret": "mock_secret_other"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let credential_response = outsider_app
        .clone()
        .oneshot(create_credential_request)
        .await
        .unwrap();
    assert_eq!(credential_response.status(), StatusCode::CREATED);
    let credential_created = read_json(credential_response).await;
    let backing_credential_id = credential_created["credential_id"]
        .as_str()
        .expect("credential id should be present")
        .to_string();

    let owner_app = app.layer(axum::Extension(create_admin_token()));
    let create_provider_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "lark-tenant-token-ownership",
                "display_name": "Lark Tenant Token Ownership",
                "provider_family": "lark_app",
                "binding_kind": "provider_app_credential",
                "grant_family": "client_credentials",
                "token_endpoint": format!("http://127.0.0.1:{}/open-apis/auth/v3/tenant_access_token/internal", addr.port()),
                "client_auth_method": "client_secret_post",
                "callback_mode": "none",
                "adapter_key": "lark_openapi",
                "adapter_version": "v1",
                "scope_template": ["contact:scope:readonly"],
                "runtime_config": {
                    "base_url": format!("http://127.0.0.1:{}", addr.port()),
                    "token_mode": "tenant_access_token",
                    "allowed_domains": ["127.0.0.1"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": ["/open-apis/contact/v3/scopes"],
                    "connectivity_probe_url": format!("http://127.0.0.1:{}/open-apis/auth/v3/tenant_access_token/internal", addr.port())
                }
            })
            .to_string(),
        ))
        .unwrap();
    let provider_response = owner_app
        .clone()
        .oneshot(create_provider_request)
        .await
        .unwrap();
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
                "alias": "lark-primary-ownership",
                "purpose": "Lark tenant token runtime",
                "subject_ref": "cli_mock",
                "subject_display_name": "Lark Tenant Token",
                "backing_credential_id": backing_credential_id
            })
            .to_string(),
        ))
        .unwrap();
    let binding_response = owner_app.oneshot(create_binding_request).await.unwrap();
    assert_eq!(binding_response.status(), StatusCode::FORBIDDEN);

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
async fn lark_binding_runtime_invoke_uses_binding_handle_to_call_governed_http_target() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock lark listener should bind");
    let addr = listener.local_addr().unwrap();
    let mock_app = Router::new()
        .route(
            "/open-apis/auth/v3/tenant_access_token/internal",
            post(|| async {
                axum::Json(json!({
                    "code": 0,
                    "msg": "ok",
                    "tenant_access_token": "tenant_access_token_mock",
                    "expire": 7200
                }))
            }),
        )
        .route(
            "/open-apis/contact/v3/scopes",
            axum::routing::get(|| async {
                (
                    [(
                        axum::http::header::HeaderName::from_static("x-tt-logid"),
                        "req_mock_123",
                    )],
                    axum::Json(json!({
                        "code": 0,
                        "data": {
                            "scopes": ["contact:scope:readonly"]
                        }
                    })),
                )
            }),
        );
    let server = tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let Some((app, pool, admin_pool, audit_storage)) = setup_postgres_backed_app().await else {
        server.abort();
        eprintln!("skip lark binding runtime invoke test: database not available");
        return;
    };

    let admin_app = app.clone().layer(axum::Extension(create_admin_token()));

    let create_credential_request = Request::builder()
        .method("POST")
        .uri("/credentials")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "service_id": "lark-app-secret",
                "credential_type": "api_key",
                "plaintext_data": {
                    "app_id": "cli_mock",
                    "app_secret": "mock_secret"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let credential_response = admin_app
        .clone()
        .oneshot(create_credential_request)
        .await
        .unwrap();
    let credential_created = read_json(credential_response).await;
    let backing_credential_id = credential_created["credential_id"]
        .as_str()
        .expect("credential id should be present")
        .to_string();

    let create_provider_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "lark-tenant-token-runtime",
                "display_name": "Lark Tenant Token Runtime",
                "provider_family": "lark_app",
                "binding_kind": "provider_app_credential",
                "grant_family": "client_credentials",
                "token_endpoint": format!("http://127.0.0.1:{}/open-apis/auth/v3/tenant_access_token/internal", addr.port()),
                "client_auth_method": "client_secret_post",
                "callback_mode": "none",
                "adapter_key": "lark_openapi",
                "adapter_version": "v1",
                "scope_template": ["contact:scope:readonly"],
                "runtime_config": {
                    "base_url": format!("http://127.0.0.1:{}", addr.port()),
                    "token_mode": "tenant_access_token",
                    "allowed_domains": ["127.0.0.1"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": ["/open-apis/contact/v3/scopes"],
                    "connectivity_probe_url": format!("http://127.0.0.1:{}/open-apis/auth/v3/tenant_access_token/internal", addr.port())
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

    let create_binding_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/bindings")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "provider_definition_id": provider_definition_id,
                "alias": "lark-runtime-primary",
                "purpose": "Lark tenant token invocation",
                "subject_ref": "cli_mock",
                "subject_display_name": "Lark Tenant Token Runtime",
                "backing_credential_id": backing_credential_id
            })
            .to_string(),
        ))
        .unwrap();
    let binding_response = admin_app
        .clone()
        .oneshot(create_binding_request)
        .await
        .unwrap();
    let binding_created = read_json(binding_response).await;
    let binding_id = binding_created["data"]["id"]
        .as_str()
        .expect("binding id should be present");
    let binding_handle = binding_created["data"]["binding_handle"]
        .as_str()
        .expect("binding handle should be present")
        .to_string();

    let validate_binding_request = Request::builder()
        .method("POST")
        .uri(format!("/oauth-broker/bindings/{binding_id}/validate"))
        .body(Body::empty())
        .unwrap();
    let validate_binding_response = admin_app
        .clone()
        .oneshot(validate_binding_request)
        .await
        .unwrap();
    assert_eq!(validate_binding_response.status(), StatusCode::OK);

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
                    "url": format!("http://127.0.0.1:{}/open-apis/contact/v3/scopes", addr.port()),
                    "headers": {
                        "x-caller-trace": "trace-174"
                    }
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
    assert_eq!(invoked["data"]["provider_request_id"], "req_mock_123");
    assert_eq!(
        invoked["data"]["body"]["data"]["scopes"][0],
        "contact:scope:readonly"
    );

    let audit_entries = audit_storage
        .lock()
        .await
        .query_recent(20)
        .expect("audit query should succeed");
    assert!(audit_entries.iter().any(|item| {
        item.entry.action == AuditAction::CredentialAccess
            && item
                .entry
                .params
                .as_ref()
                .and_then(|params| {
                    params
                        .iter()
                        .find(|(key, _)| key == "details")
                        .map(|(_, value)| value.to_string())
                })
                .map(|value| value.contains("oauth_binding_runtime_invoked"))
                .unwrap_or(false)
    }));

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
async fn lark_binding_runtime_invoke_rejects_disallowed_domain_with_structured_error() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock lark listener should bind");
    let addr = listener.local_addr().unwrap();
    let mock_app = Router::new().route(
        "/open-apis/auth/v3/tenant_access_token/internal",
        post(|| async {
            axum::Json(json!({
                "code": 0,
                "msg": "ok",
                "tenant_access_token": "tenant_access_token_mock",
                "expire": 7200
            }))
        }),
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let Some((app, pool, admin_pool, _audit_storage)) = setup_postgres_backed_app().await else {
        server.abort();
        eprintln!("skip lark binding deny test: database not available");
        return;
    };

    let admin_app = app.clone().layer(axum::Extension(create_admin_token()));

    let create_credential_request = Request::builder()
        .method("POST")
        .uri("/credentials")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "service_id": "lark-app-secret",
                "credential_type": "api_key",
                "plaintext_data": {
                    "app_id": "cli_mock",
                    "app_secret": "mock_secret"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let credential_response = admin_app
        .clone()
        .oneshot(create_credential_request)
        .await
        .unwrap();
    let credential_created = read_json(credential_response).await;
    let backing_credential_id = credential_created["credential_id"]
        .as_str()
        .expect("credential id should be present")
        .to_string();

    let create_provider_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "lark-tenant-token-deny",
                "display_name": "Lark Tenant Token Deny",
                "provider_family": "lark_app",
                "binding_kind": "provider_app_credential",
                "grant_family": "client_credentials",
                "token_endpoint": format!("http://127.0.0.1:{}/open-apis/auth/v3/tenant_access_token/internal", addr.port()),
                "client_auth_method": "client_secret_post",
                "callback_mode": "none",
                "adapter_key": "lark_openapi",
                "adapter_version": "v1",
                "scope_template": ["contact:scope:readonly"],
                "runtime_config": {
                    "base_url": format!("http://127.0.0.1:{}", addr.port()),
                    "token_mode": "tenant_access_token",
                    "allowed_domains": ["127.0.0.1"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": ["/open-apis/contact/v3/scopes"],
                    "connectivity_probe_url": format!("http://127.0.0.1:{}/open-apis/auth/v3/tenant_access_token/internal", addr.port())
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

    let create_binding_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/bindings")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "provider_definition_id": provider_definition_id,
                "alias": "lark-deny-primary",
                "purpose": "Lark tenant token deny",
                "subject_ref": "cli_mock",
                "subject_display_name": "Lark Tenant Token Deny",
                "backing_credential_id": backing_credential_id
            })
            .to_string(),
        ))
        .unwrap();
    let binding_response = admin_app
        .clone()
        .oneshot(create_binding_request)
        .await
        .unwrap();
    let binding_created = read_json(binding_response).await;
    let binding_id = binding_created["data"]["id"]
        .as_str()
        .expect("binding id should be present");
    let binding_handle = binding_created["data"]["binding_handle"]
        .as_str()
        .expect("binding handle should be present")
        .to_string();

    let validate_binding_request = Request::builder()
        .method("POST")
        .uri(format!("/oauth-broker/bindings/{binding_id}/validate"))
        .body(Body::empty())
        .unwrap();
    let validate_binding_response = admin_app
        .clone()
        .oneshot(validate_binding_request)
        .await
        .unwrap();
    assert_eq!(validate_binding_response.status(), StatusCode::OK);

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
                    "url": "https://example.com/open-apis/contact/v3/scopes"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let invoke_response = runtime_app.oneshot(invoke_request).await.unwrap();
    assert_eq!(invoke_response.status(), StatusCode::FORBIDDEN);
    let denied = read_json(invoke_response).await;
    assert_eq!(denied["success"], false);
    assert_eq!(denied["error"], "forbidden");
    assert!(
        denied["message"]
            .as_str()
            .unwrap_or_default()
            .contains("host is not allowed")
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

#[tokio::test]
async fn lark_binding_validate_reports_invalid_app_secret_as_structured_failure() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock lark listener should bind");
    let addr = listener.local_addr().unwrap();
    let mock_app = Router::new().route(
        "/open-apis/auth/v3/tenant_access_token/internal",
        post(|| async {
            axum::Json(json!({
                "code": 99991663,
                "msg": "invalid app secret"
            }))
        }),
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let Some((app, pool, admin_pool, _audit_storage)) = setup_postgres_backed_app().await else {
        server.abort();
        eprintln!("skip lark binding invalid secret test: database not available");
        return;
    };

    let admin_app = app.clone().layer(axum::Extension(create_admin_token()));

    let create_credential_request = Request::builder()
        .method("POST")
        .uri("/credentials")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "service_id": "lark-app-secret",
                "credential_type": "api_key",
                "plaintext_data": {
                    "app_id": "cli_mock",
                    "app_secret": "bad_secret"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let credential_response = admin_app
        .clone()
        .oneshot(create_credential_request)
        .await
        .unwrap();
    let credential_created = read_json(credential_response).await;
    let backing_credential_id = credential_created["credential_id"]
        .as_str()
        .expect("credential id should be present")
        .to_string();

    let create_provider_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "lark-tenant-token-invalid-secret",
                "display_name": "Lark Tenant Token Invalid Secret",
                "provider_family": "lark_app",
                "binding_kind": "provider_app_credential",
                "grant_family": "client_credentials",
                "token_endpoint": format!("http://127.0.0.1:{}/open-apis/auth/v3/tenant_access_token/internal", addr.port()),
                "client_auth_method": "client_secret_post",
                "callback_mode": "none",
                "adapter_key": "lark_openapi",
                "adapter_version": "v1",
                "scope_template": ["contact:scope:readonly"],
                "runtime_config": {
                    "base_url": format!("http://127.0.0.1:{}", addr.port()),
                    "token_mode": "tenant_access_token",
                    "allowed_domains": ["127.0.0.1"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": ["/open-apis/contact/v3/scopes"],
                    "connectivity_probe_url": format!("http://127.0.0.1:{}/open-apis/auth/v3/tenant_access_token/internal", addr.port())
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

    let create_binding_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/bindings")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "provider_definition_id": provider_definition_id,
                "alias": "lark-invalid-secret-primary",
                "purpose": "Lark tenant token invalid secret",
                "subject_ref": "cli_mock",
                "subject_display_name": "Lark Tenant Token Invalid Secret",
                "backing_credential_id": backing_credential_id
            })
            .to_string(),
        ))
        .unwrap();
    let binding_response = admin_app
        .clone()
        .oneshot(create_binding_request)
        .await
        .unwrap();
    let binding_created = read_json(binding_response).await;
    let binding_id = binding_created["data"]["id"]
        .as_str()
        .expect("binding id should be present");

    let validate_binding_request = Request::builder()
        .method("POST")
        .uri(format!("/oauth-broker/bindings/{binding_id}/validate"))
        .body(Body::empty())
        .unwrap();
    let validate_binding_response = admin_app
        .clone()
        .oneshot(validate_binding_request)
        .await
        .unwrap();
    assert_eq!(validate_binding_response.status(), StatusCode::OK);
    let validation = read_json(validate_binding_response).await;
    assert_eq!(validation["success"], true);
    assert_eq!(validation["data"]["valid"], false);
    assert_eq!(validation["data"]["status"], "draft");
    assert_eq!(validation["data"]["health_status"], "validation_failed");
    assert!(
        validation["data"]["checks"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .any(|item| {
                item["name"] == "token_probe"
                    && item["status"] == "failed"
                    && item["detail"]
                        .as_str()
                        .unwrap_or_default()
                        .contains("invalid app secret")
            })
    );

    let stored_health: String = sqlx::query_scalar(
        r#"SELECT health_status FROM public.oauth_binding_runtime_states WHERE binding_id = $1"#,
    )
    .bind(Uuid::parse_str(binding_id).unwrap())
    .fetch_one(&pool)
    .await
    .expect("should read runtime state health");
    assert_eq!(stored_health, "validation_failed");

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
async fn lark_binding_runtime_invoke_redacts_sensitive_response_fields() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock lark listener should bind");
    let addr = listener.local_addr().unwrap();
    let mock_app = Router::new()
        .route(
            "/open-apis/auth/v3/tenant_access_token/internal",
            post(|| async {
                axum::Json(json!({
                    "code": 0,
                    "msg": "ok",
                    "tenant_access_token": "tenant_access_token_mock",
                    "expire": 7200
                }))
            }),
        )
        .route(
            "/open-apis/bot/v3/info",
            axum::routing::get(|| async {
                axum::Json(json!({
                    "code": 0,
                    "data": {
                        "access_token": "should_not_escape",
                        "nested": {
                            "refresh_token": "should_also_not_escape"
                        }
                    }
                }))
            }),
        );
    let server = tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let Some((app, pool, admin_pool, _audit_storage)) = setup_postgres_backed_app().await else {
        server.abort();
        eprintln!("skip lark binding redaction test: database not available");
        return;
    };

    let admin_app = app.clone().layer(axum::Extension(create_admin_token()));

    let create_credential_request = Request::builder()
        .method("POST")
        .uri("/credentials")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "service_id": "lark-app-secret",
                "credential_type": "api_key",
                "plaintext_data": {
                    "app_id": "cli_mock",
                    "app_secret": "mock_secret"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let credential_response = admin_app
        .clone()
        .oneshot(create_credential_request)
        .await
        .unwrap();
    let credential_created = read_json(credential_response).await;
    let backing_credential_id = credential_created["credential_id"]
        .as_str()
        .expect("credential id should be present")
        .to_string();

    let create_provider_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "lark-redaction",
                "display_name": "Lark Redaction",
                "provider_family": "lark_app",
                "binding_kind": "provider_app_credential",
                "grant_family": "client_credentials",
                "token_endpoint": format!("http://127.0.0.1:{}/open-apis/auth/v3/tenant_access_token/internal", addr.port()),
                "client_auth_method": "client_secret_post",
                "callback_mode": "none",
                "adapter_key": "lark_openapi",
                "adapter_version": "v1",
                "scope_template": ["bot:info"],
                "runtime_config": {
                    "base_url": format!("http://127.0.0.1:{}", addr.port()),
                    "token_mode": "tenant_access_token",
                    "allowed_domains": ["127.0.0.1"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": ["/open-apis/bot/v3/info"],
                    "connectivity_probe_url": format!("http://127.0.0.1:{}/open-apis/auth/v3/tenant_access_token/internal", addr.port())
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

    let create_binding_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/bindings")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "provider_definition_id": provider_definition_id,
                "alias": "lark-redaction-primary",
                "purpose": "Lark redaction runtime",
                "subject_ref": "cli_mock",
                "subject_display_name": "Lark Redaction Runtime",
                "backing_credential_id": backing_credential_id
            })
            .to_string(),
        ))
        .unwrap();
    let binding_response = admin_app
        .clone()
        .oneshot(create_binding_request)
        .await
        .unwrap();
    let binding_created = read_json(binding_response).await;
    let binding_id = binding_created["data"]["id"]
        .as_str()
        .expect("binding id should be present");
    let binding_handle = binding_created["data"]["binding_handle"]
        .as_str()
        .expect("binding handle should be present")
        .to_string();

    let validate_binding_request = Request::builder()
        .method("POST")
        .uri(format!("/oauth-broker/bindings/{binding_id}/validate"))
        .body(Body::empty())
        .unwrap();
    let validate_binding_response = admin_app
        .clone()
        .oneshot(validate_binding_request)
        .await
        .unwrap();
    assert_eq!(validate_binding_response.status(), StatusCode::OK);

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
                    "url": format!("http://127.0.0.1:{}/open-apis/bot/v3/info", addr.port())
                }
            })
            .to_string(),
        ))
        .unwrap();
    let invoke_response = runtime_app.oneshot(invoke_request).await.unwrap();
    assert_eq!(invoke_response.status(), StatusCode::OK);
    let invoked = read_json(invoke_response).await;
    assert_eq!(
        invoked["data"]["body"]["data"]["access_token"],
        "[REDACTED]"
    );
    assert_eq!(
        invoked["data"]["body"]["data"]["nested"]["refresh_token"],
        "[REDACTED]"
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

#[tokio::test]
async fn lark_binding_runtime_invoke_reaches_real_feishu_bot_info_when_env_is_available() {
    let (Ok(app_id), Ok(app_secret)) = (
        std::env::var("FEISHU_APP_ID"),
        std::env::var("FEISHU_APP_SECRET"),
    ) else {
        eprintln!("skip real feishu invoke test: FEISHU_APP_ID/FEISHU_APP_SECRET not configured");
        return;
    };

    let Some((app, pool, admin_pool, audit_storage)) = setup_postgres_backed_app().await else {
        eprintln!("skip real feishu invoke test: database not available");
        return;
    };

    let admin_app = app.clone().layer(axum::Extension(create_admin_token()));

    let create_credential_request = Request::builder()
        .method("POST")
        .uri("/credentials")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "service_id": "feishu-app-secret",
                "credential_type": "api_key",
                "plaintext_data": {
                    "app_id": app_id,
                    "app_secret": app_secret
                }
            })
            .to_string(),
        ))
        .unwrap();
    let credential_response = admin_app
        .clone()
        .oneshot(create_credential_request)
        .await
        .unwrap();
    assert_eq!(credential_response.status(), StatusCode::CREATED);
    let credential_created = read_json(credential_response).await;
    let backing_credential_id = credential_created["credential_id"]
        .as_str()
        .expect("credential id should be present")
        .to_string();

    let create_provider_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "feishu-bot-runtime",
                "display_name": "Feishu Bot Runtime",
                "provider_family": "lark_app",
                "binding_kind": "provider_app_credential",
                "grant_family": "client_credentials",
                "token_endpoint": "https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal",
                "client_auth_method": "client_secret_post",
                "callback_mode": "none",
                "adapter_key": "lark_openapi",
                "adapter_version": "v1",
                "scope_template": ["bot:info"],
                "runtime_config": {
                    "base_url": "https://open.feishu.cn",
                    "token_mode": "tenant_access_token",
                    "allowed_domains": ["open.feishu.cn"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": ["/open-apis/bot/v3/info"],
                    "connectivity_probe_url": "https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal"
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
    assert_eq!(provider_response.status(), StatusCode::OK);
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
                "alias": "feishu-bot-runtime-primary",
                "purpose": "Real feishu bot info runtime invocation",
                "subject_ref": app_id,
                "subject_display_name": "Feishu Bot Runtime",
                "backing_credential_id": backing_credential_id
            })
            .to_string(),
        ))
        .unwrap();
    let binding_response = admin_app
        .clone()
        .oneshot(create_binding_request)
        .await
        .unwrap();
    assert_eq!(binding_response.status(), StatusCode::OK);
    let binding_created = read_json(binding_response).await;
    let binding_id = binding_created["data"]["id"]
        .as_str()
        .expect("binding id should be present");
    let binding_handle = binding_created["data"]["binding_handle"]
        .as_str()
        .expect("binding handle should be present")
        .to_string();

    let validate_binding_request = Request::builder()
        .method("POST")
        .uri(format!("/oauth-broker/bindings/{binding_id}/validate"))
        .body(Body::empty())
        .unwrap();
    let validate_binding_response = admin_app
        .clone()
        .oneshot(validate_binding_request)
        .await
        .unwrap();
    assert_eq!(validate_binding_response.status(), StatusCode::OK);
    let validation = read_json(validate_binding_response).await;
    eprintln!("REAL_FEISHU_VALIDATE={validation}");
    assert_eq!(validation["data"]["valid"], true);
    assert_eq!(validation["data"]["status"], "ready");

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
                    "url": "https://open.feishu.cn/open-apis/bot/v3/info"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let invoke_response = runtime_app.oneshot(invoke_request).await.unwrap();
    assert_eq!(invoke_response.status(), StatusCode::OK);
    let invoked = read_json(invoke_response).await;
    eprintln!("REAL_FEISHU_INVOKE={invoked}");
    assert_eq!(invoked["success"], true);
    assert_eq!(invoked["data"]["status_code"], 200);
    assert!(
        invoked["data"]["provider_request_id"]
            .as_str()
            .unwrap_or_default()
            .len()
            > 8
    );
    assert_eq!(invoked["data"]["body"]["code"], 0);
    assert!(
        invoked["data"]["body"]["bot"]["app_name"]
            .as_str()
            .unwrap_or_default()
            .contains("Credbridge")
    );

    let audit_entries = audit_storage
        .lock()
        .await
        .query_recent(20)
        .expect("audit query should succeed");
    assert!(audit_entries.iter().any(|item| {
        item.entry.action == AuditAction::CredentialAccess
            && item
                .entry
                .params
                .as_ref()
                .and_then(|params| {
                    params
                        .iter()
                        .find(|(key, _)| key == "details")
                        .map(|(_, value)| value.to_string())
                })
                .map(|value| value.contains("oauth_binding_runtime_invoked"))
                .unwrap_or(false)
    }));

    let db_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .expect("should read temp database name");
    pool.close().await;
    drop_temp_database(&admin_pool, &db_name).await;
    admin_pool.close().await;
}
