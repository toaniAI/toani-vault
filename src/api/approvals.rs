use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Response,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, QueryBuilder, Row, Transaction, types::Json as SqlxJson};
use uuid::Uuid;

use super::{
    middleware::{TokenScope, ValidatedToken},
    response::{ApiErrorResponse, PaginatedResponse, success_response},
    tokens::parse_uuid_str,
};

const ALLOWED_BUSINESS_TYPES: &[&str] = &["oauth_binding_access", "credential_runtime_access"];
const APPROVAL_PENDING_BUSINESS_UNIQUE_INDEX: &str =
    "idx_approval_requests_pending_business_unique";
const APPROVAL_AUDIT_OUTCOME_SUCCESS: &str = "success";
const APPROVAL_AUDIT_OUTCOME_REPLAYED: &str = "replayed";
const RUNTIME_CREDENTIAL_APPROVAL_BUSINESS_TYPE: &str = "credential_runtime_access";

#[derive(Clone)]
pub struct ApprovalApiState {
    pool: PgPool,
}

impl ApprovalApiState {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimeCredentialApprovalReservation {
    tenant_id: Uuid,
    business_id: String,
    reservation_id: Uuid,
}

#[derive(Debug, Clone)]
struct ApprovalBusinessResultRecord {
    status: ApprovalStatus,
    consumed_at: Option<DateTime<Utc>>,
    reservation_id: Option<Uuid>,
}

pub(crate) async fn acquire_runtime_credential_approval(
    pool: &PgPool,
    tenant_id: Uuid,
    requested_by: &str,
    request_id: &str,
) -> Result<RuntimeCredentialApprovalReservation, ApiErrorResponse> {
    let requested_by = Uuid::parse_str(requested_by)
        .map_err(|_| ApiErrorResponse::invalid_request("token user_id must be a valid UUID"))?;
    let now = Utc::now();
    let reservation_id = Uuid::now_v7();
    let mut tx = pool.begin().await.map_err(|error| {
        tracing::error!(
            ?error,
            "failed to begin runtime approval acquisition transaction"
        );
        ApiErrorResponse::internal_error("Failed to acquire runtime credential approval")
    })?;

    if let Some(pending) = fetch_pending_approval_record(
        &mut tx,
        tenant_id,
        RUNTIME_CREDENTIAL_APPROVAL_BUSINESS_TYPE,
        request_id,
    )
    .await?
    {
        tx.commit().await.map_err(|error| {
            tracing::error!(?error, "failed to commit pending runtime approval lookup");
            ApiErrorResponse::internal_error("Failed to acquire runtime credential approval")
        })?;
        return Err(waiting_for_approval_conflict(pending.approval_id));
    }

    let reserved_approval = sqlx::query_as::<_, (Uuid,)>(
        r#"
        UPDATE approval_business_results
        SET reservation_id = $4,
            reserved_at = $5,
            updated_at = $5
        WHERE tenant_id = $1
          AND business_type = $2
          AND business_id = $3
          AND status = 'approved'
          AND consumed_at IS NULL
          AND reservation_id IS NULL
        RETURNING approval_id
        "#,
    )
    .bind(tenant_id)
    .bind(RUNTIME_CREDENTIAL_APPROVAL_BUSINESS_TYPE)
    .bind(request_id)
    .bind(reservation_id)
    .bind(now)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| {
        tracing::error!(?error, "failed to reserve runtime approval result");
        ApiErrorResponse::internal_error("Failed to acquire runtime credential approval")
    })?;

    if reserved_approval.is_some() {
        tx.commit().await.map_err(|error| {
            tracing::error!(?error, "failed to commit runtime approval reservation");
            ApiErrorResponse::internal_error("Failed to acquire runtime credential approval")
        })?;

        return Ok(RuntimeCredentialApprovalReservation {
            tenant_id,
            business_id: request_id.to_string(),
            reservation_id,
        });
    }

    if let Some(result) = fetch_approval_business_result(
        &mut tx,
        tenant_id,
        RUNTIME_CREDENTIAL_APPROVAL_BUSINESS_TYPE,
        request_id,
    )
    .await?
        && result.status == ApprovalStatus::Approved
        && result.consumed_at.is_none()
        && result.reservation_id.is_some()
    {
        tx.commit().await.map_err(|error| {
            tracing::error!(?error, "failed to commit runtime approval in-use conflict");
            ApiErrorResponse::internal_error("Failed to acquire runtime credential approval")
        })?;
        return Err(ApiErrorResponse::conflict(
            "Runtime access approval already in use",
        ));
    }

    let approval_id = Uuid::now_v7();
    let insert_result = sqlx::query(
        r#"
        INSERT INTO approval_requests (
            approval_id,
            tenant_id,
            requested_by,
            business_type,
            business_id,
            status
        )
        VALUES ($1, $2, $3, $4, $5, 'pending')
        "#,
    )
    .bind(approval_id)
    .bind(tenant_id)
    .bind(requested_by)
    .bind(RUNTIME_CREDENTIAL_APPROVAL_BUSINESS_TYPE)
    .bind(request_id)
    .execute(&mut *tx)
    .await;

