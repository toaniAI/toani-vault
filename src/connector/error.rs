//! Connector 错误类型定义
//!
//! 提供统一的错误处理机制，支持错误分类、错误传播和错误转换。

use std::fmt;
use thiserror::Error;

/// Connector 统一错误类型
///
/// 所有 Connector 相关操作都返回此错误类型
#[derive(Debug, Error)]
pub enum ConnectorError {
    /// 参数验证失败
    ///
    /// 当输入参数不符合预期时返回此错误
    #[error("验证失败：{message}")]
    Validation {
        /// 错误消息
        message: String,
        /// 出错的字段名（如果有）
        field: Option<String>,
        /// 错误代码（用于程序化处理）
        code: Option<String>,
    },

    /// 执行超时
    ///
    /// 当 Connector 执行时间超过配置的超时时间时返回此错误
    #[error("执行超时：connector={connector}, timeout={timeout_secs}s")]
    Timeout {
        /// Connector 名称
        connector: String,
        /// 配置的超时时间（秒）
        timeout_secs: u64,
    },

    /// 执行失败
    ///
    /// Connector 业务逻辑执行失败时返回此错误
    #[error("执行失败：{message}")]
    Execution {
        /// 错误消息
        message: String,
        /// 原始错误（如果有）
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Connector 未找到
    ///
    /// 当请求的 Connector 名称在注册表中不存在时返回此错误
    #[error("Connector 未找到：{name}")]
    NotFound {
        /// 请求的 Connector 名称
        name: String,
    },

    /// 配置错误
    ///
    /// 当 Connector 配置无效或缺少必需配置时返回此错误
    #[error("配置错误：{message}")]
    Config {
        /// 错误消息
        message: String,
    },

    /// 重复注册
    ///
    /// 当尝试注册已存在的 Connector 名称时返回此错误
    #[error("重复注册：Connector '{name}' 已存在")]
    DuplicateRegistration {
        /// 已存在的 Connector 名称
        name: String,
    },

    /// 内部错误
    ///
    /// 框架内部错误，通常表示 bug 或系统故障
    #[error("内部错误：{message}")]
    Internal {
        /// 错误消息
        message: String,
    },
}

impl Clone for ConnectorError {
    fn clone(&self) -> Self {
        match self {
            Self::Validation {
                message,
                field,
                code,
            } => Self::Validation {
                message: message.clone(),
                field: field.clone(),
                code: code.clone(),
            },
            Self::Timeout {
                connector,
                timeout_secs,
            } => Self::Timeout {
                connector: connector.clone(),
                timeout_secs: *timeout_secs,
            },
            Self::Execution { message, .. } => Self::Execution {
                message: message.clone(),
                source: None, // 不克隆源错误
            },
            Self::NotFound { name } => Self::NotFound { name: name.clone() },
            Self::Config { message } => Self::Config {
                message: message.clone(),
            },
            Self::DuplicateRegistration { name } => {
                Self::DuplicateRegistration { name: name.clone() }
            }
            Self::Internal { message } => Self::Internal {
                message: message.clone(),
            },
        }
    }
}

impl ConnectorError {
    /// 创建验证错误
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
            field: None,
            code: None,
        }
    }

    /// 创建带字段的验证错误
    pub fn validation_field(message: impl Into<String>, field: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
            field: Some(field.into()),
            code: None,
        }
    }

    /// 创建带错误代码的验证错误
    pub fn validation_with_code(
        message: impl Into<String>,
        field: Option<String>,
        code: impl Into<String>,
    ) -> Self {
        Self::Validation {
            message: message.into(),
            field,
            code: Some(code.into()),
        }
    }

    /// 创建超时错误
    pub fn timeout(connector: impl Into<String>, timeout_secs: u64) -> Self {
        Self::Timeout {
            connector: connector.into(),
            timeout_secs,
        }
    }

    /// 创建执行错误
    pub fn execution(message: impl Into<String>) -> Self {
        Self::Execution {
            message: message.into(),
            source: None,
        }
    }

    /// 创建带原始错误的执行错误
    pub fn execution_with_source(
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Execution {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// 创建未找到错误
    pub fn not_found(name: impl Into<String>) -> Self {
        Self::NotFound { name: name.into() }
    }

    /// 创建配置错误
    pub fn config(message: impl Into<String>) -> Self {
        Self::Config {
            message: message.into(),
        }
    }

    /// 创建重复注册错误
    pub fn duplicate(name: impl Into<String>) -> Self {
        Self::DuplicateRegistration { name: name.into() }
    }

    /// 创建内部错误
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }

    /// 获取错误代码（用于 API 响应）
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::Validation { .. } => "VALIDATION_ERROR",
            Self::Timeout { .. } => "TIMEOUT_ERROR",
            Self::Execution { .. } => "EXECUTION_ERROR",
            Self::NotFound { .. } => "NOT_FOUND_ERROR",
            Self::Config { .. } => "CONFIG_ERROR",
            Self::DuplicateRegistration { .. } => "DUPLICATE_REGISTRATION_ERROR",
            Self::Internal { .. } => "INTERNAL_ERROR",
        }
    }

    /// 获取字段名（仅验证错误）
    pub fn field(&self) -> Option<&str> {
        match self {
            Self::Validation { field, .. } => field.as_deref(),
            _ => None,
        }
    }
}

