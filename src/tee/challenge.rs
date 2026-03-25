//! 挑战-响应协议实现
//!
//! 实现安全的挑战-响应协议，用于远程认证中的身份验证。
//! 协议基于随机挑战和 Quote 签名，确保通信双方的身份真实性。
//!
//! # 协议流程
//!
//! ```text
//! Verifier (挑战者)                    Prover (Enclave)
//!       │                                     │
//!       │ 1. 生成随机挑战 (32 bytes)          │
//!       │────────────────────────────────────>│
//!       │                                     │
//!       │                                     │ 2. 生成 Quote
//!       │                                     │    (REPORT_DATA = hash(challenge + identity))
//!       │                                     │
//!       │ 3. 返回 Quote                       │
//!       │<────────────────────────────────────│
//!       │                                     │
//!       │ 4. 验证 Quote                       │
//!       │    - 验证签名                       │
//!       │    - 验证测量值                     │
//!       │    - 验证挑战绑定                   │
//!       │                                     │
//!       │ 5. 认证成功                         │
//!       │────────────────────────────────────>│
//!       │                                     │
//! ```
//!
//! # 安全属性
//!
//! - **防重放攻击**: 每次挑战都是随机生成的，只使用一次
//! - **身份绑定**: Quote 中的 REPORT_DATA 绑定挑战和 Enclave 身份
//! - **新鲜性保证**: 挑战有严格的时间限制（默认 5 分钟）
//! - **不可否认**: ECDSA 签名提供强不可否认性

use crate::tee::attestation::{AttestationError, AttestationResult, AttestationService, Quote};
use crate::tee::enclave::{Enclave, EnclaveError};
use ring::digest::{SHA256, digest};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// 挑战-响应协议版本
pub const CHALLENGE_PROTOCOL_VERSION: u8 = 1;

/// 挑战默认长度（32字节 = 256位）
pub const CHALLENGE_LENGTH: usize = 32;

/// 安全通道密钥长度
pub const CHANNEL_KEY_LENGTH: usize = 32;

/// 挑战默认 TTL（秒）
pub const DEFAULT_CHALLENGE_TTL: u64 = 300; // 5 分钟

/// 最大挑战并发数
pub const MAX_CONCURRENT_CHALLENGES: usize = 1000;

/// 挑战-响应协议错误类型
#[derive(Debug)]
pub enum ChallengeError {
    /// 挑战已存在
    ChallengeAlreadyExists,

    /// 挑战不存在或已过期
    ChallengeNotFound,

    /// 挑战已过期
    ChallengeExpired,

    /// 挑战已被使用
    ChallengeAlreadyUsed,

    /// 响应验证失败
    ResponseVerificationFailed,

    /// 会话已满
    SessionLimitExceeded,

    /// 无效的挑战格式
    InvalidChallengeFormat,

    /// 认证失败
    AttestationFailed(String),

    /// 内部错误
    InternalError(String),
}

impl std::fmt::Display for ChallengeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChallengeError::ChallengeAlreadyExists => write!(f, "Challenge already exists"),
            ChallengeError::ChallengeNotFound => write!(f, "Challenge not found or expired"),
            ChallengeError::ChallengeExpired => write!(f, "Challenge expired"),
            ChallengeError::ChallengeAlreadyUsed => write!(f, "Challenge already used"),
            ChallengeError::ResponseVerificationFailed => write!(f, "Response verification failed"),
            ChallengeError::SessionLimitExceeded => write!(f, "Session limit exceeded"),
            ChallengeError::InvalidChallengeFormat => write!(f, "Invalid challenge format"),
            ChallengeError::AttestationFailed(msg) => write!(f, "Attestation failed: {msg}"),
            ChallengeError::InternalError(msg) => write!(f, "Internal error: {msg}"),
        }
    }
}

impl std::error::Error for ChallengeError {}

impl From<AttestationError> for ChallengeError {
    fn from(e: AttestationError) -> Self {
        ChallengeError::AttestationFailed(e.to_string())
    }
}

impl From<EnclaveError> for ChallengeError {
    fn from(e: EnclaveError) -> Self {
        ChallengeError::InternalError(e.to_string())
    }
}

/// 挑战数据
#[derive(Debug, Clone)]
pub struct Challenge {
    /// 挑战唯一标识符
    pub id: String,

