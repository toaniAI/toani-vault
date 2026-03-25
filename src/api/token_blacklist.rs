//! Token 黑名单实现
//!
//! 提供集中式 Token 黑名单存储，支持：
//! - 内存存储（用于测试和单实例部署）
//! - Redis 存储（用于生产多实例部署，支持 TTL 自动过期）
//!
//! # 安全特性
//!
//! - TTL 自动过期：Token 在过期后自动从黑名单移除
//! - 分布式一致性：Redis 后端确保多实例间状态一致
//! - 防重放攻击：重启后黑名单状态不丢失

use async_trait::async_trait;
use redis::{AsyncCommands, Client as RedisClient};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

/// Token 黑名单错误类型
#[derive(Debug, thiserror::Error)]
pub enum BlacklistError {
    #[error("Redis 连接错误: {0}")]
    RedisConnection(String),

    #[error("Redis 操作错误: {0}")]
    RedisOperation(String),

    #[error("Token 已被撤销")]
    TokenRevoked,

    #[error("内部错误: {0}")]
    Internal(String),
}

impl From<redis::RedisError> for BlacklistError {
    fn from(e: redis::RedisError) -> Self {
        BlacklistError::RedisOperation(e.to_string())
    }
}

/// Token 黑名单 trait
///
/// 抽象 Token 黑名单操作，支持多种后端实现
#[async_trait]
pub trait TokenBlacklist: Send + Sync {
    /// 将 Token ID (jti) 添加到黑名单
    ///
    /// # Arguments
    /// * `jti` - Token ID
    /// * `ttl_seconds` - 黑名单 TTL（秒），超过此时间后自动移除
    async fn blacklist_token(&self, jti: &str, ttl_seconds: u64) -> Result<(), BlacklistError>;

    /// 检查 Token ID 是否在黑名单中
    async fn is_blacklisted(&self, jti: &str) -> Result<bool, BlacklistError>;

    /// 从黑名单中移除 Token ID（用于手动清理）
    async fn remove_from_blacklist(&self, jti: &str) -> Result<(), BlacklistError>;

    /// 清理所有过期的 Token（仅内存后端需要）
    async fn cleanup_expired(&self) -> Result<usize, BlacklistError>;
}

/// 内存 Token 黑名单
///
/// 适用于单实例部署和测试环境。
/// 注意：服务重启后状态会丢失。
pub struct InMemoryTokenBlacklist {
    /// 存储结构: jti -> 过期时间戳
    entries: Arc<RwLock<HashSet<String>>>,
    /// 带 TTL 的条目: jti -> 过期时间戳
    expirations: Arc<RwLock<HashMap<String, u64>>>,
}

use std::collections::HashMap;

