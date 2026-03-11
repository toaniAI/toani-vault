//! 告警系统模块
//!
//! 提供告警检测、触发和通知功能：
//! - 错误率超过阈值告警
//! - TEE 异常告警
//! - Webhook 通知支持
//! - 告警日志记录

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// 告警严重级别
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum AlertSeverity {
    /// 信息
    Info,
    /// 警告
    Warning,
    /// 严重
    Critical,
}

impl std::fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertSeverity::Info => write!(f, "info"),
            AlertSeverity::Warning => write!(f, "warning"),
            AlertSeverity::Critical => write!(f, "critical"),
        }
    }
}

/// 告警类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AlertType {
    /// 高错误率
    HighErrorRate,
    /// TEE 异常
    TeeAnomaly,
    /// 数据库连接失败
    DatabaseConnectionFailed,
    /// Redis 连接失败
    RedisConnectionFailed,
    /// 内存使用率过高
    HighMemoryUsage,
    /// CPU 使用率过高
    HighCpuUsage,
    /// Token 验证失败率过高
    HighTokenValidationFailure,
    /// 远程认证失败
    AttestationFailed,
    /// 自定义告警
    Custom(String),
}

impl std::fmt::Display for AlertType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertType::HighErrorRate => write!(f, "high_error_rate"),
            AlertType::TeeAnomaly => write!(f, "tee_anomaly"),
            AlertType::DatabaseConnectionFailed => write!(f, "database_connection_failed"),
            AlertType::RedisConnectionFailed => write!(f, "redis_connection_failed"),
            AlertType::HighMemoryUsage => write!(f, "high_memory_usage"),
            AlertType::HighCpuUsage => write!(f, "high_cpu_usage"),
            AlertType::HighTokenValidationFailure => write!(f, "high_token_validation_failure"),
            AlertType::AttestationFailed => write!(f, "attestation_failed"),
            AlertType::Custom(s) => write!(f, "custom_{}", s),
        }
    }
}

/// 告警事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertEvent {
    /// 告警 ID
    pub id: String,
    /// 告警严重级别
    pub severity: AlertSeverity,
    /// 告警类型
    pub alert_type: AlertType,
    /// 告警标题
    pub title: String,
    /// 告警描述
    pub description: String,
    /// 触发时间戳（Unix 秒）
    pub timestamp: u64,
    /// 相关指标数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<HashMap<String, f64>>,
    /// 关联的服务或组件
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    /// 建议的修复操作
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_action: Option<String>,
}

impl AlertEvent {
    /// 创建新的告警事件
    pub fn new(
        severity: AlertSeverity,
        alert_type: AlertType,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let id = format!(
            "ALERT-{}-{}",
            alert_type,
            uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("unknown")
        );

        Self {
            id,
            severity,
            alert_type,
            title: title.into(),
            description: description.into(),
            timestamp,
            metrics: None,
            component: None,
            suggested_action: None,
        }
    }

    /// 添加指标数据
    pub fn with_metrics(mut self, metrics: HashMap<String, f64>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// 添加组件信息
    pub fn with_component(mut self, component: impl Into<String>) -> Self {
        self.component = Some(component.into());
        self
    }

    /// 添加建议操作
    pub fn with_suggested_action(mut self, action: impl Into<String>) -> Self {
        self.suggested_action = Some(action.into());
        self
    }
}

/// Webhook 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// Webhook URL
    pub url: String,
    /// HTTP 方法
    #[serde(default = "default_method")]
    pub method: String,
    /// 请求头
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// 超时时间（秒）
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    /// 重试次数
    #[serde(default = "default_retries")]
    pub retries: u32,
    /// 只发送特定严重级别的告警
    #[serde(default)]
    pub severity_filter: Vec<AlertSeverity>,
}

fn default_method() -> String {
    "POST".to_string()
}

fn default_timeout() -> u64 {
    10
}

fn default_retries() -> u32 {
    3
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            method: default_method(),
            headers: HashMap::new(),
            timeout_seconds: default_timeout(),
            retries: default_retries(),
            severity_filter: vec![],
        }
    }
}

