//! 指标收集模块
//!
//! 提供 Prometheus 格式的指标收集和导出功能：
//! - 请求计数和延迟
//! - 错误率
//! - Token 使用率
//! - TEE 性能指标

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

/// 指标收集器
#[derive(Debug, Clone)]
pub struct MetricsCollector {
    /// HTTP 请求计数器
    pub http_requests: Arc<HttpRequestMetrics>,
    /// Token 使用指标
    pub token_metrics: Arc<TokenMetrics>,
    /// TEE 性能指标
    pub tee_metrics: Arc<TeeMetrics>,
    /// 系统指标
    pub system_metrics: Arc<SystemMetrics>,
    /// 告警指标
    pub alert_metrics: Arc<AlertMetrics>,
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsCollector {
    /// 创建新的指标收集器
    pub fn new() -> Self {
        Self {
            http_requests: Arc::new(HttpRequestMetrics::new()),
            token_metrics: Arc::new(TokenMetrics::new()),
            tee_metrics: Arc::new(TeeMetrics::new()),
            system_metrics: Arc::new(SystemMetrics::new()),
            alert_metrics: Arc::new(AlertMetrics::new()),
        }
    }

    /// 记录 HTTP 请求
    pub fn record_http_request(
        &self,
        method: &str,
        path: &str,
        status_code: u16,
        duration: Duration,
    ) {
        self.http_requests
            .record(method, path, status_code, duration);
    }

    /// 记录 Token 签发
    pub fn record_token_issued(&self, scope_count: usize) {
        self.token_metrics.record_issued(scope_count);
    }

    /// 记录 Token 验证
    pub fn record_token_validation(&self, success: bool) {
        self.token_metrics.record_validation(success);
    }

    /// 记录 TEE 操作
    pub fn record_tee_operation(&self, operation: &str, duration: Duration, success: bool) {
        self.tee_metrics
            .record_operation(operation, duration, success);
    }

    /// 记录告警触发
    pub fn record_alert_triggered(&self, severity: &str, alert_type: &str) {
        self.alert_metrics.record_triggered(severity, alert_type);
    }

    /// 导出 Prometheus 格式指标
    pub fn export_prometheus(&self) -> String {
        let mut output = String::new();

        // HTTP 请求指标
        output.push_str(&self.http_requests.export_prometheus());
        output.push('\n');

        // Token 指标
        output.push_str(&self.token_metrics.export_prometheus());
        output.push('\n');

        // TEE 指标
        output.push_str(&self.tee_metrics.export_prometheus());
        output.push('\n');

        // 系统指标
        output.push_str(&self.system_metrics.export_prometheus());
        output.push('\n');

        // 告警指标
        output.push_str(&self.alert_metrics.export_prometheus());

        output
    }
}

/// HTTP 请求指标
#[derive(Debug)]
pub struct HttpRequestMetrics {
    /// 总请求数
    total_requests: AtomicU64,
    /// 按方法和路径的请求计数
    requests_by_path: Mutex<HashMap<(String, String), u64>>,
    /// 按状态码的请求计数
    requests_by_status: Mutex<HashMap<u16, u64>>,
    /// 请求延迟直方图（毫秒）
    latency_buckets: Mutex<LatencyBuckets>,
}

/// 延迟分桶
#[derive(Debug, Default)]
struct LatencyBuckets {
    /// < 10ms
    bucket_10ms: u64,
    /// < 50ms
    bucket_50ms: u64,
    /// < 100ms
    bucket_100ms: u64,
    /// < 500ms
    bucket_500ms: u64,
    /// < 1000ms
    bucket_1000ms: u64,
    /// >= 1000ms
    bucket_inf: u64,
    /// 总延迟和（用于计算平均值）
    total_latency_ms: u64,
}

impl HttpRequestMetrics {
    /// 创建新的 HTTP 请求指标
    pub fn new() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            requests_by_path: Mutex::new(HashMap::new()),
            requests_by_status: Mutex::new(HashMap::new()),
            latency_buckets: Mutex::new(LatencyBuckets::default()),
        }
    }

    /// 记录请求
    pub fn record(&self, method: &str, path: &str, status_code: u16, duration: Duration) {
        // 增加总请求数
        self.total_requests.fetch_add(1, Ordering::Relaxed);

        // 按路径计数
        if let Ok(mut paths) = self.requests_by_path.lock() {
            let key = (method.to_string(), path.to_string());
            *paths.entry(key).or_insert(0) += 1;
        }

        // 按状态码计数
        if let Ok(mut statuses) = self.requests_by_status.lock() {
            *statuses.entry(status_code).or_insert(0) += 1;
        }

        // 记录延迟
        if let Ok(mut buckets) = self.latency_buckets.lock() {
            let ms = duration.as_millis() as u64;
            buckets.total_latency_ms += ms;

            match ms {
                0..=10 => buckets.bucket_10ms += 1,
                11..=50 => buckets.bucket_50ms += 1,
                51..=100 => buckets.bucket_100ms += 1,
                101..=500 => buckets.bucket_500ms += 1,
                501..=1000 => buckets.bucket_1000ms += 1,
                _ => buckets.bucket_inf += 1,
            }
        }
    }

    /// 获取总请求数
    pub fn total_requests(&self) -> u64 {
        self.total_requests.load(Ordering::Relaxed)
    }

    /// 获取按状态码的请求数
    pub fn requests_by_status(&self, status_code: u16) -> u64 {
        if let Ok(statuses) = self.requests_by_status.lock() {
            *statuses.get(&status_code).unwrap_or(&0)
        } else {
            0
        }
    }

    /// 计算错误率（4xx 和 5xx 状态码占比）
    pub fn error_rate(&self) -> f64 {
        let total = self.total_requests.load(Ordering::Relaxed) as f64;
        if total == 0.0 {
            return 0.0;
        }

        let error_count: u64 = if let Ok(statuses) = self.requests_by_status.lock() {
            statuses
                .iter()
                .filter(|(code, _)| **code >= 400)
                .map(|(_, count)| *count)
                .sum()
        } else {
            0
        };

        (error_count as f64 / total) * 100.0
    }

    /// 导出 Prometheus 格式
    pub fn export_prometheus(&self) -> String {
        let mut output = String::new();

        // 总请求数
        output.push_str("# HELP credbridge_http_requests_total Total HTTP requests\n");
        output.push_str("# TYPE credbridge_http_requests_total counter\n");
        output.push_str(&format!(
            "credbridge_http_requests_total {}\n",
            self.total_requests.load(Ordering::Relaxed)
        ));

        // 按状态码的请求数
        output
            .push_str("\n# HELP credbridge_http_requests_by_status HTTP requests by status code\n");
        output.push_str("# TYPE credbridge_http_requests_by_status counter\n");
        if let Ok(statuses) = self.requests_by_status.lock() {
            for (code, count) in statuses.iter() {
                output.push_str(&format!(
                    "credbridge_http_requests_by_status{{code=\"{}\"}} {}\n",
                    code, count
                ));
            }
        }

        // 延迟直方图
        output.push_str(
            "\n# HELP credbridge_http_request_duration_bucket Request duration buckets\n",
        );
        output.push_str("# TYPE credbridge_http_request_duration_bucket histogram\n");
        if let Ok(buckets) = self.latency_buckets.lock() {
            let total: u64 = self.total_requests.load(Ordering::Relaxed);
            output.push_str(&format!(
                "credbridge_http_request_duration_bucket{{le=\"10\"}} {}\n",
                buckets.bucket_10ms
            ));
            output.push_str(&format!(
                "credbridge_http_request_duration_bucket{{le=\"50\"}} {}\n",
                buckets.bucket_10ms + buckets.bucket_50ms
            ));
            output.push_str(&format!(
                "credbridge_http_request_duration_bucket{{le=\"100\"}} {}\n",
                buckets.bucket_10ms + buckets.bucket_50ms + buckets.bucket_100ms
            ));
            output.push_str(&format!(
                "credbridge_http_request_duration_bucket{{le=\"500\"}} {}\n",
                buckets.bucket_10ms
                    + buckets.bucket_50ms
                    + buckets.bucket_100ms
                    + buckets.bucket_500ms
            ));
            output.push_str(&format!(
                "credbridge_http_request_duration_bucket{{le=\"1000\"}} {}\n",
                buckets.bucket_10ms
                    + buckets.bucket_50ms
                    + buckets.bucket_100ms
                    + buckets.bucket_500ms
                    + buckets.bucket_1000ms
            ));
            output.push_str(&format!(
                "credbridge_http_request_duration_bucket{{le=\"+Inf\"}} {}\n",
                total
            ));
            output.push_str(&format!(
                "credbridge_http_request_duration_sum {}\n",
                buckets.total_latency_ms
            ));
            output.push_str(&format!(
                "credbridge_http_request_duration_count {}\n",
                total
            ));
        }

        // 错误率
        output.push_str(
            "\n# HELP credbridge_http_error_rate_percentage HTTP error rate percentage\n",
        );
        output.push_str("# TYPE credbridge_http_error_rate_percentage gauge\n");
        output.push_str(&format!(
            "credbridge_http_error_rate_percentage {:.2}\n",
            self.error_rate()
        ));

        output
    }
}

