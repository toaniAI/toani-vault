# CredBridge 监控与告警指南

本文档介绍 CredBridge Vault Service 的监控与告警系统的配置和使用方法。

## 目录

- [概述](#概述)
- [健康检查](#健康检查)
- [Prometheus 指标](#prometheus-指标)
- [告警系统](#告警系统)
- [Grafana 仪表板](#grafana-仪表板)
- [配置示例](#配置示例)

## 概述

CredBridge Vault Service 内置了完整的监控与告警系统，包括：

- **健康检查端点**：检查数据库、Redis、TEE 等组件状态
- **Prometheus 指标**：导出请求数、延迟、错误率、Token 使用率等指标
- **告警系统**：基于规则触发告警，支持 Webhook 通知

## 健康检查

### 端点

| 端点                 | 描述                                   | 状态码    |
| -------------------- | -------------------------------------- | --------- |
| `GET /health`        | 进程存活检查（liveness）               | 200       |
| `GET /ready`         | 服务就绪检查（readiness）              | 200 / 503 |
| `GET /health/detail` | 详细就绪检查（包含启动自检与系统信息） | 200 / 503 |

### 响应格式

#### 基础健康检查 (`/health`)

```json
{
  "status": "alive",
  "service": "credbridge-vault",
  "version": "0.1.0",
  "message": "CredBridge service is running"
}
```

#### 服务就绪检查 (`/ready` 或 `/health/detail`)

```json
{
  "status": "healthy",
  "service": "credbridge-vault",
  "version": "0.1.0",
  "timestamp": 1710123456,
  "uptime_seconds": 3600,
  "components": [...],
  "system": {
    "cpu_usage_percent": 25.5,
    "memory_usage_percent": 45.0,
    "memory_total_bytes": 8589934592,
    "memory_available_bytes": 4724464025
  },
  "tee_details": {
    "tee_type": "SGX",
    "enclave_state": "initialized",
    "mrenclave": "abc123...",
    "initialized": true,
    "last_attestation_time": 1710123400
  }
}
```

### 状态说明

| 状态        | 含义                                   | HTTP 状态码 |
| ----------- | -------------------------------------- | ----------- |
| `alive`     | 仅表示进程仍在运行                     | 200         |
| `healthy`   | 启动自检与关键依赖满足，对外可提供服务 | 200         |
| `degraded`  | 非关键项异常，但当前仍允许服务         | 200         |
| `unhealthy` | 关键自检或依赖失败，服务未就绪         | 503         |

### Probe 建议

- Kubernetes `livenessProbe` 应指向 `/health`
- Kubernetes `readinessProbe` 应指向 `/ready`
- `/health/detail` 适合人工排障与运维系统采样，不建议替代 liveness probe

## Prometheus 指标

### 端点

```
GET /metrics
Content-Type: text/plain; version=0.0.4; charset=utf-8
```

### HTTP 请求指标

| 指标名                                    | 类型      | 描述                                                       |
| ----------------------------------------- | --------- | ---------------------------------------------------------- |
| `credbridge_http_requests_total`          | Counter   | HTTP 请求总数                                              |
| `credbridge_http_requests_by_status`      | Counter   | 按状态码的请求数                                           |
| `credbridge_http_request_duration_bucket` | Histogram | 请求延迟分布（桶：10ms, 50ms, 100ms, 500ms, 1000ms, +Inf） |
| `credbridge_http_request_duration_sum`    | Histogram | 总延迟                                                     |
| `credbridge_http_request_duration_count`  | Histogram | 延迟样本数                                                 |
| `credbridge_http_error_rate_percentage`   | Gauge     | HTTP 错误率百分比                                          |

### Token 指标

| 指标名                                       | 类型    | 描述                   |
| -------------------------------------------- | ------- | ---------------------- |
| `credbridge_tokens_issued_total`             | Counter | 签发 Token 总数        |
| `credbridge_tokens_active`                   | Gauge   | 当前活跃 Token 数      |
| `credbridge_token_validations_success_total` | Counter | Token 验证成功次数     |
| `credbridge_token_validations_failed_total`  | Counter | Token 验证失败次数     |
| `credbridge_token_validation_failure_rate`   | Gauge   | Token 验证失败率百分比 |

### TEE 性能指标

| 指标名                                    | 类型    | 描述                 |
| ----------------------------------------- | ------- | -------------------- |
| `credbridge_tee_encryption_ops_total`     | Counter | TEE 加密操作数       |
| `credbridge_tee_decryption_ops_total`     | Counter | TEE 解密操作数       |
| `credbridge_tee_key_derivation_ops_total` | Counter | TEE 密钥派生操作数   |
| `credbridge_tee_attestation_ops_total`    | Counter | TEE 远程认证操作数   |
| `credbridge_tee_epc_usage_percent`        | Gauge   | EPC 内存使用率百分比 |
| `credbridge_tee_operation_latency_ms`     | Gauge   | TEE 操作延迟（毫秒） |

### 系统指标

| 指标名                                   | 类型  | 描述                     |
| ---------------------------------------- | ----- | ------------------------ |
| `credbridge_system_uptime_seconds`       | Gauge | 系统运行时间（秒）       |
| `credbridge_system_cpu_usage_percent`    | Gauge | CPU 使用率百分比         |
| `credbridge_system_memory_usage_percent` | Gauge | 内存使用率百分比         |
| `credbridge_db_pool_usage_percent`       | Gauge | 数据库连接池使用率百分比 |
| `credbridge_redis_pool_usage_percent`    | Gauge | Redis 连接池使用率百分比 |

### 告警指标

| 指标名                              | 类型    | 描述               |
| ----------------------------------- | ------- | ------------------ |
| `credbridge_alerts_triggered_total` | Counter | 触发告警总数       |
| `credbridge_alerts_by_severity`     | Counter | 按严重级别的告警数 |
| `credbridge_alerts_by_type`         | Counter | 按类型的告警数     |

## 告警系统

### 告警规则

默认内置的告警规则：

| 规则名称                        | 严重级别 | 触发条件               | 持续时间 | 建议操作          |
| ------------------------------- | -------- | ---------------------- | -------- | ----------------- |
| `high_error_rate`               | Warning  | 错误率 > 5%            | 3 次连续 | 检查服务日志      |
| `tee_anomaly`                   | Critical | TEE 错误数 ≥ 1         | 1 次     | 检查 Enclave 状态 |
| `high_memory_usage`             | Warning  | 内存使用率 > 85%       | 5 次连续 | 考虑扩容或重启    |
| `high_token_validation_failure` | Warning  | Token 验证失败率 > 10% | 3 次连续 | 检查 Token 密钥   |

### 告警严重级别

| 级别       | 颜色    | 说明                   |
| ---------- | ------- | ---------------------- |
| `info`     | 🔵 蓝色 | 信息性告警，无需处理   |
| `warning`  | 🟡 黄色 | 警告，需要注意但不紧急 |
| `critical` | 🔴 红色 | 严重问题，需要立即处理 |

### Webhook 通知

支持通过 Webhook 发送告警通知到外部系统（如 Slack、PagerDuty、自定义 API）。

#### Webhook 配置

```rust
use vault_service::alerting::WebhookConfig;

let webhook = WebhookConfig {
    url: "https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXX".to_string(),
    method: "POST".to_string(),
    headers: {
        let mut h = std::collections::HashMap::new();
        h.insert("Content-Type".to_string(), "application/json".to_string());
        h
    },
    timeout_seconds: 10,
    retries: 3,
    severity_filter: vec![AlertSeverity::Warning, AlertSeverity::Critical],
};
```

#### Webhook 请求格式

```json
{
  "id": "ALERT-high_error_rate-abc123",
  "severity": "warning",
  "alert_type": "high_error_rate",
  "title": "[warning] high_error_rate",
  "description": "HTTP error rate is 10.50%, exceeding threshold of 5.00%",
  "timestamp": 1710123456,
  "metrics": {
    "error_rate_percent": 10.5,
    "threshold": 5.0
  },
  "component": "credbridge-vault",
  "suggested_action": "Check service logs and investigate error sources"
}
```

## Grafana 仪表板

### 推荐的 Prometheus 查询

#### 1. 请求速率（每分钟）

```promql
rate(credbridge_http_requests_total[1m])
```

#### 2. 错误率趋势

```promql
credbridge_http_error_rate_percentage
```

#### 3. P99 延迟

```promql
histogram_quantile(0.99,
  sum(rate(credbridge_http_request_duration_bucket[5m])) by (le)
)
```

#### 4. 活跃 Token 数

```promql
credbridge_tokens_active
```

#### 5. TEE 操作速率

```promql
sum(rate(credbridge_tee_encryption_ops_total[5m])) +
sum(rate(credbridge_tee_decryption_ops_total[5m]))
```

#### 6. 告警触发速率

```promql
rate(credbridge_alerts_triggered_total[1h])
```

### 仪表板配置示例

```json
{
  "dashboard": {
    "title": "CredBridge Vault Monitoring",
    "panels": [
      {
        "title": "Request Rate",
        "targets": [
          {
            "expr": "rate(credbridge_http_requests_total[1m])"
          }
        ]
      },
      {
        "title": "Error Rate",
        "targets": [
          {
            "expr": "credbridge_http_error_rate_percentage"
          }
        ],
        "alert": {
          "conditions": [
            {
              "evaluator": { "params": [5], "type": "gt" }
            }
          ]
        }
      },
      {
        "title": "Active Tokens",
        "targets": [
          {
            "expr": "credbridge_tokens_active"
          }
        ]
      }
    ]
  }
}
```

## 配置示例

### 基础配置

```rust
use vault_service::{
    metrics::MetricsCollector,
    api::{HealthState, HealthConfig},
    alerting::{AlertManager, AlertManagerConfig, WebhookConfig},
};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() {
    // 创建指标收集器
    let metrics = Arc::new(MetricsCollector::new());

    // 配置健康检查
    let health_config = HealthConfig::default();
    let health_state = Arc::new(HealthState::new(health_config)
        .with_db_check(db_health_check)
        .with_redis_check(redis_health_check)
        .with_tee_check(tee_health_check));

    // 配置告警管理器
    let alert_config = AlertManagerConfig {
        suppress_window_seconds: 300, // 5 分钟抑制窗口
        max_history_size: 1000,
        ..Default::default()
    };
    let (alert_manager, alert_rx) = AlertManager::new(alert_config);

    // 启动告警处理器
    tokio::spawn(run_alert_processor(
        alert_rx,
        Arc::new(WebhookNotifier::new(vec![webhook_config])),
    ));

    // 构建 Axum 路由
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/health/detail", get(health_check_detail))
        .route("/metrics", get(metrics_endpoint))
        .layer(Extension(metrics.clone()));
}
```

### Kubernetes 健康检查配置

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: credbridge-vault
spec:
  template:
    spec:
      containers:
        - name: vault
          image: credbridge/vault-service:latest
          livenessProbe:
            httpGet:
              path: /health
              port: 8080
            initialDelaySeconds: 10
            periodSeconds: 30
          readinessProbe:
            httpGet:
              path: /health
              port: 8080
            initialDelaySeconds: 5
            periodSeconds: 10
```

### Prometheus 抓取配置

```yaml
scrape_configs:
  - job_name: "credbridge-vault"
    static_configs:
      - targets: ["credbridge-vault:8080"]
    metrics_path: /metrics
    scrape_interval: 15s
    scrape_timeout: 10s
```

### Alertmanager 规则

```yaml
groups:
  - name: credbridge
    rules:
      - alert: CredBridgeHighErrorRate
        expr: credbridge_http_error_rate_percentage > 5
        for: 3m
        labels:
          severity: warning
        annotations:
          summary: "CredBridge error rate is high"
          description: "Error rate is {{ $value }}% for more than 3 minutes"

      - alert: CredBridgeTeeAnomaly
        expr: credbridge_alerts_by_severity{severity="critical"} > 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "CredBridge TEE anomaly detected"
          description: "Critical TEE anomaly has been detected"

      - alert: CredBridgeHighMemoryUsage
        expr: credbridge_system_memory_usage_percent > 85
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "CredBridge memory usage is high"
          description: "Memory usage is {{ $value }}%"
```

## 故障排查

### 常见问题

#### 1. 健康检查返回 503

检查各个组件状态：

- 数据库连接是否正常
- Redis 连接是否正常
- TEE Enclave 是否已初始化

#### 2. Prometheus 指标为空

确保：

- 服务已接收过请求
- 指标收集器已正确配置到路由

#### 3. Webhook 通知未送达

检查：

- Webhook URL 是否正确
- 网络连接是否正常
- 告警严重级别是否在过滤器中

## 参考

- [Prometheus 官方文档](https://prometheus.io/docs/)
- [Grafana 官方文档](https://grafana.com/docs/)
- [Vault Service README](../vault-service/README.md)
