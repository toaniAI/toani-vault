//! Redis Token 状态管理
//!
//! 实现 Token 的 Redis 存储、撤销检查和元数据管理
//! 使用以下 Redis 数据结构：
//! - Sorted Set: credbridge:tokens:{tenant_id}:active - 活跃 Token（按过期时间排序）
//! - Set: credbridge:tokens:{tenant_id}:revoked - 已撤销 Token
//! - Hash: credbridge:token:{jti} - Token 元数据
//!
//! # 使用示例
//!
//! ```rust,ignore
//! use vault_service::token::{RedisTokenStore, TokenMetadata};
//!
//! // 创建 Redis 连接
//! let client = redis::Client::open("redis://127.0.0.1/").unwrap();
//! let store = RedisTokenStore::new(client);
//!
//! // 存储 Token
//! store.store_token(
//!     "tenant_123",
//!     "jti_uuid",
//!     "user_456",
//!     "credential:read",
//!     1700000000, // exp timestamp
//! ).await.unwrap();
//!
//! // 检查是否撤销
//! let is_revoked = store.is_revoked("tenant_123", "jti_uuid").await.unwrap();
//!
//! // 撤销 Token
//! store.revoke_token("tenant_123", "jti_uuid").await.unwrap();
//! ```

use super::claims::TokenClaims;
use redis::{AsyncCommands, Client, RedisError, aio::MultiplexedConnection};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Redis Token 存储错误
#[derive(Error, Debug, Clone, PartialEq)]
pub enum TokenStoreError {
    #[error("Redis 错误: {0}")]
    RedisError(String),

    #[error("Token 不存在: {0}")]
    TokenNotFound(String),

    #[error("Token 已撤销: {0}")]
    TokenAlreadyRevoked(String),

    #[error("无效的时间戳")]
    InvalidTimestamp,

    #[error("连接错误: {0}")]
    ConnectionError(String),
}

impl From<RedisError> for TokenStoreError {
    fn from(e: RedisError) -> Self {
        TokenStoreError::RedisError(e.to_string())
    }
}

/// Token 元数据
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenMetadata {
    /// 用户 ID
    pub user_id: String,
    /// 租户 ID
    pub tenant_id: String,
    /// 权限范围
    pub scope: String,
    /// 签发时间（Unix 时间戳）
    pub issued_at: u64,
    /// 过期时间（Unix 时间戳）
    pub expires_at: u64,
    /// 是否已撤销
    #[serde(default)]
    pub revoked: bool,
    /// 撤销时间（Unix 时间戳）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<u64>,
}

impl TokenMetadata {
    /// 从 TokenClaims 创建元数据
    pub fn from_claims(claims: &TokenClaims) -> Self {
        Self {
            user_id: claims.sub.clone(),
            tenant_id: claims.aud.clone(),
            scope: claims.scope.clone(),
            issued_at: claims.iat.unwrap_or(claims.exp - 900),
            expires_at: claims.exp,
            revoked: false,
            revoked_at: None,
        }
    }

    /// 检查 Token 是否过期
    pub fn is_expired(&self) -> bool {
        current_timestamp() >= self.expires_at
    }

    /// 标记为已撤销
    pub fn mark_revoked(&mut self) {
        self.revoked = true;
        self.revoked_at = Some(current_timestamp());
    }
}

/// Redis Key 前缀常量
pub mod keys {
    /// 活跃 Token Sorted Set 前缀
    pub const ACTIVE_TOKENS_PREFIX: &str = "credbridge:tokens";

    /// 撤销 Token Set 前缀
    pub const REVOKED_TOKENS_PREFIX: &str = "credbridge:tokens";

    /// Token 元数据 Hash 前缀
    pub const TOKEN_METADATA_PREFIX: &str = "credbridge:token";

    /// 生成活跃 Token Set 的 key
    pub fn active_tokens_key(tenant_id: &str) -> String {
        format!("{}:{}:active", ACTIVE_TOKENS_PREFIX, tenant_id)
    }

