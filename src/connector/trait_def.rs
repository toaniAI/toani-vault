//! Connector Trait 定义
//!
//! 定义 Connector 的核心接口和生命周期方法。

use crate::connector::error::ConnectorError;
use crate::connector::error::{ConnectorResult, ValidationError};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

// ============================================================================
// Connector Trait
// ============================================================================

/// Connector trait - 所有连接器的基础接口
///
/// # 生命周期
///
/// Connector 的生命周期如下：
///
/// 1. **创建**: 创建 Connector 实例
/// 2. **初始化**: 调用 `init()` 建立连接、加载配置
/// 3. **验证**: 调用 `validate()` 验证输入参数
/// 4. **执行**: 调用 `execute()` 执行核心逻辑
/// 5. **清理**: 调用 `cleanup()` 释放资源
///
/// # 线程安全
///
/// 所有 Connector 实现必须是 `Send + Sync`，以支持并发执行。
#[async_trait]
pub trait Connector: Send + Sync {
    /// 连接器唯一标识符
    fn name(&self) -> &'static str;

    /// 连接器描述
    fn description(&self) -> &'static str;

    /// 初始化连接器
    async fn init(&mut self, config: ConnectorConfig) -> ConnectorResult<()>;

    /// 验证参数
    async fn validate(&self, params: &Value) -> Result<ValidatedParams, ValidationError>;

    /// 执行连接器操作
    async fn execute(&self, params: ValidatedParams) -> ConnectorResult<Value>;

    /// 清理资源
    async fn cleanup(&self) -> ConnectorResult<()>;

    /// 获取超时配置（秒）
    fn timeout_seconds(&self) -> u64 {
        30
    }

    /// 是否支持并发执行
    fn supports_concurrent(&self) -> bool {
        true
    }

    /// 获取连接器元数据
    fn metadata(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }
}

// ============================================================================
// ConnectorConfig
// ============================================================================

/// 连接器配置
#[derive(Debug, Clone, Default)]
pub struct ConnectorConfig {
    values: Arc<Mutex<HashMap<String, Value>>>,
}

impl ConnectorConfig {
    /// 创建空配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 从 HashMap 创建配置
    pub fn from_map(map: HashMap<String, Value>) -> Self {
        Self {
            values: Arc::new(Mutex::new(map)),
        }
    }

    /// 添加配置项（异步版本）
    pub async fn with_value<T: Into<Value>>(self, key: impl Into<String>, value: T) -> Self {
        let values = Arc::clone(&self.values);
        let key = key.into();
        let value = value.into();
        values.lock().await.insert(key, value);
        self
    }

    /// 同步版本：添加配置项
    pub fn with_sync<T: Into<Value>>(self, key: impl Into<String>, value: T) -> Self {
        {
            let mut values = futures::executor::block_on(self.values.lock());
            values.insert(key.into(), value.into());
            drop(values); // 显式释放锁
        }
        self
    }

    /// 获取值
    pub async fn get(&self, key: &str) -> Option<Value> {
        self.values.lock().await.get(key).cloned()
    }

    /// 获取字符串值
    pub async fn get_str(&self, key: &str) -> Option<String> {
        self.values
            .lock()
            .await
            .get(key)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
    }

    /// 获取整数值
    pub async fn get_u64(&self, key: &str) -> Option<u64> {
        self.values.lock().await.get(key).and_then(|v| v.as_u64())
    }

    /// 获取布尔值
    pub async fn get_bool(&self, key: &str) -> Option<bool> {
        self.values.lock().await.get(key).and_then(|v| v.as_bool())
    }

    /// 获取浮点值
    pub async fn get_f64(&self, key: &str) -> Option<f64> {
        self.values.lock().await.get(key).and_then(|v| v.as_f64())
    }

    /// 检查是否包含键
    pub async fn contains(&self, key: &str) -> bool {
        self.values.lock().await.contains_key(key)
    }

    /// 获取所有键
    pub async fn keys(&self) -> Vec<String> {
        self.values.lock().await.keys().cloned().collect()
    }

    /// 检查是否为空
    pub async fn is_empty(&self) -> bool {
        self.values.lock().await.is_empty()
    }

    /// 同步版本：获取值
    pub fn get_sync(&self, key: &str) -> Option<Value> {
        futures::executor::block_on(self.get(key))
    }

    /// 同步版本：获取字符串值
    pub fn get_str_sync(&self, key: &str) -> Option<String> {
        futures::executor::block_on(self.get_str(key))
    }

    /// 同步版本：获取整数值
    pub fn get_u64_sync(&self, key: &str) -> Option<u64> {
        futures::executor::block_on(self.get_u64(key))
    }

