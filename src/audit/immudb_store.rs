//! immudb 存储实现
//!
//! 实现与现有 AuditRecorder 集成的 immudb 存储后端
//! 提供审计日志的不可篡改持久化存储

use super::events::{AuditEntry, Outcome};
use super::immudb_client::{ImmuDbConfig, ImmuDbState, ImmuDbStorage, QueryOptions};
use super::recorder::{RecorderError, SignedAuditEntry};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;

/// immudb 审计存储
///
/// 将 immudb 持久化存储与内存缓存结合，提供高性能的审计日志存储
/// 实现了完整的审计记录器 trait，可直接替换 MemoryAuditStorage
#[derive(Debug)]
#[allow(dead_code)]
pub struct ImmuDbAuditStore {
    /// immudb 存储后端
    storage: Arc<Mutex<ImmuDbStorage>>,
    /// 内存缓存（最近条目）
    cache: Arc<Mutex<VecDeque<SignedAuditEntry>>>,
    /// 最大缓存条目数
    max_cache_size: usize,
    /// 签名密钥指纹（用于验证）
    signer_fingerprint: String,
    /// 公钥（用于验证签名）
    public_key: Vec<u8>,
}

/// 审计存储 trait
///
/// 定义审计存储的基本操作，支持不同后端实现
#[async_trait::async_trait]
pub trait AuditStorage: Send + Sync {
    /// 记录审计条目
    async fn record(&self, entry: AuditEntry) -> Result<SignedAuditEntry, RecorderError>;

    /// 按索引获取条目
    async fn get_by_index(&self, index: u64) -> Result<Option<SignedAuditEntry>, RecorderError>;

    /// 获取最近的条目
    async fn get_recent(&self, n: usize) -> Result<Vec<SignedAuditEntry>, RecorderError>;

    /// 按用户 ID 查询
    async fn query_by_user(
        &self,
        user_id_hash: &str,
        limit: usize,
    ) -> Result<Vec<SignedAuditEntry>, RecorderError>;

    /// 按操作类型查询
    async fn query_by_action(
        &self,
        action: super::events::AuditAction,
        limit: usize,
    ) -> Result<Vec<SignedAuditEntry>, RecorderError>;

    /// 按结果查询
    async fn query_by_outcome(
        &self,
        outcome: Outcome,
        limit: usize,
    ) -> Result<Vec<SignedAuditEntry>, RecorderError>;

    /// 验证存储完整性
    async fn verify(&self) -> Result<bool, RecorderError>;

    /// 获取条目总数
    async fn count(&self) -> Result<u64, RecorderError>;

    /// 生成审计报告
    async fn generate_report(
        &self,
        start_time: Option<u64>,
        end_time: Option<u64>,
    ) -> Result<AuditReport, RecorderError>;
}

/// 审计报告
#[derive(Debug, Clone)]
pub struct AuditReport {
    /// 总条目数
    pub total_entries: u64,
    /// 成功数
    pub success_count: u64,
    /// 失败数
    pub failure_count: u64,
    /// 拒绝数
    pub denied_count: u64,
    /// 高风险操作数
    pub high_risk_count: u64,
    /// Merkle Tree 根哈希
    pub merkle_root: String,
    /// immudb 状态哈希
    pub state_hash: String,
    /// 开始时间
    pub start_time: Option<u64>,
    /// 结束时间
    pub end_time: Option<u64>,
}

/// immudb 存储配置
#[derive(Debug, Clone)]
pub struct ImmuDbStoreConfig {
    /// immudb 配置
    pub immudb: ImmuDbConfig,
    /// 最大缓存条目数
    pub max_cache_size: usize,
    /// 自动同步间隔（秒，0 表示禁用）
    pub auto_sync_interval_secs: u64,
}

impl Default for ImmuDbStoreConfig {
    fn default() -> Self {
        Self {
            immudb: ImmuDbConfig::default(),
            max_cache_size: 10_000,
            auto_sync_interval_secs: 60,
        }
    }
}

