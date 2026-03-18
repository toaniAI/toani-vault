//! 增强的 L1 主密钥派生模块
//!
//! 实现多源密钥派生公式：HKDF(硬件测量值 || 用户密码派生值 || HSM 随机值)
//!
//! # 安全改进
//!
//! - 多源熵输入，防止单点故障
//! - 硬件测量值绑定到特定设备
//! - 用户密码派生增加用户因素
//! - HSM 随机值提供真随机熵源
//!
//! # 派生公式
//!
//! ```text
//! L1 = HKDF-SHA256(
//!   IKM = HardwareMeasurements || PasswordDerivation || HSMRandom,
//!   Salt = "CredBridge L1 Key Derivation v2.0",
//!   Info = "enclave-master-key-v2"
//! )
//! ```

use ring::digest::{SHA256, digest};
use ring::hkdf::{HKDF_SHA256, Salt};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use zeroize::Zeroize;

use super::constant_time::ct_compare;
use super::hkdf::KeyVersion;
use super::{CryptoError, KeyHandle};

/// 密钥派生错误
#[derive(Error, Debug)]
pub enum KeyDerivationError {
    #[error("硬件测量值获取失败：{0}")]
    HardwareMeasurementError(String),

    #[error("密码派生失败：{0}")]
    PasswordDerivationError(String),

    #[error("HSM 随机值获取失败：{0}")]
    HSMRandomError(String),

    #[error("密钥派生失败：{0}")]
    DerivationError(String),

    #[error("无效的输入参数：{0}")]
    InvalidInput(String),

    #[error("内存操作失败")]
    MemoryError,
}

impl From<CryptoError> for KeyDerivationError {
    fn from(err: CryptoError) -> Self {
        KeyDerivationError::DerivationError(err.to_string())
    }
}

impl From<ring::error::Unspecified> for KeyDerivationError {
    fn from(err: ring::error::Unspecified) -> Self {
        KeyDerivationError::DerivationError(err.to_string())
    }
}

/// 硬件测量值
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareMeasurements {
    /// CPU 序列号哈希
    pub cpu_id_hash: [u8; 32],
    /// 主板序列号哈希
    pub motherboard_hash: [u8; 32],
    /// TPM/SGX 测量值
    pub tee_measurement: [u8; 32],
    /// 内存指纹
    pub memory_fingerprint: [u8; 32],
}

impl HardwareMeasurements {
    /// 创建新的硬件测量值
    pub fn new(
        cpu_id_hash: [u8; 32],
        motherboard_hash: [u8; 32],
        tee_measurement: [u8; 32],
        memory_fingerprint: [u8; 32],
    ) -> Self {
        Self {
            cpu_id_hash,
            motherboard_hash,
            tee_measurement,
            memory_fingerprint,
        }
    }

    /// 从当前系统获取硬件测量值
    pub fn from_system() -> Result<Self, KeyDerivationError> {
        // 在实际实现中，这里会读取真实的硬件信息
        // 这里使用模拟值
        Ok(Self {
            cpu_id_hash: generate_simulated_hash("cpu"),
            motherboard_hash: generate_simulated_hash("motherboard"),
            tee_measurement: generate_simulated_hash("tee"),
            memory_fingerprint: generate_simulated_hash("memory"),
        })
    }

    /// 序列化为字节数组
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(128);
        bytes.extend_from_slice(&self.cpu_id_hash);
        bytes.extend_from_slice(&self.motherboard_hash);
        bytes.extend_from_slice(&self.tee_measurement);
        bytes.extend_from_slice(&self.memory_fingerprint);
        bytes
    }

    /// 验证测量值有效性
    pub fn validate(&self) -> bool {
        // 检查是否有全零的测量值（无效）
        !self.cpu_id_hash.iter().all(|&b| b == 0)
            && !self.motherboard_hash.iter().all(|&b| b == 0)
            && !self.tee_measurement.iter().all(|&b| b == 0)
            && !self.memory_fingerprint.iter().all(|&b| b == 0)
    }
}

impl Drop for HardwareMeasurements {
    fn drop(&mut self) {
        // 安全清理敏感数据
        self.cpu_id_hash.zeroize();
        self.motherboard_hash.zeroize();
        self.tee_measurement.zeroize();
        self.memory_fingerprint.zeroize();
    }
}

/// 密码派生值
#[derive(Debug, Clone)]
pub struct PasswordDerivation {
    /// PBKDF2 派生的密钥
    pub pbkdf2_key: [u8; 32],
    /// 盐值
    pub salt: [u8; 32],
    /// 迭代次数
    pub iterations: u32,
}

