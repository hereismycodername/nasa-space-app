//! Normalize Axum's extractor errors without exposing rejected request bodies.
use super::error::ApiError;
use axum::{
    extract::{FromRequest, FromRequestParts, Path, Query, Request},
    http::{request::Parts, StatusCode},
    Json,
};
use serde::de::DeserializeOwned;

pub(crate) struct ApiJson<T>(pub T);
impl<T, S> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;
    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        Json::<T>::from_request(request, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(|error| {
                let (code, message) = match error.status() {
                    StatusCode::UNPROCESSABLE_ENTITY => (
                        "INVALID_BODY",
                        "JSON body does not match the request schema",
                    ),
                    StatusCode::UNSUPPORTED_MEDIA_TYPE => (
                        "UNSUPPORTED_MEDIA_TYPE",
                        "Content-Type must be application/json",
                    ),
                    StatusCode::PAYLOAD_TOO_LARGE => {
                        ("PAYLOAD_TOO_LARGE", "Request body exceeds the 2 MiB limit")
                    }
                    _ => ("INVALID_JSON", "Request body is not valid JSON"),
                };
                ApiError::new(error.status(), code, message)
            })
    }
}

pub(crate) struct ApiQuery<T>(pub T);
impl<T, S> FromRequestParts<S> for ApiQuery<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Query::<T>::from_request_parts(parts, state)
            .await
            .map(|Query(value)| Self(value))
            .map_err(|_| {
                ApiError::new(
                    StatusCode::BAD_REQUEST,
                    "INVALID_QUERY",
                    "Query parameters do not match the request schema",
                )
            })
    }
}

pub(crate) struct ApiPath<T>(pub T);
impl<T, S> FromRequestParts<S> for ApiPath<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Path::<T>::from_request_parts(parts, state)
            .await
            .map(|Path(value)| Self(value))
            .map_err(|_| {
                ApiError::new(
                    StatusCode::BAD_REQUEST,
                    "INVALID_PATH",
                    "Path parameters do not match the request schema",
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, routing::get, Router};
    use http_body_util::BodyExt;
    use serde::Deserialize;
    use tower::ServiceExt;

    #[derive(Deserialize)]
    struct Pagination {
        limit: u32,
    }

    #[tokio::test]
    async fn typed_query_rejection_uses_the_problem_envelope() {
        // Future filtered endpoints can reuse the extractor without changing their errors.
        let app = Router::new().route(
            "/",
            get(|ApiQuery(query): ApiQuery<Pagination>| async move { query.limit.to_string() }),
        );
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/?limit=invalid")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.headers()["content-type"],
            "application/problem+json"
        );
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let problem: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(problem["code"], "INVALID_QUERY");
    }
}
