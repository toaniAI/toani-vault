use crate::cli::ConfigCommands;
use crate::config::Config;
use crate::output::{print_raw_json, input_secret, input_text};
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

    // 交互式获取 URL
    let url = match url {
        Some(u) => u,
        None => input_text("请输入 CredBridge 服务 URL")?,
    };
    config.url = Some(url);

    // 交互式获取 Token
    let token = match token {
        Some(t) => t,
        None => input_secret("请输入 API Token")?,
    };
    config.token = Some(token);

    // 保存配置
    config.save()?;

    println!("✅ 配置已保存到: {}", Config::config_path()?.display());
    Ok(())
}

async fn show_config() -> Result<()> {
    let config = Config::load()?;

    // 安全显示 Token (部分隐藏)
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
                _ => anyhow::bail!("无效的格式，可选: json, table"),
            };
        }
        "timeout" => {
            config.timeout = value.parse().context("timeout 必须是数字")?;
        }
        _ => anyhow::bail!("未知配置项: {}", key),
    }

    config.save()?;
    println!("✅ 配置 '{}' 已更新", key);
    Ok(())
}

async fn get_config(key: String) -> Result<()> {
    let config = Config::load()?;

    let value = match key.as_str() {
        "url" => config.url.unwrap_or_default(),
        "token" => config.token.map(|t| {
            if t.len() > 8 {
                format!("{}...{}", &t[..4], &t[t.len()-4..])
            } else {
                "***".to_string()
            }
        }).unwrap_or_default(),
        "output_format" => match config.output_format {
            crate::config::OutputFormat::Json => "json".to_string(),
            crate::config::OutputFormat::Table => "table".to_string(),
        },
        "timeout" => config.timeout.to_string(),
        _ => anyhow::bail!("未知配置项: {}", key),
    };

    println!("{}", value);
    Ok(())
}
