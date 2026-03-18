//! HKDF 密钥派生实现
//!
//! 实现 L0-L3 四层密钥层次架构
//! 使用 HKDF-SHA256 作为 KDF

use super::constants::KEY_LENGTH;
use super::keys::{CredentialKey, EnclaveMasterKey, HardwareRootKey, KeyPurpose, UserVaultKey};
use super::{CryptoError, KeyHandle};
use ring::hkdf::{HKDF_SHA256, Salt};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroize;

/// 密钥派生版本（用于前向保密轮换）
///
/// 版本号用于区分不同代的密钥，确保安全轮换
pub const KEY_DERIVATION_VERSION: u32 = 1;

/// 密钥轮换间隔（秒）
/// 默认：90 天
pub const KEY_ROTATION_INTERVAL_SECONDS: u64 = 90 * 24 * 60 * 60;

/// 密钥版本信息
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct KeyVersion {
    /// 主版本号（重大变更）
    pub major: u32,
    /// 次版本号（派生参数变更）
    pub minor: u32,
    /// 派生时间戳
    pub derived_at: u64,
}

impl KeyVersion {
    /// 创建新版本
    pub fn new(major: u32, minor: u32) -> Self {
        Self {
            major,
            minor,
            derived_at: current_timestamp(),
        }
    }

    /// 获取当前版本
    pub fn current() -> Self {
        Self::new(KEY_DERIVATION_VERSION, 0)
    }

    /// 检查是否需要轮换
    pub fn needs_rotation(&self, current_time: u64) -> bool {
        current_time.saturating_sub(self.derived_at) > KEY_ROTATION_INTERVAL_SECONDS
    }

    /// 版本号字符串表示
    pub fn version_string(&self) -> String {
        format!("v{}.{}.{}", self.major, self.minor, self.derived_at)
    }
}

impl Default for KeyVersion {
    fn default() -> Self {
        Self::current()
    }
}

/// 密钥层次管理器
///
/// 管理四层密钥层次的派生和缓存，支持密钥版本控制和安全轮换
pub struct KeyHierarchy {
    /// L1: Enclave 主密钥（持久存在于安全内存）
    master_key: Option<EnclaveMasterKey>,

    /// 当前密钥版本
    key_version: KeyVersion,

    /// 当前时间戳（用于测试）
    current_time: Option<u64>,

    /// 支持的旧版本（用于向后兼容解密）
    legacy_versions: Vec<KeyVersion>,
}

impl KeyHierarchy {
    /// 创建新的密钥层次管理器
    pub fn new() -> Self {
        Self {
            master_key: None,
            key_version: KeyVersion::current(),
            current_time: None,
            legacy_versions: Vec::new(),
        }
    }

    /// 使用指定版本创建（用于密钥轮换）
    pub fn with_version(major: u32, minor: u32) -> Self {
        Self {
            master_key: None,
            key_version: KeyVersion::new(major, minor),
            current_time: None,
            legacy_versions: Vec::new(),
        }
    }

    /// 使用指定时间创建（用于测试）
    #[cfg(test)]
    pub fn with_time(current_time: u64) -> Self {
        Self {
            master_key: None,
            key_version: KeyVersion::current(),
            current_time: Some(current_time),
            legacy_versions: Vec::new(),
        }
    }

