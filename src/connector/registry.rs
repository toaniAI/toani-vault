//! Connector 注册表
//!
//! 提供线程安全的全局连接器注册表，支持动态注册、注销和查询。

use crate::connector::{Connector, ConnectorError, ConnectorResult};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// ConnectorRegistry
// ============================================================================

/// Connector 注册表
///
/// 全局线程安全的连接器注册表，支持：
/// - 动态注册/注销连接器
/// - 按名称查询连接器
/// - 列出所有已注册的连接器
///
/// # 线程安全
///
/// 使用 `Arc<RwLock<>>` 保证线程安全：
/// - 读操作（get, list）使用读锁，支持并发读取
/// - 写操作（register, unregister）使用写锁，独占访问
///
/// # 示例
///
/// ```rust,no_run
/// use credbridge::connector::{ConnectorRegistry, Connector, ConnectorConfig};
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let registry = ConnectorRegistry::global();
///
///     // 注册连接器
///     registry.register("my-connector", Box::new(MyConnector))?;
///
///     // 获取连接器
///     let connector = registry.get("my-connector")?;
///
///     // 列出所有连接器
///     let names = registry.list_all();
///
///     Ok(())
/// }
/// # struct MyConnector;
/// # #[credbridge::connector::async_trait]
/// # impl Connector for MyConnector {
/// #     fn name(&self) -> &'static str { "my-connector" }
/// #     fn description(&self) -> &'static str { "我的连接器" }
/// #     async fn init(&mut self, _: ConnectorConfig) -> ConnectorResult<()> { Ok(()) }
/// #     async fn validate(&self, _: &serde_json::Value) -> Result<crate::connector::ValidatedParams, crate::connector::ValidationError> { Ok(unsafe { std::mem::zeroed() }) }
/// #     async fn execute(&self, _: crate::connector::ValidatedParams) -> ConnectorResult<serde_json::Value> { Ok(unsafe { std::mem::zeroed() }) }
/// #     async fn cleanup(&self) -> ConnectorResult<()> { Ok(()) }
/// # }
/// ```
pub struct ConnectorRegistry {
    /// 连接器存储
    connectors: RwLock<HashMap<String, Arc<dyn Connector>>>,
}

impl ConnectorRegistry {
    /// 创建新的注册表
    pub fn new() -> Self {
        Self {
            connectors: RwLock::new(HashMap::new()),
        }
    }

    /// 获取全局注册表实例
    ///
    /// 使用 `lazy_static` 创建单例全局注册表
    pub fn global() -> &'static Self {
        use once_cell::sync::Lazy;

