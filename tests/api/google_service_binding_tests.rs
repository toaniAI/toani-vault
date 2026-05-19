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
use vault_service::audit::MemoryAuditStorage;
use vault_service::crypto::hkdf::KeyHierarchy;
use vault_service::crypto::keys::HardwareRootKey;
use vault_service::services::db::{
    DatabaseConfig, DatabasePool, ensure_required_tables_on_startup,
};
use vault_service::tee::{Enclave, EnclaveConfig};
use vault_service::vault::storage::CredentialVault;

const GOOGLE_TEST_PRIVATE_KEY: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvwIBADANBgkqhkiG9w0BAQEFAASCBKkwggSlAgEAAoIBAQDE4xi6TcdIrlKW
O09mNtThBJfm2jfMq7lsMa7mSDtNp2miEbMKqXjQbaLoXniS2dwJO8kv3IQzXg0W
gz5tXc/Tva7rtdL1CUiUxDjS5H1FyWOj1s7bkBKrFbLmQBQrXxSzxzNA4vPRydU8
SFqQ4E7ah6KpR2mlgs+tjWNwm4AfzUEkOV979OMtJ0Ot8n7Mzz6D0akIwhDzagLm
9oxY53Jd0QQNW2RgNgsHRaTYSlKe4s7TbSKa3QtGdgODvKibQChxHUHFIBZ+mlHy
bYCn4C4KGUdsW6A0wV8Pd5dkeZgnvKQ1ZGi3bnDmpe82XDKT00Ix/z9aL91QIBxL
HwlRwJ17AgMBAAECggEASwsyLCSkLjg/g0KE/3Ury76X9WY8eXcEvE/tlZl3fSAv
25W6c/hnc64uNzp246ZFP4G5q9P10axp+ag5na7xnYfBidcqWrpYn1dxPzTW6Mgb
geHIw5hU/T/OigNnjKZ3ehSVnQhEHbS74XfEiU7tz05+ed4dzveel8x51/x1J+lE
zRyRu7jsRq6CykXLSWsHJ1MJNvyJAtClWNthYuGA6vQehYkt8KDBRbmeoA04MKN0
+BIKe5/KgRJR4ABmFUR+SLVP/77q76T39nPTWvDznc19IXMlhnwVTnM6jdEorqMe
bnAJF24MBzPBNANGbr5wCQKLygxTmtsZmRcu8zAWqQKBgQDz5d3GC5Ty/s+61uzq
GPd4427uTwoB6l9N8gxjh40UlVWuwV4p+mqDvtYELcrAIjcGSlCSQqgvDlbrexdP
8tHHxNj3TEpO+r1YN9+bZ/cexX/4uqVNdGzisdebN02jQACbytVZ6vyGo9Jgd1L9
ws2+g0YexvJgR9T+7TYxxC6J2QKBgQDOqBJIrGXnHYA4kk994ywAvFYTzJquhHOV
DarY8bVVTuJTJKDMSm9XB078655HccvEe2jLyeozBP0lhxKQDIGL9xmAbzPSbINf
Y3nLlG0DKsg0bEwC/eLNajWpt6K9pI2aDkRkjB/kdThTdNu1hqfDpgGMtdEuhHtB
zuaClGaZcwKBgQC0d9kiyr0bFHrG6HODQJgVBky13xwrkK2WckzCdLFqkplE5uXz
L80S0OlxTCTjCC4o5GI750ClGPot4fW8/ZJGPBzC19uAFz51gVpelo4fYcowVIMu
DcDn+Ontev1il2Ab5vj5QMw8IAnxwTlSdYthtabz7Qe5QE2VmBZqupwo+QKBgQCA
Y5LYa6LzrzRV8TBJubVAz8F6k4cWHVvhopgeKCzMTzH1DbCIu0Xo/7VnFMtE/8Hk
0/cLhOpnwBW2FvDFZb+mQWIqlOvRM3F69cZZYGFJsm6ngxDGWw1pKS8lvdzxjSYc
K/j5rsSxntHbp6JIaNwZhS05SkwnZk9dVzmGrsP/WQKBgQC8tUoLFzmMkm0Jx6N9
rjYQjFDaYcAtCsZMCQ53+n+JqL/YzTFSip53OXbGEol17x0VGhPqIRgWOBvFCA70
JYhtIWwnUjPCKvJtB2CbsnonP1Vqd1HCoCDis0VLkjEIg4Xfqo2bFe/qMrh5iED4
kLc2faOPoG+L/NIrXW6uiwGAJw==
-----END PRIVATE KEY-----"#;
const DEFAULT_GOOGLE_REAL_SERVICE_ACCOUNT_PATH: &str =
    "/Users/yvan/Projects/zkme-server/zkme-api/src/test/resources/config/config.json";

