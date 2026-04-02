use crate::cli::AuthCommands;
use crate::config::Config;
use crate::i18n::tr;
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
