//! Toani Vault Rust SDK - 错误处理示例
//!
//! 展示各种错误场景的处理方式

use toani_vault_sdk::{CredBridgeConfig, ToaniVaultSDK};
use toani_vault_sdk::types::{CredBridgeError, CredBridgeErrorCode};
use std::time::Duration;
use tokio::time::sleep;

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let (base_url, token) = crate::get_config();

    let sdk = ToaniVaultSDK::new(
        CredBridgeConfig::new(base_url).with_token(token)
    )?;

    println!("=== Toani Vault 错误处理示例 ===\n");

    // 示例 1: 凭证不存在
    println!("1. 处理凭证不存在错误...");
    match sdk.credentials().get("non-existent-id", None).await {
        Ok(_) => println!("   未预期的成功"),
        Err(e) => {
            if e.code == CredBridgeErrorCode::NotFound {
                println!("   ✓ 正确处理: 凭证不存在");
            } else {
                println!("   ✗ 未预期的错误: {}", e);
            }
        }
    }

    // 示例 2: 权限不足
    println!("\n2. 处理权限不足错误...");
    // 注意: 这需要实际只有 read 权限的 Token
    // 这里仅展示错误处理模式
    match simulate_permission_error().await {
        Ok(_) => {},
        Err(e) => {
            if e.code == CredBridgeErrorCode::InsufficientScope ||
               e.code == CredBridgeErrorCode::Forbidden {
                println!("   ✓ 正确处理: 权限不足");
            }
        }
    }

    // 示例 3: 网络错误重试
    println!("\n3. 网络错误重试...");
    match simulate_network_error().await {
        Ok(_) => println!("   请求成功"),
        Err(e) => {
            if e.is_network_error() {
                println!("   ✓ 正确处理: 网络错误");
                println!("   是否可重试: {}", e.is_retryable());
            }
        }
    }

    // 示例 4: Token 过期
    println!("\n4. 处理 Token 过期...");
    match simulate_token_expired().await {
        Ok(_) => {},
        Err(e) => {
            if e.code == CredBridgeErrorCode::TokenExpired {
                println!("   ✓ 正确处理: Token 已过期");
                println!("   建议: 刷新 Token 或重新登录");
            } else if e.code == CredBridgeErrorCode::Unauthorized {
                println!("   ✓ 正确处理: 未授权");
            }
        }
    }

    // 示例 5: 通用错误处理函数
    println!("\n5. 通用错误处理函数...");

    async fn safe_operation<T, F, Fut>(
        operation: F,
        operation_name: &str,
    ) -> Option<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, CredBridgeError>>,
    {
        match operation().await {
            Ok(result) => Some(result),
            Err(e) => {
                let (error_msg, code) = match e.code {
                    CredBridgeErrorCode::NotFound => {
                        (format!("{} 失败: 资源不存在", operation_name), "NOT_FOUND")
                    }
                    CredBridgeErrorCode::Unauthorized => {
                        (format!("{} 失败: 未授权，请重新登录", operation_name), "UNAUTHORIZED")
                    }
                    CredBridgeErrorCode::Forbidden => {
                        (format!("{} 失败: 禁止访问", operation_name), "FORBIDDEN")
                    }
                    CredBridgeErrorCode::InsufficientScope => {
                        (format!("{} 失败: 权限不足", operation_name), "INSUFFICIENT_SCOPE")
                    }
                    CredBridgeErrorCode::TokenExpired => {
                        (format!("{} 失败: Token 已过期", operation_name), "TOKEN_EXPIRED")
                    }
                    CredBridgeErrorCode::CredentialExpired => {
                        (format!("{} 失败: 凭证已过期", operation_name), "CREDENTIAL_EXPIRED")
                    }
                    CredBridgeErrorCode::NetworkError | CredBridgeErrorCode::Timeout => {
                        (format!("{} 失败: 网络错误，请稍后重试", operation_name), "NETWORK_ERROR")
                    }
                    CredBridgeErrorCode::InternalError => {
                        (format!("{} 失败: 服务器内部错误", operation_name), "INTERNAL_ERROR")
                    }
                    _ => (format!("{} 失败: {}", operation_name, e.message), "UNKNOWN"),
                };

                println!("   处理结果: {} ({})", error_msg, code);
                if let Some(req_id) = e.request_id {
                    println!("   请求ID: {}", req_id);
                }
                None
            }
        }
    }

    // 使用通用错误处理函数
    safe_operation(
        || async { sdk.credentials().get("invalid-id", None).await },
        "获取凭证",
    ).await;

    // 示例 6: 带重试的操作
    println!("\n6. 带重试的操作...");
    async fn with_retry<T, F, Fut>(
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
                        println!("   认证错误，请检查 Token");
                        return Err(e);
                    }

                    // 指数退避
                    if attempt < max_retries {
                        let delay = Duration::from_millis(2_u64.pow(attempt) * 1000);
                        println!("   重试 {}/{} 等待 {:?}...", attempt + 1, max_retries, delay);
                        sleep(delay).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            CredBridgeError::new(CredBridgeErrorCode::Unknown, "Unknown error")
        }))
    }

    // 使用带重试的操作
    match with_retry(
        || async { sdk.credentials().list(None, None).await },
        2,
    ).await {
        Ok((creds, total)) => println!("   成功获取 {} 个凭证", total),
        Err(e) => println!("   最终失败: {}", e),
    }

    // 示例 7: 批量错误处理
    println!("\n7. 批量操作错误处理...");
    let ids = vec!["valid-id-1", "valid-id-2", "invalid-id"];

    let futures: Vec<_> = ids
        .iter()
        .map(|id| {
            let sdk = sdk.clone();
            let id = id.to_string();
            async move {
                match sdk.credentials().get(&id, None).await {
                    Ok(_) => (id, true, None),
                    Err(e) => (id, false, Some(e.code.to_string())),
                }
            }
        })
        .collect();

    let results = futures::future::join_all(futures).await;
    for (id, success, error) in results {
        if success {
            println!("   ✓ {}: 成功", id);
        } else {
            println!("   ✗ {}: {}", id, error.unwrap_or_default());
        }
    }

    println!("\n=== 错误处理示例完成 ===");

    Ok(())
}

// 模拟权限错误
async fn simulate_permission_error() -> Result<(), CredBridgeError> {
    Err(CredBridgeError::new(
        CredBridgeErrorCode::InsufficientScope,
        "Insufficient scope for this operation",
    ))
}

// 模拟网络错误
async fn simulate_network_error() -> Result<(), CredBridgeError> {
    Err(CredBridgeError::new(
        CredBridgeErrorCode::NetworkError,
        "Connection timeout",
    ))
}

// 模拟 Token 过期
async fn simulate_token_expired() -> Result<(), CredBridgeError> {
    Err(CredBridgeError::new(
        CredBridgeErrorCode::TokenExpired,
        "Token has expired",
    ))
}