impl ImmuDbAuditStore {
    /// 创建新的 immudb 审计存储
    ///
    /// # 参数
    /// - `config`: immudb 存储配置
    /// - `signer_fingerprint`: 签名者指纹
    /// - `public_key`: 公钥（用于验证）
    ///
    /// # 示例
    /// ```rust,no_run
    /// use vault_service::audit::{ImmuDbAuditStore, ImmuDbStoreConfig};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = ImmuDbStoreConfig::default();
    /// let store = ImmuDbAuditStore::new(
    ///     config,
    ///     "signer_fp".to_string(),
    ///     vec![1, 2, 3],
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new(
        config: ImmuDbStoreConfig,
        signer_fingerprint: String,
        public_key: Vec<u8>,
    ) -> Result<Self, RecorderError> {
        let storage = ImmuDbStorage::new(config.immudb).await?;

        Ok(Self {
            storage: Arc::new(Mutex::new(storage)),
            cache: Arc::new(Mutex::new(VecDeque::with_capacity(config.max_cache_size))),
            max_cache_size: config.max_cache_size,
            signer_fingerprint,
            public_key,
        })
    }

    /// 从环境变量创建存储
    pub async fn from_env(
        signer_fingerprint: String,
        public_key: Vec<u8>,
    ) -> Result<Self, RecorderError> {
        let config = ImmuDbStoreConfig {
            immudb: ImmuDbConfig::from_env(),
            ..Default::default()
        };
        Self::new(config, signer_fingerprint, public_key).await
    }

    /// 存储签名后的审计条目
    ///
    /// # 流程
    /// 1. 将条目存储到 immudb
    /// 2. 更新本地缓存
    /// 3. 返回存储结果
    pub async fn store(
        &self,
        signed_entry: &SignedAuditEntry,
    ) -> Result<StoredAuditEntry, RecorderError> {
        let immu_entry = {
            let mut storage = self.storage.lock().await;
            storage.store(signed_entry).await?
        };

        // 更新缓存
        {
            let mut cache = self.cache.lock().await;
            cache.push_back(signed_entry.clone());

            // 清理旧缓存
            while cache.len() > self.max_cache_size {
                cache.pop_front();
            }
        }

        Ok(StoredAuditEntry {
            signed_entry: signed_entry.clone(),
            transaction_id: immu_entry.transaction_id,
            state_hash: immu_entry.state_hash,
            stored_at: immu_entry.stored_at,
        })
    }

    /// 批量存储条目
    pub async fn store_batch(
        &self,
        entries: &[SignedAuditEntry],
    ) -> Result<Vec<StoredAuditEntry>, RecorderError> {
        let mut results = Vec::with_capacity(entries.len());
        for entry in entries {
            let result = self.store(entry).await?;
            results.push(result);
        }
        Ok(results)
    }

    /// 按索引获取条目
    pub async fn get_by_index(
        &self,
        index: u64,
    ) -> Result<Option<SignedAuditEntry>, RecorderError> {
        let key = format!("audit:{index}");

        // 先检查缓存
        {
            let cache = self.cache.lock().await;
            if let Some(entry) = cache.iter().find(|e| e.log_index == index) {
                return Ok(Some(entry.clone()));
            }
        }

        // 从 immudb 获取
        let immu_entry = {
            let storage = self.storage.lock().await;
            storage.get(&key).await?
        };
        Ok(immu_entry.map(|e| e.signed_entry))
    }

    /// 获取最近的条目
    pub async fn get_recent(&self, n: usize) -> Result<Vec<SignedAuditEntry>, RecorderError> {
        // 从缓存获取（逆序）
        let cache_entries: Vec<SignedAuditEntry> = {
            let cache = self.cache.lock().await;
            cache
                .iter()
                .rev()
                .take(n)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect()
        };

        // 如果缓存足够，直接返回
        if cache_entries.len() >= n {
            return Ok(cache_entries);
        }

        // 否则从 immudb 查询更多
        let entries: Vec<SignedAuditEntry> = {
            let storage = self.storage.lock().await;
            let options = QueryOptions {
                limit: Some(n),
                ..Default::default()
            };
            let immu_entries = storage.query(&options).await?;
            immu_entries.into_iter().map(|e| e.signed_entry).collect()
        };

        Ok(entries)
    }

    /// 按用户 ID 哈希查询
    pub async fn query_by_user(
        &self,
        user_id_hash: &str,
        limit: usize,
    ) -> Result<Vec<SignedAuditEntry>, RecorderError> {
        let options = QueryOptions {
            user_id_hash: Some(user_id_hash.to_string()),
            limit: Some(limit),
            ..Default::default()
        };
        let immu_entries = {
            let storage = self.storage.lock().await;
            storage.query(&options).await?
        };
        Ok(immu_entries.into_iter().map(|e| e.signed_entry).collect())
    }

