//! CredBridge - Credential Vault HTTP Service
//!
//! HTTP API server entry point.
//! Supports port and runtime mode configuration via environment variables.
//!
//! # Environment Variables
//!
//! - `CREDBRIDGE_PORT` - Server port (default: 8080)
//! - `CREDBRIDGE_HOST` - Server host (default: 0.0.0.0)
//! - `CREDBRIDGE_ENV` - Runtime environment (development/production, default: development)
//! - `TEE_MODE` - TEE runtime mode (`hardware`/`simulation`)
//! - `TEE_DEBUG` - Enable TEE debug mode
//! - `SEALED_STORAGE_PATH` - Enclave sealed storage directory (default: `.sealed`)
//! - `RUST_LOG` - Log level (default: info)
//! - `CREDBRIDGE_RATE_LIMIT_REQUESTS` - Rate limit requests per window (default: 100)
//! - `CREDBRIDGE_RATE_LIMIT_WINDOW_SECONDS` - Rate limit window in seconds (default: 60)

use axum::{
    Extension, Json, Router, ServiceExt, extract::Request, http::StatusCode,
    response::IntoResponse, routing::get,
};
use serde::Serialize;
use serde_json::json;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::Layer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::normalize_path::NormalizePathLayer;
use tower_http::trace::{self, TraceLayer};
use tracing::{Level, info, warn};
use vault_service::crypto::hkdf::KeyHierarchy;

// BUG-18229: 使用 tower::util::MapRequestLayer 在 NormalizePathLayer 之前保存原始 URI
// 使 handler 能够检测原始请求路径是否以尾斜杠结尾
use axum::extract::OriginalUri;

// CredBridge internal modules
use vault_service::api::{
    API_BASE_PATH,
    attestation::{
        AttestationApiConfig, AttestationState, attestation_routes, init_attestation_api,
    },
    audit::{AuditApiState, AuditStorage, PostgresAuditStorageAdapter, audit_routes},
    auth::{AuthApiState, auth_routes, protected_auth_routes},
    credentials::{
        AppState as CredentialAppState, StorageAuditLogger, routes as credential_routes,
    },
    i18n::{LocaleResolverState, locale_middleware},
    middleware::auth_middleware,
    notifications::notifications_routes,
    rate_limit::{RateLimitConfig, RateLimitState, rate_limit_middleware},
    sandbox::{SandboxState, sandbox_routes},
    service_account_routes,
    tenant::{TenantApiState, tenant_routes},
    token_blacklist::{TokenStore, create_redis_token_store},
    token_routes,
};
use vault_service::config::{ConfigError, TeeRuntimeConfig, TeeRuntimeMode};
use vault_service::services::db::DatabasePool;
use vault_service::tee::{
    Enclave, EnclaveConfig, SelfCheckItem, SelfCheckStatus, SharedEnclave, StartupReadiness,
    TEE_HARDWARE_BUILD_ENABLED, validate_runtime_requirements,
};
use vault_service::tenant::{
    PostgresTenantConfigStore, PostgresTenantStorage, TenantConfigStore, TenantManagerBuilder,
    TenantService,
};
use vault_service::vault::backend::VaultStorageBackend;
use vault_service::vault::postgres::PostgresStorageBackend;
use vault_service::vault::storage::CredentialVault;

/// API root response
#[derive(Debug, Serialize)]
struct ApiRootResponse {
    name: String,
    version: String,
    environment: String,
    endpoints: Vec<ApiEndpoint>,
}

#[derive(Debug, Serialize)]
struct ApiEndpoint {
    path: String,
    description: String,
}

/// Health check response
#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    ready: bool,
    version: String,
    timestamp: u64,
}

/// Detailed health check response
#[derive(Debug, Serialize)]
struct HealthDetailResponse {
    status: String,
    live: bool,
    ready: bool,
    version: String,
    timestamp: u64,
    components: ComponentHealth,
}

#[derive(Debug, Serialize)]
struct ComponentHealth {
    vault: String,
    enclave: String,
    audit_log: String,
    attestation: String,
}

/// 服务器配置
#[derive(Debug, Clone)]
struct ServerConfig {
    host: String,
    port: u16,
    environment: Environment,
    tee_runtime: TeeRuntimeConfig,
    log_level: String,
    storage_backend: StorageBackendKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Environment {
    Development,
    Production,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum StorageBackendKind {
    Auto,
    Postgres,
    Vault,
}

impl Environment {
    fn as_str(&self) -> &'static str {
        match self {
            Environment::Development => "development",
            Environment::Production => "production",
        }
    }

    fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "production" | "prod" => Environment::Production,
            _ => Environment::Development,
        }
    }
}

