#![allow(clippy::field_reassign_with_default)]
#![allow(dead_code)]
#![allow(clippy::uninlined_format_args)]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
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
    let user_id = "00000000-0000-0000-0000-000000000001";
    let tenant_id = Uuid::nil().to_string();
    ValidatedToken {
        token_id: Uuid::now_v7().to_string(),
        subject: format!("{tenant_id}:{user_id}"),
        tenant_id,
        user_id: user_id.to_string(),
        expires_at: u64::MAX,
        scopes: vec![TokenScope::TenantAdmin, TokenScope::CredentialWrite],
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

async fn setup_postgres_backed_app() -> Option<(Router, sqlx::PgPool, sqlx::PgPool)> {
    let base_database_url = test_database_url()?;
    let admin_database_url = postgres_admin_url(&base_database_url)?;

    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&admin_database_url)
        .await
        .ok()?;

    let database_name = format!(
        "credbridge_google_workspace_delegated_{}",
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
    .bind("google-workspace-delegated-test")
    .bind("google workspace delegated test tenant")
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
    .bind("Google Workspace Delegated Test User")
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

async fn create_google_client_secret_credential(
    app: Router,
    service_id: &str,
    client_id: &str,
    client_secret: &str,
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
                    "client_id": client_id,
                    "client_secret": client_secret
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

fn real_google_workspace_oauth_client_path() -> String {
    std::env::var("GOOGLE_WORKSPACE_OAUTH_CLIENT_FIXTURE_PATH").unwrap_or_else(|_| {
        ".local/google-workspace/floatingdream.tech/oauth-client.json".to_string()
    })
}

fn fake_google_id_token(sub: &str, email: &str, name: &str) -> String {
    let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(
        json!({
            "sub": sub,
            "email": email,
            "email_verified": true,
            "name": name,
            "hd": "floatingdream.tech"
        })
        .to_string(),
    );
    format!("{header}.{payload}.signature")
}

fn google_workspace_provider_request(alias: &str, display_name: &str, port: u16) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": alias,
                "display_name": display_name,
                "provider_family": "google_workspace_delegated",
                "binding_kind": "delegated_user",
                "grant_family": "authorization_code_pkce",
                "authorization_endpoint": "https://accounts.google.com/o/oauth2/v2/auth",
                "token_endpoint": format!("http://127.0.0.1:{port}/token"),
                "client_id": "cli_google_123",
                "client_auth_method": "none",
                "callback_mode": "platform",
                "adapter_key": "oauth_openid",
                "adapter_version": "v1",
                "scope_template": [
                    "openid",
                    "email",
                    "profile",
                    "https://www.googleapis.com/auth/drive.metadata.readonly"
                ],
                "runtime_config": {
                    "redirect_uri": "http://localhost:8080/oauth/callback/google",
                    "transaction_ttl_minutes": 60,
                    "allowed_domains": ["127.0.0.1"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": ["/drive/v3/about"],
                    "authorization_query": {
                        "access_type": "offline",
                        "prompt": "consent"
                    }
                }
            })
            .to_string(),
        ))
        .unwrap()
}

