//! Runtime configuration helpers shared across binaries and libraries.

use std::env;
use std::fmt;

use regex::Regex;

pub const TEE_MODE_ENV: &str = "TEE_MODE";
pub const TEE_DEBUG_ENV: &str = "TEE_DEBUG";
pub const TEE_PCS_BASE_URL_ENV: &str = "TEE_PCS_BASE_URL";
pub const TEE_ENCLAVE_PATH_ENV: &str = "TEE_ENCLAVE_PATH";
pub const SEALED_STORAGE_PATH_ENV: &str = "SEALED_STORAGE_PATH";
pub const DEFAULT_SEALED_STORAGE_PATH: &str = ".sealed";

// Privy 配置常量
pub const PRIVY_APP_ID_ENV: &str = "PRIVY_APP_ID";
pub const PRIVY_APP_SECRET_ENV: &str = "PRIVY_APP_SECRET";
pub const PRIVY_JWKS_URL_ENV: &str = "PRIVY_JWKS_URL";
pub const PRIVY_API_URL_ENV: &str = "PRIVY_API_URL";
pub const PRIVY_MOCK_ENABLED_ENV: &str = "PRIVY_MOCK_ENABLED";
pub const CREDBRIDGE_AUTH_ALLOW_MEMORY_FALLBACK_ENV: &str = "CREDBRIDGE_AUTH_ALLOW_MEMORY_FALLBACK";
pub const CREDBRIDGE_TENANT_ALLOW_MEMORY_FALLBACK_ENV: &str =
    "CREDBRIDGE_TENANT_ALLOW_MEMORY_FALLBACK";
pub const CREDBRIDGE_SANDBOX_ALLOW_MEMORY_FALLBACK_ENV: &str =
    "CREDBRIDGE_SANDBOX_ALLOW_MEMORY_FALLBACK";
pub const CREDBRIDGE_TOKEN_ALLOW_MEMORY_FALLBACK_ENV: &str =
    "CREDBRIDGE_TOKEN_ALLOW_MEMORY_FALLBACK";
pub const CREDBRIDGE_AUDIT_ALLOW_MEMORY_FALLBACK_ENV: &str =
    "CREDBRIDGE_AUDIT_ALLOW_MEMORY_FALLBACK";

const INTEL_PCS_BASE_URL_PROD: &str = "https://api.trustedservices.intel.com/sgx/certification/v4";
const INTEL_PCS_BASE_URL_TEST: &str = "https://api.trustedservices.intel.com/sgx/certification/v4";
const DEFAULT_TEE_DEBUG_MODE: bool = false;

const DEFAULT_PRIVY_API_URL: &str = "https://auth.privy.io/api/v1";

/// 构建默认 JWKS URL（基于 app_id）
fn default_jwks_url(app_id: &str) -> String {
    format!("https://auth.privy.io/api/v1/apps/{app_id}/jwks.json")
}

fn validate_privy_app_id(app_id: &str) -> Result<(), ConfigError> {
    let app_id = app_id.trim();

    if app_id.is_empty() {
        return Err(ConfigError::InvalidValue(format!(
            "{PRIVY_APP_ID_ENV} cannot be empty"
        )));
    }

    let cuid_regex = Regex::new(r"^c[a-z0-9]{24}$").expect("valid privy app id regex");
    if !cuid_regex.is_match(app_id) {
        return Err(ConfigError::InvalidValue(format!(
            "{PRIVY_APP_ID_ENV} must be a valid Privy app id (cuid), got `{app_id}`"
        )));
    }

    Ok(())
}

