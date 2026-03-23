//! API 国际化支持
//!
//! 提供 locale 解析、消息目录与请求级 locale 中间件。

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::{
    extract::{FromRequestParts, Request, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{ACCEPT_LANGUAGE, CONTENT_LANGUAGE},
        request::Parts,
    },
    middleware::Next,
    response::Response,
};
use serde::Serialize;
use serde_json::Value;

use crate::api::auth::MemoryUserStore;
use crate::api::context::RequestContext;
use crate::api::middleware::ValidatedToken;
use crate::tenant::{MemoryTenantConfigStore, TenantConfigStore, TenantId};

pub const DEFAULT_LOCALE: &str = "zh-CN";
pub const EN_US_LOCALE: &str = "en-US";
pub const ZH_CN_LOCALE: &str = "zh-CN";

pub type I18nParams = BTreeMap<String, Value>;

#[derive(Debug, Clone, Serialize)]
pub struct I18nMetadata {
    pub key: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: I18nParams,
}

impl I18nMetadata {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            params: BTreeMap::new(),
        }
    }

    pub fn with_params(mut self, params: I18nParams) -> Self {
        self.params = params;
        self
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedLocale(pub String);

impl ResolvedLocale {
    pub fn new(locale: impl Into<String>) -> Self {
        Self(normalize_locale(&locale.into()).to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ResolvedLocale {
    fn default() -> Self {
        Self(DEFAULT_LOCALE.to_string())
    }
}

#[async_trait::async_trait]
impl<S> FromRequestParts<S> for ResolvedLocale
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(parts
            .extensions
            .get::<ResolvedLocale>()
            .cloned()
            .unwrap_or_else(|| ResolvedLocale::new(resolve_accept_language(&parts.headers))))
    }
}

#[derive(Clone)]
pub struct LocaleResolverState {
    pub user_store: Arc<MemoryUserStore>,
    pub tenant_store: MemoryTenantConfigStore,
}

impl LocaleResolverState {
    pub fn new(user_store: Arc<MemoryUserStore>, tenant_store: MemoryTenantConfigStore) -> Self {
        Self {
            user_store,
            tenant_store,
        }
    }
}

pub async fn locale_middleware(
    State(state): State<LocaleResolverState>,
    mut request: Request,
    next: Next,
) -> Response {
    let accept_language = resolve_accept_language(request.headers());
    let token = request.extensions().get::<ValidatedToken>().cloned();

    let user_locale = if let Some(ref token) = token {
        state
            .user_store
            .get_user(&token.user_id)
            .await
            .and_then(|user| user.locale)
    } else {
        None
    };

    let tenant_locale = if let Some(ref token) = token {
        state
            .tenant_store
            .get_config(&TenantId::from_string(token.tenant_id.clone()))
            .await
            .ok()
            .map(|config| config.settings.language)
    } else {
        None
    };

    let locale = normalize_locale(
        user_locale
            .as_deref()
            .or(tenant_locale.as_deref())
            .unwrap_or(accept_language),
    );
    let resolved = ResolvedLocale::new(locale);
    request.extensions_mut().insert(resolved.clone());

    if let Some(context) = request.extensions_mut().get_mut::<RequestContext>() {
        context.resolved_locale = resolved.as_str().to_string();
    } else if let Some(token) = token {
        let mut context = RequestContext::from_validated_token(&token);
        context.resolved_locale = resolved.as_str().to_string();
        request.extensions_mut().insert(context);
    }

    let mut response = next.run(request).await;
    set_content_language(response.headers_mut(), resolved.as_str());
    response
}

pub fn resolve_accept_language(headers: &HeaderMap) -> &'static str {
    headers
        .get(ACCEPT_LANGUAGE)
        .and_then(|value| value.to_str().ok())
        .and_then(resolve_accept_language_value)
        .unwrap_or(DEFAULT_LOCALE)
}

fn resolve_accept_language_value(value: &str) -> Option<&'static str> {
    value
        .split(',')
        .filter_map(|part| part.split(';').next())
        .map(str::trim)
        .find_map(|candidate| match normalize_locale(candidate) {
            ZH_CN_LOCALE => Some(ZH_CN_LOCALE),
            EN_US_LOCALE => Some(EN_US_LOCALE),
            _ => None,
        })
}