#[tokio::test]
async fn google_workspace_delegated_auth_transaction_start_persists_pkce_state_and_authorization_url()
 {
    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        eprintln!("skip google delegated binding transaction test: database not available");
        return;
    };

    let admin_app = app.layer(axum::Extension(create_admin_token()));
    let backing_credential_id = create_google_client_secret_credential(
        admin_app.clone(),
        "google-workspace-oauth-client",
        "cli_google_123",
        "google_secret_mock",
    )
    .await;

    let provider_response = admin_app
        .clone()
        .oneshot(google_workspace_provider_request(
            "google-workspace-delegated-user",
            "Google Workspace Delegated User",
            65531,
        ))
        .await
        .unwrap();
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
                "alias": "google-delegated-yvan",
                "purpose": "Google Workspace delegated runtime",
                "subject_ref_hint": "yvanjiang@floatingdream.tech",
                "subject_display_name": "Yvan Jiang",
                "backing_credential_id": backing_credential_id,
                "scopes": [
                    "openid",
                    "email",
                    "profile",
                    "https://www.googleapis.com/auth/drive.metadata.readonly"
                ]
            })
            .to_string(),
        ))
        .unwrap();
    let start_response = admin_app.clone().oneshot(start_request).await.unwrap();

    assert_eq!(start_response.status(), StatusCode::OK);
    let started = read_json(start_response).await;
    let authorization_url = started["data"]["authorization_url"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(authorization_url.contains("code_challenge="));
    assert!(authorization_url.contains("state="));
    assert!(authorization_url.contains("access_type=offline"));
    assert!(authorization_url.contains("prompt=consent"));
    assert_eq!(started["data"]["code_challenge_method"], "S256");

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
    assert_eq!(
        status_query.3,
        "http://localhost:8080/oauth/callback/google"
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
async fn google_workspace_delegated_callback_rejects_state_mismatch_before_code_exchange() {
    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        eprintln!("skip google delegated callback mismatch test: database not available");
        return;
    };

    let admin_app = app.layer(axum::Extension(create_admin_token()));
    let backing_credential_id = create_google_client_secret_credential(
        admin_app.clone(),
        "google-workspace-oauth-client",
        "cli_google_123",
        "google_secret_mock",
    )
    .await;

    let provider_response = admin_app
        .clone()
        .oneshot(google_workspace_provider_request(
            "google-workspace-delegated-mismatch",
            "Google Workspace Delegated Mismatch",
            65531,
        ))
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
                "alias": "google-delegated-mismatch",
                "purpose": "Google Workspace delegated mismatch test",
                "subject_ref_hint": "yvanjiang@floatingdream.tech",
                "subject_display_name": "Yvan Jiang",
                "backing_credential_id": backing_credential_id,
                "scopes": [
                    "openid",
                    "email",
                    "profile",
                    "https://www.googleapis.com/auth/drive.metadata.readonly"
                ]
            })
            .to_string(),
        ))
        .unwrap();
    let _ = admin_app.clone().oneshot(start_request).await.unwrap();

    let callback_request = Request::builder()
        .method("GET")
        .uri("/oauth-broker/callback/google?code=mock_code_123&state=bad-state")
        .body(Body::empty())
        .unwrap();
    let callback_response = admin_app.clone().oneshot(callback_request).await.unwrap();
    assert_eq!(callback_response.status(), StatusCode::BAD_REQUEST);

    let db_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .expect("should read temp database name");
    pool.close().await;
    drop_temp_database(&admin_pool, &db_name).await;
    admin_pool.close().await;
}

