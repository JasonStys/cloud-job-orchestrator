//! File: Defines the versioned Axum control-plane API and HTTP error contract.
//! Functions: `router` assembles routes; handlers validate input and delegate transactions to `PostgresStore`.
//! Variables: `AppState` contains only a cloneable pool wrapper; request bodies have Axum's default size limit.

use crate::{DagSpec, JobState, PostgresStore, StoreError};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderName, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    services::ServeDir,
    trace::TraceLayer,
};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct AppState {
    pub store: PostgresStore,
}

/// Constructs API, health, metrics, and static-dashboard routes with request tracing.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/metrics", get(metrics))
        .route("/v1/dags", post(submit).get(list_dags))
        .route("/v1/dags/{dag_id}", get(snapshot))
        .route("/v1/dags/{dag_id}/cancel", post(cancel))
        .route("/v1/leases/claim", post(claim))
        .route("/v1/leases/heartbeat", post(heartbeat))
        .route("/v1/leases/complete", post(complete))
        .route("/v1/leases/fail", post(fail))
        .nest_service(
            "/",
            ServeDir::new("web/public").append_index_html_on_directories(true),
        )
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::new(
            HeaderName::from_static("x-request-id"),
            MakeRequestUuid,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn live() -> Json<Value> {
    Json(json!({"status": "live"}))
}

async fn ready(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    if state.store.ready().await {
        Ok(Json(json!({"status": "ready"})))
    } else {
        Err(ApiError::Unavailable)
    }
}

async fn metrics(State(state): State<AppState>) -> Result<Response, ApiError> {
    if !state.store.ready().await {
        return Err(ApiError::Unavailable);
    }
    Ok((
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        "# HELP orchestrator_ready Database readiness\n# TYPE orchestrator_ready gauge\norchestrator_ready 1\n",
    )
        .into_response())
}

async fn submit(
    State(state): State<AppState>,
    Json(spec): Json<DagSpec>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    state.store.submit(&spec).await?;
    Ok((StatusCode::CREATED, Json(json!({"dag_id": spec.id}))))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    after: Option<String>,
    #[serde(default = "default_page_size")]
    limit: u16,
}

const fn default_page_size() -> u16 {
    25
}

async fn list_dags(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, ApiError> {
    let dags = state
        .store
        .list_dags(query.after.as_deref(), query.limit)
        .await?;
    let next = dags.last().cloned();
    Ok(Json(json!({"items": dags, "next_cursor": next})))
}

async fn snapshot(
    State(state): State<AppState>,
    Path(dag_id): Path<String>,
) -> Result<Json<crate::DagSnapshot>, ApiError> {
    Ok(Json(state.store.snapshot(&dag_id).await?))
}

async fn cancel(
    State(state): State<AppState>,
    Path(dag_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let cancelled = state.store.cancel(&dag_id).await?;
    Ok(Json(json!({"dag_id": dag_id, "cancelled_jobs": cancelled})))
}

#[derive(Debug, Deserialize)]
struct ClaimRequest {
    owner: String,
    #[serde(default = "default_lease_ms")]
    lease_ms: i64,
}

const fn default_lease_ms() -> i64 {
    15_000
}

async fn claim(
    State(state): State<AppState>,
    Json(request): Json<ClaimRequest>,
) -> Result<Json<Value>, ApiError> {
    let claim = state.store.claim(&request.owner, request.lease_ms).await?;
    Ok(Json(json!({"job": claim})))
}

#[derive(Debug, Deserialize)]
struct LeaseRequest {
    dag_id: String,
    job_id: String,
    owner: String,
    lease_token: Uuid,
    #[serde(default = "default_lease_ms")]
    lease_ms: i64,
}

async fn heartbeat(
    State(state): State<AppState>,
    Json(request): Json<LeaseRequest>,
) -> Result<Json<Value>, ApiError> {
    state
        .store
        .heartbeat(
            &request.dag_id,
            &request.job_id,
            &request.owner,
            request.lease_token,
            request.lease_ms,
        )
        .await?;
    Ok(Json(json!({"status": "extended"})))
}

#[derive(Debug, Deserialize)]
struct CompleteRequest {
    dag_id: String,
    job_id: String,
    owner: String,
    lease_token: Uuid,
    result: String,
}

async fn complete(
    State(state): State<AppState>,
    Json(request): Json<CompleteRequest>,
) -> Result<Json<Value>, ApiError> {
    state
        .store
        .complete(
            &request.dag_id,
            &request.job_id,
            &request.owner,
            request.lease_token,
            &request.result,
        )
        .await?;
    Ok(Json(json!({"status": "succeeded"})))
}

async fn fail(
    State(state): State<AppState>,
    Json(request): Json<LeaseRequest>,
) -> Result<Json<Value>, ApiError> {
    let state = state
        .store
        .fail(
            &request.dag_id,
            &request.job_id,
            &request.owner,
            request.lease_token,
        )
        .await?;
    Ok(Json(json!({"status": state_name(&state)})))
}

const fn state_name(state: &JobState) -> &'static str {
    match state {
        JobState::Pending => "pending",
        JobState::Ready => "ready",
        JobState::RetryWait => "retry_wait",
        JobState::Leased => "leased",
        JobState::Succeeded => "succeeded",
        JobState::DeadLettered => "dead_lettered",
        JobState::Cancelled => "cancelled",
    }
}

#[derive(Debug)]
enum ApiError {
    Store(StoreError),
    Unavailable,
}

impl From<StoreError> for ApiError {
    fn from(value: StoreError) -> Self {
        Self::Store(value)
    }
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "unavailable",
                "database is not ready".to_owned(),
            ),
            Self::Store(StoreError::NotFound) => (
                StatusCode::NOT_FOUND,
                "not_found",
                "resource not found".to_owned(),
            ),
            Self::Store(StoreError::LeaseConflict) => (
                StatusCode::CONFLICT,
                "lease_conflict",
                "lease is stale or owned by another worker".to_owned(),
            ),
            Self::Store(StoreError::Validation(error)) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_dag",
                error.to_string(),
            ),
            Self::Store(StoreError::InvalidConfiguration) => (
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "request is outside supported limits".to_owned(),
            ),
            Self::Store(StoreError::Database(error)) if is_unique_violation(&error) => (
                StatusCode::CONFLICT,
                "already_exists",
                "resource already exists".to_owned(),
            ),
            Self::Store(error) => {
                tracing::error!(error = %error, "control-plane request failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "internal control-plane error".to_owned(),
                )
            }
        };
        (status, Json(ErrorBody { code, message })).into_response()
    }
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .as_deref()
        == Some("23505")
}