    /// 生成撤销 Token Set 的 key
    pub fn revoked_tokens_key(tenant_id: &str) -> String {
        format!("{}:{}:revoked", REVOKED_TOKENS_PREFIX, tenant_id)
    }

    /// 生成 Token 元数据 Hash 的 key
    pub fn token_metadata_key(jti: &str) -> String {
        format!("{}:{}", TOKEN_METADATA_PREFIX, jti)
    }
}

/// Redis Token 存储管理器
#[derive(Clone)]
pub struct RedisTokenStore {
    client: Client,
}

impl RedisTokenStore {
    /// 创建新的 RedisTokenStore
    ///
    /// # 参数
    /// - `client`: Redis 客户端
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// 从连接字符串创建 RedisTokenStore
    ///
    /// # 参数
    /// - `connection_string`: Redis 连接字符串，如 "redis://127.0.0.1:6379"
    ///
    /// # 返回值
    /// - `Ok(RedisTokenStore)`: 创建成功
    /// - `Err(TokenStoreError)`: 创建失败
    pub fn from_url(connection_string: &str) -> Result<Self, TokenStoreError> {
        let client = Client::open(connection_string)
            .map_err(|e| TokenStoreError::ConnectionError(e.to_string()))?;
        Ok(Self::new(client))
    }