impl StorageBackendKind {
    fn from_env() -> Self {
        match env::var("CREDBRIDGE_STORAGE_BACKEND")
            .unwrap_or_else(|_| "auto".to_string())
            .to_lowercase()
            .as_str()
        {
            "postgres" | "postgresql" | "db" => StorageBackendKind::Postgres,
            "vault" => StorageBackendKind::Vault,
            _ => StorageBackendKind::Auto,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            StorageBackendKind::Auto => "auto",
            StorageBackendKind::Postgres => "postgres",
            StorageBackendKind::Vault => "vault",
        }
    }
}

fn env_var_present(name: &str) -> bool {
    env::var_os(name).is_some_and(|value| !value.is_empty())
}

fn resolve_auto_storage_backend(
    has_database_url: bool,
    has_vault_addr: bool,
    has_vault_token: bool,
) -> Result<StorageBackendKind, String> {
    if has_database_url {
        Ok(StorageBackendKind::Postgres)
    } else if has_vault_addr && has_vault_token {
        Ok(StorageBackendKind::Vault)
    } else {
        Err(
            "自动存储后端选择失败：未检测到 DATABASE_URL，且 VAULT_ADDR/VAULT_TOKEN 未同时配置。请显式配置 PostgreSQL/Vault 持久化后端。"
                .to_string(),
        )
    }
}

fn resolve_storage_backend(config: &ServerConfig) -> Result<StorageBackendKind, String> {
    match config.storage_backend {
        StorageBackendKind::Auto => resolve_auto_storage_backend(
            env_var_present("DATABASE_URL"),
            env_var_present("VAULT_ADDR"),
            env_var_present("VAULT_TOKEN"),
        ),
        backend => Ok(backend),
    }
}

impl ServerConfig {
    /// 从环境变量加载配置
    fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            host: env::var("CREDBRIDGE_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: env::var("CREDBRIDGE_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(8080),
            environment: Environment::from_str(
                &env::var("CREDBRIDGE_ENV").unwrap_or_else(|_| "development".to_string()),
            ),
            tee_runtime: TeeRuntimeConfig::from_env()?,
            log_level: env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
            storage_backend: StorageBackendKind::from_env(),
        })
    }

    fn socket_addr(&self) -> SocketAddr {
        format!("{}:{}", self.host, self.port)
            .parse()
            .expect("无效的服务器地址")
    }
}

/// 应用状态（包含所有 API 模块共享的状态）
#[derive(Clone)]
struct AppState {
    config: ServerConfig,
    credential_state: CredentialAppState,
    audit_state: AuditApiState,
    auth_state: AuthApiState,
    tenant_store: Arc<dyn TenantConfigStore>,
    tenant_service: Arc<dyn TenantService>,
    rate_limit_state: RateLimitState,
    attestation_state: Option<Arc<AttestationState>>,
    sandbox_state: SandboxState,
    startup_checks: Vec<SelfCheckItem>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 加载配置
    let config = ServerConfig::from_env()?;

    // 初始化日志
    init_logging(&config);

    // 打印启动信息
    info!("╔══════════════════════════════════════════════════════════╗");
    info!("║           CredBridge - TEE Credential Vault              ║");
    info!("║              Secure AI-Native Secret Storage             ║");
    info!("╚══════════════════════════════════════════════════════════╝");
    info!("");
    info!("🚀 正在启动 HTTP 服务器...");
    info!("📍 环境: {}", config.environment.as_str());
    info!("🔐 TEE 模式: {}", config.tee_runtime.mode);
    info!(
        "🗃️  密封存储路径: {}",
        config.tee_runtime.sealed_storage_path
    );
    if !config.tee_runtime.sealed_storage_path.starts_with('/') {
        warn!(
            sealed_storage_path = %config.tee_runtime.sealed_storage_path,
            "SEALED_STORAGE_PATH 使用相对路径；若部署环境未持久化当前工作目录，凭证在重启后可能无法解密"
        );
    }
    info!("🧱 TEE 硬件构建支持: {}", TEE_HARDWARE_BUILD_ENABLED);
    info!("🌐 地址: http://{}:{}", config.host, config.port);
    info!("🗄️  存储后端: {}", config.storage_backend.as_str());

    // 显示速率限制配置
    let rate_limit_config = RateLimitConfig::from_env();
    info!(
        "🛡️  速率限制: {}/{}秒/IP",
        rate_limit_config.requests_per_window, rate_limit_config.window_seconds
    );

    // 初始化应用状态
    let app_state = initialize_app_state(&config).await?;

    // 构建路由
    let router = build_router(app_state, &config);

    // BUG-18229: 使用 tower::util::MapRequestLayer 在 NormalizePathLayer 之前保存原始 URI
    // Layer 执行顺序：最后添加的 layer 先执行
    // 我们需要：preserve_original_uri 先执行（保存原始 URI），然后 NormalizePath 执行（修改 URI）
    // 所以：preserve_original_uri.layer(NormalizePathLayer.layer(router))
    // 这样请求流程是：preserve_original_uri（保存原始URI） -> NormalizePath（去除尾斜杠） -> router
    let preserve_original_uri =
        tower::util::MapRequestLayer::new(|mut req: axum::extract::Request| {
            // Save original URI before any path normalization
            let original_uri = OriginalUri(req.uri().clone());
            req.extensions_mut().insert(original_uri);
            req
        });
    let normalized_router = NormalizePathLayer::trim_trailing_slash().layer(router);
    let app = preserve_original_uri.layer(normalized_router);