fn real_google_service_account_path() -> String {
    std::env::var("GOOGLE_SERVICE_ACCOUNT_FIXTURE_PATH")
        .unwrap_or_else(|_| DEFAULT_GOOGLE_REAL_SERVICE_ACCOUNT_PATH.to_string())
}

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
        "credbridge_google_service_binding_{}",
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
    .bind("google-service-binding-test")
    .bind("google service binding test tenant")
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
    .bind("Google Service Binding Test User")
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
        .merge(oauth_broker_routes(broker_state));

    Some((app, database_pool.pool().clone(), admin_pool))
}

async fn read_json(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&bytes).expect("response body should be valid json")
}

#[tokio::test]
async fn google_service_account_binding_validate_promotes_binding_to_ready_after_token_probe() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock google listener should bind");
    let addr = listener.local_addr().unwrap();
    let mock_app = Router::new().route(
        "/token",
        post(|| async {
            axum::Json(json!({
                "access_token": "google_access_token_mock",
                "token_type": "Bearer",
                "expires_in": 3600
            }))
        }),
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        server.abort();
        eprintln!("skip google service binding test: database not available");
        return;
    };

    let app = app.layer(axum::Extension(create_admin_token()));

    let create_credential_request = Request::builder()
        .method("POST")
        .uri("/credentials")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "service_id": "google-service-account",
                "credential_type": "api_key",
                "plaintext_data": {
                    "type": "service_account",
                    "project_id": "credbridge-test",
                    "private_key_id": "test-key-id",
                    "private_key": GOOGLE_TEST_PRIVATE_KEY,
                    "client_email": "credbridge-test@credbridge-test.iam.gserviceaccount.com",
                    "token_uri": format!("http://127.0.0.1:{}/token", addr.port())
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
                "alias": "google-service-account",
                "display_name": "Google Service Account",
                "provider_family": "google_service_account",
                "binding_kind": "provider_app_credential",
                "grant_family": "client_credentials",
                "token_endpoint": format!("http://127.0.0.1:{}/token", addr.port()),
                "client_auth_method": "private_key_jwt",
                "callback_mode": "none",
                "adapter_key": "google_service_account",
                "adapter_version": "v1",
                "scope_template": ["https://www.googleapis.com/auth/firebase.readonly"],
                "runtime_config": {
                    "base_url": "https://firebase.googleapis.com",
                    "allowed_domains": ["firebase.googleapis.com"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": ["/v1beta1/projects/credbridge-test"],
                    "connectivity_probe_url": format!("http://127.0.0.1:{}/token", addr.port())
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
                "alias": "google-sa-primary",
                "purpose": "Google service account runtime",
                "subject_ref": "credbridge-test@credbridge-test.iam.gserviceaccount.com",
                "subject_display_name": "CredBridge Test Service Account",
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
    assert_eq!(validated["data"]["health_status"], "ready");

    let stored_health: String = sqlx::query_scalar(
        r#"SELECT health_status FROM public.oauth_binding_runtime_states WHERE binding_id = $1"#,
    )
    .bind(Uuid::parse_str(binding_id).unwrap())
    .fetch_one(&pool)
    .await
    .expect("should read runtime state health");
    assert_eq!(stored_health, "ready");

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
async fn google_service_account_runtime_invoke_calls_governed_firebase_target() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock google listener should bind");
    let addr = listener.local_addr().unwrap();
    let project_id = "credbridge-test";
    let mock_app = Router::new()
        .route(
            "/token",
            post(|| async {
                axum::Json(json!({
                    "access_token": "google_access_token_mock",
                    "token_type": "Bearer",
                    "expires_in": 3600
                }))
            }),
        )
        .route(
            &format!("/v1beta1/projects/{project_id}/adminSdkConfig"),
            axum::routing::get(|| async {
                (
                    [(
                        axum::http::header::HeaderName::from_static("x-request-id"),
                        "google_req_mock_123",
                    )],
                    axum::Json(json!({
                        "projectId": "credbridge-test",
                        "databaseURL": "https://credbridge-test.firebaseio.com",
                        "storageBucket": "credbridge-test.appspot.com",
                        "locationId": "us-central"
                    })),
                )
            }),
        );
    let server = tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        server.abort();
        eprintln!("skip google service binding invoke test: database not available");
        return;
    };

    let admin_app = app.clone().layer(axum::Extension(create_admin_token()));

    let create_credential_request = Request::builder()
        .method("POST")
        .uri("/credentials")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "service_id": "google-service-account",
                "credential_type": "api_key",
                "plaintext_data": {
                    "type": "service_account",
                    "project_id": project_id,
                    "private_key_id": "test-key-id",
                    "private_key": GOOGLE_TEST_PRIVATE_KEY,
                    "client_email": "credbridge-test@credbridge-test.iam.gserviceaccount.com",
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
                "alias": "google-service-account-runtime",
                "display_name": "Google Service Account Runtime",
                "provider_family": "google_service_account",
                "binding_kind": "provider_app_credential",
                "grant_family": "client_credentials",
                "token_endpoint": format!("http://127.0.0.1:{}/token", addr.port()),
                "client_auth_method": "private_key_jwt",
                "callback_mode": "none",
                "adapter_key": "google_service_account",
                "adapter_version": "v1",
                "scope_template": ["https://www.googleapis.com/auth/firebase.readonly"],
                "runtime_config": {
                    "base_url": format!("http://127.0.0.1:{}", addr.port()),
                    "allowed_domains": ["127.0.0.1"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": [format!("/v1beta1/projects/{project_id}/adminSdkConfig")],
                    "connectivity_probe_url": format!("http://127.0.0.1:{}/token", addr.port())
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
                "alias": "google-sa-runtime-primary",
                "purpose": "Google service account runtime invoke",
                "subject_ref": "credbridge-test@credbridge-test.iam.gserviceaccount.com",
                "subject_display_name": "CredBridge Test Service Account",
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
                    "url": format!("http://127.0.0.1:{}/v1beta1/projects/{project_id}/adminSdkConfig", addr.port())
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
        "google_req_mock_123"
    );
    assert_eq!(invoked["data"]["body"]["projectId"], project_id);

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
async fn google_service_account_validate_reports_missing_scope_as_structured_failure() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock google listener should bind");
    let addr = listener.local_addr().unwrap();
    let mock_app = Router::new().route(
        "/token",
        post(|| async {
            axum::Json(json!({
                "access_token": "google_access_token_mock",
                "token_type": "Bearer",
                "expires_in": 3600
            }))
        }),
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        server.abort();
        eprintln!("skip google service binding scope failure test: database not available");
        return;
    };

    let app = app.layer(axum::Extension(create_admin_token()));

    let create_credential_request = Request::builder()
        .method("POST")
        .uri("/credentials")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "service_id": "google-service-account",
                "credential_type": "api_key",
                "plaintext_data": {
                    "type": "service_account",
                    "project_id": "credbridge-test",
                    "private_key_id": "test-key-id",
                    "private_key": GOOGLE_TEST_PRIVATE_KEY,
                    "client_email": "credbridge-test@credbridge-test.iam.gserviceaccount.com",
                    "token_uri": format!("http://127.0.0.1:{}/token", addr.port())
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
                "alias": "google-service-account-no-scope",
                "display_name": "Google Service Account No Scope",
                "provider_family": "google_service_account",
                "binding_kind": "provider_app_credential",
                "grant_family": "client_credentials",
                "token_endpoint": format!("http://127.0.0.1:{}/token", addr.port()),
                "client_auth_method": "private_key_jwt",
                "callback_mode": "none",
                "adapter_key": "google_service_account",
                "adapter_version": "v1",
                "scope_template": [],
                "runtime_config": {
                    "base_url": "https://firebase.googleapis.com",
                    "allowed_domains": ["firebase.googleapis.com"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": ["/v1beta1/projects/credbridge-test"],
                    "connectivity_probe_url": format!("http://127.0.0.1:{}/token", addr.port())
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
                "alias": "google-sa-no-scope-primary",
                "purpose": "Google service account no scope",
                "subject_ref": "credbridge-test@credbridge-test.iam.gserviceaccount.com",
                "subject_display_name": "CredBridge Test Service Account",
                "backing_credential_id": backing_credential_id
            })
            .to_string(),
        ))
        .unwrap();
    let binding_response = app.clone().oneshot(create_binding_request).await.unwrap();
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
    assert_eq!(validated["data"]["valid"], false);
    assert_eq!(validated["data"]["status"], "draft");
    assert_eq!(validated["data"]["health_status"], "validation_failed");
    assert!(
        validated["data"]["checks"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .any(|item| {
                item["name"] == "token_probe"
                    && item["status"] == "failed"
                    && item["detail"]
                        .as_str()
                        .unwrap_or_default()
                        .contains("at least one scope")
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
async fn google_service_account_runtime_invoke_rejects_provider_token_endpoint_even_with_query() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock google listener should bind");
    let addr = listener.local_addr().unwrap();
    let mock_app = Router::new().route(
        "/token",
        post(|| async {
            axum::Json(json!({
                "access_token": "google_access_token_mock",
                "token_type": "Bearer",
                "expires_in": 3600
            }))
        }),
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        server.abort();
        eprintln!("skip google token-endpoint deny test: database not available");
        return;
    };

    let admin_app = app.clone().layer(axum::Extension(create_admin_token()));

    let create_credential_request = Request::builder()
        .method("POST")
        .uri("/credentials")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "service_id": "google-service-account",
                "credential_type": "api_key",
                "plaintext_data": {
                    "type": "service_account",
                    "project_id": "credbridge-test",
                    "private_key_id": "test-key-id",
                    "private_key": GOOGLE_TEST_PRIVATE_KEY,
                    "client_email": "credbridge-test@credbridge-test.iam.gserviceaccount.com",
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
                "alias": "google-service-account-token-endpoint-deny",
                "display_name": "Google Service Account Token Endpoint Deny",
                "provider_family": "google_service_account",
                "binding_kind": "provider_app_credential",
                "grant_family": "client_credentials",
                "token_endpoint": format!("http://127.0.0.1:{}/token", addr.port()),
                "client_auth_method": "private_key_jwt",
                "callback_mode": "none",
                "adapter_key": "google_service_account",
                "adapter_version": "v1",
                "scope_template": ["https://www.googleapis.com/auth/firebase.readonly"],
                "runtime_config": {
                    "base_url": format!("http://127.0.0.1:{}", addr.port()),
                    "allowed_domains": ["127.0.0.1"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": ["/token"],
                    "connectivity_probe_url": format!("http://127.0.0.1:{}/token", addr.port())
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
                "alias": "google-sa-token-endpoint-primary",
                "purpose": "Google token endpoint deny",
                "subject_ref": "credbridge-test@credbridge-test.iam.gserviceaccount.com",
                "subject_display_name": "CredBridge Test Service Account",
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
                    "url": format!("http://127.0.0.1:{}/token?probe=1", addr.port())
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
            .contains("token endpoint")
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
async fn google_service_account_runtime_invoke_reaches_real_firebase_admin_sdk_config_when_fixture_is_available()
 {
    let Some((app, pool, admin_pool)) = setup_postgres_backed_app().await else {
        eprintln!("skip real google service binding test: database not available");
        return;
    };

    let service_account_path = real_google_service_account_path();
    let service_account_payload = match std::fs::read_to_string(&service_account_path) {
        Ok(content) => content,
        Err(error) => {
            eprintln!("skip real google service binding test: cannot read fixture: {error}");
            return;
        }
    };
    let service_account_json: Value = serde_json::from_str(&service_account_payload)
        .expect("service account fixture must be valid JSON");
    let project_id = service_account_json["project_id"]
        .as_str()
        .expect("service account fixture must have project_id")
        .to_string();
    let client_email = service_account_json["client_email"]
        .as_str()
        .expect("service account fixture must have client_email")
        .to_string();

    let admin_app = app.clone().layer(axum::Extension(create_admin_token()));

    let create_credential_request = Request::builder()
        .method("POST")
        .uri("/credentials")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "service_id": "google-service-account-real",
                "credential_type": "api_key",
                "plaintext_data": service_account_json
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
                "alias": "google-service-account-real",
                "display_name": "Google Service Account Real",
                "provider_family": "google_service_account",
                "binding_kind": "provider_app_credential",
                "grant_family": "client_credentials",
                "token_endpoint": "https://oauth2.googleapis.com/token",
                "client_auth_method": "private_key_jwt",
                "callback_mode": "none",
                "adapter_key": "google_service_account",
                "adapter_version": "v1",
                "scope_template": ["https://www.googleapis.com/auth/firebase.readonly"],
                "runtime_config": {
                    "base_url": "https://firebase.googleapis.com",
                    "allowed_domains": ["firebase.googleapis.com"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": [format!("/v1beta1/projects/{project_id}/adminSdkConfig")],
                    "connectivity_probe_url": "https://oauth2.googleapis.com/token"
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
                "alias": "google-sa-real-primary",
                "purpose": "Real firebase admin sdk config runtime",
                "subject_ref": client_email,
                "subject_display_name": "Firebase Admin SDK Service Account",
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
    eprintln!("REAL_GOOGLE_VALIDATE={validation}");
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
                    "url": format!("https://firebase.googleapis.com/v1beta1/projects/{project_id}/adminSdkConfig")
                }
            })
            .to_string(),
        ))
        .unwrap();
    let invoke_response = runtime_app.oneshot(invoke_request).await.unwrap();
    assert_eq!(invoke_response.status(), StatusCode::OK);
    let invoked = read_json(invoke_response).await;
    eprintln!("REAL_GOOGLE_INVOKE={invoked}");
    assert_eq!(invoked["success"], true);
    assert_eq!(invoked["data"]["status_code"], 200);
    assert_eq!(invoked["data"]["body"]["projectId"], project_id);
    assert!(invoked["data"]["provider_request_id"].is_null());

    let db_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .expect("should read temp database name");
    pool.close().await;
    drop_temp_database(&admin_pool, &db_name).await;
    admin_pool.close().await;
}
