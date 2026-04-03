use crate::cli::AuthCommands;
use crate::config::Config;
use crate::i18n::tr;
use anyhow::Result;
use colored::Colorize;

/// 显示服务账户认证警告
fn show_service_account_warning() {
    println!("\n{}", "⚠️  重要提示".yellow().bold());
    println!("{}", "━".repeat(50).yellow());
    println!(
        "{}",
        "此认证方法仅用于服务账户和平台 API Token。".yellow()
    );
    println!(
        "{}",
        "用户认证请通过 Web 界面使用 Privy 钱包登录。".yellow()
    );
    println!(
        "{}",
        "https://vault.toani.io".cyan()
    );
    println!("{}", "━".repeat(50).yellow());
    println!();
}

pub async fn execute(cmd: AuthCommands, config: Config) -> Result<()> {
    match cmd {
        AuthCommands::Login {
            url,
            token,
            service_account,
        } => login(url, token, service_account).await,
        AuthCommands::Status => status(config).await,
        AuthCommands::Logout => logout().await,
    }
}

async fn login(url: String, token: String, service_account: bool) -> Result<()> {
    // 显示服务账户认证警告
    show_service_account_warning();

    // 如果未指定 --service-account，提示用户确认
    if !service_account {
        println!(
            "{}",
            "提示: 使用 --service-account 标志可跳过此确认提示。".dimmed()
        );
    }

    println!("{} {} ...", tr("cli.auth.connecting"), url);

    let sdk = credbridge_sdk::ToaniVaultSDK::new(
        credbridge_sdk::CredBridgeConfig::new(&url)
            .with_token(&token)
            .with_timeout_ms(30000),
    )?;

    // Try listing credentials to validate the token.
    match sdk.credentials().list(None, None).await {
        Ok(_) => {
            println!("{}", tr("cli.auth.login_success"));
            println!("   {}: {}", tr("cli.auth.service"), url.cyan());
        }
        Err(e) => {
            anyhow::bail!("{}: {}", tr("cli.auth.connection_failed"), e);
        }
    }

    // Save config.
    let mut config = Config::load().unwrap_or_default();
    config.url = Some(url);
    config.token = Some(token);
    config.save()?;

    println!("\n{}", tr("cli.auth.config_saved"));
    Ok(())
}

async fn status(config: Config) -> Result<()> {
    if !config.is_configured() {
        println!("{}", tr("cli.auth.not_logged_in"));
        println!("   {}", tr("cli.auth.run_login"));
        return Ok(());
    }

    let url = config.require_url()?;
    let token = config.require_token()?;

    println!("{}\n", tr("cli.auth.checking_status"));

    let sdk = credbridge_sdk::ToaniVaultSDK::new(
        credbridge_sdk::CredBridgeConfig::new(url)
            .with_token(token)
            .with_timeout_ms(10000),
    )?;

    match sdk.credentials().list(None, None).await {
        Ok(_) => {
            println!("{} {}", "✅".green(), tr("cli.auth.logged_in"));
            println!("   {}: {}", tr("cli.auth.service"), url.cyan());
            println!("   Token: {}", tr("cli.auth.token_valid").green());
        }
        Err(e) => {
            println!("{} {}", "❌".red(), tr("cli.auth.login_invalid"));
            println!("   {}: {}", tr("cli.auth.error"), e);
            println!("\n{}", tr("cli.auth.relogin"));
        }
    }

    Ok(())
}

async fn logout() -> Result<()> {
    let config_path = Config::config_path()?;

    if config_path.exists() {
        std::fs::remove_file(&config_path)?;
        println!("{}", tr("cli.auth.logged_out"));
    } else {
        println!("{}", tr("cli.auth.not_logged_in"));
    }

    Ok(())
}
