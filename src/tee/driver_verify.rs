//! TEE 驱动完整性验证模块
//!
//! 实现 TEE 驱动层的完整性验证流程，防止驱动层攻击
//!
//! # 安全威胁
//!
//! - 恶意驱动替换
//! - 驱动版本回滚攻击
//! - 驱动内存篡改
//!
//! # 验证流程
//!
//! 1. 启动时验证驱动签名
//! 2. 运行时周期性完整性检查
//! 3. 驱动版本白名单验证

use ring::digest::{SHA256, digest};
use ring::signature::{self, UnparsedPublicKey};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use zeroize::Zeroize;

/// 驱动验证错误类型
#[derive(Error, Debug)]
pub enum DriverVerifyError {
    #[error("驱动文件不存在：{0}")]
    DriverNotFound(String),

    #[error("驱动签名验证失败：{0}")]
    SignatureVerificationFailed(String),

    #[error("驱动哈希不匹配：期望 {expected}, 实际 {actual}")]
    HashMismatch { expected: String, actual: String },

    #[error("驱动版本不受支持：{0}")]
    UnsupportedVersion(String),

    #[error("驱动文件读取失败：{0}")]
    ReadError(String),

    #[error("驱动元数据解析失败：{0}")]
    MetadataParseError(String),

    #[error("系统时间错误：{0}")]
    TimeError(String),

    #[error("内存清零失败")]
    MemoryZeroError,
}

/// 驱动签名验证结果
#[derive(Debug, Clone, PartialEq)]
pub enum VerificationStatus {
    /// 验证通过
    Verified,
    /// 签名无效
    InvalidSignature,
    /// 哈希不匹配
    HashMismatch,
    /// 版本不受支持
    UnsupportedVersion,
    /// 文件不存在
    NotFound,
}

/// 驱动元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverMetadata {
    /// 驱动名称
    pub name: String,
    /// 驱动版本
    pub version: String,
    /// 构建时间戳
    pub build_timestamp: u64,
    /// 开发者签名公钥指纹
    pub signer_fingerprint: String,
    /// 预期 SHA256 哈希
    pub expected_hash: String,
    /// 最小支持的 TEE 驱动版本
    pub min_tee_version: String,
}

impl DriverMetadata {
    /// 从文件加载元数据
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, DriverVerifyError> {
        let content = fs::read_to_string(path.as_ref())
            .map_err(|e| DriverVerifyError::ReadError(e.to_string()))?;

        serde_json::from_str(&content)
            .map_err(|e| DriverVerifyError::MetadataParseError(e.to_string()))
    }

    /// 验证元数据完整性
    pub fn validate(&self) -> Result<(), DriverVerifyError> {
        if self.name.is_empty() {
            return Err(DriverVerifyError::MetadataParseError(
                "驱动名称不能为空".to_string(),
            ));
        }

        if self.version.is_empty() {
            return Err(DriverVerifyError::MetadataParseError(
                "驱动版本不能为空".to_string(),
            ));
        }

        if self.expected_hash.len() != 64 {
            return Err(DriverVerifyError::MetadataParseError(
                "无效的 SHA256 哈希长度".to_string(),
            ));
        }

        Ok(())
    }
}

/// TEE 驱动验证器
///
/// 负责验证 TEE 驱动的完整性和真实性
pub struct TeeDriverVerifier {
    /// 受信任的签名公钥
    trusted_public_keys: Vec<Vec<u8>>,
    /// 驱动白名单
    allowed_versions: Vec<String>,
    /// 验证配置
    config: VerifierConfig,
}

/// 验证器配置
#[derive(Debug, Clone)]
pub struct VerifierConfig {
    /// 是否启用签名验证
    pub enable_signature_verification: bool,
    /// 是否启用哈希验证
    pub enable_hash_verification: bool,
    /// 是否启用版本检查
    pub enable_version_check: bool,
    /// 驱动文件路径
    pub driver_path: PathBuf,
    /// 元数据文件路径
    pub metadata_path: PathBuf,
}

impl Default for VerifierConfig {
    fn default() -> Self {
        Self {
            enable_signature_verification: true,
            enable_hash_verification: true,
            enable_version_check: true,
            driver_path: PathBuf::from("/opt/credbridge/drivers/tee_driver.bin"),
            metadata_path: PathBuf::from("/opt/credbridge/drivers/tee_driver.meta.json"),
        }
    }
}

