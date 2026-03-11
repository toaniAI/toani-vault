//! 速率限制中间件测试

use vault_service::api::rate_limit::{
    RateLimitConfig, RateLimitError, RateLimitState, RateLimitStore,
};

/// 测试速率限制配置默认值
#[test]
fn test_rate_limit_config_default() {
    let config = RateLimitConfig::default();
    assert_eq!(config.requests_per_window, 100);
    assert_eq!(config.window_seconds, 60);
}

/// 测试速率限制配置从环境变量加载
#[test]
fn test_rate_limit_config_from_env() {
    // 设置环境变量
    std::env::set_var("CREDBRIDGE_RATE_LIMIT_REQUESTS", "50");
    std::env::set_var("CREDBRIDGE_RATE_LIMIT_WINDOW_SECONDS", "30");

    let config = RateLimitConfig::from_env();
    assert_eq!(config.requests_per_window, 50);
    assert_eq!(config.window_seconds, 30);

    // 清理环境变量
    std::env::remove_var("CREDBRIDGE_RATE_LIMIT_REQUESTS");
    std::env::remove_var("CREDBRIDGE_RATE_LIMIT_WINDOW_SECONDS");
}

/// 测试速率限制存储基本功能
#[test]
fn test_rate_limit_store_allows_requests() {
    let config = RateLimitConfig {
        requests_per_window: 5,
        window_seconds: 60,
    };
    let store = RateLimitStore::new(config);

    // 前 5 个请求应该通过
    for _ in 0..5 {
        assert!(store.check_and_record("192.168.1.1").is_ok());
    }

    // 第 6 个请求应该被拒绝
    let result = store.check_and_record("192.168.1.1");
    assert!(result.is_err());
    let retry_after = result.unwrap_err();
    assert!(retry_after > 0);
}

/// 测试速率限制按客户端隔离
#[test]
fn test_rate_limit_per_client_isolation() {
    let config = RateLimitConfig {
        requests_per_window: 3,
        window_seconds: 60,
    };
    let store = RateLimitStore::new(config);

    // 客户端 1 用完配额
    for _ in 0..3 {
        assert!(store.check_and_record("client1").is_ok());
    }
    assert!(store.check_and_record("client1").is_err());

    // 客户端 2 不受影响
    for _ in 0..3 {
        assert!(store.check_and_record("client2").is_ok());
    }
    assert!(store.check_and_record("client2").is_err());
}

/// 测试速率限制错误响应
#[test]
fn test_rate_limit_error() {
    let error = RateLimitError::new(30);
    assert_eq!(error.error, "rate_limit_exceeded");
    assert_eq!(error.retry_after_seconds, 30);
    assert!(error.message.contains("30"));
}

/// 测试速率限制状态创建
#[test]
fn test_rate_limit_state_creation() {
    let config = RateLimitConfig {
        requests_per_window: 10,
        window_seconds: 30,
    };
    let state = RateLimitState::new(config);
    assert_eq!(state.store.config().requests_per_window, 10);
    assert_eq!(state.store.config().window_seconds, 30);
}

/// 测试速率限制客户端计数
#[test]
fn test_rate_limit_client_count() {
    let config = RateLimitConfig {
        requests_per_window: 5,
        window_seconds: 60,
    };
    let store = RateLimitStore::new(config);

    // 初始应该为 0
    assert_eq!(store.client_count(), 0);

    // 添加客户端
    store.check_and_record("client1").unwrap();
    assert_eq!(store.client_count(), 1);

    store.check_and_record("client2").unwrap();
    assert_eq!(store.client_count(), 2);

    // 同一客户端不应增加计数
    store.check_and_record("client1").unwrap();
    assert_eq!(store.client_count(), 2);
}

/// 测试高并发场景（模拟）
#[test]
fn test_rate_limit_under_load() {
    let config = RateLimitConfig {
        requests_per_window: 100,
        window_seconds: 60,
    };
    let store = RateLimitStore::new(config);

    // 模拟单个客户端快速请求
    let client_ip = "192.168.1.100";
    let mut allowed = 0;
    let mut blocked = 0;

    for _ in 0..150 {
        match store.check_and_record(client_ip) {
            Ok(()) => allowed += 1,
            Err(_) => blocked += 1,
        }
    }

    // 应该允许 100 个请求，阻止 50 个
    assert_eq!(allowed, 100);
    assert_eq!(blocked, 50);
}