impl InMemoryTokenBlacklist {
    /// 创建新的内存黑名单
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashSet::new())),
            expirations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 获取当前时间戳
    fn now(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

impl Default for InMemoryTokenBlacklist {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TokenBlacklist for InMemoryTokenBlacklist {
    async fn blacklist_token(&self, jti: &str, ttl_seconds: u64) -> Result<(), BlacklistError> {
        let now = self.now();
        let expiration = now.saturating_add(ttl_seconds);

        {
            let mut entries = self.entries.write().await;
            entries.insert(jti.to_string());
        }

        {
            let mut expirations = self.expirations.write().await;
            expirations.insert(jti.to_string(), expiration);
        }

        Ok(())
    }

    async fn is_blacklisted(&self, jti: &str) -> Result<bool, BlacklistError> {
        // 先检查是否已过期
        {
            let expirations = self.expirations.read().await;
            if let Some(&expiration) = expirations.get(jti) {
                if self.now() > expiration {
                    // 已过期，视为不在黑名单
                    return Ok(false);
                }
            } else {
                // 没有过期记录，说明不在黑名单
                return Ok(false);
            }
        }

        let entries = self.entries.read().await;
        Ok(entries.contains(jti))
    }

    async fn remove_from_blacklist(&self, jti: &str) -> Result<(), BlacklistError> {
        {
            let mut entries = self.entries.write().await;
            entries.remove(jti);
        }
        {
            let mut expirations = self.expirations.write().await;
            expirations.remove(jti);
        }
        Ok(())
    }

    async fn cleanup_expired(&self) -> Result<usize, BlacklistError> {
        let now = self.now();
        let mut removed = 0;

        let expired_jtis: Vec<String> = {
            let expirations = self.expirations.read().await;
            expirations
                .iter()
                .filter(|(_, exp)| **exp < now)
                .map(|(jti, _)| jti.clone())
                .collect()
        };

        {
            let mut entries = self.entries.write().await;
            let mut expirations = self.expirations.write().await;

            for jti in expired_jtis {
                entries.remove(&jti);
                expirations.remove(&jti);
                removed += 1;
            }
        }

        Ok(removed)
    }
}

/// Redis Token 黑名单
///
/// 适用于生产多实例部署。
/// 使用 Redis SETEX 命令实现自动过期。
pub struct RedisTokenBlacklist {
    /// Redis 客户端
    client: RedisClient,
    /// 键前缀
    key_prefix: String,
}

impl RedisTokenBlacklist {
    /// 创建新的 Redis 黑名单
    ///
    /// # Arguments
    /// * `redis_url` - Redis 连接 URL，如 "redis://127.0.0.1:6379"
    /// * `key_prefix` - 键前缀，默认为 "token:blacklist:"
    pub fn new(redis_url: &str, key_prefix: Option<String>) -> Result<Self, BlacklistError> {
        let client = RedisClient::open(redis_url)
            .map_err(|e| BlacklistError::RedisConnection(e.to_string()))?;

        Ok(Self {
            client,
            key_prefix: key_prefix.unwrap_or_else(|| "token:blacklist:".to_string()),
        })
    }

    /// 构建完整的 Redis 键
    fn build_key(&self, jti: &str) -> String {
        format!("{}{}", self.key_prefix, jti)
    }

    /// 检查 Redis 连接是否可用
    pub async fn health_check(&self) -> Result<(), BlacklistError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| BlacklistError::RedisConnection(e.to_string()))?;

        redis::cmd("PING")
            .query_async::<_, String>(&mut conn)
            .await
            .map_err(|e| BlacklistError::RedisOperation(e.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl TokenBlacklist for RedisTokenBlacklist {
    async fn blacklist_token(&self, jti: &str, ttl_seconds: u64) -> Result<(), BlacklistError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| BlacklistError::RedisConnection(e.to_string()))?;

        let key = self.build_key(jti);

        // 使用 SETEX 设置键和过期时间
        let _: () = conn.set_ex(key, "1", ttl_seconds).await?;

        Ok(())
    }

    async fn is_blacklisted(&self, jti: &str) -> Result<bool, BlacklistError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| BlacklistError::RedisConnection(e.to_string()))?;

        let key = self.build_key(jti);

        // EXISTS 命令检查键是否存在
        let exists: bool = conn.exists(key).await?;

        Ok(exists)
    }

    async fn remove_from_blacklist(&self, jti: &str) -> Result<(), BlacklistError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| BlacklistError::RedisConnection(e.to_string()))?;

        let key = self.build_key(jti);

        let _: () = conn.del(key).await?;

        Ok(())
    }

    async fn cleanup_expired(&self) -> Result<usize, BlacklistError> {
        // Redis 自动处理过期键，无需手动清理
        Ok(0)
    }
}

/// Token 黑名单工厂
///
/// 根据配置创建合适的黑名单实现
pub struct TokenBlacklistFactory;

impl TokenBlacklistFactory {
    /// 创建内存黑名单
    pub fn create_memory() -> Arc<dyn TokenBlacklist> {
        Arc::new(InMemoryTokenBlacklist::new())
    }

