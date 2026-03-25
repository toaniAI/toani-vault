//! Runtime configuration helpers for the standalone vault-service crate.

use std::fmt;

pub const TEE_MODE_ENV: &str = "TEE_MODE";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    InvalidTeeMode(String),
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

    pub fn is_hardware(self) -> bool {
        matches!(self, TeeRuntimeMode::Hardware)
    }

    pub fn is_simulation(self) -> bool {
        matches!(self, TeeRuntimeMode::Simulation)
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
}

impl TeeRuntimeConfig {
    pub fn hardware() -> Self {
        Self {
            mode: TeeRuntimeMode::Hardware,
        }
    }

    pub fn simulation() -> Self {
        Self {
            mode: TeeRuntimeMode::Simulation,
        }
    }

    pub fn from_env() -> Result<Self, ConfigError> {
        match std::env::var(TEE_MODE_ENV) {
            Ok(value) => Ok(Self {
                mode: TeeRuntimeMode::from_env_value(&value)?,
            }),
            Err(_) => Ok(Self::hardware()),
        }
    }

    pub fn is_hardware(&self) -> bool {
        self.mode.is_hardware()
    }

    pub fn is_simulation(&self) -> bool {
        self.mode.is_simulation()
    }
}

impl Default for TeeRuntimeConfig {
    fn default() -> Self {
        Self::hardware()
    }
}

#[cfg(test)]
mod tests {
    use super::{ConfigError, TeeRuntimeConfig, TeeRuntimeMode};

    #[test]
    fn parses_simulation_mode() {
        let mode = TeeRuntimeMode::from_env_value("simulation").unwrap();
        assert_eq!(mode, TeeRuntimeMode::Simulation);
    }

    #[test]
    fn rejects_invalid_mode() {
        let error = TeeRuntimeMode::from_env_value("production").unwrap_err();
        assert!(matches!(error, ConfigError::InvalidTeeMode(_)));
    }

    #[test]
    fn simulation_config_sets_simulation_mode() {
        let config = TeeRuntimeConfig::simulation();
        assert!(config.is_simulation());
    }
}
