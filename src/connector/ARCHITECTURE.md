# Connector 框架架构设计

**Author:** CoPaw
**Date:** 2026-03-12
**Status:** Draft
**Related EP:** EP3 - Connector 框架

---

## 1. 架构概述

### 1.1 设计目标

Connector 框架为 ToaniVault 提供统一的外部服务集成接口，使第三方服务能够通过标准化方式与 ToaniVault 交互。

**核心目标：**

- 统一的接口抽象，屏蔽不同外部服务的实现差异
- 健壮的生命周期管理（初始化、执行、清理）
- 完善的错误处理和参数验证机制
- 可配置的超时控制和资源管理
- 线程安全的全局注册表

### 1.2 使用场景

| 场景         | 描述                     | 示例                                 |
| ------------ | ------------------------ | ------------------------------------ |
| 外部凭证源   | 从第三方服务同步凭证     | AWS Secrets Manager, HashiCorp Vault |
| 身份提供商   | 验证用户身份             | Okta, Auth0, Azure AD                |
| 审计日志导出 | 将审计日志发送到外部系统 | Splunk, Datadog, ELK                 |
| 通知服务     | 发送安全告警             | Slack, PagerDuty, Email              |
| 合规检查     | 执行合规性验证           | SOC2 检查，策略 enforcement          |

### 1.3 架构位置

```
┌─────────────────────────────────────────────────────────────────┐
│                        ToaniVault 架构                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │   API 层    │  │  Connector  │  │     外部服务             │  │
│  │  (Axum)     │  │   框架      │  │  (AWS/Okta/Splunk...)   │  │
│  └──────┬──────┘  └──────┬──────┘  └─────────────────────────┘  │
│         │                │                                       │
│         │                │                                       │
│         └────────────────┘                                       │
│                  │                                               │
│         ┌────────▼────────┐                                      │
│         │   Connector     │                                      │
│         │   注册表        │                                      │
│         └────────┬────────┘                                      │
│                  │                                               │
│         ┌────────▼────────────────────────────────┐              │
│         │          核心服务层                       │              │
│         │  ┌──────────┐  ┌──────────┐  ┌───────┐ │              │
│         │  │  Vault   │  │  Token   │  │ Audit │ │              │
│         │  │  Service │  │  Service │  │       │ │              │
│         │  └──────────┘  └──────────┘  └───────┘ │              │
│         └─────────────────────────────────────────┘              │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2. 核心组件设计

### 2.1 组件关系图

```
┌─────────────────────────────────────────────────────────────────┐
│                      Connector 框架组件                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────┐         ┌─────────────────────────────────┐│
│  │   Connector     │         │      ConnectorRegistry          ││
│  │     Trait       │         │  ┌───────────────────────────┐  ││
│  │  ─────────────  │         │  │ - connectors: HashMap     │  ││
│  │  + init()       │◄────────┤│  │ - register()             │  ││
│  │  + validate()   │  query  │  │ - get()                  │  ││
│  │  + execute()    │         │  │ - unregister()           │  ││
│  │  + cleanup()    │         │  │ - list_all()             │  ││
│  └─────────────────┘         │  └───────────────────────────┘  ││
│           │                  └─────────────────────────────────┘│
│           │ execute()                                           │
│           ▼                                                     │
│  ┌─────────────────┐         ┌─────────────────────────────────┐│
│  │ ValidatedParams │         │       ConnectorResult<T>        ││
│  │  ─────────────  │         │  ───────────────────────────    ││
│  │  - validated    │         │  Ok(T)                          ││
│  │  - raw          │         │  Err(ConnectorError)            ││
│  │  - metadata     │         │    - Validation                 ││
│  └─────────────────┘         │    - Timeout                    ││
│                              │    - Execution                  ││
│  ┌─────────────────┐         │    - NotFound                   ││
│  │ ValidationError │         └─────────────────────────────────┘│
│  │  ─────────────  │                                            │
│  │  - field        │                                            │
│  │  - message      │                                            │
│  │  - code         │                                            │
│  └─────────────────┘                                            │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 组件职责