    // 绑定地址并启动服务器
    let addr = config.socket_addr();
    let listener = tokio::net::TcpListener::bind(addr).await?;

    info!("✅ 服务器启动成功！");
    info!(
        "📚 API 文档: http://{}:{}/api/v1/",
        config.host, config.port
    );
    info!("💊 健康检查: http://{}:{}/health", config.host, config.port);
    info!("");

    // 启动服务器
    axum::serve(listener, ServiceExt::<Request>::into_make_service(app)).await?;

    Ok(())
}

/// 初始化日志系统
fn init_logging(config: &ServerConfig) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();
}

/// 初始化应用状态
async fn initialize_app_state(
    config: &ServerConfig,
) -> Result<AppState, Box<dyn std::error::Error>> {
    // --- Enclave ---
    info!(
        module = "enclave",
        status = "initializing",
        "开始初始化 TEE Enclave"
    );
    let tee_capabilities = validate_runtime_requirements(&config.tee_runtime)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    let enclave_config = EnclaveConfig {
        runtime_mode: config.tee_runtime.mode,
        debug_mode: config.tee_runtime.debug_mode,
        sealed_storage_path: config.tee_runtime.sealed_storage_path.clone(),
        ..Default::default()
    };
    let mut enclave = Enclave::new(enclave_config);
    enclave.initialize()?;
    let root_key_source = enclave
        .root_key_source()
        .ok_or_else(|| std::io::Error::other("Enclave initialized without an L0 source"))?;
    let tee_boot_profile = config.tee_runtime.mode.as_str();
    let tee_effective_mode = config.tee_runtime.mode.as_str();
    let tee_enclave_running = enclave.is_running();
    let tee_mrenclave_hex = if tee_enclave_running {
        hex::encode(enclave.mrenclave())
    } else {
        "unavailable".to_string()
    };
    info!(
        module = "enclave",
        status = "ready",
        tee_hardware_build_enabled = TEE_HARDWARE_BUILD_ENABLED,
        tee_initialized = tee_enclave_running,
        tee_requested_mode = tee_boot_profile,
        tee_effective_mode = tee_effective_mode,
        tee_detected_type = tee_capabilities.detected_type.description(),
        tee_remote_attestation_available = tee_capabilities.remote_attestation_available,
        mrenclave = %tee_mrenclave_hex,
        "TEE Enclave 初始化完成"
    );

    // --- Key Hierarchy ---
    info!(
        module = "key_hierarchy",
        status = "initializing",
        "开始初始化密钥层次结构"
    );
    let hierarchy = enclave.bootstrap_key_hierarchy()?;
    info!(
        module = "key_hierarchy",
        status = "ready",
        requested_mode = tee_boot_profile,
        effective_mode = tee_effective_mode,
        root_key_source = root_key_source.as_str(),
        "密钥层次结构已从 Enclave 当前 L1 状态装载"
    );
    let shared_enclave = Arc::new(tokio::sync::Mutex::new(enclave));

    // --- Storage Backend ---
    info!(
        module = "storage",
        status = "initializing",
        backend = config.storage_backend.as_str(),
        "开始初始化凭证存储后端"
    );
    let vault = Arc::new(build_credential_vault(config).await?);
    info!(
        module = "storage",
        status = "ready",
        backend = config.storage_backend.as_str(),
        "凭证存储后端就绪"
    );

    let database_pool = initialize_database_pool().await?;

    // --- Audit ---
    info!(
        module = "audit",
        status = "initializing",
        "开始初始化审计日志存储"
    );
    let (audit_storage, audit_verifier_public_key, audit_backend) =
        initialize_audit_storage(database_pool.clone()).await?;
    let audit_logger = Arc::new(StorageAuditLogger::new(audit_storage.clone()));
    info!(
        module = "audit",
        status = "ready",
        backend = audit_backend,
        "审计日志存储就绪"
    );

    let credential_state = CredentialAppState {
        vault,
        key_hierarchy: Arc::new(RwLock::new(hierarchy)),
        enclave: shared_enclave,
        audit_logger,
    };

    info!(
        event = "CREDBRIDGE_TEE_OPS_READY",
        tee_enclave_running = tee_enclave_running,
        requested_mode = tee_boot_profile,
        effective_mode = tee_effective_mode,
        root_key_source = root_key_source.as_str(),
        mrenclave = %tee_mrenclave_hex,
        storage_backend = config.storage_backend.as_str(),
        "TEE 已就绪，凭证业务加密路径已绑定 Enclave"
    );

    // --- Auth ---
    info!(
        module = "auth",
        status = "initializing",
        "开始初始化认证模块"
    );
    let audit_state = AuditApiState {
        storage: audit_storage.clone(),
        verifier_public_key: audit_verifier_public_key,
    };

    // --- Tenant (must be before Auth since Auth depends on it) ---
    info!(
        module = "tenant",
        status = "initializing",
        "开始初始化租户配置"
    );
    let tenant_store = build_tenant_config_store(database_pool.clone()).await?;
    let tenant_service: Arc<dyn TenantService> = Arc::new(PostgresTenantStorage::new(
        database_pool.clone(),
        tenant_store.clone(),
    ));
    info!(module = "tenant", status = "ready", "租户配置就绪");

    // --- Privy 配置加载 ---
    let privy_config_result = vault_service::config::PrivyConfig::from_env();
    let auth_tenant_manager = TenantManagerBuilder::new(tenant_store.clone())
        .with_tenant_storage(tenant_service.clone())
        .with_db_pool(database_pool.clone())
        .build();
    let auth_service_builder = vault_service::auth::AuthServiceImpl::new(
        Some(database_pool.pool().clone()),
        auth_tenant_manager,
    );

    // 根据 Privy 配置是否加载成功，决定是否初始化 JWKS verifier
    let auth_service = match privy_config_result {
        Ok(config) => {
            info!(
                module = "privy",
                status = "configured",
                app_id = %config.app_id,
                app_secret = %config.app_secret,
                jwks_url = %config.jwks_url,
                api_url = %config.api_url,
                mock_enabled = config.mock_enabled,
                "Privy 配置已加载"
            );
            auth_service_builder.with_privy_config(config)
        }
        Err(e) => {
            warn!(
                module = "privy",
                status = "not_configured",
                error = %e,
                "Privy 配置未加载，认证功能将受限。请设置 PRIVY_APP_ID 和 PRIVY_APP_SECRET 环境变量，或启用 PRIVY_MOCK_ENABLED=true 用于开发测试"
            );
            auth_service_builder
        }
    };
    let (token_store, token_backend) = initialize_token_store()?;
    info!(
        module = "token_state",
        status = "ready",
        backend = token_backend,
        "Token 状态存储就绪"
    );

    let auth_state =
        AuthApiState::new_with_token_store(std::sync::Arc::new(auth_service), token_store)
            .with_audit_storage(audit_storage.clone())
            .with_vault(credential_state.vault.clone());
    info!(module = "auth", status = "ready", "认证模块就绪");

    // --- Rate Limit ---
    info!(
        module = "rate_limit",
        status = "initializing",
        "开始初始化速率限制"
    );
    let rate_limit_config = RateLimitConfig::from_env();
    let rate_limit_state = initialize_rate_limit_state(rate_limit_config).await?;
    info!(
        module = "rate_limit",
        status = "ready",
        backend = "redis",
        "速率限制就绪"
    );

    // --- Attestation ---
    info!(
        module = "attestation",
        status = "initializing",
        "开始初始化 Attestation API"
    );
    let attestation_state = match init_attestation_api(
        AttestationApiConfig {
            tee_runtime: config.tee_runtime.clone(),
            root_key_source: root_key_source.as_str().to_string(),
            ..Default::default()
        },
        credential_state.enclave.clone(),
        Some(tee_capabilities.clone()),
    )
    .await
    {
        Ok(state) => {
            info!(
                module = "attestation",
                status = "ready",
                tee_hardware_build_enabled = TEE_HARDWARE_BUILD_ENABLED,
                requested_mode = tee_boot_profile,
                effective_mode = tee_effective_mode,
                root_key_source = root_key_source.as_str(),
                "Attestation API 就绪"
            );
            Some(state)
        }
        Err(e) => {
            return Err(std::io::Error::other(format!("Attestation API 初始化失败: {e}")).into());
        }
    };

    // --- Sandbox ---
    info!(
        module = "sandbox",
        status = "initializing",
        "开始初始化沙箱 API"
    );
    let sandbox_state = initialize_sandbox_state(
        config,
        Some(database_pool.pool().clone()),
        Some(credential_state.vault.clone()),
        Some(credential_state.key_hierarchy.clone()),
        Some(credential_state.enclave.clone()),
    )
    .await
    .map_err(|e| std::io::Error::other(format!("Sandbox API 初始化失败，服务启动终止: {e}")))?;
    info!(module = "sandbox", status = "ready", "沙箱 API 就绪");

    Ok(AppState {
        config: config.clone(),
        credential_state,
        audit_state,
        auth_state,
        tenant_store,
        tenant_service,
        rate_limit_state,
        attestation_state,
        sandbox_state,
        startup_checks: vec![
            SelfCheckItem::ready("vault"),
            SelfCheckItem::ready("audit_log"),
            SelfCheckItem::ready("auth"),
            SelfCheckItem::ready("tenant"),
            SelfCheckItem::ready("rate_limit"),
            SelfCheckItem::ready("sandbox"),
        ],
    })
}