    match insert_result {
        Ok(_) => {
            tx.commit().await.map_err(|error| {
                tracing::error!(?error, "failed to commit runtime approval pending creation");
                ApiErrorResponse::internal_error("Failed to acquire runtime credential approval")
            })?;
            Err(waiting_for_approval_conflict(approval_id))
        }
        Err(error) if is_pending_business_conflict(&error) => {
            let pending = fetch_pending_approval_record(
                &mut tx,
                tenant_id,
                RUNTIME_CREDENTIAL_APPROVAL_BUSINESS_TYPE,
                request_id,
            )
            .await?;

            tx.commit().await.map_err(|commit_error| {
                tracing::error!(
                    ?commit_error,
                    "failed to commit runtime approval replayed pending lookup"
                );
                ApiErrorResponse::internal_error("Failed to acquire runtime credential approval")
            })?;

            match pending {
                Some(record) => Err(waiting_for_approval_conflict(record.approval_id)),
                None => Err(ApiErrorResponse::internal_error(error.to_string())),
            }
        }
        Err(error) => Err(ApiErrorResponse::internal_error(error.to_string())),
    }
}

pub(crate) async fn release_runtime_credential_approval(
    pool: &PgPool,
    reservation: &RuntimeCredentialApprovalReservation,
) -> Result<(), ApiErrorResponse> {
    let now = Utc::now();
    let released = sqlx::query(
        r#"
        UPDATE approval_business_results
        SET reservation_id = NULL,
            reserved_at = NULL,
            updated_at = $4
        WHERE tenant_id = $1
          AND business_type = $2
          AND business_id = $3
          AND reservation_id = $5
          AND consumed_at IS NULL
        "#,
    )
    .bind(reservation.tenant_id)
    .bind(RUNTIME_CREDENTIAL_APPROVAL_BUSINESS_TYPE)
    .bind(&reservation.business_id)
    .bind(now)
    .bind(reservation.reservation_id)
    .execute(pool)
    .await
    .map_err(|error| {
        tracing::error!(?error, "failed to release runtime approval reservation");
        ApiErrorResponse::internal_error("Failed to release runtime credential approval")
    })?;

    if released.rows_affected() == 1 {
        Ok(())
    } else {
        Err(ApiErrorResponse::internal_error(
            "Failed to release runtime credential approval",
        ))
    }
}

pub(crate) async fn consume_runtime_credential_approval(
    pool: &PgPool,
    reservation: &RuntimeCredentialApprovalReservation,
) -> Result<(), ApiErrorResponse> {
    let now = Utc::now();
    let consumed = sqlx::query(
        r#"
        UPDATE approval_business_results
        SET consumed_at = $4,
            reservation_id = NULL,
            reserved_at = NULL,
            updated_at = $4
        WHERE tenant_id = $1
          AND business_type = $2
          AND business_id = $3
          AND reservation_id = $5
          AND status = 'approved'
          AND consumed_at IS NULL
        "#,
    )
    .bind(reservation.tenant_id)
    .bind(RUNTIME_CREDENTIAL_APPROVAL_BUSINESS_TYPE)
    .bind(&reservation.business_id)
    .bind(now)
    .bind(reservation.reservation_id)
    .execute(pool)
    .await
    .map_err(|error| {
        tracing::error!(?error, "failed to consume runtime approval reservation");
        ApiErrorResponse::internal_error("Failed to consume runtime credential approval")
    })?;

    if consumed.rows_affected() == 1 {
        Ok(())
    } else {
        Err(ApiErrorResponse::internal_error(
            "Failed to consume runtime credential approval",
        ))
    }
}

fn waiting_for_approval_conflict(approval_id: Uuid) -> ApiErrorResponse {
    ApiErrorResponse::conflict(format!(
        "Runtime access is waiting for approval: {approval_id}"
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Cancelled,
}

impl ApprovalStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
        }
    }

    #[allow(clippy::result_large_err)]
    fn from_db(value: &str) -> Result<Self, ApiErrorResponse> {
        match value {
            "pending" => Ok(Self::Pending),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(ApiErrorResponse::internal_error(format!(
                "Unknown approval status in storage: {value}"
            ))),
        }
    }

    fn requires_remark(self) -> bool {
        matches!(self, Self::Rejected)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApprovalStatusFilter {
    Pending,
    Completed,
    Approved,
    Rejected,
    Cancelled,
}

impl ApprovalStatusFilter {
    #[allow(clippy::result_large_err)]
    fn from_query(value: &str) -> Result<Self, ApiErrorResponse> {
        match value {
            "pending" => Ok(Self::Pending),
            "completed" => Ok(Self::Completed),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(ApiErrorResponse::invalid_request(
                "status must be one of: pending, completed, approved, rejected, cancelled",
            )),
        }
    }

    fn resolved_statuses(self) -> &'static [&'static str] {
        match self {
            Self::Pending => &["pending"],
            Self::Completed => &["approved", "rejected", "cancelled"],
            Self::Approved => &["approved"],
            Self::Rejected => &["rejected"],
            Self::Cancelled => &["cancelled"],
        }
    }
}

