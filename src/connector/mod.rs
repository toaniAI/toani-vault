//! Connector 框架
//!
//! 提供统一的外部服务集成接口，使第三方服务能够通过标准化方式与 CredBridge 交互。
//!
//! # 架构概览
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                      Connector 框架组件                          │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                 │
//! │  ┌─────────────────┐         ┌─────────────────────────────────┐│
//! │  │   Connector     │         │      ConnectorRegistry          ││
//! │  │     Trait       │◄────────│      (全局注册表)               ││
//! │  │  ─────────────  │         │                                 ││
//! │  │  + init()       │         │  - register()                   ││
//! │  │  + validate()   │         │  - get()                        ││
//! │  │  + execute()    │         │  - unregister()                 ││
//! │  │  + cleanup()    │         │  - list_all()                   ││
//! │  └─────────────────┘         └─────────────────────────────────┘│
//! │                                                                 │
//! │  ┌─────────────────┐         ┌─────────────────────────────────┐│
//! │  │ ValidatedParams │         │       ConnectorError            ││
//! │  │  (验证参数)      │         │  ───────────────────────────    ││
//! │  │                 │         │  - Validation (验证失败)         ││
//! │  └─────────────────┘         │  - Timeout (执行超时)            ││
//! │                              │  - Execution (执行失败)          ││
//! │  ┌─────────────────┐         │  - NotFound (未找到)             ││
//! │  │ ConnectorConfig │         │  - Config (配置错误)             ││
//! │  │  (连接器配置)   │         │  - Internal (内部错误)           ││
//! │  └─────────────────┘         └─────────────────────────────────┘│
//! │                                                                 │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # 生命周期
//!
//! Connector 的生命周期如下：
//!
//! ```text
//! Created ──init()──> Ready ──validate()──> Validated ──execute()──> Running
//!                                              │
//!                                              │ (失败)
//!                                              ▼
//!                                       ValidationError
//!
//! Running ──complete()──> Idle ──cleanup()──> Cleaned
//! ```
//!
//! # 使用示例
//!
//! ## 实现自定义 Connector
//!
//! ```rust,no_run
//! use credbridge::connector::{
//!     Connector, ConnectorConfig, ConnectorResult,
//!     ValidatedParams, ValidationError,
//! };
//! use async_trait::async_trait;
//! use serde_json::Value;
//!
//! struct MyConnector;
//!
//! #[async_trait]
//! impl Connector for MyConnector {
//!     fn name(&self) -> &'static str {
//!         "my-connector"
//!     }
//!
//!     fn description(&self) -> &'static str {
//!         "我的连接器"
//!     }
//!
//!     async fn init(&mut self, config: ConnectorConfig) -> ConnectorResult<()> {
//!         // 初始化逻辑
//!         Ok(())
//!     }
//!
//!     async fn validate(&self, params: &Value) -> Result<ValidatedParams, ValidationError> {
//!         // 验证逻辑
//!         Ok(ValidatedParams::new(params.clone()))
//!     }
//!
//!     async fn execute(&self, params: ValidatedParams) -> ConnectorResult<Value> {
//!         // 执行逻辑
//!         Ok(serde_json::json!({ "success": true }))
//!     }
//!
//!     async fn cleanup(&self) -> ConnectorResult<()> {
//!         // 清理逻辑
//!         Ok(())
//!     }
//! }
//! ```
//!
//! ## 注册和使用 Connector
//!
//! ```rust,no_run
//! use credbridge::connector::ConnectorRegistry;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let registry = ConnectorRegistry::global();
//!
//!     // 注册
//!     registry.register("my-connector", Box::new(MyConnector))?;
//!
//!     // 获取
//!     let connector = registry.get("my-connector").await?;
//!
//!     // 使用
//!     // connector.init().await?;
//!     // connector.execute().await?;
//!
//!     Ok(())
//! }
//! # struct MyConnector;
//! # impl credbridge::connector::Connector for MyConnector {
//! #     fn name(&self) -> &'static str { "my-connector" }
//! #     fn description(&self) -> &'static str { "我的连接器" }
//! #     async fn init(&mut self, _: credbridge::connector::ConnectorConfig) -> credbridge::connector::ConnectorResult<()> { Ok(()) }
//! #     async fn validate(&self, _: &serde_json::Value) -> Result<credbridge::connector::ValidatedParams, credbridge::connector::ValidationError> { Ok(unsafe { std::mem::zeroed() }) }
//! #     async fn execute(&self, _: credbridge::connector::ValidatedParams) -> credbridge::connector::ConnectorResult<serde_json::Value> { Ok(unsafe { std::mem::zeroed() }) }
//! #     async fn cleanup(&self) -> credbridge::connector::ConnectorResult<()> { Ok(()) }
//! # }
//! ```
//!
//! # 错误处理
//!
//! ```rust
//! use credbridge::connector::{ConnectorError, ConnectorResult};
//!
//! # fn example() -> ConnectorResult<()> {
//! // 创建不同类型的错误
//! let validation_err = ConnectorError::validation("参数验证失败");
//! let timeout_err = ConnectorError::timeout("my-connector", 30);
//! let not_found_err = ConnectorError::not_found("missing-connector");
//!
//! // 统一返回类型
//! fn my_function() -> ConnectorResult<String> {
//!     Ok("success".to_string())
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # 特性
//!
//! - **统一接口**: 所有连接器实现相同的 trait
//! - **生命周期管理**: init/validate/execute/cleanup
//! - **参数验证**: JSON Schema + 自定义规则
//! - **超时控制**: 可配置的超时时间
//! - **线程安全**: Arc + RwLock 保证并发安全
//! - **错误处理**: 完善的错误分类和传播