| 组件                 | 职责                     | 线程安全             |
| -------------------- | ------------------------ | -------------------- |
| `Connector` Trait    | 定义连接器接口和生命周期 | N/A (Trait)          |
| `ConnectorRegistry`  | 全局连接器注册表         | 是 (`Arc<RwLock<>>`) |
| `ValidatedParams`    | 验证后的参数封装         | 是 (`Clone`)         |
| `ValidationError`    | 参数验证错误详情         | 是 (`Clone`)         |
| `ConnectorError`     | 统一错误类型             | 是 (`Clone`)         |
| `ConnectorResult<T>` | 统一返回类型             | N/A (Type Alias)     |

---

## 3. 接口定义

### 3.1 Connector Trait

```rust
/// Connector trait - 所有连接器的基础接口
///
/// 生命周期：init() -> validate() -> execute() -> cleanup()
#[async_trait]
pub trait Connector: Send + Sync {
    /// 连接器唯一标识符
    fn name(&self) -> &'static str;

    /// 连接器描述
    fn description(&self) -> &'static str;

    /// 初始化连接器
    ///
    /// 在首次使用前调用，用于建立连接、加载配置等
    async fn init(&mut self, config: ConnectorConfig) -> ConnectorResult<()>;

    /// 验证参数
    ///
    /// 在执行前验证输入参数的有效性
    async fn validate(&self, params: &serde_json::Value) -> Result<ValidatedParams, ValidationError>;

    /// 执行连接器操作
    ///
    /// 核心业务逻辑执行入口
    async fn execute(&self, params: ValidatedParams) -> ConnectorResult<serde_json::Value>;

    /// 清理资源
    ///
    /// 在连接器停用或应用关闭时调用
    async fn cleanup(&self) -> ConnectorResult<()>;

    /// 获取超时配置（秒）
    fn timeout_seconds(&self) -> u64 {
        30 // 默认 30 秒
    }

    /// 是否支持并发执行
    fn supports_concurrent(&self) -> bool {
        true
    }
}
```

### 3.2 生命周期状态机

```
┌─────────────────────────────────────────────────────────────────┐
│                    Connector 生命周期状态机                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│    ┌─────────┐                                                  │
│    │ Created │                                                  │
│    └────┬────┘                                                  │
│         │ init()                                                │
│         ▼                                                       │
│    ┌─────────┐    validate()    ┌─────────────┐                 │
│    │  Ready  │ ────────────────>│  Validated  │                 │
│    └────┬────┘                  └──────┬──────┘                 │
│         │                              │ execute()               │
│         │           ┌──────────────────┘                        │
│         │           │                                           │
│         │           ▼                                           │
│         │      ┌─────────┐                                      │
│         │      │Running  │                                      │
│         │      └────┬────┘                                      │
│         │           │                                           │
│         │           │ complete()                                │
│         │           ▼                                           │
│         │      ┌─────────┐    cleanup()    ┌─────────┐         │
│         └─────>│  Idle   │ ───────────────>│ Cleaned │         │
│                └─────────┘                 └─────────┘         │
│                                                                 │
│  错误处理：                                                      │
│  - init() 失败 → 保持 Created 状态，可重试                        │
│  - validate() 失败 → 返回 ValidationError                       │
│  - execute() 失败 → 返回 ConnectorError，保持 Ready 状态          │
│  - cleanup() 失败 → 记录错误，进入 Cleaned 状态                   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 3.3 参数验证接口

```rust
/// JSON Schema 验证器
pub struct SchemaValidator {
    schema: serde_json::Value,
}

impl SchemaValidator {
    pub fn new(schema: serde_json::Value) -> Self;
    pub fn validate(&self, params: &serde_json::Value) -> Result<(), ValidationError>;
}

/// 自定义验证规则
pub trait ValidationRule: Send + Sync {
    fn name(&self) -> &'static str;
    fn validate(&self, value: &serde_json::Value) -> Result<(), String>;
}

