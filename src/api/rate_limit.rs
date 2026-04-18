//! API 速率限制中间件
//!
//! 基于客户端 IP 地址进行速率限制，默认使用 Redis 持久化窗口状态。
//! 配置项：
//! - `CREDBRIDGE_RATE_LIMIT_REQUESTS`: 每个时间窗口允许的请求数 (默认: 100)
//! - `CREDBRIDGE_RATE_LIMIT_WINDOW_SECONDS`: 时间窗口（秒）(默认: 60)

use async_trait::async_trait;
use axum::{
    Json,
    extract::{ConnectInfo, Request},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use redis::{Client as RedisClient, Script};
use serde::Serialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// 速率限制配置
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// 每个时间窗口允许的请求数
    pub requests_per_window: u32,
    /// 时间窗口（秒）
    pub window_seconds: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_window: 100,
            window_seconds: 60,
        }
    }
}

impl RateLimitConfig {
    /// 从环境变量加载配置
    pub fn from_env() -> Self {
        use std::env;

        let requests_per_window = env::var("CREDBRIDGE_RATE_LIMIT_REQUESTS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);

        let window_seconds = env::var("CREDBRIDGE_RATE_LIMIT_WINDOW_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);

        Self {
            requests_per_window,
            window_seconds,
        }
    }
}

/// 速率限制错误响应
#[derive(Debug, Serialize)]
pub struct RateLimitError {
    pub error: String,
    pub message: String,
    pub retry_after_seconds: u64,
}

impl RateLimitError {
    pub fn new(retry_after: u64) -> Self {
        Self {
            error: "rate_limit_exceeded".to_string(),
            message: format!("请求频率超过限制，请在 {retry_after} 秒后重试"),
            retry_after_seconds: retry_after,
        }
    }
}

impl IntoResponse for RateLimitError {
    fn into_response(self) -> Response {
        tracing::warn!(
            retry_after = self.retry_after_seconds,
            "rate limit exceeded"
        );

        (
            StatusCode::TOO_MANY_REQUESTS,
            [(
                axum::http::header::RETRY_AFTER,
                self.retry_after_seconds.to_string(),
            )],
            Json(self),
        )
            .into_response()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RateLimitStoreError {
    #[error("Redis 连接错误: {0}")]
    RedisConnection(String),

    #[error("Redis 操作错误: {0}")]
    RedisOperation(String),
}

impl From<redis::RedisError> for RateLimitStoreError {
    fn from(error: redis::RedisError) -> Self {
        RateLimitStoreError::RedisOperation(error.to_string())
    }
}

#[async_trait]
pub trait RateLimitBackend: Send + Sync {
    async fn check_and_record(&self, client_ip: &str) -> Result<(), RateLimitDecision>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RateLimitDecision {
    RetryAfter(u64),
    BackendUnavailable,
}

/// 客户端请求记录
#[derive(Debug, Clone)]
struct ClientRecord {
    count: u32,
    window_start: Instant,
}

impl ClientRecord {
    fn new() -> Self {
        Self {
            count: 1,
            window_start: Instant::now(),
        }
    }

    fn is_in_window(&self, window_duration: Duration) -> bool {
        self.window_start.elapsed() < window_duration
    }

    fn reset(&mut self) {
        self.count = 1;
        self.window_start = Instant::now();
    }
}

/// 内存速率限制存储，仅用于测试或显式 fallback。
#[derive(Debug)]
pub struct InMemoryRateLimitStore {
    clients: Arc<Mutex<HashMap<String, ClientRecord>>>,
    config: RateLimitConfig,
    last_cleanup: Arc<Mutex<Instant>>,
}

impl InMemoryRateLimitStore {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
            config,
            last_cleanup: Arc::new(Mutex::new(Instant::now())),
        }
    }

    async fn maybe_cleanup(&self, clients: &mut HashMap<String, ClientRecord>) {
        let mut last_cleanup = self.last_cleanup.lock().await;
        if last_cleanup.elapsed() > Duration::from_secs(300) {
            let window_duration = Duration::from_secs(self.config.window_seconds);
            clients.retain(|_, record| record.is_in_window(window_duration));
            *last_cleanup = Instant::now();
        }
    }
}

#[async_trait]
impl RateLimitBackend for InMemoryRateLimitStore {
    async fn check_and_record(&self, client_ip: &str) -> Result<(), RateLimitDecision> {
        let window_duration = Duration::from_secs(self.config.window_seconds);
        let mut clients = self.clients.lock().await;
        self.maybe_cleanup(&mut clients).await;

        match clients.get_mut(client_ip) {
            Some(record) => {
                if record.is_in_window(window_duration) {
                    if record.count >= self.config.requests_per_window {
                        let elapsed = record.window_start.elapsed();
                        let retry_after = window_duration.saturating_sub(elapsed).as_secs().max(1);
                        return Err(RateLimitDecision::RetryAfter(retry_after));
                    }
                    record.count += 1;
                    Ok(())
                } else {
                    record.reset();
                    Ok(())
                }
            }
            None => {
                clients.insert(client_ip.to_string(), ClientRecord::new());
                Ok(())
            }
        }
    }
}

/// Redis 速率限制存储，窗口状态跨重启和多实例共享。
#[derive(Debug)]
pub struct RedisRateLimitStore {
    client: RedisClient,
    config: RateLimitConfig,
    key_prefix: String,
}

impl RedisRateLimitStore {
    pub fn new(redis_url: &str, config: RateLimitConfig) -> Result<Self, RateLimitStoreError> {
        let client = RedisClient::open(redis_url)
            .map_err(|error| RateLimitStoreError::RedisConnection(error.to_string()))?;

        Ok(Self {
            client,
            config,
            key_prefix: "credbridge:rate_limit:".to_string(),
        })
    }

    pub async fn health_check(&self) -> Result<(), RateLimitStoreError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| RateLimitStoreError::RedisConnection(error.to_string()))?;

        redis::cmd("PING")
            .query_async::<_, String>(&mut conn)
            .await
            .map_err(|error| RateLimitStoreError::RedisOperation(error.to_string()))?;

        Ok(())
    }

    fn build_key(&self, client_ip: &str) -> String {
        format!("{}{}", self.key_prefix, client_ip)
    }
}

#[async_trait]
impl RateLimitBackend for RedisRateLimitStore {
    async fn check_and_record(&self, client_ip: &str) -> Result<(), RateLimitDecision> {
        let mut conn = match self.client.get_multiplexed_async_connection().await {
            Ok(conn) => conn,
            Err(error) => {
                tracing::error!(error = %error, "failed to connect to Redis rate limit store");
                return Err(RateLimitDecision::BackendUnavailable);
            }
        };

        let script = Script::new(
            r#"
local current = redis.call("INCR", KEYS[1])
if current == 1 then
  redis.call("EXPIRE", KEYS[1], ARGV[1])
end
local ttl = redis.call("TTL", KEYS[1])
return {current, ttl}
"#,
        );

        let (count, ttl): (u32, i64) = match script
            .key(self.build_key(client_ip))
            .arg(self.config.window_seconds)
            .invoke_async(&mut conn)
            .await
        {
            Ok(result) => result,
            Err(error) => {
                tracing::error!(error = %error, "failed to update Redis rate limit state");
                return Err(RateLimitDecision::BackendUnavailable);
            }
        };

        if count > self.config.requests_per_window {
            return Err(RateLimitDecision::RetryAfter(ttl.max(1) as u64));
        }

        Ok(())
    }
}

/// 速率限制状态（用于 AppState）
#[derive(Clone)]
pub struct RateLimitState {
    store: Arc<dyn RateLimitBackend>,
}

impl RateLimitState {
    pub async fn new(
        config: RateLimitConfig,
        redis_url: &str,
    ) -> Result<Self, RateLimitStoreError> {
        let store = RedisRateLimitStore::new(redis_url, config)?;
        store.health_check().await?;
        Ok(Self {
            store: Arc::new(store),
        })
    }