    /// 同步版本：获取布尔值
    pub fn get_bool_sync(&self, key: &str) -> Option<bool> {
        futures::executor::block_on(self.get_bool(key))
    }
}

// ============================================================================
// ValidatedParams
// ============================================================================

/// 验证后的参数
#[derive(Debug, Clone)]
pub struct ValidatedParams {
    raw: Value,
    metadata: HashMap<String, Value>,
}

impl ValidatedParams {
    /// 创建验证后的参数
    pub fn new(raw: Value) -> Self {
        Self {
            raw,
            metadata: HashMap::new(),
        }
    }

    /// 创建带元数据的验证参数
    pub fn with_metadata(raw: Value, metadata: HashMap<String, Value>) -> Self {
        Self { raw, metadata }
    }

    /// 获取原始参数值
    pub fn raw(&self) -> &Value {
        &self.raw
    }

    /// 获取元数据
    pub fn metadata(&self) -> &HashMap<String, Value> {
        &self.metadata
    }

    /// 设置元数据
    pub fn set_metadata(&mut self, key: impl Into<String>, value: Value) {
        self.metadata.insert(key.into(), value);
    }

    /// 获取元数据值
    pub fn get_metadata(&self, key: &str) -> Option<&Value> {
        self.metadata.get(key)
    }

    /// 转换为 JSON 值
    pub fn into_json(self) -> Value {
        self.raw
    }
}

// ============================================================================
// 超时执行辅助函数
// ============================================================================

/// 带超时执行包装器
pub async fn execute_with_timeout<T>(
    connector: &dyn Connector,
    params: ValidatedParams,
    timeout_secs: u64,
) -> Result<T, ConnectorError>
where
    T: serde::de::DeserializeOwned + Send + Sync + 'static,
{
    use tokio::time::{Duration, timeout};

    let result = timeout(Duration::from_secs(timeout_secs), connector.execute(params))
        .await
        .map_err(|_| ConnectorError::timeout(connector.name(), timeout_secs))?;

    match result {
        Ok(v) => serde_json::from_value(v)
            .map_err(|e| ConnectorError::execution_with_source("反序列化失败", e)),
        Err(e) => Err(e),
    }
}

// ============================================================================
// 空实现（用于测试和占位）
// ============================================================================

/// 空 Connector 实现
pub struct NopConnector;

#[async_trait]
impl Connector for NopConnector {
    fn name(&self) -> &'static str {
        "nop"
    }

    fn description(&self) -> &'static str {
        "空连接器（用于测试）"
    }

    async fn init(&mut self, _config: ConnectorConfig) -> ConnectorResult<()> {
        Ok(())
    }

    async fn validate(&self, params: &Value) -> Result<ValidatedParams, ValidationError> {
        Ok(ValidatedParams::new(params.clone()))
    }

    async fn execute(&self, params: ValidatedParams) -> ConnectorResult<Value> {
        Ok(params.into_json())
    }

    async fn cleanup(&self) -> ConnectorResult<()> {
        Ok(())
    }
}

impl Default for NopConnector {
    fn default() -> Self {
        Self
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connector_config() {
        let config = ConnectorConfig::new();
        let config = config.with_value("host", "localhost").await;
        let config = config.with_value("port", 8080u64).await;
        let config = config.with_value("enabled", true).await;

        assert_eq!(config.get_str_sync("host"), Some("localhost".to_string()));
        assert_eq!(config.get_u64_sync("port"), Some(8080));
        assert_eq!(config.get_bool_sync("enabled"), Some(true));
    }

    #[tokio::test]
    async fn test_validated_params() {
        let params = ValidatedParams::new(serde_json::json!({
            "key": "value"
        }));

        assert_eq!(params.raw()["key"].as_str(), Some("value"));

        let mut params = params;
        params.set_metadata("validated_at", "2024-01-01".into());
        assert_eq!(
            params.get_metadata("validated_at"),
            Some(&Value::String("2024-01-01".to_string()))
        );
    }

    #[tokio::test]
    async fn test_nop_connector() {
        let mut connector = NopConnector;

        assert_eq!(connector.name(), "nop");

        let config = ConnectorConfig::new();
        assert!(connector.init(config).await.is_ok());

        let params = serde_json::json!({ "test": "value" });
        let validated = connector.validate(&params).await.unwrap();
        let result = connector.execute(validated).await.unwrap();

        assert_eq!(result["test"].as_str(), Some("value"));
        assert!(connector.cleanup().await.is_ok());
    }
}
