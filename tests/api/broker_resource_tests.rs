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
    middleware::{TokenScope, ValidatedToken},
    oauth_broker::{OAuthBrokerApiState, oauth_broker_routes},
};
use vault_service::oauth_broker::{
    AuthTransaction, AuthTransactionStatus, Binding, BindingKind, BindingPolicySnapshot,
    BindingStatus, CreateBindingInput, CreateProviderDefinitionInput, GrantFamily,
    OAuthBrokerError, OAuthBrokerService, PgOAuthBrokerService, ProviderDefinition,
    ProviderDefinitionStatus, ProviderValidationCheck, ProviderValidationResult,
    StartAuthTransactionInput, UpdateProviderDefinitionInput,
};
use vault_service::services::db::{
    DatabaseConfig, DatabasePool, ensure_required_tables_on_startup,
};

fn create_admin_token() -> ValidatedToken {
    ValidatedToken {
        token_id: Uuid::now_v7().to_string(),
        subject: "tenant1:user1".to_string(),
        tenant_id: Uuid::nil().to_string(),
        user_id: Uuid::now_v7().to_string(),
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
struct MemoryOAuthBrokerService;

#[async_trait::async_trait]
impl OAuthBrokerService for MemoryOAuthBrokerService {
    async fn create_provider_definition(
        &self,
        input: CreateProviderDefinitionInput,
    ) -> Result<ProviderDefinition, OAuthBrokerError> {
        Ok(ProviderDefinition::new(
            Uuid::parse_str(&Uuid::nil().to_string()).unwrap(),
            Uuid::nil(),
            input.alias,
            input.display_name,
            input.provider_family,
            input.binding_kind,
            input.grant_family,
        ))
    }

    async fn list_provider_definitions(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<ProviderDefinition>, OAuthBrokerError> {
        Ok(vec![ProviderDefinition::new(
            Uuid::nil(),
            Uuid::nil(),
            "lark-app".to_string(),
            "Lark App".to_string(),
            "lark_app".to_string(),
            BindingKind::ProviderAppCredential,
            GrantFamily::ClientCredentials,
        )])
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
            BindingKind::ProviderAppCredential,
            GrantFamily::ClientCredentials,
        )))
    }

    async fn update_provider_definition(
        &self,
        _tenant_id: Uuid,
        provider_definition_id: Uuid,
        _input: UpdateProviderDefinitionInput,
    ) -> Result<ProviderDefinition, OAuthBrokerError> {
        let mut provider = ProviderDefinition::new(
            provider_definition_id,
            Uuid::nil(),
            "lark-app".to_string(),
            "Lark App".to_string(),
            "lark_app".to_string(),
            BindingKind::ProviderAppCredential,
            GrantFamily::ClientCredentials,
        );
        provider.version = 2;
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
            checks: vec![ProviderValidationCheck::passed(
                "endpoint",
                "memory validation passes in broker resource tests",
            )],
            validated_at: chrono::Utc::now(),
        })
    }

    async fn create_binding(
        &self,
        _input: CreateBindingInput,
    ) -> Result<Binding, OAuthBrokerError> {
        Ok(Binding {
            id: Uuid::now_v7(),
            tenant_id: Uuid::nil(),
            provider_definition_id: Uuid::nil(),
            binding_handle: "binding_lark_app_primary".to_string(),
            alias: "lark-primary".to_string(),
            purpose: "Lark internal app access".to_string(),
            binding_kind: BindingKind::ProviderAppCredential,
            subject_ref: "tenant_access_token".to_string(),
            subject_display_name: Some("Lark Tenant Access".to_string()),
            backing_credential_id: None,
            status: vault_service::oauth_broker::BindingStatus::Draft,
            health_status: Some("draft".to_string()),
            last_error_code: None,
            last_error_message: None,
            cooldown_until: None,
            rebind_required: false,
            last_validated_at: None,
            created_by: Uuid::nil(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
    }

    async fn list_bindings(&self, _tenant_id: Uuid) -> Result<Vec<Binding>, OAuthBrokerError> {
        Ok(vec![Binding {
            id: Uuid::nil(),
            tenant_id: Uuid::nil(),
            provider_definition_id: Uuid::nil(),
            binding_handle: "binding_lark_app_primary".to_string(),
            alias: "lark-primary".to_string(),
            purpose: "Lark internal app access".to_string(),
            binding_kind: BindingKind::ProviderAppCredential,
            subject_ref: "tenant_access_token".to_string(),
            subject_display_name: Some("Lark Tenant Access".to_string()),
            backing_credential_id: None,
            status: vault_service::oauth_broker::BindingStatus::Draft,
            health_status: Some("draft".to_string()),
            last_error_code: None,
            last_error_message: None,
            cooldown_until: None,
            rebind_required: false,
            last_validated_at: None,
            created_by: Uuid::nil(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }])
    }

    async fn get_binding(&self, binding_id: Uuid) -> Result<Option<Binding>, OAuthBrokerError> {
        Ok(Some(Binding {
            id: binding_id,
            tenant_id: Uuid::nil(),
            provider_definition_id: Uuid::nil(),
            binding_handle: "binding_lark_app_primary".to_string(),
            alias: "lark-primary".to_string(),
            purpose: "Lark internal app access".to_string(),
            binding_kind: BindingKind::ProviderAppCredential,
            subject_ref: "tenant_access_token".to_string(),
            subject_display_name: Some("Lark Tenant Access".to_string()),
            backing_credential_id: None,
            status: vault_service::oauth_broker::BindingStatus::Draft,
            health_status: Some("draft".to_string()),
            last_error_code: None,
            last_error_message: None,
            cooldown_until: None,
            rebind_required: false,
            last_validated_at: None,
            created_by: Uuid::nil(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }))
    }

    async fn get_binding_by_handle(
        &self,
        _tenant_id: Uuid,
        binding_handle: &str,
    ) -> Result<Option<Binding>, OAuthBrokerError> {
        Ok(Some(Binding {
            id: Uuid::nil(),
            tenant_id: Uuid::nil(),
            provider_definition_id: Uuid::nil(),
            binding_handle: binding_handle.to_string(),
            alias: "lark-primary".to_string(),
            purpose: "Lark internal app access".to_string(),
            binding_kind: BindingKind::ProviderAppCredential,
            subject_ref: "tenant_access_token".to_string(),
            subject_display_name: Some("Lark Tenant Access".to_string()),
            backing_credential_id: None,
            status: vault_service::oauth_broker::BindingStatus::Draft,
            health_status: Some("draft".to_string()),
            last_error_code: None,
            last_error_message: None,
            cooldown_until: None,
            rebind_required: false,
            last_validated_at: None,
            created_by: Uuid::nil(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }))
    }

    async fn get_latest_binding_policy_snapshot(
        &self,
        _binding_id: Uuid,
    ) -> Result<Option<BindingPolicySnapshot>, OAuthBrokerError> {
        Ok(None)
    }

    async fn set_binding_validation_state(
        &self,
        binding_id: Uuid,
        binding_status: BindingStatus,
        health_status: &str,
        _last_error_code: Option<&str>,
        _last_error_message: Option<&str>,
        validated_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<Binding, OAuthBrokerError> {
        Ok(Binding {
            id: binding_id,
            tenant_id: Uuid::nil(),
            provider_definition_id: Uuid::nil(),
            binding_handle: "binding_lark_app_primary".to_string(),
            alias: "lark-primary".to_string(),
            purpose: "Lark internal app access".to_string(),
            binding_kind: BindingKind::ProviderAppCredential,
            subject_ref: "tenant_access_token".to_string(),
            subject_display_name: Some("Lark Tenant Access".to_string()),
            backing_credential_id: None,
            status: binding_status,
            health_status: Some(health_status.to_string()),
            last_error_code: None,
            last_error_message: None,
            cooldown_until: None,
            rebind_required: false,
            last_validated_at: Some(validated_at),
            created_by: Uuid::nil(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
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
    let state = OAuthBrokerApiState::new(std::sync::Arc::new(MemoryOAuthBrokerService));
    oauth_broker_routes(state)
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

#[tokio::test]
async fn provider_definition_can_be_created_listed_and_inspected() {
    let app = create_test_app().layer(axum::Extension(create_admin_token()));

    let create_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "lark-app",
                "display_name": "Lark App",
                "provider_family": "lark_app",
                "binding_kind": "provider_app_credential",
                "grant_family": "client_credentials",
                "token_endpoint": "https://open.larkoffice.com/open-apis/auth/v3/tenant_access_token/internal",
                "client_id": "cli_123",
                "callback_mode": "none",
                "scope_template": ["contact:read"],
                "runtime_config": {
                    "base_url": "https://open.larkoffice.com"
                }
            })
            .to_string(),
        ))
        .unwrap();

    let create_response = app.clone().oneshot(create_request).await.unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);
    let created = read_json(create_response).await;
    assert_eq!(created["success"], true);
    assert_eq!(created["data"]["alias"], "lark-app");
    assert_eq!(created["data"]["provider_family"], "lark_app");
    assert_eq!(created["data"]["binding_kind"], "provider_app_credential");
    assert_eq!(created["data"]["grant_family"], "client_credentials");
    assert_eq!(
        created["data"]["status"],
        ProviderDefinitionStatus::Active.as_str()
    );

    let list_request = Request::builder()
        .method("GET")
        .uri("/oauth-broker/providers")
        .body(Body::empty())
        .unwrap();
    let list_response = app.clone().oneshot(list_request).await.unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);
    let listed = read_json(list_response).await;
    assert_eq!(listed["success"], true);
    assert_eq!(listed["data"][0]["alias"], "lark-app");

    let provider_id = created["data"]["id"].as_str().unwrap();
    let get_request = Request::builder()
        .method("GET")
        .uri(format!("/oauth-broker/providers/{provider_id}"))
        .body(Body::empty())
        .unwrap();
    let get_response = app.oneshot(get_request).await.unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);
    let detail = read_json(get_response).await;
    assert_eq!(detail["success"], true);
    assert_eq!(detail["data"]["id"], provider_id);
    assert_eq!(detail["data"]["alias"], "lark-app");
}

#[tokio::test]
async fn binding_can_be_created_listed_and_inspected() {
    let app = create_test_app().layer(axum::Extension(create_admin_token()));

    let create_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/bindings")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "provider_definition_id": Uuid::nil(),
                "alias": "lark-primary",
                "purpose": "Lark internal app access",
                "subject_ref": "tenant_access_token",
                "subject_display_name": "Lark Tenant Access"
            })
            .to_string(),
        ))
        .unwrap();
    let create_response = app.clone().oneshot(create_request).await.unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);
    let created = read_json(create_response).await;
    assert_eq!(created["success"], true);
    assert_eq!(created["data"]["alias"], "lark-primary");
    assert_eq!(
        created["data"]["binding_handle"],
        "binding_lark_app_primary"
    );
    assert_eq!(created["data"]["status"], "draft");

    let list_request = Request::builder()
        .method("GET")
        .uri("/oauth-broker/bindings")
        .body(Body::empty())
        .unwrap();
    let list_response = app.clone().oneshot(list_request).await.unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);
    let listed = read_json(list_response).await;
    assert_eq!(listed["success"], true);
    assert_eq!(listed["data"][0]["alias"], "lark-primary");

    let binding_id = created["data"]["id"].as_str().unwrap();
    let get_request = Request::builder()
        .method("GET")
        .uri(format!("/oauth-broker/bindings/{binding_id}"))
        .body(Body::empty())
        .unwrap();
    let get_response = app.oneshot(get_request).await.unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);
    let detail = read_json(get_response).await;
    assert_eq!(detail["success"], true);
    assert_eq!(detail["data"]["id"], binding_id);
    assert_eq!(detail["data"]["alias"], "lark-primary");
}

