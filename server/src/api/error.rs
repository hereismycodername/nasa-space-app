use crate::domain::{
    assessments::InvalidStoredTimestamp, measurements::ValidationError, series::InvalidSeriesData,
};
use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

pub(crate) type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug)]
pub(crate) struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: &'static str,
    field: Option<&'static str>,
}
impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: &'static str) -> Self {
        Self {
            status,
            code,
            message,
            field: None,
        }
    }
    pub fn with_field(mut self, field: &'static str) -> Self {
        self.field = Some(field);
        self
    }
    pub fn task_body_error(mut self) -> Self {
        if self.status == StatusCode::UNPROCESSABLE_ENTITY {
            self.status = StatusCode::BAD_REQUEST;
        }
        self
    }
}

#[derive(Serialize)]
struct Problem {
    #[serde(rename = "type")]
    kind: String,
    title: &'static str,
    status: u16,
    detail: &'static str,
    code: &'static str,
    message: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    field: Option<&'static str>,
    // Kept for existing consumers of the pre-BE-01 error envelope.
    error: &'static str,
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let problem = Problem {
            kind: format!(
                "urn:space-health:error:{}",
                self.code.to_ascii_lowercase().replace('_', "-")
            ),
            title: self.status.canonical_reason().unwrap_or("Request failed"),
            status: self.status.as_u16(),
            detail: self.message,
            code: self.code,
            message: self.message,
            field: self.field,
            error: self.message,
        };
        let mut response = (self.status, Json(problem)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}
impl From<ValidationError> for ApiError {
    fn from(error: ValidationError) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "VALIDATION_ERROR", error.message)
            .with_field(error.field)
    }
}
impl From<InvalidStoredTimestamp> for ApiError {
    fn from(_: InvalidStoredTimestamp) -> Self {
        eprintln!(
            "Storage error: invalid timestamp in a stored measurement; check database integrity"
        );
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DATABASE_ERROR",
            "Database operation failed",
        )
    }
}
impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        eprintln!("database error: {error}");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DATABASE_ERROR",
            "Database operation failed",
        )
    }
}

impl From<InvalidSeriesData> for ApiError {
    fn from(_: InvalidSeriesData) -> Self {
        eprintln!(
            "Storage error: invalid aggregate in a measurement series; check database integrity"
        );
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "DATABASE_ERROR",
            "Database operation failed",
        )
    }
}
