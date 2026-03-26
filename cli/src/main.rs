use clap::Parser;
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
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    // Load config
    let config = if let Some(config_path) = cli.config {
        Config::load_from(&config_path).unwrap_or_default()
    } else {
        Config::load().unwrap_or_default()
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
    }

    result
}