#[tokio::test]
async fn provider_definition_round_trips_through_postgres_when_database_is_available() {
    let Some(base_database_url) = test_database_url() else {
        eprintln!("skip broker postgres test: TEST_DATABASE_URL/DATABASE_URL not configured");
        return;
    };
    let Some(admin_database_url) = postgres_admin_url(&base_database_url) else {
        eprintln!("skip broker postgres test: unable to derive admin database url");
        return;
    };

    let admin_pool = match PgPoolOptions::new()
        .max_connections(2)
        .connect(&admin_database_url)
        .await
    {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("skip broker postgres test: cannot connect admin database: {error}");
            return;
        }
    };

    let database_name = format!(
        "credbridge_broker_{}",
        &Uuid::new_v4().simple().to_string()[..12]
    );
    if let Err(error) = create_temp_database(&admin_pool, &database_name).await {
        eprintln!("skip broker postgres test: cannot create temp database: {error}");
        return;
    }

    let Some(temp_url) = temp_database_url(&base_database_url, &database_name) else {
        eprintln!("skip broker postgres test: cannot derive temp database url");
        drop_temp_database(&admin_pool, &database_name).await;
        return;
    };

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
            eprintln!("skip broker postgres test: cannot connect temp database: {error}");
            drop_temp_database(&admin_pool, &database_name).await;
            return;
        }
    };

    ensure_required_tables_on_startup(&database_pool)
        .await
        .expect("bootstrap should create broker tables");

    let tenant_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO public.tenants (id, name, description, status, config)
        VALUES ($1, $2, $3, 'active', '{}'::jsonb)
        "#,
    )
    .bind(tenant_id)
    .bind(format!("tenant-{}", &tenant_id.simple().to_string()[..8]))
    .bind("broker test tenant")
    .execute(database_pool.pool())
    .await
    .expect("should seed tenant");

    sqlx::query(
        r#"
        INSERT INTO public.users (id, status, display_name, default_tenant_id, onboarding_completed)
        VALUES ($1, 'active', $2, $3, true)
        "#,
    )
    .bind(user_id)
    .bind("Broker Test User")
    .bind(tenant_id)
    .execute(database_pool.pool())
    .await
    .expect("should seed user");

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

    let service = PgOAuthBrokerService::new(database_pool.pool().clone());
    let app = oauth_broker_routes(OAuthBrokerApiState::new(std::sync::Arc::new(service)))
        .layer(axum::Extension(token));

    let create_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "lark-app",
                "display_name": "Lark App",
                "provider_family": "lark_app",
                "binding_kind": "provider_app_credential",
                "grant_family": "client_credentials",
                "token_endpoint": "https://open.larkoffice.com/open-apis/auth/v3/tenant_access_token/internal",
                "client_id": "cli_123",
                "callback_mode": "none",
                "scope_template": ["contact:read"],
                "runtime_config": {
                    "base_url": "https://open.larkoffice.com"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let create_response = app.clone().oneshot(create_request).await.unwrap();
    assert_eq!(create_response.status(), StatusCode::OK);
    let created = read_json(create_response).await;
    let provider_id = created["data"]["id"].as_str().unwrap().to_string();

    let list_request = Request::builder()
        .method("GET")
        .uri("/oauth-broker/providers")
        .body(Body::empty())
        .unwrap();
    let list_response = app.clone().oneshot(list_request).await.unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);
    let listed = read_json(list_response).await;
    assert_eq!(listed["data"][0]["id"], provider_id);

    let persisted_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)::bigint
        FROM public.oauth_provider_definitions
        WHERE id = $1
        "#,
    )
    .bind(Uuid::parse_str(&provider_id).unwrap())
    .fetch_one(database_pool.pool())
    .await
    .expect("should count persisted provider definitions");
    assert_eq!(persisted_count, 1);

    database_pool.close().await;
    drop_temp_database(&admin_pool, &database_name).await;
    admin_pool.close().await;
}

