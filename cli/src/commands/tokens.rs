use crate::cli::TokenCommands;
use crate::config::Config;
use crate::output::OutputFormatter;
use anyhow::{Context, Result};

pub async fn execute(cmd: TokenCommands, config: Config) -> Result<()> {
    if !config.is_configured() {
        anyhow::bail!("未配置，请先运行 'credbridge auth login'");
    }

    let sdk = create_sdk(&config)?;
    let formatter = OutputFormatter::new(config.output_format);

    match cmd {
        TokenCommands::Create {
            name: _,
            expires_in: _,
            scopes: _,
        } => {
            println!("ℹ️  Token 创建功能暂未实现");
            println!("   请通过 CredBridge Web 界面创建 Token");
            Ok(())
        }
        TokenCommands::List => {
            println!("ℹ️  Token 列表功能暂未实现");
            Ok(())
        }
        TokenCommands::Revoke { id: _ } => {
            revoke_token(&sdk).await
        }
        TokenCommands::Verify { token } => {
            verify_token(&sdk, &formatter, token).await
        }
    }
}

fn create_sdk(config: &Config) -> Result<credbridge_sdk::CredBridgeSDK> {
    credbridge_sdk::CredBridgeSDK::new(
        credbridge_sdk::CredBridgeConfig::new(config.require_url()?)
            .with_token(config.require_token()?)
            .with_timeout_ms(config.timeout * 1000),
    )
    .context("创建 SDK 客户端失败")
}

async fn revoke_token(sdk: &credbridge_sdk::CredBridgeSDK) -> Result<()> {
    println!("🚫 正在撤销当前 Token...\n");

    match sdk.token().revoke(None).await {
        Ok(true) => {
            println!("✅ Token 已撤销");
            println!("   请重新登录: credbridge auth login");
            Ok(())
        }
        Ok(false) => {
            anyhow::bail!("撤销 Token 失败")
        }
        Err(e) => {
            anyhow::bail!("撤销失败: {}", e)
        }
    }
}

async fn verify_token(
    sdk: &credbridge_sdk::CredBridgeSDK,
    formatter: &OutputFormatter,
    token: Option<String>,
) -> Result<()> {
    let token_to_verify = match token {
        Some(t) => t,
        None => "当前配置的 Token".to_string(),
    };

    println!("🔍 正在验证 Token: {}\n", token_to_verify);

    match sdk.token().verify(None).await {
        Ok(valid) => {
            if valid {
                formatter.print_success("Token 有效");

                // 显示 Token 信息
                if let Some(info) = sdk.token().get_token_info() {
                    let view = serde_json::json!({
                        "token_id": info.token_id,
                        "expires_at": info.expires_at,
                        "remaining_seconds": sdk.token().get_remaining_time(),
                        "scopes": info.scopes.iter().map(|s| format!("{:?}", s)).collect::<Vec<_>>(),
                    });
                    formatter.print_object(&view)?;
                }
            } else {
                formatter.print_error("Token 无效或已被撤销");
            }
            Ok(())
        }
        Err(e) => {
            anyhow::bail!("验证失败: {}", e)
        }
    }
}