pub fn normalize_locale(locale: &str) -> &'static str {
    match locale.trim().to_ascii_lowercase().as_str() {
        "en" | "en-us" => EN_US_LOCALE,
        "zh" | "zh-cn" | "zh-hans" | "zh-hans-cn" => ZH_CN_LOCALE,
        _ => DEFAULT_LOCALE,
    }
}

pub fn set_content_language(headers: &mut HeaderMap, locale: &str) {
    let value =
        HeaderValue::from_str(locale).unwrap_or_else(|_| HeaderValue::from_static(DEFAULT_LOCALE));
    headers.insert(CONTENT_LANGUAGE, value);
}

pub fn interpolate_message(template: &str, params: &I18nParams) -> String {
    let mut message = template.to_string();
    for (key, value) in params {
        let replacement = match value {
            Value::String(s) => s.clone(),
            _ => value.to_string(),
        };
        message = message.replace(&format!("{{{key}}}"), &replacement);
    }
    message
}

pub fn translate(locale: &str, key: &str, params: &I18nParams) -> String {
    let template = match (normalize_locale(locale), key) {
        ("zh-CN", "errors.auth.missing_token") => "缺少 Authorization 请求头",
        ("en-US", "errors.auth.missing_token") => "Missing Authorization header",
        ("zh-CN", "errors.auth.invalid_authorization_header") => "无效的 Authorization 请求头",
        ("en-US", "errors.auth.invalid_authorization_header") => "Invalid Authorization header",
        ("zh-CN", "errors.auth.invalid_token") => "Token 验证失败: {reason}",
        ("en-US", "errors.auth.invalid_token") => "Token validation failed: {reason}",
        ("zh-CN", "errors.auth.expired_token") => "Token 已过期",
        ("en-US", "errors.auth.expired_token") => "Token has expired",
        ("zh-CN", "errors.auth.insufficient_scope") => {
            "缺少必需的 scope: {required_scope}，当前 scopes: {current_scopes}"
        }
        ("en-US", "errors.auth.insufficient_scope") => {
            "Missing required scope: {required_scope}, current scopes: {current_scopes}"
        }
        ("zh-CN", "errors.auth.invalid_credentials") => "用户名或密码错误",
        ("en-US", "errors.auth.invalid_credentials") => "Invalid username or password",
        ("zh-CN", "errors.auth.token_generation_failed") => "Token 生成失败: {reason}",
        ("en-US", "errors.auth.token_generation_failed") => "Token generation failed: {reason}",
        ("zh-CN", "errors.auth.refresh_token_generation_failed") => {
            "Refresh Token 生成失败: {reason}"
        }
        ("en-US", "errors.auth.refresh_token_generation_failed") => {
            "Refresh token generation failed: {reason}"
        }
        ("zh-CN", "errors.auth.invalid_refresh_token") => "Refresh Token 无效: {reason}",
        ("en-US", "errors.auth.invalid_refresh_token") => "Invalid refresh token: {reason}",
        ("zh-CN", "errors.auth.user_not_found") => "用户不存在",
        ("en-US", "errors.auth.user_not_found") => "User not found",
        ("zh-CN", "errors.auth.invalid_scope") => "至少需要一个有效的 scope",
        ("en-US", "errors.auth.invalid_scope") => "At least one valid scope is required",
        ("zh-CN", "errors.auth.invalid_subject") => {
            "Token sub 声明格式无效，应为 tenant_id:user_id"
        }
        ("en-US", "errors.auth.invalid_subject") => {
            "Token subject format is invalid; expected tenant_id:user_id"
        }
        ("zh-CN", "errors.preferences.invalid_locale") => "不支持的 locale: {locale}",
        ("en-US", "errors.preferences.invalid_locale") => "Unsupported locale: {locale}",
        ("zh-CN", "errors.api.invalid_request") => "请求参数无效",
        ("en-US", "errors.api.invalid_request") => "Invalid request",
        ("zh-CN", "errors.api.unauthorized") => "未认证或 Token 无效",
        ("en-US", "errors.api.unauthorized") => "Unauthorized or invalid token",
        ("zh-CN", "errors.api.forbidden") => "权限不足",
        ("en-US", "errors.api.forbidden") => "Forbidden",
        ("zh-CN", "errors.api.not_found") => "资源不存在",
        ("en-US", "errors.api.not_found") => "Resource not found",
        ("zh-CN", "errors.api.conflict") => "资源冲突",
        ("en-US", "errors.api.conflict") => "Resource conflict",
        ("zh-CN", "errors.api.rate_limited") => "请求过于频繁，请稍后重试",
        ("en-US", "errors.api.rate_limited") => "Too many requests, please try again later",
        ("zh-CN", "errors.api.internal_error") => "服务器内部错误",
        ("en-US", "errors.api.internal_error") => "Internal server error",
        ("zh-CN", "errors.api.service_unavailable") => "服务暂不可用",
        ("en-US", "errors.api.service_unavailable") => "Service temporarily unavailable",
        ("zh-CN", "errors.api.verification_failed") => "验证失败",
        ("en-US", "errors.api.verification_failed") => "Verification failed",
        ("zh-CN", "errors.api.crypto_error") => "加密或解密失败",
        ("en-US", "errors.api.crypto_error") => "Encryption or decryption failed",
        ("zh-CN", "errors.api.tenant_isolation") => "租户隔离校验失败",
        ("en-US", "errors.api.tenant_isolation") => "Tenant isolation check failed",
        _ => {
            if normalize_locale(locale) == EN_US_LOCALE {
                "Internal server error"
            } else {
                "服务器内部错误"
            }
        }
    };

    interpolate_message(template, params)
}