fn validate_privy_url(env_var: &'static str, url: &str) -> Result<(), ConfigError> {
    let parsed = reqwest::Url::parse(url).map_err(|error| {
        ConfigError::InvalidValue(format!("{env_var} must be an absolute URL: {error}"))
    })?;

    match parsed.scheme() {
        "http" | "https" => Ok(()),
        scheme => Err(ConfigError::InvalidValue(format!(
            "{env_var} must use http or https, got `{scheme}`"
        ))),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    InvalidTeeMode(String),
    InvalidBoolean {
        env_var: &'static str,
        value: String,
    },
    MissingConfig(String),
    InvalidValue(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::InvalidTeeMode(value) => {
                write!(
                    f,
                    "invalid TEE mode `{value}`; expected `hardware` or `simulation`"
                )
            }
            ConfigError::InvalidBoolean { env_var, value } => {
                write!(
                    f,
                    "invalid boolean `{value}` for environment variable `{env_var}`"
                )
            }
            ConfigError::MissingConfig(name) => {
                write!(f, "missing required configuration: {name}")
            }
            ConfigError::InvalidValue(msg) => {
                write!(f, "invalid configuration value: {msg}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeeRuntimeMode {
    Hardware,
    Simulation,
}

impl TeeRuntimeMode {
    pub fn from_env_value(value: &str) -> Result<Self, ConfigError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "hardware" => Ok(Self::Hardware),
            "simulation" => Ok(Self::Simulation),
            other => Err(ConfigError::InvalidTeeMode(other.to_string())),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TeeRuntimeMode::Hardware => "hardware",
            TeeRuntimeMode::Simulation => "simulation",
        }
    }

    pub fn default_pcs_base_url(self) -> &'static str {
        match self {
            TeeRuntimeMode::Hardware => INTEL_PCS_BASE_URL_PROD,
            TeeRuntimeMode::Simulation => INTEL_PCS_BASE_URL_TEST,
        }
    }

    pub fn allows_simulation(self) -> bool {
        matches!(self, TeeRuntimeMode::Simulation)
    }

    pub fn is_hardware(self) -> bool {
        matches!(self, TeeRuntimeMode::Hardware)
    }
}

impl fmt::Display for TeeRuntimeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeeRuntimeConfig {
    pub mode: TeeRuntimeMode,
    pub debug_mode: bool,
    pub pcs_base_url: String,
    pub enclave_path: Option<String>,
    pub sealed_storage_path: String,
}

impl TeeRuntimeConfig {
    pub fn hardware() -> Self {
        Self {
            mode: TeeRuntimeMode::Hardware,
            debug_mode: false,
            pcs_base_url: TeeRuntimeMode::Hardware.default_pcs_base_url().to_string(),
            enclave_path: std::env::var(TEE_ENCLAVE_PATH_ENV)
                .ok()
                .filter(|value| !value.trim().is_empty()),
            sealed_storage_path: std::env::var(SEALED_STORAGE_PATH_ENV)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_SEALED_STORAGE_PATH.to_string()),
        }
    }

    pub fn simulation() -> Self {
        Self {
            mode: TeeRuntimeMode::Simulation,
            debug_mode: true,
            pcs_base_url: TeeRuntimeMode::Simulation
                .default_pcs_base_url()
                .to_string(),
            enclave_path: None,
            sealed_storage_path: std::env::var(SEALED_STORAGE_PATH_ENV)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_SEALED_STORAGE_PATH.to_string()),
        }
    }

    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_env_with(|name| std::env::var(name).ok())
    }

    pub fn from_env_with<F>(get_var: F) -> Result<Self, ConfigError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let mode = get_var(TEE_MODE_ENV)
            .map(|value| TeeRuntimeMode::from_env_value(&value))
            .transpose()?
            .unwrap_or(TeeRuntimeMode::Hardware);

        let debug_mode = get_var(TEE_DEBUG_ENV)
            .map(|value| parse_bool(TEE_DEBUG_ENV, &value))
            .transpose()?
            .unwrap_or(DEFAULT_TEE_DEBUG_MODE);

        let pcs_base_url = get_var(TEE_PCS_BASE_URL_ENV)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| mode.default_pcs_base_url().to_string());

        let enclave_path = get_var(TEE_ENCLAVE_PATH_ENV).filter(|value| !value.trim().is_empty());
        let sealed_storage_path = get_var(SEALED_STORAGE_PATH_ENV)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_SEALED_STORAGE_PATH.to_string());

        Ok(Self {
            mode,
            debug_mode,
            pcs_base_url,
            enclave_path,
            sealed_storage_path,
        })
    }

    pub fn is_simulation(&self) -> bool {
        self.mode.allows_simulation()
    }

    pub fn is_hardware(&self) -> bool {
        matches!(self.mode, TeeRuntimeMode::Hardware)
    }
}

impl Default for TeeRuntimeConfig {
    fn default() -> Self {
        Self::hardware()
    }
}

fn parse_bool(env_var: &'static str, value: &str) -> Result<bool, ConfigError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        other => Err(ConfigError::InvalidBoolean {
            env_var,
            value: other.to_string(),
        }),
    }
}

/// Privy 配置
#[derive(Debug, Clone)]
pub struct PrivyConfig {
    /// Privy App ID
    pub app_id: String,
    /// Privy App Secret
    pub app_secret: String,
    /// JWKS 端点 URL
    pub jwks_url: String,
    /// Privy API URL
    pub api_url: String,
    /// 是否启用 Mock 模式（开发回退开关）
    pub mock_enabled: bool,
}