/// 验证器组合器
pub struct ValidatorBuilder {
    schema: Option<serde_json::Value>,
    rules: Vec<Box<dyn ValidationRule>>,
}

impl ValidatorBuilder {
    pub fn new() -> Self;
    pub fn with_schema(mut self, schema: serde_json::Value) -> Self;
    pub fn with_rule(mut self, rule: Box<dyn ValidationRule>) -> Self;
    pub fn build(self) -> CompositeValidator;
}
```

### 3.4 超时控制接口

```rust
/// 超时配置
#[derive(Clone, Debug)]
pub struct TimeoutConfig {
    /// 默认超时时间（秒）
    pub default_timeout: u64,
    /// 最大允许超时（秒）
    pub max_timeout: u64,
    /// 是否启用超时
    pub enabled: bool,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            default_timeout: 30,
            max_timeout: 300,
            enabled: true,
        }
    }
}

/// 带超时的执行包装器
pub async fn execute_with_timeout<T: Send + Sync>(
    connector: &dyn Connector,
    params: ValidatedParams,
    timeout_secs: u64,
) -> ConnectorResult<T>;
```

---

## 4. 数据流图

### 4.1 请求处理流程

```
┌─────────────────────────────────────────────────────────────────┐
│                    Connector 请求处理流程                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Client Request                                                 │
│       │                                                         │
│       ▼                                                         │
│  ┌─────────────────┐                                            │
│  │  API Handler    │                                            │
│  └────────┬────────┘                                            │
│           │ 1. 解析 connector_name                              │
│           ▼                                                     │
│  ┌─────────────────┐                                            │
│  │ ConnectorRegistry │                                          │
│  │ .get(name)        │                                          │
│  └────────┬─────────┘                                            │
│           │ 2. 获取 Connector                                     │
│           ▼                                                     │
│  ┌─────────────────┐                                            │
│  │ Connector       │                                            │
│  │ .validate()     │                                            │
│  └────────┬────────┘                                            │
│           │ 3. 参数验证                                           │
│           ▼                                                     │
│  ┌─────────────────┐         ┌─────────────────┐                │
│  │ ValidatedParams │  ──NO──>│ ValidationError │                │
│  └────────┬────────┘         └─────────────────┘                │
│          │ YES                                                    │
│          ▼                                                      │
│  ┌─────────────────┐                                            │
│  │ execute_with_   │                                            │
│  │ timeout()       │                                            │
│  └────────┬────────┘                                            │
│           │ 4. 带超时执行                                         │
│           ▼                                                     │
│  ┌─────────────────┐         ┌─────────────────┐                │
│  │ ConnectorResult │<──ERR──│  ConnectorError │                │
│  └────────┬────────┘         └─────────────────┘                │
│          │ OK                                                    │
│          ▼                                                      │
│  ┌─────────────────┐                                            │
│  │ Response        │                                            │
│  └─────────────────┘                                            │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 4.2 注册表操作流程

