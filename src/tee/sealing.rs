//! SGX Sealing 模块
//!
//! 实现 SGX Sealing 密钥获取和密封数据存储
//!
//! # 安全说明
//!
//! - Sealing Key 从 Intel SGX 硬件获取，平台绑定
//! - 密封数据只能在相同 Enclave 测量值（MRENCLAVE/MRSIGNER）下解封
//! - 支持两种密封策略：MRENCLAVE（严格）和 MRSIGNER（兼容）
//!
//! # 密封策略
//!
//! - `Mrenclave`: 仅当前版本 Enclave 可解封（严格模式）
//! - `Mrsigner`: 同一签名者的不同版本 Enclave 可解封（兼容模式）

use crate::crypto::CryptoError;
use crate::crypto::constants::KEY_LENGTH;
use zeroize::ZeroizeOnDrop;

/// 密封策略
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SealPolicy {
    /// MRENCLAVE 密封：仅相同 Enclave 可解封
    Mrenclave,
    /// MRSIGNER 密封：相同签名者的 Enclave 可解封
    Mrsigner,
}

impl SealPolicy {
    /// 获取策略字符串标识
    pub fn as_str(&self) -> &'static str {
        match self {
            SealPolicy::Mrenclave => "MRENCLAVE",
            SealPolicy::Mrsigner => "MRSIGNER",
        }
    }
}

/// SGX Sealing Key（L0 硬件根密钥）
///
/// 从 Intel SGX 硬件获取的平台绑定密钥
/// 永不离开 TEE 边界
#[derive(ZeroizeOnDrop, Clone)]
pub struct SealingKey {
    /// 密钥材料（32字节）
    #[zeroize]
    pub(crate) key_material: [u8; KEY_LENGTH],

    /// 密封策略
    #[zeroize(skip)]
    pub(crate) policy: SealPolicy,

    /// CPU SVN（安全版本号）
    #[zeroize(skip)]
    pub(crate) cpusvn: [u8; 16],

    /// ISV SVN（安全版本号）
    #[zeroize(skip)]
    pub(crate) isvsvn: u16,
}

impl SealingKey {
    /// 创建新的 Sealing Key
    pub(crate) fn new(
        key_material: [u8; KEY_LENGTH],
        policy: SealPolicy,
        cpusvn: [u8; 16],
        isvsvn: u16,
    ) -> Self {
        Self {
            key_material,
            policy,
            cpusvn,
            isvsvn,
        }
    }

    /// 获取密钥材料引用
    pub(crate) fn as_bytes(&self) -> &[u8; KEY_LENGTH] {
        &self.key_material
    }

    /// 获取密封策略
    pub fn policy(&self) -> SealPolicy {
        self.policy
    }

    /// 获取 CPU SVN
    pub fn cpusvn(&self) -> &[u8; 16] {
        &self.cpusvn
    }

    /// 获取 ISV SVN
    pub fn isvsvn(&self) -> u16 {
        self.isvsvn
    }
}

/// 密封数据包
///
/// 存储在外部介质（磁盘）的加密数据格式
#[derive(Debug, Clone)]
pub struct SealedData {
    /// 密封策略
    pub policy: SealPolicy,

    /// CPU SVN
    pub cpusvn: [u8; 16],

    /// ISV SVN
    pub isvsvn: u16,

    /// 密钥请求元数据（用于重新派生解封密钥）
    pub key_request: Vec<u8>,

    /// 加密后的密文
    pub ciphertext: Vec<u8>,

    /// MAC 认证标签
    pub mac: [u8; 16],

    /// 额外认证数据（AAD）
    pub aad: Vec<u8>,

    /// 创建时间戳
    pub created_at: u64,

    /// 版本号（用于向后兼容）
    pub version: u16,
}

/// 密封数据构建器
///
/// 用于构建 SealedData 的 Builder 模式实现
pub struct SealedDataBuilder {
    policy: SealPolicy,
    cpusvn: [u8; 16],
    isvsvn: u16,
    key_request: Vec<u8>,
    ciphertext: Vec<u8>,
    mac: [u8; 16],
    aad: Vec<u8>,
    created_at: u64,
}

impl SealedDataBuilder {
    /// 创建新的构建器
    pub fn new(policy: SealPolicy, cpusvn: [u8; 16], isvsvn: u16) -> Self {
        Self {
            policy,
            cpusvn,
            isvsvn,
            key_request: Vec::new(),
            ciphertext: Vec::new(),
            mac: [0u8; 16],
            aad: Vec::new(),
            created_at: 0,
        }
    }