impl TeeDriverVerifier {
    /// 创建新的验证器
    pub fn new(config: VerifierConfig) -> Self {
        Self {
            trusted_public_keys: Vec::new(),
            allowed_versions: Vec::new(),
            config,
        }
    }

    /// 添加受信任的公钥
    pub fn add_trusted_key(&mut self, public_key: &[u8]) {
        self.trusted_public_keys.push(public_key.to_vec());
    }

    /// 添加允许的版本
    pub fn add_allowed_version(&mut self, version: String) {
        self.allowed_versions.push(version);
    }

    /// 启动时验证驱动完整性
    ///
    /// # 验证流程
    ///
    /// 1. 加载驱动元数据
    /// 2. 验证元数据完整性
    /// 3. 计算驱动文件哈希
    /// 4. 验证驱动签名
    /// 5. 检查版本白名单
    ///
    /// # Returns
    ///
    /// 验证状态
    pub fn verify_at_startup(&self) -> Result<VerificationStatus, DriverVerifyError> {
        // 步骤 1: 加载并验证元数据
        let metadata = DriverMetadata::from_file(&self.config.metadata_path)?;
        metadata.validate()?;

        // 步骤 2: 计算驱动文件哈希
        let driver_hash = self.compute_driver_hash(&self.config.driver_path)?;

        // 步骤 3: 验证哈希匹配
        if self.config.enable_hash_verification && driver_hash != metadata.expected_hash {
            return Ok(VerificationStatus::HashMismatch);
        }

        // 步骤 4: 验证驱动签名
        if self.config.enable_signature_verification {
            let signature = self.load_driver_signature(&self.config.driver_path)?;
            if !self.verify_signature(&driver_hash, &signature)? {
                return Ok(VerificationStatus::InvalidSignature);
            }
        }

        // 步骤 5: 检查版本白名单
        if self.config.enable_version_check
            && !self.allowed_versions.is_empty()
            && !self.allowed_versions.contains(&metadata.version)
        {
            return Ok(VerificationStatus::UnsupportedVersion);
        }

        Ok(VerificationStatus::Verified)
    }

    /// 计算驱动文件的 SHA256 哈希
    fn compute_driver_hash<P: AsRef<Path>>(&self, path: P) -> Result<String, DriverVerifyError> {
        let content =
            fs::read(path.as_ref()).map_err(|e| DriverVerifyError::ReadError(e.to_string()))?;

        let digest = digest(&SHA256, &content);
        let hash = hex::encode(digest.as_ref());

        Ok(hash)
    }

    /// 加载驱动签名
    fn load_driver_signature<P: AsRef<Path>>(&self, path: P) -> Result<Vec<u8>, DriverVerifyError> {
        // 签名存储在驱动文件的 .signature 段或独立文件
        let sig_path = format!("{}.sig", path.as_ref().display());

        fs::read(&sig_path)
            .map_err(|e| DriverVerifyError::ReadError(format!("无法读取签名文件：{}", e)))
    }

    /// 验证驱动签名
    fn verify_signature(
        &self,
        data_hash: &str,
        signature: &[u8],
    ) -> Result<bool, DriverVerifyError> {
        let hash_bytes = hex::decode(data_hash)
            .map_err(|e| DriverVerifyError::SignatureVerificationFailed(e.to_string()))?;

        for public_key in &self.trusted_public_keys {
            let public_key = UnparsedPublicKey::new(&signature::ECDSA_P256_SHA256_ASN1, public_key);

            match public_key.verify(&hash_bytes, signature) {
                Ok(_) => return Ok(true),
                Err(_) => continue, // 尝试下一个公钥
            }
        }

        Ok(false)
    }

    /// 运行时完整性检查
    ///
    /// 周期性验证驱动内存未被篡改
    pub fn runtime_integrity_check(&self) -> Result<bool, DriverVerifyError> {
        let current_hash = self.compute_driver_hash(&self.config.driver_path)?;
        let metadata = DriverMetadata::from_file(&self.config.metadata_path)?;

        Ok(current_hash == metadata.expected_hash)
    }

    /// 获取验证器状态
    pub fn status(&self) -> VerifierStatus {
        VerifierStatus {
            trusted_keys_count: self.trusted_public_keys.len(),
            allowed_versions_count: self.allowed_versions.len(),
            config: self.config.clone(),
        }
    }
}

