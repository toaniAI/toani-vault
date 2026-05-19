#![allow(clippy::field_reassign_with_default)]
#![allow(dead_code)]
#![allow(clippy::uninlined_format_args)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use uuid::Uuid;

use vault_service::api::{
    middleware::{TokenScope, ValidatedToken},
    oauth_broker::{OAuthBrokerApiState, oauth_broker_public_routes, oauth_broker_routes},
};
use vault_service::oauth_broker::{
    AuthTransaction, AuthTransactionStatus, Binding, BindingPolicySnapshot, BindingStatus,
    CreateBindingInput, CreateProviderDefinitionInput, OAuthBrokerError, OAuthBrokerService,
    PgOAuthBrokerService, ProviderDefinition, ProviderValidationCheck, ProviderValidationResult,
    StartAuthTransactionInput, UpdateProviderDefinitionInput,
};
use vault_service::services::db::{
    DatabaseConfig, DatabasePool, ensure_required_tables_on_startup,
};

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

#[derive(Default)]
struct MemoryProviderRegistryService;

#[async_trait::async_trait]
impl OAuthBrokerService for MemoryProviderRegistryService {
    async fn create_provider_definition(
        &self,
        input: CreateProviderDefinitionInput,
    ) -> Result<ProviderDefinition, OAuthBrokerError> {
        Ok(ProviderDefinition::from_input(input))
    }

