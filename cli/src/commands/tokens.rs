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
            expires_in,
            scopes,
        } => create_token(&sdk, &formatter, expires_in, scopes).await,
        TokenCommands::List => list_tokens(&sdk, &formatter).await,
        TokenCommands::Revoke { id } => revoke_token(&sdk, &formatter, id).await,
        TokenCommands::Verify { token } => verify_token(&sdk, &formatter, token).await,
    }
}

fn create_sdk(config: &Config) -> Result<credbridge_sdk::ToaniVaultSDK> {
    credbridge_sdk::ToaniVaultSDK::new(
        credbridge_sdk::CredBridgeConfig::new(config.require_url()?)
            .with_token(config.require_token()?)
            .with_timeout_ms(config.timeout * 1000),
    )
    .context("failed to create SDK client")
}

async fn create_token(
    sdk: &credbridge_sdk::ToaniVaultSDK,
    formatter: &OutputFormatter,
    expires_in: u64,
    scopes: Option<String>,
) -> Result<()> {
    let scopes = scopes
        .unwrap_or_else(|| "credential:read".to_string())
        .split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    let response = sdk
        .token()
        .create(None, scopes, Some(expires_in), None, None)
        .await?;
    if formatter.is_table() {
        formatter.print_diagnostic(&format!("Created token {}", response.token_id));
    } else {
        formatter.print_object(&response)?;
    }
    Ok(())
}

async fn list_tokens(
    sdk: &credbridge_sdk::ToaniVaultSDK,
    formatter: &OutputFormatter,
) -> Result<()> {
    let response = sdk.token().list(None).await?;
    if formatter.is_table() {
        let view: Vec<_> = response
            .tokens
            .iter()
            .map(|token| {
                serde_json::json!({
                    "token_id": token.token_id,
                    "user_id": token.user_id,
                    "tenant_id": token.tenant_id,
                    "revoked": token.revoked,
                    "expires_at": token.expires_at,
                })
            })
            .collect();
        formatter.print_list(
            &view,
            &["Token ID", "User ID", "Tenant ID", "Revoked", "Expires At"],
        )?;
    } else {
        formatter.print_object(&response)?;
    }
    Ok(())
}

async fn revoke_token(
    sdk: &credbridge_sdk::ToaniVaultSDK,
    formatter: &OutputFormatter,
    id: String,
) -> Result<()> {
    formatter.print_diagnostic(&format!("{}\n", tr("cli.tokens.revoking")));
    let response = sdk.token().revoke_by_id(&id, None).await?;
    if formatter.is_table() {
        formatter.print_diagnostic(tr("cli.tokens.revoked"));
    } else {
        formatter.print_object(&response)?;
    }
    Ok(())
}

async fn verify_token(
    sdk: &credbridge_sdk::ToaniVaultSDK,
    formatter: &OutputFormatter,
    token: Option<String>,
) -> Result<()> {
    let token_to_verify = token.unwrap_or_else(|| tr("cli.tokens.current_token").to_string());

    formatter.print_diagnostic(&format!(
        "{}: {}\n",
        tr("cli.tokens.verifying"),
        token_to_verify
    ));

    match sdk.token().verify(None).await {
        Ok(valid) => {
            if valid {
                formatter.print_success(tr("cli.tokens.valid"));

                if let Some(info) = sdk.token().get_token_info() {
                    let view = serde_json::json!({
                        "token_id": info.token_id,
                        "expires_at": info.expires_at,
                        "remaining_seconds": sdk.token().get_remaining_time(),
                        "scopes": info.scopes.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>(),
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
