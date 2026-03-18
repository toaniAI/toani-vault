//! Connector 框架集成测试
//!
//! 测试内容：
//! - HTTPConnector 集成测试
//! - 超时控制测试
//! - 参数验证测试
//! - 错误处理测试
//! - 线程安全测试

use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use vault_service::connector::{
    Connector, ConnectorConfig, ConnectorError, ConnectorRegistry, ConnectorResult, HttpConnector,
    HttpConnectorConfig, NopConnector, TimeoutConfig, TimeoutWrapper, ValidatedParams,
    ValidationError, execute_with_timeout,
    validator::{SchemaValidator, ValidationRule},
};

// ============================================================================
// 测试辅助工具
// ============================================================================

/// 模拟 HTTP 连接器（用于测试，不实际发送请求）
struct MockHttpConnector {
    name: &'static str,
    response_delay: Duration,
    should_fail: bool,
    call_count: std::sync::atomic::AtomicUsize,
}

impl MockHttpConnector {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            response_delay: Duration::from_millis(10),
            should_fail: false,
            call_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn with_delay(mut self, delay: Duration) -> Self {
        self.response_delay = delay;
        self
    }

    fn with_failure(mut self) -> Self {
        self.should_fail = true;
        self
    }

    fn call_count(&self) -> usize {
        self.call_count.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait]
impl Connector for MockHttpConnector {
    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> &'static str {
        "Mock HTTP Connector for testing"
    }

    async fn init(&mut self, _config: ConnectorConfig) -> ConnectorResult<()> {
        if self.should_fail {
            return Err(ConnectorError::config("初始化失败"));
        }
        Ok(())
    }

    async fn validate(&self, params: &Value) -> Result<ValidatedParams, ValidationError> {
        // 验证必需字段
        if let Some(method) = params.get("method")
            && method.as_str().is_none()
        {
            return Err(ValidationError::field("method", "方法必须是字符串"));
        }
        Ok(ValidatedParams::new(params.clone()))
    }

    async fn execute(&self, params: ValidatedParams) -> ConnectorResult<Value> {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // 模拟延迟
        tokio::time::sleep(self.response_delay).await;

        if self.should_fail {
            return Err(ConnectorError::execution("执行失败"));
        }

        Ok(json!({
            "success": true,
            "connector": self.name,
            "params": params.raw()
        }))
    }

    async fn cleanup(&self) -> ConnectorResult<()> {
        Ok(())
    }

    fn timeout_seconds(&self) -> u64 {
        5
    }
}

/// 慢速连接器（用于超时测试）
struct SlowConnector {
    delay_secs: u64,
}

#[async_trait]
impl Connector for SlowConnector {
    fn name(&self) -> &'static str {
        "slow-connector"
    }

    fn description(&self) -> &'static str {
        "慢速连接器"
    }

    async fn init(&mut self, _config: ConnectorConfig) -> ConnectorResult<()> {
        Ok(())
    }

    async fn validate(&self, params: &Value) -> Result<ValidatedParams, ValidationError> {
        Ok(ValidatedParams::new(params.clone()))
    }

    async fn execute(&self, params: ValidatedParams) -> ConnectorResult<Value> {
        tokio::time::sleep(Duration::from_secs(self.delay_secs)).await;
        Ok(params.into_json())
    }

    async fn cleanup(&self) -> ConnectorResult<()> {
        Ok(())
    }
}

// ============================================================================
// 1. HTTPConnector 集成测试
// ============================================================================

#[tokio::test]
async fn test_http_connector_full_lifecycle() {
    let config = HttpConnectorConfig::new("https://api.example.com")
        .with_timeout(30)
        .with_header("Authorization", "Bearer test-token");

    let mut connector = HttpConnector::new("test-http", config);

    // 测试基础属性
    assert_eq!(connector.name(), "test-http");
    assert!(connector.description().contains("HTTP"));

    // 初始化
    let init_config = ConnectorConfig::new()
        .with_sync("base_url", "https://api.example.com")
        .with_sync("timeout_secs", 60u64);

    let result = connector.init(init_config).await;
    assert!(result.is_ok(), "初始化失败: {:?}", result);

    // 验证参数
    let params = json!({
        "method": "GET",
        "path": "/users"
    });
    let validated = connector.validate(&params).await;
    assert!(validated.is_ok());

    // 清理
    let cleanup_result = connector.cleanup().await;
    assert!(cleanup_result.is_ok());
}

