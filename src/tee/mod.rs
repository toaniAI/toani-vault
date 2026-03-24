//! TEE（可信执行环境）模块
//!
//! 实现 SGX Enclave 相关功能
//! 包括密钥管理、ECALL/OCALL 接口、远程认证等
//!
//! # 模块结构
//!
//! ```text
//! tee/
//! ├── mod.rs           - 模块导出
//! ├── enclave.rs       - Enclave 核心实现
//! ├── keys.rs          - 密钥管理（TTL 缓存、Zeroize 安全清理）
//! ├── cleanup.rs       - 密钥清理策略与调度器
//! ├── sealing.rs       - SGX Sealing 密钥和密封存储
//! ├── attestation.rs   - SGX DCAP 远程认证协议
//! └── challenge.rs     - 挑战-响应协议
//! ```
//!
//! # 核心功能
//!
//! - **Enclave**: Enclave 生命周期管理、加密/解密操作
//! - **Keys**: 密钥管理，TTL 缓存，Enclave 重启后密钥恢复
//! - **Cleanup**: 密钥清理策略，自动 Zeroize，后台清理调度
//! - **Sealing**: SGX Sealing Key 获取、密封数据存储
//! - **Attestation**: SGX DCAP 远程认证，ECDSA P-256 Quote 生成和验证
//! - **Challenge**: 挑战-响应协议，建立安全可信通道
//!
//! # 远程认证流程
//!
//! ```text
//! 1. Verifier 生成随机挑战
//! 2. Enclave 生成 Quote (REPORT_DATA = hash(challenge + identity))
//! 3. Verifier 验证 Quote 签名和测量值
//! 4. 验证成功后建立安全通道
//! ```
//!
//! # 使用示例
//!
//! ```rust
//! use vault_service::tee::{Enclave, EnclaveConfig, EnclaveState};
//!
//! // 创建并初始化 Enclave
//! let config = EnclaveConfig::default();
//! let mut enclave = Enclave::new(config);
//! enclave.initialize().expect("Enclave initialization failed");
//!
//! // 执行加密操作
//! let plaintext = b"sensitive credential data";
//! let encrypted = enclave.encrypt_credential(
//!     "tenant_123",
//!     "user_456",
//!     "credential_789",
//!     plaintext
//! ).expect("Encryption failed");
//!
//! // 执行解密操作
//! let decrypted = enclave.decrypt_credential(
//!     "tenant_123",
//!     "user_456",
//!     "credential_789",
//!     &encrypted
//! ).expect("Decryption failed");
//!
//! assert_eq!(decrypted, plaintext);
//! ```

use crate::config::{TeeRuntimeConfig, TeeRuntimeMode};
use std::path::Path;

// 子模块定义
pub mod attestation;
pub mod challenge;
pub mod cleanup;
pub mod dcap;
pub mod driver_verify;
pub mod enclave;
pub mod keys;
pub mod quote;
pub mod sandbox;
pub mod sealing;
pub mod upgrade;

// 公开导出 - Enclave
pub use enclave::{CacheStats, Enclave, EnclaveConfig, EnclaveError, EnclaveState, EnclaveStats};

// 公开导出 - Sealing
pub use sealing::{SealPolicy, SealedData, SealedStorage, SealingKey, SealingService};

// 公开导出 - Attestation
pub use attestation::{
    ATTESTATION_PROTOCOL_VERSION, AttestationError, AttestationResult, AttestationService,
    AttestationSession, AttestationState, CHALLENGE_DEFAULT_TTL, ECDSA_P256_PUBLIC_KEY_LEN,
    ECDSA_P256_SIGNATURE_LEN, EcdsaPublicKey, EcdsaSignature, MAX_QUOTE_LEN, Quote, QuoteSignature,
    ReportBody, ReportData, SGX_MEASUREMENT_LEN, SGX_QUOTE_TYPE_ECDSA_256, SGX_QUOTE_VERSION,
    SGX_REPORT_DATA_LEN,
};

