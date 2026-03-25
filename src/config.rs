//! Runtime configuration helpers shared across binaries and libraries.

use std::fmt;

pub const TEE_MODE_ENV: &str = "TEE_MODE";
pub const TEE_DEBUG_ENV: &str = "TEE_DEBUG";
pub const TEE_PCS_BASE_URL_ENV: &str = "TEE_PCS_BASE_URL";

const INTEL_PCS_BASE_URL_PROD: &str = "https://api.trustedservices.intel.com/sgx/certification/v4";
const INTEL_PCS_BASE_URL_TEST: &str = "https://api.trustedservices.intel.com/sgx/certification/v4";
const DEFAULT_TEE_DEBUG_MODE: bool = false;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    InvalidTeeMode(String),
    InvalidBoolean {
        env_var: &'static str,
        value: String,
    },
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
}

impl TeeRuntimeConfig {
    pub fn hardware() -> Self {
        Self {
            mode: TeeRuntimeMode::Hardware,
            debug_mode: false,
            pcs_base_url: TeeRuntimeMode::Hardware.default_pcs_base_url().to_string(),
        }
    }

    pub fn simulation() -> Self {
        Self {
            mode: TeeRuntimeMode::Simulation,
            debug_mode: true,
            pcs_base_url: TeeRuntimeMode::Simulation
                .default_pcs_base_url()
                .to_string(),
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

        Ok(Self {
            mode,
            debug_mode,
            pcs_base_url,
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

#[cfg(test)]
mod tests {
    use super::{
        ConfigError, TEE_DEBUG_ENV, TEE_MODE_ENV, TEE_PCS_BASE_URL_ENV, TeeRuntimeConfig,
        TeeRuntimeMode,
    };
    use std::collections::HashMap;

    #[test]
    fn defaults_missing_mode_to_hardware() {
        let config = TeeRuntimeConfig::from_env_with(|_| None).unwrap();

        assert_eq!(config.mode, TeeRuntimeMode::Hardware);
        assert!(!config.debug_mode);
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
    fn rejects_invalid_mode() {
        let env = HashMap::from([(TEE_MODE_ENV, "production".to_string())]);
        let error = TeeRuntimeConfig::from_env_with(|name| env.get(name).cloned()).unwrap_err();

        assert!(matches!(error, ConfigError::InvalidTeeMode(_)));
    }
}