#[tokio::test]
async fn test_http_connector_validation_errors() {
    let config = HttpConnectorConfig::new("https://api.example.com");
    let connector = HttpConnector::new("test-http", config);

    // 缺少 method
    let params = json!({"path": "/users"});
    let result = connector.validate(&params).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("method"));

    // 缺少 path
    let params = json!({"method": "GET"});
    let result = connector.validate(&params).await;
    assert!(result.is_err());

    // 无效的 HTTP 方法
    let params = json!({
        "method": "INVALID_METHOD",
        "path": "/users"
    });
    let result = connector.validate(&params).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("INVALID_METHOD") || err.to_string().contains("无效"));
}

#[tokio::test]
async fn test_http_connector_config_from_connector_config() {
    let config = ConnectorConfig::new()
        .with_sync("base_url", "https://api.test.com")
        .with_sync("timeout_secs", 45u64)
        .with_sync("connect_timeout_secs", 15u64)
        .with_sync("retry_count", 3u64)
        .with_sync(
            "headers",
            json!({
                "X-API-Key": "secret123",
                "Content-Type": "application/json"
            }),
        );

    let http_config = HttpConnectorConfig::from_connector_config(&config);
    assert!(http_config.is_ok());

    let http_config = http_config.unwrap();
    assert_eq!(http_config.base_url, "https://api.test.com");
    assert_eq!(http_config.timeout_secs, 45);
    assert_eq!(http_config.connect_timeout_secs, 15);
    assert_eq!(http_config.retry_count, 3);
    assert_eq!(
        http_config.default_headers.get("X-API-Key"),
        Some(&"secret123".to_string())
    );
}

#[tokio::test]
async fn test_http_connector_missing_base_url() {
    let config = ConnectorConfig::new();
    let http_config = HttpConnectorConfig::from_connector_config(&config);
    assert!(http_config.is_err());

    let err = http_config.unwrap_err();
    assert!(matches!(err, ConnectorError::Config { .. }));
    assert!(err.to_string().contains("base_url"));
}

// ============================================================================
// 2. 超时控制测试
// ============================================================================

#[tokio::test]
async fn test_connector_timeout_success() {
    let registry = Arc::new(ConnectorRegistry::new());

    // 注册快速连接器
    registry
        .register("fast", Box::new(MockHttpConnector::new("fast")))
        .await
        .unwrap();

    let connector = registry.get("fast").await.unwrap();
    let params = ValidatedParams::new(json!({"method": "GET"}));

    // 使用足够长的超时
    let result = execute_with_timeout::<Value>(&*connector, params, 5).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_connector_timeout_failure() {
    let registry = Arc::new(ConnectorRegistry::new());

    // 注册慢速连接器（2秒延迟）
    registry
        .register("slow", Box::new(SlowConnector { delay_secs: 2 }))
        .await
        .unwrap();

    let connector = registry.get("slow").await.unwrap();
    let params = ValidatedParams::new(json!({"method": "GET"}));

    // 使用很短的超时（100ms）
    let result = execute_with_timeout::<Value>(&*connector, params, 1).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(matches!(err, ConnectorError::Timeout { .. }));
}

#[tokio::test]
async fn test_timeout_wrapper_exact_timeout() {
    let config = TimeoutConfig::new()
        .with_default_timeout(1)
        .with_min_timeout(500);

    let wrapper = TimeoutWrapper::new("test-connector", config);

    // 刚好在超时内完成
    let result = wrapper
        .run(async {
            tokio::time::sleep(Duration::from_millis(500)).await;
            Ok::<_, ConnectorError>("success")
        })
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "success");
}

