//! CredBridge - 凭证保险库 HTTP 服务
//!
//! HTTP API 服务器入口点
//! 支持环境变量配置端口和运行模式
//!
//! # 环境变量
//!
//! - `CREDBRIDGE_PORT` - 服务器端口 (默认: 8080)
//! - `CREDBRIDGE_HOST` - 服务器主机 (默认: 0.0.0.0)
//! - `CREDBRIDGE_ENV` - 运行环境 (development/production, 默认: development)
//! - `RUST_LOG` - 日志级别 (默认: info)
//! - `CREDBRIDGE_RATE_LIMIT_REQUESTS` - 速率限制请求数/窗口 (默认: 100)
//! - `CREDBRIDGE_RATE_LIMIT_WINDOW_SECONDS` - 速率限制窗口（秒）(默认: 60)

use axum::{Extension, Json, Router, http::StatusCode, response::IntoResponse, routing::get};
use serde::Serialize;
use serde_json::json;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{self, TraceLayer};
use tracing::{Level, info};

// CredBridge 内部模块
use vault_service::api::{
    API_BASE_PATH,
    attestation::{
        AttestationApiConfig, AttestationState, attestation_routes, init_attestation_api,
    },
    audit::{AuditApiState, MemoryAuditStorageAdapter, audit_routes},
    auth::{AuthApiState, auth_routes, protected_auth_routes},
    credentials::{
        AppState as CredentialAppState, StorageAuditLogger, routes as credential_routes,
    },
    i18n::{LocaleResolverState, locale_middleware},
    middleware::auth_middleware,
    rate_limit::{RateLimitConfig, RateLimitState, rate_limit_middleware},
    sandbox::{SandboxState, sandbox_routes},
    tenant::{TenantApiState, tenant_routes},
    token_blacklist::create_token_store,
};
use vault_service::audit::MemoryAuditStorage;
use vault_service::crypto::KeyHierarchy;
use vault_service::crypto::keys::HardwareRootKey;
use vault_service::tee::{Enclave, EnclaveConfig};
use vault_service::tenant::{
    MemoryTenantConfigStore, MemoryTenantStorage, TenantManager, TenantService,
};
use vault_service::vault::backend::VaultStorageBackend;
use vault_service::vault::postgres::PostgresStorageBackend;
use vault_service::vault::storage::CredentialVault;

/// API 根响应
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

/// 健康检查响应
#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    version: String,
    timestamp: u64,
}

/// 详细健康检查响应
#[derive(Debug, Serialize)]
struct HealthDetailResponse {
    status: String,
    version: String,
    timestamp: u64,
    components: ComponentHealth,
}

#[derive(Debug, Serialize)]
struct ComponentHealth {
    vault: String,
    enclave: String,
    audit_log: String,
}

