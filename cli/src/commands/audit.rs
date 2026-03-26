use crate::cli::AuditCommands;
use crate::config::Config;
use crate::i18n::tr;
use crate::output::OutputFormatter;
use anyhow::{Context, Result};

pub async fn execute(cmd: AuditCommands, config: Config) -> Result<()> {
    if !config.is_configured() {
        anyhow::bail!("{}", tr("cli.not_configured"));
    }

    let sdk = create_sdk(&config)?;
    let formatter = OutputFormatter::new(config.output_format);

    match cmd {
        AuditCommands::Logs {
            from,
            to,
            action,
            resource_type,
            limit,
        } => query_logs(&sdk, &formatter, from, to, action, resource_type, limit).await,
        AuditCommands::Export {
            output,
            format,
            from,
            to,
        } => export_logs(&sdk, output, format, from, to).await,
        AuditCommands::Verify => verify_logs(&sdk).await,
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

async fn query_logs(
    _sdk: &credbridge_sdk::CredBridgeSDK,
    _formatter: &OutputFormatter,
    _from: Option<String>,
    _to: Option<String>,
    _action: Option<String>,
    _resource_type: Option<String>,
    _limit: u32,
) -> Result<()> {
    println!("{}", tr("cli.audit.logs_unimplemented"));
    println!("   {}", tr("cli.audit.web_hint"));
    Ok(())
}

async fn export_logs(
    _sdk: &credbridge_sdk::CredBridgeSDK,
    _output: String,
    _format: String,
    _from: Option<String>,
    _to: Option<String>,
) -> Result<()> {
    println!("{}", tr("cli.audit.export_unimplemented"));
    Ok(())
}

async fn verify_logs(_sdk: &credbridge_sdk::CredBridgeSDK) -> Result<()> {
    println!("{}", tr("cli.audit.verify_unimplemented"));
    Ok(())
}
