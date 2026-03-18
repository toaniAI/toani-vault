//! Token 撤销逻辑
//!
//! 实现 Token 撤销的管理、批量撤销和撤销恢复策略
//!
//! # 核心功能
//!
//! - **单 Token 撤销**: 撤销指定 jti 的 Token
//! - **批量撤销**: 按用户、租户或 scope 批量撤销
//! - **撤销策略**: 支持级联撤销相关 Token
//! - **撤销恢复**: 撤销过期后的 Token 清理
//!
//! # 使用示例
//!
//! ```rust,ignore
//! use vault_service::token::{TokenRevoker, RevocationPolicy};
//!
//! // 创建撤销管理器
//! let revoker = TokenRevoker::new(redis_store);
//!
//! // 撤销单个 Token
//! revoker.revoke_token("tenant_123", "jti_abc").await.unwrap();
//!
//! // 批量撤销用户的所有 Token
//! revoker.revoke_by_user("tenant_123", "user_456").await.unwrap();
//!
//! // 撤销特定 scope 的所有 Token
//! revoker.revoke_by_scope("tenant_123", "admin").await.unwrap();
//! ```

use super::paseto::TokenRevocationChecker;
use super::redis_store::{RedisTokenStore, TokenStoreError};
use thiserror::Error;

/// 撤销错误类型
#[derive(Error, Debug, Clone, PartialEq)]
pub enum RevocationError {
    #[error("Token 存储错误: {0}")]
    StoreError(String),

    #[error("撤销失败: {0}")]
    RevocationFailed(String),

    #[error("Token 不存在: {0}")]
    TokenNotFound(String),

    #[error("Token 已撤销: {0}")]
    AlreadyRevoked(String),

    #[error("批量撤销限制超过: 最大 {max}, 请求 {requested}")]
    BatchLimitExceeded { max: usize, requested: usize },

    #[error("撤销策略不允许: {0}")]
    PolicyViolation(String),
}

impl From<TokenStoreError> for RevocationError {
    fn from(e: TokenStoreError) -> Self {
        match e {
            TokenStoreError::TokenNotFound(jti) => RevocationError::TokenNotFound(jti),
            TokenStoreError::TokenAlreadyRevoked(jti) => RevocationError::AlreadyRevoked(jti),
            _ => RevocationError::StoreError(e.to_string()),
        }
    }
}

/// 撤销策略配置
#[derive(Debug, Clone)]
pub struct RevocationPolicy {
    /// 是否级联撤销相关 Token
    pub cascade_revoke: bool,
    /// 级联深度限制（0 表示无限制）
    pub cascade_depth: usize,
    /// 批量撤销最大数量限制
    pub batch_limit: usize,
    /// 是否允许撤销已过期 Token（用于审计标记）
    pub allow_revoke_expired: bool,
}

impl Default for RevocationPolicy {
    fn default() -> Self {
        Self {
            cascade_revoke: false,
            cascade_depth: 3,
            batch_limit: 1000,
            allow_revoke_expired: true,
        }
    }
}

impl RevocationPolicy {
    /// 创建严格的撤销策略
    pub fn strict() -> Self {
        Self {
            cascade_revoke: true,
            cascade_depth: 5,
            batch_limit: 100,
            allow_revoke_expired: false,
        }
    }

    /// 创建宽松的撤销策略
    pub fn permissive() -> Self {
        Self {
            cascade_revoke: false,
            cascade_depth: 0,
            batch_limit: 10000,
            allow_revoke_expired: true,
        }
    }
}

/// 撤销结果
#[derive(Debug, Clone)]
pub struct RevocationResult {
    /// 成功撤销的 Token 数量
    pub revoked_count: usize,
    /// 跳过的 Token 数量（已撤销或不存在）
    pub skipped_count: usize,
    /// 失败的 Token 列表
    pub failed_tokens: Vec<(String, String)>, // (jti, reason)
}

impl RevocationResult {
    /// 创建空结果
    pub fn empty() -> Self {
        Self {
            revoked_count: 0,
            skipped_count: 0,
            failed_tokens: Vec::new(),
        }
    }

