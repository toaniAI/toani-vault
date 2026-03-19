//! CredBridge 审计模块（EP4-Story4.1 + EP4-Story4.2）
//!
//! 实现不可篡改的审计日志系统，支持以下功能：
//!
//! - **审计事件记录**: 记录凭证访问、Token 操作、系统配置变更等事件
//! - **Merkle Tree**: 计算审计链的哈希树，确保完整性
//! - **数字签名**: 对审计条目进行签名，防止篡改
//! - **PII 脱敏**: 自动敏感数据脱敏，保护用户隐私
//! - **immudb 存储**: 使用 immudb 实现不可篡改的持久化存储
//!
//! # 模块结构
//!
//! ```text
//! audit/
//! ├── mod.rs              - 模块导出
//! ├── events.rs           - 审计事件定义（AuditEntry, AuditAction, RiskTier, 等）
//! ├── recorder.rs         - 审计记录器（签名、Merkle Tree、存储）
//! ├── immudb_client.rs    - immudb 客户端封装
//! └── immudb_store.rs     - immudb 存储实现
//! ```
//!
//! # 核心类型
//!
//! ## 审计事件
//!
//! ```rust,ignore
//! use vault_service::audit::{AuditEntry, AuditAction, Outcome, RiskTier};
//!
//! // 创建审计条目
//! let entry = AuditEntry::new(
//!     "user_hash_abc123",         // 用户 ID 哈希
//!     "session_xyz789",           // 会话 ID
//!     "vault-service",            // 服务标识
//!     AuditAction::CredentialDecrypt,  // 操作类型
//!     Outcome::Success,           // 操作结果
//!     "mrenclave_measurement",    // TEE MRENCLAVE
//!     "jti_token_id",             // Action Token JTI
//! );
//! ```
//!
//! ## 审计记录器
//!
//! ```rust,ignore
//! use vault_service::audit::{AuditRecorder, MemoryAuditStorage};
//!
//! // 创建内存审计存储
//! let storage = MemoryAuditStorage::new(10000)?;
//!
//! // 记录审计事件
//! let signed_entry = storage.record(entry)?;
//!
//! // 验证链完整性
//! let is_valid = storage.verify()?;
//! ```
//!
//! ## PII 脱敏
//!
//! ```rust,ignore
//! use vault_service::audit::{AuditEntry, PiiRedactor, RedactedParam};
//!
//! // 创建带脱敏参数的审计条目
//! let entry = AuditEntry::new(...)
//!     .with_param("ssn", PiiRedactor::redact_ssn("123-45-6789"))
//!     .with_param("email", PiiRedactor::redact_email("user@example.com"))
//!     .with_param("credential_id", PiiRedactor::plain("cred_123")); // 不脱敏
//! ```
//!
//! ## immudb 存储
//!
//! ```rust,ignore
//! use vault_service::audit::{ImmuDbAuditStore, ImmuDbStoreConfig, ImmuDbConfig};
//!
//! // 创建 immudb 存储配置
//! let config = ImmuDbStoreConfig {
//!     immudb: ImmuDbConfig::from_env(),
//!     max_cache_size: 10_000,
//!     auto_sync_interval_secs: 60,
//! };
//!
//! // 创建存储实例
//! let store = ImmuDbAuditStore::new(
//!     config,
//!     signer_fingerprint,
//!     public_key,
//! ).await?;
//!
//! // 存储审计条目
//! let stored = store.store(&signed_entry).await?;
//!
//! // 验证条目完整性
//! let result = store.verify_entry(0).await?;
//! assert!(result.verified);
//! ```
//!
//! # 架构约束
//!
//! 符合以下架构约束：
//! - **FR3**: 审计日志系统
//! - **SA-004**: 不可篡改日志 + PII 脱敏
//! - **FR3-immudb**: immudb 不可篡改存储
//!
//! # 审计字段
//!
//! 每个审计条目包含：
//! - `id`: UUID v7（时间排序）
//! - `user_id_hash`: SHA-256 哈希的用户 ID
//! - `timestamp`: UTC 时间戳（毫秒）
//! - `session_id`: 会话标识
//! - `service`: 服务名称
//! - `action`: 操作类型
//! - `risk_tier`: 风险等级（Low/Medium/High/Critical）
//! - `outcome`: 操作结果（Success/Failure/Denied/Timeout/Aborted）
//! - `tee_mrenclave`: TEE MRENCLAVE 测量值
//! - `action_token_jti`: Action Token JTI
//! - `params`: 脱敏后的操作参数（可选）
//! - `error_message`: 错误信息（可选）
//!
//! # 安全特性
//!
//! 1. **不可篡改**: 使用 Merkle Tree 和数字签名确保审计日志完整性
//! 2. **隐私保护**: PII 数据自动脱敏，保留类型信息用于审计
//! 3. **可追溯**: UUID v7 支持时间排序，便于事件时序分析
//! 4. **完整性验证**: 支持验证整个审计链的完整性

