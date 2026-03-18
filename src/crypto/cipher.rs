//! AES-256-GCM 加密实现
//!
//! 实现凭证的加密和解密
//! - AES-256-GCM 算法
//! - 96-bit 随机 nonce
//! - 128-bit auth tag

use super::CryptoError;
use super::constants::{NONCE_LENGTH, PROTOCOL_VERSION};
use super::keys::CredentialKey;
use aes_gcm::{
    Aes256Gcm, Nonce as AesGcmNonce,
    aead::{Aead, KeyInit, Payload},
};
use serde::{Deserialize, Serialize};

/// 加密后的凭证数据包
///
/// 包含密文、nonce、auth tag 和元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedBlob {
    /// 协议版本
    pub version: u8,

    /// 加密算法标识
    pub algorithm: String,

    /// KDF 算法标识
    pub kdf: String,

    /// Nonce (96-bit = 12 bytes, base64 encoded)
    pub nonce: String,

    /// Auth Tag (128-bit = 16 bytes, base64 encoded)
    pub auth_tag: String,

    /// 密文 (base64 encoded)
    pub ciphertext: String,

    /// 附加认证数据（AAD）哈希
    pub aad_hash: Option<String>,
}

impl EncryptedBlob {
    /// 创建新的加密数据包
    fn new(
        version: u8,
        algorithm: String,
        kdf: String,
        nonce: Vec<u8>,
        auth_tag: Vec<u8>,
        ciphertext: Vec<u8>,
        aad_hash: Option<String>,
    ) -> Self {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

        Self {
            version,
            algorithm,
            kdf,
            nonce: URL_SAFE_NO_PAD.encode(&nonce),
            auth_tag: URL_SAFE_NO_PAD.encode(&auth_tag),
            ciphertext: URL_SAFE_NO_PAD.encode(&ciphertext),
            aad_hash,
        }
    }

    /// 获取 nonce 字节
    pub fn nonce_bytes(&self) -> Result<Vec<u8>, CryptoError> {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        URL_SAFE_NO_PAD
            .decode(&self.nonce)
            .map_err(|_| CryptoError::InvalidCiphertext)
    }

    /// 获取 auth_tag 字节
    pub fn auth_tag_bytes(&self) -> Result<Vec<u8>, CryptoError> {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        URL_SAFE_NO_PAD
            .decode(&self.auth_tag)
            .map_err(|_| CryptoError::InvalidCiphertext)
    }

    /// 获取密文字节
    pub fn ciphertext_bytes(&self) -> Result<Vec<u8>, CryptoError> {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        URL_SAFE_NO_PAD
            .decode(&self.ciphertext)
            .map_err(|_| CryptoError::InvalidCiphertext)
    }

    /// 序列化为 JSON 字符串
    pub fn to_json(&self) -> Result<String, CryptoError> {
        serde_json::to_string(self).map_err(|e| CryptoError::EncryptionError(e.to_string()))
    }

    /// 从 JSON 字符串解析
    pub fn from_json(json: &str) -> Result<Self, CryptoError> {
        serde_json::from_str(json).map_err(|_| CryptoError::InvalidCiphertext)
    }
}

/// 使用 L3 密钥加密凭证
///
/// # Arguments
/// * `key` - L3 凭证加密密钥
/// * `plaintext` - 明文凭证内容
/// * `aad` - 附加认证数据（可选）
///
/// # Returns
/// * `Ok(EncryptedBlob)` - 加密后的数据包
///
/// # Example
/// ```rust,ignore
/// let l3_key = hierarchy.derive_credential_key(&l2_key, "cred_123", KeyPurpose::CredentialEncryption)?;
/// let encrypted = encrypt_credential(&l3_key, b"secret password", Some(b"tenant:user"))?;
/// ```
pub fn encrypt_credential(
    key: &CredentialKey,
    plaintext: &[u8],
    aad: Option<&[u8]>,
) -> Result<EncryptedBlob, CryptoError> {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use rand::RngCore;

    // 生成随机 nonce (96-bit)
    let mut nonce_bytes = [0u8; NONCE_LENGTH];
    let mut rng = rand::thread_rng();
    rng.fill_bytes(&mut nonce_bytes);

    // 创建 AES-256-GCM cipher
    let cipher = Aes256Gcm::new_from_slice(key.as_bytes())
        .map_err(|_| CryptoError::EncryptionError("Invalid key".to_string()))?;

    let nonce = AesGcmNonce::from_slice(&nonce_bytes);

    // 执行加密
    let ciphertext = if let Some(aad_data) = aad {
        cipher
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext,
                    aad: aad_data,
                },
            )
            .map_err(|_| CryptoError::EncryptionError("Encryption failed".to_string()))?
    } else {
        cipher
            .encrypt(nonce, plaintext)
            .map_err(|_| CryptoError::EncryptionError("Encryption failed".to_string()))?
    };

    // aes-gcm 返回的 ciphertext 包含密文 + auth tag (最后 16 字节)
    let auth_tag_start = ciphertext.len().saturating_sub(16);
    let ciphertext_only = ciphertext[..auth_tag_start].to_vec();
    let auth_tag = ciphertext[auth_tag_start..].to_vec();

    // 计算 AAD 哈希（用于验证）
    let aad_hash = aad.map(|data| {
        let hash = ring::digest::digest(&ring::digest::SHA256, data);
        URL_SAFE_NO_PAD.encode(hash.as_ref())
    });

    Ok(EncryptedBlob::new(
        PROTOCOL_VERSION,
        "AES-256-GCM".to_string(),
        "HKDF-SHA-256".to_string(),
        nonce_bytes.to_vec(),
        auth_tag,
        ciphertext_only,
        aad_hash,
    ))
}