    /// 设置密钥请求元数据
    pub fn key_request(mut self, key_request: Vec<u8>) -> Self {
        self.key_request = key_request;
        self
    }

    /// 设置密文
    pub fn ciphertext(mut self, ciphertext: Vec<u8>) -> Self {
        self.ciphertext = ciphertext;
        self
    }

    /// 设置 MAC
    pub fn mac(mut self, mac: [u8; 16]) -> Self {
        self.mac = mac;
        self
    }

    /// 设置 AAD
    pub fn aad(mut self, aad: Vec<u8>) -> Self {
        self.aad = aad;
        self
    }

    /// 设置创建时间戳
    pub fn created_at(mut self, created_at: u64) -> Self {
        self.created_at = created_at;
        self
    }

    /// 构建 SealedData
    pub fn build(self) -> SealedData {
        SealedData {
            policy: self.policy,
            cpusvn: self.cpusvn,
            isvsvn: self.isvsvn,
            key_request: self.key_request,
            ciphertext: self.ciphertext,
            mac: self.mac,
            aad: self.aad,
            created_at: self.created_at,
            version: 1,
        }
    }
}

impl SealedData {
    /// 创建新的密封数据包
    ///
    /// 推荐使用 SealedDataBuilder 来构建复杂实例
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        policy: SealPolicy,
        cpusvn: [u8; 16],
        isvsvn: u16,
        key_request: Vec<u8>,
        ciphertext: Vec<u8>,
        mac: [u8; 16],
        aad: Vec<u8>,
        created_at: u64,
    ) -> Self {
        Self {
            policy,
            cpusvn,
            isvsvn,
            key_request,
            ciphertext,
            mac,
            aad,
            created_at,
            version: 1,
        }
    }

    /// 创建密封数据构建器
    pub fn builder(policy: SealPolicy, cpusvn: [u8; 16], isvsvn: u16) -> SealedDataBuilder {
        SealedDataBuilder::new(policy, cpusvn, isvsvn)
    }

    /// 验证密封数据是否可以被当前 Enclave 解封
    pub fn can_unseal(&self, current_cpusvn: &[u8; 16], current_isvsvn: u16) -> bool {
        // 检查版本兼容性
        if self.version != 1 {
            return false;
        }

        // 检查 SVN 兼容性（当前 SVN >= 密封时的 SVN）
        if current_isvsvn < self.isvsvn {
            return false;
        }

        // CPU SVN 检查（每个字节都要 >=）
        for (current, sealed) in current_cpusvn.iter().zip(self.cpusvn.iter()) {
            if *current < *sealed {
                return false;
            }
        }

        true
    }

    /// 序列化为字节（用于存储）
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut result = Vec::new();

        // 版本号 (2 bytes)
        result.extend_from_slice(&self.version.to_le_bytes());

        // 策略 (1 byte)
        let policy_byte = match self.policy {
            SealPolicy::Mrenclave => 0u8,
            SealPolicy::Mrsigner => 1u8,
        };
        result.push(policy_byte);

        // CPU SVN (16 bytes)
        result.extend_from_slice(&self.cpusvn);

        // ISV SVN (2 bytes)
        result.extend_from_slice(&self.isvsvn.to_le_bytes());

        // 创建时间戳 (8 bytes)
        result.extend_from_slice(&self.created_at.to_le_bytes());

        // MAC (16 bytes)
        result.extend_from_slice(&self.mac);

        // key_request 长度 (4 bytes) + 数据
        let kr_len = self.key_request.len() as u32;
        result.extend_from_slice(&kr_len.to_le_bytes());
        result.extend_from_slice(&self.key_request);

        // ciphertext 长度 (4 bytes) + 数据
        let ct_len = self.ciphertext.len() as u32;
        result.extend_from_slice(&ct_len.to_le_bytes());
        result.extend_from_slice(&self.ciphertext);

        // aad 长度 (4 bytes) + 数据
        let aad_len = self.aad.len() as u32;
        result.extend_from_slice(&aad_len.to_le_bytes());
        result.extend_from_slice(&self.aad);

        result
    }

    /// 从字节反序列化
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() < 51 {
            // 最小长度检查
            return Err(CryptoError::InvalidCiphertext);
        }

        let mut offset = 0;

        // 版本号
        let _version = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        offset += 2;

        // 策略
        let policy = match bytes[offset] {
            0 => SealPolicy::Mrenclave,
            1 => SealPolicy::Mrsigner,
            _ => return Err(CryptoError::InvalidCiphertext),
        };
        offset += 1;

        // CPU SVN
        let mut cpusvn = [0u8; 16];
        cpusvn.copy_from_slice(&bytes[offset..offset + 16]);
        offset += 16;

        // ISV SVN
        let isvsvn = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        offset += 2;

        // 创建时间戳
        let created_at = u64::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]);
        offset += 8;

        // MAC
        let mut mac = [0u8; 16];
        mac.copy_from_slice(&bytes[offset..offset + 16]);
        offset += 16;

        // key_request
        let kr_len = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        offset += 4;
        if offset + kr_len > bytes.len() {
            return Err(CryptoError::InvalidCiphertext);
        }
        let key_request = bytes[offset..offset + kr_len].to_vec();
        offset += kr_len;

        // ciphertext
        let ct_len = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        offset += 4;
        if offset + ct_len > bytes.len() {
            return Err(CryptoError::InvalidCiphertext);
        }
        let ciphertext = bytes[offset..offset + ct_len].to_vec();
        offset += ct_len;

        // aad
        let aad_len = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        offset += 4;
        if offset + aad_len > bytes.len() {
            return Err(CryptoError::InvalidCiphertext);
        }
        let aad = bytes[offset..offset + aad_len].to_vec();

        Ok(Self::new(
            policy,
            cpusvn,
            isvsvn,
            key_request,
            ciphertext,
            mac,
            aad,
            created_at,
        ))
    }
}