#[tokio::test]
async fn test_timeout_wrapper_exceeded() {
    let config = TimeoutConfig::new().with_default_timeout(1);
    let wrapper = TimeoutWrapper::new("test-connector", config);

    // 超过超时时间
    let result = wrapper
        .run(async {
            tokio::time::sleep(Duration::from_secs(3)).await;
            Ok::<_, ConnectorError>(42)
        })
        .await;

    assert!(result.is_err());
    match result {
        Err(ConnectorError::Timeout { .. }) => (),
        Err(e) => panic!("Expected Timeout error, got {:?}", e),
        Ok(_) => panic!("Expected error, got Ok"),
    }
}

// ============================================================================
// 3. 参数验证测试
// ============================================================================

#[tokio::test]
async fn test_validated_params_metadata() {
    let mut params = ValidatedParams::new(json!({
        "name": "test",
        "value": 123
    }));

    // 添加元数据
    params.set_metadata("validated_at", json!("2024-01-01T00:00:00Z"));
    params.set_metadata("validation_version", json!("1.0"));

    // 验证元数据
    assert_eq!(
        params.get_metadata("validated_at"),
        Some(&json!("2024-01-01T00:00:00Z"))
    );
    assert_eq!(
        params.get_metadata("validation_version"),
        Some(&json!("1.0"))
    );

    // 获取原始数据
    let raw = params.raw();
    assert_eq!(raw["name"], "test");
    assert_eq!(raw["value"], 123);
}

#[tokio::test]
async fn test_schema_validator_integration() {
    let schema = json!({
        "type": "object",
        "required": ["name", "email"],
        "properties": {
            "name": {
                "type": "string",
                "minLength": 2,
                "maxLength": 50
            },
            "email": {
                "type": "string",
                "pattern": r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$"
            },
            "age": {
                "type": "integer",
                "minimum": 0,
                "maximum": 150
            }
        }
    });

    let validator = SchemaValidator::new(schema);

    // 有效数据
    let valid_data = json!({
        "name": "John Doe",
        "email": "john@example.com",
        "age": 30
    });
    assert!(validator.validate(&valid_data).is_ok());

    // 缺少必需字段
    let invalid_data = json!({"name": "John"});
    let result = validator.validate(&invalid_data);
    assert!(result.is_err());

    // 字符串太短
    let invalid_data = json!({
        "name": "J",
        "email": "john@example.com"
    });
    let result = validator.validate(&invalid_data);
    assert!(result.is_err());

    // 无效的邮箱格式
    let invalid_data = json!({
        "name": "John",
        "email": "not-an-email"
    });
    let result = validator.validate(&invalid_data);
    assert!(result.is_err());

    // 年龄超出范围
    let invalid_data = json!({
        "name": "John",
        "email": "john@example.com",
        "age": 200
    });
    let result = validator.validate(&invalid_data);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_custom_validation_rule() {
    struct CustomRule;

    impl ValidationRule for CustomRule {
        fn name(&self) -> &'static str {
            "custom_rule"
        }

        fn validate(&self, value: &Value) -> Result<(), String> {
            if let Some(obj) = value.as_object()
                && let Some(val) = obj.get("custom_field")
                && val.as_str() == Some("valid")
            {
                return Ok(());
            }
            Err("custom_field 必须是 'valid'".to_string())
        }

        fn error_code(&self) -> Option<&'static str> {
            Some("CUSTOM_VALIDATION_FAILED")
        }
    }

    use vault_service::connector::validator::CompositeValidator;

    let validator = CompositeValidator::new()
        .with_schema(json!({
            "type": "object",
            "required": ["custom_field"]
        }))
        .with_rule(Box::new(CustomRule));

    // 有效数据
    let valid_data = json!({"custom_field": "valid"});
    assert!(validator.validate(&valid_data).is_ok());

    // 无效数据
    let invalid_data = json!({"custom_field": "invalid"});
    let result = validator.validate(&invalid_data);
    assert!(result.is_err());
}

// ============================================================================
// 4. 错误处理测试
// ============================================================================