async fn build_credential_vault(
    config: &ServerConfig,
) -> Result<CredentialVault, Box<dyn std::error::Error>> {
    let resolved_backend = resolve_storage_backend(config).map_err(std::io::Error::other)?;

    let backend = match resolved_backend {
        StorageBackendKind::Vault => {
            let backend = VaultStorageBackend::from_env().await?;
            info!("✅ 凭证存储已连接到 HashiCorp Vault");
            CredentialVault::with_backend(Box::new(backend))
        }
        StorageBackendKind::Postgres => {
            let backend = PostgresStorageBackend::from_env().await?;
            info!("✅ 凭证存储已连接到 PostgreSQL schema={}", backend.schema());
            CredentialVault::with_backend(Box::new(backend))
        }
        StorageBackendKind::Auto => unreachable!("auto backend should be resolved before init"),
    };

    Ok(backend)
}

/// 初始化沙箱状态
async fn initialize_sandbox_state(
    _config: &ServerConfig,
    database_pool: Option<sqlx::PgPool>,
    vault: Option<Arc<CredentialVault>>,
    key_hierarchy: Option<Arc<RwLock<KeyHierarchy>>>,
    enclave: Option<SharedEnclave>,
) -> Result<SandboxState, Box<dyn std::error::Error>> {
    use vault_service::tee::sandbox::config::SandboxConfig;

    let database_pool = database_pool
        .ok_or_else(|| std::io::Error::other("沙箱持久化要求 DATABASE_URL，内存回退已禁用"))?;

    let config = SandboxConfig::from_env();
    let state = SandboxState::new(config, Some(database_pool), vault, key_hierarchy, enclave)
        .await
        .map_err(|e| format!("沙箱初始化失败: {e:?}"))?;

    Ok(state)
}