impl PasswordDerivation {
    /// 从密码派生密钥
    pub fn derive_from_password(password: &str, salt: &[u8]) -> Result<Self, KeyDerivationError> {
        if password.is_empty() {
            return Err(KeyDerivationError::InvalidInput("密码不能为空".to_string()));
        }

        if salt.len() < 16 {
            return Err(KeyDerivationError::InvalidInput(
                "盐值长度不足（至少 16 字节）".to_string(),
            ));
        }

        // 使用 PBKDF2-HMAC-SHA256 派生密钥
        let iterations = 100_000; // OWASP 推荐值
        let mut derived_key = [0u8; 32];

        // 在实际实现中使用 PBKDF2
        // 这里简化处理
        pbkdf2_hmac_sha256(password.as_bytes(), salt, iterations, &mut derived_key);

        let mut salt_array = [0u8; 32];
        salt_array[..salt.len()].copy_from_slice(salt);

        Ok(Self {
            pbkdf2_key: derived_key,
            salt: salt_array,
            iterations,
        })
    }

    /// 生成随机盐值
    pub fn generate_salt() -> [u8; 32] {
        let mut salt = [0u8; 32];
        get_random_bytes(&mut salt);
        salt
    }

    /// 验证派生值
    pub fn validate(&self) -> bool {
        !self.pbkdf2_key.iter().all(|&b| b == 0)
            && !self.salt.iter().all(|&b| b == 0)
            && self.iterations >= 10_000
    }
}

impl Drop for PasswordDerivation {
    fn drop(&mut self) {
        self.pbkdf2_key.zeroize();
        self.salt.zeroize();
    }
}

/// HSM 随机值
#[derive(Debug, Clone)]
pub struct HSMRandomValue {
    /// HSM 生成的随机字节
    pub random_bytes: [u8; 32],
    /// HSM 设备 ID
    pub hsm_device_id: String,
    /// 生成时间戳
    pub generated_at: u64,
}

impl HSMRandomValue {
    /// 从 HSM 获取随机值
    pub fn from_hsm() -> Result<Self, KeyDerivationError> {
        let mut random_bytes = [0u8; 32];

        // 在实际实现中从 HSM 获取真随机数
        // 这里使用模拟
        get_random_bytes(&mut random_bytes);

        Ok(Self {
            random_bytes,
            hsm_device_id: "hsm_001".to_string(),
            generated_at: current_timestamp(),
        })
    }

    /// 验证随机值
    pub fn validate(&self) -> bool {
        !self.random_bytes.iter().all(|&b| b == 0)
    }
}

impl Drop for HSMRandomValue {
    fn drop(&mut self) {
        self.random_bytes.zeroize();
    }
}

/// L1 主密钥派生器
pub struct L1KeyDeriver {
    /// 硬件测量值
    hardware_measurements: Option<HardwareMeasurements>,
    /// 密码派生值
    password_derivation: Option<PasswordDerivation>,
    /// HSM 随机值
    hsm_random: Option<HSMRandomValue>,
    /// 密钥版本
    key_version: KeyVersion,
}

impl L1KeyDeriver {
    /// 创建新的派生器
    pub fn new() -> Self {
        Self {
            hardware_measurements: None,
            password_derivation: None,
            hsm_random: None,
            key_version: KeyVersion::current(),
        }
    }

    /// 设置硬件测量值
    pub fn with_hardware_measurements(mut self, measurements: HardwareMeasurements) -> Self {
        self.hardware_measurements = Some(measurements);
        self
    }

    /// 设置密码派生值
    pub fn with_password_derivation(mut self, derivation: PasswordDerivation) -> Self {
        self.password_derivation = Some(derivation);
        self
    }

    /// 设置 HSM 随机值
    pub fn with_hsm_random(mut self, hsm_random: HSMRandomValue) -> Self {
        self.hsm_random = Some(hsm_random);
        self
    }

