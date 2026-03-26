use clap::Parser;
use std::process::ExitCode;
use tracing::error;

mod cli;
mod commands;
mod config;
mod i18n;
mod output;
mod utils;

use cli::{Cli, Commands};
use config::Config;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    // Load config
    let config = if let Some(config_path) = cli.config {
        Config::load_from(&config_path).unwrap_or_default()
    } else {
        Config::load().unwrap_or_default()
    };
    let mut config = config;
    config.output_format = match cli.output.as_str() {
        "json" => crate::config::OutputFormat::Json,
        _ => crate::config::OutputFormat::Table,
    };

    // Execute command
    let result = match cli.command {
        Commands::Auth(cmd) => commands::auth::execute(cmd, config).await,
        Commands::Credentials(cmd) => commands::credentials::execute(cmd, config).await,
        Commands::Tokens(cmd) => commands::tokens::execute(cmd, config).await,
        Commands::Sandbox(cmd) => commands::sandbox::execute(cmd, config).await,
        Commands::Audit(cmd) => commands::audit::execute(cmd, config).await,
        Commands::Config(cmd) => commands::config_cmd::execute(cmd).await,
    };

    if let Err(ref e) = result {
        error!("command execution failed: {}", e);
        return map_exit_code(e);
    }

    ExitCode::SUCCESS
}

fn map_exit_code(error: &anyhow::Error) -> ExitCode {
    if let Some(sdk_error) = error.downcast_ref::<credbridge_sdk::CredBridgeError>() {
        return match sdk_error.code {
            credbridge_sdk::CredBridgeErrorCode::InvalidRequest => ExitCode::from(2),
            credbridge_sdk::CredBridgeErrorCode::Unauthorized
            | credbridge_sdk::CredBridgeErrorCode::InvalidToken
            | credbridge_sdk::CredBridgeErrorCode::TokenExpired
            | credbridge_sdk::CredBridgeErrorCode::TokenRevoked => ExitCode::from(3),
            credbridge_sdk::CredBridgeErrorCode::Forbidden
            | credbridge_sdk::CredBridgeErrorCode::InsufficientScope => ExitCode::from(4),
            credbridge_sdk::CredBridgeErrorCode::NotFound => ExitCode::from(5),
            credbridge_sdk::CredBridgeErrorCode::Timeout
            | credbridge_sdk::CredBridgeErrorCode::NetworkError => ExitCode::from(11),
            _ => ExitCode::from(10),
        };
    }

    let message = error.to_string();
    if message.contains("未配置") || message.contains("not configured") {
        ExitCode::from(3)
    } else if message.contains("invalid") || message.contains("必须") {
        ExitCode::from(2)
    } else {
        ExitCode::from(10)
    }
}