    /// 获取 Redis 连接
    async fn get_connection(&self) -> Result<MultiplexedConnection, TokenStoreError> {
        self.client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| TokenStoreError::ConnectionError(e.to_string()))
    }

    /// 存储新 Token 到 Redis
    ///
    /// 根据 Story 验收标准：
    /// - 存储 jti 到 Redis Sorted Set：credbridge:tokens:{tenant_id}:active
    /// - 设置 score 为 exp_timestamp
    /// - 设置 TTL = exp - current_time
    /// - 存储元数据到 Hash：credbridge:token:{jti}
    ///
    /// # 参数
    /// - `tenant_id`: 租户 ID
    /// - `jti`: Token 唯一标识符
    /// - `user_id`: 用户 ID
    /// - `scope`: 权限范围
    /// - `exp`: 过期时间（Unix 时间戳）
    ///
    /// # 返回值
    /// - `Ok(())`: 存储成功
    /// - `Err(TokenStoreError)`: 存储失败
    pub async fn store_token(
        &self,
        tenant_id: &str,
        jti: &str,
        user_id: &str,
        scope: &str,
        exp: u64,
    ) -> Result<(), TokenStoreError> {
        let mut conn = self.get_connection().await?;
        let now = current_timestamp();

        // 检查 exp 是否有效
        if exp <= now {
            return Err(TokenStoreError::InvalidTimestamp);
        }

        let ttl = exp - now;

        // 1. 存储到活跃 Token Sorted Set，score 为 exp
        let active_key = keys::active_tokens_key(tenant_id);
        let _: () = conn.zadd(&active_key, jti, exp as f64).await?;

        // 2. 设置活跃集合的 TTL（使用最长的 Token 有效期）
        // 注意：我们不单独设置每个 member 的 TTL，而是依赖 Sorted Set 的自动清理
        let _: () = conn.expire(&active_key, ttl as i64).await?;

        // 3. 存储 Token 元数据到 Hash
        let metadata = TokenMetadata {
            user_id: user_id.to_string(),
            tenant_id: tenant_id.to_string(),
            scope: scope.to_string(),
            issued_at: now,
            expires_at: exp,
            revoked: false,
            revoked_at: None,
        };

        let metadata_key = keys::token_metadata_key(jti);
        let metadata_json = serde_json::to_string(&metadata)
            .map_err(|e| TokenStoreError::RedisError(e.to_string()))?;

        let _: () = conn.hset(&metadata_key, "data", metadata_json).await?;
        // 元数据保留 7 天用于审计
        let _: () = conn.expire(&metadata_key, (ttl + 7 * 86400) as i64).await?;

        Ok(())
    }

    /// 存储 TokenClaims 到 Redis（便捷方法）
    ///
    /// # 参数
    /// - `claims`: Token Claims
    pub async fn store_claims(&self, claims: &TokenClaims) -> Result<(), TokenStoreError> {
        self.store_token(
            &claims.aud,
            &claims.jti,
            &claims.sub,
            &claims.scope,
            claims.exp,
        )
        .await
    }

    /// 检查 Token 是否已被撤销
    ///
    /// 根据 Story 验收标准：
    /// - 查询 Redis：SISMEMBER credbridge:tokens:{tenant_id}:revoked {jti}
    /// - 如果 jti 在撤销集合中，返回 true
    ///
    /// # 参数
    /// - `tenant_id`: 租户 ID
    /// - `jti`: Token 唯一标识符
    ///
    /// # 返回值
    /// - `Ok(true)`: Token 已被撤销
    /// - `Ok(false)`: Token 未被撤销
    /// - `Err(TokenStoreError)`: 查询失败
    pub async fn is_revoked(&self, tenant_id: &str, jti: &str) -> Result<bool, TokenStoreError> {
        let mut conn = self.get_connection().await?;
        let revoked_key = keys::revoked_tokens_key(tenant_id);

        let is_member: bool = conn.sismember(&revoked_key, jti).await?;
        Ok(is_member)
    }

    /// 撤销 Token
    ///
    /// 根据 Story 验收标准：
    /// - 从活跃集合移除 jti
    /// - 添加到撤销集合：SADD credbridge:tokens:{tenant_id}:revoked {jti}
    /// - 更新 Token 元数据：revoked=true, revoked_at=timestamp
    ///
    /// # 参数
    /// - `tenant_id`: 租户 ID
    /// - `jti`: Token 唯一标识符
    ///
    /// # 返回值
    /// - `Ok(())`: 撤销成功
    /// - `Err(TokenStoreError)`: 撤销失败
    pub async fn revoke_token(&self, tenant_id: &str, jti: &str) -> Result<(), TokenStoreError> {
        let mut conn = self.get_connection().await?;

        // 1. 从活跃集合移除
        let active_key = keys::active_tokens_key(tenant_id);
        let _: () = conn.zrem(&active_key, jti).await?;

        // 2. 添加到撤销集合
        let revoked_key = keys::revoked_tokens_key(tenant_id);
        let _: () = conn.sadd(&revoked_key, jti).await?;

        // 设置撤销集合的 TTL（30 天）
        let _: () = conn.expire(&revoked_key, 30 * 86400 as i64).await?;

        // 3. 更新元数据
        let metadata_key = keys::token_metadata_key(jti);
        let now = current_timestamp();

        // 先获取现有元数据
        let existing: Option<String> = conn.hget(&metadata_key, "data").await?;

        let mut metadata = match existing {
            Some(json) => serde_json::from_str(&json)
                .map_err(|e| TokenStoreError::RedisError(e.to_string()))?,
            None => {
                // 如果元数据不存在，创建一个最小版本
                TokenMetadata {
                    user_id: "unknown".to_string(),
                    tenant_id: tenant_id.to_string(),
                    scope: "unknown".to_string(),
                    issued_at: 0,
                    expires_at: now + 86400,
                    revoked: false,
                    revoked_at: None,
                }
            }
        };

        // 检查是否已经撤销
        if metadata.revoked {
            return Err(TokenStoreError::TokenAlreadyRevoked(jti.to_string()));
        }

        // 更新撤销状态
        metadata.mark_revoked();

        let metadata_json = serde_json::to_string(&metadata)
            .map_err(|e| TokenStoreError::RedisError(e.to_string()))?;

        let _: () = conn.hset(&metadata_key, "data", metadata_json).await?;
        let _: () = conn.hset(&metadata_key, "revoked", "true").await?;
        let _: () = conn
            .hset(&metadata_key, "revoked_at", now.to_string())
            .await?;

        Ok(())
    }

    /// 查询 Token 元数据
    ///
    /// 根据 Story 验收标准：
    /// - 从 Hash 读取：credbridge:token:{jti}
    /// - 返回 user_id, tenant_id, scope, issued_at, expires_at, revoked
    ///
    /// # 参数
    /// - `jti`: Token 唯一标识符
    ///
    /// # 返回值
    /// - `Ok(Some(TokenMetadata))`: 找到元数据
    /// - `Ok(None)`: 元数据不存在
    /// - `Err(TokenStoreError)`: 查询失败
    pub async fn get_metadata(&self, jti: &str) -> Result<Option<TokenMetadata>, TokenStoreError> {
        let mut conn = self.get_connection().await?;
        let metadata_key = keys::token_metadata_key(jti);

        let data: Option<String> = conn.hget(&metadata_key, "data").await?;

        match data {
            Some(json) => {
                let metadata: TokenMetadata = serde_json::from_str(&json)
                    .map_err(|e| TokenStoreError::RedisError(e.to_string()))?;
                Ok(Some(metadata))
            }
            None => Ok(None),
        }
    }

    /// 获取租户的活跃 Token 数量
    ///
    /// # 参数
    /// - `tenant_id`: 租户 ID
    pub async fn get_active_count(&self, tenant_id: &str) -> Result<u64, TokenStoreError> {
        let mut conn = self.get_connection().await?;
        let active_key = keys::active_tokens_key(tenant_id);

        let count: u64 = conn.zcard(&active_key).await?;
        Ok(count)
    }

    /// 获取租户的已撤销 Token 数量
    ///
    /// # 参数
    /// - `tenant_id`: 租户 ID
    pub async fn get_revoked_count(&self, tenant_id: &str) -> Result<u64, TokenStoreError> {
        let mut conn = self.get_connection().await?;
        let revoked_key = keys::revoked_tokens_key(tenant_id);

        let count: u64 = conn.scard(&revoked_key).await?;
        Ok(count)
    }

    /// 清理已过期的活跃 Token（从 Sorted Set 中删除）
    ///
    /// 定期调用此方法来清理已过期的 Token
    ///
    /// # 参数
    /// - `tenant_id`: 租户 ID
    pub async fn cleanup_expired(&self, tenant_id: &str) -> Result<u64, TokenStoreError> {
        let mut conn = self.get_connection().await?;
        let active_key = keys::active_tokens_key(tenant_id);
        let now = current_timestamp();

        // 删除 score <= now 的所有 member（已过期）
        let removed: u64 = conn.zrembyscore(&active_key, 0, now as f64).await?;
        Ok(removed)
    }

    /// 获取租户的所有活跃 Token jti 列表
    ///
    /// # 参数
    /// - `tenant_id`: 租户 ID
    /// - `limit`: 返回数量限制（0 表示无限制）
    pub async fn list_active_tokens(
        &self,
        tenant_id: &str,
        limit: usize,
    ) -> Result<Vec<String>, TokenStoreError> {
        let mut conn = self.get_connection().await?;
        let active_key = keys::active_tokens_key(tenant_id);

        let jtis: Vec<String> = if limit > 0 {
            conn.zrevrange(&active_key, 0, (limit - 1) as isize).await?
        } else {
            conn.zrevrange(&active_key, 0, -1).await?
        };

        Ok(jtis)
    }

    /// 获取租户的所有已撤销 Token jti 列表
    ///
    /// # 参数
    /// - `tenant_id`: 租户 ID
    pub async fn list_revoked_tokens(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<String>, TokenStoreError> {
        let mut conn = self.get_connection().await?;
        let revoked_key = keys::revoked_tokens_key(tenant_id);

        let jtis: Vec<String> = conn.smembers(&revoked_key).await?;
        Ok(jtis)
    }

    /// 删除 Token 的所有记录（用于测试清理）
    ///
    /// ⚠️ 警告：这是一个危险操作，会永久删除 Token 记录
    ///
    /// # 参数
    /// - `tenant_id`: 租户 ID
    /// - `jti`: Token 唯一标识符
    pub async fn delete_token(&self, tenant_id: &str, jti: &str) -> Result<(), TokenStoreError> {
        let mut conn = self.get_connection().await?;

        let active_key = keys::active_tokens_key(tenant_id);
        let revoked_key = keys::revoked_tokens_key(tenant_id);
        let metadata_key = keys::token_metadata_key(jti);

        let _: () = conn.zrem(&active_key, jti).await?;
        let _: () = conn.srem(&revoked_key, jti).await?;
        let _: () = conn.del(&metadata_key).await?;

        Ok(())
    }

    /// 删除租户的所有 Token 数据（用于测试清理）
    ///
    /// ⚠️ 警告：这是一个危险操作，会永久删除该租户的所有 Token 记录
    pub async fn delete_tenant_tokens(&self, tenant_id: &str) -> Result<(), TokenStoreError> {
        let mut conn = self.get_connection().await?;

        let active_key = keys::active_tokens_key(tenant_id);
        let revoked_key = keys::revoked_tokens_key(tenant_id);

        // 获取所有 jti 并删除元数据
        let jtis: Vec<String> = conn.zrevrange(&active_key, 0, -1).await?;
        for jti in &jtis {
            let metadata_key = keys::token_metadata_key(jti);
            let _: () = conn.del(&metadata_key).await?;
        }

        let _: () = conn.del(&active_key).await?;
        let _: () = conn.del(&revoked_key).await?;

        Ok(())
    }
}