pub mod events;
pub mod immudb_client;
pub mod immudb_store;
pub mod recorder;

// 公开导出 - 审计事件
pub use events::{
    AuditAction, AuditEntry, AuditEventId, Outcome, PiiRedactor, RedactedParam, RiskTier,
    hash_user_id,
};

// 公开导出 - 审计记录器
pub use recorder::{
    AuditLogChain, AuditRecorder, AuditReport as RecorderAuditReport, MemoryAuditStorage,
    RecorderError, SignedAuditEntry, SigningKeyPair,
};

// 公开导出 - immudb 客户端
pub use immudb_client::{
    ImmuDbAuditEntry, ImmuDbClient, ImmuDbConfig, ImmuDbState, ImmuDbStorage, QueryOptions,
    StorageStats, VerificationProof,
};

// 公开导出 - immudb 存储
pub use immudb_store::{
    AuditReport, AuditStorage, AuditStoreFactory, CacheStats, ImmuDbAuditStore, ImmuDbStoreConfig,
    StoredAuditEntry, VerificationResult,
};

// 公开导出 - 审计宏
pub use crate::audit_record;

/// 审计模块版本
pub const AUDIT_VERSION: &str = "0.2.0";

/// immudb 集成版本
pub const IMMUDB_INTEGRATION_VERSION: &str = "0.1.0";

/// 默认最大内存条目数
pub const DEFAULT_MAX_ENTRIES: usize = 100_000;

/// 高风险操作列表
pub const HIGH_RISK_ACTIONS: &[AuditAction] = &[
    AuditAction::CredentialDecrypt,
    AuditAction::CredentialDelete,
    AuditAction::AuditQuery,
    AuditAction::SystemConfigChange,
    AuditAction::KeyRotation,
    AuditAction::AdminLogin,
    AuditAction::FailedAuth,
];

/// 检查操作是否为高风险
pub fn is_high_risk_action(action: &AuditAction) -> bool {
    HIGH_RISK_ACTIONS.contains(action)
}

/// 创建内存审计存储（便捷函数）
pub fn create_memory_storage() -> Result<MemoryAuditStorage, RecorderError> {
    MemoryAuditStorage::new(DEFAULT_MAX_ENTRIES)
}

/// 审计统计信息
#[derive(Debug, Clone, Default)]
pub struct AuditStats {
    /// 总记录数
    pub total_records: u64,
    /// 成功数
    pub success_count: u64,
    /// 失败数
    pub failure_count: u64,
    /// 高风险操作数
    pub high_risk_count: u64,
    /// 最后记录时间
    pub last_record_time: Option<u64>,
}

/// 审计查询过滤器
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
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
    /// 服务
    pub service: Option<String>,
}

impl AuditFilter {
    /// 创建新的过滤器
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置开始时间
    pub fn with_start_time(mut self, time: u64) -> Self {
        self.start_time = Some(time);
        self
    }

    /// 设置结束时间
    pub fn with_end_time(mut self, time: u64) -> Self {
        self.end_time = Some(time);
        self
    }

    /// 设置用户 ID 哈希
    pub fn with_user_id_hash(mut self, hash: impl Into<String>) -> Self {
        self.user_id_hash = Some(hash.into());
        self
    }

    /// 设置操作类型
    pub fn with_action(mut self, action: AuditAction) -> Self {
        self.action = Some(action);
        self
    }

    /// 设置风险等级
    pub fn with_risk_tier(mut self, tier: RiskTier) -> Self {
        self.risk_tier = Some(tier);
        self
    }

    /// 设置操作结果
    pub fn with_outcome(mut self, outcome: Outcome) -> Self {
        self.outcome = Some(outcome);
        self
    }