```
┌─────────────────────────────────────────────────────────────────┐
│                    ConnectorRegistry 操作流程                    │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  注册流程 (Register):                                           │
│  ┌────────┐    ┌─────────────────┐    ┌─────────────────────┐  │
│  │ 用户   │    │   Registry      │    │  Connector          │  │
│  │        │    │                 │    │                     │  │
│  │register()───>│ 1. 检查名称冲突  │    │                     │  │
│  │        │    │ 2. 获取写锁      │    │                     │  │
│  │        │    │ 3. 存入 HashMap  │    │                     │  │
│  │        │    │ 4. 释放写锁      │    │                     │  │
│  │<───────┤    │                 │    │                     │  │
│  │ Result│    │                 │    │                     │  │
│  └────────┘    └─────────────────┘    └─────────────────────┘  │
│                                                                 │
│  查询流程 (Get):                                                │
│  ┌────────┐    ┌─────────────────┐    ┌─────────────────────┐  │
│  │ 用户   │    │   Registry      │    │  Connector          │  │
│  │        │    │                 │    │                     │  │
│  │  get() ────>│ 1. 获取读锁      │    │                     │  │
│  │        │    │ 2. 从 HashMap 查  │    │                     │  │
│  │        │    │ 3. 返回 Arc 引用   │───>│ (Clone of Arc)     │  │
│  │<───────┤    │ 4. 释放读锁      │    │                     │  │
│  │  Arc  │    │                 │    │                     │  │
│  └────────┘    └─────────────────┘    └─────────────────────┘  │
│                                                                 │
│  线程安全保证：                                                  │
│  - 使用 Arc<RwLock<HashMap>> 保证线程安全                          │
│  - 读操作使用读锁（共享）                                         │
│  - 写操作使用写锁（独占）                                         │
│  - 返回 Arc 引用，避免所有权转移                                   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 5. 错误处理策略

### 5.1 错误层次结构

```
┌─────────────────────────────────────────────────────────────────┐
│                    ConnectorError 层次结构                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│                      ConnectorError                              │
│                           │                                      │
│         ┌─────────────────┼─────────────────┐                   │
│         │                 │                 │                   │
│         ▼                 ▼                 ▼                   │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐         │
│  │ Validation  │  │  Timeout    │  │   Execution     │         │
│  │  验证失败    │  │  执行超时    │  │   执行失败      │         │
│  └─────────────┘  └─────────────┘  └────────┬────────┘         │
│                                             │                   │
│                          ┌──────────────────┼──────────────┐   │
│                          │                  │              │   │
│                          ▼                  ▼              ▼   │
│                   ┌─────────────┐  ┌─────────────┐  ┌───────┐ │
│                   │  NotFound   │  │   Config    │  │  ...  │ │
│                   │  未找到      │  │   配置错误   │  │       │ │
│                   └─────────────┘  └─────────────┘  └───────┘ │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 5.2 错误类型定义

```rust
#[derive(Debug, Clone, thiserror::Error)]
pub enum ConnectorError {
    /// 参数验证失败
    #[error("验证失败：{message}")]
    Validation {
        message: String,
        field: Option<String>,
        code: Option<String>,
    },

    /// 执行超时
    #[error("执行超时：connector={connector}, timeout={timeout_secs}s")]
    Timeout {
        connector: String,
        timeout_secs: u64,
    },

    /// 执行失败
    #[error("执行失败：{message}")]
    Execution {
        message: String,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Connector 未找到
    #[error("Connector 未找到：{name}")]
    NotFound {
        name: String,
    },

    /// 配置错误
    #[error("配置错误：{message}")]
    Config {
        message: String,
    },

    /// 内部错误
    #[error("内部错误：{message}")]
    Internal {
        message: String,
    },
}
```

### 5.3 错误传播策略

```rust
/// 统一返回类型
pub type ConnectorResult<T> = Result<T, ConnectorError>;

/// 错误转换辅助函数
impl From<serde_json::Error> for ConnectorError {
    fn from(err: serde_json::Error) -> Self {
        ConnectorError::Execution {
            message: format!("JSON 处理失败：{}", err),
            source: Some(Box::new(err)),
        }
    }
}

impl From<tokio::time::error::Elapsed> for ConnectorError {
    fn from(_err: tokio::time::error::Elapsed) -> Self {
        ConnectorError::Timeout {
            connector: "unknown".to_string(),
            timeout_secs: 30,
        }
    }
}
```

### 5.4 错误恢复策略

| 错误类型           | 是否可恢复 | 恢复策略               |
| ------------------ | ---------- | ---------------------- |
| `Validation`       | 是         | 修正参数后重试         |
| `Timeout`          | 是         | 增加超时时间后重试     |
| `Execution` (临时) | 是         | 指数退避重试           |
| `Execution` (永久) | 否         | 记录错误，返回用户     |
| `NotFound`         | 否         | 检查 connector 名称    |
| `Config`           | 否         | 修正配置               |
| `Internal`         | 视情况     | 记录日志，可能需要重启 |

