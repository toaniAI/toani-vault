# Toani Vault Rust SDK

> **迁移注意**: 此 crate 已从 `credbridge-sdk` 重命名为 `toani-vault-sdk`。`CredBridgeSDK` 已被弃用，请使用 `ToaniVaultSDK`。`CredBridgeSDK` 仍可作为 `ToaniVaultSDK` 的类型别名使用，以保持向后兼容性。

用于与 Toani Vault API 交互的 Rust SDK。

## 认证模式

> **重要**: 此 SDK 支持两种认证模式，请根据使用场景选择正确的方式。

### 认证模式对比

| 认证类型 | 适用场景 | 认证方式 |
|---------|---------|---------|
| **用户认证** | 最终用户访问 | Web 界面 Privy 钱包登录 |
| **服务账户认证** | 自动化、CI/CD、后台服务 | Platform API Token (此 SDK) |

### 用户认证 (Privy 钱包)

用户认证通过 Web 界面完成，使用 Privy 钱包登录：

1. 访问 https://vault.toani.io
2. 点击"使用钱包登录"
3. 通过 Privy 支持的钱包（如 MetaMask、Phantom）完成认证
4. 认证成功后获得用户 Session Token（仅表示登录态）

**注意**: `Privy Access Token` 仅用于换取 `Session Token`，不是 API 调用 token。

### 服务账户认证 (Platform API Token)

此 SDK 用于服务账户认证，适用于：

- CI/CD 管道自动化
- 后台服务/微服务
- 管理脚本和自动化工具
- 跨系统集成

使用 CLI 登录服务账户：

```bash
# 登录服务账户（需要 Platform API Token）
toani auth login --url https://vault.toani.io --token <your-platform-token> --service-account
```

或在代码中直接设置：

```rust
use toani_vault_sdk::{CredBridgeConfig, ToaniVaultSDK};

// 服务账户认证 - 使用 Platform API Token
let sdk = ToaniVaultSDK::new(
    CredBridgeConfig::new("https://vault.toani.io")
        .with_token("v4.local.your-platform-api-token")
)?;
```

### 如何获取 Platform API Token

Platform API Token 需通过管理界面或 API 创建：

1. 使用管理员账户登录 Web 界面
2. 进入"开发者中心" > "API Tokens"
3. 创建新的服务账户 Token，设置所需权限范围

### Profile Automation Tokens

也可以先拿用户 Session Token，再为当前租户签发一个用户自动化令牌：

```rust,no_run
use std::collections::HashMap;
use toani_vault_sdk::{CreateAutomationTokenRequest, RequestOptions, ToaniVaultSDK};

# async fn example(sdk: ToaniVaultSDK, session_token: String) -> Result<(), Box<dyn std::error::Error>> {
let mut headers = HashMap::new();
headers.insert("Authorization".to_string(), format!("Bearer {}", session_token));

let issued = sdk.token().create_automation_token(
    CreateAutomationTokenRequest {
        name: "ci-bot".to_string(),
        description: Some("nightly credential sync".to_string()),
        scopes: vec!["credential:read".to_string(), "audit:read".to_string()],
        ttl_seconds: Some(86_400),
        created_via: Some("sdk".to_string()),
    },
    Some(RequestOptions {
        headers: Some(headers),
        ..Default::default()
    }),
).await?;

sdk.client().set_token(issued.token_value);
# Ok(())
# }
```

---

## 特性

- **完整的凭证管理**: 创建、读取、更新、删除、解密凭证
- **Token 管理**: 自动 Token 刷新、Scope 检查、过期验证
- **类型安全**: 完整的 Rust 类型定义，编译时类型检查
- **错误处理**: 详细的错误信息，支持重试和恢复
- **异步支持**: 基于 `tokio` 的全异步 API
- **请求签名**: 支持请求签名验证

## 安装

在 `Cargo.toml` 中添加依赖：

```toml
[dependencies]
toani-vault-sdk = "0.1.0"
tokio = { version = "1.0", features = ["full"] }
```

## 快速开始

### 基本使用

```rust
use toani_vault_sdk::{CredBridgeConfig, ToaniVaultSDK, types::CredentialType};
use std::collections::HashMap;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建 SDK 实例
    let sdk = ToaniVaultSDK::new(
        CredBridgeConfig::new("https://api.toani.io")
            .with_token("your-api-token")
            .with_timeout_ms(30000)
    )?;

    // 创建凭证
    let mut data = HashMap::new();
    data.insert("username".to_string(), json!("user@example.com"));
    data.insert("password".to_string(), json!("secret_password"));

    let credential = sdk.credentials()
        .create("schwab", CredentialType::UsernamePassword, data, None, None)
        .await?;

    println!("Created credential: {}", credential.credential_id);

    // 解密凭证
    let decrypted = sdk.credentials()
        .decrypt(&credential.credential_id, Some("用户登录"), None)
        .await?;

    println!("Username: {}", decrypted.plaintext_data.get("username").unwrap());

    Ok(())
}
```

### 使用快捷方法创建凭证