#[tokio::test]
async fn test_connector_error_types() {
    // 验证错误
    let err = ConnectorError::validation("参数无效");
    assert!(matches!(err, ConnectorError::Validation { .. }));
    assert_eq!(err.error_code(), "VALIDATION_ERROR");

    // 超时错误
    let err = ConnectorError::timeout("my-connector", 30);
    assert!(matches!(err, ConnectorError::Timeout { .. }));
    assert_eq!(err.error_code(), "TIMEOUT_ERROR");
    assert!(err.to_string().contains("my-connector"));
    assert!(err.to_string().contains("30"));

    // 执行错误
    let err = ConnectorError::execution("执行失败");
    assert!(matches!(err, ConnectorError::Execution { .. }));
    assert_eq!(err.error_code(), "EXECUTION_ERROR");

    // 未找到错误
    let err = ConnectorError::not_found("missing-connector");
    assert!(matches!(err, ConnectorError::NotFound { .. }));
    assert_eq!(err.error_code(), "NOT_FOUND_ERROR");
    assert!(err.to_string().contains("missing-connector"));

    // 配置错误
    let err = ConnectorError::config("配置无效");
    assert!(matches!(err, ConnectorError::Config { .. }));
    assert_eq!(err.error_code(), "CONFIG_ERROR");

    // 重复注册错误
    let err = ConnectorError::duplicate("existing-connector");
    assert!(matches!(err, ConnectorError::DuplicateRegistration { .. }));
    assert_eq!(err.error_code(), "DUPLICATE_REGISTRATION_ERROR");

    // 内部错误
    let err = ConnectorError::internal("内部错误");
    assert!(matches!(err, ConnectorError::Internal { .. }));
    assert_eq!(err.error_code(), "INTERNAL_ERROR");
}

#[tokio::test]
async fn test_connector_error_clone() {
    let original = ConnectorError::validation_field("错误消息", "field_name");
    let cloned = original.clone();

    assert_eq!(original.error_code(), cloned.error_code());
    assert_eq!(original.field(), cloned.field());
}

#[tokio::test]
async fn test_validation_error_conversion() {
    let validation_err = ValidationError::field("email", "格式不正确");
    let connector_err = validation_err.into_connector_error();

    assert!(matches!(connector_err, ConnectorError::Validation { .. }));
    assert_eq!(connector_err.field(), Some("email"));
}

#[tokio::test]
async fn test_registry_error_handling() {
    let registry = ConnectorRegistry::new();

    // 获取不存在的连接器
    let result = registry.get("nonexistent").await;
    assert!(result.is_err());
    match result {
        Err(ConnectorError::NotFound { .. }) => (),
        Err(e) => panic!("Expected NotFound error, got {:?}", e),
        Ok(_) => panic!("Expected error, got Ok"),
    }

    // 重复注册
    registry
        .register("test", Box::new(NopConnector))
        .await
        .unwrap();
    let result = registry.register("test", Box::new(NopConnector)).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ConnectorError::DuplicateRegistration { .. }
    ));

    // 注销不存在的连接器
    let result = registry.unregister("nonexistent").await;
    assert!(result.is_err());
}

// ============================================================================
// 5. 线程安全测试
// ============================================================================

#[tokio::test]
async fn test_concurrent_registration() {
    let registry = Arc::new(ConnectorRegistry::new());
    let mut handles = vec![];

    // 并发注册 50 个连接器
    for i in 0..50 {
        let registry = Arc::clone(&registry);
        let handle = tokio::spawn(async move {
            let name = format!("connector-{}", i);
            let connector = Box::new(MockHttpConnector::new(Box::leak(
                name.clone().into_boxed_str(),
            )));
            registry.register(name, connector).await
        });
        handles.push(handle);
    }

    // 等待所有注册完成
    for handle in handles {
        assert!(handle.await.unwrap().is_ok());
    }

    // 验证所有连接器都已注册
    assert_eq!(registry.len().await, 50);

    // 验证可以并发获取
    let mut handles = vec![];
    for i in 0..50 {
        let registry = Arc::clone(&registry);
        let handle = tokio::spawn(async move {
            let name = format!("connector-{}", i);
            registry.get(&name).await.is_ok()
        });
        handles.push(handle);
    }

    for handle in handles {
        assert!(handle.await.unwrap());
    }
}

