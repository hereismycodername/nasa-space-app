use super::{
    dto::{
        AssessmentResponse, HealthResponse, HistoryQuery, LatestMeasurementsResponse,
        MeasurementPageResponse, MeasurementResponse, NewMeasurementRequest, SeriesQuery,
        SeriesResponse, TaskResponse, TaskUpdateRequest,
    },
    error::{ApiError, ApiResult},
    extractors::{ApiJson, ApiPath, ApiQuery},
};
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
    ApiQuery(query): ApiQuery<HistoryQuery>,
) -> ApiResult<Json<MeasurementPageResponse>> {
    let filter = domain::measurements::HistoryFilter::try_from(query)?;
    Ok(Json(
        storage::measurements::history(&pool, &filter).await?.into(),
    ))
}
pub(crate) async fn latest(
    State(pool): State<SqlitePool>,
) -> ApiResult<Json<LatestMeasurementsResponse>> {
    let metrics = storage::measurements::latest_per_metric(&pool, "demo-astronaut").await?;
    Ok(Json(LatestMeasurementsResponse {
        metrics: metrics.into_iter().map(Into::into).collect(),
    }))
}
pub(crate) async fn series(
    State(pool): State<SqlitePool>,
    ApiQuery(params): ApiQuery<Vec<(String, String)>>,
) -> ApiResult<Json<SeriesResponse>> {
    let filter = domain::measurements::SeriesFilter::try_from(SeriesQuery::try_from(params)?)?;
    let data = storage::measurements::series(&pool, &filter).await?;
    Ok(Json(SeriesResponse::new(&filter, data)))
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
) -> ApiResult<Json<AssessmentResponse>> {
    let latest = storage::measurements::latest_received(&pool).await?;
    Ok(Json(
        domain::assessments::assess(latest.as_ref(), Utc::now())?.into(),
    ))
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