    /// 获取当前时间戳
    fn now(&self) -> u64 {
        self.current_time.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("系统时间错误")
                .as_secs()
        })
    }

    /// 初始化 L1 主密钥（从 L0 派生）
    ///
    /// # 流程
    /// 1. 从 SGX Sealing Key 获取 L0
    /// 2. 使用 HKDF-Extract 派生 L1
    /// 3. 存储 L1 到安全内存
    ///
    /// # Arguments
    /// * `l0_key` - L0 硬件根密钥
    ///
    /// # Returns
    /// * `Ok(KeyHandle)` - L1 密钥句柄
    pub fn initialize_master_key(
        &mut self,
        l0_key: &HardwareRootKey,
    ) -> Result<KeyHandle, CryptoError> {
        // HKDF-Extract: L0 -> L1
        let salt = Salt::new(HKDF_SHA256, l0_key.as_bytes());
        let prk = salt.extract(b"CredBridge Enclave v1.0");

        // HKDF-Expand: 派生 L1 密钥材料
        let mut l1_key_material = [0u8; KEY_LENGTH];
        let info: &[u8] = b"enclave-master-key";
        let binding = [info];
        let okm = prk.expand(&binding, HKDF_SHA256)?;
        okm.fill(&mut l1_key_material)?;

        // 生成密钥句柄
        let key_handle = EnclaveMasterKey::generate_handle();

        // 创建 L1 密钥
        let master_key = EnclaveMasterKey::new(l1_key_material, key_handle, self.now());
        let handle = master_key.key_handle();

        self.master_key = Some(master_key);

        Ok(handle)
    }

    /// 派生 L2 用户保险库密钥
    ///
    /// # 流程
    /// 1. 使用 L1 作为 PRK
    /// 2. HKDF-Expand(version + tenant_id + user_id) 派生 L2
    ///
    /// # Arguments
    /// * `tenant_id` - 租户ID
    /// * `user_id` - 用户ID
    ///
    /// # Returns
    /// * `Ok(UserVaultKey)` - L2 用户保险库密钥
    pub fn derive_user_vault_key(
        &self,
        tenant_id: &str,
        user_id: &str,
    ) -> Result<UserVaultKey, CryptoError> {
        let master_key = self
            .master_key
            .as_ref()
            .ok_or_else(|| CryptoError::KeyDerivationError("L1 主密钥未初始化".to_string()))?;

        // 构建 info 字符串（包含版本信息）
        let info = format!(
            "user-vault-key:v{}:{}:{}",
            self.key_version.major, tenant_id, user_id
        );

        // HKDF-Expand: L1 -> L2
        let salt = Salt::new(HKDF_SHA256, master_key.as_bytes());
        let prk = salt.extract(b"");

        let mut l2_key_material = [0u8; KEY_LENGTH];
        let info_bytes = info.into_bytes();
        let binding = [&info_bytes[..]];
        let okm = prk.expand(&binding, HKDF_SHA256)?;
        okm.fill(&mut l2_key_material)?;

        // 生成密钥句柄（对用户ID进行哈希，包含版本）
        let key_handle = self.derive_key_handle_with_version(tenant_id, user_id);

        // 对用户ID进行简单哈希（实际应使用 SHA-256）
        let user_id_hash = format!("hash:{}:v{}", user_id, self.key_version.major);

        Ok(UserVaultKey::new(
            l2_key_material,
            tenant_id.to_string(),
            user_id_hash,
            key_handle,
            self.now(),
        ))
    }

    /// 派生 L3 凭证加密密钥
    ///
    /// # 流程
    /// 1. 使用 L2 作为 PRK
    /// 2. HKDF-Expand(version + credential_id + purpose) 派生 L3
    ///
    /// # Arguments
    /// * `l2_key` - L2 用户保险库密钥
    /// * `credential_id` - 凭证ID
    /// * `purpose` - 密钥用途
    ///
    /// # Returns
    /// * `Ok(CredentialKey)` - L3 凭证加密密钥
    pub fn derive_credential_key(
        &self,
        l2_key: &UserVaultKey,
        credential_id: &str,
        purpose: KeyPurpose,
    ) -> Result<CredentialKey, CryptoError> {
        // 构建 info 字符串（包含版本信息）
        let info = format!(
            "credential-key:v{}:{}:{}",
            self.key_version.major,
            credential_id,
            purpose.as_str()
        );

        // HKDF-Expand: L2 -> L3
        let salt = Salt::new(HKDF_SHA256, l2_key.as_bytes());
        let prk = salt.extract(b"");

        let mut l3_key_material = [0u8; KEY_LENGTH];
        let info_bytes = info.into_bytes();
        let binding = [&info_bytes[..]];
        let okm = prk.expand(&binding, HKDF_SHA256)?;
        okm.fill(&mut l3_key_material)?;

        Ok(CredentialKey::new(
            l3_key_material,
            l2_key.tenant_id.clone(),
            l2_key.user_id_hash.clone(),
            credential_id.to_string(),
            purpose,
            self.now(),
        ))
    }

    /// 批量派生多个凭证密钥（优化性能）
    ///
    /// 对于同一用户的多个凭证，可以重用 PRK 提高效率
    pub fn derive_credential_keys_batch(
        &self,
        l2_key: &UserVaultKey,
        credential_ids: &[String],
        purpose: KeyPurpose,
    ) -> Result<Vec<CredentialKey>, CryptoError> {
        // 预计算 PRK（只执行一次 extract）
        let salt = Salt::new(HKDF_SHA256, l2_key.as_bytes());
        let prk = salt.extract(b"");

        let mut keys = Vec::with_capacity(credential_ids.len());

        for credential_id in credential_ids {
            let info = format!(
                "credential-key:v{}:{}:{}",
                self.key_version.major,
                credential_id,
                purpose.as_str()
            );

            let mut l3_key_material = [0u8; KEY_LENGTH];
            let info_bytes = info.into_bytes();
            let binding = [&info_bytes[..]];
            let okm = prk.expand(&binding, HKDF_SHA256)?;
            okm.fill(&mut l3_key_material)?;

            keys.push(CredentialKey::new(
                l3_key_material,
                l2_key.tenant_id.clone(),
                l2_key.user_id_hash.clone(),
                credential_id.clone(),
                purpose,
                self.now(),
            ));
        }

        Ok(keys)
    }

    /// 派生密钥句柄（确定性派生，用于缓存查找）
    #[allow(dead_code)]
    fn derive_key_handle(&self, tenant_id: &str, user_id: &str) -> KeyHandle {
        self.derive_key_handle_with_version(tenant_id, user_id)
    }

    /// 派生带版本信息的密钥句柄
    fn derive_key_handle_with_version(&self, tenant_id: &str, user_id: &str) -> KeyHandle {
        let data = format!("v{}:{}:{}", self.key_version.major, tenant_id, user_id);
        let hash = ring::digest::digest(&ring::digest::SHA256, data.as_bytes());
        let mut handle = [0u8; 32];
        handle.copy_from_slice(hash.as_ref());
        handle
    }

    /// 获取当前密钥版本
    pub fn key_version(&self) -> &KeyVersion {
        &self.key_version
    }

    /// 检查是否需要密钥轮换
    pub fn needs_key_rotation(&self) -> bool {
        self.key_version.needs_rotation(self.now())
    }

    /// 执行密钥轮换
    ///
    /// 轮换后：
    /// 1. 旧版本移至 legacy_versions
    /// 2. 新版本号递增
    /// 3. 需要重新派生所有密钥
    pub fn rotate_key_version(&mut self) {
        // 保存旧版本
        self.legacy_versions.push(self.key_version);

        // 限制历史版本数量（防止内存无限增长）
        if self.legacy_versions.len() > 5 {
            self.legacy_versions.remove(0);
        }

        // 递增主版本号
        self.key_version = KeyVersion::new(self.key_version.major + 1, 0);
    }

    /// 获取支持的历史版本
    pub fn legacy_versions(&self) -> &[KeyVersion] {
        &self.legacy_versions
    }

    /// 获取 L1 主密钥句柄（如果已初始化）
    pub fn master_key_handle(&self) -> Option<KeyHandle> {
        self.master_key.as_ref().map(|k| k.key_handle())
    }

    /// 重新生成主密钥（用于密钥轮换）
    ///
    /// # 密钥轮换流程
    /// 1. 递增密钥版本号
    /// 2. 保存旧版本到历史版本列表
    /// 3. 派生新的主密钥
    /// 4. 旧密钥在 TTL 后自动清理
    ///
    /// # Warning
    /// 轮换后旧凭证仍可用旧版本密钥解密（在保留期内）
    pub fn rotate_master_key(
        &mut self,
        l0_key: &HardwareRootKey,
    ) -> Result<KeyHandle, CryptoError> {
        // 执行版本轮换
        self.rotate_key_version();

        // 清理旧密钥
        if let Some(mut old_key) = self.master_key.take() {
            old_key.key_material.zeroize();
        }

        // 使用新版本重新初始化
        self.initialize_master_key(l0_key)
    }

    /// 使用旧版本派生密钥（用于解密历史凭证）
    ///
    /// # Arguments
    /// * `l0_key` - L0 硬件根密钥
    /// * `version` - 历史版本号
    pub fn derive_with_legacy_version(
        &self,
        l0_key: &HardwareRootKey,
        version: u32,
    ) -> Result<KeyHandle, CryptoError> {
        // HKDF-Extract: L0 -> L1 (使用旧版本参数)
        let salt = Salt::new(HKDF_SHA256, l0_key.as_bytes());
        let prk = salt.extract(format!("CredBridge Enclave v{}", version).as_bytes());

        // HKDF-Expand: 派生 L1 密钥材料
        let mut l1_key_material = [0u8; KEY_LENGTH];
        let info: &[u8] = b"enclave-master-key";
        let binding = [info];
        let okm = prk.expand(&binding, HKDF_SHA256)?;
        okm.fill(&mut l1_key_material)?;

        // 生成密钥句柄（包含版本）
        let key_handle = EnclaveMasterKey::generate_handle();

        // 注意：这里不保存到 self.master_key，仅用于临时派生
        let master_key = EnclaveMasterKey::new(l1_key_material, key_handle, self.now());

        Ok(master_key.key_handle())
    }
}

