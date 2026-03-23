//! 审计记录器
//!
//! 实现审计事件的记录、签名和存储
//! 支持 Merkle Tree 哈希和数字签名

use super::events::{AuditEntry, AuditEventId, Outcome};
use ring::digest::{SHA256, digest};
use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// 审计记录器错误类型
#[derive(Error, Debug, Clone, PartialEq)]
pub enum RecorderError {
    #[error("存储错误: {0}")]
    StorageError(String),

    #[error("签名错误: {0}")]
    SigningError(String),

    #[error("验证错误: {0}")]
    VerificationError(String),

    #[error("序列化错误: {0}")]
    SerializationError(String),

    #[error("条目未找到: {0}")]
    EntryNotFound(AuditEventId),

    #[error("链完整性错误: {0}")]
    ChainIntegrityError(String),

    #[error("密钥错误: {0}")]
    KeyError(String),
}

/// 签名后的审计条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedAuditEntry {
    /// 原始审计条目
    pub entry: AuditEntry,

    /// 条目内容哈希（Merkle Tree 叶子节点）
    #[serde(
        serialize_with = "serialize_bytes",
        deserialize_with = "deserialize_bytes_32"
    )]
    pub content_hash: [u8; 32],

    /// 前一个条目的哈希（用于链式结构）
    #[serde(
        serialize_with = "serialize_bytes",
        deserialize_with = "deserialize_bytes_32"
    )]
    pub prev_hash: [u8; 32],

    /// 数字签名
    #[serde(serialize_with = "serialize_vec", deserialize_with = "deserialize_vec")]
    pub signature: Vec<u8>,

    /// 签名者的公钥指纹
    pub signer_fingerprint: String,

    /// 条目在日志中的索引
    pub log_index: u64,

    /// Merkle Tree 根哈希（截至此条目）
    #[serde(
        serialize_with = "serialize_bytes",
        deserialize_with = "deserialize_bytes_32"
    )]
    pub merkle_root: [u8; 32],
}

fn serialize_bytes<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&hex::encode(bytes))
}

fn deserialize_bytes_32<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
where
    D: serde::Deserializer<'de>,
{
    let hex_str = String::deserialize(deserializer)?;
    let bytes = hex::decode(&hex_str).map_err(serde::de::Error::custom)?;
    if bytes.len() != 32 {
        return Err(serde::de::Error::custom(format!(
            "Expected 32 bytes, got {}",
            bytes.len()
        )));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

fn serialize_vec<S>(vec: &[u8], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&hex::encode(vec))
}

fn deserialize_vec<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let hex_str = String::deserialize(deserializer)?;
    hex::decode(&hex_str).map_err(serde::de::Error::custom)
}

impl SignedAuditEntry {
    /// 将条目序列化为 JSON
    pub fn to_json(&self) -> Result<String, RecorderError> {
        #[derive(Serialize)]
        struct SignedEntrySer<'a> {
            entry: &'a AuditEntry,
            content_hash: String,
            prev_hash: String,
            signature: String,
            signer_fingerprint: &'a str,
            log_index: u64,
            merkle_root: String,
        }

        let ser = SignedEntrySer {
            entry: &self.entry,
            content_hash: hex::encode(self.content_hash),
            prev_hash: hex::encode(self.prev_hash),
            signature: hex::encode(&self.signature),
            signer_fingerprint: &self.signer_fingerprint,
            log_index: self.log_index,
            merkle_root: hex::encode(self.merkle_root),
        };

        serde_json::to_string(&ser).map_err(|e| RecorderError::SerializationError(e.to_string()))
    }
}

/// 审计日志链
///
/// 维护不可篡改的审计日志链，使用 Merkle Tree 和数字签名
#[derive(Debug)]
pub struct AuditLogChain {
    /// 已签名条目
    entries: VecDeque<SignedAuditEntry>,

    /// Merkle Tree 根哈希
    merkle_root: [u8; 32],

    /// 当前日志索引
    current_index: u64,

    /// 链的创世哈希
    genesis_hash: [u8; 32],
}