/// Sealing 服务
///
/// 提供 Sealing Key 获取和密封/解封功能
pub struct SealingService {
    /// 当前 CPU SVN
    cpusvn: [u8; 16],

    /// 当前 ISV SVN
    isvsvn: u16,

    /// 密封数据存储路径
    storage_path: Option<String>,
}

impl SealingService {
    /// 创建新的 Sealing 服务
    pub fn new() -> Self {
        Self {
            cpusvn: [0u8; 16],
            isvsvn: 1,
            storage_path: None,
        }
    }

    /// 设置存储路径
    pub fn with_storage_path(mut self, path: String) -> Self {
        self.storage_path = Some(path);
        self
    }

    /// 获取 Sealing Key（模拟实现）
    ///
    /// 在真实 SGX 环境中，这将调用 EGETKEY 指令获取硬件密钥
    pub fn get_sealing_key(&self, policy: SealPolicy) -> Result<SealingKey, CryptoError> {
        // 模拟 Sealing Key 获取
        // 实际 SGX 实现中，这里会调用 Intel SGX SDK 的 sgx_get_key()
        let key_material = simulate_egetkey(&self.cpusvn, self.isvsvn, policy)?;

        Ok(SealingKey::new(
            key_material,
            policy,
            self.cpusvn,
            self.isvsvn,
        ))
    }

    /// 密封数据
    ///
    /// 使用 Sealing Key 加密数据，返回密封数据包
    pub fn seal_data(
        &self,
        plaintext: &[u8],
        aad: &[u8],
        policy: SealPolicy,
    ) -> Result<SealedData, CryptoError> {
        use aes_gcm::{
            Aes256Gcm,
            aead::{Aead, AeadCore, KeyInit, OsRng},
        };

        // 获取 Sealing Key
        let sealing_key = self.get_sealing_key(policy)?;

        // 使用 AES-256-GCM 加密
        let cipher = Aes256Gcm::new_from_slice(sealing_key.as_bytes())
            .map_err(|e| CryptoError::EncryptionError(e.to_string()))?;

        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

        // 使用 AAD 加密
        let ciphertext = cipher
            .encrypt(
                &nonce,
                aes_gcm::aead::Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|e| CryptoError::EncryptionError(e.to_string()))?;

        // 提取 MAC（最后16字节）
        let mut mac = [0u8; 16];
        if ciphertext.len() >= 16 {
            mac.copy_from_slice(&ciphertext[ciphertext.len() - 16..]);
        }

        // 将 nonce 和密文合并存储
        let mut full_ciphertext = Vec::with_capacity(12 + ciphertext.len());
        full_ciphertext.extend_from_slice(&nonce);
        full_ciphertext.extend_from_slice(&ciphertext);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // 构建 key_request 元数据
        let key_request = build_key_request(&self.cpusvn, self.isvsvn, policy);

        Ok(SealedData::new(
            policy,
            self.cpusvn,
            self.isvsvn,
            key_request,
            full_ciphertext,
            mac,
            aad.to_vec(),
            now,
        ))
    }