/// Token 使用指标
#[derive(Debug)]
pub struct TokenMetrics {
    /// 签发的 Token 总数
    tokens_issued: AtomicU64,
    /// 验证成功的次数
    validations_success: AtomicU64,
    /// 验证失败的次数
    validations_failed: AtomicU64,
    /// 当前活跃的 Token 数
    active_tokens: AtomicU64,
    /// 按 scope 数量的 Token 分布
    tokens_by_scope_count: Mutex<HashMap<usize, u64>>,
}

impl TokenMetrics {
    /// 创建新的 Token 指标
    pub fn new() -> Self {
        Self {
            tokens_issued: AtomicU64::new(0),
            validations_success: AtomicU64::new(0),
            validations_failed: AtomicU64::new(0),
            active_tokens: AtomicU64::new(0),
            tokens_by_scope_count: Mutex::new(HashMap::new()),
        }
    }

    /// 记录 Token 签发
    pub fn record_issued(&self, scope_count: usize) {
        self.tokens_issued.fetch_add(1, Ordering::Relaxed);
        self.active_tokens.fetch_add(1, Ordering::Relaxed);

        if let Ok(mut scopes) = self.tokens_by_scope_count.lock() {
            *scopes.entry(scope_count).or_insert(0) += 1;
        }
    }

