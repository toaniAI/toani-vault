//! 统一 API 响应模块
//!
//! 提供所有 API 端点统一的响应格式，确保客户端可以一致地处理响应
//!
//! # 响应格式
//!
//! ## 成功响应
//! ```json
//! {
//!   "success": true,
//!   "data": { ... }
//! }
//! ```
//!
//! ## 错误响应
//! ```json
//! {
//!   "success": false,
//!   "error": "error_code",
//!   "message": "用户友好的错误描述"
//! }
//! ```
//!
//! # 错误代码规范
//!
//! | 代码 | HTTP 状态码 | 说明 |
//! |------|------------|------|
//! | `invalid_request` | 400 | 请求参数无效 |
//! | `unauthorized` | 401 | 未认证或 Token 无效 |
//! | `forbidden` | 403 | 权限不足 |
//! | `not_found` | 404 | 资源不存在 |
//! | `conflict` | 409 | 资源冲突 |
//! | `unprocessable_entity` | 422 | 请求体缺少必填字段或字段类型错误 |
//! | `rate_limited` | 429 | 请求过于频繁 |
//! | `internal_error` | 500 | 服务器内部错误 |
//! | `service_unavailable` | 503 | 服务暂不可用 |

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::api::i18n::{
    DEFAULT_LOCALE, I18nMetadata, I18nParams, default_error_key, set_content_language, translate,
};

/// 统一 API 错误代码
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// 请求参数无效
    InvalidRequest,
    /// 未认证或 Token 无效
    Unauthorized,
    /// 权限不足
    Forbidden,
    /// 资源不存在
    NotFound,
    /// 凭证不存在
    CredentialNotFound,
    /// 资源冲突
    Conflict,
    /// 请求体缺少必填字段或字段类型错误
    UnprocessableEntity,
    /// 请求过于频繁
    RateLimited,
    /// 服务器内部错误
    InternalError,
    /// 服务暂不可用
    ServiceUnavailable,
    /// 沙箱会话不在当前实例
    SandboxSessionNotLocal,
    /// 沙箱 owner 暂不可用
    SandboxOwnerUnavailable,
    /// 验证失败
    VerificationFailed,
    /// 加密/解密错误
    CryptoError,
    /// 租户隔离错误
    TenantIsolation,
}

impl ErrorCode {
    /// 获取错误代码字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCode::InvalidRequest => "invalid_request",
            ErrorCode::Unauthorized => "unauthorized",
            ErrorCode::Forbidden => "forbidden",
            ErrorCode::NotFound => "not_found",
            ErrorCode::CredentialNotFound => "credential_not_found",
            ErrorCode::Conflict => "conflict",
            ErrorCode::UnprocessableEntity => "unprocessable_entity",
            ErrorCode::RateLimited => "rate_limited",
            ErrorCode::InternalError => "internal_error",
            ErrorCode::ServiceUnavailable => "service_unavailable",
            ErrorCode::SandboxSessionNotLocal => "sandbox_session_not_local",
            ErrorCode::SandboxOwnerUnavailable => "sandbox_owner_unavailable",
            ErrorCode::VerificationFailed => "verification_failed",
            ErrorCode::CryptoError => "crypto_error",
            ErrorCode::TenantIsolation => "tenant_isolation",
        }
    }

    /// 获取对应的 HTTP 状态码
    pub fn http_status(&self) -> StatusCode {
        match self {
            ErrorCode::InvalidRequest => StatusCode::BAD_REQUEST,
            ErrorCode::Unauthorized => StatusCode::UNAUTHORIZED,
            ErrorCode::Forbidden => StatusCode::FORBIDDEN,
            ErrorCode::NotFound => StatusCode::NOT_FOUND,
            ErrorCode::CredentialNotFound => StatusCode::NOT_FOUND,
            ErrorCode::Conflict => StatusCode::CONFLICT,
            ErrorCode::UnprocessableEntity => StatusCode::UNPROCESSABLE_ENTITY,
            ErrorCode::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            ErrorCode::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::SandboxSessionNotLocal => StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::SandboxOwnerUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::VerificationFailed => StatusCode::BAD_REQUEST,
            ErrorCode::CryptoError => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::TenantIsolation => StatusCode::FORBIDDEN,
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 统一 API 错误响应
#[derive(Debug, Clone, Serialize)]
pub struct ApiErrorResponse {
    /// 是否成功（错误响应始终为 false）
    pub success: bool,
    /// 错误代码
    pub error: String,
    /// 用户友好的错误描述
    pub message: String,
    /// 国际化元数据
    #[serde(skip_serializing_if = "Option::is_none")]
    pub i18n: Option<I18nMetadata>,
    /// 当前响应语言
    pub locale: String,
}

impl ApiErrorResponse {
    /// 创建新的错误响应
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        let error = code.as_str().to_string();
        let locale = DEFAULT_LOCALE.to_string();
        Self {
            success: false,
            error: error.clone(),
            message: message.into(),
            i18n: Some(I18nMetadata::new(default_error_key(&error))),
            locale,
        }
    }

    /// 创建本地化错误响应
    pub fn localized(
        code: ErrorCode,
        locale: &str,
        key: impl Into<String>,
        params: I18nParams,
    ) -> Self {
        let key = key.into();
        Self {
            success: false,
            error: code.as_str().to_string(),
            message: translate(locale, &key, &params),
            i18n: Some(I18nMetadata::new(key).with_params(params)),
            locale: locale.to_string(),
        }
    }

    /// 创建无效请求错误
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidRequest, message)
    }

    /// 创建未认证错误
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unauthorized, message)
    }

    /// 创建权限不足错误
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Forbidden, message)
    }

    /// 创建资源不存在错误
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    /// 创建冲突错误
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Conflict, message)
    }

    /// 创建无法处理的实体错误（缺少必填字段或字段类型错误）
    pub fn unprocessable_entity(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::UnprocessableEntity, message)
    }

    /// 创建速率限制错误
    pub fn rate_limited(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::RateLimited, message)
    }

    /// 创建内部错误
    pub fn internal_error(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InternalError, message)
    }

    /// 创建服务不可用错误
    pub fn service_unavailable(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::ServiceUnavailable, message)
    }

    /// 创建验证失败错误
    pub fn verification_failed(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::VerificationFailed, message)
    }

    /// 创建加密错误
    pub fn crypto_error(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::CryptoError, message)
    }

    /// 创建租户隔离错误
    pub fn tenant_isolation(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::TenantIsolation, message)
    }
}