    async fn list_provider_definitions(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<ProviderDefinition>, OAuthBrokerError> {
        Ok(vec![])
    }

    async fn get_provider_definition(
        &self,
        provider_definition_id: Uuid,
    ) -> Result<Option<ProviderDefinition>, OAuthBrokerError> {
        Ok(Some(ProviderDefinition::new(
            provider_definition_id,
            Uuid::nil(),
            "lark-app".to_string(),
            "Lark App".to_string(),
            "lark_app".to_string(),
            vault_service::oauth_broker::BindingKind::ProviderAppCredential,
            vault_service::oauth_broker::GrantFamily::ClientCredentials,
        )))
    }

    async fn update_provider_definition(
        &self,
        _tenant_id: Uuid,
        provider_definition_id: Uuid,
        input: UpdateProviderDefinitionInput,
    ) -> Result<ProviderDefinition, OAuthBrokerError> {
        let mut provider = ProviderDefinition::new(
            provider_definition_id,
            Uuid::nil(),
            "lark-app".to_string(),
            "Lark App".to_string(),
            "lark_app".to_string(),
            vault_service::oauth_broker::BindingKind::ProviderAppCredential,
            vault_service::oauth_broker::GrantFamily::ClientCredentials,
        );
        provider.display_name = input
            .display_name
            .unwrap_or_else(|| "Lark App v2".to_string());
        provider.version = 2;
        provider.adapter_key = Some("lark_openapi".to_string());
        provider.adapter_version = Some("v1".to_string());
        Ok(provider)
    }

    async fn validate_provider_definition(
        &self,
        _tenant_id: Uuid,
        provider_definition_id: Uuid,
    ) -> Result<ProviderValidationResult, OAuthBrokerError> {
        Ok(ProviderValidationResult {
            provider_definition_id,
            valid: true,
            checks: vec![
                ProviderValidationCheck::passed("endpoint", "token endpoint is reachable"),
                ProviderValidationCheck::passed("client_auth", "client auth is supported"),
                ProviderValidationCheck::passed("callback_adapter", "adapter binding is valid"),
                ProviderValidationCheck::passed("connectivity", "probe succeeded"),
            ],
            validated_at: chrono::Utc::now(),
        })
    }

    async fn create_binding(
        &self,
        _input: CreateBindingInput,
    ) -> Result<Binding, OAuthBrokerError> {
        Err(OAuthBrokerError::InvalidRequest(
            "unused in this test".to_string(),
        ))
    }

    async fn list_bindings(&self, _tenant_id: Uuid) -> Result<Vec<Binding>, OAuthBrokerError> {
        Ok(Vec::new())
    }

    async fn get_binding(&self, _binding_id: Uuid) -> Result<Option<Binding>, OAuthBrokerError> {
        Ok(None)
    }

    async fn get_binding_by_handle(
        &self,
        _tenant_id: Uuid,
        _binding_handle: &str,
    ) -> Result<Option<Binding>, OAuthBrokerError> {
        Ok(None)
    }

    async fn get_latest_binding_policy_snapshot(
        &self,
        _binding_id: Uuid,
    ) -> Result<Option<BindingPolicySnapshot>, OAuthBrokerError> {
        Ok(None)
    }

    async fn set_binding_validation_state(
        &self,
        _binding_id: Uuid,
        _binding_status: BindingStatus,
        _health_status: &str,
        _last_error_code: Option<&str>,
        _last_error_message: Option<&str>,
        _validated_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<Binding, OAuthBrokerError> {
        Err(OAuthBrokerError::InternalError("unused".to_string()))
    }

    async fn start_auth_transaction(
        &self,
        _input: StartAuthTransactionInput,
    ) -> Result<AuthTransaction, OAuthBrokerError> {
        Err(OAuthBrokerError::InternalError("unused".to_string()))
    }

    async fn get_auth_transaction(
        &self,
        _transaction_id: Uuid,
    ) -> Result<Option<AuthTransaction>, OAuthBrokerError> {
        Ok(None)
    }

    async fn get_auth_transaction_by_state(
        &self,
        _state: &str,
    ) -> Result<Option<AuthTransaction>, OAuthBrokerError> {
        Ok(None)
    }

    async fn set_auth_transaction_status(
        &self,
        _transaction_id: Uuid,
        _status: AuthTransactionStatus,
        _consumed_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<AuthTransaction, OAuthBrokerError> {
        Err(OAuthBrokerError::InternalError("unused".to_string()))
    }
}

fn create_test_app() -> axum::Router {
    let state = OAuthBrokerApiState::new(std::sync::Arc::new(MemoryProviderRegistryService));
    oauth_broker_routes(state)
}

fn create_public_callback_test_app() -> axum::Router {
    let state = OAuthBrokerApiState::new(std::sync::Arc::new(MemoryProviderRegistryService));
    oauth_broker_public_routes(state)
}

async fn read_json(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&bytes).expect("response body should be valid json")
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

async fn setup_postgres_backed_broker_app() -> Option<(axum::Router, sqlx::PgPool, sqlx::PgPool)> {
    let base_database_url = test_database_url()?;
    let admin_database_url = postgres_admin_url(&base_database_url)?;

    let admin_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&admin_database_url)
        .await
        .ok()?;

    let database_name = format!(
        "credbridge_provider_registry_{}",
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
    .bind("provider-registry-test")
    .bind("provider registry test tenant")
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
    .bind("Provider Registry Test User")
    .bind(tenant_id)
    .execute(database_pool.pool())
    .await
    .ok()?;

    let app = oauth_broker_routes(OAuthBrokerApiState::new(std::sync::Arc::new(
        PgOAuthBrokerService::new(database_pool.pool().clone()),
    )))
    .layer(axum::Extension(create_admin_token()));

    Some((app, database_pool.pool().clone(), admin_pool))
}

async fn create_fixed_schema_auth_tables(
    pool: &sqlx::PgPool,
    schema_name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(&format!(r#"CREATE SCHEMA "{schema_name}""#))
        .execute(pool)
        .await?;

    sqlx::query(&format!(
        r#"
        CREATE TABLE "{schema_name}".tenants (
            id UUID PRIMARY KEY,
            name VARCHAR(255),
            description TEXT,
            status VARCHAR(32) NOT NULL DEFAULT 'active',
            config JSONB NOT NULL DEFAULT '{{}}'::jsonb,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#
    ))
    .execute(pool)
    .await?;

    sqlx::query(&format!(
        r#"
        CREATE TABLE "{schema_name}".users (
            id UUID PRIMARY KEY,
            status VARCHAR(32) NOT NULL DEFAULT 'active',
            display_name VARCHAR(128),
            default_tenant_id UUID,
            onboarding_completed BOOLEAN NOT NULL DEFAULT FALSE,
            deleted_at TIMESTAMPTZ,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#
    ))
    .execute(pool)
    .await?;

    Ok(())
}

#[tokio::test]
async fn provider_registry_can_update_version_and_validate_config_contract() {
    let app = create_test_app().layer(axum::Extension(create_admin_token()));
    let provider_id = Uuid::now_v7();

    let update_request = Request::builder()
        .method("PATCH")
        .uri(format!("/oauth-broker/providers/{provider_id}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "display_name": "Lark App v2",
                "adapter_key": "lark_openapi",
                "adapter_version": "v1"
            })
            .to_string(),
        ))
        .unwrap();
    let update_response = app.clone().oneshot(update_request).await.unwrap();
    assert_eq!(update_response.status(), StatusCode::OK);
    let updated = read_json(update_response).await;
    assert_eq!(updated["success"], true);
    assert_eq!(updated["data"]["display_name"], "Lark App v2");
    assert_eq!(updated["data"]["version"], 2);
    assert_eq!(updated["data"]["adapter_key"], "lark_openapi");
    assert_eq!(updated["data"]["adapter_version"], "v1");

    let validate_request = Request::builder()
        .method("POST")
        .uri(format!("/oauth-broker/providers/{provider_id}/validate"))
        .body(Body::empty())
        .unwrap();
    let validate_response = app.oneshot(validate_request).await.unwrap();
    assert_eq!(validate_response.status(), StatusCode::OK);
    let validation = read_json(validate_response).await;
    assert_eq!(validation["success"], true);
    assert_eq!(validation["data"]["valid"], true);
    assert_eq!(validation["data"]["checks"][0]["name"], "endpoint");
    assert_eq!(validation["data"]["checks"][0]["status"], "passed");
    assert_eq!(validation["data"]["checks"][1]["name"], "client_auth");
    assert_eq!(validation["data"]["checks"][2]["name"], "callback_adapter");
    assert_eq!(validation["data"]["checks"][3]["name"], "connectivity");
}

#[tokio::test]
async fn oauth_callback_alias_route_hits_public_callback_handler() {
    let app = create_public_callback_test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/oauth/callback/lark?state=test-alias-state")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = read_json(response).await;
    assert_eq!(body["success"], false);
    assert_eq!(body["error"], "invalid_request");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("state")
    );
}

#[tokio::test]
async fn provider_definition_create_works_when_runtime_search_path_points_at_fixed_schema() {
    let Some(base_database_url) = test_database_url() else {
        eprintln!("skip broker fixed-schema test: TEST_DATABASE_URL/DATABASE_URL not configured");
        return;
    };
    let Some(admin_database_url) = postgres_admin_url(&base_database_url) else {
        eprintln!("skip broker fixed-schema test: unable to derive admin database url");
        return;
    };

    let admin_pool = match PgPoolOptions::new()
        .max_connections(2)
        .connect(&admin_database_url)
        .await
    {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("skip broker fixed-schema test: cannot connect admin database: {error}");
            return;
        }
    };

    let database_name = format!(
        "credbridge_provider_fixed_{}",
        &Uuid::new_v4().simple().to_string()[..12]
    );
    if let Err(error) = create_temp_database(&admin_pool, &database_name).await {
        eprintln!("skip broker fixed-schema test: cannot create temp database: {error}");
        return;
    }

    let Some(temp_url) = temp_database_url(&base_database_url, &database_name) else {
        eprintln!("skip broker fixed-schema test: cannot derive temp database url");
        drop_temp_database(&admin_pool, &database_name).await;
        return;
    };

    let fixed_schema = "credbridge_broker_fixed";
    let alter_search_path_sql =
        format!(r#"ALTER DATABASE "{database_name}" SET search_path TO "{fixed_schema}", public"#);
    sqlx::query(&alter_search_path_sql)
        .execute(&admin_pool)
        .await
        .expect("should set database default search_path");

    let config = DatabaseConfig {
        url: temp_url.clone(),
        max_connections: 5,
        min_connections: 1,
        connect_timeout: 10,
        idle_timeout: 60,
    };
    let database_pool = match DatabasePool::new(config).await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("skip broker fixed-schema test: cannot connect temp database: {error}");
            drop_temp_database(&admin_pool, &database_name).await;
            return;
        }
    };

    create_fixed_schema_auth_tables(database_pool.pool(), fixed_schema)
        .await
        .expect("should create fixed-schema auth tables");

    let original_fixed_schema = std::env::var("CREDBRIDGE_PG_SCHEMA").ok();
    unsafe {
        std::env::set_var("CREDBRIDGE_PG_SCHEMA", fixed_schema);
    }

    ensure_required_tables_on_startup(&database_pool)
        .await
        .expect("bootstrap should repair required tables");

    let tenant_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    sqlx::query(&format!(
        r#"
        INSERT INTO "{fixed_schema}".tenants (id, name, description, status, config)
        VALUES ($1, $2, $3, 'active', '{{}}'::jsonb)
        ON CONFLICT (id) DO NOTHING
        "#
    ))
    .bind(tenant_id)
    .bind("provider-fixed-schema-test")
    .bind("provider fixed schema test tenant")
    .execute(database_pool.pool())
    .await
    .expect("should seed fixed-schema tenant");

    sqlx::query(&format!(
        r#"
        INSERT INTO "{fixed_schema}".users (id, status, display_name, default_tenant_id, onboarding_completed)
        VALUES ($1, 'active', $2, $3, true)
        ON CONFLICT (id) DO NOTHING
        "#
    ))
    .bind(user_id)
    .bind("Provider Fixed Schema Test User")
    .bind(tenant_id)
    .execute(database_pool.pool())
    .await
    .expect("should seed fixed-schema user");

    let token = ValidatedToken {
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
    };
    let app = oauth_broker_routes(OAuthBrokerApiState::new(std::sync::Arc::new(
        PgOAuthBrokerService::new(database_pool.pool().clone()),
    )))
    .layer(axum::Extension(token));

    let create_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "fixed-schema-provider",
                "display_name": "Fixed Schema Provider",
                "provider_family": "lark_app",
                "binding_kind": "provider_app_credential",
                "grant_family": "client_credentials",
                "token_endpoint": "https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal",
                "client_id": "cli_fixed_schema",
                "client_auth_method": "app_secret",
                "callback_mode": null,
                "adapter_key": "lark_openapi",
                "adapter_version": "v1",
                "scope_template": [],
                "runtime_config": {
                    "token_mode": "tenant_access_token",
                    "base_url": "https://open.feishu.cn",
                    "allowed_domains": ["open.feishu.cn"],
                    "allowed_methods": ["GET"],
                    "allowed_path_prefixes": ["/open-apis/contact/v3/scopes"]
                }
            })
            .to_string(),
        ))
        .unwrap();
    let create_response = app.oneshot(create_request).await.unwrap();

    assert_eq!(create_response.status(), StatusCode::OK);
    let created = read_json(create_response).await;
    assert_eq!(created["success"], true);
    assert_eq!(created["data"]["alias"], "fixed-schema-provider");

    let fixed_table_exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM information_schema.tables
            WHERE table_schema = $1
              AND table_name = 'oauth_provider_definitions'
        )
        "#,
    )
    .bind(fixed_schema)
    .fetch_one(database_pool.pool())
    .await
    .expect("should inspect fixed-schema broker table presence");
    assert!(fixed_table_exists);

    let fixed_count: i64 = sqlx::query_scalar(&format!(
        r#"SELECT COUNT(*) FROM "{fixed_schema}".oauth_provider_definitions WHERE alias = $1"#
    ))
    .bind("fixed-schema-provider")
    .fetch_one(database_pool.pool())
    .await
    .expect("fixed-schema broker table should be queryable");
    assert_eq!(fixed_count, 1);

    database_pool.close().await;
    match original_fixed_schema {
        Some(value) => unsafe { std::env::set_var("CREDBRIDGE_PG_SCHEMA", value) },
        None => unsafe { std::env::remove_var("CREDBRIDGE_PG_SCHEMA") },
    }
    drop_temp_database(&admin_pool, &database_name).await;
    admin_pool.close().await;
}