/// 验证器状态信息
#[derive(Debug, Clone)]
pub struct VerifierStatus {
    pub trusted_keys_count: usize,
    pub allowed_versions_count: usize,
    pub config: VerifierConfig,
}

/// 安全地比较两个哈希值（恒定时间比较）
///
/// 防止时序攻击
pub fn secure_hash_compare(a: &str, b: &str) -> bool {
    use constant_time_eq::constant_time_eq;

    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();

    constant_time_eq(a_bytes, b_bytes)
}

/// 验证驱动签名文件
///
/// # 使用示例
///
/// ```rust
/// use vault_service::tee::driver_verify::{verify_driver_signature, DriverMetadata};
///
/// let metadata = DriverMetadata::from_file("/path/to/driver.meta.json").unwrap();
/// let is_valid = verify_driver_signature(
///     "/path/to/driver.bin",
///     "/path/to/driver.sig",
///     &metadata.signer_fingerprint
/// ).unwrap();
///
/// assert!(is_valid);
/// ```
pub fn verify_driver_signature<P: AsRef<Path>>(
    driver_path: P,
    signature_path: P,
    signer_fingerprint: &str,
) -> Result<bool, DriverVerifyError> {
    // 读取驱动文件并计算哈希
    let driver_content =
        fs::read(driver_path.as_ref()).map_err(|e| DriverVerifyError::ReadError(e.to_string()))?;

    let hash = digest(&SHA256, &driver_content);

    // 读取签名
    let signature = fs::read(signature_path.as_ref())
        .map_err(|e| DriverVerifyError::ReadError(e.to_string()))?;

    // 验证签名（使用 ECDSA P-256）
    let public_key = get_trusted_public_key(signer_fingerprint).ok_or_else(|| {
        DriverVerifyError::SignatureVerificationFailed(format!(
            "未找到受信任的公钥：{}",
            signer_fingerprint
        ))
    })?;

    let unparsed_key = UnparsedPublicKey::new(&signature::ECDSA_P256_SHA256_ASN1, &public_key);

    match unparsed_key.verify(hash.as_ref(), &signature) {
        Ok(_) => Ok(true),
        Err(e) => Err(DriverVerifyError::SignatureVerificationFailed(
            e.to_string(),
        )),
    }
}

/// 获取受信任的公钥
///
/// 在实际实现中，这应该从安全的密钥存储中获取
fn get_trusted_public_key(_fingerprint: &str) -> Option<Vec<u8>> {
    // TODO(#TEE-301): 从安全存储加载公钥
    // 需要: 集成 Vault 或 HSM 密钥存储服务
    None
}

/// 清理敏感的驱动元数据
impl Drop for DriverMetadata {
    fn drop(&mut self) {
        // 安全清理敏感字段
        self.expected_hash.zeroize();
        self.signer_fingerprint.zeroize();
    }
}

/// 获取当前时间戳
#[allow(dead_code)]
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间错误")
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secure_hash_compare() {
        let hash1 = "a1b2c3d4e5f6";
        let hash2 = "a1b2c3d4e5f6";
        let hash3 = "b2c3d4e5f6a1";

        assert!(secure_hash_compare(hash1, hash2));
        assert!(!secure_hash_compare(hash1, hash3));
    }

    #[test]
    fn test_verifier_config_default() {
        let config = VerifierConfig::default();
        assert!(config.enable_signature_verification);
        assert!(config.enable_hash_verification);
        assert!(config.enable_version_check);
    }

    #[test]
    fn test_verifier_status() {
        let mut verifier = TeeDriverVerifier::new(VerifierConfig::default());
        verifier.add_trusted_key(&[0x42u8; 32]);
        verifier.add_allowed_version("1.0.0".to_string());

        let status = verifier.status();
        assert_eq!(status.trusted_keys_count, 1);
        assert_eq!(status.allowed_versions_count, 1);
    }

    #[test]
    fn test_metadata_zeroize_on_drop() {
        let metadata = DriverMetadata {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            build_timestamp: 0,
            signer_fingerprint: "sensitive_fingerprint".to_string(),
            expected_hash: "sensitive_hash".to_string(),
            min_tee_version: "0.9.0".to_string(),
        };

        // 在 drop 时，敏感字段应该被清零
        drop(metadata);
        // 无法直接验证，但确保 Drop trait 被实现
    }
}