#[tokio::test]
async fn binding_creation_persists_policy_snapshot_and_runtime_state_when_database_is_available() {
    let Some(base_database_url) = test_database_url() else {
        eprintln!("skip broker postgres test: TEST_DATABASE_URL/DATABASE_URL not configured");
        return;
    };
    let Some(admin_database_url) = postgres_admin_url(&base_database_url) else {
        eprintln!("skip broker postgres test: unable to derive admin database url");
        return;
    };

    let admin_pool = match PgPoolOptions::new()
        .max_connections(2)
        .connect(&admin_database_url)
        .await
    {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("skip broker postgres test: cannot connect admin database: {error}");
            return;
        }
    };

    let database_name = format!(
        "credbridge_binding_{}",
        &Uuid::new_v4().simple().to_string()[..12]
    );
    if let Err(error) = create_temp_database(&admin_pool, &database_name).await {
        eprintln!("skip broker postgres test: cannot create temp database: {error}");
        return;
    }

    let Some(temp_url) = temp_database_url(&base_database_url, &database_name) else {
        eprintln!("skip broker postgres test: cannot derive temp database url");
        drop_temp_database(&admin_pool, &database_name).await;
        return;
    };

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
            eprintln!("skip broker postgres test: cannot connect temp database: {error}");
            drop_temp_database(&admin_pool, &database_name).await;
            return;
        }
    };

    ensure_required_tables_on_startup(&database_pool)
        .await
        .expect("bootstrap should create broker tables");

    let tenant_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO public.tenants (id, name, description, status, config)
        VALUES ($1, $2, $3, 'active', '{}'::jsonb)
        "#,
    )
    .bind(tenant_id)
    .bind(format!("tenant-{}", &tenant_id.simple().to_string()[..8]))
    .bind("binding test tenant")
    .execute(database_pool.pool())
    .await
    .expect("should seed tenant");

    sqlx::query(
        r#"
        INSERT INTO public.users (id, status, display_name, default_tenant_id, onboarding_completed)
        VALUES ($1, 'active', $2, $3, true)
        "#,
    )
    .bind(user_id)
    .bind("Binding Test User")
    .bind(tenant_id)
    .execute(database_pool.pool())
    .await
    .expect("should seed user");

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

    let service = PgOAuthBrokerService::new(database_pool.pool().clone());
    let app = oauth_broker_routes(OAuthBrokerApiState::new(std::sync::Arc::new(service)))
        .layer(axum::Extension(token));

    let provider_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/providers")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "alias": "lark-app",
                "display_name": "Lark App",
                "provider_family": "lark_app",
                "binding_kind": "provider_app_credential",
                "grant_family": "client_credentials",
                "token_endpoint": "https://open.larkoffice.com/open-apis/auth/v3/tenant_access_token/internal",
                "client_id": "cli_123",
                "callback_mode": "none",
                "scope_template": ["contact:read"],
                "runtime_config": {
                    "base_url": "https://open.larkoffice.com"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let provider_response = app.clone().oneshot(provider_request).await.unwrap();
    assert_eq!(provider_response.status(), StatusCode::OK);
    let provider_json = read_json(provider_response).await;
    let provider_id = provider_json["data"]["id"].as_str().unwrap().to_string();

    let binding_request = Request::builder()
        .method("POST")
        .uri("/oauth-broker/bindings")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "provider_definition_id": provider_id,
                "alias": "lark-primary",
                "purpose": "Lark internal app access",
                "subject_ref": "tenant_access_token",
                "subject_display_name": "Lark Tenant Access"
            })
            .to_string(),
        ))
        .unwrap();
    let binding_response = app.clone().oneshot(binding_request).await.unwrap();
    assert_eq!(binding_response.status(), StatusCode::OK);
    let binding_json = read_json(binding_response).await;
    let binding_id = Uuid::parse_str(binding_json["data"]["id"].as_str().unwrap()).unwrap();

    let snapshot_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)::bigint
        FROM public.oauth_binding_policy_snapshots
        WHERE binding_id = $1
        "#,
    )
    .bind(binding_id)
    .fetch_one(database_pool.pool())
    .await
    .expect("should count binding policy snapshots");
    assert_eq!(snapshot_count, 1);

    let runtime_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)::bigint
        FROM public.oauth_binding_runtime_states
        WHERE binding_id = $1
        "#,
    )
    .bind(binding_id)
    .fetch_one(database_pool.pool())
    .await
    .expect("should count binding runtime states");
    assert_eq!(runtime_count, 1);

    database_pool.close().await;
    drop_temp_database(&admin_pool, &database_name).await;
    admin_pool.close().await;
}
