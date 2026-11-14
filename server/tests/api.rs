use axum::{
    body::Body,
    http::{HeaderMap, Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::sqlite::SqlitePoolOptions;
use tower::ServiceExt;

async fn setup_with_pool() -> (Router, sqlx::SqlitePool) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    (space_health_server::app(pool.clone(), "dist"), pool)
}

async fn setup() -> Router {
    setup_with_pool().await.0
}
async fn call(app: &Router, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
fn sample() -> Value {
    json!({"deviceId":"simulator-01","sessionId":"test-session","sequence":0,"metric":"heartRate","value":72,"unit":"bpm"})
}
#[tokio::test]
async fn ingestion_is_idempotent_and_rejects_sequence_conflicts() {
    let app = setup().await;
    let (status, first) = call(&app, "POST", "/api/measurements", sample()).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(first["source"], "simulator");
    assert_eq!(first["astronautId"], "demo-astronaut");
    assert!(first["receivedAt"].is_string());
    assert!(first["measuredAt"].is_null());
    assert_eq!(first["timeBasis"], "receivedAt");
    let (status, retry) = call(&app, "POST", "/api/measurements", sample()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["id"], retry["id"]);
    assert_eq!(
        first, retry,
        "a retry must preserve the original record and timestamps"
    );
    let mut conflict = sample();
    conflict["value"] = json!(99);
    assert_eq!(
        call(&app, "POST", "/api/measurements", conflict).await.0,
        StatusCode::CONFLICT
    );
    let (_, history) = call(&app, "GET", "/api/measurements", Value::Null).await;
    assert_eq!(history["items"].as_array().unwrap().len(), 1);
}
#[tokio::test]
async fn validation_and_server_owned_identity() {
    let app = setup().await;
    for (key, value) in [
        ("unit", json!("h")),
        ("deviceId", json!("unknown")),
        ("sequence", json!(-1)),
        ("measuredAt", json!("yesterday")),
        ("value", json!(-1)),
    ] {
        let mut input = sample();
        input[key] = value;
        assert_eq!(
            call(&app, "POST", "/api/measurements", input).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    let mut forged = sample();
    forged["astronautId"] = json!("another-person");
    assert_eq!(
        call(&app, "POST", "/api/measurements", forged).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}
#[tokio::test]
async fn data_freshness_and_tasks() {
    let app = setup().await;
    assert_eq!(
        call(&app, "GET", "/api/assessments", Value::Null).await.1["status"],
        "no-data"
    );
    let mut old = sample();
    old["measuredAt"] = json!("2020-01-01T00:00:00Z");
    call(&app, "POST", "/api/measurements", old).await;
    assert_eq!(
        call(&app, "GET", "/api/assessments", Value::Null).await.1["status"],
        "stale"
    );
    let (_, done) = call(
        &app,
        "PATCH",
        "/api/tasks/daily-check",
        json!({"missionDate":"2026-11-14","completed":true}),
    )
    .await;
    assert!(done["completedAt"].is_string());
    assert_eq!(
        call(
            &app,
            "PATCH",
            "/api/tasks/missing",
            json!({"missionDate":"2026-11-14","completed":true})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(&app, "GET", "/api/missing", Value::Null).await.0,
        StatusCode::NOT_FOUND
    );
}

async fn call_raw(
    app: &Router,
    method: &str,
    path: &str,
    body: String,
    content_type: Option<&str>,
) -> (StatusCode, HeaderMap, Value) {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(content_type) = content_type {
        builder = builder.header("content-type", content_type);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let payload = serde_json::from_slice(&bytes).expect("API responses must be JSON");
    (status, headers, payload)
}

fn assert_problem(
    response: &(StatusCode, HeaderMap, Value),
    expected_status: StatusCode,
    expected_code: &str,
) {
    let (status, headers, body) = response;
    assert_eq!(*status, expected_status);
    assert_eq!(headers["content-type"], "application/problem+json");
    assert_eq!(body["status"], expected_status.as_u16());
    assert_eq!(body["code"], expected_code);
    assert_eq!(
        body["type"],
        format!(
            "urn:space-health:error:{}",
            expected_code.to_lowercase().replace('_', "-")
        )
    );
    assert_eq!(body["title"], expected_status.canonical_reason().unwrap());
    assert!(!body["message"].as_str().unwrap().is_empty());
    assert_eq!(body["detail"], body["message"]);
    assert_eq!(
        body["error"], body["message"],
        "legacy error field remains available"
    );
}

#[tokio::test]
async fn malformed_json_returns_a_problem_response() {
    let app = setup().await;
    let response = call_raw(
        &app,
        "POST",
        "/api/measurements",
        "{broken".into(),
        Some("application/json"),
    )
    .await;
    assert_problem(&response, StatusCode::BAD_REQUEST, "INVALID_JSON");
}

#[tokio::test]
async fn invalid_body_shape_returns_a_problem_response() {
    let app = setup().await;
    let mut input = sample();
    input["astronautId"] = json!("not-client-owned");
    let response = call_raw(
        &app,
        "POST",
        "/api/measurements",
        input.to_string(),
        Some("application/json"),
    )
    .await;
    assert_problem(&response, StatusCode::UNPROCESSABLE_ENTITY, "INVALID_BODY");
}

#[tokio::test]
async fn missing_content_type_returns_a_problem_response() {
    let app = setup().await;
    let response = call_raw(
        &app,
        "POST",
        "/api/measurements",
        sample().to_string(),
        None,
    )
    .await;
    assert_problem(
        &response,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "UNSUPPORTED_MEDIA_TYPE",
    );
}

#[tokio::test]
async fn oversized_body_returns_a_problem_response() {
    let app = setup().await;
    let mut input = sample();
    input["sessionId"] = json!("x".repeat(2 * 1024 * 1024));
    let response = call_raw(
        &app,
        "POST",
        "/api/measurements",
        input.to_string(),
        Some("application/json"),
    )
    .await;
    assert_problem(
        &response,
        StatusCode::PAYLOAD_TOO_LARGE,
        "PAYLOAD_TOO_LARGE",
    );
}

#[tokio::test]
async fn validation_exposes_a_stable_code_and_field_without_writing_data() {
    let app = setup().await;
    let mut input = sample();
    input["unit"] = json!("h");
    let response = call_raw(
        &app,
        "POST",
        "/api/measurements",
        input.to_string(),
        Some("application/json"),
    )
    .await;
    assert_problem(&response, StatusCode::BAD_REQUEST, "VALIDATION_ERROR");
    assert_eq!(response.2["field"], "unit");
    let (_, history) = call(&app, "GET", "/api/measurements", Value::Null).await;
    assert_eq!(
        history,
        json!({"items": [], "hasMore": false, "nextCursor": null})
    );
}

#[tokio::test]
async fn unknown_device_has_its_own_error_code() {
    let app = setup().await;
    let mut input = sample();
    input["deviceId"] = json!("unregistered");
    let response = call_raw(
        &app,
        "POST",
        "/api/measurements",
        input.to_string(),
        Some("application/json"),
    )
    .await;
    assert_problem(&response, StatusCode::BAD_REQUEST, "UNKNOWN_DEVICE");
    assert_eq!(response.2["field"], "deviceId");
}

#[tokio::test]
async fn missing_routes_and_tasks_use_distinct_problem_codes() {
    let app = setup().await;
    for path in ["/api/missing", "/api", "/api/"] {
        let missing_route = call_raw(&app, "GET", path, String::new(), None).await;
        assert_problem(&missing_route, StatusCode::NOT_FOUND, "NOT_FOUND");
    }
    let missing_task = call_raw(
        &app,
        "PATCH",
        "/api/tasks/missing",
        json!({"missionDate":"2026-11-14","completed":true}).to_string(),
        Some("application/json"),
    )
    .await;
    assert_problem(&missing_task, StatusCode::NOT_FOUND, "TASK_NOT_FOUND");
}

#[tokio::test]
async fn method_not_allowed_is_structured_and_preserves_allow_header() {
    let app = setup().await;
    let response = call_raw(&app, "PUT", "/api/measurements", String::new(), None).await;
    assert_problem(
        &response,
        StatusCode::METHOD_NOT_ALLOWED,
        "METHOD_NOT_ALLOWED",
    );
    let allowed: Vec<_> = response.1["allow"]
        .to_str()
        .unwrap()
        .split(',')
        .map(str::trim)
        .collect();
    assert!(allowed.contains(&"GET"));
    assert!(allowed.contains(&"POST"));
}

#[tokio::test]
async fn invalid_path_encoding_returns_a_problem_response() {
    let app = setup().await;
    let response = call_raw(
        &app,
        "PATCH",
        "/api/tasks/%FF",
        json!({"missionDate":"2026-11-14","completed":true}).to_string(),
        Some("application/json"),
    )
    .await;
    assert_problem(&response, StatusCode::BAD_REQUEST, "INVALID_PATH");
}

#[tokio::test]
async fn database_failures_return_generic_problems() {
    let (app, pool) = setup_with_pool().await;
    pool.close().await;
    let response = call_raw(&app, "GET", "/api/health", String::new(), None).await;
    assert_problem(
        &response,
        StatusCode::INTERNAL_SERVER_ERROR,
        "DATABASE_ERROR",
    );
    assert_eq!(response.2["message"], "Database operation failed");
}

#[tokio::test]
async fn health_and_dated_task_responses_preserve_daily_completion() {
    let (app, pool) = setup_with_pool().await;
    let (status, health) = call(&app, "GET", "/api/health", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(health, json!({"status":"ok","storage":"sqlite"}));
    sqlx::query("INSERT INTO task_templates (id,title) VALUES ('extra-check','Extra check')")
        .execute(&pool)
        .await
        .unwrap();
    let before = chrono::Utc::now();
    let (status, today) = call(&app, "GET", "/api/tasks", Value::Null).await;
    let after = chrono::Utc::now();
    assert_eq!(status, StatusCode::OK);
    let date =
        chrono::NaiveDate::parse_from_str(today["missionDate"].as_str().unwrap(), "%Y-%m-%d")
            .unwrap();
    assert!(date == before.date_naive() || date == after.date_naive());
    let templates: Vec<(String, String)> =
        sqlx::query_as("SELECT id,title FROM task_templates ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    let expected: Vec<Value> = templates
        .into_iter()
        .map(|(id, title)| json!({"id":id,"title":title,"completedAt":null}))
        .collect();
    assert_eq!(today["tasks"], json!(expected));
    let yesterday = date.pred_opt().unwrap().to_string();
    let path = format!("/api/tasks?date={yesterday}");
    let (_, empty) = call(&app, "GET", &path, Value::Null).await;
    assert_eq!(empty, json!({"missionDate":yesterday,"tasks":expected}));
    let today_body = json!({"missionDate":today["missionDate"],"completed":true});
    let (status, today_mark) = call(&app, "PATCH", "/api/tasks/daily-check", today_body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(today_mark["missionDate"], today["missionDate"]);
    let body = json!({"missionDate":yesterday,"completed":true});
    let start = chrono::Utc::now();
    let (status, saved) = call(&app, "PATCH", "/api/tasks/daily-check", body.clone()).await;
    let end = chrono::Utc::now();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["missionDate"], yesterday);
    assert_eq!(saved["id"], empty["tasks"][0]["id"]);
    assert_eq!(saved["title"], empty["tasks"][0]["title"]);
    assert_eq!(saved.as_object().unwrap().len(), 4);
    let time =
        chrono::DateTime::parse_from_rfc3339(saved["completedAt"].as_str().unwrap()).unwrap();
    assert!(time >= start && time <= end);
    let (status, repeated) = call(&app, "PATCH", "/api/tasks/daily-check", body.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(repeated, saved);
    let (_, read) = call(&app, "GET", &path, Value::Null).await;
    assert_eq!(read["tasks"][0]["completedAt"], saved["completedAt"]);
    for _ in 0..2 {
        let (status, cleared) = call(
            &app,
            "PATCH",
            "/api/tasks/daily-check",
            json!({"missionDate":yesterday,"completed":false}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            cleared,
            json!({"missionDate":yesterday,"id":saved["id"],"title":saved["title"],"completedAt":null})
        );
        assert_eq!(call(&app, "GET", &path, Value::Null).await.1, empty);
        let (_, current) = call(&app, "GET", &format!("/api/tasks?date={date}"), Value::Null).await;
        assert_eq!(
            current["tasks"][0]["completedAt"],
            today_mark["completedAt"]
        );
    }
    let (_, renewed) = call(&app, "PATCH", "/api/tasks/daily-check", body).await;
    let renewed_time =
        chrono::DateTime::parse_from_rfc3339(renewed["completedAt"].as_str().unwrap()).unwrap();
    assert!(renewed_time > time);
    let legacy: Option<String> =
        sqlx::query_scalar("SELECT completed_at FROM tasks WHERE id='daily-check'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(legacy.is_none());
}

#[tokio::test]
async fn task_date_and_body_errors_use_problem_fields_and_unknown_tasks_return_404() {
    let app = setup().await;
    for date in [
        "",
        "2026-02-30",
        "2026-1-5",
        "2026-11-14T00:00:00Z",
        "2026-11-14+05:00",
        "2026-11-14 ",
    ] {
        let query = format!("date={}", date.replace('+', "%2B").replace(' ', "%20"));
        let response = call_raw(
            &app,
            "GET",
            &format!("/api/tasks?{query}"),
            String::new(),
            None,
        )
        .await;
        assert_problem(&response, StatusCode::BAD_REQUEST, "VALIDATION_ERROR");
        assert_eq!(response.2["field"], "date");
        let response = call_raw(
            &app,
            "PATCH",
            "/api/tasks/daily-check",
            json!({"missionDate":date,"completed":true}).to_string(),
            Some("application/json"),
        )
        .await;
        assert_problem(&response, StatusCode::BAD_REQUEST, "VALIDATION_ERROR");
        assert_eq!(response.2["field"], "missionDate");
    }
    for body in [
        json!({"completed":true}),
        json!({"missionDate":null,"completed":true}),
        json!({"missionDate":123,"completed":true}),
    ] {
        let response = call_raw(
            &app,
            "PATCH",
            "/api/tasks/daily-check",
            body.to_string(),
            Some("application/json"),
        )
        .await;
        assert_problem(&response, StatusCode::BAD_REQUEST, "VALIDATION_ERROR");
        assert_eq!(response.2["field"], "missionDate");
    }
    for body in [
        json!({"missionDate":"2026-11-14"}),
        json!({"missionDate":"2026-11-14","completed":"true"}),
        json!({"missionDate":"2026-11-14","completed":true,"completedAt":"2026-11-14T00:00:00Z"}),
        json!({"missionDate":"2026-11-14","completed":true,"astronautId":"other"}),
    ] {
        let response = call_raw(
            &app,
            "PATCH",
            "/api/tasks/daily-check",
            body.to_string(),
            Some("application/json"),
        )
        .await;
        assert_problem(&response, StatusCode::BAD_REQUEST, "INVALID_BODY");
    }
    for query in ["unknown=1", "date=2026-11-14&date=2026-11-15"] {
        let response = call_raw(
            &app,
            "GET",
            &format!("/api/tasks?{query}"),
            String::new(),
            None,
        )
        .await;
        assert_problem(&response, StatusCode::BAD_REQUEST, "INVALID_QUERY");
    }
    let response = call_raw(
        &app,
        "PATCH",
        "/api/tasks/missing",
        json!({"missionDate":"2026-11-14","completed":true}).to_string(),
        Some("application/json"),
    )
    .await;
    assert_problem(&response, StatusCode::NOT_FOUND, "TASK_NOT_FOUND");
}

#[tokio::test]
async fn history_uses_received_time_when_measurement_time_is_unknown() {
    let (app, pool) = setup_with_pool().await;
    let (_, first) = call(&app, "POST", "/api/measurements", sample()).await;
    let mut second_input = sample();
    second_input["sequence"] = json!(1);
    second_input["value"] = json!(74);
    let (_, second) = call(&app, "POST", "/api/measurements", second_input).await;
    sqlx::query("UPDATE measurements SET received_at=? WHERE id=?")
        .bind("2026-01-01T00:00:00+00:00")
        .bind(first["id"].as_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE measurements SET received_at=? WHERE id=?")
        .bind("2026-01-02T00:00:00+00:00")
        .bind(second["id"].as_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let (status, history) = call(&app, "GET", "/api/measurements", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    let history = &history["items"];
    assert_eq!(history[0]["id"], second["id"]);
    assert_eq!(history[1]["id"], first["id"]);
    assert_eq!(history[0]["deviceId"], "simulator-01");
    assert_eq!(history[0]["sessionId"], "test-session");
    assert_eq!(history[0]["sequence"], 1);
    assert_eq!(history[0]["metric"], "heartRate");
    assert_eq!(history[0]["unit"], "bpm");
    assert_eq!(history[0]["value"].as_f64(), Some(74.0));
    assert!(history[0]["measuredAt"].is_null());
    assert_eq!(history[0]["timeBasis"], "receivedAt");
}

#[tokio::test]
async fn history_orders_by_utc_measurement_time_with_explicit_reception_fallback() {
    let (app, pool) = setup_with_pool().await;
    let mut saved = Vec::new();
    // Arrival order and lexicographic timestamp order disagree with UTC order.
    for (sequence, time) in [
        Some("2020-01-01T10:00:00+05:00"),
        Some("2020-01-01T05:30:00Z"),
        Some("2019-12-31T23:30:00-06:00"),
        Some("2019-01-01T00:00:00Z"),
        None,
    ]
    .into_iter()
    .enumerate()
    {
        let mut input = sample();
        input["sequence"] = json!(sequence);
        input["measuredAt"] = json!(time);
        let (status, row) = call(&app, "POST", "/api/measurements", input).await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(row["measuredAt"], json!(time));
        assert_eq!(
            row["timeBasis"],
            if time.is_some() {
                "measuredAt"
            } else {
                "receivedAt"
            }
        );
        saved.push(row);
    }
    sqlx::query(
        "UPDATE measurements SET received_at='2020-01-01T06:00:00Z' WHERE measured_at IS NULL",
    )
    .execute(&pool)
    .await
    .unwrap();
    let (_, rows) = call(&app, "GET", "/api/measurements", Value::Null).await;
    let mut tied = [
        saved[1]["id"].as_str().unwrap(),
        saved[2]["id"].as_str().unwrap(),
    ];
    tied.sort_unstable_by(|a, b| b.cmp(a));
    let expected = [
        saved[4]["id"].as_str().unwrap(),
        tied[0],
        tied[1],
        saved[0]["id"].as_str().unwrap(),
        saved[3]["id"].as_str().unwrap(),
    ];
    let ids: Vec<_> = rows["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, expected);
}

#[tokio::test]
async fn reception_does_not_establish_freshness_without_measurement_time() {
    let app = setup().await;
    call(&app, "POST", "/api/measurements", sample()).await;
    let (status, assessment) = call(&app, "GET", "/api/assessments", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(assessment["status"], "unknown-time");
}

#[tokio::test]
async fn assessment_selects_last_reception_by_numeric_utc_time() {
    let (app, pool) = setup_with_pool().await;
    let mut old = sample();
    old["measuredAt"] = json!("2020-01-01T00:00:00Z");
    call(&app, "POST", "/api/measurements", old).await;
    let mut unknown = sample();
    unknown["sequence"] = json!(1);
    call(&app, "POST", "/api/measurements", unknown).await;
    sqlx::query("UPDATE measurements SET received_at=CASE sequence WHEN 0 THEN '2020-01-01T10:00:00+05:00' ELSE '2020-01-01T05:01:00Z' END")
        .execute(&pool).await.unwrap();
    let (_, assessment) = call(&app, "GET", "/api/assessments", Value::Null).await;
    assert_eq!(assessment["status"], "unknown-time");
}

#[tokio::test]
async fn accepted_rfc3339_variants_store_exact_utc_milliseconds() {
    let (app, pool) = setup_with_pool().await;
    for (sequence, (time, expected_ms)) in [
        ("1970-01-01T00:00:00Z", 0_i64),
        ("1969-12-31T23:59:59.9999Z", -1),
        ("1970-01-01t05:00:00.123456789+05:00", 123),
        ("1970-01-01 00:00:00.1z", 100),
        ("1970-01-01T23:59:00+23:59", 0),
        ("1969-12-31T19:00:00−05:00", 0),
        ("1969-12-31T23:59:60Z", 0),
    ]
    .into_iter()
    .enumerate()
    {
        let mut input = sample();
        input["sequence"] = json!(sequence);
        input["measuredAt"] = json!(time);
        let (status, row) = call(&app, "POST", "/api/measurements", input).await;
        assert_eq!(status, StatusCode::CREATED, "{time}: {row}");
        assert_eq!(row["measuredAt"], time, "keep the source timestamp intact");
        let actual: i64 = sqlx::query_scalar("SELECT measured_at_ms FROM measurements WHERE id=?")
            .bind(row["id"].as_str().unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(actual, expected_ms, "{time}");
    }
}

#[tokio::test]
async fn new_metrics_accept_manual_input_validate_fields_and_expose_all_latest_entries() {
    let app = setup().await;
    let cases = [
        ("spo2", "%", 0.0),
        ("glucose", "mg/dL", 90.0),
        ("visionBlur", "score", 1.25),
        ("exerciseMinutes", "min", 1440.0),
        ("headache", "score", 0.0),
        ("backPain", "score", 10.0),
    ];
    let (_, empty) = call(&app, "GET", "/api/measurements/latest", Value::Null).await;
    assert_eq!(empty["metrics"].as_array().unwrap().len(), 12);
    for (index, (metric, unit, value)) in cases.iter().enumerate() {
        let missing = empty["metrics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["metric"] == *metric)
            .unwrap();
        assert_eq!(missing["quality"]["status"], "missing");
        let input = json!({"deviceId":"manual-01", "sessionId":"new-metrics", "sequence":index,
            "metric":metric, "unit":unit, "value":value});
        let (status, created) = call(&app, "POST", "/api/measurements", input.clone()).await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        assert_eq!(created["source"], "manual");
        assert_eq!(created["measuredAt"], Value::Null);
        let (status, retry) = call(&app, "POST", "/api/measurements", input).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(retry, created);
    }
    for (metric, unit, value, field) in [
        ("headache", "score", 10.01, "value"),
        ("spo2", "bpm", 97.0, "unit"),
        ("glucose", "mg/dL", 0.0, "value"),
    ] {
        let input = json!({"deviceId":"manual-01", "sessionId":"invalid-new", "sequence":0,
            "metric":metric, "unit":unit, "value":value});
        let (status, error) = call(&app, "POST", "/api/measurements", input).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
        assert_eq!(error["code"], "VALIDATION_ERROR");
        assert_eq!(error["field"], field);
    }
    let (_, latest) = call(&app, "GET", "/api/measurements/latest", Value::Null).await;
    assert_eq!(latest["metrics"].as_array().unwrap().len(), 12);
    for (metric, unit, value) in cases {
        let entry = latest["metrics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["metric"] == metric)
            .unwrap();
        assert_eq!(entry["latestMeasurement"]["unit"], unit);
        assert_eq!(entry["latestMeasurement"]["value"], value);
        assert_eq!(entry["quality"]["status"], "unknownTime");
    }
    let (_, history) = call(
        &app,
        "GET",
        "/api/measurements?metric=headache",
        Value::Null,
    )
    .await;
    assert_eq!(history["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn new_series_aggregate_mean_and_last_and_summary_selects_latest_received_metric() {
    let (app, pool) = setup_with_pool().await;
    let measured = chrono::Utc::now().to_rfc3339();
    for (sequence, (metric, unit, value)) in [
        ("heartRate", "bpm", 72.0),
        ("systolicPressure", "mmHg", 120.0),
        ("diastolicPressure", "mmHg", 80.0),
        ("temperature", "°C", 36.7),
        ("sleepHours", "h", 7.5),
        ("mood", "score", 4.0),
    ]
    .into_iter()
    .enumerate()
    {
        let input = json!({"deviceId":"manual-01", "sessionId":"legacy", "sequence":sequence,
            "metric":metric, "unit":unit, "value":value, "measuredAt":measured});
        assert_eq!(
            call(&app, "POST", "/api/measurements", input).await.0,
            StatusCode::CREATED
        );
    }
    let (_, before) = call(&app, "GET", "/api/assessments", Value::Null).await;
    assert_eq!(before["status"], "not-evaluated");
    assert_eq!(before["metrics"].as_array().unwrap().len(), 12);
    assert_eq!(
        before["metrics"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["quality"]["status"] == "missing")
            .count(),
        6
    );
    for (metric, unit, values, expected, aggregation) in [
        ("spo2", "%", [96.0, 98.0], 97.0, "mean"),
        ("headache", "score", [1.5, 5.0], 5.0, "last"),
    ] {
        for (index, value) in values.into_iter().enumerate() {
            let input = json!({"deviceId":"manual-01", "sessionId":metric, "sequence":index,
                "metric":metric, "unit":unit, "value":value,
                "measuredAt":format!("2020-01-01T00:00:0{}Z", index + 1)});
            assert_eq!(
                call(&app, "POST", "/api/measurements", input).await.0,
                StatusCode::CREATED
            );
        }
        let path = format!("/api/measurements/series?metric={metric}&from=2020-01-01T00:00:00Z&to=2020-01-02T00:00:00Z&maxPoints=1");
        let (status, series) = call(&app, "GET", &path, Value::Null).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(series["aggregation"], aggregation);
        assert_eq!(series["unit"], unit);
        assert_eq!(series["points"][0]["value"], expected);
        assert_eq!(series["points"][0]["count"], 2);
        if aggregation == "mean" {
            assert_eq!(series["points"][0]["min"], 96.0);
            assert_eq!(series["points"][0]["max"], 98.0);
        } else {
            assert_eq!(series["points"][0]["min"], Value::Null);
            assert_eq!(series["points"][0]["max"], Value::Null);
        }
    }
    // Keep earlier receipts strictly older, regardless of clock resolution or id ties.
    sqlx::query("UPDATE measurements SET received_at='2020-01-01T00:00:00Z'")
        .execute(&pool)
        .await
        .unwrap();
    let input = json!({"deviceId":"manual-01", "sessionId":"headache", "sequence":2,
        "metric":"headache", "unit":"score", "value":4.5});
    let (status, headache) = call(&app, "POST", "/api/measurements", input).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(headache["measuredAt"], Value::Null);
    let (_, after) = call(&app, "GET", "/api/assessments", Value::Null).await;
    assert_eq!(after["status"], "unknown-time");
    assert_ne!(after["explanation"], before["explanation"]);
    let metrics = after["metrics"].as_array().unwrap();
    assert_eq!(metrics.len(), 12);
    assert_eq!(
        metrics
            .iter()
            .filter(|row| row["quality"]["status"] == "missing")
            .count(),
        4
    );
    let quality = &metrics
        .iter()
        .find(|row| row["metric"] == "headache")
        .unwrap()["latestReceivedQuality"];
    assert_eq!(quality["measurementId"], headache["id"]);
    assert_eq!(quality["status"], "unknownTime");
}
