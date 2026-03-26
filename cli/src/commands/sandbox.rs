use crate::cli::SandboxCommands;
use crate::config::Config;
use crate::i18n::tr;
use crate::output::OutputFormatter;
use anyhow::{Context, Result};
use credbridge_sdk::types::{CreateSandboxSessionRequest, ExecuteSandboxOperationRequest};
use std::collections::HashMap;

pub async fn execute(cmd: SandboxCommands, config: Config) -> Result<()> {
    if !config.is_configured() {
        anyhow::bail!("{}", tr("cli.not_configured"));
    }

    let sdk = create_sdk(&config)?;
    let formatter = OutputFormatter::new(config.output_format);

    match cmd {
        SandboxCommands::CreateSession {
            credential_id,
            original_intent,
        } => create_session(&sdk, &formatter, credential_id, original_intent).await,
        SandboxCommands::ListSessions => list_sessions(&sdk, &formatter).await,
        SandboxCommands::GetSession { id } => get_session(&sdk, &formatter, id).await,
        SandboxCommands::Terminate { id, force: _ } => {
            terminate_session(&sdk, &formatter, id).await
        }
        SandboxCommands::Execute {
            session_id,
            operation_type,
            params,
        } => execute_operation(&sdk, &formatter, session_id, operation_type, params).await,
        SandboxCommands::GetOperation { operation_id } => {
            get_operation(&sdk, &formatter, operation_id).await
        }
        SandboxCommands::Stats => get_stats(&sdk, &formatter).await,
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

async fn create_session(
    sdk: &credbridge_sdk::CredBridgeSDK,
    formatter: &OutputFormatter,
    credential_id: String,
    original_intent: String,
) -> Result<()> {
    formatter.print_diagnostic(&format!("{}\n", tr("cli.sandbox.create")));
    let response = sdk
        .sandbox()
        .create_session(
            CreateSandboxSessionRequest {
                credential_id,
                original_intent,
                metadata: None,
            },
            None,
        )
        .await?;
    formatter.print_object(&response)?;
    Ok(())
}

async fn list_sessions(
    sdk: &credbridge_sdk::CredBridgeSDK,
    formatter: &OutputFormatter,
) -> Result<()> {
    formatter.print_diagnostic(&format!("{}\n", tr("cli.sandbox.list")));
    let response = sdk.sandbox().list_sessions(None).await?;
    formatter.print_object(&response)?;
    Ok(())
}

async fn get_session(
    sdk: &credbridge_sdk::CredBridgeSDK,
    formatter: &OutputFormatter,
    id: String,
) -> Result<()> {
    formatter.print_diagnostic(&format!("{}\n", tr("cli.sandbox.get")));
    let response = sdk.sandbox().get_session(&id, None).await?;
    formatter.print_object(&response)?;
    Ok(())
}

async fn terminate_session(
    sdk: &credbridge_sdk::CredBridgeSDK,
    formatter: &OutputFormatter,
    id: String,
) -> Result<()> {
    formatter.print_diagnostic(&format!("{}\n", tr("cli.sandbox.terminate")));
    let response = sdk.sandbox().terminate_session(&id, None).await?;
    formatter.print_object(&response)?;
    Ok(())
}

async fn execute_operation(
    sdk: &credbridge_sdk::CredBridgeSDK,
    formatter: &OutputFormatter,
    session_id: String,
    operation_type: String,
    params: Option<String>,
) -> Result<()> {
    formatter.print_diagnostic(&format!("{}\n", tr("cli.sandbox.execute")));
    let parameters: HashMap<String, serde_json::Value> = params
        .map(|value| serde_json::from_str(&value))
        .transpose()?
        .unwrap_or_default();
    let response = sdk
        .sandbox()
        .execute(
            &session_id,
            ExecuteSandboxOperationRequest {
                operation_type: operation_type.clone(),
                description: format!("CLI {}", operation_type),
                parameters,
            },
            None,
        )
        .await?;
    formatter.print_object(&response)?;
    Ok(())
}

async fn get_operation(
    sdk: &credbridge_sdk::CredBridgeSDK,
    formatter: &OutputFormatter,
    operation_id: String,
) -> Result<()> {
    formatter.print_diagnostic(&format!("{}\n", tr("cli.sandbox.operation_result")));
    let response = sdk.sandbox().get_operation(&operation_id, None).await?;
    formatter.print_object(&response)?;
    Ok(())
}

async fn get_stats(sdk: &credbridge_sdk::CredBridgeSDK, formatter: &OutputFormatter) -> Result<()> {
    formatter.print_diagnostic(&format!("{}\n", tr("cli.sandbox.stats")));
    let response = sdk.sandbox().stats(None).await?;
    formatter.print_object(&response)?;
    Ok(())
}