impl AuditLogChain {
    /// 创建新的审计日志链
    pub fn new() -> Self {
        let genesis_hash = Self::compute_genesis_hash();
        Self {
            entries: VecDeque::new(),
            merkle_root: genesis_hash,
            current_index: 0,
            genesis_hash,
        }
    }

    /// 计算创世哈希
    fn compute_genesis_hash() -> [u8; 32] {
        let genesis_data = b"CredBridge Audit Log Genesis Block";
        let digest = digest(&SHA256, genesis_data);
        let mut hash = [0u8; 32];
        hash.copy_from_slice(digest.as_ref());
        hash
    }

    /// 添加已签名条目到链
    pub fn append(&mut self, signed_entry: SignedAuditEntry) {
        self.current_index = signed_entry.log_index + 1;
        self.merkle_root = signed_entry.merkle_root;
        self.entries.push_back(signed_entry);
    }

    /// 获取最后一个条目的哈希
    pub fn last_hash(&self) -> [u8; 32] {
        self.entries
            .back()
            .map(|e| e.content_hash)
            .unwrap_or(self.genesis_hash)
    }

    /// 获取当前日志索引
    pub fn current_index(&self) -> u64 {
        self.current_index
    }

    /// 获取 Merkle Tree 根哈希
    pub fn merkle_root(&self) -> [u8; 32] {
        self.merkle_root
    }

    /// 获取条目数量
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 检查链是否为空
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 按索引获取条目
    pub fn get(&self, index: u64) -> Option<&SignedAuditEntry> {
        self.entries.iter().find(|e| e.log_index == index)
    }

    /// 获取最近的 N 个条目
    pub fn recent(&self, n: usize) -> Vec<&SignedAuditEntry> {
        self.entries
            .iter()
            .rev()
            .take(n)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// 验证链的完整性
    pub fn verify_chain(&self, public_key: &[u8]) -> Result<bool, RecorderError> {
        let mut prev_hash = self.genesis_hash;

        for entry in &self.entries {
            // 验证链式结构
            if entry.prev_hash != prev_hash {
                return Ok(false);
            }

            // 验证内容哈希
            let computed_hash = entry.entry.content_hash();
            if computed_hash != entry.content_hash {
                return Ok(false);
            }

            // 验证签名 (Ed25519)
            let public_key_unparsed = UnparsedPublicKey::new(&ED25519, public_key);
            // 签名数据结构: SHA256(content_hash + prev_hash)
            let combined_data =
                [entry.content_hash.as_slice(), entry.prev_hash.as_slice()].concat();
            let combined_hash = digest(&SHA256, &combined_data);
            match public_key_unparsed.verify(combined_hash.as_ref(), &entry.signature) {
                Ok(_) => {}
                Err(_) => return Ok(false),
            }

            prev_hash = entry.content_hash;
        }

        Ok(true)
    }

    /// 生成审计报告
    pub fn generate_report(&self, start_time: Option<u64>, end_time: Option<u64>) -> AuditReport {
        let filtered: Vec<_> = self
            .entries
            .iter()
            .filter(|e| {
                let t = e.entry.timestamp;
                let after_start = start_time.map(|s| t >= s).unwrap_or(true);
                let before_end = end_time.map(|e| t <= e).unwrap_or(true);
                after_start && before_end
            })
            .collect();

        let total_count = filtered.len();
        let success_count = filtered
            .iter()
            .filter(|e| e.entry.outcome.is_success())
            .count();
        let failure_count = filtered
            .iter()
            .filter(|e| e.entry.outcome.is_failure())
            .count();

        AuditReport {
            total_entries: total_count,
            success_count,
            failure_count,
            merkle_root: hex::encode(self.merkle_root),
            start_time,
            end_time,
        }
    }
}

impl Default for AuditLogChain {
    fn default() -> Self {
        Self::new()
    }
}

/// 审计报告
#[derive(Debug, Clone)]
pub struct AuditReport {
    /// 总条目数
    pub total_entries: usize,
    /// 成功数
    pub success_count: usize,
    /// 失败数
    pub failure_count: usize,
    /// Merkle Tree 根哈希
    pub merkle_root: String,
    /// 开始时间
    pub start_time: Option<u64>,
    /// 结束时间
    pub end_time: Option<u64>,
}

/// 审计签名密钥对
pub struct SigningKeyPair {
    /// 私钥（用于签名）
    private_key: Vec<u8>,
    /// 公钥（用于验证）
    public_key: Vec<u8>,
    /// 密钥指纹
    fingerprint: String,
}

impl SigningKeyPair {
    /// 生成新的 Ed25519 密钥对用于审计签名
    pub fn generate() -> Result<Self, RecorderError> {
        // 使用 ring 生成 Ed25519 密钥对
        let rng = ring::rand::SystemRandom::new();
        let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng)
            .map_err(|e| RecorderError::KeyError(format!("密钥生成失败: {e:?}")))?;

