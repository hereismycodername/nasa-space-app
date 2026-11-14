//! Local Space Health API: HTTP transport, pure domain rules and SQLite storage.
#![deny(missing_docs)]

mod api;
mod domain;
mod storage;

use axum::{routing::any, Router};
use sqlx::SqlitePool;
use tower_http::services::ServeDir;

/// Build the API and static React service using an already migrated SQLite pool.
///
/// `dist` is the directory containing the production frontend build. API paths
/// are mounted under `/api`; missing API routes never fall through to static files.
pub fn app(pool: SqlitePool, dist: &str) -> Router {
    Router::new()
        // Axum nests the inner "/" at "/api"; cover the trailing slash explicitly.
        .route("/api/", any(api::not_found))
        .nest("/api", api::routes())
        .fallback_service(ServeDir::new(dist))
        .with_state(pool)
}