        static GLOBAL_REGISTRY: Lazy<ConnectorRegistry> = Lazy::new(ConnectorRegistry::new);
        &GLOBAL_REGISTRY
    }

    /// 注册连接器
    ///
    /// # 参数
    ///
    /// * `name` - 连接器名称（唯一标识符）
    /// * `connector` - 连接器实例
    ///
    /// # 返回
    ///
    /// * `Ok(())` - 注册成功
    /// * `Err(ConnectorError::DuplicateRegistration)` - 名称已存在
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use credbridge::connector::ConnectorRegistry;
    ///
    /// let registry = ConnectorRegistry::new();
    /// registry.register("aws", Box::new(AwsConnector))?;
    /// ```
    pub async fn register(
        &self,
        name: impl Into<String>,
        connector: Box<dyn Connector>,
    ) -> ConnectorResult<()> {
        let name = name.into();
        let mut connectors = self.connectors.write().await;

        if connectors.contains_key(&name) {
            return Err(ConnectorError::duplicate(name));
        }

        connectors.insert(name, Arc::from(connector));
        Ok(())
    }

    /// 注册连接器（Arc 版本）
    ///
    /// 用于注册已经包装在 Arc 中的连接器
    pub async fn register_arc(
        &self,
        name: impl Into<String>,
        connector: Arc<dyn Connector>,
    ) -> ConnectorResult<()> {
        let name = name.into();
        let mut connectors = self.connectors.write().await;

        if connectors.contains_key(&name) {
            return Err(ConnectorError::duplicate(name));
        }

        connectors.insert(name, connector);
        Ok(())
    }

    /// 获取连接器
    ///
    /// # 参数
    ///
    /// * `name` - 连接器名称
    ///
    /// # 返回
    ///
    /// * `Ok(Arc<dyn Connector>)` - 连接器实例
    /// * `Err(ConnectorError::NotFound)` - 连接器不存在
    ///
    /// # 示例
    ///
    /// ```rust,no_run
    /// use credbridge::connector::ConnectorRegistry;
    ///
    /// let registry = ConnectorRegistry::new();
    /// let connector = registry.get("aws")?;
    /// ```
    pub async fn get(&self, name: &str) -> ConnectorResult<Arc<dyn Connector>> {
        let connectors = self.connectors.read().await;

        connectors
            .get(name)
            .cloned()
            .ok_or_else(|| ConnectorError::not_found(name))
    }

    /// 尝试获取连接器（返回 Option）
    ///
    /// 如果连接器不存在，返回 `None`
    pub async fn try_get(&self, name: &str) -> Option<Arc<dyn Connector>> {
        let connectors = self.connectors.read().await;
        connectors.get(name).cloned()
    }

    /// 注销连接器
    ///
    /// # 参数
    ///
    /// * `name` - 连接器名称
    ///
    /// # 返回
    ///
    /// * `Ok(())` - 注销成功
    /// * `Err(ConnectorError::NotFound)` - 连接器不存在
    ///
    /// # 注意
    ///
    /// 注销前会自动调用 `cleanup()` 方法
    pub async fn unregister(&self, name: &str) -> ConnectorResult<()> {
        let connector = self.get(name).await?;

        // 调用清理方法
        if let Err(e) = connector.cleanup().await {
            // 记录错误，但继续注销
            eprintln!("警告：清理连接器 '{}' 时出错：{}", name, e);
        }

        let mut connectors = self.connectors.write().await;
        connectors.remove(name);

        Ok(())
    }

    /// 列出所有已注册的连接器名称
    ///
    /// 返回按字母顺序排序的名称列表
    pub async fn list_all(&self) -> Vec<String> {
        let connectors = self.connectors.read().await;
        let mut names: Vec<_> = connectors.keys().cloned().collect();
        names.sort();
        names
    }

    /// 列出所有连接器的详细信息
    ///
    /// 返回 (名称，描述) 对的列表
    pub async fn list_all_with_description(&self) -> Vec<(String, String)> {
        let connectors = self.connectors.read().await;
        let mut items: Vec<_> = connectors
            .iter()
            .map(|(name, connector)| (name.clone(), connector.description().to_string()))
            .collect();
        items.sort_by(|a, b| a.0.cmp(&b.0));
        items
    }

    /// 检查连接器是否存在
    ///
    /// # 参数
    ///
    /// * `name` - 连接器名称
    ///
    /// # 返回
    ///
    /// * `true` - 连接器已注册
    /// * `false` - 连接器未注册
    pub async fn contains(&self, name: &str) -> bool {
        let connectors = self.connectors.read().await;
        connectors.contains_key(name)
    }

    /// 获取已注册连接器数量
    pub async fn len(&self) -> usize {
        let connectors = self.connectors.read().await;
        connectors.len()
    }

    /// 检查注册表是否为空
    pub async fn is_empty(&self) -> bool {
        let connectors = self.connectors.read().await;
        connectors.is_empty()
    }

    /// 清空所有连接器
    ///
    /// 会依次调用每个连接器的 `cleanup()` 方法
    pub async fn clear(&self) {
        let mut connectors = self.connectors.write().await;

        // 依次清理
        for (name, connector) in connectors.iter() {
            if let Err(e) = connector.cleanup().await {
                eprintln!("警告：清理连接器 '{}' 时出错：{}", name, e);
            }
        }

        connectors.clear();
    }
}

impl Default for ConnectorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 便捷函数
// ============================================================================