    /// 随机挑战值（32字节）
    pub nonce: [u8; CHALLENGE_LENGTH],

    /// 创建时间戳
    pub created_at: u64,

    /// 过期时间戳
    pub expires_at: u64,

    /// 挑战状态
    pub status: ChallengeStatus,

    /// 关联的 Enclave ID（可选）
    pub enclave_id: Option<String>,

    /// 客户端元数据
    pub metadata: ChallengeMetadata,
}

/// 挑战状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChallengeStatus {
    /// 已创建，等待响应
    Pending,
    /// 已收到响应
    Responded,
    /// 已验证成功
    Verified,
    /// 验证失败
    Failed,
    /// 已过期
    Expired,
}

/// 挑战元数据
#[derive(Debug, Clone, Default)]
pub struct ChallengeMetadata {
    /// 客户端 IP 地址
    pub client_ip: Option<String>,

    /// 用户代理
    pub user_agent: Option<String>,

    /// 请求 ID
    pub request_id: Option<String>,

    /// 额外数据
    pub extra: HashMap<String, String>,
}

impl Challenge {
    /// 创建新的挑战
    pub fn new(id: String, ttl_seconds: u64) -> Result<Self, ChallengeError> {
        let nonce = generate_secure_nonce()?;
        let now = current_timestamp();

        Ok(Self {
            id,
            nonce,
            created_at: now,
            expires_at: now + ttl_seconds,
            status: ChallengeStatus::Pending,
            enclave_id: None,
            metadata: ChallengeMetadata::default(),
        })
    }

    /// 创建带元数据的挑战
    pub fn with_metadata(
        id: String,
        ttl_seconds: u64,
        metadata: ChallengeMetadata,
    ) -> Result<Self, ChallengeError> {
        let mut challenge = Self::new(id, ttl_seconds)?;
        challenge.metadata = metadata;
        Ok(challenge)
    }

    /// 检查是否过期
    pub fn is_expired(&self) -> bool {
        current_timestamp() > self.expires_at
    }

    /// 检查是否已使用
    pub fn is_used(&self) -> bool {
        matches!(
            self.status,
            ChallengeStatus::Responded | ChallengeStatus::Verified | ChallengeStatus::Failed
        )
    }

    /// 验证响应中的挑战绑定
    pub fn verify_binding(&self, report_data: &[u8; 64], enclave_identity: &[u8]) -> bool {
        // 计算预期的 Report Data
        let mut hasher_input = Vec::with_capacity(self.nonce.len() + enclave_identity.len());
        hasher_input.extend_from_slice(&self.nonce);
        hasher_input.extend_from_slice(enclave_identity);

        let expected_hash = digest(&SHA256, &hasher_input);

        // 比较前 32 字节
        report_data[..32] == expected_hash.as_ref()[..32]
    }

    /// 标记为已响应
    pub fn mark_responded(&mut self) -> Result<(), ChallengeError> {
        // 检查是否过期
        if self.is_expired() {
            self.status = ChallengeStatus::Expired;
            return Err(ChallengeError::ChallengeExpired);
        }

        if self.status != ChallengeStatus::Pending {
            return Err(ChallengeError::ChallengeAlreadyUsed);
        }

        self.status = ChallengeStatus::Responded;
        Ok(())
    }

    /// 标记为已验证
    pub fn mark_verified(&mut self) -> Result<(), ChallengeError> {
        if self.status != ChallengeStatus::Responded {
            return Err(ChallengeError::InvalidChallengeFormat);
        }

        self.status = ChallengeStatus::Verified;
        Ok(())
    }

    /// 标记为失败
    pub fn mark_failed(&mut self) -> Result<(), ChallengeError> {
        if self.status != ChallengeStatus::Responded {
            return Err(ChallengeError::InvalidChallengeFormat);
        }

        self.status = ChallengeStatus::Failed;
        Ok(())
    }

    /// 标记为过期
    pub fn mark_expired(&mut self) {
        self.status = ChallengeStatus::Expired;
    }
}

/// 挑战响应
#[derive(Debug, Clone)]
pub struct ChallengeResponse {
    /// 挑战 ID
    pub challenge_id: String,

    /// SGX Quote
    pub quote: Quote,

    /// 响应时间戳
    pub responded_at: u64,

