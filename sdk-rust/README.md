# Toani Vault Rust SDK

> **迁移注意**: 此 crate 已从 `credbridge-sdk` 重命名为 `toani-vault-sdk`。`CredBridgeSDK` 已被弃用，请使用 `ToaniVaultSDK`。`CredBridgeSDK` 仍可作为 `ToaniVaultSDK` 的类型别名使用，以保持向后兼容性。

用于与 Toani Vault API 交互的 Rust SDK。

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
