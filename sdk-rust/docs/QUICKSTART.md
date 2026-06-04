# Toani Vault Rust SDK - 快速入门

> **迁移注意**: 此 crate 已从 `credbridge-sdk` 重命名为 `toani-vault-sdk`。`CredBridgeSDK` 已被弃用，请使用 `ToaniVaultSDK`。

## 安装

在 `Cargo.toml` 中添加依赖：

```toml
[dependencies]
toani-vault-sdk = "0.1.0"
tokio = { version = "1", features = ["full"] }
```

## 初始化 SDK

```rust
use toani_vault_sdk::{CredBridgeConfig, ToaniVaultSDK};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建配置
    let config = CredBridgeConfig::new("https://api.toani.io")
        .with_token("your-api-token")  // PASETO v4.local Token
        .with_timeout_ms(30000);         // 请求超时时间（毫秒）

    // 创建 SDK 实例
    let sdk = ToaniVaultSDK::new(config)?;

    Ok(())
}
```

## 基础使用

### 创建凭证

```rust
use toani_vault_sdk::types::{CredentialType, RequestOptions};
use serde_json::json;
use std::collections::HashMap;

// 创建用户名密码凭证
async fn create_credential(sdk: &ToaniVaultSDK) -> Result<String, Box<dyn std::error::Error>> {
    let mut plaintext_data = HashMap::new();
    plaintext_data.insert("username".to_string(), json!("user@example.com"));
    plaintext_data.insert("password".to_string(), json!("secret_password"));

    let credential = sdk.credentials()
        .create(
            "schwab",
            CredentialType::UsernamePassword,
            plaintext_data,
            Some(chrono::Utc::now().timestamp() + 86400 * 30), // 30天后过期
            None,
        )
        .await?;

    println!("Created: {}", credential.credential_id);
    Ok(credential.credential_id)
}
```

### 快捷创建方法

```rust
use toani_vault_sdk::types::{CredentialProvider, CredentialType};
use toani_vault_sdk::{CreateCredentialRequest, CredentialCustomFunction};
use serde_json::json;
use std::collections::HashMap;

// 创建用户名密码凭证
let cred1 = sdk.credentials()
    .create_username_password(
        "schwab",
        "user@example.com",
        "secret_password",
        None,  // 过期时间
        None,  // 请求选项
    )
    .await?;

// 创建 API Key 凭证
let cred2 = sdk.credentials()
    .create_api_key(
        "stripe",
        "sk_live_...",
        Some("sk_secret_..."),  // 可选
        None,
        None,
    )
    .await?;

// 创建 OAuth 刷新令牌
let cred3 = sdk.credentials()
    .create_oauth_refresh(
        "google",
        "1//0d...",
        None,
        None,
    )
    .await?;

// 创建带 provider / allowed_domains / custom_functions 的 API Key 凭证
let mut exchange_plaintext = HashMap::new();
exchange_plaintext.insert("api_key".to_string(), json!("binance-api-key"));
exchange_plaintext.insert("secret_key".to_string(), json!("binance-secret-key"));

let cred4 = sdk.credentials()
    .create_with_request(
        CreateCredentialRequest {
            service_id: "binance-trading".to_string(),
            credential_type: CredentialType::ApiKey,
            plaintext_data: exchange_plaintext,
            expires_at: None,
            provider: Some(CredentialProvider::Binance),
            allowed_domains: vec!["api.binance.com:443".to_string()],
            custom_functions: vec![CredentialCustomFunction {
                function_name: "stable_recv_window".to_string(),
                function_description: Some("Returns a fixed recvWindow value".to_string()),
                function_body: "export default function func() { return \"5000\"; }".to_string(),
            }],
        },
        None,
    )
    .await?;
```

### Sandbox `http_request` 模板

```rust
use serde_json::json;
use std::collections::HashMap;
use toani_vault_sdk::{
    ExecuteSandboxOperationRequest, SandboxOperationType,
};

let okx_request = ExecuteSandboxOperationRequest {
    operation_type: SandboxOperationType::HttpRequest,
    description: "GET OKX balance".to_string(),
    parameters: HashMap::from([
        ("method".to_string(), json!("GET")),
        (
            "url".to_string(),
            json!("https://www.okx.com/api/v5/account/balance"),
        ),
        (
            "headers".to_string(),
            json!({
                "OK-ACCESS-KEY": "${credential.api_key}",
                "OK-ACCESS-TIMESTAMP": "${functions.okx_timestamp()}",
                "OK-ACCESS-PASSPHRASE": "${credential.passphrase}",
                "OK-ACCESS-SIGN": "${functions.okx_sign()}",
            }),
        ),
    ]),
};

let _okx_response = sdk.sandbox()
    .request(okx_request, None)
    .await?;

let binance_request = ExecuteSandboxOperationRequest {
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

### 获取凭证列表

```rust
use toani_vault_sdk::types::CredentialFilter;