// 公开导出 - DCAP
pub use dcap::{
    CertificateInfo, DCAP_SERVICE_VERSION, DcapAttestationReport, DcapConfig, DcapError, DcapQuote,
    DcapQuoteSignature, DcapReportBody, DcapService, EcdsaSignatureDcap, INTEL_PCS_BASE_URL_PROD,
    INTEL_PCS_BASE_URL_TEST, MAX_PCK_CERT_CHAIN_LEN,
};

// 公开导出 - Quote
pub use quote::{
    ParsedQuote, ParsedReportBody, QuoteMetadata, QuoteParseError, QuoteParser,
    QuoteSerializeError, QuoteSerializer, QuoteValidationError, QuoteValidator,
};

// 公开导出 - Challenge
pub use challenge::{
    CHALLENGE_LENGTH, CHALLENGE_PROTOCOL_VERSION, CHANNEL_KEY_LENGTH, Challenge, ChallengeError,
    ChallengeMetadata, ChallengeProtocol, ChallengeResponse, ChallengeStatus,
    DEFAULT_CHALLENGE_TTL, MAX_CONCURRENT_CHALLENGES, ProverProtocol, SecureChannel,
};

// 公开导出 - Keys (内存安全与密钥管理)
pub use keys::{
    CacheStatistics, CachedKeyEntry, DEFAULT_KEY_TTL_SECONDS, KeyManager, KeyManagerError, KeyType,
    MASTER_KEY_STORAGE_ID, MasterKeyMetadata, ProtectedKeyMaterial, UserKeyCache,
};

// 公开导出 - Cleanup (密钥清理策略)
pub use cleanup::{
    CleanupConfig, CleanupScheduler, CleanupStats, KeyCleaner, KeyLifecycle, ProtectedMemory,
    SecureScope,
};

/// TEE 模块版本
pub const TEE_VERSION: &str = "0.1.0";

/// 默认用户密钥缓存 TTL（秒）
pub const DEFAULT_USER_KEY_TTL: u64 = 300; // 5 分钟

/// 检查 TEE 环境可用性
///
/// 返回当前平台支持的 TEE 类型
pub fn detect_tee() -> TeeType {
    detect_tee_capabilities(TeeRuntimeMode::Hardware).detected_type
}

const SGX_DEVICE_PATHS: [&str; 4] = [
    "/dev/sgx_enclave",
    "/dev/sgx/enclave",
    "/dev/sgx",
    "/dev/isgx",
];

const SGX_ATTESTATION_PATHS: [&str; 3] = [
    "/dev/sgx_provision",
    "/dev/sgx/provision",
    "/var/run/aesmd/aesm.socket",
];

fn path_exists(path: &str) -> bool {
    Path::new(path).exists()
}

fn detect_tee_type_with<F>(exists: F) -> TeeType
where
    F: Fn(&str) -> bool,
{
    if SGX_DEVICE_PATHS.iter().any(|path| exists(path)) {
        TeeType::IntelSgx
    } else {
        TeeType::None
    }
}

fn remote_attestation_available_with<F>(exists: F) -> bool
where
    F: Fn(&str) -> bool,
{
    SGX_ATTESTATION_PATHS.iter().any(|path| exists(path))
}

fn detect_tee_capabilities_with<F>(requested_mode: TeeRuntimeMode, exists: F) -> TeeCapabilities
where
    F: Fn(&str) -> bool + Copy,
{
    let detected_type = detect_tee_type_with(exists);
    let hardware_available = detected_type.is_hardware();
    let remote_attestation_available =
        hardware_available && remote_attestation_available_with(exists);

    TeeCapabilities {
        requested_mode,
        detected_type,
        hardware_available,
        seal_policies: vec![SealPolicy::Mrenclave, SealPolicy::Mrsigner],
        remote_attestation_available,
        enclave_memory_mb: None,
    }
}

pub fn detect_tee_capabilities(requested_mode: TeeRuntimeMode) -> TeeCapabilities {
    detect_tee_capabilities_with(requested_mode, path_exists)
}

pub fn validate_runtime_requirements(
    runtime: &TeeRuntimeConfig,
) -> Result<TeeCapabilities, TeeRuntimeError> {
    validate_runtime_requirements_with(runtime.mode, path_exists)
}