    /// 解封数据
    ///
    /// 使用 Sealing Key 解密密封数据包
    pub fn unseal_data(&self, sealed_data: &SealedData) -> Result<Vec<u8>, CryptoError> {
        use aes_gcm::{
            Aes256Gcm, Nonce,
            aead::{Aead, KeyInit},
        };

        // 检查是否可以解封
        if !sealed_data.can_unseal(&self.cpusvn, self.isvsvn) {
            return Err(CryptoError::DecryptionError(
                "无法解封：SVN 不兼容".to_string(),
            ));
        }

        // 获取对应策略的 Sealing Key
        let sealing_key = self.get_sealing_key(sealed_data.policy)?;

        // 解密
        let cipher = Aes256Gcm::new_from_slice(sealing_key.as_bytes())
            .map_err(|e| CryptoError::DecryptionError(e.to_string()))?;

        // 从 ciphertext 中提取 nonce（前12字节）和密文
        if sealed_data.ciphertext.len() < 12 {
            return Err(CryptoError::InvalidCiphertext);
        }

        let nonce = Nonce::from_slice(&sealed_data.ciphertext[..12]);
        let ciphertext = &sealed_data.ciphertext[12..];

        let plaintext = cipher
            .decrypt(
                nonce,
                aes_gcm::aead::Payload {
                    msg: ciphertext,
                    aad: &sealed_data.aad,
                },
            )
            .map_err(|_| CryptoError::AuthenticationFailed)?;

        Ok(plaintext)
    }

    /// 获取当前 CPU SVN
    pub fn cpusvn(&self) -> &[u8; 16] {
        &self.cpusvn
    }

    /// 获取当前 ISV SVN
    pub fn isvsvn(&self) -> u16 {
        self.isvsvn
    }
}

impl Default for SealingService {
    fn default() -> Self {
        Self::new()
    }
}

/// 模拟 EGETKEY 指令
///
/// 在开发/测试环境中模拟 SGX 硬件密钥派生
fn simulate_egetkey(
    cpusvn: &[u8; 16],
    isvsvn: u16,
    policy: SealPolicy,
) -> Result<[u8; KEY_LENGTH], CryptoError> {
    use ring::hmac;

    // 使用 HMAC-SHA256 模拟硬件密钥派生
    let key = hmac::Key::new(hmac::HMAC_SHA256, b"CredBridge Simulated Sealing Key Root");

    let policy_bytes = policy.as_str().as_bytes();
    let mut data = Vec::with_capacity(16 + 2 + policy_bytes.len());
    data.extend_from_slice(cpusvn);
    data.extend_from_slice(&isvsvn.to_le_bytes());
    data.extend_from_slice(policy_bytes);

    let tag = hmac::sign(&key, &data);

    let mut result = [0u8; KEY_LENGTH];
    result.copy_from_slice(tag.as_ref());

    Ok(result)
}

/// 构建密钥请求元数据
fn build_key_request(cpusvn: &[u8; 16], isvsvn: u16, policy: SealPolicy) -> Vec<u8> {
    let mut request = Vec::with_capacity(16 + 2 + 1);
    request.extend_from_slice(cpusvn);
    request.extend_from_slice(&isvsvn.to_le_bytes());
    request.push(match policy {
        SealPolicy::Mrenclave => 0,
        SealPolicy::Mrsigner => 1,
    });
    request
}

/// 密封存储管理器
///
/// 管理密封数据的持久化存储
pub struct SealedStorage {
    /// 存储路径
    path: String,

    /// Sealing 服务
    sealing: SealingService,
}

impl SealedStorage {
    /// 创建新的密封存储
    pub fn new(path: String) -> Self {
        Self {
            path,
            sealing: SealingService::new(),
        }
    }

    /// 存储密封数据
    pub fn store(&self, key: &str, data: &SealedData) -> Result<(), CryptoError> {
        let file_path = format!("{}/{}.sealed", self.path, key);
        let bytes = data.to_bytes();

        std::fs::write(&file_path, &bytes)
            .map_err(|e| CryptoError::EncryptionError(format!("写入密封数据失败: {}", e)))?;

        Ok(())
    }

    /// 读取密封数据
    pub fn load(&self, key: &str) -> Result<SealedData, CryptoError> {
        let file_path = format!("{}/{}.sealed", self.path, key);
        let bytes = std::fs::read(&file_path)
            .map_err(|e| CryptoError::DecryptionError(format!("读取密封数据失败: {}", e)))?;

        SealedData::from_bytes(&bytes)
    }

    /// 删除密封数据
    pub fn delete(&self, key: &str) -> Result<(), CryptoError> {
        let file_path = format!("{}/{}.sealed", self.path, key);
        std::fs::remove_file(&file_path)
            .map_err(|e| CryptoError::EncryptionError(format!("删除密封数据失败: {}", e)))?;

        Ok(())
    }