// ============================================================================
// 模块声明
// ============================================================================

pub mod error;
pub mod http;
pub mod registry;
pub mod timeout;
pub mod trait_def;
pub mod validator;

// ============================================================================
// 重新导出
// ============================================================================

// 错误类型
pub use error::{ConnectorError, ConnectorResult, ValidationError};

// Trait 和核心类型
pub use trait_def::{
    Connector, ConnectorConfig, NopConnector, ValidatedParams, execute_with_timeout,
};

// 注册表
pub use registry::{ConnectorRegistry, ConnectorRegistryBuilder, get, list_all, register};

// HTTP 连接器
pub use http::{HttpConnector, HttpConnectorConfig};

// 超时控制
pub use timeout::{TimeoutConfig, TimeoutError, TimeoutWrapper};

// 验证器
pub use validator::{
    CompositeValidator, SchemaValidator, ValidationRule, ValidatorBuilder,
    rules::{
        EnumRule, MaxLengthRule, MinLengthRule, PatternRule, RangeRule, RequiredRule, TypeRule,
    },
};

// ============================================================================
// 版本常量
// ============================================================================

/// Connector 框架版本
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 默认超时时间（秒）
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 30;

/// 最大允许超时时间（秒）
pub const MAX_TIMEOUT_SECONDS: u64 = 300;

// ============================================================================
// 初始化函数
// ============================================================================

/// 初始化 Connector 框架
///
/// 调用此函数会：
/// 1. 初始化全局注册表
/// 2. 注册内置连接器
///
/// 此函数是可选的，框架会在首次使用时自动初始化
pub fn init() {
    // 初始化全局注册表（懒加载）
    let _ = ConnectorRegistry::global();
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(DEFAULT_TIMEOUT_SECONDS, 30);
        assert_eq!(MAX_TIMEOUT_SECONDS, 300);
    }

    #[tokio::test]
    async fn test_global_registry() {
        let registry = ConnectorRegistry::global();
        assert!(registry.is_empty().await);
    }

    #[test]
    fn test_nop_connector() {
        let connector = NopConnector;
        assert_eq!(connector.name(), "nop");
        assert_eq!(connector.description(), "空连接器（用于测试）");
        assert_eq!(connector.timeout_seconds(), 30);
        assert!(connector.supports_concurrent());
    }
}