/// 告警规则配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    /// 规则名称
    pub name: String,
    /// 告警类型
    pub alert_type: AlertType,
    /// 严重级别
    pub severity: AlertSeverity,
    /// 触发条件（指标名称）
    pub metric_name: String,
    /// 触发阈值
    pub threshold: f64,
    /// 比较操作符（>, <, >=, <=, ==）
    pub operator: String,
    /// 持续时间（连续超过阈值的次数）
    #[serde(default = "default_duration")]
    pub duration: u32,
    /// 告警描述模板
    pub description_template: String,
    /// 建议操作
    #[serde(default)]
    pub suggested_action: Option<String>,
    /// 是否启用
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_duration() -> u32 {
    1
}

fn default_enabled() -> bool {
    true
}

/// 告警管理器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertManagerConfig {
    /// 默认告警规则
    #[serde(default = "default_rules")]
    pub rules: Vec<AlertRule>,
    /// Webhook 配置列表
    #[serde(default)]
    pub webhooks: Vec<WebhookConfig>,
    /// 抑制重复告警的时间窗口（秒）
    #[serde(default = "default_suppress_window")]
    pub suppress_window_seconds: u64,
    /// 最大告警历史记录数
    #[serde(default = "default_max_history")]
    pub max_history_size: usize,
}

fn default_rules() -> Vec<AlertRule> {
    vec![
        AlertRule {
            name: "high_error_rate".to_string(),
            alert_type: AlertType::HighErrorRate,
            severity: AlertSeverity::Warning,
            metric_name: "error_rate_percent".to_string(),
            threshold: 5.0,
            operator: ">".to_string(),
            duration: 3,
            description_template: "HTTP error rate is {value:.2}%, exceeding threshold of {threshold:.2}%".to_string(),
            suggested_action: Some("Check service logs and investigate error sources".to_string()),
            enabled: true,
        },
        AlertRule {
            name: "tee_anomaly".to_string(),
            alert_type: AlertType::TeeAnomaly,
            severity: AlertSeverity::Critical,
            metric_name: "tee_error_count".to_string(),
            threshold: 1.0,
            operator: ">=".to_string(),
            duration: 1,
            description_template: "TEE anomaly detected: {value} errors".to_string(),
            suggested_action: Some("Check TEE enclave status and consider restart".to_string()),
            enabled: true,
        },
        AlertRule {
            name: "high_memory_usage".to_string(),
            alert_type: AlertType::HighMemoryUsage,
            severity: AlertSeverity::Warning,
            metric_name: "memory_usage_percent".to_string(),
            threshold: 85.0,
            operator: ">".to_string(),
            duration: 5,
            description_template: "Memory usage is {value:.2}%, exceeding threshold of {threshold:.2}%".to_string(),
            suggested_action: Some("Consider scaling or restart service".to_string()),
            enabled: true,
        },
        AlertRule {
            name: "high_token_validation_failure".to_string(),
            alert_type: AlertType::HighTokenValidationFailure,
            severity: AlertSeverity::Warning,
            metric_name: "token_validation_failure_rate".to_string(),
            threshold: 10.0,
            operator: ">".to_string(),
            duration: 3,
            description_template: "Token validation failure rate is {value:.2}%, exceeding threshold of {threshold:.2}%".to_string(),
            suggested_action: Some("Check token signing key and Redis connection".to_string()),
            enabled: true,
        },
    ]
}

fn default_suppress_window() -> u64 {
    300 // 5 分钟
}

fn default_max_history() -> usize {
    1000
}

impl Default for AlertManagerConfig {
    fn default() -> Self {
        Self {
            rules: default_rules(),
            webhooks: vec![],
            suppress_window_seconds: default_suppress_window(),
            max_history_size: default_max_history(),
        }
    }
}

/// 告警管理器
pub struct AlertManager {
    /// 配置
    config: AlertManagerConfig,
    /// 告警历史
    alert_history: Mutex<Vec<AlertEvent>>,
    /// 规则触发计数（用于检测持续时间）
    rule_trigger_counts: Mutex<HashMap<String, u32>>,
    /// 上次告警时间（用于抑制重复告警）
    last_alert_times: Mutex<HashMap<String, u64>>,
    /// 告警计数
    alert_counter: AtomicU64,
    /// 告警发送通道
    alert_tx: mpsc::Sender<AlertEvent>,
}