    /// 创建 Redis 黑名单
    pub fn create_redis(
        redis_url: &str,
        key_prefix: Option<String>,
    ) -> Result<Arc<dyn TokenBlacklist>, BlacklistError> {
        let blacklist = RedisTokenBlacklist::new(redis_url, key_prefix)?;
        Ok(Arc::new(blacklist))
    }
}

/// 向后兼容的 TokenStore 类型别名
pub type TokenStore = Arc<dyn TokenBlacklist>;

/// 创建 Token 存储（默认内存实现，用于向后兼容）
pub fn create_token_store() -> TokenStore {
    TokenBlacklistFactory::create_memory()
}

/// 创建 Redis Token 存储
pub fn create_redis_token_store(redis_url: &str) -> Result<TokenStore, BlacklistError> {
    TokenBlacklistFactory::create_redis(redis_url, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_blacklist_basic() {
        let blacklist = InMemoryTokenBlacklist::new();
        let jti = "test-token-123";

        // 初始不在黑名单
        assert!(!blacklist.is_blacklisted(jti).await.unwrap());

        // 添加到黑名单
        blacklist.blacklist_token(jti, 3600).await.unwrap();

        // 现在在黑名单
        assert!(blacklist.is_blacklisted(jti).await.unwrap());

        // 移除
        blacklist.remove_from_blacklist(jti).await.unwrap();

        // 不在黑名单
        assert!(!blacklist.is_blacklisted(jti).await.unwrap());
    }

    #[tokio::test]
    async fn test_memory_blacklist_expiration() {
        let blacklist = InMemoryTokenBlacklist::new();
        let jti = "test-token-expire";

        // 添加到黑名单，TTL = 1 秒
        blacklist.blacklist_token(jti, 1).await.unwrap();
        assert!(blacklist.is_blacklisted(jti).await.unwrap());

        // 等待过期
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // 已过期，不在黑名单
        assert!(!blacklist.is_blacklisted(jti).await.unwrap());
    }

    #[tokio::test]
    async fn test_memory_blacklist_cleanup() {
        let blacklist = InMemoryTokenBlacklist::new();

        // 添加多个 Token，TTL = 1 秒
        for i in 0..5 {
            blacklist
                .blacklist_token(&format!("token-{i}"), 1)
                .await
                .unwrap();
        }

        // 等待过期
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // 清理
        let cleaned = blacklist.cleanup_expired().await.unwrap();
        assert_eq!(cleaned, 5);
    }

    // Redis 测试需要本地 Redis 服务，使用 #[ignore] 标记
    #[tokio::test]
    #[ignore = "需要本地 Redis 服务"]
    async fn test_redis_blacklist_basic() {
        let blacklist = RedisTokenBlacklist::new("redis://127.0.0.1:6379", None).unwrap();
        let jti = "test-redis-token-123";

        // 初始不在黑名单
        assert!(!blacklist.is_blacklisted(jti).await.unwrap());

        // 添加到黑名单
        blacklist.blacklist_token(jti, 3600).await.unwrap();

        // 现在在黑名单
        assert!(blacklist.is_blacklisted(jti).await.unwrap());

        // 移除
        blacklist.remove_from_blacklist(jti).await.unwrap();

        // 不在黑名单
        assert!(!blacklist.is_blacklisted(jti).await.unwrap());
    }

    #[tokio::test]
    #[ignore = "需要本地 Redis 服务"]
    async fn test_redis_blacklist_expiration() {
        let blacklist = RedisTokenBlacklist::new("redis://127.0.0.1:6379", None).unwrap();
        let jti = "test-redis-token-expire";

        // 添加到黑名单，TTL = 1 秒
        blacklist.blacklist_token(jti, 1).await.unwrap();
        assert!(blacklist.is_blacklisted(jti).await.unwrap());

        // 等待过期
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // 已过期，不在黑名单
        assert!(!blacklist.is_blacklisted(jti).await.unwrap());
    }
}
