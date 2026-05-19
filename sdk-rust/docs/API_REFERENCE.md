# Toani Vault Rust SDK - API 参考

> **迁移注意**: 此 crate 已从 `credbridge-sdk` 重命名为 `toani-vault-sdk`。`CredBridgeSDK` 已被弃用，请使用 `ToaniVaultSDK`。`CredBridgeSDK` 仍可作为 `ToaniVaultSDK` 的类型别名使用。

## 目录

- [ToaniVaultSDK](#toanivaultsdk)
- [CredBridgeClient](#credbridgeclient)
- [CredentialsService](#credentialsservice)
- [TokenManager](#tokenmanager)
- [类型定义](#类型定义)
- [错误处理](#错误处理)

---

## ToaniVaultSDK

SDK 主入口，提供便捷的方法访问各种服务。

### 方法

#### new(config)

创建新的 SDK 实例。

```rust
pub fn new(config: CredBridgeConfig) -> Result<Self>
```

#### from_client(client)

从现有客户端创建 SDK 实例。

```rust
pub fn from_client(client: Arc<CredBridgeClient>) -> Self
```

#### client()

获取原始客户端。

```rust
pub fn client(&self) -> &CredBridgeClient
```

#### credentials()

获取凭证管理服务。

```rust
pub fn credentials(&self) -> CredentialsService
```

#### token()

获取 Token 管理器。

```rust
pub fn token(&self) -> TokenManager
```

#### version()

获取 SDK 版本。

```rust
pub fn version() -> &'static str
```

---

## CredBridgeClient

HTTP 客户端，实现请求、错误重试、Token 管理等功能。

### 方法

#### new(config)

创建新的客户端。

```rust
pub fn new(config: CredBridgeConfig) -> Result<Self>
```

#### get_config()

获取当前配置。

```rust
pub fn get_config(&self) -> &CredBridgeConfig
```

#### set_token(new_token)

更新 Token。

```rust
pub fn set_token(&self, new_token: impl Into<String>)
```

#### get_token()

获取当前 Token。

```rust
pub fn get_token(&self) -> Option<String>
```

#### get_token_info()

获取 Token 信息。

```rust
pub fn get_token_info(&self) -> Option<TokenInfo>
```

#### is_token_expiring_soon()

检查 Token 是否即将过期。

```rust
pub fn is_token_expiring_soon(&self) -> bool
```

#### is_token_expired()

检查 Token 是否已过期。

```rust
pub fn is_token_expired(&self) -> bool
```

#### HTTP 方法

```rust
// GET 请求
pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T>

// GET 请求（带选项）
pub async fn get_with_options<T: DeserializeOwned>(
    &self,
    path: &str,
    options: Option<RequestOptions>
) -> Result<T>

// POST 请求
pub async fn post<T: DeserializeOwned>(
    &self,
    path: &str,
    body: impl serde::Serialize
) -> Result<T>

// POST 请求（带选项）
pub async fn post_with_options<T: DeserializeOwned>(
    &self,
    path: &str,
    body: impl serde::Serialize,
    options: Option<RequestOptions>
) -> Result<T>

// PUT 请求
pub async fn put<T: DeserializeOwned>(
    &self,
    path: &str,
    body: impl serde::Serialize
) -> Result<T>

// DELETE 请求
pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T>

// DELETE 请求（带选项）
pub async fn delete_with_options<T: DeserializeOwned>(
    &self,
    path: &str,
    options: Option<RequestOptions>
) -> Result<T>

// PATCH 请求
pub async fn patch<T: DeserializeOwned>(
    &self,
    path: &str,
    body: impl serde::Serialize
) -> Result<T>
```

---

## CredentialsService

凭证管理服务，提供凭证的 CRUD 操作和解密功能。

### 方法

#### new(client)

创建新的凭证管理服务。

```rust
pub fn new(client: Arc<CredBridgeClient>) -> Self
```

#### create(service_id, credential_type, plaintext_data, expires_at, options)

创建新凭证。

```rust
pub async fn create(
    &self,
    service_id: impl Into<String>,
    credential_type: CredentialType,
    plaintext_data: HashMap<String, Value>,
    expires_at: Option<i64>,
    options: Option<RequestOptions>
) -> Result<CreateCredentialResponse>
```

**CreateCredentialResponse:**

| 字段               | 类型                               | 描述                  |
| ------------------ | ---------------------------------- | --------------------- |
| `credential_id`    | `String`                           | 凭证 ID               |
| `service_id`       | `String`                           | 服务 ID               |
| `credential_type`  | `String`                           | 凭证类型              |
| `created_at`       | `String`                           | 创建时间              |
| `expires_at`       | `Option<String>`                   | 过期时间              |
| `provider`         | `Option<CredentialProvider>`       | 交易所 / 自定义类型   |
| `allowed_domains`  | `Vec<String>`                      | `http_request` 白名单 |
| `custom_functions` | `Vec<CredentialCustomFunction>`    | 自定义模板函数        |

#### create_with_request(request, options)

创建带 `provider` / `allowed_domains` / `custom_functions` 的凭证。

```rust
pub async fn create_with_request(
    &self,
    request: CreateCredentialRequest,
    options: Option<RequestOptions>
) -> Result<CreateCredentialResponse>
```

**CreateCredentialRequest:**

| 字段               | 类型                            | 描述                  |
| ------------------ | ------------------------------- | --------------------- |
| `service_id`       | `String`                        | 服务 ID               |
| `credential_type`  | `CredentialType`               | 凭证类型              |
| `plaintext_data`   | `HashMap<String, Value>`       | 敏感字段明文          |
| `expires_at`       | `Option<i64>`                  | 过期时间              |
| `provider`         | `Option<CredentialProvider>`   | `okx` / `binance` / `custom` |
| `allowed_domains`  | `Vec<String>`                  | `sandbox http_request` 域名白名单 |
| `custom_functions` | `Vec<CredentialCustomFunction>`| 自定义模板函数        |

#### create_username_password(service_id, username, password, expires_at, options)

创建用户名密码凭证（快捷方法）。

```rust
pub async fn create_username_password(
    &self,
    service_id: impl Into<String>,
    username: impl Into<String>,
    password: impl Into<String>,
    expires_at: Option<i64>,
    options: Option<RequestOptions>
) -> Result<CreateCredentialResponse>
```

#### create_api_key(service_id, api_key, api_secret, expires_at, options)

创建 API Key 凭证（快捷方法）。

```rust
pub async fn create_api_key(
    &self,
    service_id: impl Into<String>,
    api_key: impl Into<String>,
    api_secret: Option<impl Into<String>>,
    expires_at: Option<i64>,
    options: Option<RequestOptions>
) -> Result<CreateCredentialResponse>
```

说明：

- 快捷方法会把 `api_key` 写入 `plaintext_data.api_key`
- 如果提供第三个参数，也会同时写入 `plaintext_data.api_secret`
- 对 OKX / Binance 这类需要 `secret_key`、`passphrase`、白名单域名或模板函数的场景，请使用 `create_with_request`

#### create_oauth_refresh(service_id, refresh_token, expires_at, options)

创建 OAuth 刷新令牌凭证（快捷方法）。

```rust
pub async fn create_oauth_refresh(
    &self,
    service_id: impl Into<String>,
    refresh_token: impl Into<String>,
    expires_at: Option<i64>,
    options: Option<RequestOptions>
) -> Result<CreateCredentialResponse>
```

#### list(filter, options)

获取凭证列表。

```rust
pub async fn list(
    &self,
    filter: Option<CredentialFilter>,
    options: Option<RequestOptions>
) -> Result<(Vec<CredentialMetadata>, u32)>
```

**CredentialFilter:**

| 字段              | 类型                     | 描述               |
| ----------------- | ------------------------ | ------------------ |
| `service_id`      | `Option<String>`         | 按服务 ID 过滤     |
| `credential_type` | `Option<CredentialType>` | 按凭证类型过滤     |
| `include_deleted` | `Option<bool>`           | 包含已删除的凭证   |
| `only_valid`      | `Option<bool>`           | 仅返回未过期的凭证 |

**CredentialMetadata:**

| 字段               | 类型                            | 描述                  |
| ------------------ | ------------------------------- | --------------------- |
| `credential_id`    | `String`                        | 凭证 ID               |
| `credential_type`  | `CredentialType`               | 凭证类型              |
| `user_id_hash`     | `String`                        | 用户 ID 哈希          |
| `service_id`       | `String`                        | 服务 ID               |
| `tenant_id`        | `String`                        | 租户 ID               |
| `created_at`       | `String`                        | 创建时间              |
| `expires_at`       | `Option<String>`                | 过期时间              |
| `is_deleted`       | `bool`                          | 是否已删除            |
| `provider`         | `Option<CredentialProvider>`    | 交易所 / 自定义类型   |
| `allowed_domains`  | `Vec<String>`                   | `http_request` 白名单 |
| `custom_functions` | `Vec<CredentialCustomFunction>` | 自定义模板函数        |

#### get(credential_id, options)

获取单个凭证详情。

```rust
pub async fn get(
    &self,
    credential_id: impl AsRef<str>,
    options: Option<RequestOptions>
) -> Result<GetCredentialResponse>
```

`GetCredentialResponse` 额外包含 `provider`、`allowed_domains`、`custom_functions` 字段。

#### update_with_request(credential_id, request, options)

更新凭证并同时更新 `provider` / `allowed_domains` / `custom_functions`。

```rust
pub async fn update_with_request(
    &self,
    credential_id: impl AsRef<str>,
    request: UpdateCredentialRequest,
    options: Option<RequestOptions>
) -> Result<UpdateCredentialResponse>
```

`UpdateCredentialRequest` 新增字段：

| 字段 | 类型 | 描述 |
| --- | --- | --- |
| `provider` | `Option<Option<CredentialProvider>>` | `None` 表示不更新，`Some(None)` 表示清空 |
| `allowed_domains` | `Option<Vec<String>>` | 替换域名白名单 |
| `custom_functions` | `Option<Vec<CredentialCustomFunction>>` | 替换自定义函数 |

`UpdateCredentialResponse` 额外返回 `provider`、`allowed_domains`、`custom_functions`。

#### decrypt(credential_id, reason, options)

解密凭证。

```rust
pub async fn decrypt(
    &self,
    credential_id: impl AsRef<str>,
    reason: Option<impl Into<String>>,
    options: Option<RequestOptions>
) -> Result<DecryptCredentialResponse>
```

**DecryptCredentialResponse:**

| 字段              | 类型                     | 描述           |
| ----------------- | ------------------------ | -------------- |
| `credential_id`   | `String`                 | 凭证 ID        |
| `service_id`      | `String`                 | 服务 ID        |
| `credential_type` | `String`                 | 凭证类型       |
| `plaintext_data`  | `HashMap<String, Value>` | 解密的明文数据 |

#### Sandbox `http_request` 模板

`ExecuteSandboxOperationRequest` 的 `parameters` 可以直接传模板：

```rust
use serde_json::json;
use std::collections::HashMap;
use toani_vault_sdk::{ExecuteSandboxOperationRequest, SandboxOperationType};

let request = ExecuteSandboxOperationRequest {
    operation_type: SandboxOperationType::HttpRequest,
    description: "GET Binance account".to_string(),
    parameters: HashMap::from([
        ("method".to_string(), json!("GET")),
        (
            "url".to_string(),
            json!("https://api.binance.com/api/v3/account"),
        ),
        (
            "query".to_string(),
            json!({
                "timestamp": "${functions.binance_timestamp()}",
                "recvWindow": "5000",
                "signature": "${functions.binance_sign()}",
            }),
        ),
        (
            "headers".to_string(),
            json!({
                "X-MBX-APIKEY": "${credential.api_key}",
            }),
        ),
    ]),
};
```

#### delete(credential_id, options)

删除凭证。

```rust
pub async fn delete(
    &self,
    credential_id: impl AsRef<str>,
    options: Option<RequestOptions>
) -> Result<DeleteCredentialResponse>
```

#### get_by_service(service_id, options)

获取指定服务的所有凭证。

```rust
pub async fn get_by_service(
    &self,
    service_id: impl Into<String>,
    options: Option<RequestOptions>
) -> Result<(Vec<CredentialMetadata>, u32)>
```

#### get_by_type(credential_type, options)

获取指定类型的所有凭证。

```rust
pub async fn get_by_type(
    &self,
    credential_type: CredentialType,
    options: Option<RequestOptions>
) -> Result<(Vec<CredentialMetadata>, u32)>
```

#### exists(credential_id, options)

检查凭证是否存在。

```rust
pub async fn exists(
    &self,
    credential_id: impl AsRef<str>,
    options: Option<RequestOptions>
) -> Result<bool>
```

---

## TokenManager

Token 管理器，提供 Token 验证、刷新和管理功能。

### 方法

#### new(client)

创建新的 Token 管理器。

```rust
pub fn new(client: Arc<CredBridgeClient>) -> Self
```

#### get_token_info()

获取当前 Token 信息。

```rust
pub fn get_token_info(&self) -> Option<TokenInfo>
```

**TokenInfo:**

| 字段         | 类型              | 描述                    |
| ------------ | ----------------- | ----------------------- |
| `token_id`   | `String`          | Token ID                |
| `subject`    | `String`          | 主题（租户ID:用户ID）   |
| `tenant_id`  | `String`          | 租户 ID                 |
| `user_id`    | `String`          | 用户 ID                 |
| `expires_at` | `i64`             | 过期时间（Unix 时间戳） |
| `scopes`     | `Vec<TokenScope>` | 授权 Scope 列表         |
| `issued_at`  | `i64`             | 颁发时间                |

#### get_token()

获取当前 Token。

```rust
pub fn get_token(&self) -> Option<String>
```

#### set_token(token)

设置新的 Token。

```rust
pub fn set_token(&self, token: impl Into<String>)
```

#### is_valid()

检查 Token 是否有效。

```rust
pub fn is_valid(&self) -> bool
```

#### is_expiring_soon(buffer_seconds)

检查 Token 是否即将过期。

```rust
pub fn is_expiring_soon(&self, buffer_seconds: i64) -> bool
```

- `buffer_seconds`: 过期前缓冲时间（秒）

#### get_remaining_time()

获取 Token 剩余有效时间。

```rust
pub fn get_remaining_time(&self) -> i64
```

返回剩余秒数（如果 Token 无效则返回 0）。

#### get_remaining_time_formatted()

获取 Token 剩余有效时间的友好显示字符串。

```rust
pub fn get_remaining_time_formatted(&self) -> String
```

返回格式如："5分钟", "2小时", "3天", "已过期"

#### verify(options)

验证当前 Token（向服务器确认）。

```rust
pub async fn verify(&self, options: Option<RequestOptions>) -> Result<bool>
```

#### revoke(options)

撤销当前 Token。

```rust
pub async fn revoke(&self, options: Option<RequestOptions>) -> Result<bool>
```

#### has_scope(scope)

检查 Token 是否具有指定的 Scope。

```rust
pub fn has_scope(&self, scope: TokenScope) -> bool
```

#### has_any_scope(scopes)

检查 Token 是否具有指定的任一 Scope。

```rust
pub fn has_any_scope(&self, scopes: &[TokenScope]) -> bool
```

#### has_all_scopes(scopes)

检查 Token 是否具有所有指定的 Scope。

```rust
pub fn has_all_scopes(&self, scopes: &[TokenScope]) -> bool
```

#### get_scopes()

获取 Token 中的所有 Scope。

```rust
pub fn get_scopes(&self) -> Vec<TokenScope>
```

#### get_tenant_id()

获取租户 ID。

```rust
pub fn get_tenant_id(&self) -> Option<String>
```

#### get_user_id()

获取用户 ID。

```rust
pub fn get_user_id(&self) -> Option<String>
```

#### get_token_id()

获取 Token ID。

```rust
pub fn get_token_id(&self) -> Option<String>
```

#### get_issued_at()

获取 Token 颁发时间。

```rust
pub fn get_issued_at(&self) -> Option<i64>
```

#### get_expires_at()

获取 Token 过期时间。

```rust
pub fn get_expires_at(&self) -> Option<i64>
```

---

## 类型定义

### CredentialType 枚举

```rust
pub enum CredentialType {
    UsernamePassword,
    OAuthRefresh,
    ApiKey,
    SessionCookie,
    Certificate,
    SshKey,
    DatabaseConnection,
}
```

### TokenScope 枚举

```rust
pub enum TokenScope {
    CredentialRead,
    CredentialDecrypt,
    CredentialWrite,
    AuditRead,
    Admin,
}
```

### CredBridgeConfig 结构体

```rust
pub struct CredBridgeConfig {
    pub base_url: String,
    pub token: Option<String>,
    pub tenant_id: Option<String>,
    pub user_id: Option<String>,
    pub timeout_ms: u64,
    pub max_retries: u32,
    pub auto_refresh_token: bool,
    pub token_refresh_buffer_ms: u64,
    pub headers: HashMap<String, String>,
    pub signing_key: Option<String>,
}
```

### RequestOptions 结构体

```rust
pub struct RequestOptions {
    pub timeout_ms: Option<u64>,
    pub skip_retry: bool,
    pub retries: Option<u32>,
    pub headers: HashMap<String, String>,
    pub request_id: Option<String>,
}
```

**方法：**

```rust
impl RequestOptions {
    pub fn new() -> Self
    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self
    pub fn with_skip_retry(mut self, skip_retry: bool) -> Self
    pub fn with_retries(mut self, retries: u32) -> Self
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self
}
```

---

## 错误处理

### CredBridgeError 结构体

```rust
#[derive(Debug, Error, Clone)]
#[error("CredBridge error [{code}]: {message}")]
pub struct CredBridgeError {
    pub code: CredBridgeErrorCode,
    pub message: String,
    pub status_code: Option<u16>,
    pub details: Option<HashMap<String, serde_json::Value>>,
    pub request_id: Option<String>,
}
```

**方法：**

```rust
impl CredBridgeError {
    pub fn new(code: CredBridgeErrorCode, message: impl Into<String>) -> Self
    pub fn with_status_code(mut self, status_code: u16) -> Self
    pub fn with_details(mut self, details: HashMap<String, serde_json::Value>) -> Self
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self
    pub fn is_network_error(&self) -> bool
    pub fn is_auth_error(&self) -> bool
    pub fn is_retryable(&self) -> bool
}
```

### CredBridgeErrorCode 枚举

```rust
pub enum CredBridgeErrorCode {
    Unknown,
    NetworkError,
    Timeout,
    Unauthorized,
    Forbidden,
    NotFound,
    InvalidRequest,
    InternalError,
    TokenExpired,
    InvalidToken,
    TokenRevoked,
    InsufficientScope,
    TenantIsolationViolation,
    CredentialExpired,
    DecryptionFailed,
    EncryptionFailed,
}
```

### Result 类型

```rust
pub type Result<T> = std::result::Result<T, CredBridgeError>;
```

---

## 常量

```rust
// SDK 版本
pub const VERSION: &str;
```

---

## 辅助函数

### create_client(config)

创建新的 CredBridge 客户端。

```rust
pub fn create_client(config: CredBridgeConfig) -> Result<CredBridgeClient>
```

### version()

获取 SDK 版本。

```rust
pub fn version() -> &'static str
```
