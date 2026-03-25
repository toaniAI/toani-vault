//! 密钥结构体定义
//!
//! 实现四层密钥层次架构（L0-L3），支持内存安全清理

use super::constants::KEY_LENGTH;
use super::{CryptoError, KeyHandle};
use crate::config::TeeRuntimeMode;
use rand::RngCore;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// L0: 硬件根密钥（SGX Sealing Key）
///
/// 从 Intel SGX 硬件获取的平台绑定密钥
/// 永不离开 TEE 边界，仅用于派生 L1 密钥
#[derive(ZeroizeOnDrop)]
pub struct HardwareRootKey {
    /// 密钥材料（32字节）
    #[zeroize]
    pub(crate) key_material: [u8; KEY_LENGTH],

    /// 密钥来源标识
    pub(crate) source: RootKeySource,

    /// MRSIGNER 测量值（用于验证 Enclave 签名者）
    pub(crate) mrsigner: [u8; 32],

    /// MRENCLAVE 测量值（用于验证 Enclave 身份）
    pub(crate) mrenclave: [u8; 32],
}

/// 根密钥来源
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootKeySource {
    /// Intel SGX Sealing Key
    SgxSealingKey,
    /// 模拟模式（用于开发和测试）
    Simulation,
    /// AMD SEV-SNP（未来扩展）
    SevSnp,
}

impl Zeroize for RootKeySource {
    fn zeroize(&mut self) {
        // 枚举类型的 zeroize 只需要重置为默认值即可
        *self = RootKeySource::Simulation;
    }
}

impl RootKeySource {
    pub fn as_str(self) -> &'static str {
        match self {
            RootKeySource::SgxSealingKey => "sgx_sealing_key",
            RootKeySource::Simulation => "simulation",
            RootKeySource::SevSnp => "sev_snp",
        }
    }
}

impl std::fmt::Display for RootKeySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl HardwareRootKey {
    /// 从 SGX Sealing Key 创建 L0 密钥
    ///
    /// # Safety
    /// 此函数应在 SGX Enclave 内调用，确保密钥材料不出安全边界
    pub fn from_sgx_sealing_key(sealing_key: [u8; KEY_LENGTH]) -> Self {
        Self {
            key_material: sealing_key,
            source: RootKeySource::SgxSealingKey,
            mrsigner: [0u8; 32],  // 实际实现中从 Enclave 报告获取
            mrenclave: [0u8; 32], // 实际实现中从 Enclave 报告获取
        }
    }

    /// 创建模拟模式的 L0 密钥（用于开发测试）
    ///
    /// # Warning
    /// 不要在生产环境使用！
    pub fn for_simulation() -> Result<Self, CryptoError> {
        let mut key_material = [0u8; KEY_LENGTH];
        let mut rng = rand::thread_rng();
        rng.try_fill_bytes(&mut key_material)
            .map_err(|_| CryptoError::RngError)?;

        Ok(Self {
            key_material,
            source: RootKeySource::Simulation,
            mrsigner: [0u8; 32],
            mrenclave: [0u8; 32],
        })
    }

    /// 按运行模式选择 L0 根密钥来源。
    ///
    /// `hardware` 模式必须接入真实硬件根密钥，禁止静默退回 simulation。
    pub fn for_runtime_mode(mode: TeeRuntimeMode) -> Result<Self, CryptoError> {
        match mode {
            TeeRuntimeMode::Simulation => Self::for_simulation(),
            TeeRuntimeMode::Hardware => Err(CryptoError::TeeError(
                "TEE_MODE=hardware requested, but simulation root key generation was attempted; wire a real hardware sealing key instead".to_string(),
            )),
        }
    }

    /// 获取密钥材料引用（用于派生）
    pub(crate) fn as_bytes(&self) -> &[u8; KEY_LENGTH] {
        &self.key_material
    }

    /// 获取密钥来源
    pub fn source(&self) -> RootKeySource {
        self.source
    }
}

/// L1: Enclave 主密钥
///
/// 在 TEE 内从 L0 派生，用于派生所有用户级密钥
/// 永不离开 Enclave 边界，Enclave 重启后从密封存储恢复
#[derive(ZeroizeOnDrop)]
pub struct EnclaveMasterKey {
    /// 密钥材料（32字节）
    #[zeroize]
    pub(crate) key_material: [u8; KEY_LENGTH],

    /// 密钥句柄（公开标识，不含密钥材料）
    pub(crate) key_handle: KeyHandle,

    /// 派生时间戳
    pub(crate) derived_at: u64,
}