impl IntoResponse for ApiErrorResponse {
    fn into_response(self) -> Response {
        let locale = self.locale.clone();
        let status = ErrorCode::from_string(&self.error)
            .map(|c| c.http_status())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

        if status.is_server_error() {
            tracing::error!(error_code = %self.error, message = %self.message, status = status.as_u16(), "API server error");
        } else if status.is_client_error() {
            tracing::warn!(error_code = %self.error, message = %self.message, status = status.as_u16(), "API client error");
        }

        let mut response = (status, Json(json!(self))).into_response();
        set_content_language(response.headers_mut(), &locale);
        response
    }
}

impl ErrorCode {
    /// 从字符串解析错误代码
    pub fn from_string(s: &str) -> Option<Self> {
        match s {
            "invalid_request" => Some(ErrorCode::InvalidRequest),
            "unauthorized" => Some(ErrorCode::Unauthorized),
            "forbidden" => Some(ErrorCode::Forbidden),
            "not_found" => Some(ErrorCode::NotFound),
            "credential_not_found" => Some(ErrorCode::CredentialNotFound),
            "conflict" => Some(ErrorCode::Conflict),
            "unprocessable_entity" => Some(ErrorCode::UnprocessableEntity),
            "rate_limited" => Some(ErrorCode::RateLimited),
            "internal_error" => Some(ErrorCode::InternalError),
            "service_unavailable" => Some(ErrorCode::ServiceUnavailable),
            "sandbox_session_not_local" => Some(ErrorCode::SandboxSessionNotLocal),
            "sandbox_owner_unavailable" => Some(ErrorCode::SandboxOwnerUnavailable),
            "verification_failed" => Some(ErrorCode::VerificationFailed),
            "crypto_error" => Some(ErrorCode::CryptoError),
            "tenant_isolation" => Some(ErrorCode::TenantIsolation),
            _ => None,
        }
    }
}

/// 统一 API 成功响应
#[derive(Debug, Clone, Serialize)]
pub struct ApiSuccessResponse<T> {
    /// 是否成功（成功响应始终为 true）
    pub success: bool,
    /// 响应数据
    pub data: T,
}

impl<T: Serialize> ApiSuccessResponse<T> {
    /// 创建新的成功响应
    pub fn new(data: T) -> Self {
        Self {
            success: true,
            data,
        }
    }
}