    /// 记录 Token 验证
    pub fn record_validation(&self, success: bool) {
        if success {
            self.validations_success.fetch_add(1, Ordering::Relaxed);
        } else {
            self.validations_failed.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// 记录 Token 过期/撤销
    pub fn record_token_expired(&self) {
        let current = self.active_tokens.load(Ordering::Relaxed);
        if current > 0 {
            self.active_tokens.fetch_sub(1, Ordering::Relaxed);
        }
    }

    /// 获取 Token 使用率（活跃 Token 数）
    pub fn active_tokens(&self) -> u64 {
        self.active_tokens.load(Ordering::Relaxed)
    }

    /// 获取验证失败率
    pub fn validation_failure_rate(&self) -> f64 {
        let success = self.validations_success.load(Ordering::Relaxed) as f64;
        let failed = self.validations_failed.load(Ordering::Relaxed) as f64;
        let total = success + failed;

        if total == 0.0 {
            return 0.0;
        }

        (failed / total) * 100.0
    }

    /// 导出 Prometheus 格式
    pub fn export_prometheus(&self) -> String {
        let mut output = String::new();

        // Token 签发总数
        output.push_str("# HELP credbridge_tokens_issued_total Total tokens issued\n");
        output.push_str("# TYPE credbridge_tokens_issued_total counter\n");
        output.push_str(&format!(
            "credbridge_tokens_issued_total {}\n",
            self.tokens_issued.load(Ordering::Relaxed)
        ));

        // 活跃 Token 数
        output.push_str("\n# HELP credbridge_tokens_active Current active tokens\n");
        output.push_str("# TYPE credbridge_tokens_active gauge\n");
        output.push_str(&format!(
            "credbridge_tokens_active {}\n",
            self.active_tokens.load(Ordering::Relaxed)
        ));

        // Token 验证成功
        output.push_str(
            "\n# HELP credbridge_token_validations_success_total Successful token validations\n",
        );
        output.push_str("# TYPE credbridge_token_validations_success_total counter\n");
        output.push_str(&format!(
            "credbridge_token_validations_success_total {}\n",
            self.validations_success.load(Ordering::Relaxed)
        ));

        // Token 验证失败
        output.push_str(
            "\n# HELP credbridge_token_validations_failed_total Failed token validations\n",
        );
        output.push_str("# TYPE credbridge_token_validations_failed_total counter\n");
        output.push_str(&format!(
            "credbridge_token_validations_failed_total {}\n",
            self.validations_failed.load(Ordering::Relaxed)
        ));

        // 验证失败率
        output.push_str(
            "\n# HELP credbridge_token_validation_failure_rate Token validation failure rate\n",
        );
        output.push_str("# TYPE credbridge_token_validation_failure_rate gauge\n");
        output.push_str(&format!(
            "credbridge_token_validation_failure_rate {:.2}\n",
            self.validation_failure_rate()
        ));

        output
    }
}

/// TEE 性能指标
#[derive(Debug)]
pub struct TeeMetrics {
    /// 加密操作计数
    encryption_ops: AtomicU64,
    /// 解密操作计数
    decryption_ops: AtomicU64,
    /// 密钥派生操作计数
    key_derivation_ops: AtomicU64,
    /// 远程认证操作计数
    attestation_ops: AtomicU64,
    /// 操作延迟（毫秒）
    operation_latencies: Mutex<HashMap<String, Vec<u64>>>,
    /// EPC 内存使用率（百分比）
    epc_usage_percent: AtomicU64,
}

impl TeeMetrics {
    /// 创建新的 TEE 指标
    pub fn new() -> Self {
        Self {
            encryption_ops: AtomicU64::new(0),
            decryption_ops: AtomicU64::new(0),
            key_derivation_ops: AtomicU64::new(0),
            attestation_ops: AtomicU64::new(0),
            operation_latencies: Mutex::new(HashMap::new()),
            epc_usage_percent: AtomicU64::new(0),
        }
    }

    /// 记录 TEE 操作
    pub fn record_operation(&self, operation: &str, duration: Duration, _success: bool) {
        let ms = duration.as_millis() as u64;

        match operation {
            "encrypt" | "encryption" => {
                self.encryption_ops.fetch_add(1, Ordering::Relaxed);
            }
            "decrypt" | "decryption" => {
                self.decryption_ops.fetch_add(1, Ordering::Relaxed);
            }
            "derive" | "key_derivation" => {
                self.key_derivation_ops.fetch_add(1, Ordering::Relaxed);
            }
            "attest" | "attestation" => {
                self.attestation_ops.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }

        if let Ok(mut latencies) = self.operation_latencies.lock() {
            latencies.entry(operation.to_string()).or_default().push(ms);
        }
    }

    /// 设置 EPC 使用率
    pub fn set_epc_usage(&self, percent: f64) {
        self.epc_usage_percent
            .store((percent * 100.0) as u64, Ordering::Relaxed);
    }

    /// 获取平均操作延迟
    pub fn avg_operation_latency(&self, operation: &str) -> f64 {
        if let Ok(latencies) = self.operation_latencies.lock() {
            if let Some(times) = latencies.get(operation) {
                if times.is_empty() {
                    return 0.0;
                }
                let sum: u64 = times.iter().sum();
                return sum as f64 / times.len() as f64;
            }
        }
        0.0
    }

    /// 导出 Prometheus 格式
    pub fn export_prometheus(&self) -> String {
        let mut output = String::new();

        // 加密操作
        output.push_str("# HELP credbridge_tee_encryption_ops_total TEE encryption operations\n");
        output.push_str("# TYPE credbridge_tee_encryption_ops_total counter\n");
        output.push_str(&format!(
            "credbridge_tee_encryption_ops_total {}\n",
            self.encryption_ops.load(Ordering::Relaxed)
        ));

        // 解密操作
        output.push_str("\n# HELP credbridge_tee_decryption_ops_total TEE decryption operations\n");
        output.push_str("# TYPE credbridge_tee_decryption_ops_total counter\n");
        output.push_str(&format!(
            "credbridge_tee_decryption_ops_total {}\n",
            self.decryption_ops.load(Ordering::Relaxed)
        ));

        // 密钥派生
        output.push_str(
            "\n# HELP credbridge_tee_key_derivation_ops_total TEE key derivation operations\n",
        );
        output.push_str("# TYPE credbridge_tee_key_derivation_ops_total counter\n");
        output.push_str(&format!(
            "credbridge_tee_key_derivation_ops_total {}\n",
            self.key_derivation_ops.load(Ordering::Relaxed)
        ));

        // 远程认证
        output
            .push_str("\n# HELP credbridge_tee_attestation_ops_total TEE attestation operations\n");
        output.push_str("# TYPE credbridge_tee_attestation_ops_total counter\n");
        output.push_str(&format!(
            "credbridge_tee_attestation_ops_total {}\n",
            self.attestation_ops.load(Ordering::Relaxed)
        ));

        // EPC 使用率
        output.push_str(
            "\n# HELP credbridge_tee_epc_usage_percent TEE EPC memory usage percentage\n",
        );
        output.push_str("# TYPE credbridge_tee_epc_usage_percent gauge\n");
        output.push_str(&format!(
            "credbridge_tee_epc_usage_percent {:.2}\n",
            self.epc_usage_percent.load(Ordering::Relaxed) as f64 / 100.0
        ));

        // 操作延迟
        output.push_str(
            "\n# HELP credbridge_tee_operation_latency_ms TEE operation latency in milliseconds\n",
        );
        output.push_str("# TYPE credbridge_tee_operation_latency_ms gauge\n");
        if let Ok(latencies) = self.operation_latencies.lock() {
            for op in ["encrypt", "decrypt", "derive", "attest"] {
                let avg = if let Some(times) = latencies.get(op) {
                    if times.is_empty() {
                        0.0
                    } else {
                        times.iter().sum::<u64>() as f64 / times.len() as f64
                    }
                } else {
                    0.0
                };
                output.push_str(&format!(
                    "credbridge_tee_operation_latency_ms{{operation=\"{}\"}} {:.2}\n",
                    op, avg
                ));
            }
        }

        output
    }
}

/// 系统指标
#[derive(Debug)]
pub struct SystemMetrics {
    /// 启动时间
    start_time: SystemTime,
    /// CPU 使用率
    cpu_usage: AtomicU64,
    /// 内存使用率
    memory_usage: AtomicU64,
    /// 数据库连接池使用情况
    db_pool_usage: AtomicU64,
    /// Redis 连接池使用情况
    redis_pool_usage: AtomicU64,
}

impl SystemMetrics {
    /// 创建新的系统指标
    pub fn new() -> Self {
        Self {
            start_time: SystemTime::now(),
            cpu_usage: AtomicU64::new(0),
            memory_usage: AtomicU64::new(0),
            db_pool_usage: AtomicU64::new(0),
            redis_pool_usage: AtomicU64::new(0),
        }
    }

    /// 设置 CPU 使用率
    pub fn set_cpu_usage(&self, percent: f64) {
        self.cpu_usage
            .store((percent * 100.0) as u64, Ordering::Relaxed);
    }

    /// 设置内存使用率
    pub fn set_memory_usage(&self, percent: f64) {
        self.memory_usage
            .store((percent * 100.0) as u64, Ordering::Relaxed);
    }

    /// 设置数据库连接池使用率
    pub fn set_db_pool_usage(&self, percent: f64) {
        self.db_pool_usage
            .store((percent * 100.0) as u64, Ordering::Relaxed);
    }

    /// 设置 Redis 连接池使用率
    pub fn set_redis_pool_usage(&self, percent: f64) {
        self.redis_pool_usage
            .store((percent * 100.0) as u64, Ordering::Relaxed);
    }

    /// 获取运行时间（秒）
    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().unwrap_or_default().as_secs()
    }

    /// 导出 Prometheus 格式
    pub fn export_prometheus(&self) -> String {
        let mut output = String::new();

        // 运行时间
        output.push_str("# HELP credbridge_system_uptime_seconds System uptime in seconds\n");
        output.push_str("# TYPE credbridge_system_uptime_seconds gauge\n");
        output.push_str(&format!(
            "credbridge_system_uptime_seconds {}\n",
            self.uptime_seconds()
        ));

        // CPU 使用率
        output.push_str("\n# HELP credbridge_system_cpu_usage_percent CPU usage percentage\n");
        output.push_str("# TYPE credbridge_system_cpu_usage_percent gauge\n");
        output.push_str(&format!(
            "credbridge_system_cpu_usage_percent {:.2}\n",
            self.cpu_usage.load(Ordering::Relaxed) as f64 / 100.0
        ));

        // 内存使用率
        output
            .push_str("\n# HELP credbridge_system_memory_usage_percent Memory usage percentage\n");
        output.push_str("# TYPE credbridge_system_memory_usage_percent gauge\n");
        output.push_str(&format!(
            "credbridge_system_memory_usage_percent {:.2}\n",
            self.memory_usage.load(Ordering::Relaxed) as f64 / 100.0
        ));

        // 数据库连接池使用率
        output
            .push_str("\n# HELP credbridge_db_pool_usage_percent Database connection pool usage\n");
        output.push_str("# TYPE credbridge_db_pool_usage_percent gauge\n");
        output.push_str(&format!(
            "credbridge_db_pool_usage_percent {:.2}\n",
            self.db_pool_usage.load(Ordering::Relaxed) as f64 / 100.0
        ));

        // Redis 连接池使用率
        output
            .push_str("\n# HELP credbridge_redis_pool_usage_percent Redis connection pool usage\n");
        output.push_str("# TYPE credbridge_redis_pool_usage_percent gauge\n");
        output.push_str(&format!(
            "credbridge_redis_pool_usage_percent {:.2}\n",
            self.redis_pool_usage.load(Ordering::Relaxed) as f64 / 100.0
        ));

        output
    }
}

/// 告警指标
#[derive(Debug)]
pub struct AlertMetrics {
    /// 触发的告警总数
    alerts_triggered: AtomicU64,
    /// 按严重级别的告警计数
    alerts_by_severity: Mutex<HashMap<String, u64>>,
    /// 按类型的告警计数
    alerts_by_type: Mutex<HashMap<String, u64>>,
}

impl AlertMetrics {
    /// 创建新的告警指标
    pub fn new() -> Self {
        Self {
            alerts_triggered: AtomicU64::new(0),
            alerts_by_severity: Mutex::new(HashMap::new()),
            alerts_by_type: Mutex::new(HashMap::new()),
        }
    }

    /// 记录告警触发
    pub fn record_triggered(&self, severity: &str, alert_type: &str) {
        self.alerts_triggered.fetch_add(1, Ordering::Relaxed);

        if let Ok(mut severities) = self.alerts_by_severity.lock() {
            *severities.entry(severity.to_string()).or_insert(0) += 1;
        }

        if let Ok(mut types) = self.alerts_by_type.lock() {
            *types.entry(alert_type.to_string()).or_insert(0) += 1;
        }
    }

    /// 导出 Prometheus 格式
    pub fn export_prometheus(&self) -> String {
        let mut output = String::new();

        // 告警总数
        output.push_str("# HELP credbridge_alerts_triggered_total Total alerts triggered\n");
        output.push_str("# TYPE credbridge_alerts_triggered_total counter\n");
        output.push_str(&format!(
            "credbridge_alerts_triggered_total {}\n",
            self.alerts_triggered.load(Ordering::Relaxed)
        ));

        // 按严重级别
        output.push_str("\n# HELP credbridge_alerts_by_severity Alerts by severity\n");
        output.push_str("# TYPE credbridge_alerts_by_severity counter\n");
        if let Ok(severities) = self.alerts_by_severity.lock() {
            for (sev, count) in severities.iter() {
                output.push_str(&format!(
                    "credbridge_alerts_by_severity{{severity=\"{}\"}} {}\n",
                    sev, count
                ));
            }
        }

        // 按类型
        output.push_str("\n# HELP credbridge_alerts_by_type Alerts by type\n");
        output.push_str("# TYPE credbridge_alerts_by_type counter\n");
        if let Ok(types) = self.alerts_by_type.lock() {
            for (t, count) in types.iter() {
                output.push_str(&format!(
                    "credbridge_alerts_by_type{{type=\"{}\"}} {}\n",
                    t, count
                ));
            }
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collector_creation() {
        let collector = MetricsCollector::new();
        assert_eq!(collector.http_requests.total_requests(), 0);
        assert_eq!(collector.token_metrics.active_tokens(), 0);
    }

    #[test]
    fn test_http_request_metrics() {
        let metrics = HttpRequestMetrics::new();

        metrics.record("GET", "/test", 200, Duration::from_millis(50));
        metrics.record("POST", "/test", 500, Duration::from_millis(100));

        assert_eq!(metrics.total_requests(), 2);
        assert_eq!(metrics.requests_by_status(200), 1);
        assert_eq!(metrics.requests_by_status(500), 1);
        assert!(metrics.error_rate() > 0.0);
    }

    #[test]
    fn test_token_metrics() {
        let metrics = TokenMetrics::new();

        metrics.record_issued(3);
        metrics.record_issued(5);
        metrics.record_validation(true);
        metrics.record_validation(false);

        assert_eq!(metrics.active_tokens(), 2);
        assert!(metrics.validation_failure_rate() > 0.0);
    }

    #[test]
    fn test_prometheus_export() {
        let collector = MetricsCollector::new();

        collector.record_http_request("GET", "/test", 200, Duration::from_millis(50));
        collector.record_token_issued(3);
        collector.record_alert_triggered("high", "tee_error");

        let output = collector.export_prometheus();

        assert!(output.contains("credbridge_http_requests_total"));
        assert!(output.contains("credbridge_tokens_issued_total"));
        assert!(output.contains("credbridge_alerts_triggered_total"));
    }
}
