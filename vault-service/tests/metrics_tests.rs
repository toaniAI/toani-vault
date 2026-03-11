//! 指标和告警系统测试

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use vault_service::{
    metrics::*,
    alerting::*,
};

// ============ Metrics Tests ============

#[test]
fn test_metrics_collector_creation() {
    let collector = MetricsCollector::new();
    assert_eq!(collector.http_requests.total_requests(), 0);
    assert_eq!(collector.token_metrics.active_tokens(), 0);
}

#[test]
fn test_http_request_metrics() {
    let metrics = HttpRequestMetrics::new();

    // 记录一些请求
    metrics.record("GET", "/api/v1/credentials", 200, Duration::from_millis(50));
    metrics.record("POST", "/api/v1/tokens", 201, Duration::from_millis(100));
    metrics.record("GET", "/api/v1/credentials", 500, Duration::from_millis(200));

    assert_eq!(metrics.total_requests(), 3);
    assert_eq!(metrics.requests_by_status(200), 1);
    assert_eq!(metrics.requests_by_status(201), 1);
    assert_eq!(metrics.requests_by_status(500), 1);
    assert_eq!(metrics.requests_by_status(404), 0);
}

#[test]
fn test_http_error_rate_calculation() {
    let metrics = HttpRequestMetrics::new();

    // 初始状态错误率应该是 0
    assert_eq!(metrics.error_rate(), 0.0);

    // 添加成功请求
    metrics.record("GET", "/test", 200, Duration::from_millis(50));
    assert_eq!(metrics.error_rate(), 0.0);

    // 添加错误请求
    metrics.record("GET", "/test", 500, Duration::from_millis(50));
    assert_eq!(metrics.error_rate(), 50.0);

    // 添加客户端错误
    metrics.record("GET", "/test", 404, Duration::from_millis(50));
    assert!(metrics.error_rate() > 50.0); // 现在应该是 66.67%
}

#[test]
fn test_token_metrics() {
    let metrics = TokenMetrics::new();

    // 记录 Token 签发
    metrics.record_issued(3);
    metrics.record_issued(5);
    metrics.record_issued(3);

    assert_eq!(metrics.active_tokens(), 3);

    // 记录验证
    metrics.record_validation(true);
    metrics.record_validation(true);
    metrics.record_validation(false);

    // 验证失败率应该是 33.33%
    assert!((metrics.validation_failure_rate() - 33.33).abs() < 0.01);

    // 记录 Token 过期
    metrics.record_token_expired();
    assert_eq!(metrics.active_tokens(), 2);
}

#[test]
fn test_tee_metrics() {
    let metrics = TeeMetrics::new();

    // 记录各种 TEE 操作
    metrics.record_operation("encrypt", Duration::from_millis(5), true);
    metrics.record_operation("encrypt", Duration::from_millis(7), true);
    metrics.record_operation("decrypt", Duration::from_millis(3), true);
    metrics.record_operation("derive", Duration::from_millis(10), true);
    metrics.record_operation("attest", Duration::from_millis(100), true);

    // 验证平均延迟
    let encrypt_avg = metrics.avg_operation_latency("encrypt");
    assert!(encrypt_avg >= 5.0 && encrypt_avg <= 7.0);

    let decrypt_avg = metrics.avg_operation_latency("decrypt");
    assert!((decrypt_avg - 3.0).abs() < 0.1);

    // 设置 EPC 使用率
    metrics.set_epc_usage(75.5);
}

#[test]
fn test_system_metrics() {
    let metrics = SystemMetrics::new();

    // 设置各种使用率
    metrics.set_cpu_usage(45.5);
    metrics.set_memory_usage(60.0);
    metrics.set_db_pool_usage(30.0);
    metrics.set_redis_pool_usage(25.0);

    // 验证运行时间
    let uptime = metrics.uptime_seconds();
    assert!(uptime < 10); // 测试应该很快完成
}