impl EnclaveMasterKey {
    /// 从密钥材料和句柄创建 L1 密钥
    pub(crate) fn new(
        key_material: [u8; KEY_LENGTH],
        key_handle: KeyHandle,
        derived_at: u64,
    ) -> Self {
        Self {
            key_material,
            key_handle,
            derived_at,
        }
    }

    /// 获取密钥材料引用（用于派生 L2 密钥）
    pub(crate) fn as_bytes(&self) -> &[u8; KEY_LENGTH] {
        &self.key_material
    }

    /// 获取密钥句柄
    pub fn key_handle(&self) -> KeyHandle {
        self.key_handle
    }

    /// 生成新的密钥句柄
    pub(crate) fn generate_handle() -> KeyHandle {
        let mut handle = [0u8; 32];
        let mut rng = rand::thread_rng();
        rng.fill_bytes(&mut handle);
        handle
    }
}

/// L2: 用户保险库密钥
///
/// 每用户独立的密钥，用于派生凭证加密密钥
/// 在内存中临时缓存（TTL: 5分钟），定期轮换
#[derive(ZeroizeOnDrop)]
pub struct UserVaultKey {
    /// 密钥材料（32字节）
    #[zeroize]
    pub(crate) key_material: [u8; KEY_LENGTH],

    /// 租户ID
    #[zeroize(skip)]
    pub(crate) tenant_id: String,

    /// 用户ID（哈希后）
    #[zeroize(skip)]
    pub(crate) user_id_hash: String,

    /// 密钥句柄
    pub(crate) key_handle: KeyHandle,

    /// 派生时间戳（用于 TTL 检查）
    #[zeroize(skip)]
    pub(crate) derived_at: u64,

    /// 最后访问时间戳
    #[zeroize(skip)]
    pub(crate) last_accessed_at: u64,
}

impl UserVaultKey {
    /// 创建 L2 密钥
    pub(crate) fn new(
        key_material: [u8; KEY_LENGTH],
        tenant_id: String,
        user_id_hash: String,
        key_handle: KeyHandle,
        derived_at: u64,
    ) -> Self {
        Self {
            key_material,
            tenant_id,
            user_id_hash,
            key_handle,
            derived_at,
            last_accessed_at: derived_at,
        }
    }

    /// 获取密钥材料引用（用于派生 L3 密钥）
    pub(crate) fn as_bytes(&self) -> &[u8; KEY_LENGTH] {
        &self.key_material
    }

    /// 获取密钥句柄
    pub fn key_handle(&self) -> KeyHandle {
        self.key_handle
    }

    /// 获取租户ID
    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }

    /// 获取用户ID哈希
    pub fn user_id_hash(&self) -> &str {
        &self.user_id_hash
    }

    /// 更新最后访问时间
    pub fn touch(&mut self, timestamp: u64) {
        self.last_accessed_at = timestamp;
    }

    /// 检查密钥是否过期（TTL: 5分钟）
    pub fn is_expired(&self, current_time: u64, ttl_seconds: u64) -> bool {
        current_time.saturating_sub(self.last_accessed_at) > ttl_seconds
    }
}

/// L3: 凭证加密密钥
///
/// 每条凭证独立的加密密钥
/// 一次一密，用完即焚，不持久化存储
#[derive(ZeroizeOnDrop)]
pub struct CredentialKey {
    /// 密钥材料（32字节）
    #[zeroize]
    pub(crate) key_material: [u8; KEY_LENGTH],

    /// 租户ID
    #[zeroize(skip)]
    pub(crate) tenant_id: String,

    /// 用户ID哈希
    #[zeroize(skip)]
    pub(crate) user_id_hash: String,

    /// 凭证ID
    #[zeroize(skip)]
    pub(crate) credential_id: String,

    /// 密钥用途
    #[zeroize(skip)]
    pub(crate) purpose: KeyPurpose,

    /// 派生时间戳
    #[zeroize(skip)]
    pub(crate) derived_at: u64,
}

/// 密钥用途枚举
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KeyPurpose {
    /// 凭证内容加密
    CredentialEncryption,
    /// 凭证内容解密
    CredentialDecryption,
    /// 签名操作
    Signing,
    /// 派生子密钥
    KeyDerivation,
}

impl KeyPurpose {
    /// 获取用途字符串标识
    pub fn as_str(&self) -> &'static str {
        match self {
            KeyPurpose::CredentialEncryption => "encrypt",
            KeyPurpose::CredentialDecryption => "decrypt",
            KeyPurpose::Signing => "sign",
            KeyPurpose::KeyDerivation => "derive",
        }
    }
}