---

## 6. 使用示例

### 6.1 实现自定义 Connector

```rust
use credbridge::connector::{
    Connector, ConnectorConfig, ConnectorResult,
    ValidatedParams, ValidationError,
};
use async_trait::async_trait;
use serde_json;

/// AWS Secrets Manager Connector 示例
pub struct AwsSecretsManagerConnector {
    region: String,
    initialized: bool,
}

#[async_trait]
impl Connector for AwsSecretsManagerConnector {
    fn name(&self) -> &'static str {
        "aws-secrets-manager"
    }

    fn description(&self) -> &'static str {
        "AWS Secrets Manager 集成连接器"
    }

    async fn init(&mut self, config: ConnectorConfig) -> ConnectorResult<()> {
        // 从配置中读取 AWS 区域
        self.region = config.get("region")
            .ok_or_else(|| ConnectorError::Config {
                message: "缺少 region 配置".to_string(),
            })?
            .as_str()
            .unwrap_or("us-east-1")
            .to_string();

        // 初始化 AWS SDK 客户端
        // ...

        self.initialized = true;
        Ok(())
    }

    async fn validate(&self, params: &serde_json::Value) -> Result<ValidatedParams, ValidationError> {
        // 验证必需的字段
        let secret_id = params.get("secret_id")
            .ok_or_else(|| ValidationError {
                field: Some("secret_id".to_string()),
                message: "secret_id 是必需的".to_string(),
                code: Some("MISSING_FIELD".to_string()),
            })?;

        if !secret_id.is_string() {
            return Err(ValidationError {
                field: Some("secret_id".to_string()),
                message: "secret_id 必须是字符串".to_string(),
                code: Some("INVALID_TYPE".to_string()),
            });
        }

        Ok(ValidatedParams::new(params.clone()))
    }

    async fn execute(&self, params: ValidatedParams) -> ConnectorResult<serde_json::Value> {
        // 执行 AWS Secrets Manager API 调用
        let secret_id = params.raw().get("secret_id").unwrap().as_str().unwrap();

        // 模拟获取 secret
        let secret_value = self.get_secret(secret_id).await?;

        Ok(serde_json::json!({
            "secret_id": secret_id,
            "secret_value": secret_value,
            "retrieved_at": chrono::Utc::now().to_rfc3339(),
        }))
    }

    async fn cleanup(&self) -> ConnectorResult<()> {
        // 清理 AWS SDK 客户端资源
        Ok(())
    }

    fn timeout_seconds(&self) -> u64 {
        60 // AWS 调用可能需要更长时间
    }
}

impl AwsSecretsManagerConnector {
    pub fn new() -> Self {
        Self {
            region: String::new(),
            initialized: false,
        }
    }

    async fn get_secret(&self, secret_id: &str) -> ConnectorResult<String> {
        // 实际实现会调用 AWS SDK
        Ok("dummy-secret-value".to_string())
    }
}
```

### 6.2 注册和使用 Connector

```rust
use credbridge::connector::{ConnectorRegistry, ConnectorConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 获取全局注册表
    let registry = ConnectorRegistry::global();

    // 创建 connector 实例
    let aws_connector = AwsSecretsManagerConnector::new();

    // 注册 connector
    registry.register("aws-secrets-manager", Box::new(aws_connector))?;

    // 获取 connector
    let connector = registry.get("aws-secrets-manager")?;

    // 初始化 connector
    let config = ConnectorConfig::new()
        .with("region", "us-west-2");

    connector.lock().await.init(config).await?;

    // 执行 connector
    let params = serde_json::json!({
        "secret_id": "my-secret-key",
    });

    let result = connector.lock().await.execute(
        connector.lock().await.validate(&params).await?
    ).await?;

    println!("结果：{:?}", result);

    Ok(())
}
```

### 6.3 带超时的执行

