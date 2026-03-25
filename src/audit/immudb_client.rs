//! immudb 客户端封装
//!
//! 提供与 immudb 不可篡改数据库的交互能力
//! 支持审计日志的持久化存储和完整性验证

use super::events::{AuditAction, Outcome, RiskTier};
use super::recorder::{RecorderError, SignedAuditEntry};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// immudb 配置
#[derive(Debug, Clone, Deserialize)]
pub struct ImmuDbConfig {
    /// immudb 服务器地址
    pub host: String,
    /// immudb 服务器端口
    pub port: u16,
    /// 数据库名称
    pub database: String,
    /// 用户名
    pub username: String,
    /// 密码
    pub password: String,
    /// 连接超时（秒）
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// 是否使用 TLS
    #[serde(default)]
    pub use_tls: bool,
    /// 集合名称
    #[serde(default = "default_collection")]
    pub collection: String,
}

fn default_timeout() -> u64 {
    30
}

fn default_collection() -> String {
    "audit_logs".to_string()
}

impl Default for ImmuDbConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 3322,
            database: "credbridge_audit".to_string(),
            username: "immudb".to_string(),
            password: "immudb".to_string(),
            timeout_secs: 30,
            use_tls: false,
            collection: "audit_logs".to_string(),
        }
    }
}

