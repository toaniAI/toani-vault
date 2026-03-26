use crate::cli::TokenCommands;
use crate::config::Config;
use crate::i18n::tr;
use crate::output::OutputFormatter;
use anyhow::{Context, Result};

pub async fn execute(cmd: TokenCommands, config: Config) -> Result<()> {
    if !config.is_configured() {
        anyhow::bail!("{}", tr("cli.not_configured"));
    }

    let sdk = create_sdk(&config)?;
    let formatter = OutputFormatter::new(config.output_format);

    match cmd {
        TokenCommands::Create {
            name: _,
            expires_in: _,
            scopes: _,
        } => {
            println!("{}", tr("cli.tokens.create_unimplemented"));
            println!("   {}", tr("cli.tokens.use_web"));
            Ok(())
        }
        TokenCommands::List => {
            println!("{}", tr("cli.tokens.list_unimplemented"));
            Ok(())
        }
        TokenCommands::Revoke { id: _ } => revoke_token(&sdk).await,
        TokenCommands::Verify { token } => verify_token(&sdk, &formatter, token).await,
    }
}

fn create_sdk(config: &Config) -> Result<credbridge_sdk::CredBridgeSDK> {
    credbridge_sdk::CredBridgeSDK::new(
        credbridge_sdk::CredBridgeConfig::new(config.require_url()?)
            .with_token(config.require_token()?)
            .with_timeout_ms(config.timeout * 1000),
    )
    .context("failed to create SDK client")
}

async fn revoke_token(sdk: &credbridge_sdk::CredBridgeSDK) -> Result<()> {
    println!("{}\n", tr("cli.tokens.revoking"));

    match sdk.token().revoke(None).await {
        Ok(true) => {
            println!("{}", tr("cli.tokens.revoked"));
            println!("   {}", tr("cli.auth.relogin"));
            Ok(())
        }
        Ok(false) => anyhow::bail!("{}", tr("cli.tokens.revoke_failed")),
        Err(e) => anyhow::bail!("{}: {}", tr("cli.tokens.revoke_failed"), e),
    }
}

async fn verify_token(
    sdk: &credbridge_sdk::CredBridgeSDK,
    formatter: &OutputFormatter,
    token: Option<String>,
) -> Result<()> {
    let token_to_verify = token.unwrap_or_else(|| tr("cli.tokens.current_token").to_string());

    println!("{}: {}\n", tr("cli.tokens.verifying"), token_to_verify);

    match sdk.token().verify(None).await {
        Ok(valid) => {
            if valid {
                formatter.print_success(tr("cli.tokens.valid"));

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
                formatter.print_error(tr("cli.tokens.invalid"));
            }
            Ok(())
        }
        Err(e) => anyhow::bail!("{}: {}", tr("cli.tokens.verify_failed"), e),
    }
}
