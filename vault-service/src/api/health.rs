//! 健康检查 API 模块
//!
//! 提供服务健康状态检查端点，包括：
//! - 数据库连接状态
//! - Redis 连接状态
//! - TEE 状态
//! - 各依赖服务状态

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::time::timeout;

/// 健康检查状态枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    /// 健康
    Healthy,
    /// 不健康
    Unhealthy,
    /// 降级（部分功能可用）
    Degraded,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Unhealthy => write!(f, "unhealthy"),
            HealthStatus::Degraded => write!(f, "degraded"),
        }
    }
}

/// 单个组件健康状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    /// 组件名称
    pub name: String,
    /// 组件状态
    pub status: HealthStatus,
    /// 响应时间（毫秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    /// 错误信息（如果不健康）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// 额外元数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// 健康检查响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    /// 整体状态
    pub status: HealthStatus,
    /// 服务名称
    pub service: String,
    /// 版本
    pub version: String,
    /// 当前时间戳（Unix 秒）
    pub timestamp: u64,
    /// 运行时间（秒）
    pub uptime_seconds: u64,
    /// 各组件状态
    pub components: Vec<ComponentHealth>,
}

/// 详细健康检查响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedHealthResponse {
    /// 基础健康信息
    #[serde(flatten)]
    pub basic: HealthResponse,
    /// 系统信息
    pub system: SystemInfo,
    /// TEE 详细信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tee_details: Option<TeeHealthDetails>,
}

/// 系统信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    /// CPU 使用率（百分比）
    pub cpu_usage_percent: f64,
    /// 内存使用率（百分比）
    pub memory_usage_percent: f64,
    /// 总内存（字节）
    pub memory_total_bytes: u64,
    /// 可用内存（字节）
    pub memory_available_bytes: u64,
}

/// TEE 健康详细信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeeHealthDetails {
    /// TEE 类型
    pub tee_type: String,
    /// Enclave 状态
    pub enclave_state: String,
    /// MRENCLAVE（如果是 SGX）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mrenclave: Option<String>,
    /// 是否已初始化
    pub initialized: bool,
    /// 最后认证时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_attestation_time: Option<u64>,
}

/// 健康检查配置
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// 服务名称
    pub service_name: String,
    /// 服务版本
    pub version: String,
    /// 健康检查超时
    pub check_timeout: Duration,
    /// 启动时间
    pub start_time: Instant,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            service_name: "credbridge-vault".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            check_timeout: Duration::from_secs(5),
            start_time: Instant::now(),
        }
    }
}

/// 健康检查状态共享状态
pub struct HealthState {
    /// 配置
    pub config: HealthConfig,
    /// 数据库连接检查函数
    pub db_health_check:
        Option<Arc<dyn Fn() -> BoxFuture<'static, Result<(), String>> + Send + Sync>>,
    /// Redis 连接检查函数
    pub redis_health_check:
        Option<Arc<dyn Fn() -> BoxFuture<'static, Result<(), String>> + Send + Sync>>,
    /// TEE 状态检查函数
    pub tee_health_check:
        Option<Arc<dyn Fn() -> BoxFuture<'static, Result<TeeHealthDetails, String>> + Send + Sync>>,
}

use std::future::Future;
use std::pin::Pin;

/// BoxFuture 类型别名
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

impl HealthState {
    /// 创建新的健康检查状态
    pub fn new(config: HealthConfig) -> Self {
        Self {
            config,
            db_health_check: None,
            redis_health_check: None,
            tee_health_check: None,
        }
    }

    /// 设置数据库健康检查
    pub fn with_db_check<F, Fut>(mut self, check: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        self.db_health_check = Some(Arc::new(move || Box::pin(check())));
        self
    }

    /// 设置 Redis 健康检查
    pub fn with_redis_check<F, Fut>(mut self, check: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        self.redis_health_check = Some(Arc::new(move || Box::pin(check())));
        self
    }

