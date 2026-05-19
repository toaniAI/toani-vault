use axum::{
    Extension, Json, Router, extract::State, http::StatusCode, response::Response, routing::post,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    middleware::{TokenScope, ValidatedToken},
    response::{ApiErrorResponse, success_response},
    tokens::parse_uuid_str,
};

const ALLOWED_BUSINESS_TYPES: &[&str] = &["oauth_binding_access"];

#[derive(Clone)]
pub struct ApprovalApiState {
    pool: PgPool,
}

impl ApprovalApiState {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateApprovalRequest {
    business_type: String,
    business_id: String,
}

#[derive(Debug, Serialize)]
struct CreateApprovalResponse {
    approval_id: String,
    status: String,
}

pub fn approval_routes(state: ApprovalApiState) -> Router {
    Router::new()
        .route("/approvals", post(create_approval_request_handler))
        .with_state(state)
}

async fn create_approval_request_handler(
    State(state): State<ApprovalApiState>,
    Extension(token): Extension<ValidatedToken>,
    Json(request): Json<CreateApprovalRequest>,
) -> Result<Response, ApiErrorResponse> {
    if !token.scopes.contains(&TokenScope::TenantAdmin) {
        return Err(ApiErrorResponse::forbidden(
            "Tenant admin scope is required for approval initiation",
        ));
    }

    let business_type = request.business_type.trim();
    if business_type.is_empty() {
        return Err(ApiErrorResponse::invalid_request(
            "business_type is required",
        ));
    }
    if !ALLOWED_BUSINESS_TYPES.contains(&business_type) {
        return Err(ApiErrorResponse::invalid_request(format!(
            "business_type must be one of: {}",
            ALLOWED_BUSINESS_TYPES.join(", ")
        )));
    }

    let business_id = request.business_id.trim();
    if business_id.is_empty() {
        return Err(ApiErrorResponse::invalid_request("business_id is required"));
    }

    let approval_id = Uuid::now_v7();
    let tenant_id = parse_uuid_str(&token.tenant_id, "tenant_id")?;
    let requested_by = parse_uuid_str(&token.user_id, "user_id")?;
    let status = "pending";

    sqlx::query(
        r#"
        INSERT INTO approval_requests (
            approval_id,
            tenant_id,
            requested_by,
            business_type,
            business_id,
            status
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(approval_id)
    .bind(tenant_id)
    .bind(requested_by)
    .bind(business_type)
    .bind(business_id)
    .bind(status)
    .execute(&state.pool)
    .await
    .map_err(|error| {
        tracing::error!(?error, "failed to create approval request");
        ApiErrorResponse::internal_error("Failed to create approval request")
    })?;

    Ok(success_response(
        StatusCode::CREATED,
        CreateApprovalResponse {
            approval_id: approval_id.to_string(),
            status: status.to_string(),
        },
    ))
}
