//! Local Space Health API: HTTP transport, pure domain rules and SQLite storage.
#![deny(missing_docs)]

pub mod admin;
mod api;
pub mod database;
mod domain;
mod storage;

use axum::{
    extract::Request,
    http::{Method, StatusCode, Uri},
    middleware::{self, Next},
    response::Response,
    routing::any,
    Router,
};
use sqlx::SqlitePool;
use std::time::Instant;
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
        .layer(middleware::from_fn(log_api_request))
        .with_state(pool)
}

async fn log_api_request(request: Request, next: Next) -> Response {
    if request.uri().path() != "/api" && !request.uri().path().starts_with("/api/") {
        return next.run(request).await;
    }
    let method = request.method().clone();
    let uri = request.uri().clone();
    let started = Instant::now();
    let response = next.run(request).await;
    if std::env::var("REQUEST_LOG").as_deref() != Ok("0") {
        let line = request_log_line(
            &method,
            &uri,
            response.status(),
            started.elapsed().as_millis(),
        );
        if response.status().is_server_error() {
            eprintln!("{line} - API request failed; check storage diagnostics");
        } else {
            println!("{line}");
        }
    } else if response.status().is_server_error() {
        eprintln!(
            "API request failed with HTTP {}; check storage diagnostics",
            response.status().as_u16()
        );
    }
    response
}

fn request_log_line(method: &Method, uri: &Uri, status: StatusCode, duration_ms: u128) -> String {
    format!(
        "{method} {} {} {duration_ms}ms",
        uri.path(),
        status.as_u16()
    )
}

#[cfg(test)]
mod request_log_tests {
    use super::*;

    #[test]
    fn log_contains_method_path_status_and_duration_without_query_or_body() {
        let request = Request::builder()
            .method("POST")
            .uri("/api/measurements?private=query-secret")
            .body(axum::body::Body::from("body-secret value=72"))
            .unwrap();
        let line = request_log_line(request.method(), request.uri(), StatusCode::CREATED, 12);
        assert_eq!(line, "POST /api/measurements 201 12ms");
        for private in ["?", "private", "query-secret", "body-secret", "value", "72"] {
            assert!(!line.contains(private));
        }
    }
}