    /// 设置 TEE 健康检查
    pub fn with_tee_check<F, Fut>(mut self, check: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<TeeHealthDetails, String>> + Send + 'static,
    {
        self.tee_health_check = Some(Arc::new(move || Box::pin(check())));
        self
    }
}

/// 基础健康检查端点
pub async fn health_check(State(state): State<Arc<HealthState>>) -> impl IntoResponse {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let uptime = state.config.start_time.elapsed().as_secs();

    // 执行各组件健康检查
    let components = check_all_components(&state).await;

    // 确定整体状态
    let status = determine_overall_status(&components);

    let response = HealthResponse {
        status,
        service: state.config.service_name.clone(),
        version: state.config.version.clone(),
        timestamp,
        uptime_seconds: uptime,
        components,
    };

    let status_code = match response.status {
        HealthStatus::Healthy => StatusCode::OK,
        HealthStatus::Degraded => StatusCode::OK,
        HealthStatus::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
    };

    (status_code, Json(response))
}

/// 详细健康检查端点
pub async fn health_check_detail(State(state): State<Arc<HealthState>>) -> impl IntoResponse {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let uptime = state.config.start_time.elapsed().as_secs();
    let components = check_all_components(&state).await;
    let status = determine_overall_status(&components);

    // 获取系统信息
    let system = get_system_info().await;

    // 获取 TEE 详细信息
    let tee_details: Option<TeeHealthDetails> = if let Some(ref check) = state.tee_health_check {
        match timeout(state.config.check_timeout, check()).await {
            Ok(Ok(details)) => Some(details),
            Ok(Err(_)) => None,
            Err(_) => None,
        }
    } else {
        None
    };

    let basic = HealthResponse {
        status,
        service: state.config.service_name.clone(),
        version: state.config.version.clone(),
        timestamp,
        uptime_seconds: uptime,
        components,
    };

    let response = DetailedHealthResponse {
        basic,
        system,
        tee_details,
    };

    let status_code = match response.basic.status {
        HealthStatus::Healthy => StatusCode::OK,
        HealthStatus::Degraded => StatusCode::OK,
        HealthStatus::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
    };

    (status_code, Json(response))
}

/// 检查所有组件健康状态
async fn check_all_components(state: &HealthState) -> Vec<ComponentHealth> {
    let mut components = Vec::new();

    // 检查数据库
    if let Some(ref check) = state.db_health_check {
        let start = Instant::now();
        let result = timeout(state.config.check_timeout, (check)()).await;
        let latency = start.elapsed().as_millis() as u64;

        let (status, error) = match result {
            Ok(Ok(())) => (HealthStatus::Healthy, None::<String>),
            Ok(Err(e)) => (HealthStatus::Unhealthy, Some(e)),
            Err(_) => (
                HealthStatus::Unhealthy,
                Some("Health check timeout".to_string()),
            ),
        };

        components.push(ComponentHealth {
            name: "database".to_string(),
            status,
            latency_ms: Some(latency),
            error,
            metadata: None,
        });
    }

    // 检查 Redis
    if let Some(ref check) = state.redis_health_check {
        let start = Instant::now();
        let result = timeout(state.config.check_timeout, (check)()).await;
        let latency = start.elapsed().as_millis() as u64;

        let (status, error) = match result {
            Ok(Ok(())) => (HealthStatus::Healthy, None::<String>),
            Ok(Err(e)) => (HealthStatus::Unhealthy, Some(e)),
            Err(_) => (
                HealthStatus::Unhealthy,
                Some("Health check timeout".to_string()),
            ),
        };

        components.push(ComponentHealth {
            name: "redis".to_string(),
            status,
            latency_ms: Some(latency),
            error,
            metadata: None,
        });
    }

    // 检查 TEE
    if let Some(ref check) = state.tee_health_check {
        let start = Instant::now();
        let result = timeout(state.config.check_timeout, (check)()).await;
        let latency = start.elapsed().as_millis() as u64;

        let (status, error, metadata) = match result {
            Ok(Ok(details)) => {
                let metadata = serde_json::json!({
                    "tee_type": details.tee_type,
                    "enclave_state": details.enclave_state,
                    "initialized": details.initialized,
                });
                (HealthStatus::Healthy, None::<String>, Some(metadata))
            }
            Ok(Err(e)) => (HealthStatus::Unhealthy, Some(e), None),
            Err(_) => (
                HealthStatus::Unhealthy,
                Some("Health check timeout".to_string()),
                None,
            ),
        };

        components.push(ComponentHealth {
            name: "tee".to_string(),
            status,
            latency_ms: Some(latency),
            error,
            metadata,
        });
    }

    components
}

/// 确定整体健康状态
fn determine_overall_status(components: &[ComponentHealth]) -> HealthStatus {
    if components.is_empty() {
        return HealthStatus::Healthy;
    }

    let unhealthy_count = components
        .iter()
        .filter(|c| c.status == HealthStatus::Unhealthy)
        .count();

    if unhealthy_count == components.len() {
        HealthStatus::Unhealthy
    } else if unhealthy_count > 0 {
        HealthStatus::Degraded
    } else {
        HealthStatus::Healthy
    }
}

/// 获取系统信息
async fn get_system_info() -> SystemInfo {
    // 在实际实现中，这里应该调用系统 API 获取真实的系统信息
    // 这里使用模拟数据
    SystemInfo {
        cpu_usage_percent: 0.0,
        memory_usage_percent: 0.0,
        memory_total_bytes: 0,
        memory_available_bytes: 0,
    }
}

/// 模拟数据库健康检查（用于测试）
pub async fn mock_db_health_check() -> Result<(), String> {
    Ok(())
}

/// 模拟 Redis 健康检查（用于测试）
pub async fn mock_redis_health_check() -> Result<(), String> {
    Ok(())
}

/// 模拟 TEE 健康检查（用于测试）
pub async fn mock_tee_health_check() -> Result<TeeHealthDetails, String> {
    Ok(TeeHealthDetails {
        tee_type: "SGX".to_string(),
        enclave_state: "initialized".to_string(),
        mrenclave: Some("mock_mrenclave_123456".to_string()),
        initialized: true,
        last_attestation_time: Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_display() {
        assert_eq!(HealthStatus::Healthy.to_string(), "healthy");
        assert_eq!(HealthStatus::Unhealthy.to_string(), "unhealthy");
        assert_eq!(HealthStatus::Degraded.to_string(), "degraded");
    }

    #[test]
    fn test_determine_overall_status() {
        // 全部健康
        let components = vec![
            ComponentHealth {
                name: "db".to_string(),
                status: HealthStatus::Healthy,
                latency_ms: None,
                error: None,
                metadata: None,
            },
            ComponentHealth {
                name: "redis".to_string(),
                status: HealthStatus::Healthy,
                latency_ms: None,
                error: None,
                metadata: None,
            },
        ];
        assert_eq!(determine_overall_status(&components), HealthStatus::Healthy);

        // 部分不健康 -> 降级
        let components = vec![
            ComponentHealth {
                name: "db".to_string(),
                status: HealthStatus::Healthy,
                latency_ms: None,
                error: None,
                metadata: None,
            },
            ComponentHealth {
                name: "redis".to_string(),
                status: HealthStatus::Unhealthy,
                latency_ms: None,
                error: None,
                metadata: None,
            },
        ];
        assert_eq!(
            determine_overall_status(&components),
            HealthStatus::Degraded
        );

        // 全部不健康
        let components = vec![
            ComponentHealth {
                name: "db".to_string(),
                status: HealthStatus::Unhealthy,
                latency_ms: None,
                error: None,
                metadata: None,
            },
            ComponentHealth {
                name: "redis".to_string(),
                status: HealthStatus::Unhealthy,
                latency_ms: None,
                error: None,
                metadata: None,
            },
        ];
        assert_eq!(
            determine_overall_status(&components),
            HealthStatus::Unhealthy
        );
    }
}