#[derive(Debug, Deserialize)]
struct ListApprovalRequestsQuery {
    status: Option<String>,
    #[serde(default = "default_page")]
    page: usize,
    #[serde(default = "default_page_size")]
    page_size: usize,
}

fn default_page() -> usize {
    1
}

fn default_page_size() -> usize {
    20
}

#[derive(Debug)]
struct ApprovalRecord {
    approval_id: Uuid,
    tenant_id: Uuid,
    requested_by: Uuid,
    business_type: String,
    business_id: String,
    status: ApprovalStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    processed_by: Option<Uuid>,
    processed_at: Option<DateTime<Utc>>,
    remark: Option<String>,
    result_code: Option<String>,
    result_payload: Option<Value>,
    business_result_written_at: Option<DateTime<Utc>>,
}

impl ApprovalRecord {
    #[allow(clippy::result_large_err)]
    fn from_row(row: &sqlx::postgres::PgRow) -> Result<Self, ApiErrorResponse> {
        let status =
            ApprovalStatus::from_db(row.try_get::<&str, _>("status").map_err(|error| {
                ApiErrorResponse::internal_error(format!(
                    "Failed to read approval status from storage: {error}"
                ))
            })?)?;

        Ok(Self {
            approval_id: row.try_get("approval_id").map_err(internal_read_error)?,
            tenant_id: row.try_get("tenant_id").map_err(internal_read_error)?,
            requested_by: row.try_get("requested_by").map_err(internal_read_error)?,
            business_type: row.try_get("business_type").map_err(internal_read_error)?,
            business_id: row.try_get("business_id").map_err(internal_read_error)?,
            status,
            created_at: row.try_get("created_at").map_err(internal_read_error)?,
            updated_at: row.try_get("updated_at").map_err(internal_read_error)?,
            processed_by: row.try_get("processed_by").map_err(internal_read_error)?,
            processed_at: row.try_get("processed_at").map_err(internal_read_error)?,
            remark: row.try_get("remark").map_err(internal_read_error)?,
            result_code: row.try_get("result_code").map_err(internal_read_error)?,
            result_payload: row.try_get("result_payload").map_err(internal_read_error)?,
            business_result_written_at: row
                .try_get("business_result_written_at")
                .map_err(internal_read_error)?,
        })
    }
}