#[tokio::test]
async fn google_workspace_delegated_callback_success_creates_ready_binding_and_replay_is_rejected()
{
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock google listener should bind");
    let addr = listener.local_addr().unwrap();
    let id_token = fake_google_id_token(
        "google_sub_123",
        "yvanjiang@floatingdream.tech",
        "Yvan Jiang",
    );
    let mock_app = Router::new().route(
        "/token",
        axum::routing::post(move || {
            let id_token = id_token.clone();
            async move {
                axum::Json(json!({
                    "access_token": "ya29.mock_access_123",
                    "refresh_token": "1//mock_refresh_123",
                    "expires_in": 3600,
                    "scope": "openid email profile https://www.googleapis.com/auth/drive.metadata.readonly",
                    "token_type": "Bearer",
                    "id_token": id_token
                }))
            }
        }),
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        server.abort();
        eprintln!("skip google delegated callback success test: database not available");
        return;
    };

    let admin_app = app.layer(axum::Extension(create_admin_token()));
    let backing_credential_id = create_google_client_secret_credential(
        admin_app.clone(),
        "google-workspace-oauth-client",
        "cli_google_123",
        "google_secret_mock",
    )
    .await;

    let provider_response = admin_app
        .clone()
        .oneshot(google_workspace_provider_request(
            "google-workspace-delegated-success",
            "Google Workspace Delegated Success",
            addr.port(),
        ))
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
                "alias": "google-delegated-yvan-success",
                "purpose": "Google Workspace delegated callback success",
                "subject_ref_hint": "yvanjiang@floatingdream.tech",
                "subject_display_name": "Yvan Jiang",
                "backing_credential_id": backing_credential_id,
                "scopes": [
                    "openid",
                    "email",
                    "profile",
                    "https://www.googleapis.com/auth/drive.metadata.readonly"
                ]
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
            "/oauth-broker/callback/google?code=mock_code_123&state={state}"
        ))
        .body(Body::empty())
        .unwrap();
    let callback_response = admin_app.clone().oneshot(callback_request).await.unwrap();
    assert_eq!(callback_response.status(), StatusCode::OK);
    let callback_body = read_json(callback_response).await;
    assert_eq!(callback_body["success"], true);
    assert_eq!(callback_body["data"]["status"], "ready");

    let binding_handle = callback_body["data"]["binding_handle"]
        .as_str()
        .expect("binding handle should be present");
    let stored: (String, String, String, Option<String>) = sqlx::query_as(
        r#"
        SELECT b.status, rs.health_status, b.subject_ref, b.subject_display_name
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
    assert_eq!(stored.2, "google_sub_123");
    assert_eq!(stored.3.as_deref(), Some("Yvan Jiang"));

    let granted_scopes: Value = sqlx::query_scalar(
        r#"
        SELECT granted_scopes
        FROM public.oauth_binding_policy_snapshots ps
        JOIN public.oauth_bindings b ON b.id = ps.binding_id
        WHERE b.binding_handle = $1
        ORDER BY ps.version DESC
        LIMIT 1
        "#,
    )
    .bind(binding_handle)
    .fetch_one(&pool)
    .await
    .expect("binding policy snapshot should persist");
    let granted_scopes = granted_scopes.as_array().cloned().unwrap_or_default();
    assert!(granted_scopes.iter().any(|item| {
        item.as_str() == Some("https://www.googleapis.com/auth/drive.metadata.readonly")
    }));

    let replay_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/oauth-broker/callback/google?code=mock_code_456&state={state}"
        ))
        .body(Body::empty())
        .unwrap();
    let replay_response = admin_app.clone().oneshot(replay_request).await.unwrap();
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
async fn google_workspace_delegated_live_acceptance_reaches_real_drive_about_when_env_is_available()
{
    if std::env::var("GOOGLE_WORKSPACE_DELEGATED_LIVE")
        .ok()
        .as_deref()
        != Some("1")
    {
        eprintln!("skip live google delegated acceptance test: GOOGLE_WORKSPACE_DELEGATED_LIVE!=1");
        return;
    }

    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        eprintln!("skip live google delegated acceptance test: database not available");
        return;
    };

    let listener = match tokio::net::TcpListener::bind("127.0.0.1:8080").await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!(
                "skip live google delegated acceptance test: cannot bind 127.0.0.1:8080: {error}"
            );
            let db_name: String = sqlx::query_scalar("SELECT current_database()")
                .fetch_one(&pool)
                .await
                .expect("should read temp database name");
            pool.close().await;
            drop_temp_database(&admin_pool, &db_name).await;
            admin_pool.close().await;
            return;
        }
    };
    let server_app = app.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, server_app).await.unwrap();
    });

    let fixture_path = real_google_workspace_oauth_client_path();
    let fixture_payload = match std::fs::read_to_string(&fixture_path) {
        Ok(content) => content,
        Err(error) => {
            server.abort();
            eprintln!("skip live google delegated acceptance test: cannot read fixture: {error}");
            let db_name: String = sqlx::query_scalar("SELECT current_database()")
                .fetch_one(&pool)
                .await
                .expect("should read temp database name");
            pool.close().await;
            drop_temp_database(&admin_pool, &db_name).await;
            admin_pool.close().await;
            return;
        }
    };
    let fixture_json: Value =
        serde_json::from_str(&fixture_payload).expect("oauth client fixture must be valid JSON");
    let client = fixture_json["web"].clone();
    let client_id = client["client_id"]
        .as_str()
        .expect("oauth client fixture must have client_id");
    let client_secret = client["client_secret"]
        .as_str()
        .expect("oauth client fixture must have client_secret");

    let admin_app = app.clone().layer(axum::Extension(create_admin_token()));
    let backing_credential_id = create_google_client_secret_credential(
        admin_app.clone(),
        "google-workspace-oauth-client-live",
        client_id,
        client_secret,
    )
    .await;

    let create_provider_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "google-workspace-delegated-live",
                "display_name": "Google Workspace Delegated Live",
                "provider_family": "google_workspace_delegated",
                "binding_kind": "delegated_user",
                "grant_family": "authorization_code_pkce",
                "authorization_endpoint": client["auth_uri"],
                "token_endpoint": client["token_uri"],
                "client_id": client_id,
                "client_auth_method": "none",
                "callback_mode": "platform",
                "adapter_key": "oauth_openid",
                "adapter_version": "v1",
                "scope_template": fixture_json["workspace"]["default_scopes"],
                "runtime_config": {
                    "redirect_uri": "http://localhost:8080/oauth/callback/google",
                    "transaction_ttl_minutes": 60,
                    "allowed_domains": ["www.googleapis.com"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": ["/drive/v3/about"],
                    "authorization_query": {
                        "access_type": "offline",
                        "prompt": "consent"
                    }
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

    let start_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/transactions/start")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "provider_definition_id": provider_definition_id,
                "alias": "google-live-yvan",
                "purpose": "ZKM-177 live acceptance",
                "subject_ref_hint": fixture_json["workspace"]["admin_email"],
                "subject_display_name": "Yvan Jiang",
                "backing_credential_id": backing_credential_id,
                "scopes": fixture_json["workspace"]["default_scopes"]
            })
            .to_string(),
        ))
        .unwrap();
    let start_response = admin_app.clone().oneshot(start_request).await.unwrap();
    assert_eq!(start_response.status(), StatusCode::OK);
    let started = read_json(start_response).await;
    let auth_url = started["data"]["authorization_url"]
        .as_str()
        .expect("authorization url should be present")
        .to_string();
    let transaction_id = started["data"]["transaction_id"]
        .as_str()
        .expect("transaction id should be present")
        .to_string();

    eprintln!("LIVE_GOOGLE_WORKSPACE_AUTH_URL={auth_url}");
    eprintln!("LIVE_GOOGLE_WORKSPACE_TRANSACTION_ID={transaction_id}");

    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(900);
    let binding_handle = loop {
        if tokio::time::Instant::now() >= deadline {
            panic!("timed out waiting for live Google delegated callback");
        }

        let found: Option<String> = sqlx::query_scalar(
            r#"
            SELECT binding_handle
            FROM public.oauth_bindings
            WHERE alias = 'google-live-yvan'
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(&pool)
        .await
        .expect("binding lookup should succeed");

        if let Some(binding_handle) = found {
            break binding_handle;
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    };

    let runtime_app = app.layer(axum::Extension(create_runtime_token(&binding_handle)));
    let invoke_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/runtime/invoke")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "binding_handle": binding_handle,
                "request": {
                    "method": fixture_json["workspace"]["runtime_target"]["method"],
                    "url": "https://www.googleapis.com/drive/v3/about",
                    "query": {
                        "fields": "user(displayName,emailAddress,permissionId),storageQuota"
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();
    let invoke_response = runtime_app.oneshot(invoke_request).await.unwrap();
    assert_eq!(invoke_response.status(), StatusCode::OK);
    let invoked = read_json(invoke_response).await;
    eprintln!("LIVE_GOOGLE_WORKSPACE_RUNTIME_RESPONSE={}", invoked);
    assert_eq!(invoked["success"], true);
    assert_eq!(invoked["data"]["status_code"], 200);
    assert!(invoked["data"]["body"]["user"].is_object());
    assert!(invoked["data"]["body"]["storageQuota"].is_object());

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
async fn google_workspace_delegated_binding_runtime_invoke_uses_binding_handle_after_callback_success()
 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock google listener should bind");
    let addr = listener.local_addr().unwrap();
    let id_token = fake_google_id_token(
        "google_sub_123",
        "yvanjiang@floatingdream.tech",
        "Yvan Jiang",
    );
    let mock_app = Router::new()
        .route(
            "/token",
            axum::routing::post(move || {
                let id_token = id_token.clone();
                async move {
                    axum::Json(json!({
                        "access_token": "ya29.mock_access_123",
                        "refresh_token": "1//mock_refresh_123",
                        "expires_in": 3600,
                        "scope": "openid email profile https://www.googleapis.com/auth/drive.metadata.readonly",
                        "token_type": "Bearer",
                        "id_token": id_token
                    }))
                }
            }),
        )
        .route(
            "/drive/v3/about",
            axum::routing::get(|| async {
                (
                    [(
                        axum::http::header::HeaderName::from_static("x-request-id"),
                        "google_drive_req_123",
                    )],
                    axum::Json(json!({
                        "user": {
                            "displayName": "Yvan Jiang",
                            "emailAddress": "yvanjiang@floatingdream.tech",
                            "permissionId": "perm_123"
                        },
                        "storageQuota": {
                            "limit": "1024",
                            "usage": "1",
                            "usageInDrive": "1"
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
        eprintln!("skip google delegated runtime invoke test: database not available");
        return;
    };

    let admin_app = app.clone().layer(axum::Extension(create_admin_token()));
    let backing_credential_id = create_google_client_secret_credential(
        admin_app.clone(),
        "google-workspace-oauth-client",
        "cli_google_123",
        "google_secret_mock",
    )
    .await;

    let provider_response = admin_app
        .clone()
        .oneshot(google_workspace_provider_request(
            "google-workspace-delegated-runtime",
            "Google Workspace Delegated Runtime",
            addr.port(),
        ))
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
                "alias": "google-delegated-yvan-runtime",
                "purpose": "Google Workspace delegated runtime invoke",
                "subject_ref_hint": "yvanjiang@floatingdream.tech",
                "subject_display_name": "Yvan Jiang",
                "backing_credential_id": backing_credential_id,
                "scopes": [
                    "openid",
                    "email",
                    "profile",
                    "https://www.googleapis.com/auth/drive.metadata.readonly"
                ]
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
            "/oauth-broker/callback/google?code=mock_code_123&state={state}"
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
                    "url": format!("http://127.0.0.1:{}/drive/v3/about", addr.port()),
                    "query": {
                        "fields": "user(displayName,emailAddress,permissionId),storageQuota"
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
    assert_eq!(invoked["data"]["status_code"], 200);
    assert_eq!(
        invoked["data"]["provider_request_id"],
        "google_drive_req_123"
    );
    assert_eq!(
        invoked["data"]["body"]["user"]["emailAddress"],
        "yvanjiang@floatingdream.tech"
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