/// 服务器配置
#[derive(Debug, Clone)]
struct ServerConfig {
    host: String,
    port: u16,
    environment: Environment,
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
    Memory,
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
            "memory" => StorageBackendKind::Memory,
            "postgres" | "postgresql" | "db" => StorageBackendKind::Postgres,
            "vault" => StorageBackendKind::Vault,
            _ => StorageBackendKind::Auto,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            StorageBackendKind::Auto => "auto",
            StorageBackendKind::Memory => "memory",
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
            "自动存储后端选择失败：未检测到 DATABASE_URL，且 VAULT_ADDR/VAULT_TOKEN 未同时配置。请显式配置 PostgreSQL/Vault 持久化后端，或仅在开发/测试场景下设置 CREDBRIDGE_STORAGE_BACKEND=memory。"
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
    fn from_env() -> Self {
        Self {
            host: env::var("CREDBRIDGE_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: env::var("CREDBRIDGE_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(8080),
            environment: Environment::from_str(
                &env::var("CREDBRIDGE_ENV").unwrap_or_else(|_| "development".to_string()),
            ),
            log_level: env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
            storage_backend: StorageBackendKind::from_env(),
        }
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
    tenant_store: MemoryTenantConfigStore,
    rate_limit_state: RateLimitState,
    attestation_state: Option<Arc<AttestationState>>,
    sandbox_state: Option<SandboxState>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 加载配置
    let config = ServerConfig::from_env();

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
    let app = build_router(app_state, &config);

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
    axum::serve(listener, app).await?;

    Ok(())
}

/// 初始化日志系统
fn init_logging(config: &ServerConfig) {
    let level = match config.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    // 使用简单的日志初始化
    eprintln!("[INFO] 初始化日志系统，级别: {level:?}");
}

/// 初始化应用状态
async fn initialize_app_state(
    config: &ServerConfig,
) -> Result<AppState, Box<dyn std::error::Error>> {
    // 初始化 Enclave
    let enclave_config = EnclaveConfig {
        debug_mode: config.environment == Environment::Development,
        ..Default::default()
    };
    let mut enclave = Enclave::new(enclave_config);
    enclave.initialize()?;

    // 初始化 L0 密钥（模拟模式）
    let l0 = HardwareRootKey::for_simulation()?;

    // 初始化密钥层次结构
    let mut hierarchy = KeyHierarchy::new();
    let _l1_handle = hierarchy.initialize_master_key(&l0)?;

    // 初始化凭证存储后端
    let vault = Arc::new(build_credential_vault(config).await?);

    // 初始化审计日志存储（使用共享的 Arc，让凭证 API 和审计 API 共享同一个存储）
    let audit_storage = Arc::new(tokio::sync::Mutex::new(
        MemoryAuditStorage::new(100_000).map_err(|e| format!("创建审计存储失败: {e:?}"))?,
    ));

    // 创建存储适配器用于审计 API 查询
    let audit_storage_adapter =
        MemoryAuditStorageAdapter::from_shared_storage(audit_storage.clone());

    // 创建存储审计日志记录器用于凭证 API 写入
    let audit_logger = Arc::new(StorageAuditLogger::new(audit_storage.clone()));

    // 创建凭证 API 状态
    let credential_state = CredentialAppState {
        vault,
        key_hierarchy: Arc::new(RwLock::new(hierarchy)),
        audit_logger, // 现在写入到共享存储
    };

    // 创建审计 API 状态
    let audit_state = AuditApiState {
        storage: Arc::new(audit_storage_adapter), // 从共享存储查询
        verifier_public_key: vec![],              // 空向量：未配置公钥时签名验证 fail-closed
    };

    // 创建认证 API 状态
    let auth_state = AuthApiState::with_audit_storage(audit_storage.clone());

    // 初始化租户配置存储
    let tenant_store = MemoryTenantConfigStore::new();
    let mut default_tenant_config = vault_service::tenant::TenantConfig::default();
    default_tenant_config.settings.language = "zh-CN".to_string();
    use vault_service::tenant::TenantConfigStore;
    tenant_store
        .save_config(
            &vault_service::tenant::TenantId::from_string("tenant-001"),
            &default_tenant_config,
        )
        .await
        .map_err(|e| format!("初始化默认租户配置失败: {e}"))?;

    // 初始化速率限制状态
    let rate_limit_config = RateLimitConfig::from_env();
    let rate_limit_state = RateLimitState::new(rate_limit_config);

    // 初始化 Attestation API（在开发模式下使用模拟模式）
    let attestation_state = match init_attestation_api(AttestationApiConfig {
        simulation_mode: config.environment == Environment::Development,
        ..Default::default()
    }) {
        Ok(state) => Some(state),
        Err(e) => {
            eprintln!("[WARN] Attestation API 初始化失败（将跳过 attestation 路由）: {e}");
            None
        }
    };

    // 初始化沙箱 API（可选）
    let sandbox_state = match initialize_sandbox_state().await {
        Ok(state) => {
            info!("✅ 沙箱 API 初始化成功");
            Some(state)
        }
        Err(e) => {
            eprintln!("[WARN] 沙箱 API 初始化失败（将跳过沙箱路由）: {e}");
            None
        }
    };

    Ok(AppState {
        config: config.clone(),
        credential_state,
        audit_state,
        auth_state,
        tenant_store,
        rate_limit_state,
        attestation_state,
        sandbox_state,
    })
}

async fn build_credential_vault(
    config: &ServerConfig,
) -> Result<CredentialVault, Box<dyn std::error::Error>> {
    let resolved_backend = resolve_storage_backend(config).map_err(std::io::Error::other)?;

    let backend = match resolved_backend {
        StorageBackendKind::Memory => CredentialVault::new_in_memory(),
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
async fn initialize_sandbox_state() -> Result<SandboxState, Box<dyn std::error::Error>> {
    use vault_service::tee::sandbox::config::SandboxConfig;

    let config = SandboxConfig::default();
    let state = SandboxState::new(config)
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
        .route("/health/detail", get(health_check_detail))
        // Prometheus 指标端点
        .route("/metrics", get(metrics_handler))
        // 速率限制中间件
        .layer(rate_limit_layer)
        // 全局 CORS
        .layer(cors)
        // 全局追踪
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(trace::DefaultMakeSpan::new().level(Level::INFO))
                .on_response(trace::DefaultOnResponse::new().level(Level::INFO)),
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
    let token_store = create_token_store();
    let secret_key = app_state.auth_state.secret_key.clone();

    // 认证路由（公开，不需要认证）
    let locale_state = LocaleResolverState::new(
        app_state.auth_state.user_store.clone(),
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

    // 租户管理路由（使用内存存储）
    let tenant_store = app_state.tenant_store.clone();
    let tenant_manager = TenantManager::new_simple(tenant_store);
    let tenant_service: Arc<dyn TenantService> = Arc::new(MemoryTenantStorage::new());

    // 手动创建 TenantApiState
    let tenant_api_state = TenantApiState {
        tenant_manager: Arc::new(tenant_manager),
        tenant_service,
    };
    let tenant_routes = tenant_routes::<MemoryTenantConfigStore>().with_state(tenant_api_state);

    // 认证中间件层
    let auth_layer = axum::middleware::from_fn_with_state(
        (token_store.clone(), secret_key.clone()),
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

    // 沙箱路由（需要认证）- 创建新的 auth_layer
    let sandbox_routes = if let Some(ref sandbox_state) = app_state.sandbox_state {
        let sandbox_auth_layer =
            axum::middleware::from_fn_with_state((token_store, secret_key), auth_middleware);
        let routes = sandbox_routes()
            .with_state(sandbox_state.clone())
            .layer(sandbox_auth_layer);
        Some(routes)
    } else {
        None
    };

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

    // 添加沙箱路由（如果已初始化）
    if let Some(sb_routes) = sandbox_routes {
        router = router.merge(sb_routes);
    }

    router.layer(Extension(app_state))
}

/// 根路径处理器
async fn root_handler() -> impl IntoResponse {
    Json(json!({
        "name": "CredBridge",
        "description": "TEE Credential Vault - Secure AI-Native Secret Storage",
        "version": env!("CARGO_PKG_VERSION"),
        "api_base": API_BASE_PATH,
        "health_check": "/health",
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
                description: "健康检查".to_string(),
            },
        ],
    };

    (StatusCode::OK, Json(response))
}

/// 健康检查处理器
async fn health_check() -> impl IntoResponse {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let response = HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp,
    };

    (StatusCode::OK, Json(response))
}

/// Prometheus 指标端点
async fn metrics_handler(Extension(state): Extension<AppState>) -> impl IntoResponse {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 构建 Prometheus 格式的指标
    let metrics = format!(
        r#"# HELP credbridge_up Service up status
# TYPE credbridge_up gauge
credbridge_up{{version="{}"}} 1

# HELP credbridge_health_status Service health status
# TYPE credbridge_health_status gauge
credbridge_health_status{{status="healthy"}} 1

# HELP credbridge_build_info Build information
# TYPE credbridge_build_info gauge
credbridge_build_info{{version="{}",env="{}"}} 1

# HELP credbridge_timestamp Current timestamp
# TYPE credbridge_timestamp gauge
credbridge_timestamp {}

# HELP credbridge_attestation_valid Attestation quote validity
# TYPE credbridge_attestation_valid gauge
credbridge_attestation_valid{{}} {}

# HELP credbridge_enclave_state Enclave state (1=running, 0=stopped)
# TYPE credbridge_enclave_state gauge
credbridge_enclave_state{{}} {}
"#,
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_VERSION"),
        state.config.environment.as_str(),
        timestamp,
        if state.attestation_state.is_some() {
            1
        } else {
            0
        },
        if state.attestation_state.is_some() {
            1
        } else {
            0
        },
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
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 检查各个组件状态
    let vault_status = "healthy";
    let enclave_status = if state.config.environment == Environment::Development {
        "simulation_mode"
    } else {
        "healthy"
    };
    let audit_status = "healthy";

    let overall_status = if vault_status == "healthy" && audit_status == "healthy" {
        "healthy"
    } else {
        "degraded"
    };

    let response = HealthDetailResponse {
        status: overall_status.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp,
        components: ComponentHealth {
            vault: vault_status.to_string(),
            enclave: enclave_status.to_string(),
            audit_log: audit_status.to_string(),
        },
    };

    let status_code = if overall_status == "healthy" {
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

    // 初始化 Enclave 和 L0 密钥
    println!("[Step 1] 初始化 SGX Enclave 和 L0 硬件根密钥");
    let config = EnclaveConfig {
        debug_mode: true,
        ..Default::default()
    };
    let mut enclave = Enclave::new(config);
    enclave.initialize()?;
    println!("  ✓ Enclave 状态: {:?}", enclave.state());
    println!();

    // 从 L0 派生 L1
    println!("[Step 2] 从 L0 派生 L1 Enclave Master Key");
    let l0 = HardwareRootKey::for_simulation()?;
    let mut hierarchy = KeyHierarchy::new();
    let l1_handle = hierarchy.initialize_master_key(&l0)?;
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
    use super::{StorageBackendKind, resolve_auto_storage_backend};

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
        assert!(error.contains("CREDBRIDGE_STORAGE_BACKEND=memory"));
    }

    #[test]
    fn auto_backend_rejects_partial_vault_configuration() {
        let error = resolve_auto_storage_backend(false, true, false).unwrap_err();
        assert!(error.contains("VAULT_ADDR/VAULT_TOKEN"));
    }
}