    /// 派生 L1 主密钥
    ///
    /// # 派生流程
    ///
    /// 1. 收集所有熵源
    /// 2. 拼接：Hardware || Password || HSM
    /// 3. HKDF-SHA256 派生
    /// 4. 安全清理临时数据
    ///
    /// # 返回
    /// L1 主密钥句柄
    pub fn derive_l1_key(&self) -> Result<KeyHandle, KeyDerivationError> {
        // 验证所有必需的熵源都已设置
        let hw = self
            .hardware_measurements
            .as_ref()
            .ok_or_else(|| KeyDerivationError::InvalidInput("硬件测量值未设置".to_string()))?;

        let pwd = self
            .password_derivation
            .as_ref()
            .ok_or_else(|| KeyDerivationError::InvalidInput("密码派生值未设置".to_string()))?;

        let hsm = self
            .hsm_random
            .as_ref()
            .ok_or_else(|| KeyDerivationError::InvalidInput("HSM 随机值未设置".to_string()))?;

        // 验证所有熵源有效
        if !hw.validate() {
            return Err(KeyDerivationError::HardwareMeasurementError(
                "无效的硬件测量值".to_string(),
            ));
        }

        if !pwd.validate() {
            return Err(KeyDerivationError::PasswordDerivationError(
                "无效的密码派生值".to_string(),
            ));
        }

        if !hsm.validate() {
            return Err(KeyDerivationError::HSMRandomError(
                "无效的 HSM 随机值".to_string(),
            ));
        }

        // 拼接所有熵源：Hardware || Password || HSM
        let mut ikm = Vec::with_capacity(96);
        ikm.extend_from_slice(&hw.to_bytes());
        ikm.extend_from_slice(&pwd.pbkdf2_key);
        ikm.extend_from_slice(&hsm.random_bytes);

        // HKDF-SHA256 派生
        let salt = Salt::new(HKDF_SHA256, b"CredBridge L1 Key Derivation v2.0");
        let prk = salt.extract(&ikm);

        let info = format!("enclave-master-key-v{}", self.key_version.major);
        let info_slice: &[u8] = info.as_bytes();
        let slices = [info_slice];
        let mut l1_key = [0u8; 32];

        let okm = prk.expand(&slices, HKDF_SHA256)?;
        okm.fill(&mut l1_key)?;

        // 生成密钥句柄
        let key_handle = generate_key_handle(&l1_key);

        Ok(key_handle)
    }

    /// 获取当前密钥版本
    pub fn key_version(&self) -> &KeyVersion {
        &self.key_version
    }
}

impl Default for L1KeyDeriver {
    fn default() -> Self {
        Self::new()
    }
}

/// 简化的密钥派生接口
///
/// # 参数
/// * `hardware_measurements` - 硬件测量值
/// * `password` - 用户密码
/// * `password_salt` - 密码盐值
/// * `hsm_random` - HSM 随机值
///
/// # 返回
/// L1 主密钥句柄
pub fn derive_l1_key_enhanced(
    hardware_measurements: &HardwareMeasurements,
    password: &str,
    password_salt: &[u8],
    hsm_random: &HSMRandomValue,
) -> Result<KeyHandle, KeyDerivationError> {
    // 派生密码密钥
    let password_derivation = PasswordDerivation::derive_from_password(password, password_salt)?;

    // 创建派生器
    let deriver = L1KeyDeriver::new()
        .with_hardware_measurements(hardware_measurements.clone())
        .with_password_derivation(password_derivation)
        .with_hsm_random(hsm_random.clone());

    // 派生 L1 密钥
    deriver.derive_l1_key()
}

/// 生成密钥句柄
fn generate_key_handle(key_material: &[u8]) -> KeyHandle {
    let hash = digest(&SHA256, key_material);
    let mut handle = [0u8; 32];
    handle.copy_from_slice(hash.as_ref());
    handle
}

/// PBKDF2-HMAC-SHA256 实现（简化版）
fn pbkdf2_hmac_sha256(password: &[u8], salt: &[u8], iterations: u32, output: &mut [u8]) {
    // 在实际实现中使用标准的 PBKDF2 实现
    // 这里简化处理，仅用于演示
    let mut data = Vec::new();
    data.extend_from_slice(password);
    data.extend_from_slice(salt);
    data.extend_from_slice(&iterations.to_be_bytes());

    let hash = digest(&SHA256, &data);
    let len = output.len().min(hash.as_ref().len());
    output[..len].copy_from_slice(&hash.as_ref()[..len]);
}

/// 获取随机字节
fn get_random_bytes(buffer: &mut [u8]) {
    // 在实际实现中使用密码学安全的随机数生成器
    // 这里简化处理
    use rand::RngCore;
    let mut rng = rand::thread_rng();
    rng.fill_bytes(buffer);
}

/// 生成模拟的硬件哈希（仅用于测试/模拟）
fn generate_simulated_hash(seed: &str) -> [u8; 32] {
    use ring::digest::{SHA256, digest};
    let hash = digest(&SHA256, seed.as_bytes());
    let mut result = [0u8; 32];
    result.copy_from_slice(hash.as_ref());
    result
}

/// 获取当前时间戳
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间错误")
        .as_secs()
}