impl AlertManager {
    /// 创建新的告警管理器
    pub fn new(config: AlertManagerConfig) -> (Self, mpsc::Receiver<AlertEvent>) {
        let (alert_tx, alert_rx) = mpsc::channel(100);

        let manager = Self {
            config,
            alert_history: Mutex::new(Vec::new()),
            rule_trigger_counts: Mutex::new(HashMap::new()),
            last_alert_times: Mutex::new(HashMap::new()),
            alert_counter: AtomicU64::new(0),
            alert_tx,
        };

        (manager, alert_rx)
    }

    /// 评估指标并触发告警
    pub fn evaluate_metric(&self, metric_name: &str, value: f64) {
        for rule in &self.config.rules {
            if !rule.enabled || rule.metric_name != metric_name {
                continue;
            }

            let triggered = match rule.operator.as_str() {
                ">" => value > rule.threshold,
                ">=" => value >= rule.threshold,
                "<" => value < rule.threshold,
                "<=" => value <= rule.threshold,
                "==" => (value - rule.threshold).abs() < f64::EPSILON,
                _ => {
                    warn!("Unknown operator: {}", rule.operator);
                    continue;
                }
            };

            if triggered {
                self.handle_rule_trigger(rule, value);
            } else {
                // 重置触发计数
                if let Ok(mut counts) = self.rule_trigger_counts.lock() {
                    counts.remove(&rule.name);
                }
            }
        }
    }

    /// 处理规则触发
    fn handle_rule_trigger(&self, rule: &AlertRule, value: f64) {
        // 增加触发计数
        let count = {
            let mut counts = self.rule_trigger_counts.lock().unwrap();
            let count = counts.entry(rule.name.clone()).or_insert(0);
            *count += 1;
            *count
        };

        // 检查是否达到持续时间要求
        if count < rule.duration {
            return;
        }

        // 检查是否需要抑制（重复告警）
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        {
            let last_times = self.last_alert_times.lock().unwrap();
            if let Some(last_time) = last_times.get(&rule.name) {
                if now - last_time < self.config.suppress_window_seconds {
                    return; // 抑制告警
                }
            }
        }

        // 创建告警事件
        let description = rule
            .description_template
            .replace("{value}", &format!("{:.2}", value))
            .replace("{threshold}", &format!("{:.2}", rule.threshold));

        let mut alert = AlertEvent::new(
            rule.severity.clone(),
            rule.alert_type.clone(),
            format!("[{}] {}", rule.severity, rule.name),
            description,
        )
        .with_component("credbridge-vault")
        .with_metrics({
            let mut metrics = HashMap::new();
            metrics.insert(rule.metric_name.clone(), value);
            metrics.insert("threshold".to_string(), rule.threshold);
            metrics
        });

        if let Some(ref action) = rule.suggested_action {
            alert = alert.with_suggested_action(action.clone());
        }

        // 发送告警
        self.send_alert(alert);

        // 更新上次告警时间
        {
            let mut last_times = self.last_alert_times.lock().unwrap();
            last_times.insert(rule.name.clone(), now);
        }

        // 重置触发计数
        {
            let mut counts = self.rule_trigger_counts.lock().unwrap();
            counts.remove(&rule.name);
        }
    }

    /// 发送告警
    fn send_alert(&self, alert: AlertEvent) {
        // 记录告警日志
        match alert.severity {
            AlertSeverity::Info => {
                info!(
                    alert_id = %alert.id,
                    alert_type = %alert.alert_type,
                    "Alert triggered: {}",
                    alert.title
                );
            }
            AlertSeverity::Warning => {
                warn!(
                    alert_id = %alert.id,
                    alert_type = %alert.alert_type,
                    "Alert triggered: {}",
                    alert.title
                );
            }
            AlertSeverity::Critical => {
                error!(
                    alert_id = %alert.id,
                    alert_type = %alert.alert_type,
                    "Alert triggered: {}",
                    alert.title
                );
            }
        }

        // 添加到历史
        {
            let mut history = self.alert_history.lock().unwrap();
            history.push(alert.clone());
            if history.len() > self.config.max_history_size {
                history.remove(0);
            }
        }

        // 增加计数
        self.alert_counter.fetch_add(1, Ordering::Relaxed);

        // 发送到处理通道
        let _ = self.alert_tx.try_send(alert);
    }

