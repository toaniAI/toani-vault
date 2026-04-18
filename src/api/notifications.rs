//! 通知 API
//!
//! 提供 Dashboard 顶部通知列表的最小可用接口。

use axum::{Extension, Json, Router, routing::get};
use chrono::{Duration, Utc};
use serde::Serialize;

use crate::api::middleware::ValidatedToken;
use crate::api::response::ApiSuccessResponse;

#[derive(Debug, Clone, Serialize)]
pub struct NotificationItem {
    pub id: String,
    pub title: String,
    pub message: String,
    pub level: String,
    pub is_read: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotificationListResponse {
    pub items: Vec<NotificationItem>,
    pub total: usize,
}

pub async fn list_notifications(
    Extension(token): Extension<ValidatedToken>,
) -> Json<ApiSuccessResponse<NotificationListResponse>> {
    let now = Utc::now();
    let items = vec![
        NotificationItem {
            id: format!("welcome-{}", token.user_id),
            title: "TEE Runtime Ready".to_string(),
            message: "可信执行环境与凭证保护链路当前运行正常。".to_string(),
            level: "info".to_string(),
            is_read: false,
            created_at: now.to_rfc3339(),
        },
        NotificationItem {
            id: format!("audit-{}", token.tenant_id),
            title: "Audit Stream Active".to_string(),
            message: "审计日志链路已同步，最近操作可在审计页面查看。".to_string(),
            level: "success".to_string(),
            is_read: true,
            created_at: (now - Duration::minutes(10)).to_rfc3339(),
        },
    ];

    Json(ApiSuccessResponse::new(NotificationListResponse {
        total: items.len(),
        items,
    }))
}

pub fn notifications_routes() -> Router {
    Router::new().route("/notifications", get(list_notifications))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Extension, body::Body, http::Request};
    use tower::ServiceExt;

    fn create_test_token() -> ValidatedToken {
        use crate::api::middleware::TokenScope;
        use std::time::{SystemTime, UNIX_EPOCH};

        ValidatedToken {
            token_id: "test-token-id".to_string(),
            subject: "tenant-001:user-001".to_string(),
            tenant_id: "tenant-001".to_string(),
            user_id: "user-001".to_string(),
            expires_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_secs()
                + 3600,
            scopes: vec![TokenScope::CredentialRead],
            issued_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_secs(),
            membership_id: None,
            metadata: std::collections::HashMap::new(),
            subject_type: crate::token::TOKEN_SUBJECT_TYPE_USER.to_string(),
            issued_from: crate::token::TOKEN_ISSUED_FROM_SESSION.to_string(),
            allowed_credential_ids: None,
        }
    }

    #[tokio::test]
    async fn notifications_route_returns_items() {
        let app = notifications_routes().layer(Extension(create_test_token()));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/notifications")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body bytes");
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).expect("json body");

        assert_eq!(json["success"], true);
        assert_eq!(json["data"]["total"], 2);
        assert_eq!(json["data"]["items"][0]["is_read"], false);
    }
}