/// 构建路由器
fn build_router(app_state: AppState, config: &ServerConfig) -> Router {
    // CORS 配置
    let cors = create_cors_layer(config);

    // 构建 API 路由
    let api_routes = build_api_routes(app_state.clone());

    // 速率限制层
    let rate_limit_layer = axum::middleware::from_fn(rate_limit_middleware);

    // 构建主路由器
    Router::new()
        // 根路径
        .route("/", get(root_handler))
        // API 路由
        .nest(API_BASE_PATH, api_routes)
        // 健康检查路由
        .route("/health", get(health_check))
        .route("/ready", get(health_check_detail))
        .route("/health/detail", get(health_check_detail))
        // Prometheus 指标端点
        .route("/metrics", get(metrics_handler))
        // 速率限制中间件
        .layer(rate_limit_layer)
        // 全局 CORS
        .layer(cors)
        // 请求/响应日志拦截器（最外层，覆盖所有请求）
        .layer(axum::middleware::from_fn(
            vault_service::api::logging_middleware::request_logging_middleware,
        ))
        // 全局追踪（span 生命周期）
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(trace::DefaultMakeSpan::new().level(Level::DEBUG))
                .on_response(trace::DefaultOnResponse::new().level(Level::DEBUG)),
        )
        // 添加速率限制状态扩展
        .layer(Extension(app_state.rate_limit_state.clone()))
        // 添加配置扩展
        .layer(Extension(app_state))
}

/// 创建 CORS 层
fn create_cors_layer(config: &ServerConfig) -> CorsLayer {
    let cors = CorsLayer::new().allow_methods(Any).allow_headers(Any);

    if config.environment == Environment::Production {
        // 生产环境使用更严格的 CORS 配置
        cors.allow_origin(
            env::var("CREDBRIDGE_ALLOWED_ORIGINS")
                .ok()
                .and_then(|origins| {
                    let origins: Vec<_> =
                        origins.split(',').filter_map(|s| s.parse().ok()).collect();
                    if origins.is_empty() {
                        None
                    } else {
                        Some(tower_http::cors::AllowOrigin::list(origins))
                    }
                })
                .unwrap_or_else(|| {
                    tower_http::cors::AllowOrigin::exact("https://credbridge.io".parse().unwrap())
                }),
        )
    } else {
        // 开发环境允许所有来源
        cors.allow_origin(Any)
    }
}

