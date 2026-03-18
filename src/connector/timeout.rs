//! 超时控制模块
//!
//! 提供可配置的超时控制机制，确保连接器执行不会无限期阻塞。

use crate::connector::error::{ConnectorError, ConnectorResult};
use std::time::Duration;
use tokio::time::{Instant, timeout};

// ============================================================================
// TimeoutConfig
// ============================================================================

/// 超时配置
#[derive(Clone, Debug)]
pub struct TimeoutConfig {
    /// 默认超时时间（秒）
    pub default_timeout: Duration,
    /// 最大允许超时（秒）
    pub max_timeout: Duration,
    /// 最小允许超时（秒）
    pub min_timeout: Duration,
    /// 是否启用超时
    pub enabled: bool,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_secs(30),
            max_timeout: Duration::from_secs(300),
            min_timeout: Duration::from_millis(100),
            enabled: true,
        }
    }
}

impl TimeoutConfig {
    /// 创建新的超时配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置默认超时
    pub fn with_default_timeout(mut self, secs: u64) -> Self {
        self.default_timeout = Duration::from_secs(secs);
        self
    }

    /// 设置最大超时
    pub fn with_max_timeout(mut self, secs: u64) -> Self {
        self.max_timeout = Duration::from_secs(secs);
        self
    }

    /// 设置最小超时
    pub fn with_min_timeout(mut self, millis: u64) -> Self {
        self.min_timeout = Duration::from_millis(millis);
        self
    }

    /// 禁用超时
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// 验证超时值是否在允许范围内
    pub fn validate_timeout(&self, timeout: Duration) -> ConnectorResult<Duration> {
        if !self.enabled {
            return Ok(timeout);
        }

        if timeout < self.min_timeout {
            return Err(ConnectorError::config(format!(
                "超时时间 {}ms 小于最小允许值 {}ms",
                timeout.as_millis(),
                self.min_timeout.as_millis()
            )));
        }

        if timeout > self.max_timeout {
            return Err(ConnectorError::config(format!(
                "超时时间 {}s 大于最大允许值 {}s",
                timeout.as_secs(),
                self.max_timeout.as_secs()
            )));
        }

        Ok(timeout)
    }

    /// 获取有效的超时时间
    ///
    /// 如果传入的超时为 0 或未设置，则使用默认超时
    pub fn effective_timeout(&self, requested: Option<Duration>) -> Duration {
        match requested {
            Some(d) if d > Duration::ZERO => {
                if d > self.max_timeout {
                    self.max_timeout
                } else if d < self.min_timeout {
                    self.min_timeout
                } else {
                    d
                }
            }
            _ => self.default_timeout,
        }
    }
}

// ============================================================================
// TimeoutError
// ============================================================================

/// 超时错误
#[derive(Debug, Clone)]
pub struct TimeoutError {
    /// 连接器名称
    pub connector_name: String,
    /// 超时时间
    pub timeout: Duration,
    /// 已执行时间
    pub elapsed: Duration,
}

impl std::fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "连接器 '{}' 执行超时：配置超时 {:?}，已执行 {:?}",
            self.connector_name, self.timeout, self.elapsed
        )
    }
}

impl std::error::Error for TimeoutError {}

impl From<TimeoutError> for ConnectorError {
    fn from(err: TimeoutError) -> Self {
        ConnectorError::timeout(err.connector_name, err.timeout.as_secs())
    }
}

// ============================================================================
// TimeoutWrapper
// ============================================================================

/// 超时包装器
///
/// 用于包装异步操作并应用超时控制。
///
/// # 示例
///
/// ```rust,no_run
/// use credbridge::connector::timeout::{TimeoutWrapper, TimeoutConfig};
/// use std::time::Duration;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let config = TimeoutConfig::new()
///         .with_default_timeout(30);
///
///     let wrapper = TimeoutWrapper::new("my-connector", config);
///
///     let result = wrapper.run(async {
///         // 执行耗时操作
///         tokio::time::sleep(Duration::from_secs(5)).await;
///         Ok::<_, ConnectorError>(42)
///     }).await?;
///
///     Ok(())
/// }
/// ```
pub struct TimeoutWrapper {
    /// 连接器名称
    connector_name: String,
    /// 超时配置
    config: TimeoutConfig,
    /// 自定义超时（覆盖配置中的默认值）
    custom_timeout: Option<Duration>,
}

impl TimeoutWrapper {
    /// 创建新的超时包装器
    pub fn new(connector_name: impl Into<String>, config: TimeoutConfig) -> Self {
        Self {
            connector_name: connector_name.into(),
            config,
            custom_timeout: None,
        }
    }