#[tokio::test]
async fn provider_registry_update_persists_version_in_postgres_when_database_is_available() {
    let Some((app, pool, admin_pool)) = setup_postgres_backed_broker_app().await else {
        eprintln!("skip provider registry postgres test: database not available");
        return;
    };

    let create_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "lark-app-versioned",
                "display_name": "Lark App Versioned",
                "provider_family": "lark_app",
                "binding_kind": "provider_app_credential",
                "grant_family": "client_credentials",
                "token_endpoint": "https://mock.example/token",
                "callback_mode": "none",
                "scope_template": ["contact:read"],
                "runtime_config": {"connectivity_probe_url": "http://127.0.0.1:65530/probe"}
            })
            .to_string(),
        ))
        .unwrap();
    let create_response = app.clone().oneshot(create_request).await.unwrap();
    let created = read_json(create_response).await;
    let provider_id = created["data"]["id"].as_str().unwrap().to_string();

    let update_request = Request::builder()
        .method("PATCH")
        .uri(format!("/oauth-broker/providers/{provider_id}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "display_name": "Lark App Versioned v2",
                "client_id": "cli_123",
                "client_auth_method": "client_secret_basic",
                "adapter_key": "lark_openapi",
                "adapter_version": "v1"
            })
            .to_string(),
        ))
        .unwrap();
    let update_response = app.oneshot(update_request).await.unwrap();
    assert_eq!(update_response.status(), StatusCode::OK);
    let updated = read_json(update_response).await;
    assert_eq!(updated["data"]["version"], 2);

    let stored_version: i32 = sqlx::query_scalar(
        r#"SELECT version FROM public.oauth_provider_definitions WHERE id = $1"#,
    )
    .bind(Uuid::parse_str(&provider_id).unwrap())
    .fetch_one(&pool)
    .await
    .expect("should read stored provider version");
    assert_eq!(stored_version, 2);

    let db_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .expect("should read temp database name");
    pool.close().await;
    drop_temp_database(&admin_pool, &db_name).await;
    admin_pool.close().await;
}

