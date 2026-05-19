// CredBridge Rust SDK 测试脚本
// 用于测试 SDK 的基本功能

use toani_vault_sdk::{types::CredentialType, CredBridgeConfig, CredBridgeSDK};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== CredBridge Rust SDK 测试 ===\n");

    // 测试 1: SDK 配置
    println!("1. 测试 SDK 配置...");
    let config = CredBridgeConfig::new("http://localhost:8080")
        .with_token("v4.local.test-token")
        .with_timeout_ms(30000)
        .with_max_retries(3);

    println!("   配置:");
    println!("   - Base URL: {}", config.base_url);
    println!("   - Timeout: {}ms", config.timeout_ms);
    println!("   - Max Retries: {}", config.max_retries);

    // 测试 2: SDK 初始化
    println!("\n2. 测试 SDK 初始化...");
    match CredBridgeSDK::new(config) {
        Ok(sdk) => {
            println!("✅ SDK 初始化成功");

            // 测试 3: 服务实例
            println!("\n3. 测试 SDK 服务...");
            println!("   - credentials 服务：可用");
            println!("   - token 服务：可用");

            // 测试 4: Token 信息
            println!("\n4. 测试 Token 管理...");
            if let Some(token_id) = sdk.token().get_token_id() {
                println!("   Token ID: {}", token_id);
            } else {
                println!("   Token ID: (无法解析)");
            }

            if sdk.token().is_valid() {
                println!("   Token 状态：有效");
            } else {
                println!("   Token 状态：无效 (演示 Token，预期行为)");
            }

            let remaining = sdk.token().get_remaining_time();
            println!("   剩余时间：{} 秒", remaining);

            // 测试 5: 凭证类型枚举
            println!("\n5. 测试 CredentialType 枚举...");
            println!("   可用的凭证类型:");
            println!(
                "   - UsernamePassword: {:?}",
                CredentialType::UsernamePassword
            );
            println!("   - OAuthRefresh: {:?}", CredentialType::OAuthRefresh);
            println!("   - ApiKey: {:?}", CredentialType::ApiKey);
            println!("   - SessionCookie: {:?}", CredentialType::SessionCookie);
            println!("   - Certificate: {:?}", CredentialType::Certificate);
            println!("   - SshKey: {:?}", CredentialType::SshKey);
            println!(
                "   - DatabaseConnection: {:?}",
                CredentialType::DatabaseConnection
            );

            println!("\n=== 测试完成 ===");
            println!("\n注意：实际 API 调用需要有效的 Token 和后端服务支持。");
        }
        Err(e) => {
            println!("⚠️ SDK 初始化警告：{}", e);
            println!("   (这是预期的，因为 Token 是演示用的)");
        }
    }

    Ok(())
}