/// 获取当前 Unix 时间戳（秒）
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
    fn test_keys_generation() {
        assert_eq!(
            keys::active_tokens_key("tenant_123"),
            "credbridge:tokens:tenant_123:active"
        );
        assert_eq!(
            keys::revoked_tokens_key("tenant_123"),
            "credbridge:tokens:tenant_123:revoked"
        );
        assert_eq!(
            keys::token_metadata_key("jti_abc"),
            "credbridge:token:jti_abc"
        );
    }

    #[test]
    fn test_token_metadata_from_claims() {
        let claims =
            TokenClaims::with_default_ttl("user_123", "tenant_456", "credential:read", true);

        let metadata = TokenMetadata::from_claims(&claims);

        assert_eq!(metadata.user_id, "user_123");
        assert_eq!(metadata.tenant_id, "tenant_456");
        assert_eq!(metadata.scope, "credential:read");
        assert!(!metadata.revoked);
        assert!(metadata.revoked_at.is_none());
    }

    #[test]
    fn test_token_metadata_is_expired() {
        let mut metadata = TokenMetadata {
            user_id: "user_123".to_string(),
            tenant_id: "tenant_456".to_string(),
            scope: "credential:read".to_string(),
            issued_at: 0,
            expires_at: current_timestamp() + 3600,
            revoked: false,
            revoked_at: None,
        };

        assert!(!metadata.is_expired());

        metadata.expires_at = 1; // 过去的时间
        assert!(metadata.is_expired());
    }

    #[test]
    fn test_token_metadata_mark_revoked() {
        let mut metadata = TokenMetadata {
            user_id: "user_123".to_string(),
            tenant_id: "tenant_456".to_string(),
            scope: "credential:read".to_string(),
            issued_at: 0,
            expires_at: current_timestamp() + 3600,
            revoked: false,
            revoked_at: None,
        };

        let before = current_timestamp();
        metadata.mark_revoked();
        let after = current_timestamp();

        assert!(metadata.revoked);
        assert!(metadata.revoked_at.is_some());
        let revoked_at = metadata.revoked_at.unwrap();
        assert!(revoked_at >= before && revoked_at <= after);
    }

    #[test]
    fn test_token_store_error_display() {
        let err = TokenStoreError::TokenNotFound("jti_123".to_string());
        assert_eq!(err.to_string(), "Token 不存在: jti_123");

        let err = TokenStoreError::InvalidTimestamp;
        assert_eq!(err.to_string(), "无效的时间戳");
    }
}