// 获取所有凭证
let (credentials, total) = sdk.credentials().list(None, None).await?;
println!("Total: {}", total);

// 按服务过滤
let filter = CredentialFilter {
    service_id: Some("schwab".to_string()),
    ..Default::default()
};
let (credentials, _) = sdk.credentials().list(Some(filter), None).await?;

// 按类型过滤
let filter = CredentialFilter {
    credential_type: Some(CredentialType::ApiKey),
    ..Default::default()
};
let (api_keys, _) = sdk.credentials().list(Some(filter), None).await?;
```

### 获取和解密凭证

```rust
// 获取凭证详情（不包含明文）
let credential = sdk.credentials()
    .get("credential-id", None)
    .await?;

// 解密凭证
let decrypted = sdk.credentials()
    .decrypt("credential-id", Some("用户登录操作"), None)
    .await?;

println!("Username: {}", decrypted.plaintext_data.get("username").unwrap());
println!("Password: {}", decrypted.plaintext_data.get("password").unwrap());
```

### 删除凭证

```rust
let result = sdk.credentials()
    .delete("credential-id", None)
    .await?;

if result.deleted {
    println!("删除成功");
}
```

## Token 管理

```rust
use toani_vault_sdk::types::TokenScope;

// 获取当前 Token 信息
let token_info = sdk.token().get_token_info();
if let Some(info) = token_info {
    println!("Tenant: {}", info.tenant_id);
    println!("User: {}", info.user_id);
    println!("Scopes: {:?}", info.scopes);
}

// 检查 Token 权限
if sdk.token().has_scope(TokenScope::CredentialRead) {
    println!("有读取权限");
}

// 检查 Token 是否即将过期
if sdk.token().is_expiring_soon(300) {  // 5分钟内
    println!("Token 即将过期");
}

// 获取剩余时间
let remaining = sdk.token().get_remaining_time();
println!("剩余时间: {}", sdk.token().get_remaining_time_formatted());

// 验证 Token（向服务器确认）
let is_valid = sdk.token().verify(None).await?;
```

## 错误处理

```rust
use toani_vault_sdk::types::{CredBridgeError, CredBridgeErrorCode};

match sdk.credentials().get("invalid-id", None).await {
    Ok(credential) => {
        println!("Found: {:?}", credential);
    }
    Err(e) => {
        match e.code {
            CredBridgeErrorCode::NotFound => {
                println!("凭证不存在");
            }
            CredBridgeErrorCode::Unauthorized => {
                println!("未授权");
            }
            CredBridgeErrorCode::Forbidden => {
                println!("禁止访问");
            }
            _ => {
                println!("错误: {}", e);
            }
        }
    }
}
```

## 请求选项

```rust
use toani_vault_sdk::types::RequestOptions;

// 自定义请求选项
let options = RequestOptions::new()
    .with_timeout_ms(10000)      // 单次请求超时
    .with_skip_retry(true)        // 跳过重试
    .with_retries(5)              // 自定义重试次数
    .with_header("X-Request-Source", "mobile-app");

let credential = sdk.credentials()
    .get("credential-id", Some(options))
    .await?;
```

## 完整示例

```rust
use toani_vault_sdk::{CredBridgeConfig, ToaniVaultSDK};
use toani_vault_sdk::types::{CredentialType, RequestOptions};
use serde_json::json;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化 SDK
    let config = CredBridgeConfig::new("https://api.toani.io")
        .with_token("your-api-token");

    let sdk = ToaniVaultSDK::new(config)?;

    // 创建凭证
    let mut data = HashMap::new();
    data.insert("username".to_string(), json!("user@example.com"));
    data.insert("password".to_string(), json!("secure_password"));

    let credential = sdk.credentials()
        .create(
            "schwab",
            CredentialType::UsernamePassword,
            data,
            None,
            None,
        )
        .await?;

    println!("Created credential: {}", credential.credential_id);

    // 解密凭证
    let decrypted = sdk.credentials()
        .decrypt(&credential.credential_id, Some("用户登录"), None)
        .await?;

    println!("Username: {}", decrypted.plaintext_data.get("username").unwrap());

    // 删除凭证
    sdk.credentials()
        .delete(&credential.credential_id, None)
        .await?;

    println!("Credential deleted successfully");

    Ok(())
}
```

## 下一步

- 查看 [API 参考](./API_REFERENCE.md) 了解完整的 API 文档
- 查看 [高级示例](./EXAMPLES.md) 学习更多使用场景