/// 统一返回类型
pub type ConnectorResult<T> = Result<T, ConnectorError>;

// ============================================================================
// From 转换实现
// ============================================================================

impl From<serde_json::Error> for ConnectorError {
    fn from(err: serde_json::Error) -> Self {
        Self::execution_with_source("JSON 处理失败", err)
    }
}

impl From<tokio::time::error::Elapsed> for ConnectorError {
    fn from(_err: tokio::time::error::Elapsed) -> Self {
        Self::Timeout {
            connector: "unknown".to_string(),
            timeout_secs: 30,
        }
    }
}

// ============================================================================
// 验证错误
// ============================================================================

/// 参数验证错误详情
///
/// 提供详细的验证失败信息，包括字段名、错误消息和错误代码
#[derive(Debug, Clone, Error)]
pub struct ValidationError {
    /// 出错的字段名
    pub field: Option<String>,
    /// 错误消息
    pub message: String,
    /// 错误代码
    pub code: Option<String>,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.field {
            Some(field) => write!(f, "验证失败 [{}]: {}", field, self.message),
            None => write!(f, "验证失败：{}", self.message),
        }
    }
}

impl ValidationError {
    /// 创建简单的验证错误
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            field: None,
            code: None,
        }
    }

    /// 创建带字段的验证错误
    pub fn field(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: Some(field.into()),
            message: message.into(),
            code: None,
        }
    }

    /// 创建带错误代码的验证错误
    pub fn with_code(
        field: Option<String>,
        message: impl Into<String>,
        code: impl Into<String>,
    ) -> Self {
        Self {
            field,
            message: message.into(),
            code: Some(code.into()),
        }
    }

    /// 转换为 ConnectorError
    pub fn into_connector_error(self) -> ConnectorError {
        ConnectorError::Validation {
            message: self.message,
            field: self.field,
            code: self.code,
        }
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_error_display() {
        // 无字段
        let err = ValidationError::new("一般错误");
        assert_eq!(err.to_string(), "验证失败：一般错误");

        // 有字段
        let err = ValidationError::field("name", "必填字段");
        assert_eq!(err.to_string(), "验证失败 [name]: 必填字段");

        // 带错误代码
        let err = ValidationError::with_code(
            Some("email".to_string()),
            "格式不正确".to_string(),
            "INVALID_FORMAT".to_string(),
        );
        assert!(err.to_string().contains("[email]"));
        assert!(err.to_string().contains("格式不正确"));
    }

    #[test]
    fn test_connector_error_codes() {
        let err = ConnectorError::validation("测试");
        assert_eq!(err.error_code(), "VALIDATION_ERROR");

        let err = ConnectorError::timeout("test", 30);
        assert_eq!(err.error_code(), "TIMEOUT_ERROR");

        let err = ConnectorError::execution("测试");
        assert_eq!(err.error_code(), "EXECUTION_ERROR");

        let err = ConnectorError::not_found("test");
        assert_eq!(err.error_code(), "NOT_FOUND_ERROR");

        let err = ConnectorError::config("测试");
        assert_eq!(err.error_code(), "CONFIG_ERROR");

        let err = ConnectorError::duplicate("test");
        assert_eq!(err.error_code(), "DUPLICATE_REGISTRATION_ERROR");

        let err = ConnectorError::internal("测试");
        assert_eq!(err.error_code(), "INTERNAL_ERROR");
    }

    #[test]
    fn test_connector_error_field() {
        let err = ConnectorError::validation_field("错误", "field_name");
        assert_eq!(err.field(), Some("field_name"));

        let err = ConnectorError::execution("错误");
        assert_eq!(err.field(), None);
    }

    #[test]
    fn test_error_conversion() {
        // serde_json 错误转换
        let json_err = serde_json::from_str::<serde_json::Value>("invalid json");
        assert!(json_err.is_err());

        let connector_err: ConnectorError = json_err.unwrap_err().into();
        assert!(matches!(connector_err, ConnectorError::Execution { .. }));
    }
}