    /// 检查键是否存在
    pub fn exists(&self, key: &str) -> bool {
        let file_path = format!("{}/{}.sealed", self.path, key);
        std::path::Path::new(&file_path).exists()
    }

    /// 获取 Sealing 服务
    pub fn sealing(&self) -> &SealingService {
        &self.sealing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seal_policy() {
        assert_eq!(SealPolicy::Mrenclave.as_str(), "MRENCLAVE");
        assert_eq!(SealPolicy::Mrsigner.as_str(), "MRSIGNER");
    }

    #[test]
    fn test_sealing_key_creation() {
        let key_material = [0x42u8; 32];
        let cpusvn = [0x01u8; 16];
        let key = SealingKey::new(key_material, SealPolicy::Mrenclave, cpusvn, 1);

        assert_eq!(key.as_bytes(), &key_material);
        assert_eq!(key.policy(), SealPolicy::Mrenclave);
        assert_eq!(key.cpusvn(), &cpusvn);
        assert_eq!(key.isvsvn(), 1);
    }

    #[test]
    fn test_sealed_data_serialization() {
        let sealed = SealedData::new(
            SealPolicy::Mrsigner,
            [0x01u8; 16],
            1,
            vec![1, 2, 3],
            vec![4, 5, 6],
            [7u8; 16],
            vec![8, 9, 10],
            1234567890,
        );

        let bytes = sealed.to_bytes();
        let restored = SealedData::from_bytes(&bytes).unwrap();

        assert_eq!(restored.policy, sealed.policy);
        assert_eq!(restored.cpusvn, sealed.cpusvn);
        assert_eq!(restored.isvsvn, sealed.isvsvn);
        assert_eq!(restored.key_request, sealed.key_request);
        assert_eq!(restored.ciphertext, sealed.ciphertext);
        assert_eq!(restored.mac, sealed.mac);
        assert_eq!(restored.aad, sealed.aad);
        assert_eq!(restored.created_at, sealed.created_at);
    }

    #[test]
    fn test_sealed_data_can_unseal() {
        let sealed = SealedData::new(
            SealPolicy::Mrenclave,
            [0x01u8; 16],
            1,
            vec![1, 2, 3],
            vec![4, 5, 6],
            [7u8; 16],
            vec![8, 9, 10],
            1234567890,
        );

        // 相同 SVN 可以解封
        assert!(sealed.can_unseal(&[0x01u8; 16], 1));

        // 更高 SVN 可以解封（向前兼容）
        assert!(sealed.can_unseal(&[0x02u8; 16], 2));

        // 更低 SVN 不能解封
        assert!(!sealed.can_unseal(&[0x00u8; 16], 0));
    }

    #[test]
    fn test_sealing_service() {
        let service = SealingService::new();

        // 获取 Sealing Key
        let key1 = service.get_sealing_key(SealPolicy::Mrenclave).unwrap();
        let key2 = service.get_sealing_key(SealPolicy::Mrenclave).unwrap();

        // 相同配置应该产生相同的密钥
        assert_eq!(key1.as_bytes(), key2.as_bytes());

        // 不同策略应该产生不同的密钥
        let key3 = service.get_sealing_key(SealPolicy::Mrsigner).unwrap();
        assert_ne!(key1.as_bytes(), key3.as_bytes());
    }

    #[test]
    fn test_seal_and_unseal() {
        let service = SealingService::new();
        let plaintext = b"Hello, SGX Sealing!";
        let aad = b"additional authenticated data";

        // 密封数据
        let sealed = service
            .seal_data(plaintext, aad, SealPolicy::Mrsigner)
            .unwrap();

        // 解封数据
        let decrypted = service.unseal_data(&sealed).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_simulate_egetkey() {
        let cpusvn1 = [0x01u8; 16];
        let cpusvn2 = [0x02u8; 16];

        let key1 = simulate_egetkey(&cpusvn1, 1, SealPolicy::Mrenclave).unwrap();
        let key2 = simulate_egetkey(&cpusvn1, 1, SealPolicy::Mrenclave).unwrap();
        let key3 = simulate_egetkey(&cpusvn2, 1, SealPolicy::Mrenclave).unwrap();
        let key4 = simulate_egetkey(&cpusvn1, 1, SealPolicy::Mrsigner).unwrap();

        // 相同输入产生相同输出
        assert_eq!(key1, key2);

        // 不同 CPU SVN 产生不同密钥
        assert_ne!(key1, key3);

        // 不同策略产生不同密钥
        assert_ne!(key1, key4);
    }
}