```rust
// 创建用户名密码凭证
let credential = sdk.credentials()
    .create_username_password(
        "schwab",
        "user@example.com",
        "secret_password",
        None,  // expires_at
        None,  // options
    )
    .await?;

// 创建 API Key 凭证
let credential = sdk.credentials()
    .create_api_key(
        "stripe",
        "sk_live_...",
        Some("sk_secret_..."),
        None,
        None,
    )
    .await?;

// 创建 OAuth 刷新令牌凭证
let credential = sdk.credentials()
    .create_oauth_refresh(
        "google",
        "1//0d...",
        None,
        None,
    )
    .await?;
```

### Token 管理

```rust
use toani_vault_sdk::types::TokenScope;

// 检查 Token 权限
if sdk.token().has_scope(TokenScope::CredentialRead) {
    println!("Can read credentials");
}

// 检查 Token 是否即将过期
if sdk.token().is_expiring_soon(300) {  // 300秒 = 5分钟
    println!("Token will expire soon");
}

// 获取 Token 剩余有效时间
let remaining_seconds = sdk.token().get_remaining_time();
println!("Token expires in {} seconds", remaining_seconds);

// 验证 Token（向服务器发送验证请求）
let is_valid = sdk.token().verify(None).await?;
if !is_valid {
    println!("Token is invalid or revoked");
}

// 撤销 Token
let revoked = sdk.token().revoke(None).await?;
if revoked {
    println!("Token revoked successfully");
}

// 列表/详情/按 ID 撤销
let tokens = sdk.token().list(None).await?;
if let Some(first) = tokens.tokens.first() {
    let detail = sdk.token().get(&first.token_id, None).await?;
    sdk.token().revoke_by_id(&detail.token_id, None).await?;
}

// Service Account
let service_account = sdk.service_accounts().create(
    toani_vault_sdk::CreateServiceAccountRequest {
        name: "ci-bot".to_string(),
        description: Some("automation".to_string()),
        scope_ceiling: vec!["credential:read".to_string(), "tokens:read".to_string()],
    },
    None,
).await?;

let _service_account_token = sdk.service_accounts().create_token(
    &service_account.id,
    toani_vault_sdk::CreateServiceAccountTokenRequest {
        scopes: vec!["credential:read".to_string()],
        ttl_seconds: Some(3600),
        display_name: Some("ci-job-token".to_string()),
    },
    None,
).await?;
```

## 配置选项

```rust
use toani_vault_sdk::CredBridgeConfig;

let config = CredBridgeConfig::new("https://api.toani.io")
    .with_token("your-api-token")
    .with_tenant_id("tenant1")
    .with_user_id("user1")
    .with_timeout_ms(30000)           // 请求超时时间（毫秒）
    .with_max_retries(3)              // 最大重试次数
    .with_auto_refresh_token(true);   // 自动刷新 Token
```

## 错误处理

SDK 使用 `CredBridgeError` 作为统一错误类型：

```rust
use toani_vault_sdk::types::{CredBridgeErrorCode, CredBridgeError};

match sdk.credentials().get("invalid-id", None).await {
    Ok(credential) => println!("Found: {:?}", credential),
    Err(e) => {
        match e.code {
            CredBridgeErrorCode::NotFound => {
                println!("Credential not found");
            }
            CredBridgeErrorCode::Unauthorized => {
                println!("Not authorized");
            }
            CredBridgeErrorCode::Forbidden => {
                println!("Access forbidden");
            }
            CredBridgeErrorCode::TokenExpired => {
                println!("Token expired, please refresh");
            }
            _ => println!("Error: {}", e),
        }

        // 检查错误是否可重试
        if e.is_retryable() {
            println!("This error is retryable");
        }

        // 获取 HTTP 状态码
        if let Some(status) = e.status_code {
            println!("HTTP Status: {}", status);
        }
    }
}
```

## 请求选项

可以为单个请求设置选项：

```rust
use toani_vault_sdk::types::RequestOptions;

let options = RequestOptions::new()
    .with_timeout_ms(10000)           // 自定义超时
    .with_retries(5)                  // 自定义重试次数
    .with_skip_retry(false)
    .with_request_id("custom-id")     // 自定义请求 ID
    .with_header("X-Custom-Header", "value");

let credential = sdk.credentials()
    .get("credential-id", Some(options))
    .await?;
```

## 凭证过滤

```rust
use toani_vault_sdk::types::{CredentialFilter, CredentialType};

// 按服务 ID 过滤
let filter = CredentialFilter {
    service_id: Some("schwab".to_string()),
    ..Default::default()
};
let (credentials, total) = sdk.credentials()
    .list(Some(filter), None)
    .await?;

// 按凭证类型过滤
let filter = CredentialFilter {
    credential_type: Some(CredentialType::ApiKey),
    ..Default::default()
};

// 仅返回未过期的凭证
let filter = CredentialFilter {
    only_valid: Some(true),
    ..Default::default()
};
```

## API 文档

查看 [docs.rs](https://docs.rs/toani-vault-sdk) 获取完整的 API 文档。

## 示例

更多示例请查看 [EXAMPLES.md](./EXAMPLES.md)。

## 特性标志

```toml
[dependencies]
toani-vault-sdk = { version = "0.1.0", default-features = false, features = ["native-tls"] }
```

- `rustls` (默认): 使用 rustls 进行 TLS 连接
- `native-tls`: 使用系统原生 TLS

## 许可证

MIT License