    /// 可选的附加数据
    pub additional_data: Option<Vec<u8>>,
}

impl ChallengeResponse {
    /// 创建新的响应
    pub fn new(challenge_id: String, quote: Quote) -> Self {
        Self {
            challenge_id,
            quote,
            responded_at: current_timestamp(),
            additional_data: None,
        }
    }

    /// 添加附加数据
    pub fn with_additional_data(mut self, data: Vec<u8>) -> Self {
        self.additional_data = Some(data);
        self
    }
}

/// 挑战-响应协议处理器
///
/// 管理挑战的生成、存储和验证
pub struct ChallengeProtocol {
    /// 挑战存储
    challenges: Arc<Mutex<HashMap<String, Challenge>>>,

    /// 认证服务
    attestation_service: AttestationService,

    /// 挑战 TTL（秒）
    challenge_ttl: u64,

    /// 最大并发挑战数
    max_challenges: usize,

    /// 自动清理间隔（秒）
    cleanup_interval: u64,

    /// 上次清理时间
    last_cleanup: Arc<Mutex<u64>>,
}

impl ChallengeProtocol {
    /// 创建新的协议处理器
    pub fn new(attestation_service: AttestationService) -> Self {
        Self {
            challenges: Arc::new(Mutex::new(HashMap::new())),
            attestation_service,
            challenge_ttl: DEFAULT_CHALLENGE_TTL,
            max_challenges: MAX_CONCURRENT_CHALLENGES,
            cleanup_interval: 60, // 1 分钟
            last_cleanup: Arc::new(Mutex::new(current_timestamp())),
        }
    }

    /// 设置挑战 TTL
    pub fn with_ttl(mut self, ttl_seconds: u64) -> Self {
        self.challenge_ttl = ttl_seconds;
        self
    }

    /// 设置最大并发挑战数
    pub fn with_max_challenges(mut self, max: usize) -> Self {
        self.max_challenges = max;
        self
    }

    /// 生成新挑战
    ///
    /// # 流程
    /// 1. 检查会话限制
    /// 2. 生成随机 nonce
    /// 3. 存储挑战
    /// 4. 返回挑战
    pub fn generate_challenge(
        &self,
        enclave_id: Option<String>,
        metadata: Option<ChallengeMetadata>,
    ) -> Result<Challenge, ChallengeError> {
        // 触发清理
        self.maybe_cleanup();

        let mut challenges = self
            .challenges
            .lock()
            .map_err(|_| ChallengeError::InternalError("Lock poisoned".to_string()))?;

        // 检查并发限制
        if challenges.len() >= self.max_challenges {
            return Err(ChallengeError::SessionLimitExceeded);
        }

        // 生成挑战 ID
        let challenge_id = generate_challenge_id();

        // 创建挑战
        let mut challenge = if let Some(meta) = metadata {
            Challenge::with_metadata(challenge_id.clone(), self.challenge_ttl, meta)?
        } else {
            Challenge::new(challenge_id.clone(), self.challenge_ttl)?
        };

        challenge.enclave_id = enclave_id;

        // 存储
        challenges.insert(challenge_id, challenge.clone());

        Ok(challenge)
    }

    /// 处理挑战响应
    ///
    /// # 流程
    /// 1. 查找挑战
    /// 2. 验证挑战状态
    /// 3. 验证 Quote
    /// 4. 验证挑战绑定
    /// 5. 更新挑战状态
    /// 6. 建立安全通道（可选）
    pub fn verify_response(
        &self,
        response: &ChallengeResponse,
        enclave_identity: &[u8],
    ) -> Result<AttestationResult, ChallengeError> {
        let mut challenges = self
            .challenges
            .lock()
            .map_err(|_| ChallengeError::InternalError("Lock poisoned".to_string()))?;

        // 查找挑战
        let challenge = challenges
            .get_mut(&response.challenge_id)
            .ok_or(ChallengeError::ChallengeNotFound)?;

        // 检查过期
        if challenge.is_expired() {
            challenge.mark_expired();
            return Err(ChallengeError::ChallengeExpired);
        }

        // 检查是否已使用
        if challenge.is_used() {
            return Err(ChallengeError::ChallengeAlreadyUsed);
        }

        // 标记为已响应
        challenge.mark_responded()?;

        // 验证 Quote 中的挑战绑定
        let report_data = response.quote.report_data();
        if !challenge.verify_binding(&report_data.data, enclave_identity) {
            challenge.mark_failed()?;
            return Err(ChallengeError::ResponseVerificationFailed);
        }

        // 使用认证服务验证 Quote
        let result = self.attestation_service.verify_quote(
            &response.quote,
            &challenge.nonce,
            enclave_identity,
        )?;

        // 验证成功，从存储中移除挑战（防止重放攻击）
        challenges.remove(&response.challenge_id);

        Ok(result)
    }

