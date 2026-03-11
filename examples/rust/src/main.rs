//! CredBridge Rust SDK - 示例程序
//!
//! 运行示例:
//! ```bash
//! cargo run --example basic_usage
//! cargo run --example batch_operations
//! cargo run --example token_management
//! ```

mod basic_usage;
mod batch_operations;
mod token_management;
mod error_handling;
mod axum_integration;

use std::env;

pub fn get_config() -> (String, String) {
    let base_url = env::var("CREDBRIDGE_BASE_URL")
        .unwrap_or_else(|_| "https://api.credbridge.io".to_string());
    let token = env::var("CREDBRIDGE_TOKEN")
        .unwrap_or_else(|_| "your-api-token".to_string());

    (base_url, token)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("CredBridge Rust SDK 示例");
    println!("========================\n");

    // 运行基础示例
    println!("运行基础使用示例...");
    if let Err(e) = basic_usage::run().await {
        eprintln!("基础示例错误: {}", e);
    }

    println!("\n");

    // 运行批量操作示例
    println!("运行批量操作示例...");
    if let Err(e) = batch_operations::run().await {
        eprintln!("批量示例错误: {}", e);
    }

    println!("\n");

    // 运行 Token 管理示例
    println!("运行 Token 管理示例...");
    if let Err(e) = token_management::run().await {
        eprintln!("Token 示例错误: {}", e);
    }

    Ok(())
}
