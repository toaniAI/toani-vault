//! CredBridge 加密模块
//!
//! 实现四层密钥层次架构（L0-L3）和 AES-256-GCM 加密

pub mod cipher;
pub mod constant_time;
pub mod enclave_key;
pub mod hkdf;
pub mod key_derivation;
pub mod key_rotation;
pub mod keys;

pub use cipher::{EncryptedBlob, decrypt_credential, encrypt_credential};
pub use enclave_key::{EnclaveKeyManager, KeyAlgorithm, KeyConfig, KeyState, Signature};
pub use hkdf::KeyHierarchy;
pub use keys::{CredentialKey, EnclaveMasterKey, HardwareRootKey, KeyPurpose, UserVaultKey};

use thiserror::Error;

/// 加密错误类型
#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("密钥派生失败: {0}")]
    KeyDerivationError(String),

    #[error("加密失败: {0}")]
    EncryptionError(String),

    #[error("解密失败: {0}")]
    DecryptionError(String),

    #[error("无效的密钥长度: 期望 {expected}, 实际 {actual}")]
    InvalidKeyLength { expected: usize, actual: usize },

    #[error("无效的密文格式")]
    InvalidCiphertext,

    #[error("认证标签验证失败")]
    AuthenticationFailed,

    #[error("TEE 错误: {0}")]
    TeeError(String),

    #[error("随机数生成失败")]
    RngError,
}

impl From<ring::error::Unspecified> for CryptoError {
    fn from(_: ring::error::Unspecified) -> Self {
        CryptoError::EncryptionError("Ring 操作失败".to_string())
    }
}

/// 密钥句柄类型（公开标识，不含密钥材料）
pub type KeyHandle = [u8; 32];

/// 密钥常量定义
pub mod constants {
    /// AES-256 密钥长度（字节）
    pub const KEY_LENGTH: usize = 32;

    /// AES-GCM nonce 长度（96-bit = 12 bytes）
    pub const NONCE_LENGTH: usize = 12;

    /// AES-GCM auth tag 长度（128-bit = 16 bytes）
    pub const AUTH_TAG_LENGTH: usize = 16;

    /// HKDF 盐值长度
    pub const HKDF_SALT_LENGTH: usize = 32;

    /// HKDF 信息字符串最大长度
    pub const HKDF_INFO_MAX_LENGTH: usize = 256;

    /// 密钥派生算法标识
    pub const KDF_HKDF_SHA256: &str = "HKDF-SHA-256";

    /// 加密算法标识
    pub const ALGORITHM_AES_256_GCM: &str = "AES-256-GCM";

    /// 协议版本
    pub const PROTOCOL_VERSION: u8 = 2;
}

#[cfg(test)]
mod tests {
    #![allow(unused_imports)]
    use super::constants::*;
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(KEY_LENGTH, 32);
        assert_eq!(NONCE_LENGTH, 12);
        assert_eq!(AUTH_TAG_LENGTH, 16);
    }
}