    /// 设置自定义超时
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.custom_timeout = Some(timeout);
        self
    }

    /// 获取有效超时时间
    pub fn effective_timeout(&self) -> Duration {
        self.config.effective_timeout(self.custom_timeout)
    }

    /// 运行带超时的异步操作
    pub async fn run<F, T, E>(&self, future: F) -> Result<T, ConnectorError>
    where
        F: std::future::Future<Output = Result<T, E>>,
        E: Into<ConnectorError> + Send + 'static,
    {
        if !self.config.enabled {
            return future.await.map_err(|e| e.into());
        }

        let timeout_duration = self.effective_timeout();
        let start = Instant::now();

        match timeout(timeout_duration, future).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(e)) => Err(e.into()),
            Err(_) => Err(TimeoutError {
                connector_name: self.connector_name.clone(),
                timeout: timeout_duration,
                elapsed: start.elapsed(),
            }
            .into()),
        }
    }

    /// 运行带超时的异步操作（返回原始错误类型）
    pub async fn run_raw<F, T, E>(&self, future: F) -> Result<Result<T, E>, TimeoutError>
    where
        F: std::future::Future<Output = Result<T, E>>,
    {
        if !self.config.enabled {
            return Ok(future.await);
        }

        let timeout_duration = self.effective_timeout();
        let start = Instant::now();

        match timeout(timeout_duration, future).await {
            Ok(result) => Ok(result),
            Err(_) => Err(TimeoutError {
                connector_name: self.connector_name.clone(),
                timeout: timeout_duration,
                elapsed: start.elapsed(),
            }),
        }
    }
}

// ============================================================================
// 便捷函数
// ============================================================================

/// 带超时执行异步操作
///
/// # 示例
///
/// ```rust,no_run
/// use credbridge::connector::timeout::with_timeout;
/// use std::time::Duration;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let result = with_timeout(
///         "my-connector",
///         Duration::from_secs(30),
///         async {
///             // 执行耗时操作
///             Ok::<_, ConnectorError>(42)
///         }
///     ).await?;
///
///     Ok(())
/// }
/// ```
pub async fn with_timeout<F, T, E>(
    connector_name: &str,
    timeout_duration: Duration,
    future: F,
) -> Result<T, ConnectorError>
where
    F: std::future::Future<Output = Result<T, E>>,
    E: Into<ConnectorError> + Send + 'static,
{
    let start = Instant::now();

    match timeout(timeout_duration, future).await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(e)) => Err(e.into()),
        Err(_) => Err(TimeoutError {
            connector_name: connector_name.to_string(),
            timeout: timeout_duration,
            elapsed: start.elapsed(),
        }
        .into()),
    }
}

/// 带超时执行异步操作（简单版本，只返回 Ok/Err）
pub async fn with_timeout_simple<F, T>(timeout_duration: Duration, future: F) -> ConnectorResult<T>
where
    F: std::future::Future<Output = ConnectorResult<T>>,
{
    let _start = Instant::now();

    match timeout(timeout_duration, future).await {
        Ok(result) => result,
        Err(_) => Err(ConnectorError::timeout(
            "unknown",
            timeout_duration.as_secs(),
        )),
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_timeout_config() {
        let config = TimeoutConfig::new()
            .with_default_timeout(60)
            .with_max_timeout(300)
            .with_min_timeout(500);

        assert_eq!(config.default_timeout, Duration::from_secs(60));
        assert_eq!(config.max_timeout, Duration::from_secs(300));
        assert_eq!(config.min_timeout, Duration::from_millis(500));
        assert!(config.enabled);
    }

    #[test]
    fn test_timeout_config_disabled() {
        let config = TimeoutConfig::new().disabled();
        assert!(!config.enabled);
    }

    #[test]
    fn test_effective_timeout() {
        let config = TimeoutConfig::new()
            .with_default_timeout(30)
            .with_max_timeout(60)
            .with_min_timeout(1000);

        // 未设置，使用默认值
        assert_eq!(config.effective_timeout(None), Duration::from_secs(30));

        // 设置值在范围内
        assert_eq!(
            config.effective_timeout(Some(Duration::from_secs(45))),
            Duration::from_secs(45)
        );

        // 超过最大值
        assert_eq!(
            config.effective_timeout(Some(Duration::from_secs(100))),
            Duration::from_secs(60)
        );

        // 小于最小值
        assert_eq!(
            config.effective_timeout(Some(Duration::from_millis(500))),
            Duration::from_millis(1000)
        );
    }

    #[tokio::test]
    async fn test_timeout_wrapper_success() {
        let config = TimeoutConfig::new().with_default_timeout(5);
        let wrapper = TimeoutWrapper::new("test", config);

        let result: ConnectorResult<i32> = wrapper.run(async { Ok::<_, ConnectorError>(42) }).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_timeout_wrapper_timeout() {
        let config = TimeoutConfig::new().with_default_timeout(1);
        let wrapper = TimeoutWrapper::new("test", config);

        let result: ConnectorResult<i32> = wrapper
            .run(async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                Ok::<_, ConnectorError>(42)
            })
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ConnectorError::Timeout { .. }));
    }

    #[tokio::test]
    async fn test_timeout_wrapper_disabled() {
        let config = TimeoutConfig::new().disabled();
        let wrapper = TimeoutWrapper::new("test", config);

        // 即使操作很慢，也不会超时
        let result: ConnectorResult<i32> = wrapper
            .run(async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok::<_, ConnectorError>(42)
            })
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_with_timeout_function() {
        // 成功情况
        let result = with_timeout("test", Duration::from_secs(5), async {
            Ok::<_, ConnectorError>(42)
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);

        // 超时情况
        let result = with_timeout("test", Duration::from_millis(100), async {
            tokio::time::sleep(Duration::from_secs(5)).await;
            Ok::<_, ConnectorError>(42)
        })
        .await;

        assert!(result.is_err());
    }
}