impl<T: Serialize> IntoResponse for ApiSuccessResponse<T> {
    fn into_response(self) -> Response {
        (StatusCode::OK, Json(json!(self))).into_response()
    }
}

/// 创建带状态码的成功响应
pub fn success_response<T: Serialize>(status: StatusCode, data: T) -> Response {
    (status, Json(json!(ApiSuccessResponse::new(data)))).into_response()
}

/// 创建带状态码的错误响应
pub fn error_response(status: StatusCode, code: ErrorCode, message: impl Into<String>) -> Response {
    (status, Json(json!(ApiErrorResponse::new(code, message)))).into_response()
}

/// 创建带 i18n 元数据的错误响应
pub fn error_response_with_i18n(
    status: StatusCode,
    code: ErrorCode,
    locale: &str,
    key: &str,
    params: I18nParams,
) -> Response {
    ApiErrorResponse::localized(code, locale, key, params)
        .with_status(status)
        .into_response()
}

trait IntoResponseWithStatus {
    fn with_status(self, status: StatusCode) -> (StatusCode, Self)
    where
        Self: Sized,
    {
        (status, self)
    }
}

impl IntoResponseWithStatus for ApiErrorResponse {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_display() {
        assert_eq!(ErrorCode::InvalidRequest.to_string(), "invalid_request");
        assert_eq!(ErrorCode::Unauthorized.to_string(), "unauthorized");
        assert_eq!(ErrorCode::Forbidden.to_string(), "forbidden");
        assert_eq!(ErrorCode::NotFound.to_string(), "not_found");
        assert_eq!(
            ErrorCode::CredentialNotFound.to_string(),
            "credential_not_found"
        );
        assert_eq!(ErrorCode::InternalError.to_string(), "internal_error");
    }

    #[test]
    fn test_error_code_http_status() {
        assert_eq!(
            ErrorCode::InvalidRequest.http_status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            ErrorCode::Unauthorized.http_status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(ErrorCode::Forbidden.http_status(), StatusCode::FORBIDDEN);
        assert_eq!(ErrorCode::NotFound.http_status(), StatusCode::NOT_FOUND);
        assert_eq!(
            ErrorCode::CredentialNotFound.http_status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            ErrorCode::InternalError.http_status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_error_code_from_string() {
        assert_eq!(
            ErrorCode::from_string("invalid_request"),
            Some(ErrorCode::InvalidRequest)
        );
        assert_eq!(
            ErrorCode::from_string("unauthorized"),
            Some(ErrorCode::Unauthorized)
        );
        assert_eq!(
            ErrorCode::from_string("credential_not_found"),
            Some(ErrorCode::CredentialNotFound)
        );
        assert_eq!(ErrorCode::from_string("unknown"), None);
    }

    #[test]
    fn test_api_error_response_creation() {
        let response = ApiErrorResponse::invalid_request("参数不能为空");
        assert!(!response.success);
        assert_eq!(response.error, "invalid_request");
        assert_eq!(response.message, "参数不能为空");
    }

    #[test]
    fn test_api_error_response_serialization() {
        let response = ApiErrorResponse::not_found("凭证不存在");
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("\"success\":false"));
        assert!(json.contains("\"error\":\"not_found\""));
        assert!(json.contains("\"message\":\"凭证不存在\""));
    }

    #[test]
    fn test_api_success_response() {
        let response = ApiSuccessResponse::new(vec!["item1", "item2"]);
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"data\":[\"item1\",\"item2\"]"));
    }

    #[test]
    fn test_convenience_constructors() {
        let err = ApiErrorResponse::unauthorized("Token 已过期");
        assert_eq!(err.error, "unauthorized");
        assert_eq!(err.locale, "zh-CN");
        assert!(err.i18n.is_some());

        let err = ApiErrorResponse::forbidden("权限不足");
        assert_eq!(err.error, "forbidden");

        let err = ApiErrorResponse::rate_limited("请求过于频繁，请稍后重试");
        assert_eq!(err.error, "rate_limited");
    }

    #[test]
    fn test_localized_error_response() {
        let response = ApiErrorResponse::localized(
            ErrorCode::Unauthorized,
            "en-US",
            "errors.api.unauthorized",
            I18nParams::new(),
        );

        assert_eq!(response.message, "Unauthorized or invalid token");
        assert_eq!(response.locale, "en-US");
        assert_eq!(response.i18n.unwrap().key, "errors.api.unauthorized");
    }
}