impl PrivyConfig {
    /// 从环境变量加载配置
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_env_with(|name| env::var(name).ok())
    }

    pub fn from_env_with<F>(get_var: F) -> Result<Self, ConfigError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let mock_enabled = get_var(PRIVY_MOCK_ENABLED_ENV)
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        // Mock 模式下，允许缺失关键配置
        if mock_enabled {
            let app_id = get_var(PRIVY_APP_ID_ENV).unwrap_or_else(|| "mock-app-id".to_string());
            let jwks_url = get_var(PRIVY_JWKS_URL_ENV).unwrap_or_else(|| default_jwks_url(&app_id));
            let api_url =
                get_var(PRIVY_API_URL_ENV).unwrap_or_else(|| DEFAULT_PRIVY_API_URL.to_string());

            if app_id != "mock-app-id" {
                validate_privy_app_id(&app_id)?;
            }
            validate_privy_url(PRIVY_JWKS_URL_ENV, &jwks_url)?;
            validate_privy_url(PRIVY_API_URL_ENV, &api_url)?;

            return Ok(Self {
                app_id,
                app_secret: get_var(PRIVY_APP_SECRET_ENV)
                    .unwrap_or_else(|| "mock-secret".to_string()),
                jwks_url,
                api_url,
                mock_enabled,
            });
        }

        // 非 Mock 模式下，关键配置必须存在
        let app_id = get_var(PRIVY_APP_ID_ENV).ok_or_else(|| {
            ConfigError::MissingConfig(format!(
                "{PRIVY_APP_ID_ENV} is required when {PRIVY_MOCK_ENABLED_ENV} is not set"
            ))
        })?;

        let app_secret = get_var(PRIVY_APP_SECRET_ENV).ok_or_else(|| {
            ConfigError::MissingConfig(format!(
                "{PRIVY_APP_SECRET_ENV} is required when {PRIVY_MOCK_ENABLED_ENV} is not set"
            ))
        })?;

        // JWKS URL 默认为基于 app_id 构建的 URL，可通过环境变量覆盖
        let jwks_url = get_var(PRIVY_JWKS_URL_ENV).unwrap_or_else(|| default_jwks_url(&app_id));

        let api_url =
            get_var(PRIVY_API_URL_ENV).unwrap_or_else(|| DEFAULT_PRIVY_API_URL.to_string());

        // 验证配置值
        validate_privy_app_id(&app_id)?;

        if app_secret.is_empty() {
            return Err(ConfigError::InvalidValue(format!(
                "{PRIVY_APP_SECRET_ENV} cannot be empty"
            )));
        }

        validate_privy_url(PRIVY_JWKS_URL_ENV, &jwks_url)?;
        validate_privy_url(PRIVY_API_URL_ENV, &api_url)?;

        Ok(Self {
            app_id,
            app_secret,
            jwks_url,
            api_url,
            mock_enabled,
        })
    }

    /// 检查是否配置了真实 Privy 凭证
    pub fn has_real_credentials(&self) -> bool {
        !self.mock_enabled && self.app_id != "mock-app-id" && !self.app_secret.starts_with("mock-")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ConfigError, DEFAULT_SEALED_STORAGE_PATH, PRIVY_API_URL_ENV, PRIVY_APP_ID_ENV,
        PRIVY_APP_SECRET_ENV, PRIVY_JWKS_URL_ENV, PRIVY_MOCK_ENABLED_ENV, PrivyConfig,
        SEALED_STORAGE_PATH_ENV, TEE_DEBUG_ENV, TEE_ENCLAVE_PATH_ENV, TEE_MODE_ENV,
        TEE_PCS_BASE_URL_ENV, TeeRuntimeConfig, TeeRuntimeMode,
    };
    use std::collections::HashMap;

    #[test]
    fn defaults_missing_mode_to_hardware() {
        let config = TeeRuntimeConfig::from_env_with(|_| None).unwrap();

        assert_eq!(config.mode, TeeRuntimeMode::Hardware);
        assert!(!config.debug_mode);
        assert_eq!(config.sealed_storage_path, DEFAULT_SEALED_STORAGE_PATH);
    }

    #[test]
    fn parses_explicit_simulation_mode() {
        let env = HashMap::from([(TEE_MODE_ENV, "simulation".to_string())]);
        let config = TeeRuntimeConfig::from_env_with(|name| env.get(name).cloned()).unwrap();

        assert_eq!(config.mode, TeeRuntimeMode::Simulation);
        assert!(!config.debug_mode);
    }

    #[test]
    fn debug_mode_is_independent_from_runtime_mode() {
        let env = HashMap::from([
            (TEE_MODE_ENV, "simulation".to_string()),
            (TEE_DEBUG_ENV, "true".to_string()),
        ]);
        let config = TeeRuntimeConfig::from_env_with(|name| env.get(name).cloned()).unwrap();

        assert_eq!(config.mode, TeeRuntimeMode::Simulation);
        assert!(config.debug_mode);
    }

    #[test]
    fn honors_explicit_debug_and_pcs_url() {
        let env = HashMap::from([
            (TEE_MODE_ENV, "hardware".to_string()),
            (TEE_DEBUG_ENV, "true".to_string()),
            (TEE_PCS_BASE_URL_ENV, "https://pcs.example.test".to_string()),
        ]);
        let config = TeeRuntimeConfig::from_env_with(|name| env.get(name).cloned()).unwrap();

        assert_eq!(config.mode, TeeRuntimeMode::Hardware);
        assert!(config.debug_mode);
        assert_eq!(config.pcs_base_url, "https://pcs.example.test");
    }

    #[test]
    fn captures_explicit_enclave_path() {
        let env = HashMap::from([
            (TEE_MODE_ENV, "hardware".to_string()),
            (
                TEE_ENCLAVE_PATH_ENV,
                "/tmp/credbridge_enclave.signed.so".to_string(),
            ),
        ]);
        let config = TeeRuntimeConfig::from_env_with(|name| env.get(name).cloned()).unwrap();

        assert_eq!(
            config.enclave_path.as_deref(),
            Some("/tmp/credbridge_enclave.signed.so")
        );
    }

    #[test]
    fn captures_explicit_sealed_storage_path() {
        let env = HashMap::from([(
            SEALED_STORAGE_PATH_ENV,
            "/var/lib/credbridge/sealed".to_string(),
        )]);
        let config = TeeRuntimeConfig::from_env_with(|name| env.get(name).cloned()).unwrap();

        assert_eq!(config.sealed_storage_path, "/var/lib/credbridge/sealed");
    }

    #[test]
    fn rejects_invalid_mode() {
        let env = HashMap::from([(TEE_MODE_ENV, "production".to_string())]);
        let error = TeeRuntimeConfig::from_env_with(|name| env.get(name).cloned()).unwrap_err();

        assert!(matches!(error, ConfigError::InvalidTeeMode(_)));
    }

    #[test]
    fn privy_config_accepts_valid_real_credentials() {
        let env = HashMap::from([
            (PRIVY_APP_ID_ENV, "cmniaifov006t0cl1af6l5xqs".to_string()),
            (PRIVY_APP_SECRET_ENV, "privy_app_secret_example".to_string()),
        ]);

        let config = PrivyConfig::from_env_with(|name| env.get(name).cloned()).unwrap();

        assert_eq!(config.app_id, "cmniaifov006t0cl1af6l5xqs");
        assert_eq!(
            config.jwks_url,
            "https://auth.privy.io/api/v1/apps/cmniaifov006t0cl1af6l5xqs/jwks.json"
        );
        assert_eq!(config.api_url, "https://auth.privy.io/api/v1");
    }

    #[test]
    fn privy_config_rejects_invalid_app_id_format() {
        let env = HashMap::from([
            (PRIVY_APP_ID_ENV, "mock-app-id".to_string()),
            (PRIVY_APP_SECRET_ENV, "privy_app_secret_example".to_string()),
        ]);

        let error = PrivyConfig::from_env_with(|name| env.get(name).cloned()).unwrap_err();

        assert!(matches!(error, ConfigError::InvalidValue(_)));
        assert!(error.to_string().contains(PRIVY_APP_ID_ENV));
    }

    #[test]
    fn privy_config_rejects_invalid_jwks_url() {
        let env = HashMap::from([
            (PRIVY_APP_ID_ENV, "cmniaifov006t0cl1af6l5xqs".to_string()),
            (PRIVY_APP_SECRET_ENV, "privy_app_secret_example".to_string()),
            (PRIVY_JWKS_URL_ENV, "/jwks.json".to_string()),
        ]);

        let error = PrivyConfig::from_env_with(|name| env.get(name).cloned()).unwrap_err();

        assert!(matches!(error, ConfigError::InvalidValue(_)));
        assert!(error.to_string().contains(PRIVY_JWKS_URL_ENV));
    }

    #[test]
    fn privy_config_rejects_invalid_api_url_in_mock_mode() {
        let env = HashMap::from([
            (PRIVY_MOCK_ENABLED_ENV, "true".to_string()),
            (PRIVY_API_URL_ENV, "not-a-url".to_string()),
        ]);

        let error = PrivyConfig::from_env_with(|name| env.get(name).cloned()).unwrap_err();

        assert!(matches!(error, ConfigError::InvalidValue(_)));
        assert!(error.to_string().contains(PRIVY_API_URL_ENV));
    }
}
