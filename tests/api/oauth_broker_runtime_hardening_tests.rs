#![allow(clippy::field_reassign_with_default)]
#![allow(dead_code)]
#![allow(clippy::uninlined_format_args)]

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use tokio::sync::{Mutex, RwLock};
use tower::ServiceExt;
use uuid::Uuid;

use vault_service::api::approvals::{ApprovalApiState, approval_routes};
use vault_service::api::audit::MemoryAuditStorageAdapter;
use vault_service::api::credentials::{
    AppState as CredentialAppState, AuditLogger, DefaultAuditLogger, routes as credential_routes,
};
use vault_service::api::middleware::{TokenScope, ValidatedToken};
use vault_service::api::oauth_broker::{OAuthBrokerApiState, oauth_broker_routes};
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
        "credbridge_runtime_hardening_{}",
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
    .bind("oauth-runtime-hardening-test")
    .bind("oauth runtime hardening test tenant")
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
    .bind("OAuth Runtime Hardening Test User")
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
    .with_approval_pool(database_pool.pool().clone())
    .with_credential_runtime(
        credential_state.vault.clone(),
        credential_state.key_hierarchy.clone(),
        credential_state.enclave.clone(),
    );

    let app = Router::new()
        .merge(credential_routes().with_state(credential_state))
        .merge(oauth_broker_routes(broker_state))
        .nest(
            "/api/v1",
            approval_routes(ApprovalApiState::new(database_pool.pool().clone())),
        );

    Some((app, database_pool.pool().clone(), admin_pool))
}

async fn read_json(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&bytes).expect("response body should be valid json")
}