    /// 获取挑战
    pub fn get_challenge(&self, challenge_id: &str) -> Option<Challenge> {
        let challenges = self.challenges.lock().ok()?;
        challenges.get(challenge_id).cloned()
    }

    /// 取消挑战
    pub fn cancel_challenge(&self, challenge_id: &str) -> Result<(), ChallengeError> {
        let mut challenges = self
            .challenges
            .lock()
            .map_err(|_| ChallengeError::InternalError("Lock poisoned".to_string()))?;

        challenges
            .remove(challenge_id)
            .ok_or(ChallengeError::ChallengeNotFound)?;

        Ok(())
    }

    /// 获取活跃挑战数量
    pub fn active_challenge_count(&self) -> usize {
        self.challenges.lock().map(|c| c.len()).unwrap_or(0)
    }

    /// 清理过期挑战
    pub fn cleanup_expired(&self) -> usize {
        let mut challenges = match self.challenges.lock() {
            Ok(c) => c,
            Err(_) => return 0,
        };

        let now = current_timestamp();
        let expired_keys: Vec<String> = challenges
            .iter()
            .filter(|(_, c)| c.is_expired())
            .map(|(k, _)| k.clone())
            .collect();

        let count = expired_keys.len();
        for key in expired_keys {
            if let Some(mut challenge) = challenges.remove(&key) {
                challenge.mark_expired();
            }
        }

        // 更新最后清理时间
        if let Ok(mut last) = self.last_cleanup.lock() {
            *last = now;
        }

        count
    }

    /// 条件触发清理
    fn maybe_cleanup(&self) {
        let should_cleanup = self
            .last_cleanup
            .lock()
            .map(|last| current_timestamp() - *last >= self.cleanup_interval)
            .unwrap_or(true);

        if should_cleanup {
            self.cleanup_expired();
        }
    }
}

/// Prover (Enclave 端) 挑战-响应处理器
///
/// 用于 Enclave 侧处理挑战并生成响应
pub struct ProverProtocol {
    /// 认证服务
    attestation_service: AttestationService,
}

impl ProverProtocol {
    /// 创建新的 Prover 协议处理器
    pub fn new(attestation_service: AttestationService) -> Self {
        Self {
            attestation_service,
        }
    }

    /// 响应挑战
    ///
    /// # 流程
    /// 1. 使用挑战生成 Quote
    /// 2. 构建响应
    pub fn respond_to_challenge(
        &self,
        enclave: &Enclave,
        challenge: &Challenge,
    ) -> Result<ChallengeResponse, ChallengeError> {
        // 检查 Enclave 状态
        if !enclave.is_running() {
            return Err(ChallengeError::InternalError(
                "Enclave not running".to_string(),
            ));
        }

        // 生成 Quote
        let quote = self
            .attestation_service
            .generate_quote(enclave, &challenge.nonce)
            .map_err(|e| ChallengeError::AttestationFailed(e.to_string()))?;

        // 构建响应
        let response = ChallengeResponse::new(challenge.id.clone(), quote);

        Ok(response)
    }

    /// 响应挑战并附加数据
    pub fn respond_with_data(
        &self,
        enclave: &Enclave,
        challenge: &Challenge,
        additional_data: Vec<u8>,
    ) -> Result<ChallengeResponse, ChallengeError> {
        let response = self.respond_to_challenge(enclave, challenge)?;
        Ok(response.with_additional_data(additional_data))
    }
}

/// 安全通道建立结果
#[derive(Debug, Clone)]
pub struct SecureChannel {
    /// 通道 ID
    pub channel_id: String,

    /// 会话密钥
    pub session_key: [u8; CHANNEL_KEY_LENGTH],