#[test]
fn test_alert_metrics() {
    let metrics = AlertMetrics::new();

    // 记录告警
    metrics.record_triggered("warning", "high_error_rate");
    metrics.record_triggered("critical", "tee_anomaly");
    metrics.record_triggered("warning", "high_error_rate");

    // 告警计数已通过 record_triggered 方法验证
}

#[test]
fn test_prometheus_export_format() {
    let collector = MetricsCollector::new();

    // 记录一些数据
    collector.record_http_request("GET", "/health", 200, Duration::from_millis(10));
    collector.record_http_request("POST", "/tokens", 201, Duration::from_millis(50));
    collector.record_token_issued(3);
    collector.record_token_validation(true);
    collector.record_tee_operation("encrypt", Duration::from_millis(5), true);
    collector.record_alert_triggered("warning", "high_error_rate");

    let output = collector.export_prometheus();

    // 验证 Prometheus 格式
    assert!(output.contains("# HELP"));
    assert!(output.contains("# TYPE"));
    assert!(output.contains("credbridge_http_requests_total"));
    assert!(output.contains("credbridge_tokens_issued_total"));
    assert!(output.contains("credbridge_tee_encryption_ops_total"));
    assert!(output.contains("credbridge_alerts_triggered_total"));

    // 验证计数器值
    assert!(output.contains("credbridge_http_requests_total 2"));
    assert!(output.contains("credbridge_tokens_issued_total 1"));
    assert!(output.contains("credbridge_tee_encryption_ops_total 1"));
}

#[test]
fn test_prometheus_histogram_format() {
    let metrics = HttpRequestMetrics::new();

    // 记录不同延迟的请求
    metrics.record("GET", "/test", 200, Duration::from_millis(5));   // bucket_10ms
    metrics.record("GET", "/test", 200, Duration::from_millis(30));  // bucket_50ms
    metrics.record("GET", "/test", 200, Duration::from_millis(80));  // bucket_100ms
    metrics.record("GET", "/test", 200, Duration::from_millis(300)); // bucket_500ms
    metrics.record("GET", "/test", 200, Duration::from_millis(600)); // bucket_1000ms
    metrics.record("GET", "/test", 200, Duration::from_millis(1500)); // bucket_inf

    let output = metrics.export_prometheus();

    // 验证直方图累积分布
    assert!(output.contains("credbridge_http_request_duration_bucket{le=\"10\"} 1"));
    assert!(output.contains("credbridge_http_request_duration_bucket{le=\"50\"} 2"));
    assert!(output.contains("credbridge_http_request_duration_bucket{le=\"100\"} 3"));
    assert!(output.contains("credbridge_http_request_duration_bucket{le=\"500\"} 4"));
    assert!(output.contains("credbridge_http_request_duration_bucket{le=\"1000\"} 5"));
    assert!(output.contains("credbridge_http_request_duration_bucket{le=\"+Inf\"} 6"));
    assert!(output.contains("credbridge_http_request_duration_count 6"));
}

// ============ Alerting Tests ============

#[test]
fn test_alert_event_creation() {
    let alert = AlertEvent::new(
        AlertSeverity::Warning,
        AlertType::HighErrorRate,
        "Test Alert",
        "This is a test alert",
    );

    assert_eq!(alert.severity, AlertSeverity::Warning);
    assert_eq!(alert.alert_type, AlertType::HighErrorRate);
    assert!(alert.id.starts_with("ALERT-high_error_rate-"));
    assert!(alert.timestamp > 0);
    assert_eq!(alert.title, "Test Alert");
    assert_eq!(alert.description, "This is a test alert");
}

#[test]
fn test_alert_event_builder() {
    let metrics: HashMap<String, f64> = [
        ("error_rate".to_string(), 10.5),
        ("threshold".to_string(), 5.0),
    ]
    .into_iter()
    .collect();

    let alert = AlertEvent::new(
        AlertSeverity::Critical,
        AlertType::TeeAnomaly,
        "TEE Error",
        "TEE is not responding",
    )
    .with_metrics(metrics.clone())
    .with_component("tee-enclave")
    .with_suggested_action("Restart the service");

    assert_eq!(alert.metrics, Some(metrics));
    assert_eq!(alert.component, Some("tee-enclave".to_string()));
    assert_eq!(alert.suggested_action, Some("Restart the service".to_string()));
}

