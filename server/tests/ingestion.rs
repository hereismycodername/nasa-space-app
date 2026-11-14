use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use chrono::{Duration, Utc};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    SqlitePool,
};
use std::{path::PathBuf, sync::Arc};
use tokio::{sync::Barrier, task::JoinSet};
use tower::ServiceExt;

async fn setup() -> (Router, SqlitePool) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    (space_health_server::app(pool.clone(), "dist"), pool)
}

struct TemporaryDatabase(PathBuf);

impl Drop for TemporaryDatabase {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn setup_concurrent() -> (Router, SqlitePool, TemporaryDatabase) {
    let directory = TemporaryDatabase(
        std::env::temp_dir().join(format!("space-health-ingestion-{}", uuid::Uuid::new_v4())),
    );
    std::fs::create_dir(&directory.0).unwrap();
    let options = SqliteConnectOptions::new()
        .filename(directory.0.join("test.db"))
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .min_connections(5)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    (
        space_health_server::app(pool.clone(), "dist"),
        pool,
        directory,
    )
}

async fn call(app: &Router, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    call_raw(app, method, path, body.to_string()).await
}

async fn call_raw(app: &Router, method: &str, path: &str, body: String) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

fn sample() -> Value {
    json!({
        "deviceId": "simulator-01", "sessionId": "ingestion-test", "sequence": 0,
        "metric": "heartRate", "value": 72, "unit": "bpm",
    })
}

async fn row_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM measurements")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn race(app: &Router, payloads: Vec<Value>) -> Vec<(StatusCode, Value)> {
    let barrier = Arc::new(Barrier::new(payloads.len()));
    let mut pending = JoinSet::new();
    for payload in payloads {
        let app = app.clone();
        let barrier = Arc::clone(&barrier);
        pending.spawn(async move {
            barrier.wait().await;
            call(&app, "POST", "/api/measurements", payload).await
        });
    }
    let mut responses = Vec::new();
    while let Some(response) = pending.join_next().await {
        responses.push(response.unwrap());
    }
    responses
}

#[tokio::test]
async fn equivalent_timestamp_retries_return_the_entire_original_record() {
    let (app, pool) = setup().await;
    let mut input = sample();
    input["measuredAt"] = json!("2020-01-01T13:00:00.123450000+05:00");
    let (status, first) = call(&app, "POST", "/api/measurements", input.clone()).await;
    assert_eq!(status, StatusCode::CREATED);
    for time in [
        "2020-01-01T08:00:00.12345Z",
        "2020-01-01t08:00:00.123450000z",
        "2020-01-01T03:00:00.123450−05:00",
    ] {
        input["measuredAt"] = json!(time);
        let (status, repeated) = call(&app, "POST", "/api/measurements", input.clone()).await;
        assert_eq!(status, StatusCode::OK, "{time}: {repeated}");
        assert_eq!(
            repeated, first,
            "id, first receipt and original time text must survive"
        );
    }
    assert_eq!(row_count(&pool).await, 1);
}

#[tokio::test]
async fn different_instants_inside_one_millisecond_conflict_without_overwriting() {
    let (app, pool) = setup().await;
    let mut input = sample();
    input["measuredAt"] = json!("2020-01-01T08:00:00.123000001Z");
    let (_, first) = call(&app, "POST", "/api/measurements", input.clone()).await;
    input["measuredAt"] = json!("2020-01-01T13:00:00.123000002+05:00");
    let (status, error) = call(&app, "POST", "/api/measurements", input).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["code"], "SEQUENCE_CONFLICT");
    let (_, rows) = call(&app, "GET", "/api/measurements", Value::Null).await;
    assert_eq!(rows, json!([first]));
    assert_eq!(row_count(&pool).await, 1);
}