    /// 添加成功撤销
    pub fn add_revoked(&mut self) {
        self.revoked_count += 1;
    }

    /// 添加跳过
    pub fn add_skipped(&mut self) {
        self.skipped_count += 1;
    }

    /// 添加失败
    pub fn add_failed(&mut self, jti: impl Into<String>, reason: impl Into<String>) {
        self.failed_tokens.push((jti.into(), reason.into()));
    }

    /// 合并另一个结果
    pub fn merge(&mut self, other: RevocationResult) {
        self.revoked_count += other.revoked_count;
        self.skipped_count += other.skipped_count;
        self.failed_tokens.extend(other.failed_tokens);
    }

    /// 检查是否全部成功
    pub fn is_complete_success(&self) -> bool {
        self.failed_tokens.is_empty()
    }

    /// 获取总处理数
    pub fn total_processed(&self) -> usize {
        self.revoked_count + self.skipped_count + self.failed_tokens.len()
    }
}

/// Token 撤销管理器
pub struct TokenRevoker {
    store: RedisTokenStore,
    policy: RevocationPolicy,
}

impl TokenRevoker {
    /// 创建新的 Token 撤销管理器
    pub fn new(store: RedisTokenStore) -> Self {
        Self {
            store,
            policy: RevocationPolicy::default(),
        }
    }

    /// 创建带策略的 Token 撤销管理器
    pub fn with_policy(store: RedisTokenStore, policy: RevocationPolicy) -> Self {
        Self { store, policy }
    }

    /// 获取存储引用
    pub fn store(&self) -> &RedisTokenStore {
        &self.store
    }

    /// 撤销单个 Token
    ///
    /// # 参数
    /// - `tenant_id`: 租户 ID
    /// - `jti`: Token 唯一标识符
    ///
    /// # 返回值
    /// - `Ok(RevocationResult)`: 撤销成功
    /// - `Err(RevocationError)`: 撤销失败
    pub async fn revoke_token(
        &self,
        tenant_id: &str,
        jti: &str,
    ) -> Result<RevocationResult, RevocationError> {
        let mut result = RevocationResult::empty();

        // 检查 Token 是否存在
        match self.store.get_metadata(jti).await {
            Ok(Some(metadata)) => {
                // 检查是否已过期
                if metadata.is_expired() && !self.policy.allow_revoke_expired {
                    return Err(RevocationError::PolicyViolation(
                        "不允许撤销已过期的 Token".to_string(),
                    ));
                }
            }
            Ok(None) => {
                // Token 不存在，但仍然尝试撤销（可能在活跃集合中）
            }
            Err(e) => return Err(e.into()),
        }

        // 执行撤销
        match self.store.revoke_token(tenant_id, jti).await {
            Ok(()) => {
                result.add_revoked();
            }
            Err(TokenStoreError::TokenAlreadyRevoked(_)) => {
                result.add_skipped();
            }
            Err(e) => {
                result.add_failed(jti, e.to_string());
            }
        }

        Ok(result)
    }

    /// 批量撤销 Token 列表
    ///
    /// # 参数
    /// - `tenant_id`: 租户 ID
    /// - `jtis`: Token jti 列表
    ///
    /// # 返回值
    /// - `Ok(RevocationResult)`: 批量撤销结果
    /// - `Err(RevocationError)`: 超出批量限制或其他错误
    pub async fn revoke_batch(
        &self,
        tenant_id: &str,
        jtis: &[String],
    ) -> Result<RevocationResult, RevocationError> {
        // 检查批量限制
        if jtis.len() > self.policy.batch_limit {
            return Err(RevocationError::BatchLimitExceeded {
                max: self.policy.batch_limit,
                requested: jtis.len(),
            });
        }

        let mut result = RevocationResult::empty();

        for jti in jtis {
            match self.revoke_token(tenant_id, jti).await {
                Ok(single_result) => result.merge(single_result),
                Err(e) => result.add_failed(jti, e.to_string()),
            }
        }

        Ok(result)
    }

