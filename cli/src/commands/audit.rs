use crate::cli::AuditCommands;
use crate::config::Config;
use crate::output::OutputFormatter;
use anyhow::{Context, Result};

pub async fn execute(cmd: AuditCommands, config: Config) -> Result<()> {
    if !config.is_configured() {
        anyhow::bail!("未配置，请先运行 'credbridge auth login'");
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
        } => {
            query_logs(&sdk, &formatter, from, to, action, resource_type, limit).await
        }
        AuditCommands::Export {
            output,
            format,
            from,
            to,
        } => {
            export_logs(&sdk, output, format, from, to).await
        }
        AuditCommands::Verify => verify_logs(&sdk).await,
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

async fn query_logs(
    _sdk: &credbridge_sdk::CredBridgeSDK,
    _formatter: &OutputFormatter,
    _from: Option<String>,
    _to: Option<String>,
    _action: Option<String>,
    _resource_type: Option<String>,
    _limit: u32,
) -> Result<()> {
    println!("📋 审计日志查询功能暂未实现");
    println!("   请通过 CredBridge Web 界面查看审计日志");
    Ok(())
}

async fn export_logs(
    _sdk: &credbridge_sdk::CredBridgeSDK,
    _output: String,
    _format: String,
    _from: Option<String>,
    _to: Option<String>,
) -> Result<()> {
    println!("📤 审计日志导出功能暂未实现");
    Ok(())
}

async fn verify_logs(_sdk: &credbridge_sdk::CredBridgeSDK) -> Result<()> {
    println!("🔐 审计日志完整性验证功能暂未实现");
    Ok(())
}
