//! CredBridge Rust SDK - Axum Web 框架集成示例
//!
//! 展示如何在 Axum 应用中集成 CredBridge SDK

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, post},
    Router,
};
use credbridge_sdk::{CredBridgeConfig, CredBridgeSDK};
use credbridge_sdk::types::{CredBridgeError, CredBridgeErrorCode, CredentialType, TokenScope};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// 应用状态
#[derive(Clone)]
struct AppState {
    sdk: Arc<CredBridgeSDK>,
}

// 创建凭证请求
#[derive(Deserialize)]
struct CreateCredentialRequest {
    service_id: String,
    credential_type: String,
    data: serde_json::Value,
    expires_at: Option<i64>,
}

// 解密请求
#[derive(Deserialize)]
struct DecryptRequest {
    reason: String,
}

// 凭证响应
#[derive(Serialize)]
struct CredentialResponse {
    credential_id: String,
    service_id: String,
    credential_type: String,
    created_at: String,
}

// API 错误响应
#[derive(Serialize)]
struct ApiError {
    error: String,
    code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<String>,
}

// 从 CredBridgeError 转换为 HTTP 响应
trait IntoResponse {
    fn into_response(self) -> (StatusCode, Json<ApiError>);
}

impl IntoResponse for CredBridgeError {
    fn into_response(self) -> (StatusCode, Json<ApiError>) {
        let status = match self.code {
            CredBridgeErrorCode::NotFound => StatusCode::NOT_FOUND,
            CredBridgeErrorCode::Unauthorized | CredBridgeErrorCode::TokenExpired => StatusCode::UNAUTHORIZED,
            CredBridgeErrorCode::Forbidden | CredBridgeErrorCode::InsufficientScope => StatusCode::FORBIDDEN,
            CredBridgeErrorCode::InvalidRequest => StatusCode::BAD_REQUEST,
            CredBridgeErrorCode::NetworkError | CredBridgeErrorCode::Timeout => StatusCode::SERVICE_UNAVAILABLE,
            CredBridgeErrorCode::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let error = ApiError {
            error: self.message,
            code: self.code.to_string(),
            request_id: self.request_id,
        };

        (status, Json(error))
    }
}

// 检查权限辅助函数
fn check_scope(state: &AppState, scope: TokenScope) -> Result<(), (StatusCode, Json<ApiError>)> {
    if state.sdk.token().has_scope(scope) {
        Ok(())
    } else {
        let error = ApiError {
            error: format!("Missing required scope: {:?}", scope),
            code: "INSUFFICIENT_SCOPE".to_string(),
            request_id: None,
        };
        Err((StatusCode::FORBIDDEN, Json(error)))
    }
}

// 获取当前用户信息
async fn get_current_user(State(state): State<AppState>) -> Json<serde_json::Value> {
    let token = state.sdk.token();
    Json(serde_json::json!({
        "tenant_id": token.get_tenant_id(),
        "user_id": token.get_user_id(),
        "scopes": token.get_scopes(),
        "expires_in": token.get_remaining_time_formatted(),
    }))
}

// 获取凭证列表
async fn list_credentials(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    check_scope(&state, TokenScope::CredentialRead)?;

    let (credentials, total) = state.sdk.credentials()
        .list(None, None)
        .await
        .map_err(|e| e.into_response())?;

    Ok(Json(serde_json::json!({
        "credentials": credentials,
        "total": total,
    })))
}

// 获取单个凭证
async fn get_credential(
    State(state): State<AppState>,
    Path(credential_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    check_scope(&state, TokenScope::CredentialRead)?;

    let credential = state.sdk.credentials()
        .get(&credential_id, None)
        .await
        .map_err(|e| e.into_response())?;

    Ok(Json(serde_json::to_value(credential).unwrap_or_default()))
}

// 解密凭证
async fn decrypt_credential(
    State(state): State<AppState>,
    Path(credential_id): Path<String>,
    Json(req): Json<DecryptRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    check_scope(&state, TokenScope::CredentialDecrypt)?;

    let decrypted = state.sdk.credentials()
        .decrypt(&credential_id, Some(req.reason), None)
        .await
        .map_err(|e| e.into_response())?;

    Ok(Json(serde_json::to_value(decrypted).unwrap_or_default()))
}

// 创建凭证
async fn create_credential(
    State(state): State<AppState>,
    Json(req): Json<CreateCredentialRequest>,
) -> Result<(StatusCode, Json<CredentialResponse>), (StatusCode, Json<ApiError>)> {
    check_scope(&state, TokenScope::CredentialWrite)?;

    let cred_type = parse_credential_type(&req.credential_type)
        .map_err(|e| {
            let error = ApiError {
                error: e,
                code: "INVALID_TYPE".to_string(),
                request_id: None,
            };
            (StatusCode::BAD_REQUEST, Json(error))
        })?;

    let data = match req.data {
        serde_json::Value::Object(map) => map.into_iter().collect(),
        _ => {
            let error = ApiError {
                error: "Invalid data format".to_string(),
                code: "INVALID_DATA".to_string(),
                request_id: None,
            };
            return Err((StatusCode::BAD_REQUEST, Json(error)));
        }
    };

    let result = state.sdk.credentials()
        .create(req.service_id, cred_type, data, req.expires_at, None)
        .await
        .map_err(|e| e.into_response())?;

    let response = CredentialResponse {
        credential_id: result.credential_id,
        service_id: result.service_id,
        credential_type: result.credential_type,
        created_at: result.created_at,
    };

    Ok((StatusCode::CREATED, Json(response)))
}

// 删除凭证
async fn delete_credential(
    State(state): State<AppState>,
    Path(credential_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    check_scope(&state, TokenScope::CredentialWrite)?;

    state.sdk.credentials()
        .delete(&credential_id, None)
        .await
        .map_err(|e| e.into_response())?;

    Ok(StatusCode::NO_CONTENT)
}

fn parse_credential_type(s: &str) -> Result<CredentialType, String> {
    match s {
        "username_password" => Ok(CredentialType::UsernamePassword),
        "oauth_refresh" => Ok(CredentialType::OAuthRefresh),
        "api_key" => Ok(CredentialType::ApiKey),
        "session_cookie" => Ok(CredentialType::SessionCookie),
        "kyc_document" => Ok(CredentialType::KycDocument),
        _ => Err(format!("Unknown credential type: {}", s)),
    }
}

// 创建路由器
pub fn create_router(sdk: CredBridgeSDK) -> Router {
    let state = AppState {
        sdk: Arc::new(sdk),
    };

    Router::new()
        .route("/api/me", get(get_current_user))
        .route("/api/credentials", get(list_credentials).post(create_credential))
        .route(
            "/api/credentials/:id",
            get(get_credential).delete(delete_credential),
        )
        .route("/api/credentials/:id/decrypt", post(decrypt_credential))
        .with_state(state)
}

// 启动服务器示例
/*
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let base_url = std::env::var("CREDBRIDGE_BASE_URL")
        .unwrap_or_else(|_| "https://api.credbridge.io".to_string());
    let token = std::env::var("CREDBRIDGE_TOKEN")
        .expect("CREDBRIDGE_TOKEN must be set");

    let config = CredBridgeConfig::new(base_url)
        .with_token(token);

    let sdk = CredBridgeSDK::new(config)?;
    let app = create_router(sdk);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    println!("Server running on http://0.0.0.0:3000");
    axum::serve(listener, app).await?;

    Ok(())
}
*/

// 示例运行函数
pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Axum 集成示例 ===\n");
    println!("此示例展示了如何在 Axum 应用中使用 CredBridge SDK。");
    println!("请查看源代码了解完整实现。\n");
    println!("API Endpoints:");
    println!("  GET  /api/me                    - 获取当前用户信息");
    println!("  GET  /api/credentials           - 获取凭证列表");
    println!("  GET  /api/credentials/:id       - 获取凭证详情");
    println!("  POST /api/credentials/:id/decrypt - 解密凭证");
    println!("  POST /api/credentials           - 创建凭证");
    println!("  DELETE /api/credentials/:id     - 删除凭证");

    Ok(())
}