/// 注册连接器（使用全局注册表）
///
/// 这是 `ConnectorRegistry::global().register()` 的便捷函数
pub async fn register(
    name: impl Into<String>,
    connector: Box<dyn Connector>,
) -> ConnectorResult<()> {
    ConnectorRegistry::global().register(name, connector).await
}

/// 获取连接器（使用全局注册表）
///
/// 这是 `ConnectorRegistry::global().get()` 的便捷函数
pub async fn get(name: &str) -> ConnectorResult<Arc<dyn Connector>> {
    ConnectorRegistry::global().get(name).await
}

/// 列出所有连接器（使用全局注册表）
///
/// 这是 `ConnectorRegistry::global().list_all()` 的便捷函数
pub async fn list_all() -> Vec<String> {
    ConnectorRegistry::global().list_all().await
}

// ============================================================================
// Builder 模式
// ============================================================================

/// 注册表构建器
///
/// 用于链式注册多个连接器
///
/// # 示例
///
/// ```rust,no_run
/// use credbridge::connector::ConnectorRegistryBuilder;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let registry = ConnectorRegistryBuilder::new()
///     .add("aws", Box::new(AwsConnector))
///     .add("gcp", Box::new(GcpConnector))
///     .add("azure", Box::new(AzureConnector))
///     .build();
///
/// // 注册到全局注册表
/// registry.register_all(ConnectorRegistry::global()).await?;
/// # Ok(())
/// # }
/// # struct AwsConnector;
/// # struct GcpConnector;
/// # struct AzureConnector;
/// ```
pub struct ConnectorRegistryBuilder {
    connectors: Vec<(String, Box<dyn Connector>)>,
}

impl ConnectorRegistryBuilder {
    /// 创建新的构建器
    pub fn new() -> Self {
        Self {
            connectors: Vec::new(),
        }
    }

    /// 添加连接器
    pub fn add(mut self, name: impl Into<String>, connector: Box<dyn Connector>) -> Self {
        self.connectors.push((name.into(), connector));
        self
    }

    /// 构建并返回新的注册表
    pub async fn build(self) -> ConnectorRegistry {
        let registry = ConnectorRegistry::new();

        for (name, connector) in self.connectors {
            if let Err(e) = registry.register(name, connector).await {
                eprintln!("警告：注册连接器失败：{}", e);
            }
        }

        registry
    }

    /// 构建并注册到全局注册表
    pub async fn register_all(self, target: &ConnectorRegistry) -> ConnectorResult<()> {
        for (name, connector) in self.connectors {
            target.register(name, connector).await?;
        }
        Ok(())
    }
}

