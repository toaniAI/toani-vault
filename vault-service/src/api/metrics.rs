//! Prometheus 指标端点
//!
//! 提供 `/metrics` 端点，返回 Prometheus 格式的指标数据

use axum::{
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use std::sync::Arc;

use crate::metrics::MetricsCollector;

/// 指标状态
pub struct MetricsState {
    /// 指标收集器
    pub collector: Arc<MetricsCollector>,
}

impl MetricsState {
    /// 创建新的指标状态
    pub fn new(collector: Arc<MetricsCollector>) -> Self {
        Self { collector }
    }
}

/// Prometheus 指标端点
///
/// 返回所有收集的指标，格式为 Prometheus exposition format
pub async fn metrics_endpoint(State(state): State<Arc<MetricsState>>) -> impl IntoResponse {
    let metrics_output = state.collector.export_prometheus();

    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )
        .body(metrics_output)
        .unwrap()
}

/// 带身份验证的指标端点中间件
///
/// 在生产环境中，指标端点应该受到保护，防止未授权访问
#[derive(Clone)]
pub struct MetricsAuthConfig {
    /// 是否启用身份验证
    pub enabled: bool,
    /// Bearer Token（简单身份验证）
    pub bearer_token: Option<String>,
    /// 允许的网络（IP 白名单）
    pub allowed_networks: Vec<String>,
}

impl Default for MetricsAuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bearer_token: None,
            allowed_networks: vec!["127.0.0.1".to_string(), "::1".to_string()],
        }
    }
}

/// 验证指标访问权限
pub fn validate_metrics_access(
    auth_header: Option<&str>,
    client_ip: &str,
    config: &MetricsAuthConfig,
) -> bool {
    // 如果身份验证未启用，允许所有访问
    if !config.enabled {
        return true;
    }

    // 检查 IP 白名单
    if config.allowed_networks.contains(&client_ip.to_string()) {
        return true;
    }

    // 检查 Bearer Token
    if let Some(expected_token) = &config.bearer_token {
        if let Some(header_value) = auth_header {
            let expected = format!("Bearer {}", expected_token);
            if header_value == expected {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_auth_disabled() {
        let config = MetricsAuthConfig {
            enabled: false,
            bearer_token: Some("secret".to_string()),
            allowed_networks: vec![],
        };

        // 当身份验证禁用时，任何请求都应该通过
        assert!(validate_metrics_access(None, "192.168.1.1", &config));
        assert!(validate_metrics_access(
            Some("invalid"),
            "192.168.1.1",
            &config
        ));
    }

    #[test]
    fn test_metrics_auth_by_ip() {
        let config = MetricsAuthConfig {
            enabled: true,
            bearer_token: None,
            allowed_networks: vec!["127.0.0.1".to_string()],
        };

        assert!(validate_metrics_access(None, "127.0.0.1", &config));
        assert!(!validate_metrics_access(None, "192.168.1.1", &config));
    }

    #[test]
    fn test_metrics_auth_by_token() {
        let config = MetricsAuthConfig {
            enabled: true,
            bearer_token: Some("secret_token".to_string()),
            allowed_networks: vec![],
        };

        assert!(validate_metrics_access(
            Some("Bearer secret_token"),
            "192.168.1.1",
            &config
        ));
        assert!(!validate_metrics_access(
            Some("Bearer wrong_token"),
            "192.168.1.1",
            &config
        ));
        assert!(!validate_metrics_access(None, "192.168.1.1", &config));
    }
}