impl Default for KeyHierarchy {
    fn default() -> Self {
        Self::new()
    }
}

/// 密钥派生工具函数
pub mod utils {
    use super::*;

    /// 使用 HKDF-SHA256 派生密钥
    ///
    /// # Arguments
    /// * `ikm` - 输入密钥材料
    /// * `salt` - 盐值
    /// * `info` - 上下文信息
    /// * `length` - 输出密钥长度
    ///
    /// # Returns
    /// 派生出的密钥材料
    pub fn hkdf_derive(
        ikm: &[u8],
        salt: &[u8],
        info: &[u8],
        length: usize,
    ) -> Result<Vec<u8>, CryptoError> {
        let hkdf_salt = Salt::new(HKDF_SHA256, salt);
        let prk = hkdf_salt.extract(ikm);

        let mut output = vec![0u8; length];
        let info_slice: &[u8] = info;
        let binding = [info_slice];
        let okm = prk.expand(&binding, HKDF_SHA256)?;
        okm.fill(&mut output)?;

        Ok(output)
    }

    /// 派生 Token 签名密钥
    pub fn derive_token_key(
        master_key: &[u8],
        tenant_id: &str,
        user_id: &str,
    ) -> Result<[u8; KEY_LENGTH], CryptoError> {
        let info = format!("token-key:{}:{}", tenant_id, user_id);
        let mut key = [0u8; KEY_LENGTH];

        let salt = Salt::new(HKDF_SHA256, master_key);
        let prk = salt.extract(b"");

        let info_bytes = info.into_bytes();
        let binding = [&info_bytes[..]];
        let okm = prk.expand(&binding, HKDF_SHA256)?;
        okm.fill(&mut key)?;

        Ok(key)
    }
}