#[tokio::test]
async fn test_concurrent_execute() {
    let registry = Arc::new(ConnectorRegistry::new());
    let connector = MockHttpConnector::new("concurrent-test");

    registry
        .register("concurrent", Box::new(connector))
        .await
        .unwrap();

    let mut handles = vec![];

    // 并发执行 100 次
    for i in 0..100 {
        let registry = Arc::clone(&registry);
        let handle = tokio::spawn(async move {
            let connector = registry.get("concurrent").await.unwrap();
            let params = ValidatedParams::new(json!({"index": i}));
            connector.execute(params).await
        });
        handles.push(handle);
    }

    // 所有执行都应该成功
    for handle in handles {
        assert!(handle.await.unwrap().is_ok());
    }

    // 验证调用次数
    let _connector = registry.get("concurrent").await.unwrap();
    // 注意：由于 connector 是 Arc<dyn Connector>，我们无法直接访问 MockHttpConnector 的 call_count
    // 但这证明了并发执行不会 panic
}

#[tokio::test]
async fn test_concurrent_mixed_operations() {
    let registry = Arc::new(ConnectorRegistry::new());
    let mut handles = vec![];

    // 混合操作：注册、获取、列表
    for i in 0..20 {
        let registry = Arc::clone(&registry);
        let handle = tokio::spawn(async move {
            // 注册
            let name = format!("conn-{}", i);
            let connector = Box::new(NopConnector);
            let _ = registry.register(name.clone(), connector).await;

            // 获取
            let _ = registry.get(&name).await;

            // 列表
            let _ = registry.list_all().await;

            // 再次获取
            registry.get(&name).await.is_ok()
        });
        handles.push(handle);
    }

    for handle in handles {
        assert!(handle.await.unwrap());
    }
}

// ============================================================================
// 6. ConnectorConfig 测试
// ============================================================================

#[tokio::test]
async fn test_connector_config_operations() {
    let config = ConnectorConfig::new();

    // 添加各种类型
    let config = config
        .with_value("string_key", "string_value")
        .await
        .with_value("int_key", 42u64)
        .await
        .with_value("bool_key", true)
        .await
        .with_value("float_key", 3.14)
        .await
        .with_value("json_key", json!({"nested": "value"}))
        .await;

    // 验证获取
    assert_eq!(
        config.get_str("string_key").await,
        Some("string_value".to_string())
    );
    assert_eq!(config.get_u64("int_key").await, Some(42));
    assert_eq!(config.get_bool("bool_key").await, Some(true));
    assert_eq!(config.get_f64("float_key").await, Some(3.14));

    // 检查存在性
    assert!(config.contains("string_key").await);
    assert!(!config.contains("nonexistent").await);

    // 获取所有键
    let keys = config.keys().await;
    assert_eq!(keys.len(), 5);

    // 非空检查
    assert!(!config.is_empty().await);
}

#[tokio::test]
async fn test_connector_config_sync_operations() {
    let config = ConnectorConfig::new()
        .with_sync("key1", "value1")
        .with_sync("key2", 123u64)
        .with_sync("key3", true);

    assert_eq!(config.get_str_sync("key1"), Some("value1".to_string()));
    assert_eq!(config.get_u64_sync("key2"), Some(123));
    assert_eq!(config.get_bool_sync("key3"), Some(true));
}

// ============================================================================
// 7. 注册表构建器测试
// ============================================================================

#[tokio::test]
async fn test_registry_builder() {
    use vault_service::connector::ConnectorRegistryBuilder;

    let builder = ConnectorRegistryBuilder::new()
        .add("conn-a", Box::new(NopConnector))
        .add("conn-b", Box::new(NopConnector))
        .add("conn-c", Box::new(NopConnector));

    let registry = builder.build().await;

    assert_eq!(registry.len().await, 3);
    assert!(registry.contains("conn-a").await);
    assert!(registry.contains("conn-b").await);
    assert!(registry.contains("conn-c").await);
}