```rust
use credbridge::connector::{execute_with_timeout, ValidatedParams};
use tokio::time::{timeout, Duration};

async fn execute_connector(
    connector: Arc<Mutex<dyn Connector>>,
    params: ValidatedParams,
) -> ConnectorResult<serde_json::Value> {
    let timeout_secs = connector.lock().await.timeout_seconds();

    execute_with_timeout(
        connector.lock().await.as_ref(),
        params,
        timeout_secs,
    ).await
}

// 或者手动控制超时
async fn execute_with_manual_timeout(
    connector: Arc<Mutex<dyn Connector>>,
    params: ValidatedParams,
    timeout_secs: u64,
) -> ConnectorResult<serde_json::Value> {
    tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        connector.lock().await.execute(params),
    )
    .await
    .map_err(|_| ConnectorError::Timeout {
        connector: "unknown".to_string(),
        timeout_secs,
    })?
}
```

### 6.4 参数验证器组合

```rust
use credbridge::connector::{ValidatorBuilder, ValidationRule};
use serde_json::Value;

// 自定义验证规则：检查字符串长度
struct MaxLengthRule {
    field: String,
    max_length: usize,
}

impl ValidationRule for MaxLengthRule {
    fn name(&self) -> &'static str {
        "max_length"
    }

    fn validate(&self, value: &Value) -> Result<(), String> {
        if let Value::String(s) = value {
            if s.len() > self.max_length {
                return Err(format!(
                    "{} 长度不能超过 {} 字符",
                    self.field, self.max_length
                ));
            }
        }
        Ok(())
    }
}

// 构建复合验证器
let validator = ValidatorBuilder::new()
    .with_schema(serde_json::json!({
        "type": "object",
        "required": ["secret_id"],
        "properties": {
            "secret_id": { "type": "string" }
        }
    }))
    .with_rule(Box::new(MaxLengthRule {
        field: "secret_id".to_string(),
        max_length: 256,
    }))
    .build();

// 使用验证器
let params = serde_json::json!({
    "secret_id": "my-secret",
});

validator.validate(&params)?;
```

---

## 7. 测试策略

### 7.1 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_error_display() {
        let err = ValidationError {
            field: Some("name".to_string()),
            message: "必填字段".to_string(),
            code: Some("REQUIRED".to_string()),
        };

        assert_eq!(err.to_string(), "验证失败 [name]: 必填字段");
    }

    #[tokio::test]
    async fn test_connector_registry_thread_safety() {
        let registry = Arc::new(ConnectorRegistry::new());
        let mut handles = vec![];

        // 并发注册
        for i in 0..10 {
            let registry = Arc::clone(&registry);
            let handle = tokio::spawn(async move {
                let connector = MockConnector::new();
                registry.register(&format!("connector-{}", i), Box::new(connector))
            });
            handles.push(handle);
        }

        // 等待所有注册完成
        for handle in handles {
            handle.await.unwrap().unwrap();
        }

        // 验证所有 connector 都已注册
        assert_eq!(registry.list_all().len(), 10);
    }
}
```

### 7.2 集成测试

```rust
// tests/connector_integration.rs

#[tokio::test]
async fn test_full_connector_lifecycle() {
    let registry = ConnectorRegistry::global();
    let connector = TestConnector::new();

    // 注册
    registry.register("test-connector", Box::new(connector)).unwrap();

    // 获取
    let connector = registry.get("test-connector").unwrap();

    // 初始化
    let config = ConnectorConfig::default();
    connector.lock().await.init(config).await.unwrap();

    // 验证
    let params = serde_json::json!({ "key": "value" });
    let validated = connector.lock().await.validate(&params).await.unwrap();

    // 执行
    let result = connector.lock().await.execute(validated).await.unwrap();

    // 清理
    connector.lock().await.cleanup().await.unwrap();

    assert!(result.get("success").unwrap().as_bool().unwrap());
}
```

---

## 8. 扩展点

### 8.1 预定义验证规则

框架可扩展的预定义验证规则：

| 规则            | 描述             |
| --------------- | ---------------- |
| `RequiredRule`  | 检查字段是否存在 |
| `TypeRule`      | 检查字段类型     |
| `MinLengthRule` | 最小字符串长度   |
| `MaxLengthRule` | 最大字符串长度   |
| `PatternRule`   | 正则表达式匹配   |
| `EnumRule`      | 枚举值检查       |
| `RangeRule`     | 数值范围检查     |

### 8.2 Connector 模板

框架可提供的基础模板：

- `HttpConnector` - HTTP API 连接器基类
- `DatabaseConnector` - 数据库连接器基类
- `FileConnector` - 文件系统连接器基类
- `MessageQueueConnector` - 消息队列连接器基类

---

## 9. 性能考虑

### 9.1 连接池

对于需要建立连接的 Connector（如数据库、HTTP），应实现连接池：

```rust
pub struct PooledConnector<P> {
    pool: Arc<ConnectionPool<P>>,
    // ...
}

