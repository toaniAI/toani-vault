# Toani Vault Rust SDK 使用示例

> **迁移注意**: 此 crate 已从 `credbridge-sdk` 重命名为 `toani-vault-sdk`。`CredBridgeSDK` 已被弃用，请使用 `ToaniVaultSDK`。

本文档提供 Toani Vault Rust SDK 的各种使用示例。

## 目录

- [基础示例](#基础示例)
- [凭证管理](#凭证管理)
- [Token 管理](#token-管理)
- [错误处理](#错误处理)
- [高级用法](#高级用法)

## 基础示例

### 初始化 SDK

```rust
use toani_vault_sdk::{CredBridgeConfig, ToaniVaultSDK};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 基础配置
    let sdk = ToaniVaultSDK::new(
        CredBridgeConfig::new("https://api.toani.io")
            .with_token("your-api-token")
    )?;

    // 完整配置
    let sdk = ToaniVaultSDK::new(
        CredBridgeConfig::new("https://api.toani.io")
            .with_token("your-api-token")
            .with_tenant_id("tenant1")
            .with_user_id("user1")
            .with_timeout_ms(30000)
            .with_max_retries(3)
            .with_auto_refresh_token(true)
    )?;

    Ok(())
}
```

### 使用底层客户端

```rust
use toani_vault_sdk::{CredBridgeConfig, CredBridgeClient};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建客户端
    let client = Arc::new(CredBridgeClient::new(
        CredBridgeConfig::new("https://api.toani.io")
            .with_token("your-api-token")
    )?);

    // 直接使用客户端发送请求
    let response: serde_json::Value = client
        .get("/credentials")
        .await?;

    println!("{}", serde_json::to_string_pretty(&response)?);

    Ok(())
}
```

## 凭证管理

### 创建凭证

```rust
use toani_vault_sdk::{ToaniVaultSDK, CredBridgeConfig, types::CredentialType};
use std::collections::HashMap;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sdk = ToaniVaultSDK::new(
        CredBridgeConfig::new("https://api.toani.io")
            .with_token("your-api-token")
    )?;

    // 方法 1: 使用通用 create 方法
    let mut data = HashMap::new();
    data.insert("username".to_string(), json!("user@example.com"));
    data.insert("password".to_string(), json!("secret_password"));
    data.insert("mfa_code".to_string(), json!("123456"));

    let credential = sdk.credentials()
        .create(
            "schwab",                           // service_id
            CredentialType::UsernamePassword,   // credential_type
            data,                               // plaintext_data
            Some(1893456000),                   // expires_at (Unix timestamp)
            None,                               // options
        )
        .await?;

    println!("Created credential ID: {}", credential.credential_id);
    println!("Service: {}", credential.service_id);

    // 方法 2: 使用快捷方法
    let credential = sdk.credentials()
        .create_username_password(
            "schwab",
            "user@example.com",
            "secret_password",
            Some(1893456000),
            None,
        )
        .await?;

    Ok(())
}
```

### 创建不同类型的凭证

```rust
use toani_vault_sdk::{ToaniVaultSDK, CredBridgeConfig, types::CredentialType};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sdk = ToaniVaultSDK::new(
        CredBridgeConfig::new("https://api.toani.io")
            .with_token("your-api-token")
    )?;

    // API Key 凭证
    let api_key_cred = sdk.credentials()
        .create_api_key(
            "stripe",
            "pk_live_...",
            Some("sk_live_..."),
            None,
            None,
        )
        .await?;

    // OAuth 刷新令牌凭证
    let oauth_cred = sdk.credentials()
        .create_oauth_refresh(
            "google",
            "1//0dY4r_...",
            None,
            None,
        )
        .await?;

    // 自定义凭证
    use std::collections::HashMap;
    use serde_json::json;

    let mut data = HashMap::new();
    data.insert("private_key".to_string(), json!("-----BEGIN RSA PRIVATE KEY-----\n..."));
    data.insert("certificate".to_string(), json!("-----BEGIN CERTIFICATE-----\n..."));

    let cert_cred = sdk.credentials()
        .create(
            "internal-ca",
            CredentialType::Certificate,
            data,
            None,
            None,
        )
        .await?;

    Ok(())
}
```

### 列示和过滤凭证

```rust
use toani_vault_sdk::{ToaniVaultSDK, CredBridgeConfig, types::{CredentialFilter, CredentialType}};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sdk = ToaniVaultSDK::new(
        CredBridgeConfig::new("https://api.toani.io")
            .with_token("your-api-token")
    )?;

    // 获取所有凭证
    let (credentials, total) = sdk.credentials().list(None, None).await?;
    println!("Total credentials: {}", total);
    for cred in credentials {
        println!("- {} ({})", cred.credential_id, cred.service_id);
    }

    // 按服务 ID 过滤
    let filter = CredentialFilter {
        service_id: Some("schwab".to_string()),
        ..Default::default()
    };
    let (schwab_creds, _) = sdk.credentials()
        .list(Some(filter), None)
        .await?;
    println!("Schwab credentials: {}", schwab_creds.len());

    // 按凭证类型过滤
    let filter = CredentialFilter {
        credential_type: Some(CredentialType::ApiKey),
        ..Default::default()
    };
    let (api_keys, _) = sdk.credentials()
        .list(Some(filter), None)
        .await?;

    // 仅显示未过期的凭证
    let filter = CredentialFilter {
        only_valid: Some(true),
        ..Default::default()
    };

    // 使用快捷方法
    let (schwab_creds, _) = sdk.credentials()
        .get_by_service("schwab", None)
        .await?;

    let (api_keys, _) = sdk.credentials()
        .get_by_type(CredentialType::ApiKey, None)
        .await?;

    Ok(())
}
```

### 获取和解密凭证

```rust
use toani_vault_sdk::{ToaniVaultSDK, CredBridgeConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sdk = ToaniVaultSDK::new(
        CredBridgeConfig::new("https://api.toani.io")
            .with_token("your-api-token")
    )?;

    let credential_id = "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c";

    // 获取凭证元数据（不含明文）
    let credential = sdk.credentials().get(credential_id, None).await?;
    println!("Service: {}", credential.service_id);
    println!("Type: {:?}", credential.credential_type);
    println!("Created at: {}", credential.created_at);

    // 解密凭证获取明文
    let decrypted = sdk.credentials()
        .decrypt(credential_id, Some("用户登录操作"), None)
        .await?;

    println!("Decrypted credential ID: {}", decrypted.credential_id);

    // 访问明文数据
    if let Some(username) = decrypted.plaintext_data.get("username") {
        println!("Username: {}", username);
    }
    if let Some(password) = decrypted.plaintext_data.get("password") {
        println!("Password: {}", password);
    }

    Ok(())
}
```

### 删除凭证

```rust
use toani_vault_sdk::{ToaniVaultSDK, CredBridgeConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sdk = ToaniVaultSDK::new(
        CredBridgeConfig::new("https://api.toani.io")
            .with_token("your-api-token")
    )?;

    let credential_id = "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c";

    // 删除凭证
    let result = sdk.credentials().delete(credential_id, None).await?;
    if result.deleted {
        println!("Credential {} deleted successfully", result.credential_id);
    }

    // 检查凭证是否存在
    let exists = sdk.credentials().exists(credential_id, None).await?;
    if !exists {
        println!("Credential no longer exists");
    }

    Ok(())
}
```

## Token 管理

### Token 验证和检查

```rust
use toani_vault_sdk::{ToaniVaultSDK, CredBridgeConfig, types::TokenScope};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sdk = ToaniVaultSDK::new(
        CredBridgeConfig::new("https://api.toani.io")
            .with_token("your-api-token")
    )?;

    // 检查 Token 是否有效
    if sdk.token().is_valid() {
        println!("Token is valid");
    }

    // 检查 Token 是否即将过期（5分钟缓冲）
    if sdk.token().is_expiring_soon(300) {
        println!("Warning: Token will expire soon!");
    }

    // 获取剩余有效时间
    let remaining = sdk.token().get_remaining_time();
    println!("Token expires in {} seconds", remaining);

    // 获取格式化的剩余时间
    let formatted = sdk.token().get_remaining_time_formatted();
    println!("Token expires in: {}", formatted);  // 输出: "2小时" 或 "5分钟" 等

    // 向服务器验证 Token（检查是否被撤销）
    let is_valid = sdk.token().verify(None).await?;
    if !is_valid {
        println!("Token is invalid or has been revoked");
    }

    Ok(())
}
```

### Token 权限检查

```rust
use toani_vault_sdk::{ToaniVaultSDK, CredBridgeConfig, types::TokenScope};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sdk = ToaniVaultSDK::new(
        CredBridgeConfig::new("https://api.toani.io")
            .with_token("your-api-token")
    )?;

    // 检查单个 Scope
    if sdk.token().has_scope(TokenScope::CredentialRead) {
        println!("Can read credentials");
    }

    if sdk.token().has_scope(TokenScope::CredentialDecrypt) {
        println!("Can decrypt credentials");
    }

    // 检查任一 Scope
    if sdk.token().has_any_scope(&[
        TokenScope::CredentialRead,
        TokenScope::CredentialWrite,
    ]) {
        println!("Can read or write credentials");
    }

    // 检查所有 Scope
    if sdk.token().has_all_scopes(&[
        TokenScope::CredentialRead,
        TokenScope::CredentialDecrypt,
    ]) {
        println!("Can read AND decrypt credentials");
    }

    // 获取所有 Scope
    let scopes = sdk.token().get_scopes();
    println!("Token scopes: {:?}", scopes);

    Ok(())
}
```

### Token 信息获取

```rust
use toani_vault_sdk::{ToaniVaultSDK, CredBridgeConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sdk = ToaniVaultSDK::new(
        CredBridgeConfig::new("https://api.toani.io")
            .with_token("your-api-token")
    )?;

    // 获取 Token 信息
    if let Some(token_id) = sdk.token().get_token_id() {
        println!("Token ID: {}", token_id);
    }

    if let Some(tenant_id) = sdk.token().get_tenant_id() {
        println!("Tenant ID: {}", tenant_id);
    }

    if let Some(user_id) = sdk.token().get_user_id() {
        println!("User ID: {}", user_id);
    }

    if let Some(issued_at) = sdk.token().get_issued_at() {
        let datetime = chrono::DateTime::from_timestamp(issued_at, 0);
        println!("Issued at: {:?}", datetime);
    }

    if let Some(expires_at) = sdk.token().get_expires_at() {
        let datetime = chrono::DateTime::from_timestamp(expires_at, 0);
        println!("Expires at: {:?}", datetime);
    }

    Ok(())
}
```

## 错误处理

### 基本错误处理

```rust
use toani_vault_sdk::{ToaniVaultSDK, CredBridgeConfig, types::CredBridgeErrorCode};

#[tokio::main]
async fn main() {
    let sdk = ToaniVaultSDK::new(
        CredBridgeConfig::new("https://api.toani.io")
            .with_token("invalid-token")
    ).unwrap();

    match sdk.credentials().get("some-id", None).await {
        Ok(credential) => {
            println!("Found: {:?}", credential);
        }
        Err(e) => {
            match e.code {
                CredBridgeErrorCode::NotFound => {
                    eprintln!("Credential not found");
                }
                CredBridgeErrorCode::Unauthorized => {
                    eprintln!("Authentication failed - check your token");
                }
                CredBridgeErrorCode::Forbidden => {
                    eprintln!("Access denied - insufficient permissions");
                }
                CredBridgeErrorCode::TokenExpired => {
                    eprintln!("Token expired - please refresh");
                }
                CredBridgeErrorCode::NetworkError => {
                    eprintln!("Network error - check your connection");
                }
                CredBridgeErrorCode::Timeout => {
                    eprintln!("Request timed out");
                }
                _ => {
                    eprintln!("Unexpected error: {}", e);
                    if let Some(details) = &e.details {
                        eprintln!("Details: {:?}", details);
                    }
                }
            }

            // 获取请求 ID（用于故障排查）
            if let Some(request_id) = &e.request_id {
                eprintln!("Request ID: {}", request_id);
            }
        }
    }
}
```

### 可重试错误处理

```rust
use toani_vault_sdk::{ToaniVaultSDK, CredBridgeConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sdk = ToaniVaultSDK::new(
        CredBridgeConfig::new("https://api.toani.io")
            .with_token("your-api-token")
            .with_max_retries(3)  // SDK 会自动重试
    )?;

    match sdk.credentials().get("credential-id", None).await {
        Ok(credential) => {
            println!("Found: {:?}", credential);
        }
        Err(e) => {
            if e.is_retryable() {
                println!("Error is retryable but all retries failed: {}", e);
                // 可以在这里实现自定义重试逻辑
            } else {
                println!("Non-retryable error: {}", e);
            }
        }
    }

    Ok(())
}
```

## 高级用法

### 自定义请求选项

```rust
use toani_vault_sdk::{ToaniVaultSDK, CredBridgeConfig, types::RequestOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sdk = ToaniVaultSDK::new(
        CredBridgeConfig::new("https://api.toani.io")
            .with_token("your-api-token")
    )?;

    // 为单个请求设置自定义选项
    let options = RequestOptions::new()
        .with_timeout_ms(10000)           // 10秒超时
        .with_retries(5)                  // 最多重试5次
        .with_request_id("my-custom-id")  // 自定义请求ID
        .with_header("X-Request-Source", "my-app")
        .with_header("X-Correlation-ID", "abc-123");

    let credential = sdk.credentials()
        .get("credential-id", Some(options))
        .await?;

    // 跳过重试
    let options = RequestOptions::new().with_skip_retry(true);
    let credential = sdk.credentials()
        .get("credential-id", Some(options))
        .await?;

    Ok(())
}
```

### 并发请求

```rust
use toani_vault_sdk::{ToaniVaultSDK, CredBridgeConfig};
use futures::future::join_all;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sdk = ToaniVaultSDK::new(
        CredBridgeConfig::new("https://api.toani.io")
            .with_token("your-api-token")
    )?;

    let credential_ids = vec![
        "id-1",
        "id-2",
        "id-3",
        "id-4",
        "id-5",
    ];

    // 并发获取多个凭证
    let futures: Vec<_> = credential_ids
        .iter()
        .map(|id| sdk.credentials().get(*id, None))
        .collect();

    let results = join_all(futures).await;

    for (id, result) in credential_ids.iter().zip(results) {
        match result {
            Ok(credential) => println!("{}: Found {}", id, credential.service_id),
            Err(e) => println!("{}: Error - {}", id, e),
        }
    }

    Ok(())
}
```

### 批量创建凭证

```rust
use toani_vault_sdk::{ToaniVaultSDK, CredBridgeConfig, types::CredentialType};
use futures::future::join_all;
use std::collections::HashMap;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sdk = ToaniVaultSDK::new(
        CredBridgeConfig::new("https://api.toani.io")
            .with_token("your-api-token")
    )?;

    let services = vec![
        ("schwab", "user1", "pass1"),
        ("etrade", "user2", "pass2"),
        ("ibkr", "user3", "pass3"),
    ];

    // 并发创建凭证
    let futures: Vec<_> = services
        .into_iter()
        .map(|(service, username, password)| {
            sdk.credentials().create_username_password(
                service,
                username,
                password,
                None,
                None,
            )
        })
        .collect();

    let results = join_all(futures).await;

    for result in results {
        match result {
            Ok(credential) => {
                println!("Created: {}", credential.credential_id);
            }
            Err(e) => {
                eprintln!("Failed to create credential: {}", e);
            }
        }
    }

    Ok(())
}
```

### Token 刷新处理

```rust
use toani_vault_sdk::{ToaniVaultSDK, CredBridgeConfig};
use tokio::time::{interval, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sdk = ToaniVaultSDK::new(
        CredBridgeConfig::new("https://api.toani.io")
            .with_token("initial-token")
            .with_auto_refresh_token(true)
    )?;

    // 定期检查 Token 状态
    let mut check_interval = interval(Duration::from_secs(60));

    loop {
        check_interval.tick().await;

        // 检查 Token 是否即将过期
        if sdk.token().is_expiring_soon(600) {  // 10分钟缓冲
            println!("Token is expiring soon, refreshing...");

            // 获取新 Token（这里需要根据实际认证流程实现）
            let new_token = refresh_token().await?;
            sdk.token().set_token(new_token);

            println!("Token refreshed successfully");
        }
    }
}

async fn refresh_token() -> Result<String, Box<dyn std::error::Error>> {
    // 实现 Token 刷新逻辑
    // 这通常涉及调用认证服务器
    Ok("new-token".to_string())
}
```

## 完整示例：凭证管理 CLI

```rust
use toani_vault_sdk::{ToaniVaultSDK, CredBridgeConfig, types::{CredentialType, CredentialFilter}};
use std::collections::HashMap;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 从环境变量读取配置
    let api_url = std::env::var("TOANI_VAULT_URL")
        .unwrap_or_else(|_| "https://api.toani.io".to_string());
    let token = std::env::var("TOANI_VAULT_TOKEN")
        .expect("TOANI_VAULT_TOKEN environment variable not set");

    let sdk = ToaniVaultSDK::new(
        CredBridgeConfig::new(api_url)
            .with_token(token)
    )?;

    // 显示 Token 信息
    println!("Token Info:");
    if let Some(tenant_id) = sdk.token().get_tenant_id() {
        println!("  Tenant: {}", tenant_id);
    }
    if let Some(user_id) = sdk.token().get_user_id() {
        println!("  User: {}", user_id);
    }
    println!("  Remaining: {}", sdk.token().get_remaining_time_formatted());

    // 列出所有凭证
    println!("\nCredentials:");
    let (credentials, total) = sdk.credentials().list(None, None).await?;
    println!("Total: {}", total);

    for cred in credentials {
        println!(
            "  - {} [{}] {}",
            cred.credential_id,
            cred.credential_type,
            cred.service_id
        );
    }

    // 示例：创建新凭证
    let mut data = HashMap::new();
    data.insert("username".to_string(), json!("cli-user"));
    data.insert("password".to_string(), json!("cli-password"));

    let new_cred = sdk.credentials()
        .create("cli-test", CredentialType::UsernamePassword, data, None, None)
        .await?;

    println!("\nCreated new credential: {}", new_cred.credential_id);

    // 示例：解密凭证
    let decrypted = sdk.credentials()
        .decrypt(&new_cred.credential_id, Some("CLI demo"), None)
        .await?;

    println!("\nDecrypted data:");
    for (key, value) in decrypted.plaintext_data {
        let display_value = if key.contains("password") || key.contains("secret") {
            "***".to_string()
        } else {
            value.to_string()
        };
        println!("  {}: {}", key, display_value);
    }

    // 清理：删除凭证
    let deleted = sdk.credentials()
        .delete(&new_cred.credential_id, None)
        .await?;

    if deleted.deleted {
        println!("\nCredential deleted successfully");
    }

    Ok(())
}
```

运行 CLI 示例：

```bash
export TOANI_VAULT_URL="https://api.toani.io"
export TOANI_VAULT_TOKEN="your-api-token"
cargo run --example cli
```