impl CredentialKey {
    /// 创建 L3 密钥
    pub(crate) fn new(
        key_material: [u8; KEY_LENGTH],
        tenant_id: String,
        user_id_hash: String,
        credential_id: String,
        purpose: KeyPurpose,
        derived_at: u64,
    ) -> Self {
        Self {
            key_material,
            tenant_id,
            user_id_hash,
            credential_id,
            purpose,
            derived_at,
        }
    }

    /// 获取密钥材料引用（用于加密/解密）
    pub(crate) fn as_bytes(&self) -> &[u8; KEY_LENGTH] {
        &self.key_material
    }

    /// 获取凭证ID
    pub fn credential_id(&self) -> &str {
        &self.credential_id
    }

    /// 获取密钥用途
    pub fn purpose(&self) -> KeyPurpose {
        self.purpose
    }
}

/// 密钥引用（用于缓存管理，不含实际密钥材料）
#[derive(Debug, Clone)]
pub struct KeyReference {
    pub key_handle: KeyHandle,
    pub key_type: KeyType,
    pub tenant_id: String,
    pub user_id_hash: Option<String>,
    pub created_at: u64,
    pub expires_at: u64,
}

/// 密钥类型枚举
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KeyType {
    HardwareRoot,
    EnclaveMaster,
    UserVault,
    Credential,
}

/// 安全密钥生成器
pub struct SecureKeyGenerator;

impl SecureKeyGenerator {
    /// 生成安全的随机密钥材料
    pub fn generate_random_key() -> Result<[u8; KEY_LENGTH], CryptoError> {
        let mut key = [0u8; KEY_LENGTH];
        let mut rng = rand::thread_rng();
        rng.try_fill_bytes(&mut key)
            .map_err(|_| CryptoError::RngError)?;
        Ok(key)
    }

    /// 生成随机 nonce（用于 AES-GCM）
    pub fn generate_nonce() -> Result<[u8; 12], CryptoError> {
        let mut nonce = [0u8; 12];
        let mut rng = rand::thread_rng();
        rng.try_fill_bytes(&mut nonce)
            .map_err(|_| CryptoError::RngError)?;
        Ok(nonce)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hardware_root_key_simulation() {
        let key = HardwareRootKey::for_simulation().unwrap();
        assert_eq!(key.key_material.len(), KEY_LENGTH);
        assert_eq!(key.source, RootKeySource::Simulation);
    }

    #[test]
    fn test_hardware_root_key_for_runtime_mode_rejects_hardware() {
        let error = match HardwareRootKey::for_runtime_mode(TeeRuntimeMode::Hardware) {
            Ok(_) => panic!("expected hardware root key generation to be rejected in this build"),
            Err(e) => e,
        };
        assert!(
            error
                .to_string()
                .contains("simulation root key generation was attempted")
        );
    }

    #[test]
    fn test_enclave_master_key() {
        let key_material = [0x42u8; KEY_LENGTH];
        let handle = EnclaveMasterKey::generate_handle();
        let key = EnclaveMasterKey::new(key_material, handle, 1234567890);

        assert_eq!(key.key_handle(), handle);
        assert_eq!(key.derived_at, 1234567890);
    }

    #[test]
    fn test_user_vault_key_expiration() {
        let key_material = [0x42u8; KEY_LENGTH];
        let key = UserVaultKey::new(
            key_material,
            "tenant_123".to_string(),
            "user_hash_abc".to_string(),
            [0u8; 32],
            1000,
        );

        // TTL = 300秒 (5分钟)
        assert!(!key.is_expired(1200, 300)); // 200秒 < 300秒，未过期
        assert!(key.is_expired(1500, 300)); // 500秒 > 300秒，已过期
    }

    #[test]
    fn test_credential_key_purpose() {
        assert_eq!(KeyPurpose::CredentialEncryption.as_str(), "encrypt");
        assert_eq!(KeyPurpose::CredentialDecryption.as_str(), "decrypt");
        assert_eq!(KeyPurpose::Signing.as_str(), "sign");
        assert_eq!(KeyPurpose::KeyDerivation.as_str(), "derive");
    }

    #[test]
    fn test_secure_key_generator() {
        let key1 = SecureKeyGenerator::generate_random_key().unwrap();
        let key2 = SecureKeyGenerator::generate_random_key().unwrap();

        // 生成的密钥应该是随机的
        assert_ne!(key1, key2);
        assert_eq!(key1.len(), KEY_LENGTH);
    }
}