/// 安全比较硬件测量值
pub fn secure_compare_hardware_measurements(
    a: &HardwareMeasurements,
    b: &HardwareMeasurements,
) -> bool {
    ct_compare(&a.to_bytes(), &b.to_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_hardware_measurements() -> HardwareMeasurements {
        HardwareMeasurements::new([0x01u8; 32], [0x02u8; 32], [0x03u8; 32], [0x04u8; 32])
    }

    fn create_test_password_derivation() -> PasswordDerivation {
        PasswordDerivation {
            pbkdf2_key: [0x05u8; 32],
            salt: [0x06u8; 32],
            iterations: 100_000,
        }
    }

    fn create_test_hsm_random() -> HSMRandomValue {
        HSMRandomValue {
            random_bytes: [0x07u8; 32],
            hsm_device_id: "test_hsm".to_string(),
            generated_at: current_timestamp(),
        }
    }

    #[test]
    fn test_hardware_measurements_validation() {
        let hw = create_test_hardware_measurements();
        assert!(hw.validate());

        let invalid_hw =
            HardwareMeasurements::new([0u8; 32], [0x02u8; 32], [0x03u8; 32], [0x04u8; 32]);
        assert!(!invalid_hw.validate());
    }

    #[test]
    fn test_password_derivation_validation() {
        let pwd = create_test_password_derivation();
        assert!(pwd.validate());

        let invalid_pwd = PasswordDerivation {
            pbkdf2_key: [0u8; 32],
            salt: [0x06u8; 32],
            iterations: 100_000,
        };
        assert!(!invalid_pwd.validate());
    }

    #[test]
    fn test_hsm_random_validation() {
        let hsm = create_test_hsm_random();
        assert!(hsm.validate());

        let invalid_hsm = HSMRandomValue {
            random_bytes: [0u8; 32],
            hsm_device_id: "test_hsm".to_string(),
            generated_at: current_timestamp(),
        };
        assert!(!invalid_hsm.validate());
    }

    #[test]
    fn test_l1_key_deriver() {
        let hw = create_test_hardware_measurements();
        let pwd = create_test_password_derivation();
        let hsm = create_test_hsm_random();

        let deriver = L1KeyDeriver::new()
            .with_hardware_measurements(hw)
            .with_password_derivation(pwd)
            .with_hsm_random(hsm);

        let key_handle = deriver.derive_l1_key().unwrap();

        // 验证句柄不为空
        assert_ne!(key_handle, [0u8; 32]);
    }

    #[test]
    fn test_l1_key_deriver_missing_entropy() {
        let deriver = L1KeyDeriver::new();

        // 缺少所有熵源
        let result = deriver.derive_l1_key();
        assert!(result.is_err());
    }

    #[test]
    fn test_derive_l1_key_enhanced() {
        let hw = create_test_hardware_measurements();
        let hsm = create_test_hsm_random();
        let password = "secure_password_123";
        let salt = PasswordDerivation::generate_salt();

        let key_handle = derive_l1_key_enhanced(&hw, password, &salt, &hsm).unwrap();
        assert_ne!(key_handle, [0u8; 32]);
    }

    #[test]
    fn test_password_derivation_from_password() {
        let password = "my_secure_password";
        let salt = PasswordDerivation::generate_salt();

        let derivation = PasswordDerivation::derive_from_password(password, &salt).unwrap();
        assert!(derivation.validate());
        assert_eq!(derivation.iterations, 100_000);
    }

    #[test]
    fn test_password_derivation_empty_password() {
        let salt = PasswordDerivation::generate_salt();
        let result = PasswordDerivation::derive_from_password("", &salt);
        assert!(result.is_err());
    }

    #[test]
    fn test_password_derivation_short_salt() {
        let short_salt = [0u8; 8];
        let result = PasswordDerivation::derive_from_password("password", &short_salt);
        assert!(result.is_err());
    }

    #[test]
    fn test_hardware_measurements_to_bytes() {
        let hw = create_test_hardware_measurements();
        let bytes = hw.to_bytes();

        assert_eq!(bytes.len(), 128); // 4 * 32 bytes
    }

    #[test]
    fn test_secure_compare_hardware_measurements() {
        let hw1 = create_test_hardware_measurements();
        let hw2 = create_test_hardware_measurements();
        let hw3 = HardwareMeasurements::new([0xFFu8; 32], [0x02u8; 32], [0x03u8; 32], [0x04u8; 32]);

        assert!(secure_compare_hardware_measurements(&hw1, &hw2));
        assert!(!secure_compare_hardware_measurements(&hw1, &hw3));
    }

    #[test]
    fn test_hsm_random_from_hsm() {
        let hsm = HSMRandomValue::from_hsm().unwrap();
        assert!(hsm.validate());
        assert!(!hsm.random_bytes.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_key_version_integration() {
        let hw = create_test_hardware_measurements();
        let pwd = create_test_password_derivation();
        let hsm = create_test_hsm_random();

        let deriver = L1KeyDeriver::new()
            .with_hardware_measurements(hw)
            .with_password_derivation(pwd)
            .with_hsm_random(hsm);

        assert_eq!(deriver.key_version().major, 1);
    }
}