/// 使用 L3 密钥解密凭证
///
/// # Arguments
/// * `key` - L3 凭证加密密钥（应为 CredentialDecryption 用途）
/// * `blob` - 加密数据包
/// * `aad` - 附加认证数据（必须与加密时相同）
///
/// # Returns
/// * `Ok(Vec<u8>)` - 解密后的明文
///
/// # Errors
/// * `AuthenticationFailed` - auth tag 验证失败（数据被篡改或密钥错误）
pub fn decrypt_credential(
    key: &CredentialKey,
    blob: &EncryptedBlob,
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, CryptoError> {
    // 验证算法
    if blob.algorithm != "AES-256-GCM" {
        return Err(CryptoError::EncryptionError(format!(
            "Unsupported algorithm: {}",
            blob.algorithm
        )));
    }

    // 解析 nonce
    let nonce_bytes = blob.nonce_bytes()?;
    if nonce_bytes.len() != NONCE_LENGTH {
        return Err(CryptoError::InvalidCiphertext);
    }

    // 获取密文和 auth tag
    let mut ciphertext = blob.ciphertext_bytes()?;
    let auth_tag = blob.auth_tag_bytes()?;

    // 合并密文和 auth tag
    ciphertext.extend_from_slice(&auth_tag);

    // 创建 AES-256-GCM cipher
    let cipher = Aes256Gcm::new_from_slice(key.as_bytes())
        .map_err(|_| CryptoError::DecryptionError("Invalid key".to_string()))?;

    let nonce = AesGcmNonce::from_slice(&nonce_bytes);

    // 执行解密
    let plaintext = if let Some(aad_data) = aad {
        cipher
            .decrypt(
                nonce,
                Payload {
                    msg: ciphertext.as_slice(),
                    aad: aad_data,
                },
            )
            .map_err(|_| CryptoError::AuthenticationFailed)?
    } else {
        cipher
            .decrypt(nonce, ciphertext.as_slice())
            .map_err(|_| CryptoError::AuthenticationFailed)?
    };

    Ok(plaintext)
}

/// 加密上下文（用于批量加密）
pub struct EncryptionContext {
    aad_template: Vec<u8>,
}

impl EncryptionContext {
    /// 创建加密上下文
    pub fn new(tenant_id: &str, user_id: &str) -> Self {
        let aad = format!("{}:{}", tenant_id, user_id);
        Self {
            aad_template: aad.into_bytes(),
        }
    }

    /// 加密单个凭证
    pub fn encrypt(
        &self,
        key: &CredentialKey,
        plaintext: &[u8],
    ) -> Result<EncryptedBlob, CryptoError> {
        encrypt_credential(key, plaintext, Some(&self.aad_template))
    }
}

/// 解密上下文（用于批量解密）
pub struct DecryptionContext {
    aad_template: Vec<u8>,
}

impl DecryptionContext {
    /// 创建解密上下文
    pub fn new(tenant_id: &str, user_id: &str) -> Self {
        let aad = format!("{}:{}", tenant_id, user_id);
        Self {
            aad_template: aad.into_bytes(),
        }
    }

    /// 解密单个凭证
    pub fn decrypt(
        &self,
        key: &CredentialKey,
        blob: &EncryptedBlob,
    ) -> Result<Vec<u8>, CryptoError> {
        decrypt_credential(key, blob, Some(&self.aad_template))
    }
}

#[cfg(test)]
mod tests {
    use super::super::hkdf::KeyHierarchy;
    use super::super::keys::{HardwareRootKey, KeyPurpose};
    use super::*;

    fn setup_test_key() -> (KeyHierarchy, CredentialKey) {
        let l0 = HardwareRootKey::for_simulation().unwrap();
        let mut hierarchy = KeyHierarchy::new();
        hierarchy.initialize_master_key(&l0).unwrap();

        let l2_key = hierarchy
            .derive_user_vault_key("tenant_123", "user_456")
            .unwrap();
        let l3_key = hierarchy
            .derive_credential_key(&l2_key, "cred_789", KeyPurpose::CredentialEncryption)
            .unwrap();

        (hierarchy, l3_key)
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let (_, l3_key) = setup_test_key();
        let plaintext = b"my secret password";

        // 加密
        let blob = encrypt_credential(&l3_key, plaintext, None).unwrap();

        // 使用相同的密钥解密（aes-gcm 是对称加密，加密解密用相同密钥）
        // 注意：在实际业务中，应该使用相同的 purpose 派生密钥
        let decrypted = decrypt_credential(&l3_key, &blob, None).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_with_aad() {
        let (_, l3_key) = setup_test_key();
        let plaintext = b"my secret password";
        let aad = b"tenant_123:user_456";

        // 加密
        let blob = encrypt_credential(&l3_key, plaintext, Some(aad)).unwrap();

        // 验证 AAD 哈希存在
        assert!(blob.aad_hash.is_some());

        // 使用相同密钥解密
        let decrypted = decrypt_credential(&l3_key, &blob, Some(aad)).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_with_wrong_aad_fails() {
        let (_, l3_key) = setup_test_key();
        let plaintext = b"my secret password";
        let aad = b"tenant_123:user_456";

        // 加密
        let blob = encrypt_credential(&l3_key, plaintext, Some(aad)).unwrap();

        // 使用错误 AAD 解密应该失败
        let l0 = HardwareRootKey::for_simulation().unwrap();
        let mut hierarchy = KeyHierarchy::new();
        hierarchy.initialize_master_key(&l0).unwrap();
        let l2_key = hierarchy
            .derive_user_vault_key("tenant_123", "user_456")
            .unwrap();
        let l3_decrypt_key = hierarchy
            .derive_credential_key(&l2_key, "cred_789", KeyPurpose::CredentialDecryption)
            .unwrap();

        let wrong_aad = b"wrong:aad";
        let result = decrypt_credential(&l3_decrypt_key, &blob, Some(wrong_aad));
        assert!(matches!(result, Err(CryptoError::AuthenticationFailed)));
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let (_, l3_key) = setup_test_key();
        let plaintext = b"my secret password";

        // 加密
        let blob = encrypt_credential(&l3_key, plaintext, None).unwrap();

        // 使用错误密钥解密应该失败
        let l0 = HardwareRootKey::for_simulation().unwrap();
        let mut hierarchy = KeyHierarchy::new();
        hierarchy.initialize_master_key(&l0).unwrap();
        let l2_key = hierarchy
            .derive_user_vault_key("tenant_123", "user_456")
            .unwrap();
        let wrong_key = hierarchy
            .derive_credential_key(&l2_key, "different_cred", KeyPurpose::CredentialDecryption)
            .unwrap();

        let result = decrypt_credential(&wrong_key, &blob, None);
        assert!(matches!(result, Err(CryptoError::AuthenticationFailed)));
    }

    #[test]
    fn test_blob_serialization() {
        let (_, l3_key) = setup_test_key();
        let plaintext = b"test data";

        let blob = encrypt_credential(&l3_key, plaintext, None).unwrap();

        // 序列化
        let json = blob.to_json().unwrap();

        // 反序列化
        let restored = EncryptedBlob::from_json(&json).unwrap();

        assert_eq!(restored.version, blob.version);
        assert_eq!(restored.algorithm, blob.algorithm);
        assert_eq!(restored.nonce, blob.nonce);
        assert_eq!(restored.ciphertext, blob.ciphertext);
    }

    #[test]
    fn test_nonce_uniqueness() {
        let (_, l3_key) = setup_test_key();
        let plaintext = b"test";

        let blob1 = encrypt_credential(&l3_key, plaintext, None).unwrap();
        let blob2 = encrypt_credential(&l3_key, plaintext, None).unwrap();

        // 两次加密的 nonce 应该不同
        assert_ne!(blob1.nonce, blob2.nonce);

        // nonce 应该可以正确解码
        let nonce1 = blob1.nonce_bytes().unwrap();
        let nonce2 = blob2.nonce_bytes().unwrap();

        assert_eq!(nonce1.len(), NONCE_LENGTH);
        assert_eq!(nonce2.len(), NONCE_LENGTH);
    }

    #[test]
    fn test_context_encryption() {
        let (_, l3_key) = setup_test_key();
        let plaintext = b"test data";

        let ctx = EncryptionContext::new("tenant_123", "user_456");
        let blob = ctx.encrypt(&l3_key, plaintext).unwrap();

        let decrypt_ctx = DecryptionContext::new("tenant_123", "user_456");

        // 使用正确上下文解密
        let decrypted = decrypt_ctx.decrypt(&l3_key, &blob).unwrap();
        assert_eq!(decrypted, plaintext);

        // 使用错误上下文解密应该失败
        let wrong_ctx = DecryptionContext::new("wrong_tenant", "wrong_user");
        let result = wrong_ctx.decrypt(&l3_key, &blob);
        assert!(matches!(result, Err(CryptoError::AuthenticationFailed)));
    }
}
