use crate::config::TeeRuntimeMode;
use crate::crypto::keys::RootKeySource;
use crate::crypto::{CryptoError, HardwareRootKey};
use crate::tee::sealing::SealPolicy;

/// Provider 请求参数。
#[derive(Debug, Clone)]
pub struct ProviderRequest {
    pub runtime_mode: TeeRuntimeMode,
    pub seal_policy: SealPolicy,
    pub enclave_name: String,
}

/// Provider 返回的 Enclave 身份材料。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderIdentity {
    pub mrenclave: [u8; 32],
    pub mrsigner: [u8; 32],
}

/// Provider 启动产物：L0 与对应身份。
pub struct ProviderBootstrap {
    pub l0_key: HardwareRootKey,
    pub identity: ProviderIdentity,
}

impl std::fmt::Debug for ProviderBootstrap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderBootstrap")
            .field("root_key_source", &self.l0_key.source())
            .field("identity", &self.identity)
            .finish()
    }
}

#[derive(Debug)]
pub enum TeeProviderError {
    FeatureDisabled {
        feature: &'static str,
        requested_mode: TeeRuntimeMode,
    },
    BackendUnavailable(String),
    Crypto(CryptoError),
}

impl std::fmt::Display for TeeProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TeeProviderError::FeatureDisabled {
                feature,
                requested_mode,
            } => write!(
                f,
                "TEE_MODE={requested_mode} requested, but the `{feature}` feature is not enabled in this build; refusing to fall back to simulation"
            ),
            TeeProviderError::BackendUnavailable(message) => f.write_str(message),
            TeeProviderError::Crypto(error) => write!(f, "provider crypto error: {error}"),
        }
    }
}

impl std::error::Error for TeeProviderError {}

impl From<CryptoError> for TeeProviderError {
    fn from(error: CryptoError) -> Self {
        Self::Crypto(error)
    }
}

trait TeeProvider: Send + Sync {
    fn bootstrap(&self, request: &ProviderRequest) -> Result<ProviderBootstrap, TeeProviderError>;
}

struct SimulationProvider;

impl TeeProvider for SimulationProvider {
    fn bootstrap(&self, request: &ProviderRequest) -> Result<ProviderBootstrap, TeeProviderError> {
        let _seal_policy = request.seal_policy;

        Ok(ProviderBootstrap {
            l0_key: HardwareRootKey::for_simulation()?,
            identity: simulated_identity(&request.enclave_name),
        })
    }
}

#[cfg(not(feature = "tee-hardware"))]
struct MissingHardwareProvider;

#[cfg(not(feature = "tee-hardware"))]
impl TeeProvider for MissingHardwareProvider {
    fn bootstrap(&self, request: &ProviderRequest) -> Result<ProviderBootstrap, TeeProviderError> {
        Err(TeeProviderError::FeatureDisabled {
            feature: "tee-hardware",
            requested_mode: request.runtime_mode,
        })
    }
}

#[cfg(feature = "tee-hardware")]
struct FeatureGatedHardwareProvider;

#[cfg(feature = "tee-hardware")]
impl TeeProvider for FeatureGatedHardwareProvider {
    fn bootstrap(&self, request: &ProviderRequest) -> Result<ProviderBootstrap, TeeProviderError> {
        let _seal_policy = request.seal_policy;

        Err(TeeProviderError::BackendUnavailable(format!(
            "TEE_MODE={} requested and the `tee-hardware` feature is enabled, but no real SGX hardware provider has been wired yet; refusing to fall back to simulation",
            request.runtime_mode
        )))
    }
}

pub fn bootstrap_enclave(request: &ProviderRequest) -> Result<ProviderBootstrap, TeeProviderError> {
    match request.runtime_mode {
        TeeRuntimeMode::Simulation => SimulationProvider.bootstrap(request),
        TeeRuntimeMode::Hardware => hardware_provider().bootstrap(request),
    }
}

fn simulated_identity(enclave_name: &str) -> ProviderIdentity {
    use ring::digest::{SHA256, digest};

    let enclave_data = format!("{enclave_name}-v{}", env!("CARGO_PKG_VERSION"));
    let signer_data = "CredBridge-Signer-v1";

    let mrenclave = digest(&SHA256, enclave_data.as_bytes());
    let mrsigner = digest(&SHA256, signer_data.as_bytes());

    let mut identity = ProviderIdentity {
        mrenclave: [0u8; 32],
        mrsigner: [0u8; 32],
    };
    identity.mrenclave.copy_from_slice(mrenclave.as_ref());
    identity.mrsigner.copy_from_slice(mrsigner.as_ref());
    identity
}

#[cfg(not(feature = "tee-hardware"))]
fn hardware_provider() -> Box<dyn TeeProvider> {
    Box::new(MissingHardwareProvider)
}

#[cfg(feature = "tee-hardware")]
fn hardware_provider() -> Box<dyn TeeProvider> {
    Box::new(FeatureGatedHardwareProvider)
}

pub fn root_key_source(bootstrap: &ProviderBootstrap) -> RootKeySource {
    bootstrap.l0_key.source()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(mode: TeeRuntimeMode) -> ProviderRequest {
        ProviderRequest {
            runtime_mode: mode,
            seal_policy: SealPolicy::Mrsigner,
            enclave_name: "credbridge-test".to_string(),
        }
    }

    #[test]
    fn simulation_bootstrap_uses_simulation_root_source() {
        let bootstrap = bootstrap_enclave(&request(TeeRuntimeMode::Simulation)).unwrap();

        assert_eq!(root_key_source(&bootstrap), RootKeySource::Simulation);
        assert_ne!(bootstrap.identity.mrenclave, [0u8; 32]);
        assert_ne!(bootstrap.identity.mrsigner, [0u8; 32]);
    }

    #[cfg(not(feature = "tee-hardware"))]
    #[test]
    fn hardware_bootstrap_fails_closed_when_feature_is_disabled() {
        let error = bootstrap_enclave(&request(TeeRuntimeMode::Hardware)).unwrap_err();

        assert!(matches!(
            error,
            TeeProviderError::FeatureDisabled {
                feature: "tee-hardware",
                requested_mode: TeeRuntimeMode::Hardware,
            }
        ));
        assert!(
            error
                .to_string()
                .contains("refusing to fall back to simulation")
        );
    }

    #[cfg(feature = "tee-hardware")]
    #[test]
    fn hardware_bootstrap_fails_closed_when_backend_is_missing() {
        let error = bootstrap_enclave(&request(TeeRuntimeMode::Hardware)).unwrap_err();

        assert!(matches!(error, TeeProviderError::BackendUnavailable(_)));
        assert!(error.to_string().contains("no real SGX hardware provider"));
    }
}