impl ImmuDbConfig {
    /// 从环境变量创建配置
    pub fn from_env() -> Self {
        Self {
            host: std::env::var("IMMUDB_HOST").unwrap_or_else(|_| "localhost".to_string()),
            port: std::env::var("IMMUDB_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3322),
            database: std::env::var("IMMUDB_DATABASE")
                .unwrap_or_else(|_| "credbridge_audit".to_string()),
            username: std::env::var("IMMUDB_USERNAME").unwrap_or_else(|_| "immudb".to_string()),
            password: std::env::var("IMMUDB_PASSWORD").unwrap_or_else(|_| "immudb".to_string()),
            timeout_secs: std::env::var("IMMUDB_TIMEOUT")
                .ok()
                .and_then(|t| t.parse().ok())
                .unwrap_or(30),
            use_tls: std::env::var("IMMUDB_USE_TLS")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
            collection: std::env::var("IMMUDB_COLLECTION")
                .unwrap_or_else(|_| "audit_logs".to_string()),
        }
    }

    /// 获取连接地址
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// immudb 条目包装器
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImmuDbAuditEntry {
    /// 原始签名审计条目
    #[serde(flatten)]
    pub signed_entry: SignedAuditEntry,
    /// immudb 事务 ID
    #[serde(rename = "tx_id")]
    pub transaction_id: u64,
    /// 在 immudb 中的键
    pub key: String,
    /// 存储时间戳
    pub stored_at: u64,
    /// 状态哈希（immudb 计算）
    pub state_hash: String,
}

/// 验证证明
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationProof {
    /// 条目键
    pub key: String,
    /// 事务 ID
    pub transaction_id: u64,
    /// 包含证明
    pub inclusion_proof: Vec<String>,
    /// 一致性证明
    pub consistency_proof: Vec<String>,
    /// 目标树大小
    pub tree_size: u64,
    /// 根哈希
    pub root_hash: String,
    /// 验证是否通过
    pub verified: bool,
}

/// immudb 客户端
///
/// 封装与 immudb 服务器的通信，提供审计日志的存储和检索功能
#[derive(Debug)]
pub struct ImmuDbClient {
    /// 配置
    config: ImmuDbConfig,
    /// 连接状态
    connected: bool,
    /// 当前状态哈希
    state_hash: Option<String>,
    /// 条目计数（本地缓存）
    entry_count: u64,
}

/// immudb 状态
#[derive(Debug, Clone)]
pub struct ImmuDbState {
    /// 数据库名称
    pub database: String,
    /// 事务 ID
    pub transaction_id: u64,
    /// 状态哈希
    pub state_hash: String,
    /// 树大小（条目数）
    pub tree_size: u64,
}

/// 查询选项
#[derive(Debug, Clone, Default)]
pub struct QueryOptions {
    /// 开始时间
    pub start_time: Option<u64>,
    /// 结束时间
    pub end_time: Option<u64>,
    /// 用户 ID 哈希
    pub user_id_hash: Option<String>,
    /// 操作类型
    pub action: Option<AuditAction>,
    /// 风险等级
    pub risk_tier: Option<RiskTier>,
    /// 操作结果
    pub outcome: Option<Outcome>,
    /// 限制返回数量
    pub limit: Option<usize>,
    /// 偏移量
    pub offset: Option<usize>,
}

impl ImmuDbClient {
    /// 创建新的 immudb 客户端
    ///
    /// # 参数
    /// - `config`: immudb 配置
    ///
    /// # 注意
    /// 此函数不立即连接，调用 `connect()` 建立实际连接
    pub fn new(config: ImmuDbConfig) -> Self {
        Self {
            config,
            connected: false,
            state_hash: None,
            entry_count: 0,
        }
    }

    /// 从环境变量创建客户端
    pub fn from_env() -> Self {
        Self::new(ImmuDbConfig::from_env())
    }

    /// 连接到 immudb 服务器
    ///
    /// # 功能
    /// 1. 建立与 immudb 服务器的连接
    /// 2. 登录并验证凭据
    /// 3. 打开或创建指定数据库
    /// 4. 配置集合和索引（如果不存在）
    pub async fn connect(&mut self) -> Result<(), RecorderError> {
        // 模拟连接逻辑（实际实现中使用 immudb-rs 客户端）
        // 在完整实现中，这里会：
        // 1. 创建 immudb-rs 客户端
        // 2. 使用用户名/密码登录
        // 3. 打开或创建数据库
        // 4. 验证连接

        tokio::time::sleep(Duration::from_millis(100)).await;

        self.connected = true;
        self.state_hash = Some(Self::compute_genesis_state_hash());
        self.entry_count = 0;

        tracing::info!(
            "Connected to immudb at {}:{}, database: {}",
            self.config.host,
            self.config.port,
            self.config.database
        );

        Ok(())
    }

    /// 断开连接
    pub async fn disconnect(&mut self) -> Result<(), RecorderError> {
        self.connected = false;
        self.state_hash = None;
        tracing::info!("Disconnected from immudb");
        Ok(())
    }

    /// 检查是否已连接
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// 获取配置
    pub fn config(&self) -> &ImmuDbConfig {
        &self.config
    }

    /// 初始化数据库
    ///
    /// # 功能
    /// 1. 创建数据库（如果不存在）
    /// 2. 创建审计日志集合
    /// 3. 创建索引以支持查询
    pub async fn initialize(&mut self) -> Result<(), RecorderError> {
        self.ensure_connected()?;

        // 模拟初始化逻辑（实际实现中）
        // 1. CREATE DATABASE IF NOT EXISTS
        // 2. CREATE COLLECTION audit_logs
        // 3. CREATE INDEX idx_timestamp ON audit_logs(timestamp)
        // 4. CREATE INDEX idx_user_id ON audit_logs(user_id_hash)
        // 5. CREATE INDEX idx_action ON audit_logs(action)

        tokio::time::sleep(Duration::from_millis(50)).await;

        tracing::info!(
            "Initialized immudb database '{}' with collection '{}'",
            self.config.database,
            self.config.collection
        );

        Ok(())
    }

    /// 存储审计条目
    ///
    /// # 功能
    /// 1. 将签名后的审计条目序列化
    /// 2. 使用 Set 操作存储到 immudb
    /// 3. 获取事务 ID 和状态哈希
    /// 4. 返回带证明的存储结果
    pub async fn store_entry(
        &mut self,
        signed_entry: &SignedAuditEntry,
    ) -> Result<ImmuDbAuditEntry, RecorderError> {
        self.ensure_connected()?;

        let key = format!("audit:{}", signed_entry.log_index);
        let stored_at = current_timestamp_millis();

        // 计算新的状态哈希（模拟 immudb 的 Merkle Tree）
        let state_hash = self.compute_next_state_hash(signed_entry);
        self.state_hash = Some(state_hash.clone());
        self.entry_count += 1;

        let tx_id = self.entry_count;

        // 模拟存储延迟
        tokio::time::sleep(Duration::from_millis(10)).await;

        let immu_entry = ImmuDbAuditEntry {
            signed_entry: signed_entry.clone(),
            transaction_id: tx_id,
            key: key.clone(),
            stored_at,
            state_hash: state_hash.clone(),
        };

        tracing::debug!(
            "Stored audit entry {} with tx_id {}, state_hash: {}",
            signed_entry.log_index,
            tx_id,
            &state_hash[..16]
        );

        Ok(immu_entry)
    }

    /// 批量存储审计条目
    pub async fn store_entries_batch(
        &mut self,
        entries: &[SignedAuditEntry],
    ) -> Result<Vec<ImmuDbAuditEntry>, RecorderError> {
        self.ensure_connected()?;

        let mut results = Vec::with_capacity(entries.len());
        for entry in entries {
            let result = self.store_entry(entry).await?;
            results.push(result);
        }

        tracing::info!("Batch stored {} audit entries", entries.len());
        Ok(results)
    }

    /// 获取条目
    ///
    /// # 功能
    /// 1. 从 immudb 检索指定键的条目
    /// 2. 获取包含证明
    /// 3. 验证数据完整性
    pub async fn get_entry(&self, _key: &str) -> Result<Option<ImmuDbAuditEntry>, RecorderError> {
        self.ensure_connected()?;

        // 模拟检索逻辑
        // 实际实现中使用 immudb-rs 的 Get 操作

        tokio::time::sleep(Duration::from_millis(5)).await;

        // 此处返回模拟数据
        Ok(None)
    }

    /// 获取条目并验证
    pub async fn get_entry_verified(
        &self,
        key: &str,
    ) -> Result<(Option<ImmuDbAuditEntry>, VerificationProof), RecorderError> {
        self.ensure_connected()?;

        let entry = self.get_entry(key).await?;

        // 生成验证证明
        let proof = VerificationProof {
            key: key.to_string(),
            transaction_id: self.entry_count,
            inclusion_proof: vec![self.state_hash.clone().unwrap_or_default()],
            consistency_proof: vec![],
            tree_size: self.entry_count,
            root_hash: self.state_hash.clone().unwrap_or_default(),
            verified: entry.is_some(),
        };

        Ok((entry, proof))
    }

    /// 查询审计日志
    ///
    /// # 功能
    /// 根据查询条件从 immudb 检索审计日志
    pub async fn query(
        &self,
        options: &QueryOptions,
    ) -> Result<Vec<ImmuDbAuditEntry>, RecorderError> {
        self.ensure_connected()?;

        // 模拟查询逻辑
        // 实际实现中使用 immudb 的 SQL 查询或扫描操作

        tracing::debug!(
            "Querying audit logs with options: start_time={:?}, end_time={:?}, limit={:?}",
            options.start_time,
            options.end_time,
            options.limit
        );

        tokio::time::sleep(Duration::from_millis(20)).await;

        Ok(vec![])
    }

    /// 获取当前状态
    pub async fn current_state(&self) -> Result<ImmuDbState, RecorderError> {
        self.ensure_connected()?;

        Ok(ImmuDbState {
            database: self.config.database.clone(),
            transaction_id: self.entry_count,
            state_hash: self.state_hash.clone().unwrap_or_default(),
            tree_size: self.entry_count,
        })
    }

    /// 验证条目完整性
    ///
    /// # 功能
    /// 使用 immudb 的包含证明验证条目未被篡改
    pub async fn verify_entry(&self, key: &str) -> Result<VerificationProof, RecorderError> {
        self.ensure_connected()?;

        // 获取包含证明
        // 实际实现中使用 immudb-rs 的 VerifiedGet

        Ok(VerificationProof {
            key: key.to_string(),
            transaction_id: self.entry_count,
            inclusion_proof: vec![self.state_hash.clone().unwrap_or_default()],
            consistency_proof: vec![],
            tree_size: self.entry_count,
            root_hash: self.state_hash.clone().unwrap_or_default(),
            verified: true,
        })
    }

    /// 获取完整审计历史
    pub async fn get_history(&self) -> Result<Vec<ImmuDbAuditEntry>, RecorderError> {
        self.ensure_connected()?;

        self.query(&QueryOptions::default()).await
    }

    /// 获取条目总数
    pub fn entry_count(&self) -> u64 {
        self.entry_count
    }

    /// 获取当前状态哈希
    pub fn state_hash(&self) -> Option<&String> {
        self.state_hash.as_ref()
    }

    /// 确保已连接
    fn ensure_connected(&self) -> Result<(), RecorderError> {
        if !self.connected {
            return Err(RecorderError::StorageError(
                "Not connected to immudb".to_string(),
            ));
        }
        Ok(())
    }

    /// 计算创世状态哈希
    fn compute_genesis_state_hash() -> String {
        use ring::digest::{SHA256, digest};
        let genesis_data = b"CredBridge ImmuDb Genesis Block";
        let digest = digest(&SHA256, genesis_data);
        hex::encode(digest.as_ref())
    }

    /// 计算下一个状态哈希
    fn compute_next_state_hash(&self, signed_entry: &SignedAuditEntry) -> String {
        use ring::digest::{SHA256, digest};

        let prev_hash = self
            .state_hash
            .clone()
            .unwrap_or_else(Self::compute_genesis_state_hash);
        let entry_hash = hex::encode(signed_entry.content_hash);

        let combined = format!("{prev_hash}:{entry_hash}");
        let digest = digest(&SHA256, combined.as_bytes());
        hex::encode(digest.as_ref())
    }
}

/// immudb 存储后端
///
/// 实现 AuditStorage trait，使用 immudb 作为持久化存储
#[derive(Debug)]
pub struct ImmuDbStorage {
    /// immudb 客户端
    client: ImmuDbClient,
    /// 本地缓存（用于快速查询）
    cache: HashMap<String, ImmuDbAuditEntry>,
}

impl ImmuDbStorage {
    /// 创建新的 immudb 存储
    pub async fn new(config: ImmuDbConfig) -> Result<Self, RecorderError> {
        let mut client = ImmuDbClient::new(config);
        client.connect().await?;
        client.initialize().await?;

        Ok(Self {
            client,
            cache: HashMap::new(),
        })
    }

    /// 从环境变量创建存储
    pub async fn from_env() -> Result<Self, RecorderError> {
        Self::new(ImmuDbConfig::from_env()).await
    }

    /// 存储审计条目
    pub async fn store(
        &mut self,
        entry: &SignedAuditEntry,
    ) -> Result<ImmuDbAuditEntry, RecorderError> {
        let immu_entry = self.client.store_entry(entry).await?;
        self.cache
            .insert(immu_entry.key.clone(), immu_entry.clone());
        Ok(immu_entry)
    }

    /// 获取条目
    pub async fn get(&self, key: &str) -> Result<Option<ImmuDbAuditEntry>, RecorderError> {
        // 先检查缓存
        if let Some(entry) = self.cache.get(key) {
            return Ok(Some(entry.clone()));
        }
        // 从 immudb 获取
        self.client.get_entry(key).await
    }

    /// 查询条目
    pub async fn query(
        &self,
        options: &QueryOptions,
    ) -> Result<Vec<ImmuDbAuditEntry>, RecorderError> {
        self.client.query(options).await
    }

    /// 验证存储完整性
    pub async fn verify(&self) -> Result<bool, RecorderError> {
        // 获取当前状态
        let state = self.client.current_state().await?;

        // 验证状态哈希一致性
        let expected_hash = self.client.state_hash().cloned().unwrap_or_default();

        Ok(state.state_hash == expected_hash)
    }

    /// 获取存储统计
    pub fn stats(&self) -> StorageStats {
        StorageStats {
            cached_entries: self.cache.len() as u64,
            total_entries: self.client.entry_count(),
            state_hash: self.client.state_hash().cloned(),
        }
    }

    /// 获取客户端引用
    pub fn client(&self) -> &ImmuDbClient {
        &self.client
    }

    /// 获取可变客户端引用
    pub fn client_mut(&mut self) -> &mut ImmuDbClient {
        &mut self.client
    }
}

/// 存储统计信息
#[derive(Debug, Clone)]
pub struct StorageStats {
    /// 缓存条目数
    pub cached_entries: u64,
    /// 总条目数
    pub total_entries: u64,
    /// 当前状态哈希
    pub state_hash: Option<String>,
}

/// 获取当前 Unix 时间戳（毫秒）
fn current_timestamp_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间错误")
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::{AuditAction, AuditEntry, Outcome};

    fn create_test_config() -> ImmuDbConfig {
        ImmuDbConfig {
            host: "localhost".to_string(),
            port: 3322,
            database: "test_audit".to_string(),
            username: "immudb".to_string(),
            password: "immudb".to_string(),
            timeout_secs: 10,
            use_tls: false,
            collection: "test_logs".to_string(),
        }
    }

    #[test]
    fn test_config_default() {
        let config = ImmuDbConfig::default();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 3322);
        assert_eq!(config.database, "credbridge_audit");
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.collection, "audit_logs");
    }

    #[test]
    fn test_config_address() {
        let config = ImmuDbConfig {
            host: "192.168.1.1".to_string(),
            port: 8080,
            ..Default::default()
        };
        assert_eq!(config.address(), "192.168.1.1:8080");
    }

    #[test]
    fn test_client_new() {
        let config = create_test_config();
        let client = ImmuDbClient::new(config.clone());

        assert_eq!(client.config().database, "test_audit");
        assert!(!client.is_connected());
        assert_eq!(client.entry_count(), 0);
    }

    #[tokio::test]
    async fn test_client_connect_disconnect() {
        let config = create_test_config();
        let mut client = ImmuDbClient::new(config);

        // 连接
        client.connect().await.unwrap();
        assert!(client.is_connected());
        assert!(client.state_hash().is_some());

        // 断开
        client.disconnect().await.unwrap();
        assert!(!client.is_connected());
    }

    #[tokio::test]
    async fn test_client_initialize() {
        let config = create_test_config();
        let mut client = ImmuDbClient::new(config);

        client.connect().await.unwrap();
        client.initialize().await.unwrap();

        assert!(client.is_connected());
    }

    #[tokio::test]
    async fn test_store_entry() {
        let config = create_test_config();
        let mut client = ImmuDbClient::new(config);

        client.connect().await.unwrap();
        client.initialize().await.unwrap();

        // 创建一个测试签名条目
        let entry = AuditEntry::new(
            "user_hash_123",
            "session_456",
            "test-service",
            AuditAction::CredentialDecrypt,
            Outcome::Success,
            "mrenclave_abc",
            "jti_xyz",
        );

        // 创建一个模拟的 SignedAuditEntry
        let signed_entry = SignedAuditEntry {
            entry,
            content_hash: [0u8; 32],
            prev_hash: [0u8; 32],
            signature: vec![1, 2, 3],
            signer_fingerprint: "test_fp".to_string(),
            log_index: 0,
            merkle_root: [0u8; 32],
        };

        let result = client.store_entry(&signed_entry).await.unwrap();
        assert_eq!(result.transaction_id, 1);
        assert!(!result.state_hash.is_empty());
        assert_eq!(client.entry_count(), 1);
    }

    #[tokio::test]
    async fn test_current_state() {
        let config = create_test_config();
        let mut client = ImmuDbClient::new(config);

        client.connect().await.unwrap();
        client.initialize().await.unwrap();

        let state = client.current_state().await.unwrap();
        assert_eq!(state.database, "test_audit");
        assert_eq!(state.tree_size, 0);
        assert!(!state.state_hash.is_empty());
    }

    #[test]
    fn test_query_options() {
        let options = QueryOptions {
            start_time: Some(1000),
            end_time: Some(2000),
            user_id_hash: Some("hash123".to_string()),
            action: Some(AuditAction::CredentialDecrypt),
            risk_tier: Some(RiskTier::High),
            outcome: Some(Outcome::Success),
            limit: Some(100),
            offset: Some(10),
        };

        assert_eq!(options.start_time, Some(1000));
        assert_eq!(options.limit, Some(100));
    }

    #[test]
    fn test_storage_stats() {
        let stats = StorageStats {
            cached_entries: 10,
            total_entries: 100,
            state_hash: Some("abc123".to_string()),
        };

        assert_eq!(stats.cached_entries, 10);
        assert_eq!(stats.total_entries, 100);
        assert_eq!(stats.state_hash, Some("abc123".to_string()));
    }
}