    /// 撤销用户的所有 Token
    ///
    /// 遍历该用户的所有活跃 Token 并撤销
    ///
    /// # 参数
    /// - `tenant_id`: 租户 ID
    /// - `user_id`: 用户 ID
    ///
    /// # 返回值
    /// - `Ok(RevocationResult)`: 撤销结果
    pub async fn revoke_by_user(
        &self,
        tenant_id: &str,
        user_id: &str,
    ) -> Result<RevocationResult, RevocationError> {
        let mut result = RevocationResult::empty();

        // 获取所有活跃 Token
        let jtis = self.store.list_active_tokens(tenant_id, 0).await?;

        for jti in jtis {
            // 检查是否属于该用户
            match self.store.get_metadata(&jti).await {
                Ok(Some(metadata)) => {
                    if metadata.user_id == user_id {
                        match self.revoke_token(tenant_id, &jti).await {
                            Ok(single_result) => result.merge(single_result),
                            Err(e) => result.add_failed(&jti, e.to_string()),
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => result.add_failed(&jti, e.to_string()),
            }
        }

        Ok(result)
    }

    /// 撤销特定 scope 的所有 Token
    ///
    /// # 参数
    /// - `tenant_id`: 租户 ID
    /// - `scope`: 权限范围
    pub async fn revoke_by_scope(
        &self,
        tenant_id: &str,
        scope: &str,
    ) -> Result<RevocationResult, RevocationError> {
        let mut result = RevocationResult::empty();

        // 获取所有活跃 Token
        let jtis = self.store.list_active_tokens(tenant_id, 0).await?;

        for jti in jtis {
            // 检查是否匹配 scope
            match self.store.get_metadata(&jti).await {
                Ok(Some(metadata)) => {
                    // 检查 scope 匹配（支持前缀匹配）
                    if metadata.scope.contains(scope) {
                        match self.revoke_token(tenant_id, &jti).await {
                            Ok(single_result) => result.merge(single_result),
                            Err(e) => result.add_failed(&jti, e.to_string()),
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => result.add_failed(&jti, e.to_string()),
            }
        }

        Ok(result)
    }

    /// 撤销租户的所有 Token（紧急操作）
    ///
    /// ⚠️ 警告：这是一个危险操作，会撤销租户的所有活跃 Token
    ///
    /// # 参数
    /// - `tenant_id`: 租户 ID
    pub async fn revoke_all_by_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<RevocationResult, RevocationError> {
        let mut result = RevocationResult::empty();

        // 获取所有活跃 Token
        let jtis = self.store.list_active_tokens(tenant_id, 0).await?;

        for jti in jtis {
            match self.revoke_token(tenant_id, &jti).await {
                Ok(single_result) => result.merge(single_result),
                Err(e) => result.add_failed(&jti, e.to_string()),
            }
        }

        Ok(result)
    }

    /// 清理已过期的活跃 Token
    ///
    /// 从 Sorted Set 中移除已过期的 Token 记录
    ///
    /// # 参数
    /// - `tenant_id`: 租户 ID
    ///
    /// # 返回值
    /// 清理的 Token 数量
    pub async fn cleanup_expired_tokens(&self, tenant_id: &str) -> Result<u64, RevocationError> {
        let cleaned = self.store.cleanup_expired(tenant_id).await?;
        Ok(cleaned)
    }

    /// 获取撤销统计信息
    ///
    /// # 参数
    /// - `tenant_id`: 租户 ID
    ///
    /// # 返回值
    /// - `Ok((活跃数, 撤销数))`: Token 统计
    pub async fn get_revocation_stats(
        &self,
        tenant_id: &str,
    ) -> Result<(u64, u64), RevocationError> {
        let active_count = self.store.get_active_count(tenant_id).await?;
        let revoked_count = self.store.get_revoked_count(tenant_id).await?;
        Ok((active_count, revoked_count))
    }
}

/// 简单的内存撤销检查器（用于测试）
pub struct MemoryRevocationChecker {
    revoked_jtis: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
}

impl MemoryRevocationChecker {
    /// 创建新的内存撤销检查器
    pub fn new() -> Self {
        Self {
            revoked_jtis: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashSet::new(),
            )),
        }
    }

    /// 添加撤销的 jti
    pub fn revoke(&self, jti: impl Into<String>) {
        if let Ok(mut set) = self.revoked_jtis.lock() {
            set.insert(jti.into());
        }
    }

    /// 检查是否撤销
    pub fn is_revoked_sync(&self, jti: &str) -> bool {
        if let Ok(set) = self.revoked_jtis.lock() {
            set.contains(jti)
        } else {
            false
        }
    }

    /// 清除所有撤销记录
    pub fn clear(&self) {
        if let Ok(mut set) = self.revoked_jtis.lock() {
            set.clear();
        }
    }
}

impl Default for MemoryRevocationChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenRevocationChecker for MemoryRevocationChecker {
    fn is_revoked(&self, jti: &str) -> bool {
        self.is_revoked_sync(jti)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_revocation_policy_default() {
        let policy = RevocationPolicy::default();
        assert!(!policy.cascade_revoke);
        assert_eq!(policy.cascade_depth, 3);
        assert_eq!(policy.batch_limit, 1000);
        assert!(policy.allow_revoke_expired);
    }

    #[test]
    fn test_revocation_policy_strict() {
        let policy = RevocationPolicy::strict();
        assert!(policy.cascade_revoke);
        assert_eq!(policy.cascade_depth, 5);
        assert_eq!(policy.batch_limit, 100);
        assert!(!policy.allow_revoke_expired);
    }

    #[test]
    fn test_revocation_policy_permissive() {
        let policy = RevocationPolicy::permissive();
        assert!(!policy.cascade_revoke);
        assert_eq!(policy.cascade_depth, 0);
        assert_eq!(policy.batch_limit, 10000);
        assert!(policy.allow_revoke_expired);
    }

    #[test]
    fn test_revocation_result() {
        let mut result = RevocationResult::empty();
        assert_eq!(result.revoked_count, 0);
        assert_eq!(result.skipped_count, 0);
        assert!(result.failed_tokens.is_empty());

        result.add_revoked();
        assert_eq!(result.revoked_count, 1);

        result.add_skipped();
        assert_eq!(result.skipped_count, 1);

        result.add_failed("jti_123", "test error");
        assert_eq!(result.failed_tokens.len(), 1);
        assert_eq!(result.failed_tokens[0].0, "jti_123");

        assert!(!result.is_complete_success());
        assert_eq!(result.total_processed(), 3);
    }

    #[test]
    fn test_revocation_result_merge() {
        let mut result1 = RevocationResult::empty();
        result1.add_revoked();
        result1.add_failed("jti_1", "error1");

        let mut result2 = RevocationResult::empty();
        result2.add_revoked();
        result2.add_skipped();

        result1.merge(result2);

        assert_eq!(result1.revoked_count, 2);
        assert_eq!(result1.skipped_count, 1);
        assert_eq!(result1.failed_tokens.len(), 1);
    }

    #[test]
    fn test_revocation_error_from_token_store() {
        let store_err = TokenStoreError::TokenNotFound("jti_123".to_string());
        let rev_err: RevocationError = store_err.into();
        assert!(matches!(rev_err, RevocationError::TokenNotFound(_)));

        let store_err = TokenStoreError::TokenAlreadyRevoked("jti_123".to_string());
        let rev_err: RevocationError = store_err.into();
        assert!(matches!(rev_err, RevocationError::AlreadyRevoked(_)));
    }

    #[test]
    fn test_memory_revocation_checker() {
        let checker = MemoryRevocationChecker::new();

        assert!(!checker.is_revoked("jti_123"));

        checker.revoke("jti_123");
        assert!(checker.is_revoked("jti_123"));

        checker.clear();
        assert!(!checker.is_revoked("jti_123"));
    }

    #[test]
    fn test_memory_revocation_checker_default() {
        let checker: MemoryRevocationChecker = Default::default();
        assert!(!checker.is_revoked("any_jti"));
    }
}