        let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes.as_ref())
            .map_err(|e| RecorderError::KeyError(format!("密钥解析失败: {e:?}")))?;

        let public_key = key_pair.public_key().as_ref().to_vec();
        let fingerprint = Self::compute_fingerprint(&public_key);

        Ok(Self {
            private_key: pkcs8_bytes.as_ref().to_vec(),
            public_key,
            fingerprint,
        })
    }

    /// 计算公钥指纹
    fn compute_fingerprint(public_key: &[u8]) -> String {
        let digest = digest(&SHA256, public_key);
        hex::encode(&digest.as_ref()[..16]) // 只取前 16 字节
    }

    /// 获取公钥
    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    /// 获取密钥指纹
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// 对数据进行签名
    pub fn sign(&self, data: &[u8]) -> Result<Vec<u8>, RecorderError> {
        let key_pair = Ed25519KeyPair::from_pkcs8(&self.private_key)
            .map_err(|e| RecorderError::SigningError(format!("密钥加载失败: {e:?}")))?;

        let signature = key_pair.sign(data);
        Ok(signature.as_ref().to_vec())
    }
}

/// 审计记录器
///
/// 主审计记录器，负责签名和存储审计事件
pub struct AuditRecorder {
    /// 审计日志链
    chain: Arc<Mutex<AuditLogChain>>,

    /// 签名密钥对
    signing_key: SigningKeyPair,

    /// 最大保留条目数（内存中）
    max_entries: usize,
}

impl AuditRecorder {
    /// 创建新的审计记录器
    ///
    /// # 参数
    /// - `max_entries`: 内存中最大保留条目数
    pub fn new(max_entries: usize) -> Result<Self, RecorderError> {
        let signing_key = SigningKeyPair::generate()?;

        Ok(Self {
            chain: Arc::new(Mutex::new(AuditLogChain::new())),
            signing_key,
            max_entries,
        })
    }

    /// 使用指定密钥创建审计记录器
    pub fn with_key(max_entries: usize, signing_key: SigningKeyPair) -> Self {
        Self {
            chain: Arc::new(Mutex::new(AuditLogChain::new())),
            signing_key,
            max_entries,
        }
    }

    /// 记录审计事件
    ///
    /// # 参数
    /// - `entry`: 审计条目
    ///
    /// # 返回值
    /// 返回签名后的审计条目
    pub fn record(&self, entry: AuditEntry) -> Result<SignedAuditEntry, RecorderError> {
        let mut chain = self
            .chain
            .lock()
            .map_err(|e| RecorderError::StorageError(format!("锁获取失败: {e}")))?;

        let log_index = chain.current_index();
        let content_hash = entry.content_hash();
        let prev_hash = chain.last_hash();

        // 计算组合哈希（内容 + 前一个哈希）
        let combined_data = [content_hash.as_slice(), prev_hash.as_slice()].concat();
        let combined_hash = digest(&SHA256, &combined_data);

        // 签名
        let signature = self.signing_key.sign(combined_hash.as_ref())?;

        // 计算新的 Merkle Tree 根哈希
        let merkle_root = self.compute_merkle_root(&chain, content_hash);

        let signed_entry = SignedAuditEntry {
            entry,
            content_hash,
            prev_hash,
            signature,
            signer_fingerprint: self.signing_key.fingerprint().to_string(),
            log_index,
            merkle_root,
        };

        chain.append(signed_entry.clone());

        // 清理旧条目（如果超过限制）
        self.cleanup_old_entries(&mut chain)?;

        Ok(signed_entry)
    }