fn validate_runtime_requirements_with<F>(
    requested_mode: TeeRuntimeMode,
    exists: F,
) -> Result<TeeCapabilities, TeeRuntimeError>
where
    F: Fn(&str) -> bool + Copy,
{
    let capabilities = detect_tee_capabilities_with(requested_mode, exists);

    if requested_mode.is_hardware() && !capabilities.hardware_available {
        return Err(TeeRuntimeError::HardwareUnavailable {
            requested_mode,
            detected_type: capabilities.detected_type,
        });
    }

    if requested_mode.is_hardware() && !capabilities.remote_attestation_available {
        return Err(TeeRuntimeError::RemoteAttestationUnavailable {
            requested_mode,
            detected_type: capabilities.detected_type,
        });
    }

    Ok(capabilities)
}

/// TEE 运行时校验错误
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeeRuntimeError {
    /// 请求硬件模式，但没有检测到受支持硬件
    HardwareUnavailable {
        requested_mode: TeeRuntimeMode,
        detected_type: TeeType,
    },
    /// 请求硬件模式，但缺少远程认证前置条件
    RemoteAttestationUnavailable {
        requested_mode: TeeRuntimeMode,
        detected_type: TeeType,
    },
}

impl std::fmt::Display for TeeRuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TeeRuntimeError::HardwareUnavailable {
                requested_mode,
                detected_type,
            } => write!(
                f,
                "TEE_MODE={} requested, but no supported hardware TEE was detected ({})",
                requested_mode,
                detected_type.description()
            ),
            TeeRuntimeError::RemoteAttestationUnavailable {
                requested_mode,
                detected_type,
            } => write!(
                f,
                "TEE_MODE={} requested, but remote attestation prerequisites are missing for detected TEE ({})",
                requested_mode,
                detected_type.description()
            ),
        }
    }
}

impl std::error::Error for TeeRuntimeError {}

/// TEE 类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeeType {
    /// Intel SGX
    IntelSgx,
    /// AMD SEV-SNP
    AmdSevSnp,
    /// AWS Nitro Enclaves
    AwsNitro,
    /// ARM TrustZone
    ArmTrustZone,
    /// 模拟模式（开发测试）
    Simulation,
    /// 无 TEE 支持
    None,
}

impl TeeType {
    /// 检查是否支持硬件 TEE
    pub fn is_hardware(&self) -> bool {
        matches!(
            self,
            TeeType::IntelSgx | TeeType::AmdSevSnp | TeeType::AwsNitro | TeeType::ArmTrustZone
        )
    }

    /// 获取 TEE 类型描述
    pub fn description(&self) -> &'static str {
        match self {
            TeeType::IntelSgx => "Intel SGX",
            TeeType::AmdSevSnp => "AMD SEV-SNP",
            TeeType::AwsNitro => "AWS Nitro Enclaves",
            TeeType::ArmTrustZone => "ARM TrustZone",
            TeeType::Simulation => "Software Simulation",
            TeeType::None => "No TEE Support",
        }
    }
}

/// TEE 能力信息
#[derive(Debug, Clone, PartialEq)]
pub struct TeeCapabilities {
    /// 调用方请求的运行模式
    pub requested_mode: TeeRuntimeMode,
    /// 底层探测到的 TEE 类型
    pub detected_type: TeeType,
    /// 是否检测到硬件 TEE
    pub hardware_available: bool,
    /// 支持的密封策略
    pub seal_policies: Vec<SealPolicy>,
    /// 远程认证支持
    pub remote_attestation_available: bool,
    /// 安全飞地内存大小（MB）
    pub enclave_memory_mb: Option<u32>,
}

/// 获取 TEE 能力信息
pub fn get_capabilities() -> TeeCapabilities {
    detect_tee_capabilities(TeeRuntimeMode::Hardware)
}

/// TEE 健康状态
#[derive(Debug, Clone)]
pub struct TeeHealth {
    /// Enclave 状态
    pub enclave_state: EnclaveState,
    /// TEE 类型
    pub tee_type: TeeType,
    /// 是否健康
    pub healthy: bool,
    /// 错误信息（如果不健康）
    pub error: Option<String>,
}

/// 安全清零内存
///
/// 使用 `zeroize` crate 确保敏感数据被彻底清理
#[inline]
pub fn secure_zero(data: &mut [u8]) {
    use zeroize::Zeroize;
    data.zeroize();
}

