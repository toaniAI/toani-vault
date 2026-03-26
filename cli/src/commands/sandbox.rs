use crate::cli::SandboxCommands;
use crate::config::Config;
use crate::i18n::tr;
use crate::output::OutputFormatter;
use anyhow::{Context, Result};

pub async fn execute(cmd: SandboxCommands, config: Config) -> Result<()> {
    if !config.is_configured() {
        anyhow::bail!("{}", tr("cli.not_configured"));
    }

    let _sdk = create_sdk(&config)?;
    let _formatter = OutputFormatter::new(config.output_format);

    match cmd {
        SandboxCommands::CreateSession { name, timeout } => {
            create_session(name, timeout).await
        }
        SandboxCommands::ListSessions => list_sessions().await,
        SandboxCommands::GetSession { id } => get_session(id).await,
        SandboxCommands::Terminate { id, force } => terminate_session(id, force).await,
        SandboxCommands::Execute {
            session_id,
            operation_type,
            params,
        } => execute_operation(session_id, operation_type, params).await,
        SandboxCommands::GetOperation { operation_id } => get_operation(operation_id).await,
        SandboxCommands::Stats => get_stats().await,
    }
}

fn create_sdk(config: &Config) -> Result<credbridge_sdk::CredBridgeSDK> {
    credbridge_sdk::CredBridgeSDK::new(
        credbridge_sdk::CredBridgeConfig::new(config.require_url()?)
            .with_token(config.require_token()?)
            .with_timeout_ms(config.timeout * 1000),
    )
    .context(tr("cli.sandbox.sdk_failed"))
}

async fn create_session(name: Option<String>, timeout: u32) -> Result<()> {
    println!("{}\n", tr("cli.sandbox.create"));

    let name = name.unwrap_or_else(|| format!("cli-session-{}"
, chrono::Utc::now().timestamp()));

    println!("{}", tr("cli.sandbox.unimplemented"));
    println!("   Name: {}", name);
    println!("   Timeout: {}s", timeout);

    Ok(())
}

async fn list_sessions() -> Result<()> {
    println!("{}\n", tr("cli.sandbox.list"));

    println!("{}", tr("cli.sandbox.unimplemented"));

    Ok(())
}

async fn get_session(id: String) -> Result<()> {
    println!("{}\n", tr("cli.sandbox.get"));

    println!("{}", tr("cli.sandbox.unimplemented"));
    println!("   Session ID: {}", id);

    Ok(())
}

async fn terminate_session(id: String, _force: bool) -> Result<()> {
    println!("{}\n", tr("cli.sandbox.terminate"));

    println!("{}", tr("cli.sandbox.unimplemented"));
    println!("   Session ID: {}", id);

    Ok(())
}

async fn execute_operation(
    session_id: String,
    operation_type: String,
    _params: Option<String>,
) -> Result<()> {
    println!("{}\n", tr("cli.sandbox.execute"));

    println!("{}", tr("cli.sandbox.unimplemented"));
    println!("   Session ID: {}", session_id);
    println!("   Operation type: {}", operation_type);

    Ok(())
}

async fn get_operation(operation_id: String) -> Result<()> {
    println!("{}\n", tr("cli.sandbox.operation_result"));

    println!("{}", tr("cli.sandbox.unimplemented"));
    println!("   Operation ID: {}", operation_id);

    Ok(())
}

async fn get_stats() -> Result<()> {
    println!("{}\n", tr("cli.sandbox.stats"));

    println!("{}", tr("cli.sandbox.unimplemented"));

    Ok(())
}