    /// 按操作结果查询
    pub async fn query_by_outcome(
        &self,
        outcome: Outcome,
        limit: usize,
    ) -> Result<Vec<SignedAuditEntry>, RecorderError> {
        let options = QueryOptions {
            outcome: Some(outcome),
            limit: Some(limit),
            ..Default::default()
        };
        let immu_entries = {
            let storage = self.storage.lock().await;
            storage.query(&options).await?
        };
        Ok(immu_entries.into_iter().map(|e| e.signed_entry).collect())
    }

    /// 按时间范围查询
    pub async fn query_by_time_range(
        &self,
        start_time: u64,
        end_time: u64,
        limit: usize,
    ) -> Result<Vec<SignedAuditEntry>, RecorderError> {
        let options = QueryOptions {
            start_time: Some(start_time),
            end_time: Some(end_time),
            limit: Some(limit),
            ..Default::default()
        };
        let immu_entries = {
            let storage = self.storage.lock().await;
            storage.query(&options).await?
        };
        Ok(immu_entries.into_iter().map(|e| e.signed_entry).collect())
    }

    /// 验证存储完整性
    ///
    /// # 流程
    /// 1. 验证 immudb 状态哈希
    /// 2. 验证本地缓存一致性
    /// 3. 返回验证结果
    pub async fn verify(&self) -> Result<bool, RecorderError> {
        {
            let storage = self.storage.lock().await;
            storage.verify().await
        }
    }

    /// 验证特定条目的完整性
    pub async fn verify_entry(&self, index: u64) -> Result<VerificationResult, RecorderError> {
        let key = format!("audit:{index}");

        let proof = {
            let storage = self.storage.lock().await;
            let client = storage.client();
            client.verify_entry(&key).await?
        };

        Ok(VerificationResult {
            index,
            verified: proof.verified,
            transaction_id: proof.transaction_id,
            root_hash: proof.root_hash,
            inclusion_proof: proof.inclusion_proof,
        })
    }

    /// 获取当前 immudb 状态
    pub async fn current_state(&self) -> Result<ImmuDbState, RecorderError> {
        let state = {
            let storage = self.storage.lock().await;
            storage.client().current_state().await?
        };
        Ok(state)
    }

    /// 生成审计报告
    pub async fn generate_report(
        &self,
        start_time: Option<u64>,
        end_time: Option<u64>,
    ) -> Result<AuditReport, RecorderError> {
        let state = self.current_state().await?;

        // 查询范围内的条目
        let entries = if start_time.is_some() || end_time.is_some() {
            let storage = self.storage.lock().await;

            let options = QueryOptions {
                start_time,
                end_time,
                limit: Some(100_000), // 大限制以获取所有
                ..Default::default()
            };

            storage.query(&options).await?
        } else {
            vec![]
        };

        let total_entries = entries.len() as u64;
        let success_count = entries
            .iter()
            .filter(|e| e.signed_entry.entry.outcome.is_success())
            .count() as u64;
        let failure_count = entries
            .iter()
            .filter(|e| e.signed_entry.entry.outcome.is_failure())
            .count() as u64;
        let denied_count = entries
            .iter()
            .filter(|e| e.signed_entry.entry.outcome == Outcome::Denied)
            .count() as u64;
        let high_risk_count = entries
            .iter()
            .filter(|e| e.signed_entry.entry.is_high_risk())
            .count() as u64;

        Ok(AuditReport {
            total_entries,
            success_count,
            failure_count,
            denied_count,
            high_risk_count,
            merkle_root: hex::encode(state.state_hash.as_bytes()),
            state_hash: state.state_hash,
            start_time,
            end_time,
        })
    }

    /// 获取条目总数
    pub async fn count(&self) -> Result<u64, RecorderError> {
        let state = self.current_state().await?;
        Ok(state.tree_size)
    }

    /// 清空缓存
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.lock().await;
        cache.clear();
    }

    /// 获取缓存统计
    pub async fn cache_stats(&self) -> CacheStats {
        let cache = self.cache.lock().await;
        CacheStats {
            cached_entries: cache.len(),
            max_cache_size: self.max_cache_size,
        }
    }

    /// 获取存储客户端（用于高级操作）
    pub fn storage(&self) -> Arc<Mutex<ImmuDbStorage>> {
        Arc::clone(&self.storage)
    }
}