impl Default for ConnectorRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connector::{ConnectorConfig, ConnectorResult, ValidatedParams, ValidationError};
    use async_trait::async_trait;
    use serde_json::Value;

    // 测试用 Connector
    struct TestConnector {
        name: &'static str,
        description: &'static str,
    }

    impl TestConnector {
        fn new(name: &'static str, description: &'static str) -> Self {
            Self { name, description }
        }
    }

    #[async_trait]
    impl Connector for TestConnector {
        fn name(&self) -> &'static str {
            self.name
        }

        fn description(&self) -> &'static str {
            self.description
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

    #[tokio::test]
    async fn test_registry_basic() {
        let registry = ConnectorRegistry::new();

        // 注册
        let connector = Box::new(TestConnector::new("test", "测试连接器"));
        assert!(registry.register("test", connector).await.is_ok());

        // 获取
        let retrieved = registry.get("test").await;
        assert!(retrieved.is_ok());
        assert_eq!(retrieved.unwrap().name(), "test");

        // 检查存在
        assert!(registry.contains("test").await);
        assert!(!registry.contains("nonexistent").await);
    }

    #[tokio::test]
    async fn test_registry_duplicate() {
        let registry = ConnectorRegistry::new();

        // 第一次注册
        assert!(
            registry
                .register("test", Box::new(TestConnector::new("test", "测试")))
                .await
                .is_ok()
        );

        // 重复注册
        let result = registry
            .register("test", Box::new(TestConnector::new("test", "测试 2")))
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConnectorError::DuplicateRegistration { .. }
        ));
    }

    #[tokio::test]
    async fn test_registry_not_found() {
        let registry = ConnectorRegistry::new();

        let result = registry.get("nonexistent").await;
        assert!(result.is_err());
        // 检查错误类型
        match result {
            Err(ConnectorError::NotFound { name }) => assert_eq!(name, "nonexistent"),
            Err(e) => panic!("Expected NotFound error, got {}", e),
            Ok(_) => panic!("Expected error, got Ok"),
        }
    }

    #[tokio::test]
    async fn test_registry_unregister() {
        let registry = ConnectorRegistry::new();

        // 注册
        registry
            .register("test", Box::new(TestConnector::new("test", "测试")))
            .await
            .unwrap();

        // 注销
        assert!(registry.unregister("test").await.is_ok());

        // 验证已注销
        assert!(!registry.contains("test").await);
        assert!(registry.get("test").await.is_err());
    }

    #[tokio::test]
    async fn test_registry_list() {
        let registry = ConnectorRegistry::new();

        // 注册多个
        registry
            .register("c", Box::new(TestConnector::new("c", "C")))
            .await
            .unwrap();
        registry
            .register("a", Box::new(TestConnector::new("a", "A")))
            .await
            .unwrap();
        registry
            .register("b", Box::new(TestConnector::new("b", "B")))
            .await
            .unwrap();

        // 列出（应已排序）
        let names = registry.list_all().await;
        assert_eq!(names, vec!["a", "b", "c"]);

        // 列出带描述
        let items = registry.list_all_with_description().await;
        assert_eq!(items[0].1, "A");
        assert_eq!(items[1].1, "B");
        assert_eq!(items[2].1, "C");
    }

    #[tokio::test]
    async fn test_registry_clear() {
        let registry = ConnectorRegistry::new();

        registry
            .register("a", Box::new(TestConnector::new("a", "A")))
            .await
            .unwrap();
        registry
            .register("b", Box::new(TestConnector::new("b", "B")))
            .await
            .unwrap();

        assert_eq!(registry.len().await, 2);

        registry.clear().await;

        assert_eq!(registry.len().await, 0);
        assert!(registry.is_empty().await);
    }

    #[tokio::test]
    async fn test_registry_thread_safety() {
        let registry = Arc::new(ConnectorRegistry::new());
        let mut handles = vec![];

        // 并发注册
        for i in 0..10 {
            let registry = Arc::clone(&registry);
            let handle = tokio::spawn(async move {
                // 使用静态字符串
                let name = match i {
                    0 => "connector-0",
                    1 => "connector-1",
                    2 => "connector-2",
                    3 => "connector-3",
                    4 => "connector-4",
                    5 => "connector-5",
                    6 => "connector-6",
                    7 => "connector-7",
                    8 => "connector-8",
                    9 => "connector-9",
                    _ => unreachable!(),
                };
                let desc = match i {
                    0 => "描述 0",
                    1 => "描述 1",
                    2 => "描述 2",
                    3 => "描述 3",
                    4 => "描述 4",
                    5 => "描述 5",
                    6 => "描述 6",
                    7 => "描述 7",
                    8 => "描述 8",
                    9 => "描述 9",
                    _ => unreachable!(),
                };
                let connector = Box::new(TestConnector::new(name, desc));
                registry.register(name, connector).await
            });
            handles.push(handle);
        }

        // 等待所有注册完成
        for handle in handles {
            handle.await.unwrap().unwrap();
        }

        // 验证所有 connector 都已注册
        assert_eq!(registry.len().await, 10);
    }

    #[tokio::test]
    async fn test_builder_pattern() {
        let builder = ConnectorRegistryBuilder::new()
            .add("a", Box::new(TestConnector::new("a", "A")))
            .add("b", Box::new(TestConnector::new("b", "B")))
            .add("c", Box::new(TestConnector::new("c", "C")));

        let registry = builder.build().await;

        assert_eq!(registry.len().await, 3);
        assert!(registry.contains("a").await);
        assert!(registry.contains("b").await);
        assert!(registry.contains("c").await);
    }
}