#[tokio::test]
async fn absent_and_null_time_are_equivalent_but_known_time_is_a_conflict() {
    let (app, pool) = setup().await;
    let mut input = sample();
    let (_, first) = call(&app, "POST", "/api/measurements", input.clone()).await;
    input["measuredAt"] = Value::Null;
    let (status, repeated) = call(&app, "POST", "/api/measurements", input.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(repeated, first);
    input["measuredAt"] = json!("2020-01-01T00:00:00Z");
    assert_eq!(
        call(&app, "POST", "/api/measurements", input.clone())
            .await
            .0,
        StatusCode::CONFLICT
    );
    input["sequence"] = json!(1);
    assert_eq!(
        call(&app, "POST", "/api/measurements", input.clone())
            .await
            .0,
        StatusCode::CREATED
    );
    input["measuredAt"] = Value::Null;
    assert_eq!(
        call(&app, "POST", "/api/measurements", input).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(row_count(&pool).await, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_equivalent_retries_create_exactly_one_row_and_one_created_response() {
    let (app, pool, _directory) = setup_concurrent().await;
    let payloads = (0..16)
        .map(|index| {
            let mut input = sample();
            input["measuredAt"] = json!(if index % 2 == 0 {
                "2020-01-01T13:00:00.123456789+05:00"
            } else {
                "2020-01-01T08:00:00.123456789Z"
            });
            input
        })
        .collect();
    let responses = race(&app, payloads).await;
    assert_eq!(
        responses
            .iter()
            .filter(|(status, _)| *status == StatusCode::CREATED)
            .count(),
        1
    );
    assert_eq!(
        responses
            .iter()
            .filter(|(status, _)| *status == StatusCode::OK)
            .count(),
        15,
        "{responses:?}"
    );
    let original = &responses
        .iter()
        .find(|(status, _)| *status == StatusCode::CREATED)
        .unwrap()
        .1;
    assert!(responses.iter().all(|(_, value)| value == original));
    assert_eq!(row_count(&pool).await, 1);
    pool.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_different_payloads_keep_the_winner_and_reject_the_other_payload() {
    let (app, pool, _directory) = setup_concurrent().await;
    let payloads = (0..16)
        .map(|index| {
            let mut input = sample();
            input["value"] = json!(if index % 2 == 0 { 72 } else { 99 });
            input
        })
        .collect();
    let responses = race(&app, payloads).await;
    assert_eq!(
        responses
            .iter()
            .filter(|(status, _)| *status == StatusCode::CREATED)
            .count(),
        1
    );
    assert_eq!(
        responses
            .iter()
            .filter(|(status, _)| *status == StatusCode::OK)
            .count(),
        7,
        "{responses:?}"
    );
    assert_eq!(
        responses
            .iter()
            .filter(|(status, _)| *status == StatusCode::CONFLICT)
            .count(),
        8,
        "{responses:?}"
    );
    let original = &responses
        .iter()
        .find(|(status, _)| *status == StatusCode::CREATED)
        .unwrap()
        .1;
    for (status, response) in &responses {
        if *status == StatusCode::CONFLICT {
            assert_eq!(response["code"], "SEQUENCE_CONFLICT");
        } else {
            assert_eq!(response, original);
        }
    }
    let (_, history) = call(&app, "GET", "/api/measurements", Value::Null).await;
    assert_eq!(history, json!([original]));
    assert_eq!(row_count(&pool).await, 1);
    pool.close().await;
}

#[tokio::test]
async fn a_delayed_old_measurement_is_saved_without_becoming_the_newest_or_fresh() {
    let (app, pool) = setup().await;
    let mut input = sample();
    input["measuredAt"] = json!("2020-01-02T00:00:00Z");
    let (_, recent) = call(&app, "POST", "/api/measurements", input.clone()).await;
    // Fix receipt order explicitly, independent of clock resolution on the host.
    sqlx::query("UPDATE measurements SET received_at='2020-01-02T00:00:00Z'")
        .execute(&pool)
        .await
        .unwrap();
    input["sequence"] = json!(1);
    input["measuredAt"] = json!("2020-01-01T00:00:00Z");
    let (status, delayed) = call(&app, "POST", "/api/measurements", input).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(delayed["timeBasis"], "measuredAt");
    let (_, history) = call(&app, "GET", "/api/measurements", Value::Null).await;
    assert_eq!(history[0]["id"], recent["id"]);
    assert_eq!(history[1], delayed);
    let (_, assessment) = call(&app, "GET", "/api/assessments", Value::Null).await;
    assert_eq!(assessment["status"], "stale");
    assert_eq!(row_count(&pool).await, 2);
}

#[tokio::test]
async fn client_cannot_assign_any_server_owned_measurement_field() {
    let (app, pool) = setup().await;
    for (field, value) in [
        ("id", json!("forged-id")),
        ("astronautId", json!("another-person")),
        ("source", json!("sensor")),
        ("receivedAt", json!("2020-01-01T00:00:00Z")),
        ("timeBasis", json!("receivedAt")),
    ] {
        let mut input = sample();
        input[field] = value;
        let (status, error) = call(&app, "POST", "/api/measurements", input).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{field}: {error}");
        assert_eq!(error["code"], "INVALID_BODY");
    }
    assert_eq!(row_count(&pool).await, 0);
}

#[tokio::test]
async fn invalid_inputs_and_unknown_devices_never_write_a_measurement() {
    let (app, pool) = setup().await;
    for (field, value, code) in [
        ("deviceId", json!("not-registered"), "UNKNOWN_DEVICE"),
        ("deviceId", json!(""), "VALIDATION_ERROR"),
        ("sessionId", json!("ё".repeat(65)), "VALIDATION_ERROR"),
        ("sequence", json!(-1), "VALIDATION_ERROR"),
        ("metric", json!("unsupported"), "VALIDATION_ERROR"),
        ("unit", json!("h"), "VALIDATION_ERROR"),
        ("value", json!(0), "VALIDATION_ERROR"),
        ("value", json!(-1), "VALIDATION_ERROR"),
        (
            "measuredAt",
            json!("2020-02-30T00:00:00Z"),
            "VALIDATION_ERROR",
        ),
        (
            "measuredAt",
            json!("2020-01-01T00:00:00"),
            "VALIDATION_ERROR",
        ),
        (
            "measuredAt",
            json!((Utc::now() + Duration::hours(1)).to_rfc3339()),
            "VALIDATION_ERROR",
        ),
    ] {
        let mut input = sample();
        input[field] = value;
        let (status, error) = call(&app, "POST", "/api/measurements", input).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{field}: {error}");
        assert_eq!(error["code"], code);
        assert_eq!(error["field"], field);
    }
    assert_eq!(row_count(&pool).await, 0);
}

#[tokio::test]
async fn finite_metric_boundaries_are_enforced_without_clinical_thresholds() {
    let (app, pool) = setup().await;
    for (sequence, (metric, unit, value, expected)) in [
        ("sleepHours", "h", 0.0, StatusCode::CREATED),
        ("sleepHours", "h", 24.0, StatusCode::CREATED),
        ("sleepHours", "h", 24.01, StatusCode::BAD_REQUEST),
        ("sleepHours", "h", -0.01, StatusCode::BAD_REQUEST),
        ("mood", "score", 1.0, StatusCode::CREATED),
        ("mood", "score", 5.0, StatusCode::CREATED),
        ("mood", "score", 0.99, StatusCode::BAD_REQUEST),
        ("mood", "score", 5.01, StatusCode::BAD_REQUEST),
        ("temperature", "°C", 36.5, StatusCode::CREATED),
        ("systolicPressure", "mmHg", 120.0, StatusCode::CREATED),
        ("diastolicPressure", "mmHg", 80.0, StatusCode::CREATED),
    ]
    .into_iter()
    .enumerate()
    {
        let mut input = sample();
        input["sequence"] = json!(sequence);
        input["metric"] = json!(metric);
        input["unit"] = json!(unit);
        input["value"] = json!(value);
        let (status, body) = call(&app, "POST", "/api/measurements", input).await;
        assert_eq!(status, expected, "{metric} {value}: {body}");
    }
    assert_eq!(row_count(&pool).await, 7);
}

#[tokio::test]
async fn non_finite_json_numbers_are_rejected_before_storage() {
    let (app, pool) = setup().await;
    for value in ["NaN", "Infinity", "-Infinity", "1e400"] {
        let body = format!(
            r#"{{"deviceId":"simulator-01","sessionId":"invalid-numbers","sequence":0,"metric":"heartRate","value":{value},"unit":"bpm"}}"#
        );
        let (status, _) = call_raw(&app, "POST", "/api/measurements", body).await;
        assert!(
            matches!(
                status,
                StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY
            ),
            "{value}: {status}"
        );
    }
    assert_eq!(row_count(&pool).await, 0);
}

#[tokio::test]
async fn failed_insert_returns_a_database_error_without_acknowledging_storage() {
    let (app, pool) = setup().await;
    sqlx::query("CREATE TRIGGER reject_measurement BEFORE INSERT ON measurements BEGIN SELECT RAISE(ABORT, 'test insert failure'); END")
        .execute(&pool).await.unwrap();
    let (status, error) = call(&app, "POST", "/api/measurements", sample()).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(error["code"], "DATABASE_ERROR");
    assert_eq!(row_count(&pool).await, 0);
}

#[tokio::test]
async fn failure_loading_inserted_measurement_rolls_back_the_entire_write() {
    let (app, pool) = setup().await;
    sqlx::query("CREATE TABLE write_audit (measurement_id TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TRIGGER remove_inserted_measurement AFTER INSERT ON measurements BEGIN INSERT INTO write_audit VALUES (NEW.id); DELETE FROM measurements WHERE id=NEW.id; END")
        .execute(&pool).await.unwrap();
    let (status, error) = call(&app, "POST", "/api/measurements", sample()).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(error["code"], "DATABASE_ERROR");
    assert_eq!(row_count(&pool).await, 0);
    let side_effects: i64 = sqlx::query_scalar("SELECT count(*) FROM write_audit")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        side_effects, 0,
        "a failed read must not commit insert side effects"
    );
    sqlx::query("DROP TRIGGER remove_inserted_measurement")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(&app, "POST", "/api/measurements", sample()).await.0,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn commit_failure_returns_an_error_and_rolls_back_the_measurement() {
    let (app, pool) = setup().await;
    sqlx::query("CREATE TABLE commit_parent (id TEXT PRIMARY KEY)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE commit_guard (measurement_id TEXT NOT NULL REFERENCES commit_parent(id) DEFERRABLE INITIALLY DEFERRED)")
        .execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER fail_measurement_commit AFTER INSERT ON measurements BEGIN INSERT INTO commit_guard VALUES (NEW.id); END")
        .execute(&pool).await.unwrap();
    let (status, error) = call(&app, "POST", "/api/measurements", sample()).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(error["code"], "DATABASE_ERROR");
    assert_eq!(row_count(&pool).await, 0);
    let side_effects: i64 = sqlx::query_scalar("SELECT count(*) FROM commit_guard")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(side_effects, 0);
    sqlx::query("DROP TRIGGER fail_measurement_commit")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        call(&app, "POST", "/api/measurements", sample()).await.0,
        StatusCode::CREATED
    );
}
