use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// CLI 配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// CredBridge 服务 URL
    pub url: Option<String>,

    /// API Token
    pub token: Option<String>,

    /// 默认输出格式
    #[serde(default)]
    pub output_format: OutputFormat,

    /// 超时时间 (秒)
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Table,
    Json,
}

fn default_timeout() -> u64 {
    30
}

impl Config {
    /// 获取配置文件路径
    pub fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir().context("无法获取配置目录")?;
        Ok(config_dir.join("credbridge").join("config.toml"))
    }

    /// 从指定路径加载配置
    pub fn load_from(path: &str) -> Result<Self> {
        let content =
            std::fs::read_to_string(path).with_context(|| format!("读取配置文件失败: {path}"))?;

        let config: Config = toml::from_str(&content).context("解析配置文件失败")?;

        Ok(config)
    }

    /// 加载配置
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("读取配置文件失败: {}", path.display()))?;

        let config: Config = toml::from_str(&content).context("解析配置文件失败")?;

        Ok(config)
    }

    /// 保存配置
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;

        // 确保父目录存在
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建配置目录失败: {}", parent.display()))?;
        }

        let content = toml::to_string_pretty(self).context("序列化配置失败")?;

        std::fs::write(&path, content)
            .with_context(|| format!("写入配置文件失败: {}", path.display()))?;

        // 设置文件权限为 0600 (仅用户可读写)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path)?.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&path, perms)?;
        }

        Ok(())
    }

    /// 检查是否已配置
    pub fn is_configured(&self) -> bool {
        self.url.is_some() && self.token.is_some()
    }

    /// 获取 URL，未配置时返回错误
    pub fn require_url(&self) -> Result<&str> {
        self.url
            .as_deref()
            .context("未配置服务 URL，请先运行 'credbridge config init'")
    }

    /// 获取 Token，未配置时返回错误
    pub fn require_token(&self) -> Result<&str> {
        self.token
            .as_deref()
            .context("未配置 API Token，请先运行 'credbridge config init'")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_serialize() {
        let config = Config {
            url: Some("https://api.credbridge.io".to_string()),
            token: Some("test_token".to_string()),
            output_format: OutputFormat::Json,
            timeout: 60,
        };

        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("https://api.credbridge.io"));
        assert!(toml_str.contains("json"));
    }

    #[test]
    fn test_config_deserialize() {
        let toml_str = r#"
url = "https://api.credbridge.io"
token = "secret_token"
output_format = "table"
timeout = 45
"#;

        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.url, Some("https://api.credbridge.io".to_string()));
        assert_eq!(config.timeout, 45);
    }
}