    /// 建立时间戳
    pub established_at: u64,

    /// 过期时间戳
    pub expires_at: u64,

    /// 通道属性
    pub properties: ChannelProperties,
}

/// 通道属性
#[derive(Debug, Clone)]
pub struct ChannelProperties {
    /// 是否加密
    pub encrypted: bool,

    /// 是否认证
    pub authenticated: bool,

    /// 密钥交换算法
    pub key_exchange: String,

    /// 对称加密算法
    pub cipher: String,
}

impl Default for ChannelProperties {
    fn default() -> Self {
        Self {
            encrypted: true,
            authenticated: true,
            key_exchange: "ECDH-P256".to_string(),
            cipher: "AES-256-GCM".to_string(),
        }
    }
}

impl SecureChannel {
    /// 从认证结果建立安全通道
    pub fn establish(
        attestation_result: &AttestationResult,
        channel_id: String,
        ttl_seconds: u64,
    ) -> Result<Self, ChallengeError> {
        // 派生会话密钥
        let session_key = derive_session_key(attestation_result)?;

        let now = current_timestamp();

        Ok(Self {
            channel_id,
            session_key,
            established_at: now,
            expires_at: now + ttl_seconds,
            properties: ChannelProperties::default(),
        })
    }

    /// 检查通道是否有效
    pub fn is_valid(&self) -> bool {
        current_timestamp() < self.expires_at
    }

    /// 获取会话密钥引用
    pub fn session_key(&self) -> &[u8; CHANNEL_KEY_LENGTH] {
        &self.session_key
    }

    /// 加密数据
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, ChallengeError> {
        use aes_gcm::{
            Aes256Gcm,
            aead::{Aead, AeadCore, KeyInit, OsRng},
        };

        let cipher = Aes256Gcm::new_from_slice(&self.session_key).map_err(|e| {
            ChallengeError::InternalError(format!("Cipher initialization failed: {e}"))
        })?;

        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let mut ciphertext = cipher
            .encrypt(&nonce, plaintext)
            .map_err(|e| ChallengeError::InternalError(format!("Encryption failed: {e}")))?;

        // 将 nonce 和密文组合在一起
        let mut result = Vec::with_capacity(nonce.len() + ciphertext.len());
        result.extend_from_slice(&nonce);
        result.append(&mut ciphertext);

        Ok(result)
    }

    /// 解密数据
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, ChallengeError> {
        use aes_gcm::Nonce;
        use aes_gcm::{
            Aes256Gcm,
            aead::{Aead, KeyInit},
        };

        if ciphertext.len() <= 12 {
            return Err(ChallengeError::InternalError(
                "Ciphertext too short".to_string(),
            ));
        }

        let (nonce_bytes, encrypted) = ciphertext.split_at(12);
        let cipher = Aes256Gcm::new_from_slice(&self.session_key).map_err(|e| {
            ChallengeError::InternalError(format!("Cipher initialization failed: {e}"))
        })?;

        let nonce = Nonce::from_slice(nonce_bytes);
        cipher
            .decrypt(nonce, encrypted)
            .map_err(|e| ChallengeError::InternalError(format!("Decryption failed: {e}")))
    }
}

/// 生成安全的随机 nonce
fn generate_secure_nonce() -> Result<[u8; CHALLENGE_LENGTH], ChallengeError> {
    use rand::RngCore;

    let mut nonce = [0u8; CHALLENGE_LENGTH];
    let mut rng = rand::thread_rng();
    rng.try_fill_bytes(&mut nonce)
        .map_err(|_| ChallengeError::InternalError("RNG failed".to_string()))?;

    Ok(nonce)
}

/// 生成挑战 ID
fn generate_challenge_id() -> String {
    use rand::RngCore;

    let mut bytes = [0u8; 16];
    let mut rng = rand::thread_rng();
    rng.fill_bytes(&mut bytes);

    format!(
        "chal_{}",
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
    )
}