/// 生成安全的随机字节
#[inline]
pub fn secure_random_bytes(len: usize) -> Result<Vec<u8>, crate::crypto::CryptoError> {
    use rand::RngCore;

    let mut buf = vec![0u8; len];
    let mut rng = rand::thread_rng();
    rng.try_fill_bytes(&mut buf)
        .map_err(|_| crate::crypto::CryptoError::RngError)?;

    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TeeRuntimeMode;

    #[test]
    fn test_detect_tee() {
        let tee = detect_tee_type_with(|_| false);
        assert_eq!(tee, TeeType::None);
    }

    #[test]
    fn test_tee_type_is_hardware() {
        assert!(TeeType::IntelSgx.is_hardware());
        assert!(TeeType::AmdSevSnp.is_hardware());
        assert!(!TeeType::Simulation.is_hardware());
        assert!(!TeeType::None.is_hardware());
    }

    #[test]
    fn test_tee_type_description() {
        assert_eq!(TeeType::IntelSgx.description(), "Intel SGX");
        assert_eq!(TeeType::Simulation.description(), "Software Simulation");
    }

    #[test]
    fn test_get_capabilities() {
        let caps = detect_tee_capabilities(TeeRuntimeMode::Simulation);
        assert_eq!(caps.requested_mode, TeeRuntimeMode::Simulation);
        assert_eq!(caps.detected_type, TeeType::None);
        assert!(!caps.hardware_available);
        assert_eq!(caps.seal_policies.len(), 2);
        assert!(!caps.remote_attestation_available);
    }

    #[test]
    fn detects_sgx_when_device_path_exists() {
        let tee = detect_tee_type_with(|path| path == "/dev/sgx_enclave");
        assert_eq!(tee, TeeType::IntelSgx);
    }

    #[test]
    fn detects_remote_attestation_when_aesm_socket_exists() {
        assert!(remote_attestation_available_with(
            |path| path == "/var/run/aesmd/aesm.socket"
        ));
    }

    #[test]
    fn keeps_requested_mode_distinct_from_detected_type() {
        let caps = detect_tee_capabilities_with(TeeRuntimeMode::Simulation, |path| {
            path == "/dev/sgx_enclave" || path == "/var/run/aesmd/aesm.socket"
        });

        assert_eq!(caps.requested_mode, TeeRuntimeMode::Simulation);
        assert_eq!(caps.detected_type, TeeType::IntelSgx);
        assert!(caps.hardware_available);
        assert!(caps.remote_attestation_available);
    }

    #[test]
    fn rejects_hardware_mode_without_detected_hardware() {
        let error =
            validate_runtime_requirements_with(TeeRuntimeMode::Hardware, |_| false).unwrap_err();

        assert!(matches!(
            error,
            TeeRuntimeError::HardwareUnavailable {
                requested_mode: TeeRuntimeMode::Hardware,
                detected_type: TeeType::None,
            }
        ));
    }

    #[test]
    fn rejects_hardware_mode_without_remote_attestation_prerequisites() {
        let error = validate_runtime_requirements_with(TeeRuntimeMode::Hardware, |path| {
            path == "/dev/sgx_enclave"
        })
        .unwrap_err();

        assert!(matches!(
            error,
            TeeRuntimeError::RemoteAttestationUnavailable {
                requested_mode: TeeRuntimeMode::Hardware,
                detected_type: TeeType::IntelSgx,
            }
        ));
    }

    #[test]
    fn test_secure_zero() {
        let mut data = vec![0x42u8; 32];
        secure_zero(&mut data);
        assert!(data.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_secure_random_bytes() {
        let bytes1 = secure_random_bytes(32).unwrap();
        let bytes2 = secure_random_bytes(32).unwrap();

        assert_eq!(bytes1.len(), 32);
        assert_eq!(bytes2.len(), 32);
        assert_ne!(bytes1, bytes2); // 随机值应该不同
    }

    #[test]
    fn test_tee_version() {
        assert_eq!(TEE_VERSION, "0.1.0");
    }

    #[test]
    fn test_default_user_key_ttl() {
        assert_eq!(DEFAULT_USER_KEY_TTL, 300);
    }
}
