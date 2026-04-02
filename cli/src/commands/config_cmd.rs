use crate::cli::ConfigCommands;
use crate::config::Config;
use crate::i18n::tr;
use crate::output::{input_secret, input_text, print_raw_json};
use anyhow::{Context, Result};

pub async fn execute(cmd: ConfigCommands) -> Result<()> {
    match cmd {
        ConfigCommands::Init { url, token } => init_config(url, token).await,
        ConfigCommands::Show => show_config().await,
        ConfigCommands::Set { key, value } => set_config(key, value).await,
        ConfigCommands::Get { key } => get_config(key).await,
    }
}

async fn init_config(url: Option<String>, token: Option<String>) -> Result<()> {
    let mut config = Config::default();

    let url = match url {
        Some(u) => u,
        None => input_text(tr("cli.config.enter_url"))?,
    };
    config.url = Some(url);

    let token = match token {
        Some(t) => t,
        None => input_secret(tr("cli.config.enter_token"))?,
    };
    config.token = Some(token);

    config.save()?;

    println!(
        "{}: {}",
        tr("cli.config.saved_to"),
        Config::config_path()?.display()
    );
    Ok(())
}

async fn show_config() -> Result<()> {
    let config = Config::load()?;

    let display_config = serde_json::json!({
        "url": config.url,
        "token": config.token.as_ref().map(|t| {
            if t.len() > 8 {
                format!("{}...{}", &t[..4], &t[t.len()-4..])
            } else {
                "***".to_string()
            }
        }),
        "output_format": config.output_format,
        "timeout": config.timeout,
    });

    print_raw_json(&display_config)?;
    Ok(())
}

async fn set_config(key: String, value: String) -> Result<()> {
    let mut config = Config::load()?;

    match key.as_str() {
        "url" => config.url = Some(value),
        "token" => config.token = Some(value),
        "output_format" => {
            config.output_format = match value.as_str() {
                "json" => crate::config::OutputFormat::Json,
                "table" => crate::config::OutputFormat::Table,
                _ => anyhow::bail!("{}", tr("cli.config.invalid_format")),
            };
        }
        "timeout" => {
            config.timeout = value.parse().context(tr("cli.config.timeout_number"))?;
        }
        _ => anyhow::bail!("{}: {}", tr("cli.config.unknown_key"), key),
    }

    config.save()?;
    println!("{}: {}", tr("cli.config.updated"), key);
    Ok(())
}

async fn get_config(key: String) -> Result<()> {
    let config = Config::load()?;

    let value = match key.as_str() {
        "url" => config.url.unwrap_or_default(),
        "token" => config
            .token
            .map(|t| {
                if t.len() > 8 {
                    format!("{}...{}", &t[..4], &t[t.len() - 4..])
                } else {
                    "***".to_string()
                }
            })
            .unwrap_or_default(),
        "output_format" => match config.output_format {
            crate::config::OutputFormat::Json => "json".to_string(),
            crate::config::OutputFormat::Table => "table".to_string(),
        },
        "timeout" => config.timeout.to_string(),
        _ => anyhow::bail!("{}: {}", tr("cli.config.unknown_key"), key),
    };

    println!("{value}");
    Ok(())
}