#[tokio::test]
async fn test_registry_builder_register_all() {
    use vault_service::connector::ConnectorRegistryBuilder;

    let target = ConnectorRegistry::new();

    let builder = ConnectorRegistryBuilder::new()
        .add("x", Box::new(NopConnector))
        .add("y", Box::new(NopConnector));

    let result = builder.register_all(&target).await;
    assert!(result.is_ok());

    assert_eq!(target.len().await, 2);
}

// ============================================================================
// 8. 便捷函数测试
// ============================================================================

#[tokio::test]
async fn test_global_registry_helpers() {
    use vault_service::connector::{get, list_all, register};

    // 注册
    let result = register("helper-test", Box::new(NopConnector)).await;
    // 注意：如果测试并行运行，可能已经存在
    if let Err(ref e) = result {
        assert!(matches!(e, ConnectorError::DuplicateRegistration { .. }));
    }

    // 获取
    let result = get("helper-test").await;
    assert!(result.is_ok());

    // 列表
    let list = list_all().await;
    assert!(!list.is_empty());
}

// ============================================================================
// 9. TimeoutConfig 边界测试
// ============================================================================

#[tokio::test]
async fn test_timeout_config_boundaries() {
    let config = TimeoutConfig::new()
        .with_default_timeout(30)
        .with_max_timeout(60)
        .with_min_timeout(1000);

    // 测试边界值
    assert_eq!(
        config.effective_timeout(Some(Duration::from_secs(0))),
        Duration::from_secs(30) // 使用默认值
    );

    assert_eq!(
        config.effective_timeout(Some(Duration::from_secs(100))),
        Duration::from_secs(60) // 限制为最大值
    );

    assert_eq!(
        config.effective_timeout(Some(Duration::from_millis(500))),
        Duration::from_millis(1000) // 限制为最小值
    );

    // 验证超时值
    let result = config.validate_timeout(Duration::from_secs(30));
    assert!(result.is_ok());

    let result = config.validate_timeout(Duration::from_secs(100));
    assert!(result.is_err());
}

#[tokio::test]
async fn test_disabled_timeout() {
    let config = TimeoutConfig::new().disabled();
    assert!(!config.enabled);

    // 禁用超时时，任何超时值都应该是有效的
    let result = config.validate_timeout(Duration::from_secs(1000));
    assert!(result.is_ok());
}

// ============================================================================
// 10. 完整的集成场景测试
// ============================================================================

#[tokio::test]
async fn test_full_integration_scenario() {
    // 创建新的注册表
    let registry = ConnectorRegistry::new();

    // 1. 注册 HTTP 连接器
    let http_config = HttpConnectorConfig::new("https://api.example.com")
        .with_timeout(30)
        .with_header("X-API-Version", "v1");
    let mut http_connector = HttpConnector::new("api-client", http_config);

    let init_config = ConnectorConfig::new()
        .with_sync("base_url", "https://api.example.com")
        .with_sync("timeout_secs", 30u64);

    http_connector.init(init_config).await.unwrap();

    registry
        .register("http-api", Box::new(http_connector))
        .await
        .unwrap();

    // 2. 注册另一个连接器
    registry
        .register("mock-service", Box::new(MockHttpConnector::new("mock")))
        .await
        .unwrap();

    // 3. 列出所有连接器
    let connectors = registry.list_all_with_description().await;
    assert_eq!(connectors.len(), 2);

    // 4. 获取并使用连接器
    let connector = registry.get("mock-service").await.unwrap();
    let params = ValidatedParams::new(json!({
        "method": "POST",
        "path": "/test",
        "body": {"key": "value"}
    }));

    let result = connector.execute(params).await;
    assert!(result.is_ok());

    // 5. 验证执行结果
    let response = result.unwrap();
    assert_eq!(response["success"], true);
    assert_eq!(response["connector"], "mock");

    // 6. 清理
    let http_connector = registry.get("http-api").await.unwrap();
    assert!(http_connector.cleanup().await.is_ok());

    registry.unregister("http-api").await.unwrap();
    registry.unregister("mock-service").await.unwrap();

    assert!(registry.is_empty().await);
}