impl ApprovalBusinessResultRecord {
    #[allow(clippy::result_large_err)]
    fn from_row(row: &sqlx::postgres::PgRow) -> Result<Self, ApiErrorResponse> {
        let status =
            ApprovalStatus::from_db(row.try_get::<&str, _>("status").map_err(|error| {
                ApiErrorResponse::internal_error(format!(
                    "Failed to read approval business result status from storage: {error}"
                ))
            })?)?;

        Ok(Self {
            status,
            consumed_at: row.try_get("consumed_at").map_err(internal_read_error)?,
            reservation_id: row.try_get("reservation_id").map_err(internal_read_error)?,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateApprovalRequest {
    business_type: String,
    business_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProcessApprovalRequest {
    #[serde(default)]
    remark: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreateApprovalResponse {
    approval_id: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct ApprovalDetailResponse {
    approval_id: String,
    tenant_id: String,
    requested_by: String,
    business_type: String,
    business_id: String,
    status: String,
    created_at: String,
    updated_at: String,
    processed_by: Option<String>,
    processed_at: Option<String>,
    remark: Option<String>,
    result_code: Option<String>,
    result_payload: Option<Value>,
    business_result_written_at: Option<String>,
}

impl From<ApprovalRecord> for ApprovalDetailResponse {
    fn from(record: ApprovalRecord) -> Self {
        Self {
            approval_id: record.approval_id.to_string(),
            tenant_id: record.tenant_id.to_string(),
            requested_by: record.requested_by.to_string(),
            business_type: record.business_type,
            business_id: record.business_id,
            status: record.status.as_str().to_string(),
            created_at: record.created_at.to_rfc3339(),
            updated_at: record.updated_at.to_rfc3339(),
            processed_by: record.processed_by.map(|value| value.to_string()),
            processed_at: record.processed_at.map(|value| value.to_rfc3339()),
            remark: record.remark,
            result_code: record.result_code,
            result_payload: record.result_payload,
            business_result_written_at: record
                .business_result_written_at
                .map(|value| value.to_rfc3339()),
        }
    }
}

#[derive(Debug, Serialize)]
struct ApprovalCountsResponse {
    pending: usize,
    approved: usize,
    rejected: usize,
    cancelled: usize,
    completed: usize,
}

#[derive(Debug, Serialize)]
struct ApprovalListResponse {
    items: Vec<ApprovalDetailResponse>,
    page: usize,
    page_size: usize,
    total: usize,
    total_pages: usize,
    counts: ApprovalCountsResponse,
}

impl ApprovalListResponse {
    fn new(
        items: Vec<ApprovalDetailResponse>,
        page: usize,
        page_size: usize,
        total: usize,
        counts: ApprovalCountsResponse,
    ) -> Self {
        let paginated = PaginatedResponse::new(items, page, page_size, total);
        Self {
            items: paginated.items,
            page: paginated.page,
            page_size: paginated.page_size,
            total: paginated.total,
            total_pages: paginated.total_pages,
            counts,
        }
    }
}

struct ApprovalAuditEventInsert<'a> {
    approval_id: Uuid,
    tenant_id: Uuid,
    business_type: &'a str,
    business_id: &'a str,
    action: &'a str,
    outcome: &'a str,
    actor_id: Uuid,
    metadata: Value,
}

pub fn approval_routes(state: ApprovalApiState) -> Router {
    Router::new()
        .route(
            "/approvals",
            get(list_approval_requests_handler).post(create_approval_request_handler),
        )
        .route("/approvals/:approval_id", get(get_approval_request_handler))
        .route(
            "/approvals/:approval_id/approve",
            post(approve_approval_request_handler),
        )
        .route(
            "/approvals/:approval_id/reject",
            post(reject_approval_request_handler),
        )
        .route(
            "/approvals/:approval_id/cancel",
            post(cancel_approval_request_handler),
        )
        .with_state(state)
}

async fn list_approval_requests_handler(
    State(state): State<ApprovalApiState>,
    Extension(token): Extension<ValidatedToken>,
    Query(query): Query<ListApprovalRequestsQuery>,
) -> Result<Response, ApiErrorResponse> {
    if query.page == 0 {
        return Err(ApiErrorResponse::invalid_request("page must be >= 1"));
    }
    if query.page_size == 0 || query.page_size > 100 {
        return Err(ApiErrorResponse::invalid_request(
            "page_size must be between 1 and 100",
        ));
    }

    let (tenant_id, _) = require_tenant_admin(&token, "approval listing")?;
    let status_filter = query
        .status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ApprovalStatusFilter::from_query)
        .transpose()?;
    let counts = count_approval_records(&state.pool, tenant_id).await?;
    let (records, total) = list_approval_records(
        &state.pool,
        tenant_id,
        status_filter,
        query.page,
        query.page_size,
    )
    .await?;
    let items = records
        .into_iter()
        .map(ApprovalDetailResponse::from)
        .collect();

    Ok(success_response(
        StatusCode::OK,
        ApprovalListResponse::new(items, query.page, query.page_size, total, counts),
    ))
}

async fn create_approval_request_handler(
    State(state): State<ApprovalApiState>,
    Extension(token): Extension<ValidatedToken>,
    Json(request): Json<CreateApprovalRequest>,
) -> Result<Response, ApiErrorResponse> {
    let (tenant_id, requested_by) = require_tenant_admin(&token, "approval initiation")?;

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
    let mut tx = state.pool.begin().await.map_err(|error| {
        tracing::error!(?error, "failed to begin approval initiation transaction");
        ApiErrorResponse::internal_error("Failed to create approval request")
    })?;

    let insert_result = sqlx::query(
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
    .bind(ApprovalStatus::Pending.as_str())
    .execute(&mut *tx)
    .await;

    match insert_result {
        Ok(_) => {
            insert_approval_audit_event(
                &mut tx,
                ApprovalAuditEventInsert {
                    approval_id,
                    tenant_id,
                    business_type,
                    business_id,
                    action: "initiated",
                    outcome: APPROVAL_AUDIT_OUTCOME_SUCCESS,
                    actor_id: requested_by,
                    metadata: build_approval_audit_metadata(ApprovalStatus::Pending, None, false),
                },
            )
            .await?;

            tx.commit().await.map_err(|error| {
                tracing::error!(?error, "failed to commit approval initiation transaction");
                ApiErrorResponse::internal_error("Failed to create approval request")
            })?;

            Ok(success_response(
                StatusCode::CREATED,
                CreateApprovalResponse {
                    approval_id: approval_id.to_string(),
                    status: ApprovalStatus::Pending.as_str().to_string(),
                },
            ))
        }
        Err(error) if is_pending_business_conflict(&error) => {
            tx.rollback().await.map_err(|rollback_error| {
                tracing::error!(
                    ?rollback_error,
                    "failed to roll back approval initiation replay transaction"
                );
                ApiErrorResponse::internal_error("Failed to create approval request")
            })?;

            let mut replay_tx = state.pool.begin().await.map_err(|begin_error| {
                tracing::error!(
                    ?begin_error,
                    "failed to begin approval initiation replay transaction"
                );
                ApiErrorResponse::internal_error("Failed to create approval request")
            })?;

            let existing = fetch_pending_approval_record(
                &mut replay_tx,
                tenant_id,
                business_type,
                business_id,
            )
            .await?
            .ok_or_else(|| {
                ApiErrorResponse::conflict(
                    "Approval initiation replay detected but no pending request was found",
                )
            })?;

            insert_approval_audit_event(
                &mut replay_tx,
                ApprovalAuditEventInsert {
                    approval_id: existing.approval_id,
                    tenant_id,
                    business_type,
                    business_id,
                    action: "initiation_replayed",
                    outcome: APPROVAL_AUDIT_OUTCOME_REPLAYED,
                    actor_id: requested_by,
                    metadata: build_approval_audit_metadata(
                        existing.status,
                        existing.remark.as_deref(),
                        existing.business_result_written_at.is_some(),
                    ),
                },
            )
            .await?;

            replay_tx.commit().await.map_err(|commit_error| {
                tracing::error!(
                    ?commit_error,
                    "failed to commit approval initiation replay transaction"
                );
                ApiErrorResponse::internal_error("Failed to create approval request")
            })?;

            Ok(success_response(
                StatusCode::OK,
                CreateApprovalResponse {
                    approval_id: existing.approval_id.to_string(),
                    status: existing.status.as_str().to_string(),
                },
            ))
        }
        Err(error) => {
            tracing::error!(?error, "failed to create approval request");
            Err(ApiErrorResponse::internal_error(
                "Failed to create approval request",
            ))
        }
    }
}

async fn get_approval_request_handler(
    State(state): State<ApprovalApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(approval_id): Path<String>,
) -> Result<Response, ApiErrorResponse> {
    let (tenant_id, _) = require_tenant_admin(&token, "approval inspection")?;
    let approval_id = parse_uuid_str(&approval_id, "approval_id")?;

    let record = fetch_approval_record(&state.pool, tenant_id, approval_id)
        .await?
        .ok_or_else(|| ApiErrorResponse::not_found("Approval request not found"))?;

    Ok(success_response(
        StatusCode::OK,
        ApprovalDetailResponse::from(record),
    ))
}

async fn approve_approval_request_handler(
    State(state): State<ApprovalApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(approval_id): Path<String>,
    Json(request): Json<ProcessApprovalRequest>,
) -> Result<Response, ApiErrorResponse> {
    process_approval_request(
        &state.pool,
        &token,
        &approval_id,
        ApprovalStatus::Approved,
        request.remark,
    )
    .await
}

async fn reject_approval_request_handler(
    State(state): State<ApprovalApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(approval_id): Path<String>,
    Json(request): Json<ProcessApprovalRequest>,
) -> Result<Response, ApiErrorResponse> {
    process_approval_request(
        &state.pool,
        &token,
        &approval_id,
        ApprovalStatus::Rejected,
        request.remark,
    )
    .await
}

async fn cancel_approval_request_handler(
    State(state): State<ApprovalApiState>,
    Extension(token): Extension<ValidatedToken>,
    Path(approval_id): Path<String>,
    Json(request): Json<ProcessApprovalRequest>,
) -> Result<Response, ApiErrorResponse> {
    process_approval_request(
        &state.pool,
        &token,
        &approval_id,
        ApprovalStatus::Cancelled,
        request.remark,
    )
    .await
}

async fn process_approval_request(
    pool: &PgPool,
    token: &ValidatedToken,
    approval_id: &str,
    terminal_status: ApprovalStatus,
    remark: Option<String>,
) -> Result<Response, ApiErrorResponse> {
    let (tenant_id, processed_by) = require_tenant_admin(token, "approval processing")?;
    let approval_id = parse_uuid_str(approval_id, "approval_id")?;
    let record = transition_approval_request(
        pool,
        approval_id,
        tenant_id,
        processed_by,
        terminal_status,
        remark,
    )
    .await?;

    Ok(success_response(
        StatusCode::OK,
        ApprovalDetailResponse::from(record),
    ))
}

#[allow(clippy::result_large_err)]
fn require_tenant_admin(
    token: &ValidatedToken,
    action: &str,
) -> Result<(Uuid, Uuid), ApiErrorResponse> {
    if !token.scopes.contains(&TokenScope::TenantAdmin) {
        return Err(ApiErrorResponse::forbidden(format!(
            "Tenant admin scope is required for {action}"
        )));
    }

    Ok((
        parse_uuid_str(&token.tenant_id, "tenant_id")?,
        parse_uuid_str(&token.user_id, "user_id")?,
    ))
}

async fn fetch_approval_record(
    pool: &PgPool,
    tenant_id: Uuid,
    approval_id: Uuid,
) -> Result<Option<ApprovalRecord>, ApiErrorResponse> {
    let row = sqlx::query(
        r#"
        SELECT
            approval_id,
            tenant_id,
            requested_by,
            business_type,
            business_id,
            status,
            created_at,
            updated_at,
            processed_by,
            processed_at,
            remark,
            result_code,
            result_payload,
            business_result_written_at
        FROM approval_requests
        WHERE approval_id = $1
          AND tenant_id = $2
        "#,
    )
    .bind(approval_id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        tracing::error!(?error, "failed to fetch approval request");
        ApiErrorResponse::internal_error("Failed to fetch approval request")
    })?;

    match row {
        Some(value) => ApprovalRecord::from_row(&value).map(Some),
        None => Ok(None),
    }
}

async fn list_approval_records(
    pool: &PgPool,
    tenant_id: Uuid,
    status_filter: Option<ApprovalStatusFilter>,
    page: usize,
    page_size: usize,
) -> Result<(Vec<ApprovalRecord>, usize), ApiErrorResponse> {
    let offset = (page - 1) * page_size;
    let mut count_query = QueryBuilder::<Postgres>::new(
        "SELECT COUNT(*)::BIGINT FROM approval_requests WHERE tenant_id = ",
    );
    count_query.push_bind(tenant_id);
    if let Some(status_filter) = status_filter {
        count_query.push(" AND status IN (");
        let mut separated = count_query.separated(", ");
        for status in status_filter.resolved_statuses() {
            separated.push_bind(status);
        }
        separated.push_unseparated(")");
    }
    let total = count_query
        .build_query_scalar::<i64>()
        .fetch_one(pool)
        .await
        .map_err(|error| {
            tracing::error!(?error, "failed to count approval requests");
            ApiErrorResponse::internal_error("Failed to list approval requests")
        })? as usize;

    let mut list_query = QueryBuilder::<Postgres>::new(
        r#"
        SELECT
            approval_id,
            tenant_id,
            requested_by,
            business_type,
            business_id,
            status,
            created_at,
            updated_at,
            processed_by,
            processed_at,
            remark,
            result_code,
            result_payload,
            business_result_written_at
        FROM approval_requests
        WHERE tenant_id =
        "#,
    );
    list_query.push_bind(tenant_id);
    if let Some(status_filter) = status_filter {
        list_query.push(" AND status IN (");
        let mut separated = list_query.separated(", ");
        for status in status_filter.resolved_statuses() {
            separated.push_bind(status);
        }
        separated.push_unseparated(")");
    }
    list_query.push(
        r#"
        ORDER BY
            CASE
                WHEN status = 'pending' THEN 0
                WHEN status = 'approved' THEN 1
                WHEN status = 'rejected' THEN 2
                WHEN status = 'cancelled' THEN 3
                ELSE 4
            END,
            updated_at DESC,
            created_at DESC,
            approval_id DESC
        LIMIT
        "#,
    );
    list_query.push_bind(page_size as i64);
    list_query.push(" OFFSET ");
    list_query.push_bind(offset as i64);

    let rows = list_query.build().fetch_all(pool).await.map_err(|error| {
        tracing::error!(?error, "failed to list approval requests");
        ApiErrorResponse::internal_error("Failed to list approval requests")
    })?;

    let mut records = Vec::with_capacity(rows.len());
    for row in rows {
        records.push(ApprovalRecord::from_row(&row)?);
    }
    Ok((records, total))
}

async fn count_approval_records(
    pool: &PgPool,
    tenant_id: Uuid,
) -> Result<ApprovalCountsResponse, ApiErrorResponse> {
    let row = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE status = 'pending') AS pending_count,
            COUNT(*) FILTER (WHERE status = 'approved') AS approved_count,
            COUNT(*) FILTER (WHERE status = 'rejected') AS rejected_count,
            COUNT(*) FILTER (WHERE status = 'cancelled') AS cancelled_count
        FROM approval_requests
        WHERE tenant_id = $1
        "#,
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(|error| {
        tracing::error!(?error, "failed to count approval status buckets");
        ApiErrorResponse::internal_error("Failed to list approval requests")
    })?;

    let pending = row.0 as usize;
    let approved = row.1 as usize;
    let rejected = row.2 as usize;
    let cancelled = row.3 as usize;

    Ok(ApprovalCountsResponse {
        pending,
        approved,
        rejected,
        cancelled,
        completed: approved + rejected + cancelled,
    })
}

async fn fetch_pending_approval_record(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    business_type: &str,
    business_id: &str,
) -> Result<Option<ApprovalRecord>, ApiErrorResponse> {
    let row = sqlx::query(
        r#"
        SELECT
            approval_id,
            tenant_id,
            requested_by,
            business_type,
            business_id,
            status,
            created_at,
            updated_at,
            processed_by,
            processed_at,
            remark,
            result_code,
            result_payload,
            business_result_written_at
        FROM approval_requests
        WHERE tenant_id = $1
          AND business_type = $2
          AND business_id = $3
          AND status = 'pending'
        ORDER BY created_at DESC, approval_id DESC
        LIMIT 1
        FOR UPDATE
        "#,
    )
    .bind(tenant_id)
    .bind(business_type)
    .bind(business_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| {
        tracing::error!(?error, "failed to fetch pending approval request");
        ApiErrorResponse::internal_error("Failed to create approval request")
    })?;

    match row {
        Some(value) => ApprovalRecord::from_row(&value).map(Some),
        None => Ok(None),
    }
}

async fn fetch_approval_business_result(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    business_type: &str,
    business_id: &str,
) -> Result<Option<ApprovalBusinessResultRecord>, ApiErrorResponse> {
    let row = sqlx::query(
        r#"
        SELECT status, consumed_at, reservation_id
        FROM approval_business_results
        WHERE tenant_id = $1
          AND business_type = $2
          AND business_id = $3
        "#,
    )
    .bind(tenant_id)
    .bind(business_type)
    .bind(business_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| {
        tracing::error!(?error, "failed to fetch approval business result");
        ApiErrorResponse::internal_error("Failed to acquire runtime credential approval")
    })?;

    match row {
        Some(value) => ApprovalBusinessResultRecord::from_row(&value).map(Some),
        None => Ok(None),
    }
}

async fn transition_approval_request(
    pool: &PgPool,
    approval_id: Uuid,
    tenant_id: Uuid,
    processed_by: Uuid,
    terminal_status: ApprovalStatus,
    remark: Option<String>,
) -> Result<ApprovalRecord, ApiErrorResponse> {
    let normalized_remark = normalize_optional_string(remark);
    if terminal_status.requires_remark() && normalized_remark.is_none() {
        return Err(ApiErrorResponse::invalid_request(
            "remark is required when rejecting an approval request",
        ));
    }

    let mut tx = pool.begin().await.map_err(|error| {
        tracing::error!(?error, "failed to begin approval transition transaction");
        ApiErrorResponse::internal_error("Failed to update approval request")
    })?;

    let row = sqlx::query(
        r#"
        SELECT
            approval_id,
            tenant_id,
            requested_by,
            business_type,
            business_id,
            status,
            created_at,
            updated_at,
            processed_by,
            processed_at,
            remark,
            result_code,
            result_payload,
            business_result_written_at
        FROM approval_requests
        WHERE approval_id = $1
          AND tenant_id = $2
        FOR UPDATE
        "#,
    )
    .bind(approval_id)
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| {
        tracing::error!(?error, "failed to lock approval request for transition");
        ApiErrorResponse::internal_error("Failed to update approval request")
    })?;

    let current = match row {
        Some(value) => ApprovalRecord::from_row(&value)?,
        None => return Err(ApiErrorResponse::not_found("Approval request not found")),
    };

    if current.status == terminal_status {
        insert_approval_audit_event(
            &mut tx,
            ApprovalAuditEventInsert {
                approval_id: current.approval_id,
                tenant_id: current.tenant_id,
                business_type: &current.business_type,
                business_id: &current.business_id,
                action: replayed_transition_audit_action(terminal_status),
                outcome: APPROVAL_AUDIT_OUTCOME_REPLAYED,
                actor_id: processed_by,
                metadata: build_approval_audit_metadata(
                    current.status,
                    current.remark.as_deref(),
                    current.business_result_written_at.is_some(),
                ),
            },
        )
        .await?;

        tx.commit().await.map_err(|error| {
            tracing::error!(?error, "failed to commit idempotent approval transition");
            ApiErrorResponse::internal_error("Failed to update approval request")
        })?;
        return Ok(current);
    }

    if current.status != ApprovalStatus::Pending {
        return Err(ApiErrorResponse::conflict(format!(
            "Approval request is already in terminal status {}",
            current.status.as_str()
        )));
    }

    let processed_at = Utc::now();
    let result_payload = build_result_payload(
        &current,
        terminal_status,
        normalized_remark.as_deref(),
        processed_by,
        processed_at,
    );
    let result_code = terminal_status.as_str().to_string();

    let updated_row = sqlx::query(
        r#"
        UPDATE approval_requests
        SET status = $3,
            processed_by = $4,
            processed_at = $5,
            remark = $6,
            result_code = $7,
            result_payload = $8,
            business_result_written_at = $9,
            updated_at = $5
        WHERE approval_id = $1
          AND tenant_id = $2
        RETURNING
            approval_id,
            tenant_id,
            requested_by,
            business_type,
            business_id,
            status,
            created_at,
            updated_at,
            processed_by,
            processed_at,
            remark,
            result_code,
            result_payload,
            business_result_written_at
        "#,
    )
    .bind(approval_id)
    .bind(tenant_id)
    .bind(terminal_status.as_str())
    .bind(processed_by)
    .bind(processed_at)
    .bind(normalized_remark.as_deref())
    .bind(&result_code)
    .bind(SqlxJson(result_payload.clone()))
    .bind(processed_at)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| {
        tracing::error!(?error, "failed to persist approval terminal transition");
        ApiErrorResponse::internal_error("Failed to update approval request")
    })?;

    write_business_result(
        &mut tx,
        &current,
        approval_id,
        terminal_status,
        &result_code,
        &result_payload,
        processed_at,
    )
    .await?;

    insert_approval_audit_event(
        &mut tx,
        ApprovalAuditEventInsert {
            approval_id: current.approval_id,
            tenant_id: current.tenant_id,
            business_type: &current.business_type,
            business_id: &current.business_id,
            action: terminal_transition_audit_action(terminal_status),
            outcome: APPROVAL_AUDIT_OUTCOME_SUCCESS,
            actor_id: processed_by,
            metadata: build_approval_audit_metadata(
                terminal_status,
                normalized_remark.as_deref(),
                true,
            ),
        },
    )
    .await?;

    tx.commit().await.map_err(|error| {
        tracing::error!(?error, "failed to commit approval terminal transition");
        ApiErrorResponse::internal_error("Failed to update approval request")
    })?;

    ApprovalRecord::from_row(&updated_row)
}

async fn write_business_result(
    tx: &mut Transaction<'_, Postgres>,
    record: &ApprovalRecord,
    approval_id: Uuid,
    terminal_status: ApprovalStatus,
    result_code: &str,
    result_payload: &Value,
    updated_at: DateTime<Utc>,
) -> Result<(), ApiErrorResponse> {
    match record.business_type.as_str() {
        "oauth_binding_access" | "credential_runtime_access" => {
            sqlx::query(
                r#"
                INSERT INTO approval_business_results (
                    tenant_id,
                    business_type,
                    business_id,
                    approval_id,
                    status,
                    result_code,
                    result_payload,
                    consumed_at,
                    reservation_id,
                    reserved_at,
                    updated_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, NULL, NULL, NULL, $8)
                ON CONFLICT (tenant_id, business_type, business_id)
                DO UPDATE SET
                    approval_id = EXCLUDED.approval_id,
                    status = EXCLUDED.status,
                    result_code = EXCLUDED.result_code,
                    result_payload = EXCLUDED.result_payload,
                    consumed_at = NULL,
                    reservation_id = NULL,
                    reserved_at = NULL,
                    updated_at = EXCLUDED.updated_at
                "#,
            )
            .bind(record.tenant_id)
            .bind(&record.business_type)
            .bind(&record.business_id)
            .bind(approval_id)
            .bind(terminal_status.as_str())
            .bind(result_code)
            .bind(SqlxJson(result_payload.clone()))
            .bind(updated_at)
            .execute(&mut **tx)
            .await
            .map_err(|error| {
                tracing::error!(?error, "failed to persist approval business writeback");
                ApiErrorResponse::internal_error("Failed to write approval business result")
            })?;

            Ok(())
        }
        _ => Err(ApiErrorResponse::invalid_request(format!(
            "Unsupported approval business_type: {}",
            record.business_type
        ))),
    }
}

fn build_result_payload(
    record: &ApprovalRecord,
    terminal_status: ApprovalStatus,
    remark: Option<&str>,
    processed_by: Uuid,
    processed_at: DateTime<Utc>,
) -> Value {
    json!({
        "business_type": record.business_type,
        "business_id": record.business_id,
        "approval_id": record.approval_id,
        "status": terminal_status.as_str(),
        "approved": terminal_status == ApprovalStatus::Approved,
        "remark": remark,
        "processed_by": processed_by,
        "processed_at": processed_at.to_rfc3339(),
    })
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn is_pending_business_conflict(error: &sqlx::Error) -> bool {
    match error {
        sqlx::Error::Database(database_error) => {
            if database_error.code().as_deref() != Some("23505") {
                return false;
            }

            match database_error.constraint() {
                Some(constraint) => constraint == APPROVAL_PENDING_BUSINESS_UNIQUE_INDEX,
                None => true,
            }
        }
        _ => false,
    }
}

fn terminal_transition_audit_action(status: ApprovalStatus) -> &'static str {
    match status {
        ApprovalStatus::Approved => "approved",
        ApprovalStatus::Rejected => "rejected",
        ApprovalStatus::Cancelled => "cancelled",
        ApprovalStatus::Pending => "initiated",
    }
}

fn replayed_transition_audit_action(status: ApprovalStatus) -> &'static str {
    match status {
        ApprovalStatus::Approved => "approved_replayed",
        ApprovalStatus::Rejected => "rejected_replayed",
        ApprovalStatus::Cancelled => "cancelled_replayed",
        ApprovalStatus::Pending => "initiation_replayed",
    }
}

fn build_approval_audit_metadata(
    status: ApprovalStatus,
    remark: Option<&str>,
    business_result_written: bool,
) -> Value {
    json!({
        "status": status.as_str(),
        "remark_present": remark.is_some(),
        "remark_redacted": remark.is_some(),
        "business_result_written": business_result_written,
    })
}

async fn insert_approval_audit_event(
    tx: &mut Transaction<'_, Postgres>,
    event: ApprovalAuditEventInsert<'_>,
) -> Result<(), ApiErrorResponse> {
    sqlx::query(
        r#"
        INSERT INTO approval_audit_events (
            audit_id,
            approval_id,
            tenant_id,
            business_type,
            business_id,
            action,
            outcome,
            actor_id,
            metadata
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(event.approval_id)
    .bind(event.tenant_id)
    .bind(event.business_type)
    .bind(event.business_id)
    .bind(event.action)
    .bind(event.outcome)
    .bind(event.actor_id)
    .bind(SqlxJson(event.metadata))
    .execute(&mut **tx)
    .await
    .map_err(|error| {
        tracing::error!(?error, "failed to persist approval audit event");
        ApiErrorResponse::internal_error("Failed to persist approval audit event")
    })?;

    Ok(())
}

fn internal_read_error(error: sqlx::Error) -> ApiErrorResponse {
    ApiErrorResponse::internal_error(format!(
        "Failed to decode approval request from storage: {error}"
    ))
}
