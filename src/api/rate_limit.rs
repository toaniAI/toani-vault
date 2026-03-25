//! API 速率限制中间件
//!
//! 基于 IP 地址的速率限制，防止 DoS 攻击
//! 配置项：
//! - CREDBRIDGE_RATE_LIMIT_REQUESTS: 每个时间窗口允许的请求数 (默认: 100)
//! - CREDBRIDGE_RATE_LIMIT_WINDOW_SECONDS: 时间窗口（秒）(默认: 60)

use axum::{
    Json,
    extract::{ConnectInfo, Request},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

/// 客户端请求记录
#[derive(Debug, Clone)]
struct ClientRecord {
    /// 请求次数
    count: u32,
    /// 窗口开始时间
    window_start: Instant,
}

impl ClientRecord {
    fn new() -> Self {
        Self {
            count: 1,
            window_start: Instant::now(),
        }
    }

    /// 检查是否在窗口内
    fn is_in_window(&self, window_duration: Duration) -> bool {
        self.window_start.elapsed() < window_duration
    }

    /// 重置窗口
    fn reset(&mut self) {
        self.count = 1;
        self.window_start = Instant::now();
    }
}

/// 速率限制存储
#[derive(Debug, Clone)]
pub struct RateLimitStore {
    /// 客户端记录 (IP -> 请求记录)
    clients: Arc<Mutex<HashMap<String, ClientRecord>>>,
    /// 配置
    config: RateLimitConfig,
    /// 最后清理时间
    last_cleanup: Arc<Mutex<Instant>>,
}

impl RateLimitStore {
    /// 创建新的速率限制存储
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
            config,
            last_cleanup: Arc::new(Mutex::new(Instant::now())),
        }
    }

    /// 检查并记录请求
    /// 返回：Ok(()) 表示允许请求，Err(retry_after) 表示需要等待的秒数
    pub fn check_and_record(&self, client_ip: &str) -> Result<(), u64> {
        let window_duration = Duration::from_secs(self.config.window_seconds);
        let mut clients = self.clients.lock().unwrap();

        // 定期清理过期记录（每 5 分钟）
        self.maybe_cleanup(&mut clients);

        match clients.get_mut(client_ip) {
            Some(record) => {
                if record.is_in_window(window_duration) {
                    // 在窗口内，检查是否超过限制
                    if record.count >= self.config.requests_per_window {
                        let elapsed = record.window_start.elapsed();
                        let retry_after = window_duration.saturating_sub(elapsed).as_secs().max(1);
                        return Err(retry_after);
                    }
                    record.count += 1;
                    Ok(())
                } else {
                    // 窗口过期，重置
                    record.reset();
                    Ok(())
                }
            }
            None => {
                // 新客户端
                clients.insert(client_ip.to_string(), ClientRecord::new());
                Ok(())
            }
        }
    }

    /// 可能需要清理过期记录
    fn maybe_cleanup(&self, clients: &mut std::sync::MutexGuard<HashMap<String, ClientRecord>>) {
        let mut last_cleanup = self.last_cleanup.lock().unwrap();
        if last_cleanup.elapsed() > Duration::from_secs(300) {
            // 5 分钟清理一次
            let window_duration = Duration::from_secs(self.config.window_seconds);
            clients.retain(|_, record| record.is_in_window(window_duration));
            *last_cleanup = Instant::now();
        }
    }

    /// 获取当前配置
    pub fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    /// 获取当前客户端数量（用于监控）
    pub fn client_count(&self) -> usize {
        self.clients.lock().unwrap().len()
    }
}

/// 从请求中提取客户端 IP
fn extract_client_ip(request: &Request) -> String {
    // 1. 首先尝试从 X-Forwarded-For 头获取（如果通过代理）
    if let Some(forwarded) = request.headers().get("X-Forwarded-For")
        && let Ok(forwarded_str) = forwarded.to_str()
    {
        // 取第一个 IP（最原始的客户端）
        if let Some(first_ip) = forwarded_str.split(',').next() {
            return first_ip.trim().to_string();
        }
    }

    // 2. 尝试 X-Real-IP 头
    if let Some(real_ip) = request.headers().get("X-Real-IP")
        && let Ok(real_ip_str) = real_ip.to_str()
    {
        return real_ip_str.trim().to_string();
    }

    // 3. 使用连接地址
    if let Some(ConnectInfo(addr)) = request.extensions().get::<ConnectInfo<SocketAddr>>() {
        return addr.ip().to_string();
    }

    // 4. 无法获取 IP，使用 "unknown"
    "unknown".to_string()
}

/// 速率限制中间件
pub async fn rate_limit_middleware(request: Request, next: Next) -> Response {
    // 从请求扩展中获取速率限制存储
    let store = request.extensions().get::<RateLimitStore>().cloned();

    let Some(store) = store else {
        // 如果没有配置速率限制，直接放行
        return next.run(request).await;
    };

    let client_ip = extract_client_ip(&request);

    // 检查速率限制
    match store.check_and_record(&client_ip) {
        Ok(()) => {
            // 允许请求
            next.run(request).await
        }
        Err(retry_after) => {
            // 超出限制
            RateLimitError::new(retry_after).into_response()
        }
    }
}

/// 速率限制状态（用于 AppState）
#[derive(Clone)]
pub struct RateLimitState {
    pub store: RateLimitStore,
}

impl RateLimitState {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            store: RateLimitStore::new(config),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_config_default() {
        let config = RateLimitConfig::default();
        assert_eq!(config.requests_per_window, 100);
        assert_eq!(config.window_seconds, 60);
    }

    #[test]
    fn test_rate_limit_store_allows_requests() {
        let config = RateLimitConfig {
            requests_per_window: 3,
            window_seconds: 60,
        };
        let store = RateLimitStore::new(config);

        // 前 3 个请求应该通过
        assert!(store.check_and_record("192.168.1.1").is_ok());
        assert!(store.check_and_record("192.168.1.1").is_ok());
        assert!(store.check_and_record("192.168.1.1").is_ok());

        // 第 4 个请求应该被拒绝
        assert!(store.check_and_record("192.168.1.1").is_err());
    }

    #[test]
    fn test_rate_limit_store_per_client() {
        let config = RateLimitConfig {
            requests_per_window: 2,
            window_seconds: 60,
        };
        let store = RateLimitStore::new(config);

        // 客户端 1 用完配额
        assert!(store.check_and_record("192.168.1.1").is_ok());
        assert!(store.check_and_record("192.168.1.1").is_ok());
        assert!(store.check_and_record("192.168.1.1").is_err());

        // 客户端 2 不受影响
        assert!(store.check_and_record("192.168.1.2").is_ok());
        assert!(store.check_and_record("192.168.1.2").is_ok());
        assert!(store.check_and_record("192.168.1.2").is_err());
    }

    #[test]
    fn test_rate_limit_error_response() {
        let error = RateLimitError::new(30);
        assert_eq!(error.error, "rate_limit_exceeded");
        assert_eq!(error.retry_after_seconds, 30);
        assert!(error.message.contains("30"));
    }

    #[test]
    fn test_client_record() {
        let mut record = ClientRecord::new();
        assert_eq!(record.count, 1);

        // 应该在窗口内
        assert!(record.is_in_window(Duration::from_secs(60)));

        // 重置
        record.reset();
        assert_eq!(record.count, 1);
    }
}