impl<P> PooledConnector<P> {
    pub async fn get_connection(&self) -> Result<Connection, ConnectorError> {
        self.pool.acquire()
            .await
            .map_err(|e| ConnectorError::Execution {
                message: "获取连接失败".to_string(),
                source: Some(Box::new(e)),
            })
    }
}
```

### 9.2 缓存策略

对于读多写少的场景，可实现缓存：

```rust
pub struct CachedConnector {
    inner: Box<dyn Connector>,
    cache: Arc<DashMap<String, CacheEntry>>,
    ttl: Duration,
}
```

---

## 10. 安全考虑

### 10.1 敏感数据处理

```rust
/// 敏感参数（自动 zeroize）
#[derive(Clone, ZeroizeOnDrop)]
pub struct SensitiveParam {
    #[zeroize(skip)]
    metadata: ParamMetadata,
    value: Vec<u8>,  // 敏感数据
}
```

### 10.2 审计日志

所有 Connector 执行应记录审计日志：

```rust
async fn execute_with_audit(
    connector: &dyn Connector,
    params: ValidatedParams,
    user_id: &str,
) -> ConnectorResult<serde_json::Value> {
    let recorder = AuditRecorder::global();

    recorder.record(AuditEntry::new()
        .action(format!("connector.{}", connector.name()))
        .user_id(user_id)
        .timestamp(chrono::Utc::now())
    ).await;

    let result = connector.execute(params).await?;

    recorder.record(AuditEntry::new()
        .action(format!("connector.{}.complete", connector.name()))
        .user_id(user_id)
        .success(true)
    ).await;

    Ok(result)
}
```

---

## 11. 验收标准检查清单

- [ ] trait 定义完整
  - [x] 生命周期方法：`init()`, `execute()`, `cleanup()`
  - [x] 参数验证方法：`validate()`
  - [x] 超时控制：`timeout_seconds()`
  - [x] 返回类型：`ConnectorResult<T>`

- [x] 生命周期清晰
  - [x] 状态机定义
  - [x] 状态转换规则
  - [x] 错误处理策略

- [x] 错误处理完善
  - [x] `Validation` 错误
  - [x] `Timeout` 错误
  - [x] `Execution` 错误
  - [x] `NotFound` 错误
  - [x] `Config` 错误

- [ ] 代码框架可编译
  - [ ] `mod.rs` 模块定义
  - [ ] `trait.rs` trait 定义
  - [ ] `error.rs` 错误类型
  - [ ] `registry.rs` 注册表

- [x] 文档清晰易懂
  - [x] 架构概述
  - [x] 组件设计
  - [x] 接口定义
  - [x] 数据流图
  - [x] 错误处理策略
  - [x] 使用示例

---

## 12. 参考资料

- [Rust Async Trait](https://docs.rs/async-trait/latest/async_trait/)
- [thiserror - 错误处理库](https://docs.rs/thiserror/latest/thiserror/)
- [serde_json - JSON 处理](https://docs.rs/serde_json/latest/serde_json/)
- [tokio - 异步运行时](https://docs.rs/tokio/latest/tokio/)
- [JSON Schema 验证](https://json-schema.org/)