/// 构建 API 路由
fn build_api_routes(app_state: AppState) -> Router {
    // 创建 Token 存储用于黑名单检查
    let token_store = app_state.auth_state.token_store.clone();
    // Generate a secret key for PASETO token validation
    // In production, this should come from a secure configuration or Vault
    let secret_key = vec![0u8; 32]; // Placeholder - should be from config

    // 认证路由（公开，不需要认证）
    let locale_state = LocaleResolverState::new(
        app_state.auth_state.auth_service.clone(),
        app_state.tenant_store.clone(),
    );
    let public_locale_layer =
        axum::middleware::from_fn_with_state(locale_state.clone(), locale_middleware);
    let protected_locale_layer =
        axum::middleware::from_fn_with_state(locale_state, locale_middleware);

    let auth_routes = auth_routes()
        .with_state(app_state.auth_state.clone())
        .layer(public_locale_layer);
    let protected_auth_routes = protected_auth_routes().with_state(app_state.auth_state.clone());

    // ========== 受保护的路由（需要认证） ==========

    // 凭证管理路由
    let credential_routes = credential_routes().with_state(app_state.credential_state.clone());

    // 审计日志路由
    let audit_routes = audit_routes(app_state.audit_state.clone());

    // 租户管理路由（持久化存储）
    let tenant_store = app_state.tenant_store.clone();
    let tenant_service = app_state.tenant_service.clone();
    let tenant_manager = TenantManagerBuilder::new(tenant_store)
        .with_tenant_storage(tenant_service.clone())
        .build();

    // 手动创建 TenantApiState
    let tenant_api_state = TenantApiState {
        tenant_manager: Arc::new(tenant_manager),
        tenant_service,
        auth_service: Some(app_state.auth_state.auth_service.clone()),
    };
    let tenant_routes = tenant_routes::<Arc<dyn TenantConfigStore>>().with_state(tenant_api_state);
    let notifications_routes = notifications_routes();
    let token_routes = token_routes(app_state.auth_state.clone());
    let service_account_routes = service_account_routes(app_state.auth_state.clone());

    // 认证中间件层
    let auth_layer = axum::middleware::from_fn_with_state(
        (
            token_store.clone(),
            secret_key.clone(),
            app_state.auth_state.auth_service.clone(),
        ),
        auth_middleware,
    );

    // 构建受保护的路由组
    let protected_routes = Router::new()
        // 嵌套凭证路由
        .merge(credential_routes)
        // 嵌套审计路由
        .merge(audit_routes)
        // 嵌套租户路由
        .merge(tenant_routes)
        // 通知列表路由
        .merge(notifications_routes)
        .merge(token_routes)
        .merge(service_account_routes)
        // 认证用户信息与偏好
        .merge(protected_auth_routes)
        // locale 解析
        .layer(protected_locale_layer)
        // 应用认证中间件
        .layer(auth_layer);

    // Attestation 路由（独立，可能需要不同的认证策略）
    // 注意：Attestation 端点部分公开，部分需要认证
    let attestation_routes = if let Some(ref att_state) = app_state.attestation_state {
        let routes = attestation_routes(Arc::clone(att_state));
        Some(Router::new().nest("/attestation", routes))
    } else {
        None
    };

    // 沙箱路由（需要认证）
    let sandbox_auth_layer = axum::middleware::from_fn_with_state(
        (
            token_store,
            secret_key,
            app_state.auth_state.auth_service.clone(),
        ),
        auth_middleware,
    );
    let sandbox_routes = sandbox_routes()
        .with_state(app_state.sandbox_state.clone())
        .layer(sandbox_auth_layer);

    // 合并所有路由
    let mut router = Router::new()
        .route("/", get(api_root_handler))
        // 认证路由（公开）
        .merge(auth_routes)
        // 受保护的路由
        .merge(protected_routes);

    // 添加 attestation 路由（如果已初始化）
    if let Some(att_routes) = attestation_routes {
        router = router.merge(att_routes);
    }

    router = router.merge(sandbox_routes);

    router.layer(Extension(app_state))
}

async fn initialize_database_pool() -> Result<DatabasePool, Box<dyn std::error::Error>> {
    let pool = DatabasePool::from_env()
        .await
        .map_err(|error| std::io::Error::other(format!("数据库连接池初始化失败: {error}")))?;
    pool.health_check()
        .await
        .map_err(|error| std::io::Error::other(format!("数据库健康检查失败: {error}")))?;
    info!(
        module = "database",
        status = "ready",
        backend = "postgres",
        "数据库连接池就绪"
    );
    Ok(pool)
}

async fn initialize_audit_storage(
    database_pool: DatabasePool,
) -> Result<(Arc<dyn AuditStorage>, Vec<u8>, &'static str), Box<dyn std::error::Error>> {
    let storage = PostgresAuditStorageAdapter::new(database_pool.pool().clone())
        .await
        .map_err(|error| {
            std::io::Error::other(format!("PostgreSQL 审计存储初始化失败: {error}"))
        })?;
    let public_key = storage.public_key().to_vec();
    Ok((Arc::new(storage), public_key, "postgres"))
}

async fn build_tenant_config_store(
    database_pool: DatabasePool,
) -> Result<Arc<dyn TenantConfigStore>, Box<dyn std::error::Error>> {
    info!(
        module = "tenant",
        backend = "postgres",
        "租户配置使用 PostgreSQL"
    );
    Ok(Arc::new(PostgresTenantConfigStore::new(database_pool)))
}

fn initialize_token_store() -> Result<(TokenStore, &'static str), Box<dyn std::error::Error>> {
    let redis_url = env::var("REDIS_URL")
        .map_err(|_| std::io::Error::other("token state 要求 REDIS_URL，内存回退已禁用"))?;
    let token_store = create_redis_token_store(&redis_url)
        .map_err(|error| std::io::Error::other(format!("Redis Token 存储初始化失败: {error}")))?;
    Ok((token_store, "redis"))
}