#[tokio::test]
async fn provider_validation_returns_structured_success_and_failure_against_mock_probe_targets() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock listener should bind");
    let addr = listener.local_addr().unwrap();
    let mock_app = axum::Router::new().route("/probe", get(|| async { "ok" }));
    let server = tokio::spawn(async move {
        axum::serve(listener, mock_app).await.unwrap();
    });

    let Some((app, pool, admin_pool)) = setup_postgres_backed_broker_app().await else {
        server.abort();
        eprintln!("skip provider validation test: database not available");
        return;
    };

    let create_ok_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "lark-app-probe-ok",
                "display_name": "Lark App Probe OK",
                "provider_family": "lark_app",
                "binding_kind": "provider_app_credential",
                "grant_family": "client_credentials",
                "token_endpoint": format!("http://127.0.0.1:{}/probe", addr.port()),
                "callback_mode": "none",
                "scope_template": ["contact:read"],
                "runtime_config": {"connectivity_probe_url": format!("http://127.0.0.1:{}/probe", addr.port())}
            })
            .to_string(),
        ))
        .unwrap();
    let ok_created = read_json(app.clone().oneshot(create_ok_request).await.unwrap()).await;
    let ok_provider_id = ok_created["data"]["id"].as_str().unwrap().to_string();

    let ok_update_request = Request::builder()
        .method("PATCH")
        .uri(format!("/oauth-broker/providers/{ok_provider_id}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "client_id": "cli_ok",
                "client_auth_method": "client_secret_basic",
                "adapter_key": "lark_openapi",
                "adapter_version": "v1"
            })
            .to_string(),
        ))
        .unwrap();
    let ok_update_response = app.clone().oneshot(ok_update_request).await.unwrap();
    assert_eq!(ok_update_response.status(), StatusCode::OK);

    let ok_validate_request = Request::builder()
        .method("POST")
        .uri(format!("/oauth-broker/providers/{ok_provider_id}/validate"))
        .body(Body::empty())
        .unwrap();
    let ok_validation = read_json(app.clone().oneshot(ok_validate_request).await.unwrap()).await;
    assert_eq!(ok_validation["data"]["valid"], true);
    assert_eq!(ok_validation["data"]["checks"][0]["status"], "passed");
    assert_eq!(ok_validation["data"]["checks"][1]["status"], "passed");
    assert_eq!(ok_validation["data"]["checks"][2]["status"], "passed");
    assert_eq!(ok_validation["data"]["checks"][3]["status"], "passed");

    let create_bad_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "lark-app-probe-bad",
                "display_name": "Lark App Probe Bad",
                "provider_family": "lark_app",
                "binding_kind": "provider_app_credential",
                "grant_family": "client_credentials",
                "token_endpoint": "http://127.0.0.1:65530/probe",
                "callback_mode": "none",
                "scope_template": ["contact:read"],
                "runtime_config": {"connectivity_probe_url": "http://127.0.0.1:65530/probe"}
            })
            .to_string(),
        ))
        .unwrap();
    let bad_created = read_json(app.clone().oneshot(create_bad_request).await.unwrap()).await;
    let bad_provider_id = bad_created["data"]["id"].as_str().unwrap().to_string();

    let bad_update_request = Request::builder()
        .method("PATCH")
        .uri(format!("/oauth-broker/providers/{bad_provider_id}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "client_id": "cli_bad",
                "client_auth_method": "unsupported_method",
                "adapter_key": "unknown_adapter",
                "adapter_version": "v9"
            })
            .to_string(),
        ))
        .unwrap();
    let bad_update_response = app.clone().oneshot(bad_update_request).await.unwrap();
    assert_eq!(bad_update_response.status(), StatusCode::OK);

    let bad_validate_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/oauth-broker/providers/{bad_provider_id}/validate"
        ))
        .body(Body::empty())
        .unwrap();
    let bad_validation = read_json(app.oneshot(bad_validate_request).await.unwrap()).await;
    assert_eq!(bad_validation["data"]["valid"], false);
    assert_eq!(bad_validation["data"]["checks"][1]["status"], "failed");
    assert_eq!(bad_validation["data"]["checks"][2]["status"], "failed");
    assert_eq!(bad_validation["data"]["checks"][3]["status"], "failed");

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
async fn provider_registry_rejects_cross_tenant_update_and_validate() {
    let Some((app, pool, admin_pool)) = setup_postgres_backed_broker_app().await else {
        eprintln!("skip provider registry tenant isolation test: database not available");
        return;
    };

    let owner_app = app.clone().layer(axum::Extension(create_admin_token()));
    let create_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "cross-tenant-provider",
                "display_name": "Cross Tenant Provider",
                "provider_family": "lark_app",
                "binding_kind": "provider_app_credential",
                "grant_family": "client_credentials",
                "token_endpoint": "https://mock.example/token",
                "callback_mode": "none",
                "scope_template": ["contact:read"],
                "runtime_config": {"connectivity_probe_url": "http://127.0.0.1:65530/probe"}
            })
            .to_string(),
        ))
        .unwrap();
    let created = read_json(owner_app.clone().oneshot(create_request).await.unwrap()).await;
    let provider_id = created["data"]["id"].as_str().unwrap().to_string();

    let outsider_app = app.layer(axum::Extension(create_admin_token_for(
        Uuid::new_v4(),
        "00000000-0000-0000-0000-000000000099".to_string(),
    )));

    let update_request = Request::builder()
        .method("PATCH")
        .uri(format!("/oauth-broker/providers/{provider_id}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"display_name": "stolen"}).to_string()))
        .unwrap();
    let update_response = outsider_app.clone().oneshot(update_request).await.unwrap();
    assert_eq!(update_response.status(), StatusCode::FORBIDDEN);

    let validate_request = Request::builder()
        .method("POST")
        .uri(format!("/oauth-broker/providers/{provider_id}/validate"))
        .body(Body::empty())
        .unwrap();
    let validate_response = outsider_app.oneshot(validate_request).await.unwrap();
    assert_eq!(validate_response.status(), StatusCode::FORBIDDEN);

    let db_name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .expect("should read temp database name");
    pool.close().await;
    drop_temp_database(&admin_pool, &db_name).await;
    admin_pool.close().await;
}