    /// 计算 Merkle Tree 根哈希
    fn compute_merkle_root(&self, chain: &AuditLogChain, new_content_hash: [u8; 32]) -> [u8; 32] {
        let last_hash = chain.last_hash();
        let combined = [last_hash.as_slice(), new_content_hash.as_slice()].concat();
        let digest = digest(&SHA256, &combined);
        let mut root = [0u8; 32];
        root.copy_from_slice(digest.as_ref());
        root
    }

    /// 清理旧条目
    fn cleanup_old_entries(&self, chain: &mut AuditLogChain) -> Result<(), RecorderError> {
        while chain.len() > self.max_entries {
            chain.entries.pop_front();
        }
        Ok(())
    }

    /// 按索引获取条目
    pub fn get_entry(&self, index: u64) -> Result<Option<SignedAuditEntry>, RecorderError> {
        let chain = self
            .chain
            .lock()
            .map_err(|e| RecorderError::StorageError(format!("锁获取失败: {e}")))?;
        Ok(chain.get(index).cloned())
    }

    /// 获取最近的条目
    pub fn recent_entries(&self, n: usize) -> Result<Vec<SignedAuditEntry>, RecorderError> {
        let chain = self
            .chain
            .lock()
            .map_err(|e| RecorderError::StorageError(format!("锁获取失败: {e}")))?;
        Ok(chain.recent(n).into_iter().cloned().collect())
    }

    /// 获取当前 Merkle Tree 根哈希
    pub fn merkle_root(&self) -> Result<[u8; 32], RecorderError> {
        let chain = self
            .chain
            .lock()
            .map_err(|e| RecorderError::StorageError(format!("锁获取失败: {e}")))?;
        Ok(chain.merkle_root())
    }

    /// 验证审计链完整性
    pub fn verify_chain(&self) -> Result<bool, RecorderError> {
        let chain = self
            .chain
            .lock()
            .map_err(|e| RecorderError::StorageError(format!("锁获取失败: {e}")))?;
        chain.verify_chain(&self.signing_key.public_key)
    }

    /// 获取公钥
    pub fn public_key(&self) -> &[u8] {
        self.signing_key.public_key()
    }

    /// 获取密钥指纹
    pub fn fingerprint(&self) -> &str {
        self.signing_key.fingerprint()
    }

    /// 生成审计报告
    pub fn generate_report(
        &self,
        start_time: Option<u64>,
        end_time: Option<u64>,
    ) -> Result<AuditReport, RecorderError> {
        let chain = self
            .chain
            .lock()
            .map_err(|e| RecorderError::StorageError(format!("锁获取失败: {e}")))?;
        Ok(chain.generate_report(start_time, end_time))
    }

    /// 获取条目总数
    pub fn entry_count(&self) -> Result<usize, RecorderError> {
        let chain = self
            .chain
            .lock()
            .map_err(|e| RecorderError::StorageError(format!("锁获取失败: {e}")))?;
        Ok(chain.len())
    }

    /// 导出所有条目为 JSON
    pub fn export_json(&self) -> Result<String, RecorderError> {
        let chain = self
            .chain
            .lock()
            .map_err(|e| RecorderError::StorageError(format!("锁获取失败: {e}")))?;

        // 克隆条目以拥有所有权
        let entries: Vec<SignedAuditEntry> = chain.entries.iter().cloned().collect();
        serde_json::to_string(&entries)
            .map_err(|e| RecorderError::SerializationError(e.to_string()))
    }
}

/// 内存存储的审计后端
pub struct MemoryAuditStorage {
    /// 审计记录器
    recorder: AuditRecorder,
}

impl MemoryAuditStorage {
    /// 创建新的内存审计存储
    pub fn new(max_entries: usize) -> Result<Self, RecorderError> {
        let recorder = AuditRecorder::new(max_entries)?;
        Ok(Self { recorder })
    }