async fn initialize_rate_limit_state(
    config: RateLimitConfig,
) -> Result<RateLimitState, Box<dyn std::error::Error>> {
    let redis_url = env::var("REDIS_URL")
        .map_err(|_| std::io::Error::other("rate limit state 要求 REDIS_URL，内存回退已禁用"))?;
    RateLimitState::new(config, &redis_url)
        .await
        .map_err(|error| std::io::Error::other(format!("Redis 限流存储初始化失败: {error}")).into())
}

#[derive(Debug, Clone)]
struct RuntimeOverview {
    readiness: StartupReadiness,
    vault_status: String,
    enclave_status: String,
    audit_status: String,
    attestation_status: String,
    enclave_running: bool,
    attestation_quote_valid: bool,
}

async fn build_runtime_overview(state: &AppState) -> RuntimeOverview {
    let enclave_check = {
        let enclave = state.credential_state.enclave.lock().await;
        if enclave.is_running() {
            SelfCheckItem::ready("enclave")
        } else {
            SelfCheckItem::failed("enclave", format!("enclave state is {}", enclave.state()))
        }
    };

    let (attestation_check, attestation_status, attestation_quote_valid) =
        if let Some(attestation_state) = &state.attestation_state {
            let snapshot = attestation_state.runtime_snapshot().await;
            let quote_valid = snapshot.quote_valid;
            (
                snapshot.as_readiness_check(),
                snapshot.health_label().to_string(),
                quote_valid,
            )
        } else {
            (
                SelfCheckItem::failed("attestation", "attestation API unavailable"),
                SelfCheckStatus::Failed.as_str().to_string(),
                false,
            )
        };

    let mut checks = state.startup_checks.clone();
    checks.push(enclave_check.clone());
    checks.push(attestation_check);

    let readiness = StartupReadiness::from_checks(checks);

    RuntimeOverview {
        vault_status: component_status(&readiness, "vault"),
        enclave_status: enclave_check.status.as_str().to_string(),
        audit_status: component_status(&readiness, "audit_log"),
        attestation_status,
        enclave_running: enclave_check.status.is_ready(),
        attestation_quote_valid,
        readiness,
    }
}

fn component_status(readiness: &StartupReadiness, component: &str) -> String {
    readiness
        .checks
        .iter()
        .find(|check| check.component == component)
        .map(|check| check.status.as_str().to_string())
        .unwrap_or_else(|| SelfCheckStatus::Failed.as_str().to_string())
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn render_metrics(
    environment: &str,
    timestamp: u64,
    readiness: &StartupReadiness,
    enclave_running: bool,
    attestation_quote_valid: bool,
) -> String {
    format!(
        r#"# HELP credbridge_up Service liveness status
# TYPE credbridge_up gauge
credbridge_up{{version="{}"}} 1

# HELP credbridge_ready Service readiness status
# TYPE credbridge_ready gauge
credbridge_ready{{status="{}"}} {}

# HELP credbridge_build_info Build information
# TYPE credbridge_build_info gauge
credbridge_build_info{{version="{}",env="{}"}} 1

# HELP credbridge_timestamp Current timestamp
# TYPE credbridge_timestamp gauge
credbridge_timestamp {}

# HELP credbridge_attestation_quote_valid Attestation quote validity
# TYPE credbridge_attestation_quote_valid gauge
credbridge_attestation_quote_valid{{}} {}

# HELP credbridge_enclave_running Enclave running state
# TYPE credbridge_enclave_running gauge
credbridge_enclave_running{{}} {}
"#,
        env!("CARGO_PKG_VERSION"),
        readiness.status.as_str(),
        u8::from(readiness.ready),
        env!("CARGO_PKG_VERSION"),
        environment,
        timestamp,
        u8::from(attestation_quote_valid),
        u8::from(enclave_running),
    )
}

/// 根路径处理器
async fn root_handler() -> impl IntoResponse {
    Json(json!({
        "name": "CredBridge",
        "description": "TEE Credential Vault - Secure AI-Native Secret Storage",
        "version": env!("CARGO_PKG_VERSION"),
        "api_base": API_BASE_PATH,
        "health_check": "/health",
        "readiness_check": "/ready",
    }))
}

/// API 根路径处理器
async fn api_root_handler(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let response = ApiRootResponse {
        name: "CredBridge API".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        environment: state.config.environment.as_str().to_string(),
        endpoints: vec![
            ApiEndpoint {
                path: format!("{API_BASE_PATH}/credentials"),
                description: "凭证管理 API".to_string(),
            },
            ApiEndpoint {
                path: format!("{API_BASE_PATH}/audit/logs"),
                description: "审计日志 API".to_string(),
            },
            ApiEndpoint {
                path: format!("{API_BASE_PATH}/tenants"),
                description: "租户管理 API".to_string(),
            },
            ApiEndpoint {
                path: "/health".to_string(),
                description: "进程存活检查（liveness）".to_string(),
            },
            ApiEndpoint {
                path: "/ready".to_string(),
                description: "服务就绪检查（readiness）".to_string(),
            },
        ],
    };

    (StatusCode::OK, Json(response))
}

/// 健康检查处理器
async fn health_check(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let timestamp = current_timestamp();
    let overview = build_runtime_overview(&state).await;

    let response = HealthResponse {
        status: "alive".to_string(),
        ready: overview.readiness.ready,
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp,
    };

    (StatusCode::OK, Json(response))
}

/// Prometheus 指标端点
async fn metrics_handler(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let timestamp = current_timestamp();
    let overview = build_runtime_overview(&state).await;
    let metrics = render_metrics(
        state.config.environment.as_str(),
        timestamp,
        &overview.readiness,
        overview.enclave_running,
        overview.attestation_quote_valid,
    );

    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; charset=utf-8",
        )],
        metrics,
    )
}