pub fn default_error_key(error: &str) -> String {
    match error {
        "missing_token" => "errors.auth.missing_token".to_string(),
        "invalid_token" => "errors.auth.invalid_token".to_string(),
        "expired_token" => "errors.auth.expired_token".to_string(),
        "insufficient_scope" => "errors.auth.insufficient_scope".to_string(),
        "invalid_credentials" => "errors.auth.invalid_credentials".to_string(),
        "token_generation_failed" => "errors.auth.token_generation_failed".to_string(),
        "invalid_refresh_token" => "errors.auth.invalid_refresh_token".to_string(),
        "user_not_found" => "errors.auth.user_not_found".to_string(),
        "invalid_scope" => "errors.auth.invalid_scope".to_string(),
        "invalid_request" => "errors.api.invalid_request".to_string(),
        "unauthorized" => "errors.api.unauthorized".to_string(),
        "forbidden" => "errors.api.forbidden".to_string(),
        "not_found" => "errors.api.not_found".to_string(),
        "conflict" => "errors.api.conflict".to_string(),
        "rate_limited" => "errors.api.rate_limited".to_string(),
        "internal_error" => "errors.api.internal_error".to_string(),
        "service_unavailable" => "errors.api.service_unavailable".to_string(),
        "verification_failed" => "errors.api.verification_failed".to_string(),
        "crypto_error" => "errors.api.crypto_error".to_string(),
        "tenant_isolation" => "errors.api.tenant_isolation".to_string(),
        _ => "errors.api.internal_error".to_string(),
    }
}

pub fn invalid_locale_response(locale: &str, invalid_locale: &str) -> Response {
    let mut params = I18nParams::new();
    params.insert(
        "locale".to_string(),
        Value::String(invalid_locale.to_string()),
    );
    crate::api::response::error_response_with_i18n(
        StatusCode::BAD_REQUEST,
        crate::api::response::ErrorCode::InvalidRequest,
        locale,
        "errors.preferences.invalid_locale",
        params,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_locale() {
        assert_eq!(normalize_locale("en"), EN_US_LOCALE);
        assert_eq!(normalize_locale("en-US"), EN_US_LOCALE);
        assert_eq!(normalize_locale("zh"), ZH_CN_LOCALE);
        assert_eq!(normalize_locale("zh-Hans"), ZH_CN_LOCALE);
        assert_eq!(normalize_locale("fr-FR"), DEFAULT_LOCALE);
    }

    #[test]
    fn test_accept_language_resolution() {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT_LANGUAGE,
            HeaderValue::from_static("en-US,en;q=0.9,zh-CN;q=0.8"),
        );
        assert_eq!(resolve_accept_language(&headers), EN_US_LOCALE);
    }

    #[test]
    fn test_translate_with_params() {
        let mut params = I18nParams::new();
        params.insert("reason".to_string(), Value::String("boom".to_string()));
        assert_eq!(
            translate("en-US", "errors.auth.invalid_token", &params),
            "Token validation failed: boom"
        );
    }
}