    /// 手动触发告警
    pub fn trigger_alert(&self, alert: AlertEvent) {
        self.send_alert(alert);
    }

    /// 获取告警历史
    pub fn get_alert_history(&self) -> Vec<AlertEvent> {
        self.alert_history.lock().unwrap().clone()
    }

    /// 获取告警计数
    pub fn get_alert_count(&self) -> u64 {
        self.alert_counter.load(Ordering::Relaxed)
    }

    /// 获取配置
    pub fn config(&self) -> &AlertManagerConfig {
        &self.config
    }
}

/// Webhook 通知器
pub struct WebhookNotifier {
    /// HTTP 客户端
    client: reqwest::Client,
    /// Webhook 配置
    config: Vec<WebhookConfig>,
}

impl WebhookNotifier {
    /// 创建新的 Webhook 通知器
    pub fn new(config: Vec<WebhookConfig>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        Self { client, config }
    }

    /// 发送告警到 Webhook
    pub async fn send_alert(&self, alert: &AlertEvent) {
        for webhook in &self.config {
            // 检查严重级别过滤器
            if !webhook.severity_filter.is_empty()
                && !webhook.severity_filter.contains(&alert.severity)
            {
                continue;
            }

            // 发送请求
            let mut attempt = 0;
            while attempt <= webhook.retries {
                match self.send_to_webhook(webhook, alert).await {
                    Ok(_) => {
                        info!(
                            "Alert sent to webhook: {}, alert_id: {}",
                            webhook.url, alert.id
                        );
                        break;
                    }
                    Err(e) => {
                        attempt += 1;
                        if attempt > webhook.retries {
                            error!(
                                "Failed to send alert to webhook {} after {} attempts: {}",
                                webhook.url, webhook.retries + 1, e
                            );
                        } else {
                            tokio::time::sleep(Duration::from_millis(100 * attempt as u64)).await;
                        }
                    }
                }
            }
        }
    }

    /// 发送单个 Webhook 请求
    async fn send_to_webhook(
        &self,
        config: &WebhookConfig,
        alert: &AlertEvent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let timeout = Duration::from_secs(config.timeout_seconds);

        let mut request = self
            .client
            .request(
                config.method.parse()?,
                reqwest::Url::parse(&config.url)?,
            )
            .timeout(timeout)
            .json(alert);

        // 添加自定义请求头
        for (key, value) in &config.headers {
            request = request.header(key, value);
        }

        let response = request.send().await?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!("Webhook returned status: {}", response.status()).into())
        }
    }
}

/// 告警处理工作流
pub async fn run_alert_processor(
    mut alert_rx: mpsc::Receiver<AlertEvent>,
    webhook_notifier: Arc<WebhookNotifier>,
) {
    while let Some(alert) = alert_rx.recv().await {
        // 发送 Webhook 通知
        webhook_notifier.send_alert(&alert).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn test_alert_event_builder() {
        let metrics: HashMap<String, f64> = [("error_rate".to_string(), 10.5)].into_iter().collect();

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
        assert_eq!(
            AlertType::Custom("my_alert".to_string()).to_string(),
            "custom_my_alert"
        );
    }

    #[test]
    fn test_default_rules() {
        let rules = default_rules();
        assert!(!rules.is_empty());

        let high_error_rate = rules
            .iter()
            .find(|r| r.name == "high_error_rate")
            .expect("high_error_rate rule should exist");

        assert_eq!(high_error_rate.threshold, 5.0);
        assert_eq!(high_error_rate.severity, AlertSeverity::Warning);
        assert!(high_error_rate.enabled);
    }
}
