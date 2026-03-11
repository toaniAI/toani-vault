//! CredBridge Rust SDK - Token 管理示例
//!
//! 展示 Token 验证、权限检查和刷新操作

use credbridge_sdk::{CredBridgeConfig, CredBridgeSDK};
use credbridge_sdk::types::TokenScope;

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let (base_url, token) = crate::get_config();

    let config = CredBridgeConfig::new(base_url)
        .with_token(token);

    let sdk = CredBridgeSDK::new(config)?;

    println!("=== CredBridge Token 管理示例 ===\n");

    // 1. 获取 Token 信息
    println!("1. 获取 Token 信息...");
    if let Some(token_info) = sdk.token().get_token_info() {
        println!("   Token ID: {}", token_info.token_id);
        println!("   主题: {}", token_info.subject);
        println!("   租户ID: {}", token_info.tenant_id);
        println!("   用户ID: {}", token_info.user_id);
        println!("   颁发时间: {}", chrono::DateTime::from_timestamp(token_info.issued_at, 0)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default());
        println!("   过期时间: {}", chrono::DateTime::from_timestamp(token_info.expires_at, 0)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default());
    }

    // 2. 检查 Token 有效性
    println!("\n2. 检查 Token 有效性...");
    println!("   是否有效: {}", sdk.token().is_valid());
    println!("   是否即将过期 (5分钟): {}", sdk.token().is_expiring_soon(300));
    println!("   是否即将过期 (1小时): {}", sdk.token().is_expiring_soon(3600));
    println!("   剩余秒数: {}", sdk.token().get_remaining_time());
    println!("   剩余时间: {}", sdk.token().get_remaining_time_formatted());

    // 3. 检查权限
    println!("\n3. 检查权限...");
    println!("   是否有 read 权限: {}", sdk.token().has_scope(TokenScope::CredentialRead));
    println!("   是否有 decrypt 权限: {}", sdk.token().has_scope(TokenScope::CredentialDecrypt));
    println!("   是否有 write 权限: {}", sdk.token().has_scope(TokenScope::CredentialWrite));
    println!("   所有权限: {:?}", sdk.token().get_scopes());

    // 4. 权限组合检查
    println!("\n4. 权限组合检查...");
    let read_scopes = vec![TokenScope::CredentialRead, TokenScope::CredentialDecrypt];
    let write_scopes = vec![TokenScope::CredentialRead, TokenScope::CredentialWrite];

    println!("   是否有 read+decrypt: {}", sdk.token().has_all_scopes(&read_scopes));
    println!("   是否有 read+write: {}", sdk.token().has_all_scopes(&write_scopes));
    println!("   是否有任一 read/write: {}", sdk.token().has_any_scope(&read_scopes));

    // 5. 验证 Token（向服务器确认）
    println!("\n5. 验证 Token（向服务器确认）...");
    match sdk.token().verify(None).await {
        Ok(is_valid) => println!("   服务器验证结果: {}", is_valid),
        Err(e) => println!("   验证错误: {}", e),
    }

    // 6. 权限检查辅助函数
    println!("\n6. 权限检查辅助函数...");
    fn check_permission(sdk: &CredBridgeSDK, scope: TokenScope) {
        if sdk.token().has_scope(scope) {
            println!("   ✓ 有 {:?} 权限", scope);
        } else {
            println!("   ✗ 缺少 {:?} 权限", scope);
        }
    }

    check_permission(&sdk, TokenScope::CredentialRead);
    check_permission(&sdk, TokenScope::CredentialDecrypt);
    check_permission(&sdk, TokenScope::CredentialWrite);
    check_permission(&sdk, TokenScope::AuditRead);
    check_permission(&sdk, TokenScope::Admin);

    // 7. 检查并报告权限不足
    println!("\n7. 检查并报告权限不足...");
    let required_scopes = vec![
        TokenScope::CredentialRead,
        TokenScope::CredentialDecrypt,
        TokenScope::CredentialWrite,
    ];

    let granted: Vec<_> = required_scopes
        .iter()
        .filter(|s| sdk.token().has_scope(**s))
        .cloned()
        .collect();

    let missing: Vec<_> = required_scopes
        .iter()
        .filter(|s| !sdk.token().has_scope(**s))
        .cloned()
        .collect();

    println!("   已授予权限: {:?}", granted);
    println!("   缺失权限: {:?}", missing);

    if !missing.is_empty() {
        println!("   ⚠️  警告: 缺少以下权限，某些操作可能失败: {:?}", missing);
    }

    println!("\n=== Token 管理示例完成 ===");

    Ok(())
}

/// Token 刷新监控示例
pub async fn token_refresh_monitor() -> Result<(), Box<dyn std::error::Error>> {
    let (base_url, token) = crate::get_config();

    let config = CredBridgeConfig::new(base_url)
        .with_token(token);

    let sdk = CredBridgeSDK::new(config)?;

    println!("\n=== Token 刷新监控示例 ===\n");

    // 模拟定期检查 Token 状态
    for i in 0..3 {
        println!("检查 #{}", i + 1);
        println!("   Token 有效: {}", sdk.token().is_valid());
        println!("   剩余时间: {}", sdk.token().get_remaining_time_formatted());

        if sdk.token().is_expiring_soon(600) {
            println!("   ⚠️  Token 即将过期，需要刷新!");
            // 这里可以实现实际的 Token 刷新逻辑
            // let new_token = refresh_token().await?;
            // sdk.client().set_token(new_token);
        } else {
            println!("   ✓ Token 状态良好");
        }

        // 模拟间隔
        if i < 2 {
            println!("   等待 1 秒...\n");
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    }

    println!("\n=== Token 刷新监控示例完成 ===");

    Ok(())
}
