use crate::cli::AuthCommands;
use crate::config::Config;
use anyhow::Result;
use colored::Colorize;

pub async fn execute(cmd: AuthCommands, config: Config) -> Result<()> {
    match cmd {
        AuthCommands::Login { url, token } => login(url, token).await,
        AuthCommands::Status => status(config).await,
        AuthCommands::Logout => logout().await,
    }
}

async fn login(url: String, token: String) -> Result<()> {
    println!("🔌 正在连接到 {} ...", url);

    let sdk = credbridge_sdk::CredBridgeSDK::new(
        credbridge_sdk::CredBridgeConfig::new(&url)
            .with_token(&token)
            .with_timeout_ms(30000),
    )?;

    // 尝试列出凭证来验证 Token
    match sdk.credentials().list(None, None).await {
        Ok(_) => {
            println!("✅ 登录成功!");
            println!("   服务: {}", url.cyan());
        }
        Err(e) => {
            anyhow::bail!("连接失败: {}", e);
        }
    }

    // 保存配置
    let mut config = Config::load().unwrap_or_default();
    config.url = Some(url);
    config.token = Some(token);
    config.save()?;

    println!("\n配置已保存。");
    Ok(())
}

async fn status(config: Config) -> Result<()> {
    if !config.is_configured() {
        println!("⚠️  未登录");
        println!("   请运行: credbridge auth login");
        return Ok(());
    }

    let url = config.require_url()?;
    let token = config.require_token()?;

    println!("🔍 检查登录状态...\n");

    let sdk = credbridge_sdk::CredBridgeSDK::new(
        credbridge_sdk::CredBridgeConfig::new(url)
            .with_token(token)
            .with_timeout_ms(10000),
    )?;

    match sdk.credentials().list(None, None).await {
        Ok(_) => {
            println!("{} 已登录", "✅".green());
            println!("   服务: {}", url.cyan());
            println!("   Token: {}", "有效".green());
        }
        Err(e) => {
            println!("{} 登录无效", "❌".red());
            println!("   错误: {}", e);
            println!("\n请重新登录: credbridge auth login");
        }
    }

    Ok(())
}

async fn logout() -> Result<()> {
    let config_path = Config::config_path()?;

    if config_path.exists() {
        std::fs::remove_file(&config_path)?;
        println!("✅ 已登出，配置已删除");
    } else {
        println!("ℹ️  未登录");
    }

    Ok(())
}