#[tokio::test]
async fn oauth_openid_runtime_invoke_enters_cooldown_then_rebind_required_after_refresh_failures() {
    let refresh_attempts = Arc::new(AtomicUsize::new(0));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock oauth listener should bind");
    let addr = listener.local_addr().unwrap();
    let refresh_attempts_for_route = refresh_attempts.clone();
    let mock_app = Router::new()
        .route(
            "/token",
            post(move || {
                let refresh_attempts = refresh_attempts_for_route.clone();
                async move {
                    match refresh_attempts.fetch_add(1, Ordering::SeqCst) {
                        0 => (
                            StatusCode::BAD_GATEWAY,
                            axum::Json(json!({
                                "error": "temporarily_unavailable",
                                "error_description": "transient upstream failure"
                            })),
                        )
                            .into_response(),
                        _ => (
                            StatusCode::BAD_REQUEST,
                            axum::Json(json!({
                                "error": "invalid_grant",
                                "error_description": "refresh token was revoked"
                            })),
                        )
                            .into_response(),
                    }
                }
            }),
        )
        .route(
            "/drive/v3/about",
            get(|| async { axum::Json(json!({ "ok": true })) }),
        );
    let server = tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        server.abort();
        eprintln!("skip oauth runtime hardening test: database not available");
        return;
    };

    let admin_app = app.clone().layer(axum::Extension(create_admin_token()));
    let create_credential_request = Request::builder()
        .method("POST")
        .uri("/credentials")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "service_id": "google-runtime-hardening-secret",
                "credential_type": "oauth_refresh",
                "plaintext_data": {
                    "refreshToken": "1//refresh_runtime_hardening",
                    "client_id": "google-client-id",
                    "client_secret": "google-client-secret",
                    "token_uri": format!("http://127.0.0.1:{}/token", addr.port())
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
                "alias": "google-runtime-hardening",
                "display_name": "Google Runtime Hardening",
                "provider_family": "google_workspace",
                "binding_kind": "delegated_user",
                "grant_family": "authorization_code_pkce",
                "authorization_endpoint": format!("http://127.0.0.1:{}/authorize", addr.port()),
                "token_endpoint": format!("http://127.0.0.1:{}/token", addr.port()),
                "client_id": "google-client-id",
                "client_auth_method": "client_secret_post",
                "callback_mode": "query",
                "adapter_key": "oauth_openid",
                "adapter_version": "v1",
                "scope_template": ["openid", "email", "profile"],
                "runtime_config": {
                    "redirect_uri": "http://localhost:8080/oauth-broker/callback/google",
                    "allowed_domains": ["127.0.0.1"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": ["/drive/v3/about"],
                    "cooldown_seconds": 60,
                    "rebind_after_refresh_failures": 2
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
                "alias": "google-runtime-hardening-binding",
                "purpose": "Google runtime hardening coverage",
                "subject_ref": "google-user-123",
                "subject_display_name": "Google Runtime Hardening User",
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
    let binding_status = binding_response.status();
    let binding_created = read_json(binding_response).await;
    assert_eq!(
        binding_status,
        StatusCode::OK,
        "binding creation should succeed: {}",
        binding_created
    );
    let binding_id = binding_created["data"]["id"]
        .as_str()
        .expect("binding id should be present")
        .to_string();
    let binding_handle = binding_created["data"]["binding_handle"]
        .as_str()
        .expect("binding handle should be present")
        .to_string();

    sqlx::query(
        r#"
        UPDATE public.oauth_bindings
        SET status = 'ready', updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(Uuid::parse_str(&binding_id).unwrap())
    .execute(&pool)
    .await
    .expect("should mark binding ready");
    sqlx::query(
        r#"
        UPDATE public.oauth_binding_runtime_states
        SET health_status = 'ready',
            last_error_code = NULL,
            last_error_message = NULL,
            cooldown_until = NULL,
            rebind_required = FALSE,
            updated_at = NOW()
        WHERE binding_id = $1
        "#,
    )
    .bind(Uuid::parse_str(&binding_id).unwrap())
    .execute(&pool)
    .await
    .expect("should mark runtime state ready");

    let runtime_app = app
        .clone()
        .layer(axum::Extension(create_runtime_token(&binding_handle)));
    let invoke_request = || {
        Request::builder()
            .method("POST")
            .uri("/oauth-broker/runtime/invoke")
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({
                    "binding_handle": binding_handle,
                    "request": {
                        "method": "GET",
                        "url": format!("http://127.0.0.1:{}/drive/v3/about", addr.port()),
                        "headers": {}
                    }
                })
                .to_string(),
            ))
            .unwrap()
    };

    let first_response = runtime_app.clone().oneshot(invoke_request()).await.unwrap();
    assert_eq!(first_response.status(), StatusCode::CONFLICT);
    let first_error = read_json(first_response).await;
    assert_eq!(first_error["success"], false);
    assert_eq!(first_error["error"], "conflict");
    assert!(
        first_error["message"]
            .as_str()
            .unwrap_or_default()
            .contains("cooldown"),
        "first refresh failure should surface cooldown semantics: {}",
        first_error
    );

    let binding_after_first = admin_app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/oauth-broker/bindings/{binding_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(binding_after_first.status(), StatusCode::OK);
    let binding_after_first = read_json(binding_after_first).await;
    assert_eq!(binding_after_first["data"]["health_status"], "cooldown");
    assert_eq!(binding_after_first["data"]["rebind_required"], false);

    let first_runtime_state = sqlx::query_as::<
        _,
        (
            String,
            Option<String>,
            bool,
            Option<chrono::DateTime<chrono::Utc>>,
        ),
    >(
        r#"
        SELECT health_status, last_error_code, rebind_required, cooldown_until
        FROM public.oauth_binding_runtime_states
        WHERE binding_id = $1
        "#,
    )
    .bind(Uuid::parse_str(&binding_id).unwrap())
    .fetch_one(&pool)
    .await
    .expect("should read runtime state after first failure");
    assert_eq!(first_runtime_state.0, "cooldown");
    assert_eq!(
        first_runtime_state.1.as_deref(),
        Some("token_refresh_failed")
    );
    assert!(!first_runtime_state.2);
    assert!(first_runtime_state.3.is_some());

    sqlx::query(
        r#"
        UPDATE public.oauth_binding_runtime_states
        SET cooldown_until = NOW() - INTERVAL '1 minute',
            updated_at = NOW()
        WHERE binding_id = $1
        "#,
    )
    .bind(Uuid::parse_str(&binding_id).unwrap())
    .execute(&pool)
    .await
    .expect("should expire cooldown for second attempt");

    let second_response = runtime_app.oneshot(invoke_request()).await.unwrap();
    assert_eq!(second_response.status(), StatusCode::CONFLICT);
    let second_error = read_json(second_response).await;
    assert_eq!(second_error["success"], false);
    assert_eq!(second_error["error"], "conflict");
    assert!(
        second_error["message"]
            .as_str()
            .unwrap_or_default()
            .contains("rebind"),
        "repeat refresh failure should surface rebind semantics: {}",
        second_error
    );

    let binding_after_second = admin_app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/oauth-broker/bindings/{binding_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(binding_after_second.status(), StatusCode::OK);
    let binding_after_second = read_json(binding_after_second).await;
    assert_eq!(
        binding_after_second["data"]["health_status"],
        "rebind_required"
    );
    assert_eq!(binding_after_second["data"]["rebind_required"], true);

    let second_runtime_state = sqlx::query_as::<_, (String, Option<String>, bool)>(
        r#"
        SELECT health_status, last_error_code, rebind_required
        FROM public.oauth_binding_runtime_states
        WHERE binding_id = $1
        "#,
    )
    .bind(Uuid::parse_str(&binding_id).unwrap())
    .fetch_one(&pool)
    .await
    .expect("should read runtime state after second failure");
    assert_eq!(second_runtime_state.0, "rebind_required");
    assert_eq!(
        second_runtime_state.1.as_deref(),
        Some("token_rebind_required")
    );
    assert!(second_runtime_state.2);

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
async fn oauth_openid_runtime_invoke_reuses_fresh_token_and_refreshes_expired_token() {
    let refresh_attempts = Arc::new(AtomicUsize::new(0));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock oauth listener should bind");
    let addr = listener.local_addr().unwrap();
    let refresh_attempts_for_route = refresh_attempts.clone();
    let mock_app = Router::new()
        .route(
            "/token",
            post(move || {
                let refresh_attempts = refresh_attempts_for_route.clone();
                async move {
                    refresh_attempts.fetch_add(1, Ordering::SeqCst);
                    axum::Json(json!({
                        "access_token": "refreshed_access_token",
                        "expires_in": 3600,
                        "token_type": "Bearer"
                    }))
                }
            }),
        )
        .route(
            "/drive/v3/about",
            get(|headers: axum::http::HeaderMap| async move {
                let authorization = headers
                    .get(axum::http::header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default()
                    .to_string();
                let body = match authorization.as_str() {
                    "Bearer cached_access_token" => json!({ "mode": "cached" }),
                    "Bearer refreshed_access_token" => json!({ "mode": "refreshed" }),
                    other => json!({ "mode": "unexpected", "authorization": other }),
                };
                (StatusCode::OK, axum::Json(body))
            }),
        );
    let server = tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        server.abort();
        eprintln!("skip oauth token reuse test: database not available");
        return;
    };

    let admin_app = app.clone().layer(axum::Extension(create_admin_token()));
    let create_provider_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "google-runtime-cache",
                "display_name": "Google Runtime Cache",
                "provider_family": "google_workspace",
                "binding_kind": "delegated_user",
                "grant_family": "authorization_code_pkce",
                "authorization_endpoint": format!("http://127.0.0.1:{}/authorize", addr.port()),
                "token_endpoint": format!("http://127.0.0.1:{}/token", addr.port()),
                "client_id": "google-client-id",
                "client_auth_method": "client_secret_post",
                "callback_mode": "query",
                "adapter_key": "oauth_openid",
                "adapter_version": "v1",
                "scope_template": ["openid", "email", "profile"],
                "runtime_config": {
                    "redirect_uri": "http://localhost:8080/oauth-broker/callback/google",
                    "allowed_domains": ["127.0.0.1"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": ["/drive/v3/about"]
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

    let create_binding_with_payload =
        |service_id: String, alias: String, plaintext_data: serde_json::Value| {
            let admin_app = admin_app.clone();
            let pool = pool.clone();
            let provider_definition_id = provider_definition_id.to_string();
            async move {
                let create_credential_request = Request::builder()
                    .method("POST")
                    .uri("/credentials")
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        json!({
                            "service_id": service_id,
                            "credential_type": "oauth_refresh",
                            "plaintext_data": plaintext_data
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

                let create_binding_request = Request::builder()
                    .method("POST")
                    .uri("/oauth-broker/bindings")
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        json!({
                            "provider_definition_id": provider_definition_id,
                            "alias": alias,
                            "purpose": format!("{alias} purpose"),
                            "subject_ref": format!("{alias}@example.com"),
                            "subject_display_name": alias,
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
                    .expect("binding id should be present")
                    .to_string();
                let binding_handle = binding_created["data"]["binding_handle"]
                    .as_str()
                    .expect("binding handle should be present")
                    .to_string();

                sqlx::query(
                    r#"
                UPDATE public.oauth_bindings
                SET status = 'ready', updated_at = NOW()
                WHERE id = $1
                "#,
                )
                .bind(Uuid::parse_str(&binding_id).unwrap())
                .execute(&pool)
                .await
                .expect("should mark binding ready");
                sqlx::query(
                    r#"
                UPDATE public.oauth_binding_runtime_states
                SET health_status = 'ready',
                    last_error_code = NULL,
                    last_error_message = NULL,
                    cooldown_until = NULL,
                    rebind_required = FALSE,
                    updated_at = NOW()
                WHERE binding_id = $1
                "#,
                )
                .bind(Uuid::parse_str(&binding_id).unwrap())
                .execute(&pool)
                .await
                .expect("should mark runtime state ready");

                (binding_handle, binding_id)
            }
        };

    let (fresh_binding_handle, _fresh_binding_id) = create_binding_with_payload(
        "oauth-fresh-token".to_string(),
        "oauth-fresh-token".to_string(),
        json!({
            "access_token": "cached_access_token",
            "access_token_expires_at": (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
            "refreshToken": "fresh_refresh_token",
            "client_id": "google-client-id",
            "client_secret": "google-client-secret",
            "token_uri": format!("http://127.0.0.1:{}/token", addr.port())
        }),
    )
    .await;

    let fresh_runtime_app = app
        .clone()
        .layer(axum::Extension(create_runtime_token(&fresh_binding_handle)));
    let fresh_invoke = fresh_runtime_app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/oauth-broker/runtime/invoke")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "binding_handle": fresh_binding_handle,
                        "request": {
                            "method": "GET",
                            "url": format!("http://127.0.0.1:{}/drive/v3/about", addr.port()),
                            "headers": {}
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(fresh_invoke.status(), StatusCode::OK);
    let fresh_invoked = read_json(fresh_invoke).await;
    assert_eq!(fresh_invoked["data"]["body"]["mode"], "cached");
    assert_eq!(refresh_attempts.load(Ordering::SeqCst), 0);

    let (expired_binding_handle, _expired_binding_id) = create_binding_with_payload(
        "oauth-expired-token".to_string(),
        "oauth-expired-token".to_string(),
        json!({
            "access_token": "expired_access_token",
            "access_token_expires_at": (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339(),
            "refreshToken": "expired_refresh_token",
            "client_id": "google-client-id",
            "client_secret": "google-client-secret",
            "token_uri": format!("http://127.0.0.1:{}/token", addr.port())
        }),
    )
    .await;

    let expired_runtime_app = app.clone().layer(axum::Extension(create_runtime_token(
        &expired_binding_handle,
    )));
    let expired_invoke = expired_runtime_app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/oauth-broker/runtime/invoke")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "binding_handle": expired_binding_handle,
                        "request": {
                            "method": "GET",
                            "url": format!("http://127.0.0.1:{}/drive/v3/about", addr.port()),
                            "headers": {}
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(expired_invoke.status(), StatusCode::OK);
    let expired_invoked = read_json(expired_invoke).await;
    assert_eq!(expired_invoked["data"]["body"]["mode"], "refreshed");
    assert_eq!(refresh_attempts.load(Ordering::SeqCst), 1);

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
async fn oauth_runtime_invoke_requires_approval_creates_pending_request_and_retries_after_approval()
{
    let token_exchange_attempts = Arc::new(AtomicUsize::new(0));
    let resource_call_attempts = Arc::new(AtomicUsize::new(0));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock oauth listener should bind");
    let addr = listener.local_addr().unwrap();
    let token_exchange_for_route = token_exchange_attempts.clone();
    let resource_call_for_route = resource_call_attempts.clone();
    let mock_app = Router::new()
        .route(
            "/token",
            post(move || {
                let token_exchange_for_route = token_exchange_for_route.clone();
                async move {
                    token_exchange_for_route.fetch_add(1, Ordering::SeqCst);
                    axum::Json(json!({
                        "access_token": "runtime_approval_access_token",
                        "token_type": "Bearer",
                        "expires_in": 3600
                    }))
                }
            }),
        )
        .route(
            "/drive/v3/about",
            get(move || {
                let resource_call_for_route = resource_call_for_route.clone();
                async move {
                    resource_call_for_route.fetch_add(1, Ordering::SeqCst);
                    axum::Json(json!({ "ok": true }))
                }
            }),
        );
    let server = tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        server.abort();
        eprintln!("skip oauth runtime approval test: database not available");
        return;
    };

    let admin_app = app.clone().layer(axum::Extension(create_admin_token()));
    let create_credential_request = Request::builder()
        .method("POST")
        .uri("/credentials")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "service_id": "oauth-runtime-approval-secret",
                "credential_type": "oauth_refresh",
                "requires_approval": true,
                "plaintext_data": {
                    "refreshToken": "1//runtime_approval_refresh",
                    "client_id": "google-client-id",
                    "client_secret": "google-client-secret",
                    "token_uri": format!("http://127.0.0.1:{}/token", addr.port())
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
                "alias": "google-runtime-approval",
                "display_name": "Google Runtime Approval",
                "provider_family": "google_workspace",
                "binding_kind": "delegated_user",
                "grant_family": "authorization_code_pkce",
                "authorization_endpoint": format!("http://127.0.0.1:{}/authorize", addr.port()),
                "token_endpoint": format!("http://127.0.0.1:{}/token", addr.port()),
                "client_id": "google-client-id",
                "client_auth_method": "client_secret_post",
                "callback_mode": "query",
                "adapter_key": "oauth_openid",
                "adapter_version": "v1",
                "scope_template": ["openid", "email", "profile"],
                "runtime_config": {
                    "redirect_uri": "http://localhost:8080/oauth-broker/callback/google",
                    "allowed_domains": ["127.0.0.1"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": ["/drive/v3/about"]
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
                "alias": "google-runtime-approval-binding",
                "purpose": "Google runtime approval coverage",
                "subject_ref": "google-user-approval",
                "subject_display_name": "Google Runtime Approval User",
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
        .expect("binding id should be present")
        .to_string();
    let binding_handle = binding_created["data"]["binding_handle"]
        .as_str()
        .expect("binding handle should be present")
        .to_string();

    sqlx::query(
        r#"
        UPDATE public.oauth_bindings
        SET status = 'ready', updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(Uuid::parse_str(&binding_id).unwrap())
    .execute(&pool)
    .await
    .expect("should mark binding ready");
    sqlx::query(
        r#"
        UPDATE public.oauth_binding_runtime_states
        SET health_status = 'ready',
            last_error_code = NULL,
            last_error_message = NULL,
            cooldown_until = NULL,
            rebind_required = FALSE,
            updated_at = NOW()
        WHERE binding_id = $1
        "#,
    )
    .bind(Uuid::parse_str(&binding_id).unwrap())
    .execute(&pool)
    .await
    .expect("should mark runtime state ready");

    let runtime_request_id = "runtime-request-approval-001";
    let runtime_app = app
        .clone()
        .layer(axum::Extension(create_runtime_token(&binding_handle)));
    let invoke_request = || {
        Request::builder()
            .method("POST")
            .uri("/oauth-broker/runtime/invoke")
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({
                    "binding_handle": binding_handle.clone(),
                    "request_id": runtime_request_id,
                    "request": {
                        "method": "GET",
                        "url": format!("http://127.0.0.1:{}/drive/v3/about", addr.port()),
                        "headers": {}
                    }
                })
                .to_string(),
            ))
            .unwrap()
    };

    let first_fut = runtime_app.clone().oneshot(invoke_request());
    let replay_fut = runtime_app.clone().oneshot(invoke_request());
    let (first_response, replay_response) = tokio::join!(first_fut, replay_fut);
    let first_response = first_response.unwrap();
    let replay_response = replay_response.unwrap();

    assert_eq!(first_response.status(), StatusCode::CONFLICT);
    assert_eq!(replay_response.status(), StatusCode::CONFLICT);

    let first_error = read_json(first_response).await;
    let replay_error = read_json(replay_response).await;
    assert_eq!(first_error["success"], false);
    assert_eq!(replay_error["success"], false);
    assert_eq!(first_error["error"], "conflict");
    assert_eq!(replay_error["error"], "conflict");
    assert!(
        first_error["message"]
            .as_str()
            .unwrap_or_default()
            .contains("waiting for approval"),
        "first invoke should be blocked by approval gate: {}",
        first_error
    );
    assert!(
        replay_error["message"]
            .as_str()
            .unwrap_or_default()
            .contains("waiting for approval"),
        "replay invoke should reuse pending approval instead of 500: {}",
        replay_error
    );
    assert_eq!(token_exchange_attempts.load(Ordering::SeqCst), 0);
    assert_eq!(resource_call_attempts.load(Ordering::SeqCst), 0);

    let pending_request = sqlx::query_as::<_, (Uuid, String)>(
        r#"
        SELECT approval_id, status
        FROM approval_requests
        WHERE tenant_id = $1
          AND business_type = 'credential_runtime_access'
          AND business_id = $2
        ORDER BY created_at DESC, approval_id DESC
        LIMIT 1
        "#,
    )
    .bind(Uuid::nil())
    .bind(runtime_request_id)
    .fetch_one(&pool)
    .await
    .expect("pending runtime approval request should exist");
    assert_eq!(pending_request.1, "pending");
    let pending_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM approval_requests
        WHERE tenant_id = $1
          AND business_type = 'credential_runtime_access'
          AND business_id = $2
          AND status = 'pending'
        "#,
    )
    .bind(Uuid::nil())
    .bind(runtime_request_id)
    .fetch_one(&pool)
    .await
    .expect("pending runtime approval request count query should succeed");
    assert_eq!(pending_count, 1);

    let approve_response = admin_app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/approvals/{}/approve", pending_request.0))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({ "remark": "approved by runtime test" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approve_response.status(), StatusCode::OK);
    let approved = read_json(approve_response).await;
    assert_eq!(approved["data"]["status"], "approved");

    let retry_response = runtime_app.clone().oneshot(invoke_request()).await.unwrap();
    assert_eq!(retry_response.status(), StatusCode::OK);
    let retried = read_json(retry_response).await;
    assert_eq!(retried["success"], true);
    assert_eq!(token_exchange_attempts.load(Ordering::SeqCst), 1);
    assert_eq!(resource_call_attempts.load(Ordering::SeqCst), 1);

    let business_result = sqlx::query_as::<
        _,
        (
            String,
            Option<String>,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<Uuid>,
        ),
    >(
        r#"
        SELECT status, result_code, consumed_at, reservation_id
        FROM approval_business_results
        WHERE tenant_id = $1
          AND business_type = 'credential_runtime_access'
          AND business_id = $2
        "#,
    )
    .bind(Uuid::nil())
    .bind(runtime_request_id)
    .fetch_one(&pool)
    .await
    .expect("runtime approval business writeback should exist");
    assert_eq!(business_result.0, "approved");
    assert_eq!(business_result.1.as_deref(), Some("approved"));
    assert!(business_result.2.is_some());
    assert!(business_result.3.is_none());

    let second_retry_response = runtime_app.clone().oneshot(invoke_request()).await.unwrap();
    assert_eq!(second_retry_response.status(), StatusCode::CONFLICT);
    let second_retry_error = read_json(second_retry_response).await;
    assert_eq!(second_retry_error["success"], false);
    assert_eq!(second_retry_error["error"], "conflict");
    assert!(
        second_retry_error["message"]
            .as_str()
            .unwrap_or_default()
            .contains("waiting for approval"),
        "second invoke after successful runtime access should require reapproval: {}",
        second_retry_error
    );

    let second_pending_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM approval_requests
        WHERE tenant_id = $1
          AND business_type = 'credential_runtime_access'
          AND business_id = $2
          AND status = 'pending'
        "#,
    )
    .bind(Uuid::nil())
    .bind(runtime_request_id)
    .fetch_one(&pool)
    .await
    .expect("second pending runtime approval request count query should succeed");
    assert_eq!(second_pending_count, 1);

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
async fn oauth_runtime_invoke_reject_and_cancel_make_original_request_fail() {
    let token_exchange_attempts = Arc::new(AtomicUsize::new(0));
    let resource_call_attempts = Arc::new(AtomicUsize::new(0));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock oauth listener should bind");
    let addr = listener.local_addr().unwrap();
    let token_exchange_for_route = token_exchange_attempts.clone();
    let resource_call_for_route = resource_call_attempts.clone();
    let mock_app = Router::new()
        .route(
            "/token",
            post(move || {
                let token_exchange_for_route = token_exchange_for_route.clone();
                async move {
                    token_exchange_for_route.fetch_add(1, Ordering::SeqCst);
                    axum::Json(json!({
                        "access_token": "runtime_terminal_access_token",
                        "token_type": "Bearer",
                        "expires_in": 3600
                    }))
                }
            }),
        )
        .route(
            "/drive/v3/about",
            get(move || {
                let resource_call_for_route = resource_call_for_route.clone();
                async move {
                    resource_call_for_route.fetch_add(1, Ordering::SeqCst);
                    axum::Json(json!({ "ok": true }))
                }
            }),
        );
    let server = tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        server.abort();
        eprintln!("skip oauth runtime terminal approval test: database not available");
        return;
    };

    let admin_app = app.clone().layer(axum::Extension(create_admin_token()));
    let create_credential_request = Request::builder()
        .method("POST")
        .uri("/credentials")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "service_id": "oauth-runtime-terminal-secret",
                "credential_type": "oauth_refresh",
                "requires_approval": true,
                "plaintext_data": {
                    "refreshToken": "1//runtime_terminal_refresh",
                    "client_id": "google-client-id",
                    "client_secret": "google-client-secret",
                    "token_uri": format!("http://127.0.0.1:{}/token", addr.port())
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
                "alias": "google-runtime-terminal",
                "display_name": "Google Runtime Terminal",
                "provider_family": "google_workspace",
                "binding_kind": "delegated_user",
                "grant_family": "authorization_code_pkce",
                "authorization_endpoint": format!("http://127.0.0.1:{}/authorize", addr.port()),
                "token_endpoint": format!("http://127.0.0.1:{}/token", addr.port()),
                "client_id": "google-client-id",
                "client_auth_method": "client_secret_post",
                "callback_mode": "query",
                "adapter_key": "oauth_openid",
                "adapter_version": "v1",
                "scope_template": ["openid", "email", "profile"],
                "runtime_config": {
                    "redirect_uri": "http://localhost:8080/oauth-broker/callback/google",
                    "allowed_domains": ["127.0.0.1"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": ["/drive/v3/about"]
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
                "alias": "google-runtime-terminal-binding",
                "purpose": "Google runtime terminal coverage",
                "subject_ref": "google-user-terminal",
                "subject_display_name": "Google Runtime Terminal User",
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
        .expect("binding id should be present")
        .to_string();
    let binding_handle = binding_created["data"]["binding_handle"]
        .as_str()
        .expect("binding handle should be present")
        .to_string();

    sqlx::query(
        r#"
        UPDATE public.oauth_bindings
        SET status = 'ready', updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(Uuid::parse_str(&binding_id).unwrap())
    .execute(&pool)
    .await
    .expect("should mark binding ready");
    sqlx::query(
        r#"
        UPDATE public.oauth_binding_runtime_states
        SET health_status = 'ready',
            last_error_code = NULL,
            last_error_message = NULL,
            cooldown_until = NULL,
            rebind_required = FALSE,
            updated_at = NOW()
        WHERE binding_id = $1
        "#,
    )
    .bind(Uuid::parse_str(&binding_id).unwrap())
    .execute(&pool)
    .await
    .expect("should mark runtime state ready");

    let runtime_app = app
        .clone()
        .layer(axum::Extension(create_runtime_token(&binding_handle)));

    for (request_id, transition_action, terminal_status, remark) in [
        (
            "runtime-request-rejected-001",
            "reject",
            "rejected",
            "rejected by runtime terminal test",
        ),
        (
            "runtime-request-cancelled-001",
            "cancel",
            "cancelled",
            "cancelled by runtime terminal test",
        ),
    ] {
        let first_response = runtime_app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/oauth-broker/runtime/invoke")
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        json!({
                            "binding_handle": binding_handle.clone(),
                            "request_id": request_id,
                            "request": {
                                "method": "GET",
                                "url": format!("http://127.0.0.1:{}/drive/v3/about", addr.port()),
                                "headers": {}
                            }
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first_response.status(), StatusCode::CONFLICT);
        let first_error = read_json(first_response).await;
        assert_eq!(first_error["success"], false);
        assert_eq!(first_error["error"], "conflict");
        assert!(
            first_error["message"]
                .as_str()
                .unwrap_or_default()
                .contains("waiting for approval"),
            "runtime request should be blocked by pending approval: {}",
            first_error
        );

        let pending_request = sqlx::query_as::<_, (Uuid, String)>(
            r#"
            SELECT approval_id, status
            FROM approval_requests
            WHERE tenant_id = $1
              AND business_type = 'credential_runtime_access'
              AND business_id = $2
            ORDER BY created_at DESC, approval_id DESC
            LIMIT 1
            "#,
        )
        .bind(Uuid::nil())
        .bind(request_id)
        .fetch_one(&pool)
        .await
        .expect("pending runtime approval request should exist");
        assert_eq!(pending_request.1, "pending");

        let transition_response = admin_app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/api/v1/approvals/{}/{}",
                        pending_request.0, transition_action
                    ))
                    .header("Content-Type", "application/json")
                    .body(Body::from(json!({ "remark": remark }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(transition_response.status(), StatusCode::OK);
        let transitioned = read_json(transition_response).await;
        assert_eq!(transitioned["data"]["status"], terminal_status);
        assert_eq!(transitioned["data"]["remark"], remark);

        let persisted_terminal_state = sqlx::query_as::<_, (String, Option<String>)>(
            r#"
            SELECT status, remark
            FROM approval_requests
            WHERE approval_id = $1
            "#,
        )
        .bind(pending_request.0)
        .fetch_one(&pool)
        .await
        .expect("terminal approval request should persist");
        assert_eq!(persisted_terminal_state.0, terminal_status);
        assert_eq!(persisted_terminal_state.1.as_deref(), Some(remark));

        let business_result = sqlx::query_as::<
            _,
            (
                String,
                Option<String>,
                Option<chrono::DateTime<chrono::Utc>>,
                Option<Uuid>,
            ),
        >(
            r#"
            SELECT status, result_code, consumed_at, reservation_id
            FROM approval_business_results
            WHERE tenant_id = $1
              AND business_type = 'credential_runtime_access'
              AND business_id = $2
            "#,
        )
        .bind(Uuid::nil())
        .bind(request_id)
        .fetch_one(&pool)
        .await
        .expect("runtime approval business writeback should exist");
        assert_eq!(business_result.0, terminal_status);
        assert_eq!(business_result.1.as_deref(), Some(terminal_status));
        assert!(business_result.2.is_none());
        assert!(business_result.3.is_none());

        let retry_response = runtime_app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/oauth-broker/runtime/invoke")
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        json!({
                            "binding_handle": binding_handle.clone(),
                            "request_id": request_id,
                            "request": {
                                "method": "GET",
                                "url": format!("http://127.0.0.1:{}/drive/v3/about", addr.port()),
                                "headers": {}
                            }
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(retry_response.status(), StatusCode::CONFLICT);
        let retry_error = read_json(retry_response).await;
        assert_eq!(retry_error["success"], false);
        assert_eq!(retry_error["error"], "conflict");
        assert!(
            retry_error["message"]
                .as_str()
                .unwrap_or_default()
                .contains("waiting for approval"),
            "runtime retry after terminal decision should create a new pending approval: {}",
            retry_error
        );
        assert_eq!(token_exchange_attempts.load(Ordering::SeqCst), 0);
        assert_eq!(resource_call_attempts.load(Ordering::SeqCst), 0);

        let pending_after_terminal: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM approval_requests
            WHERE tenant_id = $1
              AND business_type = 'credential_runtime_access'
              AND business_id = $2
              AND status = 'pending'
            "#,
        )
        .bind(Uuid::nil())
        .bind(request_id)
        .fetch_one(&pool)
        .await
        .expect("pending runtime approval request count after terminal retry should succeed");
        assert_eq!(pending_after_terminal, 1);
    }

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
async fn oauth_runtime_invoke_releases_approval_when_token_exchange_fails() {
    let unused_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("unused listener should bind");
    let unused_port = unused_listener.local_addr().unwrap().port();
    drop(unused_listener);

    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        eprintln!("skip oauth runtime approval release test: database not available");
        return;
    };

    let admin_app = app.clone().layer(axum::Extension(create_admin_token()));
    let create_credential_request = Request::builder()
        .method("POST")
        .uri("/credentials")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "service_id": "oauth-runtime-release-secret",
                "credential_type": "oauth_refresh",
                "requires_approval": true,
                "plaintext_data": {
                    "refreshToken": "1//runtime_release_refresh",
                    "client_id": "google-client-id",
                    "client_secret": "google-client-secret",
                    "token_uri": format!("http://127.0.0.1:{unused_port}/token")
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
                "alias": "google-runtime-release",
                "display_name": "Google Runtime Release",
                "provider_family": "google_workspace",
                "binding_kind": "delegated_user",
                "grant_family": "authorization_code_pkce",
                "authorization_endpoint": format!("http://127.0.0.1:{unused_port}/authorize"),
                "token_endpoint": format!("http://127.0.0.1:{unused_port}/token"),
                "client_id": "google-client-id",
                "client_auth_method": "client_secret_post",
                "callback_mode": "query",
                "adapter_key": "oauth_openid",
                "adapter_version": "v1",
                "scope_template": ["openid", "email", "profile"],
                "runtime_config": {
                    "redirect_uri": "http://localhost:8080/oauth-broker/callback/google",
                    "allowed_domains": ["127.0.0.1"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": ["/drive/v3/about"]
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
                "alias": "google-runtime-release-binding",
                "purpose": "Google runtime approval release coverage",
                "subject_ref": "google-user-release",
                "subject_display_name": "Google Runtime Release User",
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
        .expect("binding id should be present")
        .to_string();
    let binding_handle = binding_created["data"]["binding_handle"]
        .as_str()
        .expect("binding handle should be present")
        .to_string();

    sqlx::query(
        r#"
        UPDATE public.oauth_bindings
        SET status = 'ready', updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(Uuid::parse_str(&binding_id).unwrap())
    .execute(&pool)
    .await
    .expect("should mark binding ready");
    sqlx::query(
        r#"
        UPDATE public.oauth_binding_runtime_states
        SET health_status = 'ready',
            last_error_code = NULL,
            last_error_message = NULL,
            cooldown_until = NULL,
            rebind_required = FALSE,
            updated_at = NOW()
        WHERE binding_id = $1
        "#,
    )
    .bind(Uuid::parse_str(&binding_id).unwrap())
    .execute(&pool)
    .await
    .expect("should mark runtime state ready");

    let request_id = "runtime-request-release-001";
    let runtime_app = app
        .clone()
        .layer(axum::Extension(create_runtime_token(&binding_handle)));
    let invoke_request = || {
        Request::builder()
            .method("POST")
            .uri("/oauth-broker/runtime/invoke")
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({
                    "binding_handle": binding_handle.clone(),
                    "request_id": request_id,
                    "request": {
                        "method": "GET",
                        "url": format!("http://127.0.0.1:{unused_port}/drive/v3/about"),
                        "headers": {}
                    }
                })
                .to_string(),
            ))
            .unwrap()
    };

    let first_response = runtime_app.clone().oneshot(invoke_request()).await.unwrap();
    assert_eq!(first_response.status(), StatusCode::CONFLICT);
    let first_error = read_json(first_response).await;
    assert!(
        first_error["message"]
            .as_str()
            .unwrap_or_default()
            .contains("waiting for approval")
    );

    let pending_request = sqlx::query_as::<_, (Uuid,)>(
        r#"
        SELECT approval_id
        FROM approval_requests
        WHERE tenant_id = $1
          AND business_type = 'credential_runtime_access'
          AND business_id = $2
          AND status = 'pending'
        ORDER BY created_at DESC, approval_id DESC
        LIMIT 1
        "#,
    )
    .bind(Uuid::nil())
    .bind(request_id)
    .fetch_one(&pool)
    .await
    .expect("pending runtime approval request should exist");

    let approve_response = admin_app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/approvals/{}/approve", pending_request.0))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({ "remark": "approved before token exchange failure" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approve_response.status(), StatusCode::OK);

    let failed_response = runtime_app.clone().oneshot(invoke_request()).await.unwrap();
    assert_eq!(failed_response.status(), StatusCode::CONFLICT);

    let released_business_result = sqlx::query_as::<
        _,
        (
            String,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<Uuid>,
            Option<chrono::DateTime<chrono::Utc>>,
        ),
    >(
        r#"
        SELECT status, consumed_at, reservation_id, reserved_at
        FROM approval_business_results
        WHERE tenant_id = $1
          AND business_type = 'credential_runtime_access'
          AND business_id = $2
        "#,
    )
    .bind(Uuid::nil())
    .bind(request_id)
    .fetch_one(&pool)
    .await
    .expect("runtime approval business result should remain reusable after token exchange failure");
    assert_eq!(released_business_result.0, "approved");
    assert!(released_business_result.1.is_none());
    assert!(released_business_result.2.is_none());
    assert!(released_business_result.3.is_none());

    let retry_response = runtime_app.clone().oneshot(invoke_request()).await.unwrap();
    assert_eq!(retry_response.status(), StatusCode::CONFLICT);

    let pending_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM approval_requests
        WHERE tenant_id = $1
          AND business_type = 'credential_runtime_access'
          AND business_id = $2
          AND status = 'pending'
        "#,
    )
    .bind(Uuid::nil())
    .bind(request_id)
    .fetch_one(&pool)
    .await
    .expect("token exchange failure should not create a new pending approval");
    assert_eq!(pending_count, 0);

    let db_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .expect("should read temp database name");
    pool.close().await;
    drop_temp_database(&admin_pool, &db_name).await;
    admin_pool.close().await;
}

#[tokio::test]
async fn oauth_runtime_invoke_without_approval_flag_skips_approval_request_creation() {
    let token_exchange_attempts = Arc::new(AtomicUsize::new(0));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock oauth listener should bind");
    let addr = listener.local_addr().unwrap();
    let token_exchange_for_route = token_exchange_attempts.clone();
    let mock_app = Router::new()
        .route(
            "/token",
            post(move || {
                let token_exchange_for_route = token_exchange_for_route.clone();
                async move {
                    token_exchange_for_route.fetch_add(1, Ordering::SeqCst);
                    axum::Json(json!({
                        "access_token": "runtime_no_approval_access_token",
                        "token_type": "Bearer",
                        "expires_in": 3600
                    }))
                }
            }),
        )
        .route(
            "/drive/v3/about",
            get(|| async { axum::Json(json!({ "ok": true })) }),
        );
    let server = tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        server.abort();
        eprintln!("skip oauth runtime approval bypass test: database not available");
        return;
    };

    let admin_app = app.clone().layer(axum::Extension(create_admin_token()));
    let create_credential_request = Request::builder()
        .method("POST")
        .uri("/credentials")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "service_id": "oauth-runtime-no-approval-secret",
                "credential_type": "oauth_refresh",
                "requires_approval": false,
                "plaintext_data": {
                    "refreshToken": "1//runtime_no_approval_refresh",
                    "client_id": "google-client-id",
                    "client_secret": "google-client-secret",
                    "token_uri": format!("http://127.0.0.1:{}/token", addr.port())
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
                "alias": "google-runtime-no-approval",
                "display_name": "Google Runtime No Approval",
                "provider_family": "google_workspace",
                "binding_kind": "delegated_user",
                "grant_family": "authorization_code_pkce",
                "authorization_endpoint": format!("http://127.0.0.1:{}/authorize", addr.port()),
                "token_endpoint": format!("http://127.0.0.1:{}/token", addr.port()),
                "client_id": "google-client-id",
                "client_auth_method": "client_secret_post",
                "callback_mode": "query",
                "adapter_key": "oauth_openid",
                "adapter_version": "v1",
                "scope_template": ["openid", "email", "profile"],
                "runtime_config": {
                    "redirect_uri": "http://localhost:8080/oauth-broker/callback/google",
                    "allowed_domains": ["127.0.0.1"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": ["/drive/v3/about"]
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
                "alias": "google-runtime-no-approval-binding",
                "purpose": "Google runtime no approval coverage",
                "subject_ref": "google-user-no-approval",
                "subject_display_name": "Google Runtime No Approval User",
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
        .expect("binding id should be present")
        .to_string();
    let binding_handle = binding_created["data"]["binding_handle"]
        .as_str()
        .expect("binding handle should be present")
        .to_string();

    sqlx::query(
        r#"
        UPDATE public.oauth_bindings
        SET status = 'ready', updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(Uuid::parse_str(&binding_id).unwrap())
    .execute(&pool)
    .await
    .expect("should mark binding ready");
    sqlx::query(
        r#"
        UPDATE public.oauth_binding_runtime_states
        SET health_status = 'ready',
            last_error_code = NULL,
            last_error_message = NULL,
            cooldown_until = NULL,
            rebind_required = FALSE,
            updated_at = NOW()
        WHERE binding_id = $1
        "#,
    )
    .bind(Uuid::parse_str(&binding_id).unwrap())
    .execute(&pool)
    .await
    .expect("should mark runtime state ready");

    let runtime_request_id = "runtime-request-no-approval-001";
    let runtime_app = app
        .clone()
        .layer(axum::Extension(create_runtime_token(&binding_handle)));
    let runtime_response = runtime_app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/oauth-broker/runtime/invoke")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "binding_handle": binding_handle,
                        "request_id": runtime_request_id,
                        "request": {
                            "method": "GET",
                            "url": format!("http://127.0.0.1:{}/drive/v3/about", addr.port()),
                            "headers": {}
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(runtime_response.status(), StatusCode::OK);
    assert_eq!(token_exchange_attempts.load(Ordering::SeqCst), 1);

    let pending_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM approval_requests
        WHERE tenant_id = $1
          AND business_type = 'credential_runtime_access'
          AND business_id = $2
        "#,
    )
    .bind(Uuid::nil())
    .bind(runtime_request_id)
    .fetch_one(&pool)
    .await
    .expect("approval request count query should succeed");
    assert_eq!(pending_count, 0);

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
async fn binding_lifecycle_supports_staged_policy_apply_then_revoke_and_delete() {
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

    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        server.abort();
        eprintln!("skip binding lifecycle hardening test: database not available");
        return;
    };

    let admin_app = app.clone().layer(axum::Extension(create_admin_token()));
    let create_credential_request = Request::builder()
        .method("POST")
        .uri("/credentials")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "service_id": "lark-lifecycle-secret",
                "credential_type": "api_key",
                "allowed_domains": ["127.0.0.1"],
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
                "alias": "lark-lifecycle-provider",
                "display_name": "Lark Lifecycle Provider",
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
                "alias": "lark-lifecycle-binding",
                "purpose": "Lark lifecycle coverage",
                "subject_ref": "tenant_access_token",
                "subject_display_name": "Lark Lifecycle Binding",
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
    let binding_status = binding_response.status();
    let binding_created = read_json(binding_response).await;
    assert_eq!(
        binding_status,
        StatusCode::OK,
        "binding creation should succeed: {}",
        binding_created
    );
    let binding_id = binding_created["data"]["id"]
        .as_str()
        .expect("binding id should be present")
        .to_string();

    let validate_binding_response = admin_app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/oauth-broker/bindings/{binding_id}/validate"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(validate_binding_response.status(), StatusCode::OK);

    let staged_policy_response = admin_app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/oauth-broker/bindings/{binding_id}/staged-policy"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "granted_scopes": ["contact:scope:readonly", "contact:scope:write"],
                        "effective_scopes": ["contact:scope:readonly"],
                        "allowed_domains": ["api.example.com"],
                        "allowed_methods": ["POST"],
                        "allowed_path_prefixes": ["/v2/contact"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(staged_policy_response.status(), StatusCode::OK);
    let staged_policy = read_json(staged_policy_response).await;
    assert_eq!(
        staged_policy["data"]["validation_status"],
        "pending_validation"
    );

    let revalidate_response = admin_app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/oauth-broker/bindings/{binding_id}/staged-policy/revalidate"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revalidate_response.status(), StatusCode::OK);
    let revalidated = read_json(revalidate_response).await;
    assert_eq!(revalidated["data"]["valid"], true);
    assert_eq!(revalidated["data"]["validation_status"], "validated");

    let apply_response = admin_app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/oauth-broker/bindings/{binding_id}/staged-policy/apply"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(apply_response.status(), StatusCode::OK);
    let applied = read_json(apply_response).await;
    assert_eq!(applied["data"]["version"], 2);

    let applied_policy = sqlx::query_as::<_, (i32, Vec<String>, Vec<String>, Vec<String>)>(
        r#"
        SELECT version,
               ARRAY(SELECT jsonb_array_elements_text(allowed_domains)) AS allowed_domains,
               ARRAY(SELECT jsonb_array_elements_text(allowed_methods)) AS allowed_methods,
               ARRAY(SELECT jsonb_array_elements_text(allowed_path_prefixes)) AS allowed_path_prefixes
        FROM public.oauth_binding_policy_snapshots
        WHERE binding_id = $1
          AND applied_at IS NOT NULL
        ORDER BY version DESC
        LIMIT 1
        "#,
    )
    .bind(Uuid::parse_str(&binding_id).unwrap())
    .fetch_one(&pool)
    .await
    .expect("should read applied staged policy");
    assert_eq!(applied_policy.0, 2);
    assert_eq!(applied_policy.1, vec!["api.example.com".to_string()]);
    assert_eq!(applied_policy.2, vec!["POST".to_string()]);
    assert_eq!(applied_policy.3, vec!["/v2/contact".to_string()]);

    let revoke_response = admin_app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/oauth-broker/bindings/{binding_id}/revoke"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoke_response.status(), StatusCode::OK);
    let revoked = read_json(revoke_response).await;
    assert_eq!(revoked["data"]["status"], "revoked");
    assert_eq!(revoked["data"]["health_status"], "revoked");

    let delete_response = admin_app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/oauth-broker/bindings/{binding_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete_response.status(), StatusCode::OK);
    let deleted = read_json(delete_response).await;
    assert_eq!(deleted["data"]["deleted"], true);

    let remaining_bindings: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM public.oauth_bindings
        WHERE id = $1
        "#,
    )
    .bind(Uuid::parse_str(&binding_id).unwrap())
    .fetch_one(&pool)
    .await
    .expect("should count remaining bindings");
    assert_eq!(remaining_bindings, 0);

    let db_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .expect("should read temp database name");
    pool.close().await;
    drop_temp_database(&admin_pool, &db_name).await;
    admin_pool.close().await;
    server.abort();
}