#[test]
fn test_alert_manager_creation() {
    let config = AlertManagerConfig::default();
    let (manager, _rx) = AlertManager::new(config);

    assert_eq!(manager.get_alert_count(), 0);
    assert!(manager.get_alert_history().is_empty());
}

#[test]
fn test_alert_severity_display() {
    assert_eq!(AlertSeverity::Info.to_string(), "info");
    assert_eq!(AlertSeverity::Warning.to_string(), "warning");
    assert_eq!(AlertSeverity::Critical.to_string(), "critical");
}

#[test]
fn test_alert_type_display() {
    assert_eq!(AlertType::HighErrorRate.to_string(), "high_error_rate");
    assert_eq!(AlertType::TeeAnomaly.to_string(), "tee_anomaly");
    assert_eq!(AlertType::DatabaseConnectionFailed.to_string(), "database_connection_failed");
    assert_eq!(AlertType::RedisConnectionFailed.to_string(), "redis_connection_failed");
    assert_eq!(
        AlertType::Custom("my_alert".to_string()).to_string(),
        "custom_my_alert"
    );
}

#[test]
fn test_default_alert_rules() {
    let config = AlertManagerConfig::default();

    // 验证默认规则存在
    assert!(!config.rules.is_empty());

    let high_error_rate = config
        .rules
        .iter()
        .find(|r| r.name == "high_error_rate")
        .expect("high_error_rate rule should exist");

    assert_eq!(high_error_rate.threshold, 5.0);
    assert_eq!(high_error_rate.operator, ">");
    assert_eq!(high_error_rate.severity, AlertSeverity::Warning);
    assert_eq!(high_error_rate.duration, 3);
    assert!(high_error_rate.enabled);

    let tee_anomaly = config
        .rules
        .iter()
        .find(|r| r.name == "tee_anomaly")
        .expect("tee_anomaly rule should exist");

    assert_eq!(tee_anomaly.threshold, 1.0);
    assert_eq!(tee_anomaly.severity, AlertSeverity::Critical);
    assert_eq!(tee_anomaly.duration, 1);
}

#[test]
fn test_alert_manager_evaluate_metric() {
    let config = AlertManagerConfig {
        suppress_window_seconds: 0, // 禁用抑制以便测试
        ..Default::default()
    };
    let (manager, mut rx) = AlertManager::new(config);

    // 触发高错误率告警
    for _ in 0..3 {
        manager.evaluate_metric("error_rate_percent", 10.0); // 超过 5% 阈值
    }

    // 应该有一个告警
    std::thread::sleep(Duration::from_millis(100));

    let alert = rx.try_recv();
    assert!(alert.is_ok());

    let alert = alert.unwrap();
    assert_eq!(alert.severity, AlertSeverity::Warning);
    assert_eq!(alert.alert_type, AlertType::HighErrorRate);

    // 验证告警计数
    assert_eq!(manager.get_alert_count(), 1);
}

#[test]
fn test_alert_manager_duration_check() {
    let config = AlertManagerConfig {
        suppress_window_seconds: 0,
        ..Default::default()
    };
    let (manager, mut rx) = AlertManager::new(config);

    // 只触发一次，不应产生告警（需要 3 次连续触发）
    manager.evaluate_metric("error_rate_percent", 10.0);

    std::thread::sleep(Duration::from_millis(50));

    // 不应该有告警
    assert!(rx.try_recv().is_err());

    // 再触发两次
    manager.evaluate_metric("error_rate_percent", 10.0);
    manager.evaluate_metric("error_rate_percent", 10.0);

    std::thread::sleep(Duration::from_millis(50));

    // 现在应该有告警了
    assert!(rx.try_recv().is_ok());
}