    /// 记录审计事件
    pub fn record(&self, entry: AuditEntry) -> Result<SignedAuditEntry, RecorderError> {
        self.recorder.record(entry)
    }

    /// 查询最近的审计事件
    pub fn query_recent(&self, n: usize) -> Result<Vec<SignedAuditEntry>, RecorderError> {
        self.recorder.recent_entries(n)
    }

    /// 按索引获取审计事件
    pub fn get_by_index(&self, index: u64) -> Result<Option<SignedAuditEntry>, RecorderError> {
        self.recorder.get_entry(index)
    }

    /// 按结果筛选事件
    pub fn query_by_outcome(
        &self,
        outcome: Outcome,
    ) -> Result<Vec<SignedAuditEntry>, RecorderError> {
        let all = self.recorder.recent_entries(self.recorder.entry_count()?)?;
        Ok(all
            .into_iter()
            .filter(|e| e.entry.outcome == outcome)
            .collect())
    }

    /// 验证链完整性
    pub fn verify(&self) -> Result<bool, RecorderError> {
        self.recorder.verify_chain()
    }

    /// 获取审计记录器引用
    pub fn recorder(&self) -> &AuditRecorder {
        &self.recorder
    }
}

/// 便捷的审计记录宏（简化用法）
#[macro_export]
macro_rules! audit_record {
    ($recorder:expr, $user_id_hash:expr, $session_id:expr, $service:expr, $action:expr, $outcome:expr, $tee_mrenclave:expr, $action_token_jti:expr) => {
        {
            let entry = $crate::audit::AuditEntry::new(
                $user_id_hash,
                $session_id,
                $service,
                $action,
                $outcome,
                $tee_mrenclave,
                $action_token_jti,
            );
            $recorder.record(entry)
        }
    };
    ($recorder:expr, $user_id_hash:expr, $session_id:expr, $service:expr, $action:expr, $outcome:expr, $tee_mrenclave:expr, $action_token_jti:expr, $($key:expr => $value:expr),+) => {
        {
            let entry = $crate::audit::AuditEntry::new(
                $user_id_hash,
                $session_id,
                $service,
                $action,
                $outcome,
                $tee_mrenclave,
                $action_token_jti,
            )
            $(.with_param($key, $value))+
            ;
            $recorder.record(entry)
        }
    };
}

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::super::events::AuditAction;
    use super::*;

    #[test]
    fn test_audit_log_chain_new() {
        let chain = AuditLogChain::new();
        assert_eq!(chain.current_index(), 0);
        assert_eq!(chain.len(), 0);
        assert!(chain.is_empty());
        // 创世链有一个创世哈希
    }

    #[test]
    fn test_signing_key_pair_generate() {
        let key_pair = SigningKeyPair::generate().unwrap();
        assert!(!key_pair.public_key().is_empty());
        assert!(!key_pair.fingerprint().is_empty());
    }

    #[test]
    fn test_signing_key_pair_sign_and_verify() {
        let key_pair = SigningKeyPair::generate().unwrap();
        let data = b"test data to sign";

        let signature = key_pair.sign(data).unwrap();
        assert!(!signature.is_empty());

        // 使用公钥验证签名 (Ed25519)
        let public_key = UnparsedPublicKey::new(&ED25519, key_pair.public_key());
        let result = public_key.verify(data, &signature);
        assert!(result.is_ok());
    }

    #[test]
    fn test_audit_recorder_new() {
        let recorder = AuditRecorder::new(100).unwrap();
        assert!(!recorder.fingerprint().is_empty());
        assert!(!recorder.public_key().is_empty());
    }

    #[test]
    fn test_audit_recorder_record() {
        let recorder = AuditRecorder::new(100).unwrap();
        let entry = AuditEntry::new(
            "user_hash",
            "session_123",
            "vault-service",
            AuditAction::CredentialDecrypt,
            Outcome::Success,
            "mrenclave_abc",
            "jti_xyz",
        );

        let signed = recorder.record(entry).unwrap();
        assert_eq!(signed.log_index, 0);
        assert_eq!(signed.signer_fingerprint, recorder.fingerprint());
        assert!(!signed.signature.is_empty());

        // 验证条目数
        assert_eq!(recorder.entry_count().unwrap(), 1);
    }

    #[test]
    fn test_audit_recorder_chain_integrity() {
        let recorder = AuditRecorder::new(100).unwrap();

        // 记录多个条目
        for i in 0..5 {
            let entry = AuditEntry::new(
                format!("user_hash_{i}"),
                "session_123",
                "vault-service",
                AuditAction::TokenValidate,
                Outcome::Success,
                "mrenclave_abc",
                format!("jti_{i}"),
            );
            recorder.record(entry).unwrap();
        }

        assert_eq!(recorder.entry_count().unwrap(), 5);

        // 验证链完整性
        assert!(recorder.verify_chain().unwrap());
    }

    #[test]
    fn test_audit_recorder_recent_entries() {
        let recorder = AuditRecorder::new(100).unwrap();

        for i in 0..10 {
            let entry = AuditEntry::new(
                format!("user_{i}"),
                "session",
                "service",
                AuditAction::TokenValidate,
                Outcome::Success,
                "mrenclave",
                format!("jti_{i}"),
            );
            recorder.record(entry).unwrap();
        }

        let recent = recorder.recent_entries(3).unwrap();
        assert_eq!(recent.len(), 3);
        // 最近的条目应该有最高的索引
        assert_eq!(recent[2].log_index, 9);
    }

    #[test]
    fn test_memory_audit_storage() {
        let storage = MemoryAuditStorage::new(100).unwrap();

        let entry = AuditEntry::new(
            "user_hash",
            "session",
            "service",
            AuditAction::CredentialDecrypt,
            Outcome::Success,
            "mrenclave",
            "jti",
        );

        storage.record(entry).unwrap();

        let recent = storage.query_recent(10).unwrap();
        assert_eq!(recent.len(), 1);
        assert!(storage.verify().unwrap());
    }

    #[test]
    fn test_audit_recorder_generate_report() {
        let recorder = AuditRecorder::new(100).unwrap();

        // 记录成功和失败的事件
        for i in 0..5 {
            let outcome = if i % 2 == 0 {
                Outcome::Success
            } else {
                Outcome::Failure
            };
            let entry = AuditEntry::new(
                format!("user_{i}"),
                "session",
                "service",
                AuditAction::TokenValidate,
                outcome,
                "mrenclave",
                format!("jti_{i}"),
            );
            recorder.record(entry).unwrap();
        }

        let report = recorder.generate_report(None, None).unwrap();
        assert_eq!(report.total_entries, 5);
        assert_eq!(report.success_count, 3); // 0, 2, 4
        assert_eq!(report.failure_count, 2); // 1, 3
    }

    #[test]
    fn test_audit_recorder_cleanup() {
        let recorder = AuditRecorder::new(3).unwrap();

        // 记录超过限制的条目
        for i in 0..5 {
            let entry = AuditEntry::new(
                format!("user_{i}"),
                "session",
                "service",
                AuditAction::TokenValidate,
                Outcome::Success,
                "mrenclave",
                format!("jti_{i}"),
            );
            recorder.record(entry).unwrap();
        }

        // 由于清理，内存中只保留 max_entries 个
        // 注意：清理只影响 VecDeque，索引继续递增
        assert_eq!(recorder.entry_count().unwrap(), 3);
    }

    #[test]
    fn test_signed_entry_json() {
        let recorder = AuditRecorder::new(100).unwrap();
        let entry = AuditEntry::new(
            "user_hash",
            "session",
            "service",
            AuditAction::CredentialDecrypt,
            Outcome::Success,
            "mrenclave",
            "jti",
        );

        let signed = recorder.record(entry).unwrap();
        let json = signed.to_json().unwrap();

        assert!(json.contains("log_index"));
        assert!(json.contains("content_hash"));
        assert!(json.contains("signature"));
    }
}
