//! Runtime configuration helpers shared across binaries and libraries.

use std::env;
use std::fmt;

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

const DEFAULT_PRIVY_JWKS_URL: &str = "https://auth.privy.io/api/v1/sessions/jwks.json";
const DEFAULT_PRIVY_API_URL: &str = "https://auth.privy.io/api/v1";

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
        let mock_enabled = env::var(PRIVY_MOCK_ENABLED_ENV)
            .ok()
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        // Mock 模式下，允许缺失关键配置
        if mock_enabled {
            return Ok(Self {
                app_id: env::var(PRIVY_APP_ID_ENV).unwrap_or_else(|_| "mock-app-id".to_string()),
                app_secret: env::var(PRIVY_APP_SECRET_ENV)
                    .unwrap_or_else(|_| "mock-secret".to_string()),
                jwks_url: env::var(PRIVY_JWKS_URL_ENV)
                    .unwrap_or_else(|_| DEFAULT_PRIVY_JWKS_URL.to_string()),
                api_url: env::var(PRIVY_API_URL_ENV)
                    .unwrap_or_else(|_| DEFAULT_PRIVY_API_URL.to_string()),
                mock_enabled,
            });
        }

        // 非 Mock 模式下，关键配置必须存在
        let app_id = env::var(PRIVY_APP_ID_ENV).map_err(|_| {
            ConfigError::MissingConfig(format!(
                "{PRIVY_APP_ID_ENV} is required when {PRIVY_MOCK_ENABLED_ENV} is not set"
            ))
        })?;

        let app_secret = env::var(PRIVY_APP_SECRET_ENV).map_err(|_| {
            ConfigError::MissingConfig(format!(
                "{PRIVY_APP_SECRET_ENV} is required when {PRIVY_MOCK_ENABLED_ENV} is not set"
            ))
        })?;

        let jwks_url =
            env::var(PRIVY_JWKS_URL_ENV).unwrap_or_else(|_| DEFAULT_PRIVY_JWKS_URL.to_string());

        let api_url =
            env::var(PRIVY_API_URL_ENV).unwrap_or_else(|_| DEFAULT_PRIVY_API_URL.to_string());

        // 验证配置值
        if app_id.is_empty() {
            return Err(ConfigError::InvalidValue(format!(
                "{PRIVY_APP_ID_ENV} cannot be empty"
            )));
        }

        if app_secret.is_empty() {
            return Err(ConfigError::InvalidValue(format!(
                "{PRIVY_APP_SECRET_ENV} cannot be empty"
            )));
        }

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
        ConfigError, DEFAULT_SEALED_STORAGE_PATH, SEALED_STORAGE_PATH_ENV, TEE_DEBUG_ENV,
        TEE_ENCLAVE_PATH_ENV, TEE_MODE_ENV, TEE_PCS_BASE_URL_ENV, TeeRuntimeConfig, TeeRuntimeMode,
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
}