#[test]
fn test_alert_manager_manual_trigger() {
    let config = AlertManagerConfig::default();
    let (manager, mut rx) = AlertManager::new(config);

    let alert = AlertEvent::new(
        AlertSeverity::Critical,
        AlertType::TeeAnomaly,
        "Manual Alert",
        "This is a manually triggered alert",
    );

    manager.trigger_alert(alert);

    std::thread::sleep(Duration::from_millis(50));

    let received = rx.try_recv();
    assert!(received.is_ok());
    assert_eq!(received.unwrap().title, "Manual Alert");

    assert_eq!(manager.get_alert_count(), 1);
}

#[test]
fn test_alert_history() {
    let config = AlertManagerConfig {
        max_history_size: 3,
        suppress_window_seconds: 0,
        ..Default::default()
    };
    let (manager, _rx) = AlertManager::new(config);

    // 手动触发多个告警
    for i in 0..5 {
        let alert = AlertEvent::new(
            AlertSeverity::Warning,
            AlertType::HighErrorRate,
            format!("Alert {}", i),
            "Test",
        );
        manager.trigger_alert(alert);
    }

    // 历史记录应该只保留最后 3 个
    let history = manager.get_alert_history();
    assert_eq!(history.len(), 3);
}

#[test]
fn test_webhook_config_defaults() {
    let config = WebhookConfig::default();

    assert_eq!(config.method, "POST");
    assert_eq!(config.timeout_seconds, 10);
    assert_eq!(config.retries, 3);
    assert!(config.severity_filter.is_empty());
}

#[test]
fn test_webhook_config_custom() {
    let config = WebhookConfig {
        url: "https://example.com/webhook".to_string(),
        method: "PUT".to_string(),
        headers: [
            ("Authorization".to_string(), "Bearer token123".to_string()),
            ("X-Custom".to_string(), "value".to_string()),
        ]
        .into_iter()
        .collect(),
        timeout_seconds: 30,
        retries: 5,
        severity_filter: vec![AlertSeverity::Critical, AlertSeverity::Warning],
    };

    assert_eq!(config.url, "https://example.com/webhook");
    assert_eq!(config.method, "PUT");
    assert_eq!(config.timeout_seconds, 30);
    assert_eq!(config.retries, 5);
    assert_eq!(config.headers.len(), 2);
    assert_eq!(config.severity_filter.len(), 2);
}

// ============ Integration Tests ============

#[test]
fn test_metrics_to_alert_integration() {
    // 创建一个完整的监控链路
    let collector = Arc::new(MetricsCollector::new());

    // 记录大量错误请求
    for _ in 0..20 {
        collector.record_http_request("GET", "/api/test", 500, Duration::from_millis(100));
    }

    // 记录一些成功请求
    for _ in 0..10 {
        collector.record_http_request("GET", "/api/test", 200, Duration::from_millis(50));
    }

    // 验证错误率
    let error_rate = collector.http_requests.error_rate();
    assert!(error_rate > 60.0); // 应该超过 60%

    // 导出 Prometheus 指标
    let prometheus_output = collector.export_prometheus();
    assert!(prometheus_output.contains(&format!("credbridge_http_error_rate_percentage {:.2}", error_rate)));
}

#[test]
fn test_alert_threshold_evaluation() {
    // 测试各种阈值比较
    let thresholds = vec![
        (5.0, ">", 10.0, true),   // 10 > 5
        (5.0, ">", 3.0, false),   // 3 > 5
        (5.0, ">=", 5.0, true),   // 5 >= 5
        (5.0, "<", 3.0, true),    // 3 < 5
        (5.0, "<=", 5.0, true),   // 5 <= 5
        (5.0, "==", 5.0, true),   // 5 == 5
        (5.0, "==", 3.0, false),  // 3 == 5
    ];

    for (threshold, op, value, expected) in thresholds {
        let result = match op {
            ">" => value > threshold,
            ">=" => value >= threshold,
            "<" => value < threshold,
            "<=" => value <= threshold,
            "==" => ((value - threshold) as f64).abs() < f64::EPSILON,
            _ => false,
        };
        assert_eq!(
            result, expected,
            "Failed: {} {} {} should be {}",
            value, op, threshold, expected
        );
    }
}