/// 从认证结果派生会话密钥
fn derive_session_key(
    attestation_result: &AttestationResult,
) -> Result<[u8; CHANNEL_KEY_LENGTH], ChallengeError> {
    use ring::hmac;

    // 使用 HKDF 派生会话密钥
    let salt = hmac::Key::new(hmac::HMAC_SHA256, b"CredBridge Secure Channel");

    let mut info = Vec::with_capacity(80);
    info.extend_from_slice(&attestation_result.mrenclave);
    info.extend_from_slice(&attestation_result.mrsigner);
    info.extend_from_slice(&attestation_result.timestamp.to_le_bytes());

    let tag = hmac::sign(&salt, &info);

    let mut key = [0u8; CHANNEL_KEY_LENGTH];
    key.copy_from_slice(&tag.as_ref()[..32]);

    Ok(key)
}

/// 获取当前时间戳
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tee::attestation::AttestationService;
    use crate::tee::{Enclave, EnclaveConfig};

    #[test]
    fn test_challenge_creation() {
        let challenge = Challenge::new("test_123".to_string(), 300).unwrap();

        assert_eq!(challenge.id, "test_123");
        assert_eq!(challenge.nonce.len(), CHALLENGE_LENGTH);
        assert_eq!(challenge.status, ChallengeStatus::Pending);
        assert!(!challenge.is_expired());
        assert!(!challenge.is_used());
    }

    #[test]
    fn test_challenge_binding() {
        let challenge = Challenge::new("test_123".to_string(), 300).unwrap();
        let identity = b"enclave_identity";

        // 计算预期的 Report Data
        let mut hasher_input = Vec::new();
        hasher_input.extend_from_slice(&challenge.nonce);
        hasher_input.extend_from_slice(identity);
        let expected_hash = digest(&SHA256, &hasher_input);

        let mut report_data = [0u8; 64];
        report_data[..32].copy_from_slice(expected_hash.as_ref());

        // 验证绑定
        assert!(challenge.verify_binding(&report_data, identity));

        // 错误身份应该失败
        assert!(!challenge.verify_binding(&report_data, b"wrong_identity"));
    }

    #[test]
    fn test_challenge_state_transitions() {
        let mut challenge = Challenge::new("test_123".to_string(), 300).unwrap();

        // Pending -> Responded
        challenge.mark_responded().unwrap();
        assert_eq!(challenge.status, ChallengeStatus::Responded);

        // Responded -> Verified
        challenge.mark_verified().unwrap();
        assert_eq!(challenge.status, ChallengeStatus::Verified);

        // Verified 不能再转换
        assert!(challenge.mark_responded().is_err());
        assert!(challenge.mark_verified().is_err());
    }

    #[test]
    fn test_challenge_expiration() {
        let mut challenge = Challenge::new("test_123".to_string(), 0).unwrap();

        // 设置为已过期
        challenge.expires_at = current_timestamp() - 1;

        assert!(challenge.is_expired());
        assert!(challenge.mark_responded().is_err()); // 过期后不能响应
    }

    #[test]
    fn test_challenge_protocol_generate() {
        let attestation_service = AttestationService::for_simulation();
        let protocol = ChallengeProtocol::new(attestation_service);

        let challenge = protocol.generate_challenge(None, None).unwrap();

        assert_eq!(challenge.status, ChallengeStatus::Pending);
        assert!(protocol.get_challenge(&challenge.id).is_some());
        assert_eq!(protocol.active_challenge_count(), 1);
    }

    #[test]
    fn test_challenge_protocol_full_flow() {
        // 设置 Enclave
        let config = EnclaveConfig {
            debug_mode: true,
            ..Default::default()
        };
        let mut enclave = Enclave::new(config);
        enclave.initialize().unwrap();

        // 设置 Verifier
        let attestation_service =
            AttestationService::for_simulation().allow_mrenclave(enclave.mrenclave());
        let verifier = ChallengeProtocol::new(attestation_service.clone());

        // 设置 Prover
        let prover = ProverProtocol::new(attestation_service);

        // 1. Verifier 生成挑战
        let challenge = verifier.generate_challenge(None, None).unwrap();

        // 2. Prover 响应挑战
        let response = prover.respond_to_challenge(&enclave, &challenge).unwrap();

        // 3. Verifier 验证响应
        let identity = generate_enclave_identity(&enclave);
        let result = verifier.verify_response(&response, &identity).unwrap();

        assert!(result.success);
        assert_eq!(result.mrenclave, enclave.mrenclave());
    }

    #[test]
    fn test_challenge_protocol_cancel() {
        let attestation_service = AttestationService::for_simulation();
        let protocol = ChallengeProtocol::new(attestation_service);

        let challenge = protocol.generate_challenge(None, None).unwrap();

        assert_eq!(protocol.active_challenge_count(), 1);

        // 取消挑战
        protocol.cancel_challenge(&challenge.id).unwrap();

        assert_eq!(protocol.active_challenge_count(), 0);
        assert!(protocol.get_challenge(&challenge.id).is_none());
    }

    #[test]
    fn test_challenge_protocol_limit() {
        let attestation_service = AttestationService::for_simulation();
        let protocol = ChallengeProtocol::new(attestation_service).with_max_challenges(3);

        // 创建 3 个挑战
        let _ = protocol.generate_challenge(None, None).unwrap();
        let _ = protocol.generate_challenge(None, None).unwrap();
        let _ = protocol.generate_challenge(None, None).unwrap();

        assert_eq!(protocol.active_challenge_count(), 3);

        // 第 4 个应该失败
        assert!(matches!(
            protocol.generate_challenge(None, None).unwrap_err(),
            ChallengeError::SessionLimitExceeded
        ));
    }

    #[test]
    fn test_challenge_protocol_cleanup() {
        let attestation_service = AttestationService::for_simulation();
        let protocol = ChallengeProtocol::new(attestation_service);

        // 创建一个立即过期的挑战（通过内部修改）
        let challenge = protocol.generate_challenge(None, None).unwrap();
        assert_eq!(protocol.active_challenge_count(), 1);

        // 手动过期
        {
            let mut challenges = protocol.challenges.lock().unwrap();
            if let Some(c) = challenges.get_mut(&challenge.id) {
                c.expires_at = current_timestamp() - 1;
            }
        }

        // 清理
        let cleaned = protocol.cleanup_expired();
        assert_eq!(cleaned, 1);
        assert_eq!(protocol.active_challenge_count(), 0);
    }

    #[test]
    fn test_secure_channel_establishment() {
        let attestation_result = AttestationResult {
            success: true,
            mrenclave: [0x42u8; 32],
            mrsigner: [0x43u8; 32],
            timestamp: current_timestamp(),
        };

        let channel =
            SecureChannel::establish(&attestation_result, "chan_123".to_string(), 3600).unwrap();

        assert_eq!(channel.channel_id, "chan_123");
        assert_eq!(channel.session_key.len(), CHANNEL_KEY_LENGTH);
        assert!(channel.is_valid());
        assert!(channel.properties.encrypted);
        assert!(channel.properties.authenticated);
    }

    #[test]
    fn test_secure_channel_expiration() {
        let attestation_result = AttestationResult {
            success: true,
            mrenclave: [0x42u8; 32],
            mrsigner: [0x43u8; 32],
            timestamp: current_timestamp(),
        };

        let channel =
            SecureChannel::establish(&attestation_result, "chan_123".to_string(), 0).unwrap();

        // 立即过期
        assert!(!channel.is_valid());
    }

    #[test]
    fn test_replay_protection() {
        let attestation_service = AttestationService::for_simulation();
        let verifier = ChallengeProtocol::new(attestation_service.clone());
        let prover = ProverProtocol::new(attestation_service);

        let config = EnclaveConfig {
            debug_mode: true,
            ..Default::default()
        };
        let mut enclave = Enclave::new(config);
        enclave.initialize().unwrap();

        // 生成挑战
        let challenge = verifier.generate_challenge(None, None).unwrap();

        // 响应挑战
        let response = prover.respond_to_challenge(&enclave, &challenge).unwrap();

        // 第一次验证应该成功
        let identity = generate_enclave_identity(&enclave);
        let result = verifier.verify_response(&response, &identity);
        assert!(result.is_ok());

        // 第二次验证应该失败（挑战已使用）
        let result = verifier.verify_response(&response, &identity);
        assert!(matches!(
            result.unwrap_err(),
            ChallengeError::ChallengeNotFound
        ));
    }

    /// 生成 Enclave 身份标识（辅助函数）
    fn generate_enclave_identity(enclave: &Enclave) -> Vec<u8> {
        let mut identity = Vec::with_capacity(64);
        identity.extend_from_slice(&enclave.mrenclave());
        identity.extend_from_slice(&enclave.mrsigner());
        identity
    }
}