    pub fn new_in_memory(config: RateLimitConfig) -> Self {
        Self {
            store: Arc::new(InMemoryRateLimitStore::new(config)),
        }
    }

    pub async fn check_and_record(&self, client_ip: &str) -> Result<(), RateLimitDecision> {
        self.store.check_and_record(client_ip).await
    }
}

/// 从请求中提取客户端 IP
fn extract_client_ip(request: &Request) -> String {
    if let Some(forwarded) = request.headers().get("X-Forwarded-For")
        && let Ok(forwarded_str) = forwarded.to_str()
        && let Some(first_ip) = forwarded_str.split(',').next()
    {
        return first_ip.trim().to_string();
    }

    if let Some(real_ip) = request.headers().get("X-Real-IP")
        && let Ok(real_ip_str) = real_ip.to_str()
    {
        return real_ip_str.trim().to_string();
    }

    if let Some(ConnectInfo(addr)) = request.extensions().get::<ConnectInfo<SocketAddr>>() {
        return addr.ip().to_string();
    }

    "unknown".to_string()
}

/// 速率限制中间件
pub async fn rate_limit_middleware(request: Request, next: Next) -> Response {
    let state = request.extensions().get::<RateLimitState>().cloned();

    let Some(state) = state else {
        return next.run(request).await;
    };

    let client_ip = extract_client_ip(&request);

    match state.check_and_record(&client_ip).await {
        Ok(()) => next.run(request).await,
        Err(RateLimitDecision::RetryAfter(retry_after)) => {
            RateLimitError::new(retry_after).into_response()
        }
        Err(RateLimitDecision::BackendUnavailable) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "rate_limit_backend_unavailable",
                "message": "Rate limit backend unavailable",
            })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Duration, sleep};

    #[test]
    fn test_rate_limit_config_default() {
        let config = RateLimitConfig::default();
        assert_eq!(config.requests_per_window, 100);
        assert_eq!(config.window_seconds, 60);
    }

    #[tokio::test]
    async fn test_rate_limit_store_allows_requests() {
        let config = RateLimitConfig {
            requests_per_window: 3,
            window_seconds: 60,
        };
        let store = InMemoryRateLimitStore::new(config);

        assert!(store.check_and_record("192.168.1.1").await.is_ok());
        assert!(store.check_and_record("192.168.1.1").await.is_ok());
        assert!(store.check_and_record("192.168.1.1").await.is_ok());
        assert!(store.check_and_record("192.168.1.1").await.is_err());
    }

    #[tokio::test]
    async fn test_rate_limit_store_per_client() {
        let config = RateLimitConfig {
            requests_per_window: 2,
            window_seconds: 60,
        };
        let store = InMemoryRateLimitStore::new(config);

        assert!(store.check_and_record("192.168.1.1").await.is_ok());
        assert!(store.check_and_record("192.168.1.1").await.is_ok());
        assert!(store.check_and_record("192.168.1.1").await.is_err());

        assert!(store.check_and_record("192.168.1.2").await.is_ok());
        assert!(store.check_and_record("192.168.1.2").await.is_ok());
        assert!(store.check_and_record("192.168.1.2").await.is_err());
    }

    #[tokio::test]
    async fn test_rate_limit_store_window_expires() {
        let config = RateLimitConfig {
            requests_per_window: 1,
            window_seconds: 1,
        };
        let store = InMemoryRateLimitStore::new(config);

        assert!(store.check_and_record("192.168.1.1").await.is_ok());
        assert!(store.check_and_record("192.168.1.1").await.is_err());

        sleep(Duration::from_millis(1100)).await;

        assert!(store.check_and_record("192.168.1.1").await.is_ok());
    }

    #[test]
    fn test_rate_limit_error_response() {
        let error = RateLimitError::new(30);
        assert_eq!(error.error, "rate_limit_exceeded");
        assert_eq!(error.retry_after_seconds, 30);
        assert!(error.message.contains("30"));
    }

    #[tokio::test]
    #[ignore = "requires local Redis service"]
    async fn test_redis_rate_limit_store_persists_window_state() {
        let config = RateLimitConfig {
            requests_per_window: 1,
            window_seconds: 5,
        };
        let store_a = RedisRateLimitStore::new("redis://127.0.0.1:6379", config.clone()).unwrap();
        let store_b = RedisRateLimitStore::new("redis://127.0.0.1:6379", config).unwrap();

        assert!(store_a.check_and_record("203.0.113.1").await.is_ok());
        assert!(store_b.check_and_record("203.0.113.1").await.is_err());
    }
}