/// 详细健康检查处理器
async fn health_check_detail(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let timestamp = current_timestamp();
    let overview = build_runtime_overview(&state).await;

    let response = HealthDetailResponse {
        status: overview.readiness.status.as_str().to_string(),
        live: overview.readiness.live,
        ready: overview.readiness.ready,
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp,
        components: ComponentHealth {
            vault: overview.vault_status,
            enclave: overview.enclave_status,
            audit_log: overview.audit_status,
            attestation: overview.attestation_status,
        },
    };

    let status_code = if overview.readiness.ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status_code, Json(response))
}

// 开发模式演示函数（可选）
#[allow(dead_code)]
fn demonstrate_key_hierarchy() -> Result<(), Box<dyn std::error::Error>> {
    println!("📋 演示: L0-L3 四层密钥层次架构 (HKDF-SHA256)");
    println!("═══════════════════════════════════════════════════════");
    println!();

    // 初始化 Enclave
    println!("[Step 1] 初始化 SGX Enclave");
    let config = EnclaveConfig {
        runtime_mode: TeeRuntimeMode::Simulation,
        debug_mode: true,
        ..Default::default()
    };
    let mut enclave = Enclave::new(config);
    enclave.initialize()?;
    println!("  ✓ Enclave 状态: {:?}", enclave.state());
    println!(
        "  ✓ L0 来源: {}",
        enclave
            .root_key_source()
            .map(|source| source.as_str())
            .unwrap_or("unknown")
    );
    println!();

    // 从 Enclave 当前 L1 构造层次快照
    println!("[Step 2] 从 Enclave 当前 L1 构造层次快照");
    let hierarchy = enclave.bootstrap_key_hierarchy()?;
    let l1_handle = hierarchy
        .master_key_handle()
        .ok_or_else(|| std::io::Error::other("missing master key handle"))?;
    println!("  ✓ L1 Master Key 句柄: {}", hex_encode(&l1_handle[..8]));
    println!();

    println!("✅ 密钥层次架构演示完成！");

    Ok(())
}

/// hex 编码辅助函数
fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut result, "{byte:02x}").unwrap();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{StorageBackendKind, render_metrics, resolve_auto_storage_backend};
    use vault_service::tee::{SelfCheckItem, StartupReadiness};

    #[test]
    fn auto_backend_prefers_postgres_when_database_url_exists() {
        let backend = resolve_auto_storage_backend(true, false, false).unwrap();
        assert_eq!(backend, StorageBackendKind::Postgres);
    }

    #[test]
    fn auto_backend_uses_vault_when_both_vault_vars_exist() {
        let backend = resolve_auto_storage_backend(false, true, true).unwrap();
        assert_eq!(backend, StorageBackendKind::Vault);
    }

    #[test]
    fn auto_backend_rejects_missing_persistent_configuration() {
        let error = resolve_auto_storage_backend(false, false, false).unwrap_err();
        assert!(error.contains("显式配置 PostgreSQL/Vault 持久化后端"));
    }

    #[test]
    fn auto_backend_rejects_partial_vault_configuration() {
        let error = resolve_auto_storage_backend(false, true, false).unwrap_err();
        assert!(error.contains("VAULT_ADDR/VAULT_TOKEN"));
    }

    #[test]
    fn metrics_render_readiness_and_attestation_truthfully() {
        let readiness = StartupReadiness::from_checks(vec![
            SelfCheckItem::ready("vault"),
            SelfCheckItem::failed("attestation", "no quote"),
        ]);

        let metrics = render_metrics("development", 123, &readiness, true, false);

        assert!(metrics.contains("credbridge_up"));
        assert!(metrics.contains("credbridge_ready{status=\"failed\"} 0"));
        assert!(metrics.contains("credbridge_attestation_quote_valid{} 0"));
        assert!(metrics.contains("credbridge_enclave_running{} 1"));
    }
}