/// 获取当前时间戳
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间错误")
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::super::keys::RootKeySource;
    use super::*;

    #[test]
    fn test_initialize_master_key() {
        let l0 = HardwareRootKey::for_simulation().unwrap();
        let mut hierarchy = KeyHierarchy::new();

        let handle = hierarchy.initialize_master_key(&l0).unwrap();

        // 验证句柄不为空
        assert_ne!(handle, [0u8; 32]);
        assert!(hierarchy.master_key_handle().is_some());
    }

    #[test]
    fn test_derive_user_vault_key() {
        let l0 = HardwareRootKey::for_simulation().unwrap();
        let mut hierarchy = KeyHierarchy::new();
        hierarchy.initialize_master_key(&l0).unwrap();

        let l2_key = hierarchy
            .derive_user_vault_key("tenant_123", "user_456")
            .unwrap();

        assert_eq!(l2_key.tenant_id(), "tenant_123");
        assert_eq!(l2_key.user_id_hash(), "hash:user_456:v1");
    }

    #[test]
    fn test_derive_credential_key() {
        let l0 = HardwareRootKey::for_simulation().unwrap();
        let mut hierarchy = KeyHierarchy::new();
        hierarchy.initialize_master_key(&l0).unwrap();

        let l2_key = hierarchy
            .derive_user_vault_key("tenant_123", "user_456")
            .unwrap();

        let l3_key = hierarchy
            .derive_credential_key(&l2_key, "cred_789", KeyPurpose::CredentialEncryption)
            .unwrap();

        assert_eq!(l3_key.credential_id(), "cred_789");
        assert_eq!(l3_key.purpose(), KeyPurpose::CredentialEncryption);
    }

    #[test]
    fn test_derive_credential_keys_batch() {
        let l0 = HardwareRootKey::for_simulation().unwrap();
        let mut hierarchy = KeyHierarchy::new();
        hierarchy.initialize_master_key(&l0).unwrap();

        let l2_key = hierarchy
            .derive_user_vault_key("tenant_123", "user_456")
            .unwrap();

        let credential_ids = vec![
            "cred_1".to_string(),
            "cred_2".to_string(),
            "cred_3".to_string(),
        ];

        let l3_keys = hierarchy
            .derive_credential_keys_batch(
                &l2_key,
                &credential_ids,
                KeyPurpose::CredentialEncryption,
            )
            .unwrap();

        assert_eq!(l3_keys.len(), 3);
        assert_eq!(l3_keys[0].credential_id(), "cred_1");
        assert_eq!(l3_keys[1].credential_id(), "cred_2");
        assert_eq!(l3_keys[2].credential_id(), "cred_3");
    }

    #[test]
    fn test_key_derivation_determinism() {
        // 相同输入应该产生相同的派生密钥
        let l0_1 = HardwareRootKey::for_simulation().unwrap();
        let l0_2 = HardwareRootKey {
            key_material: *l0_1.as_bytes(),
            source: RootKeySource::Simulation,
            mrsigner: [0u8; 32],
            mrenclave: [0u8; 32],
        };

        let mut hierarchy1 = KeyHierarchy::new();
        let mut hierarchy2 = KeyHierarchy::new();

        hierarchy1.initialize_master_key(&l0_1).unwrap();
        hierarchy2.initialize_master_key(&l0_2).unwrap();

        let l2_1 = hierarchy1
            .derive_user_vault_key("tenant_123", "user_456")
            .unwrap();
        let l2_2 = hierarchy2
            .derive_user_vault_key("tenant_123", "user_456")
            .unwrap();

        // 相同输入应该产生相同的密钥
        assert_eq!(l2_1.as_bytes(), l2_2.as_bytes());
    }

    #[test]
    fn test_uniqueness_per_user() {
        // 不同用户应该有不同的密钥
        let l0 = HardwareRootKey::for_simulation().unwrap();
        let mut hierarchy = KeyHierarchy::new();
        hierarchy.initialize_master_key(&l0).unwrap();

        let l2_user1 = hierarchy
            .derive_user_vault_key("tenant_123", "user_1")
            .unwrap();
        let l2_user2 = hierarchy
            .derive_user_vault_key("tenant_123", "user_2")
            .unwrap();

        assert_ne!(l2_user1.as_bytes(), l2_user2.as_bytes());
    }

    #[test]
    fn test_uniqueness_per_tenant() {
        // 不同租户应该有不同的密钥
        let l0 = HardwareRootKey::for_simulation().unwrap();
        let mut hierarchy = KeyHierarchy::new();
        hierarchy.initialize_master_key(&l0).unwrap();

        let l2_tenant1 = hierarchy
            .derive_user_vault_key("tenant_1", "user_456")
            .unwrap();
        let l2_tenant2 = hierarchy
            .derive_user_vault_key("tenant_2", "user_456")
            .unwrap();

        assert_ne!(l2_tenant1.as_bytes(), l2_tenant2.as_bytes());
    }

    #[test]
    fn test_uniqueness_per_credential() {
        // 不同凭证应该有不同的密钥
        let l0 = HardwareRootKey::for_simulation().unwrap();
        let mut hierarchy = KeyHierarchy::new();
        hierarchy.initialize_master_key(&l0).unwrap();

        let l2_key = hierarchy
            .derive_user_vault_key("tenant_123", "user_456")
            .unwrap();

        let l3_cred1 = hierarchy
            .derive_credential_key(&l2_key, "cred_1", KeyPurpose::CredentialEncryption)
            .unwrap();
        let l3_cred2 = hierarchy
            .derive_credential_key(&l2_key, "cred_2", KeyPurpose::CredentialEncryption)
            .unwrap();

        assert_ne!(l3_cred1.as_bytes(), l3_cred2.as_bytes());
    }

    #[test]
    fn test_different_purposes_different_keys() {
        // 不同用途应该产生不同的密钥
        let l0 = HardwareRootKey::for_simulation().unwrap();
        let mut hierarchy = KeyHierarchy::new();
        hierarchy.initialize_master_key(&l0).unwrap();

        let l2_key = hierarchy
            .derive_user_vault_key("tenant_123", "user_456")
            .unwrap();

        let l3_encrypt = hierarchy
            .derive_credential_key(&l2_key, "cred_789", KeyPurpose::CredentialEncryption)
            .unwrap();
        let l3_decrypt = hierarchy
            .derive_credential_key(&l2_key, "cred_789", KeyPurpose::CredentialDecryption)
            .unwrap();
        let l3_sign = hierarchy
            .derive_credential_key(&l2_key, "cred_789", KeyPurpose::Signing)
            .unwrap();

        assert_ne!(l3_encrypt.as_bytes(), l3_decrypt.as_bytes());
        assert_ne!(l3_encrypt.as_bytes(), l3_sign.as_bytes());
        assert_ne!(l3_decrypt.as_bytes(), l3_sign.as_bytes());
    }

    #[test]
    fn test_hkdf_derive_util() {
        let ikm = b"input key material";
        let salt = b"salt value";
        let info = b"application specific info";

        let key1 = utils::hkdf_derive(ikm, salt, info, 32).unwrap();
        let key2 = utils::hkdf_derive(ikm, salt, info, 32).unwrap();

        assert_eq!(key1, key2);
        assert_eq!(key1.len(), 32);
    }

    #[test]
    fn test_derive_token_key() {
        let master_key = [0x42u8; 32];
        let key1 = utils::derive_token_key(&master_key, "tenant_123", "user_456").unwrap();
        let key2 = utils::derive_token_key(&master_key, "tenant_123", "user_456").unwrap();

        // 相同输入应该产生相同的密钥
        assert_eq!(key1, key2);

        // 不同租户/用户应该产生不同的密钥
        let key3 = utils::derive_token_key(&master_key, "tenant_456", "user_456").unwrap();
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_key_version() {
        let version = KeyVersion::current();
        assert_eq!(version.major, KEY_DERIVATION_VERSION);

        // 检查版本字符串
        let version_str = version.version_string();
        assert!(version_str.starts_with("v1."));
    }

    #[test]
    fn test_key_version_rotation() {
        let mut hierarchy = KeyHierarchy::new();

        // 初始版本
        assert_eq!(hierarchy.key_version().major, 1);

        // 执行轮换
        hierarchy.rotate_key_version();

        // 新版本
        assert_eq!(hierarchy.key_version().major, 2);

        // 旧版本在历史列表中
        assert_eq!(hierarchy.legacy_versions().len(), 1);
        assert_eq!(hierarchy.legacy_versions()[0].major, 1);
    }

    #[test]
    fn test_key_rotation_changes_derived_keys() {
        let l0 = HardwareRootKey::for_simulation().unwrap();

        // 版本 1
        let mut hierarchy_v1 = KeyHierarchy::with_version(1, 0);
        hierarchy_v1.initialize_master_key(&l0).unwrap();
        let l2_v1 = hierarchy_v1
            .derive_user_vault_key("tenant_123", "user_456")
            .unwrap();

        // 版本 2
        let mut hierarchy_v2 = KeyHierarchy::with_version(2, 0);
        hierarchy_v2.initialize_master_key(&l0).unwrap();
        let l2_v2 = hierarchy_v2
            .derive_user_vault_key("tenant_123", "user_456")
            .unwrap();

        // 不同版本应该产生不同的密钥
        assert_ne!(l2_v1.as_bytes(), l2_v2.as_bytes());
    }

    #[test]
    fn test_key_version_needs_rotation() {
        let current_time = current_timestamp();

        // 新创建的版本不需要轮换
        let new_version = KeyVersion::new(1, 0);
        assert!(!new_version.needs_rotation(current_time));

        // 91 天前的版本需要轮换
        let old_time = current_time - (91 * 24 * 60 * 60);
        let old_version = KeyVersion {
            major: 1,
            minor: 0,
            derived_at: old_time,
        };
        assert!(old_version.needs_rotation(current_time));
    }
}