    /// 设置服务
    pub fn with_service(mut self, service: impl Into<String>) -> Self {
        self.service = Some(service.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_version() {
        assert_eq!(AUDIT_VERSION, "0.2.0");
    }

    #[test]
    fn test_is_high_risk_action() {
        assert!(is_high_risk_action(&AuditAction::CredentialDecrypt));
        assert!(is_high_risk_action(&AuditAction::KeyRotation));
        assert!(!is_high_risk_action(&AuditAction::TokenValidate));
        assert!(!is_high_risk_action(&AuditAction::CredentialAccess));
    }

    #[test]
    fn test_create_memory_storage() {
        let storage = create_memory_storage().unwrap();
        assert!(storage.verify().unwrap());
    }

    #[test]
    fn test_audit_filter() {
        let filter = AuditFilter::new()
            .with_start_time(1000)
            .with_end_time(2000)
            .with_user_id_hash("hash123")
            .with_action(AuditAction::CredentialDecrypt)
            .with_risk_tier(RiskTier::High)
            .with_outcome(Outcome::Success)
            .with_service("vault-service");

        assert_eq!(filter.start_time, Some(1000));
        assert_eq!(filter.end_time, Some(2000));
        assert_eq!(filter.user_id_hash, Some("hash123".to_string()));
        assert_eq!(filter.action, Some(AuditAction::CredentialDecrypt));
        assert_eq!(filter.risk_tier, Some(RiskTier::High));
        assert_eq!(filter.outcome, Some(Outcome::Success));
        assert_eq!(filter.service, Some("vault-service".to_string()));
    }

    #[test]
    fn test_audit_stats_default() {
        let stats = AuditStats::default();
        assert_eq!(stats.total_records, 0);
        assert_eq!(stats.success_count, 0);
        assert_eq!(stats.failure_count, 0);
        assert_eq!(stats.high_risk_count, 0);
        assert!(stats.last_record_time.is_none());
    }

    #[test]
    fn test_audit_entry_with_chain() {
        let storage = create_memory_storage().unwrap();

        // 记录多个事件
        for i in 0..3 {
            let entry = AuditEntry::new(
                format!("user_{}", i),
                "session_123",
                "vault-service",
                AuditAction::CredentialDecrypt,
                Outcome::Success,
                "mrenclave_abc",
                format!("jti_{}", i),
            );
            storage.record(entry).unwrap();
        }

        // 验证链完整性
        assert!(storage.verify().unwrap());

        // 获取最近的条目
        let recent = storage.query_recent(2).unwrap();
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn test_pii_redaction_in_entry() {
        let entry = AuditEntry::new(
            "user_hash",
            "session",
            "service",
            AuditAction::CredentialDecrypt,
            Outcome::Success,
            "mrenclave",
            "jti",
        )
        .with_param("ssn", PiiRedactor::redact_ssn("123-45-6789"))
        .with_param("password", PiiRedactor::redact_password("secret123"))
        .with_param("api_key", PiiRedactor::redact_api_key("key_abc"))
        .with_param("normal", PiiRedactor::plain("visible_value"));

        let params = entry.params.as_ref().unwrap();
        assert_eq!(params.len(), 4);

        // 验证脱敏值
        let ssn_param = params.iter().find(|(k, _)| k == "ssn").unwrap();
        assert_eq!(ssn_param.1.to_string(), "[SSN_REDACTED]");

        let password_param = params.iter().find(|(k, _)| k == "password").unwrap();
        assert_eq!(password_param.1.to_string(), "[PASSWORD_REDACTED]");

        let normal_param = params.iter().find(|(k, _)| k == "normal").unwrap();
        assert_eq!(normal_param.1.to_string(), "visible_value");
    }

    #[test]
    fn test_audit_filter_by_outcome() {
        let storage = create_memory_storage().unwrap();

        // 记录成功和失败的事件
        let success_entry = AuditEntry::new(
            "user_1",
            "session",
            "service",
            AuditAction::TokenValidate,
            Outcome::Success,
            "mrenclave",
            "jti_1",
        );
        storage.record(success_entry).unwrap();

        let failure_entry = AuditEntry::new(
            "user_2",
            "session",
            "service",
            AuditAction::CredentialDecrypt,
            Outcome::Failure,
            "mrenclave",
            "jti_2",
        );
        storage.record(failure_entry).unwrap();

        // 查询失败的事件
        let failures = storage.query_by_outcome(Outcome::Failure).unwrap();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].entry.outcome, Outcome::Failure);
        assert_eq!(failures[0].entry.action, AuditAction::CredentialDecrypt);
    }

    #[test]
    fn test_risk_tier_auto_assignment() {
        let storage = create_memory_storage().unwrap();

        // 低风险操作
        let low_risk_entry = AuditEntry::new(
            "user",
            "session",
            "service",
            AuditAction::TokenValidate,
            Outcome::Success,
            "mrenclave",
            "jti",
        );
        let signed_low = storage.record(low_risk_entry).unwrap();
        assert_eq!(signed_low.entry.risk_tier, RiskTier::Low);

        // 高风险操作
        let high_risk_entry = AuditEntry::new(
            "user",
            "session",
            "service",
            AuditAction::CredentialDecrypt,
            Outcome::Success,
            "mrenclave",
            "jti",
        );
        let signed_high = storage.record(high_risk_entry).unwrap();
        assert_eq!(signed_high.entry.risk_tier, RiskTier::High);
        assert!(signed_high.entry.is_high_risk());
    }
}
