//! HTTP routes, DTOs, extractors and problem responses.
mod dto;
mod error;
mod extractors;
mod handlers;

pub(crate) use handlers::not_found;

use axum::{
    extract::DefaultBodyLimit,
    routing::{any, get, patch},
    Router,
};
use sqlx::SqlitePool;

pub(crate) fn routes() -> Router<SqlitePool> {
    Router::new()
        .route("/", any(handlers::not_found))
        .route("/health", get(handlers::health))
        .route("/measurements/series", get(handlers::series))
        .route(
            "/measurements",
            get(handlers::history).post(handlers::ingest),
        )
        .route("/assessments", get(handlers::assessments))
        .route("/measurements/latest", get(handlers::latest))
        .route("/tasks", get(handlers::tasks))
        .route("/tasks/{id}", patch(handlers::update_task))
        .fallback(handlers::not_found)
        .method_not_allowed_fallback(handlers::method_not_allowed)
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
}
