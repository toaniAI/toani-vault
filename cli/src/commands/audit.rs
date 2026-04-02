use crate::cli::AuditCommands;
use crate::config::Config;
use crate::i18n::tr;
use crate::output::OutputFormatter;
use anyhow::{Context, Result};
use base64::Engine as _;
use credbridge_sdk::types::{
    AuditExportFormat, AuditExportRequest, AuditLogFilter, AuditVerifyRequest,
};

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

fn create_sdk(config: &Config) -> Result<credbridge_sdk::ToaniVaultSDK> {
    credbridge_sdk::ToaniVaultSDK::new(
        credbridge_sdk::CredBridgeConfig::new(config.require_url()?)
            .with_token(config.require_token()?)
            .with_timeout_ms(config.timeout * 1000),
    )
    .context("failed to create SDK client")
}

async fn query_logs(
    sdk: &credbridge_sdk::ToaniVaultSDK,
    formatter: &OutputFormatter,
    from: Option<String>,
    to: Option<String>,
    action: Option<String>,
    _resource_type: Option<String>,
    limit: u32,
) -> Result<()> {
    let filter = AuditLogFilter {
        start_time: parse_timestamp(from)?,
        end_time: parse_timestamp(to)?,
        action,
        limit: Some(limit),
        ..Default::default()
    };
    let response = sdk.audit().logs(filter, None).await?;
    if formatter.is_table() {
        let view: Vec<_> = response
            .data
            .items
            .iter()
            .map(|item| {
                serde_json::json!({
                    "id": item.id,
                    "timestamp": item.timestamp,
                    "service": item.service,
                    "action": item.action,
                    "outcome": item.outcome,
                    "risk_tier": item.risk_tier,
                })
            })
            .collect();
        formatter.print_list(
            &view,
            &[
                "ID",
                "Timestamp",
                "Service",
                "Action",
                "Outcome",
                "Risk Tier",
            ],
        )?;
    } else {
        formatter.print_object(&response)?;
    }
    Ok(())
}

async fn export_logs(
    sdk: &credbridge_sdk::ToaniVaultSDK,
    output: String,
    format: String,
    from: Option<String>,
    to: Option<String>,
) -> Result<()> {
    let request = AuditExportRequest {
        start_time: parse_timestamp(from)?,
        end_time: parse_timestamp(to)?,
        format: match format.as_str() {
            "csv" => AuditExportFormat::Csv,
            _ => AuditExportFormat::Json,
        },
        user_id_hash: None,
        action: None,
    };
    let response = sdk.audit().export(request, None).await?;
    let data = response
        .data
        .ok_or_else(|| anyhow::anyhow!("missing export payload"))?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(&data.content)?;
    std::fs::write(&output, bytes)?;
    eprintln!("export written to {output}");
    Ok(())
}

async fn verify_logs(sdk: &credbridge_sdk::ToaniVaultSDK) -> Result<()> {
    let logs = sdk.audit().logs(AuditLogFilter::default(), None).await?;
    let first = logs
        .data
        .items
        .first()
        .ok_or_else(|| anyhow::anyhow!("no audit log entries available to verify"))?;
    let response = sdk
        .audit()
        .verify(
            AuditVerifyRequest {
                id: Some(first.id.clone()),
                log_index: None,
            },
            None,
        )
        .await?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}

fn parse_timestamp(value: Option<String>) -> Result<Option<u64>> {
    value
        .map(|item| {
            chrono::DateTime::parse_from_rfc3339(&item)
                .map(|dt| dt.timestamp_millis() as u64)
                .map_err(|e| anyhow::anyhow!("invalid timestamp `{item}`: {e}"))
        })
        .transpose()
}
