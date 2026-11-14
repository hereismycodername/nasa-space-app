use super::{
    dto::{
        AssessmentResponse, HealthResponse, HistoryRequest, HistoryResponse, LatestRequest,
        LatestResponse, MeasurementResponse, NewMeasurementRequest, SeriesRequest, SeriesResponse,
        TaskResponse, TaskUpdateRequest,
    },
    error::{ApiError, ApiResult},
    extractors::{ApiJson, ApiPath, ApiQuery},
};
use crate::domain::history::{HistoryQuery, DEMO_ASTRONAUT_ID};
use crate::{domain, storage};
use axum::{extract::State, http::StatusCode, Json};
use chrono::Utc;
use sqlx::SqlitePool;
use std::collections::HashMap;

pub(crate) async fn health(State(pool): State<SqlitePool>) -> ApiResult<Json<HealthResponse>> {
    storage::check_connection(&pool).await?;
    Ok(Json(HealthResponse {
        status: "ok",
        storage: "sqlite",
    }))
}
pub(crate) async fn history(
    State(pool): State<SqlitePool>,
    ApiQuery(request): ApiQuery<HistoryRequest>,
) -> ApiResult<Json<HistoryResponse>> {
    let query = HistoryQuery::try_from(request)?;
    Ok(Json(
        storage::measurements::history(&pool, DEMO_ASTRONAUT_ID, &query)
            .await?
            .into(),
    ))
}

pub(crate) async fn latest_measurements(
    State(pool): State<SqlitePool>,
    ApiQuery(_): ApiQuery<LatestRequest>,
) -> ApiResult<Json<LatestResponse>> {
    let metrics = storage::measurements::latest_by_metric(&pool, DEMO_ASTRONAUT_ID).await?;
    Ok(Json(LatestResponse::from_data(
        DEMO_ASTRONAUT_ID,
        metrics,
        Utc::now(),
    )?))
}

pub(crate) async fn measurement_series(
    State(pool): State<SqlitePool>,
    ApiQuery(request): ApiQuery<SeriesRequest>,
) -> ApiResult<Json<SeriesResponse>> {
    let query = domain::series::SeriesQuery::try_from(request)?;
    let buckets = storage::series::aggregate(&pool, DEMO_ASTRONAUT_ID, &query).await?;
    Ok(Json(SeriesResponse::from_data(
        DEMO_ASTRONAUT_ID,
        query,
        buckets,
    )?))
}
pub(crate) async fn ingest(
    State(pool): State<SqlitePool>,
    ApiJson(request): ApiJson<NewMeasurementRequest>,
) -> ApiResult<(StatusCode, Json<MeasurementResponse>)> {
    let received_at = Utc::now();
    let input = domain::measurements::NewMeasurement::from(request);
    input.validate(received_at)?;
    let device = storage::measurements::device(&pool, &input.device_id)
        .await?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "UNKNOWN_DEVICE",
                "Unknown deviceId",
            )
            .with_field("deviceId")
        })?;
    let outcome =
        storage::measurements::insert_or_get(&pool, &input, &device, &received_at.to_rfc3339())
            .await?;
    let matches = outcome.measurement.matches_payload(&input).map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DATABASE_ERROR",
            "Database operation failed",
        )
    })?;
    if !matches {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "SEQUENCE_CONFLICT",
            "Sequence already used for a different measurement",
        ));
    }
    Ok((
        if outcome.created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(outcome.measurement.into()),
    ))
}
pub(crate) async fn assessments(
    State(pool): State<SqlitePool>,
    ApiQuery(_): ApiQuery<LatestRequest>,
) -> ApiResult<Json<AssessmentResponse>> {
    let metrics = storage::measurements::latest_by_metric(&pool, DEMO_ASTRONAUT_ID).await?;
    Ok(Json(AssessmentResponse::from_data(
        DEMO_ASTRONAUT_ID,
        &metrics,
        Utc::now(),
    )?))
}
pub(crate) async fn tasks(
    State(pool): State<SqlitePool>,
    ApiQuery(_): ApiQuery<HashMap<String, String>>,
) -> ApiResult<Json<Vec<TaskResponse>>> {
    Ok(Json(
        storage::tasks::list(&pool)
            .await?
            .into_iter()
            .map(Into::into)
            .collect(),
    ))
}
pub(crate) async fn update_task(
    State(pool): State<SqlitePool>,
    ApiPath(id): ApiPath<String>,
    ApiJson(input): ApiJson<TaskUpdateRequest>,
) -> ApiResult<Json<TaskResponse>> {
    let completed_at = domain::tasks::completion_time(input.completed, Utc::now());
    let task = storage::tasks::set_completion(&pool, &id, completed_at)
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "TASK_NOT_FOUND", "Task not found"))?;
    Ok(Json(task.into()))
}
pub(crate) async fn not_found() -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, "NOT_FOUND", "Unknown API route")
}
pub(crate) async fn method_not_allowed() -> ApiError {
    ApiError::new(
        StatusCode::METHOD_NOT_ALLOWED,
        "METHOD_NOT_ALLOWED",
        "HTTP method is not allowed for this route",
    )
}