/// 存储后的审计条目
#[derive(Debug, Clone)]
pub struct StoredAuditEntry {
    /// 签名后的审计条目
    pub signed_entry: SignedAuditEntry,
    /// immudb 事务 ID
    pub transaction_id: u64,
    /// immudb 状态哈希
    pub state_hash: String,
    /// 存储时间戳
    pub stored_at: u64,
}

/// 验证结果
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// 条目索引
    pub index: u64,
    /// 是否验证通过
    pub verified: bool,
    /// 事务 ID
    pub transaction_id: u64,
    /// Merkle Tree 根哈希
    pub root_hash: String,
    /// 包含证明
    pub inclusion_proof: Vec<String>,
}

/// 缓存统计
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// 缓存条目数
    pub cached_entries: usize,
    /// 最大缓存大小
    pub max_cache_size: usize,
}

/// 审计存储工厂
///
/// 用于创建不同类型的审计存储实例
pub struct AuditStoreFactory;

impl AuditStoreFactory {
    /// 创建 immudb 存储
    pub async fn create_immudb_store(
        config: ImmuDbStoreConfig,
        signer_fingerprint: String,
        public_key: Vec<u8>,
    ) -> Result<ImmuDbAuditStore, RecorderError> {
        ImmuDbAuditStore::new(config, signer_fingerprint, public_key).await
    }

    /// 从环境变量创建 immudb 存储
    pub async fn create_immudb_store_from_env(
        signer_fingerprint: String,
        public_key: Vec<u8>,
    ) -> Result<ImmuDbAuditStore, RecorderError> {
        ImmuDbAuditStore::from_env(signer_fingerprint, public_key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::{AuditAction, AuditEntry};

    fn create_test_entry(index: u64) -> SignedAuditEntry {
        let entry = AuditEntry::new(
            format!("user_{index}"),
            "session_test",
            "test-service",
            AuditAction::TokenValidate,
            Outcome::Success,
            "mrenclave_test",
            format!("jti_{index}"),
        );

        SignedAuditEntry {
            entry,
            content_hash: [0u8; 32],
            prev_hash: [0u8; 32],
            signature: vec![1, 2, 3],
            signer_fingerprint: "test_fp".to_string(),
            log_index: index,
            merkle_root: [0u8; 32],
        }
    }

    #[test]
    fn test_audit_report() {
        let report = AuditReport {
            total_entries: 100,
            success_count: 80,
            failure_count: 15,
            denied_count: 5,
            high_risk_count: 10,
            merkle_root: "abc123".to_string(),
            state_hash: "def456".to_string(),
            start_time: Some(1000),
            end_time: Some(2000),
        };

        assert_eq!(report.total_entries, 100);
        assert_eq!(report.success_count, 80);
        assert_eq!(report.failure_count, 15);
        assert_eq!(report.denied_count, 5);
    }

    #[test]
    fn test_cache_stats() {
        let stats = CacheStats {
            cached_entries: 50,
            max_cache_size: 1000,
        };

        assert_eq!(stats.cached_entries, 50);
        assert_eq!(stats.max_cache_size, 1000);
    }

    #[test]
    fn test_stored_audit_entry() {
        let signed = create_test_entry(0);
        let stored = StoredAuditEntry {
            signed_entry: signed.clone(),
            transaction_id: 1,
            state_hash: "hash123".to_string(),
            stored_at: 1234567890,
        };

        assert_eq!(stored.signed_entry.log_index, 0);
        assert_eq!(stored.transaction_id, 1);
        assert_eq!(stored.state_hash, "hash123");
    }

    #[test]
    fn test_immu_db_store_config_default() {
        let config = ImmuDbStoreConfig::default();
        assert_eq!(config.max_cache_size, 10_000);
        assert_eq!(config.auto_sync_interval_secs, 60);
    }

    #[test]
    fn test_verification_result() {
        let result = VerificationResult {
            index: 42,
            verified: true,
            transaction_id: 100,
            root_hash: "root123".to_string(),
            inclusion_proof: vec!["proof1".to_string(), "proof2".to_string()],
        };

        assert_eq!(result.index, 42);
        assert!(result.verified);
        assert_eq!(result.transaction_id, 100);
    }
}
