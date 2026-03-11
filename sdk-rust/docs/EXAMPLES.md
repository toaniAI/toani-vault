# CredBridge Rust SDK - 高级示例

## 目录

1. [基础凭证操作](#1-基础凭证操作)
2. [批量凭证管理](#2-批量凭证管理)
3. [Token 管理](#3-token-管理)
4. [错误处理与重试](#4-错误处理与重试)
5. [审计日志](#5-审计日志)
6. [Axum Web 集成](#6-axum-web-集成)
7. [多租户管理](#7-多租户管理)
8. [自动刷新 Token](#8-自动刷新-token)
9. [凭证轮换](#9-凭证轮换)

---

## 1. 基础凭证操作

### 1.1 创建不同类型的凭证

```rust
use credbridge_sdk::{CredBridgeConfig, CredBridgeSDK};
use credbridge_sdk::types::CredentialType;
use serde_json::json;
use std::collections::HashMap;

struct CredentialCreator {
    sdk: CredBridgeSDK,
}

impl CredentialCreator {
    fn new(base_url: &str, token: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let config = CredBridgeConfig::new(base_url)
            .with_token(token);
        let sdk = CredBridgeSDK::new(config)?;
        Ok(Self { sdk })
    }

    // 创建用户名密码凭证
    async fn create_user_credentials(
        &self,
        service_id: &str,
        username: &str,
        password: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let credential = self.sdk.credentials()
            .create_username_password(
                service_id,
                username,
                password,
                Some(chrono::Utc::now().timestamp() + 86400 * 90), // 90天过期
                None,
            )
            .await?;

        Ok(credential.credential_id)
    }

    // 创建 API Key 凭证
    async fn create_api_credentials(
        &self,
        service_id: &str,
        api_key: &str,
        api_secret: Option<&str>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let credential = self.sdk.credentials()
            .create_api_key(
                service_id,
                api_key,
                api_secret,
                Some(chrono::Utc::now().timestamp() + 86400 * 365), // 1年过期
                None,
            )
            .await?;

        Ok(credential.credential_id)
    }

    // 创建 OAuth 刷新令牌
    async fn create_oauth_credentials(
        &self,
        service_id: &str,
        refresh_token: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let credential = self.sdk.credentials()
            .create_oauth_refresh(
                service_id,
                refresh_token,
                Some(chrono::Utc::now().timestamp() + 86400 * 180), // 180天过期
                None,
            )
            .await?;

        Ok(credential.credential_id)
    }

    // 创建自定义凭证
    async fn create_custom_credential(
        &self,
        service_id: &str,
        data: HashMap<String, serde_json::Value>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let credential = self.sdk.credentials()
            .create(
                service_id,
                CredentialType::SessionCookie,
                data,
                Some(chrono::Utc::now().timestamp() + 3600), // 1小时过期
                None,
            )
            .await?;

        Ok(credential.credential_id)
    }
}
```

### 1.2 凭证生命周期管理

```rust
use credbridge_sdk::types::{CredBridgeError, CredBridgeErrorCode};

struct CredentialLifecycleManager {
    sdk: CredBridgeSDK,
}

impl CredentialLifecycleManager {
    fn new(sdk: CredBridgeSDK) -> Self {
        Self { sdk }
    }

    // 安全获取凭证（带解密）
    async fn get_credential_secure(
        &self,
        credential_id: &str,
        reason: &str,
    ) -> Result<(serde_json::Value, HashMap<String, serde_json::Value>), Box<dyn std::error::Error>> {
        // 先获取元数据
        let metadata = self.sdk.credentials()
            .get(credential_id, None)
            .await?;

        // 检查凭证是否过期
        if let Some(expires_at) = &metadata.expires_at {
            let expires = chrono::DateTime::parse_from_rfc3339(expires_at)?;
            if expires < chrono::Utc::now() {
                return Err("Credential has expired".into());
            }
        }

        // 解密凭证
        let decrypted = self.sdk.credentials()
            .decrypt(credential_id, Some(reason), None)
            .await?;

        Ok((serde_json::to_value(&metadata)?, decrypted.plaintext_data))
    }

    // 批量更新过期凭证
    async fn renew_expiring_credentials(
        &self,
        days_before_expiry: i64,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let (credentials, _) = self.sdk.credentials()
            .list(None, None)
            .await?;

        let now = chrono::Utc::now().timestamp();
        let threshold = now + days_before_expiry * 24 * 60 * 60;

        let expiring: Vec<_> = credentials
            .into_iter()
            .filter(|cred| {
                cred.expires_at
                    .as_ref()
                    .and_then(|e| chrono::DateTime::parse_from_rfc3339(e).ok())
                    .map(|dt| dt.timestamp() < threshold)
                    .unwrap_or(false)
            })
            .map(|cred| cred.credential_id)
            .collect();

        println!("Found {} credentials expiring soon", expiring.len());
        Ok(expiring)
    }

    // 安全删除凭证
    async fn delete_credential_secure(
        &self,
        credential_id: &str,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        // 验证凭证存在
        let exists = self.sdk.credentials()
            .exists(credential_id, None)
            .await?;

        if !exists {
            return Err("Credential does not exist".into());
        }

        // 删除凭证
        let result = self.sdk.credentials()
            .delete(credential_id, None)
            .await?;

        if result.deleted {
            println!("Credential {} deleted successfully", credential_id);
        }

        Ok(result.deleted)
    }
}
```

---

## 2. 批量凭证管理

```rust
use futures::future::join_all;

struct BulkCredentialManager {
    sdk: CredBridgeSDK,
}

impl BulkCredentialManager {
    fn new(sdk: CredBridgeSDK) -> Self {
        Self { sdk }
    }

    // 批量创建凭证
    async fn bulk_create(
        &self,
        credentials: Vec<(String, CredentialType, HashMap<String, serde_json::Value>, Option<i64>)>,
    ) -> (Vec<String>, Vec<(usize, Box<dyn std::error::Error>)>) {
        let mut successful = Vec::new();
        let mut failed = Vec::new();

        for (idx, (service_id, cred_type, data, expires)) in credentials.into_iter().enumerate() {
            match self.sdk.credentials()
                .create(&service_id, cred_type, data, expires, None)
                .await {
                Ok(result) => successful.push(result.credential_id),
                Err(e) => failed.push((idx, Box::new(e) as Box<dyn std::error::Error>)),
            }
        }

        (successful, failed)
    }

    // 批量删除凭证
    async fn bulk_delete(
        &self,
        credential_ids: Vec<String>,
    ) -> Vec<(String, Result<bool, CredBridgeError>)> {
        let futures: Vec<_> = credential_ids
            .clone()
            .into_iter()
            .map(|id| async move {
                let result = self.sdk.credentials()
                    .delete(&id, None)
                    .await
                    .map(|r| r.deleted);
                (id, result)
            })
            .collect();

        join_all(futures).await
    }

    // 并发获取多个凭证
    async fn get_multiple(
        &self,
        credential_ids: Vec<String>,
    ) -> Vec<(String, Result<serde_json::Value, CredBridgeError>)> {
        let futures: Vec<_> = credential_ids
            .clone()
            .into_iter()
            .map(|id| async move {
                let result = self.sdk.credentials()
                    .get(&id, None)
                    .await
                    .map(|r| serde_json::to_value(r).unwrap_or_default());
                (id, result)
            })
            .collect();

        join_all(futures).await
    }
}
```

---

## 3. Token 管理

```rust
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};

struct TokenManager {
    sdk: Arc<CredBridgeSDK>,
    refresh_handle: Option<tokio::task::JoinHandle<()>>,
}

impl TokenManager {
    fn new(sdk: CredBridgeSDK) -> Self {
        let sdk = Arc::new(sdk);
        Self {
            sdk,
            refresh_handle: None,
        }
    }

    // 启动 Token 监控
    fn start_monitoring(&mut self) {
        let sdk = Arc::clone(&self.sdk);

        self.refresh_handle = Some(tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(60));

            loop {
                ticker.tick().await;

                if let Err(e) = Self::check_and_refresh(&sdk).await {
                    eprintln!("Token check failed: {}", e);
                }
            }
        }));
    }

    // 停止 Token 监控
    fn stop_monitoring(&mut self) {
        if let Some(handle) = self.refresh_handle.take() {
            handle.abort();
        }
    }

    // 检查并刷新 Token
    async fn check_and_refresh(sdk: &CredBridgeSDK) -> Result<(), Box<dyn std::error::Error>> {
        let token = sdk.token();

        // 检查 Token 是否有效
        if !token.is_valid() {
            return Err("Token is invalid or expired".into());
        }

        // 检查 Token 是否即将过期（10分钟内）
        if token.is_expiring_soon(600) {
            println!("Token expires in {}", token.get_remaining_time_formatted());

            // 验证 Token（向服务器确认）
            let is_valid = token.verify(None).await?;
            if !is_valid {
                return Err("Token has been revoked".into());
            }
        }

        Ok(())
    }

    // 检查权限
    fn check_permissions(&self, required_scopes: &[TokenScope]) -> PermissionCheck {
        let token = self.sdk.token();

        let has_all = token.has_all_scopes(required_scopes);
        let has_any = token.has_any_scope(required_scopes);
        let granted = token.get_scopes();
        let missing: Vec<_> = required_scopes
            .iter()
            .filter(|s| !token.has_scope(**s))
            .cloned()
            .collect();

        PermissionCheck {
            has_all,
            has_any,
            missing,
            granted,
        }
    }
}

struct PermissionCheck {
    has_all: bool,
    has_any: bool,
    missing: Vec<TokenScope>,
    granted: Vec<TokenScope>,
}
```

---

## 4. 错误处理与重试

```rust
use std::time::Duration;
use tokio::time::sleep;

struct SafeCredentialClient {
    sdk: CredBridgeSDK,
}

impl SafeCredentialClient {
    fn new(sdk: CredBridgeSDK) -> Self {
        Self { sdk }
    }

    // 带重试的凭证操作
    async fn with_retry<T, F, Fut>(
        &self,
        operation: F,
        max_retries: u32,
    ) -> Result<T, CredBridgeError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, CredBridgeError>>,
    {
        let mut last_error = None;

        for attempt in 0..=max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = Some(e.clone());

                    // 不可重试的错误直接抛出
                    if !e.is_retryable() {
                        return Err(e);
                    }

                    // 认证错误需要特殊处理
                    if e.is_auth_error() {
                        eprintln!("Authentication error, please check your token");
                        return Err(e);
                    }

                    // 指数退避
                    if attempt < max_retries {
                        let delay = Duration::from_millis(2_u64.pow(attempt) * 1000);
                        println!("Retry {}/{} after {:?}", attempt + 1, max_retries, delay);
                        sleep(delay).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            CredBridgeError::new(CredBridgeErrorCode::Unknown, "Unknown error")
        }))
    }

    // 处理特定错误
    fn handle_credential_error(e: &CredBridgeError) -> ErrorResponse {
        let (error, code) = match e.code {
            CredBridgeErrorCode::NotFound => ("凭证不存在", "NOT_FOUND"),
            CredBridgeErrorCode::Unauthorized => ("未授权访问", "UNAUTHORIZED"),
            CredBridgeErrorCode::Forbidden => ("禁止访问", "FORBIDDEN"),
            CredBridgeErrorCode::InsufficientScope => ("权限不足", "INSUFFICIENT_SCOPE"),
            CredBridgeErrorCode::TokenExpired => ("Token 已过期", "TOKEN_EXPIRED"),
            CredBridgeErrorCode::CredentialExpired => ("凭证已过期", "CREDENTIAL_EXPIRED"),
            CredBridgeErrorCode::NetworkError | CredBridgeErrorCode::Timeout => {
                ("网络错误，请稍后重试", "NETWORK_ERROR")
            }
            _ => (e.message.as_str(), "UNKNOWN"),
        };

        ErrorResponse {
            success: false,
            error: error.to_string(),
            code: code.to_string(),
        }
    }

    // 安全获取凭证
    async fn safe_get_credential(
        &self,
        credential_id: &str,
    ) -> Result<serde_json::Value, ErrorResponse> {
        match self.with_retry(|| self.sdk.credentials().get(credential_id, None), 3).await {
            Ok(credential) => Ok(serde_json::to_value(credential).unwrap_or_default()),
            Err(e) => Err(Self::handle_credential_error(&e)),
        }
    }

    // 安全解密凭证
    async fn safe_decrypt_credential(
        &self,
        credential_id: &str,
        reason: &str,
    ) -> Result<HashMap<String, serde_json::Value>, ErrorResponse> {
        match self.with_retry(|| self.sdk.credentials().decrypt(credential_id, Some(reason), None), 3).await {
            Ok(decrypted) => Ok(decrypted.plaintext_data),
            Err(e) => Err(Self::handle_credential_error(&e)),
        }
    }
}

#[derive(Debug, serde::Serialize)]
struct ErrorResponse {
    success: bool,
    error: String,
    code: String,
}
```

---

## 5. 审计日志

```rust
use chrono::{DateTime, Utc};

struct AuditLogger {
    sdk: Arc<CredBridgeSDK>,
}

impl AuditLogger {
    fn new(sdk: CredBridgeSDK) -> Self {
        Self { sdk: Arc::new(sdk) }
    }

    // 记录凭证访问
    async fn log_credential_access(
        &self,
        credential_id: &str,
        action: CredentialAction,
        success: bool,
        metadata: Option<serde_json::Value>,
    ) {
        println!(
            "[AUDIT] {} credential {}: {} - {:?}",
            action.as_str(),
            credential_id,
            if success { "success" } else { "failed" },
            metadata
        );
    }

    // 获取凭证使用统计
    async fn get_credential_usage_stats(
        &self,
        credential_id: &str,
        days: i64,
    ) -> Result<CredentialUsageStats, Box<dyn std::error::Error>> {
        // 这里可以实现从审计日志服务获取统计信息
        // 这里仅为示例
        Ok(CredentialUsageStats {
            credential_id: credential_id.to_string(),
            period_days: days,
            access_count: 42,
            decrypt_count: 15,
            last_accessed: Utc::now(),
        })
    }
}

enum CredentialAction {
    Read,
    Decrypt,
    Create,
    Delete,
}

impl CredentialAction {
    fn as_str(&self) -> &'static str {
        match self {
            CredentialAction::Read => "read",
            CredentialAction::Decrypt => "decrypt",
            CredentialAction::Create => "create",
            CredentialAction::Delete => "delete",
        }
    }
}

struct CredentialUsageStats {
    credential_id: String,
    period_days: i64,
    access_count: u64,
    decrypt_count: u64,
    last_accessed: DateTime<Utc>,
}
```

---

## 6. Axum Web 集成

```rust
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post, delete},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// 应用状态
#[derive(Clone)]
struct AppState {
    sdk: Arc<CredBridgeSDK>,
}

// 创建凭证请求
#[derive(Deserialize)]
struct CreateCredentialRequest {
    service_id: String,
    credential_type: String,
    data: serde_json::Value,
    expires_at: Option<i64>,
}

// 凭证响应
#[derive(Serialize)]
struct CredentialResponse {
    credential_id: String,
    service_id: String,
    credential_type: String,
    created_at: String,
}

// 创建凭证处理器
async fn create_credential(
    State(state): State<AppState>,
    Json(req): Json<CreateCredentialRequest>,
) -> Result<Json<CredentialResponse>, (StatusCode, String)> {
    let cred_type = parse_credential_type(&req.credential_type)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    let data = match req.data {
        serde_json::Value::Object(map) => map.into_iter().collect(),
        _ => return Err((StatusCode::BAD_REQUEST, "Invalid data format".to_string())),
    };

    let result = state.sdk.credentials()
        .create(req.service_id, cred_type, data, req.expires_at, None)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(CredentialResponse {
        credential_id: result.credential_id,
        service_id: result.service_id,
        credential_type: result.credential_type,
        created_at: result.created_at,
    }))
}

// 获取凭证处理器
async fn get_credential(
    State(state): State<AppState>,
    Path(credential_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let credential = state.sdk.credentials()
        .get(&credential_id, None)
        .await
        .map_err(|e| {
            if e.code == CredBridgeErrorCode::NotFound {
                (StatusCode::NOT_FOUND, "Credential not found".to_string())
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
        })?;

    Ok(Json(serde_json::to_value(credential).unwrap_or_default()))
}

// 解密凭证处理器
async fn decrypt_credential(
    State(state): State<AppState>,
    Path(credential_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let decrypted = state.sdk.credentials()
        .decrypt(&credential_id, Some("API request"), None)
        .await
        .map_err(|e| {
            if e.code == CredBridgeErrorCode::NotFound {
                (StatusCode::NOT_FOUND, "Credential not found".to_string())
            } else if e.code == CredBridgeErrorCode::InsufficientScope {
                (StatusCode::FORBIDDEN, "Insufficient permissions".to_string())
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
        })?;

    Ok(Json(serde_json::to_value(decrypted).unwrap_or_default()))
}

// 删除凭证处理器
async fn delete_credential(
    State(state): State<AppState>,
    Path(credential_id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    state.sdk.credentials()
        .delete(&credential_id, None)
        .await
        .map_err(|e| {
            if e.code == CredBridgeErrorCode::NotFound {
                (StatusCode::NOT_FOUND, "Credential not found".to_string())
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
        })?;

    Ok(StatusCode::NO_CONTENT)
}

// 权限检查中间件
async fn require_scopes(
    state: &AppState,
    scopes: &[TokenScope],
) -> Result<(), (StatusCode, String)> {
    let token = state.sdk.token();

    if !token.has_all_scopes(scopes) {
        return Err((
            StatusCode::FORBIDDEN,
            format!(
                "Required scopes: {:?}, Granted: {:?}",
                scopes,
                token.get_scopes()
            ),
        ));
    }

    Ok(())
}

fn parse_credential_type(s: &str) -> Result<CredentialType, String> {
    match s {
        "username_password" => Ok(CredentialType::UsernamePassword),
        "oauth_refresh" => Ok(CredentialType::OAuthRefresh),
        "api_key" => Ok(CredentialType::ApiKey),
        "session_cookie" => Ok(CredentialType::SessionCookie),
        _ => Err(format!("Unknown credential type: {}", s)),
    }
}

// 创建路由器
fn create_router(sdk: CredBridgeSDK) -> Router {
    let state = AppState {
        sdk: Arc::new(sdk),
    };

    Router::new()
        .route("/credentials", post(create_credential))
        .route("/credentials/:id", get(get_credential).delete(delete_credential))
        .route("/credentials/:id/decrypt", post(decrypt_credential))
        .with_state(state)
}

// 启动服务器示例
/*
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = CredBridgeConfig::new("https://api.credbridge.io")
        .with_token(std::env::var("CREDBRIDGE_TOKEN")?);

    let sdk = CredBridgeSDK::new(config)?;
    let app = create_router(sdk);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}
*/
```

---

## 7. 多租户管理

```rust
use std::collections::HashMap;

struct MultiTenantCredentialManager {
    base_url: String,
    tenant_tokens: HashMap<String, String>,
    clients: RwLock<HashMap<String, Arc<CredBridgeSDK>>>,
}

impl MultiTenantCredentialManager {
    fn new(base_url: String) -> Self {
        Self {
            base_url,
            tenant_tokens: HashMap::new(),
            clients: RwLock::new(HashMap::new()),
        }
    }

    // 注册租户 Token
    fn register_tenant(&mut self, tenant_id: String, token: String) {
        self.tenant_tokens.insert(tenant_id, token);
    }

    // 获取或创建租户客户端
    async fn get_client(&self, tenant_id: &str) -> Result<Arc<CredBridgeSDK>, Box<dyn std::error::Error>> {
        // 先尝试读取
        {
            let clients = self.clients.read().await;
            if let Some(client) = clients.get(tenant_id) {
                return Ok(Arc::clone(client));
            }
        }

        // 需要创建新客户端
        let token = self.tenant_tokens
            .get(tenant_id)
            .ok_or("Tenant not found")?;

        let config = CredBridgeConfig::new(&self.base_url)
            .with_token(token);

        let sdk = Arc::new(CredBridgeSDK::new(config)?);

        // 写入缓存
        let mut clients = self.clients.write().await;
        clients.insert(tenant_id.to_string(), Arc::clone(&sdk));

        Ok(sdk)
    }

    // 跨租户操作
    async fn get_all_credentials(
        &self,
        tenant_ids: Vec<String>,
    ) -> Vec<(String, Result<Vec<serde_json::Value>, String>)> {
        let futures: Vec<_> = tenant_ids
            .clone()
            .into_iter()
            .map(|tenant_id| async move {
                let result = async {
                    let client = self.get_client(&tenant_id).await
                        .map_err(|e| e.to_string())?;

                    let (credentials, _) = client.credentials()
                        .list(None, None)
                        .await
                        .map_err(|e| e.to_string())?;

                    let json_creds: Vec<_> = credentials
                        .into_iter()
                        .map(|c| serde_json::to_value(c).unwrap_or_default())
                        .collect();

                    Ok(json_creds)
                }.await;

                (tenant_id, result)
            })
            .collect();

        join_all(futures).await
    }

    // 租户隔离验证
    async fn validate_tenant_access(
        &self,
        tenant_id: &str,
        credential_id: &str,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let client = self.get_client(tenant_id).await?;

        match client.credentials().get(credential_id, None).await {
            Ok(credential) => Ok(credential.tenant_id == tenant_id),
            Err(_) => Ok(false),
        }
    }
}
```

---

## 8. 自动刷新 Token

```rust
use tokio::sync::Mutex;

struct AutoRefreshTokenClient {
    sdk: Arc<CredBridgeSDK>,
    refresh_callback: Arc<dyn Fn() -> futures::future::BoxFuture<'static, Result<String, Box<dyn std::error::Error + Send>>> + Send + Sync>,
    is_refreshing: Mutex<bool>,
}

impl AutoRefreshTokenClient {
    fn new<F, Fut>(
        sdk: CredBridgeSDK,
        refresh_callback: F,
    ) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<String, Box<dyn std::error::Error + Send>>> + Send + 'static,
    {
        Self {
            sdk: Arc::new(sdk),
            refresh_callback: Arc::new(move || Box::pin(refresh_callback())),
            is_refreshing: Mutex::new(false),
        }
    }

    // 启动自动刷新
    fn start_auto_refresh(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(60));

            loop {
                ticker.tick().await;

                if let Err(e) = self.check_and_refresh().await {
                    eprintln!("Auto refresh failed: {}", e);
                }
            }
        })
    }

    // 检查并刷新 Token
    async fn check_and_refresh(&self) -> Result<(), Box<dyn std::error::Error>> {
        let token = self.sdk.token();

        // 如果 Token 将在 5 分钟内过期，触发刷新
        if token.is_expiring_soon(300) {
            self.refresh_token().await?;
        }

        Ok(())
    }

    // 手动刷新 Token
    async fn refresh_token(&self) -> Result<String, Box<dyn std::error::Error>> {
        // 检查是否正在刷新
        let mut is_refreshing = self.is_refreshing.lock().await;
        if *is_refreshing {
            return Err("Token refresh already in progress".into());
        }
        *is_refreshing = true;
        drop(is_refreshing);

        println!("Refreshing token...");

        let new_token = (self.refresh_callback)().await?;
        self.sdk.client().set_token(&new_token);

        println!("Token refreshed successfully");

        *self.is_refreshing.lock().await = false;

        Ok(new_token)
    }
}

// 使用示例
/*
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = CredBridgeConfig::new("https://api.credbridge.io")
        .with_token(initial_token);

    let sdk = CredBridgeSDK::new(config)?;

    let auto_refresh = Arc::new(AutoRefreshTokenClient::new(
        sdk,
        || async {
            // 调用 Token 刷新端点
            let client = reqwest::Client::new();
            let response = client
                .post("https://api.credbridge.io/api/v1/tokens/refresh")
                .bearer_auth(current_token)
                .send()
                .await?;

            let json: serde_json::Value = response.json().await?;
            Ok(json["token"].as_str().unwrap_or_default().to_string())
        }
    ));

    // 启动自动刷新
    let handle = auto_refresh.clone().start_auto_refresh();

    // ... 你的业务逻辑 ...

    // 清理时停止自动刷新
    handle.abort();

    Ok(())
}
*/
```

---

## 9. 凭证轮换

```rust
struct CredentialRotator {
    sdk: CredBridgeSDK,
}

impl CredentialRotator {
    fn new(sdk: CredBridgeSDK) -> Self {
        Self { sdk }
    }

    // 轮换单个凭证
    async fn rotate_credential(
        &self,
        credential_id: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // 获取旧凭证
        let old = self.sdk.credentials()
            .get(credential_id, None)
            .await?;

        // 解密旧凭证
        let decrypted = self.sdk.credentials()
            .decrypt(credential_id, Some("Credential rotation"), None)
            .await?;

        // 创建新凭证
        let new_cred = self.sdk.credentials()
            .create(
                &old.service_id,
                old.credential_type.parse()?,
                decrypted.plaintext_data,
                Some(chrono::Utc::now().timestamp() + 86400 * 90), // 新凭证90天过期
                None,
            )
            .await?;

        // 删除旧凭证
        self.sdk.credentials()
            .delete(credential_id, None)
            .await?;

        println!(
            "Rotated credential {} -> {}",
            credential_id,
            new_cred.credential_id
        );

        Ok(new_cred.credential_id)
    }

    // 按服务轮换所有凭证
    async fn rotate_service_credentials(
        &self,
        service_id: &str,
    ) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
        let (credentials, _) = self.sdk.credentials()
            .get_by_service(service_id, None)
            .await?;

        let mut rotated = Vec::new();

        for cred in credentials {
            match self.rotate_credential(&cred.credential_id).await {
                Ok(new_id) => {
                    rotated.push((cred.credential_id, new_id));
                }
                Err(e) => {
                    eprintln!("Failed to rotate {}: {}", cred.credential_id, e);
                }
            }
        }

        Ok(rotated)
    }

    // 自动轮换即将过期的凭证
    async fn auto_rotate_expiring(
        &self,
        days_before_expiry: i64,
    ) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
        let (credentials, _) = self.sdk.credentials()
            .list(None, None)
            .await?;

        let now = chrono::Utc::now().timestamp();
        let threshold = now + days_before_expiry * 24 * 60 * 60;

        let mut rotated = Vec::new();

        for cred in credentials {
            let should_rotate = cred.expires_at
                .as_ref()
                .and_then(|e| chrono::DateTime::parse_from_rfc3339(e).ok())
                .map(|dt| dt.timestamp() < threshold)
                .unwrap_or(false);

            if should_rotate {
                match self.rotate_credential(&cred.credential_id).await {
                    Ok(new_id) => {
                        rotated.push((cred.credential_id, new_id));
                    }
                    Err(e) => {
                        eprintln!("Failed to rotate {}: {}", cred.credential_id, e);
                    }
                }
            }
        }

        println!("Auto-rotated {} credentials", rotated.len());
        Ok(rotated)
    }
}
```
