use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
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

async fn get(app: &Router, path: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn seed(pool: &SqlitePool, id: &str, metric: &str, measured: Option<&str>, received: &str) {
    let (value, unit) = if metric == "sleepHours" {
        (7.5, "h")
    } else {
        (72.0, "bpm")
    };
    sqlx::query("INSERT INTO measurements(id,device_id,astronaut_id,session_id,sequence,metric,value,unit,measured_at,received_at,source) VALUES (?,'manual-01','demo-astronaut',?,0,?,?,?,?,?,'manual')")
        .bind(id).bind(id).bind(metric).bind(value).bind(unit).bind(measured).bind(received)
        .execute(pool).await.unwrap();
}

fn ids(page: &Value) -> Vec<&str> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn empty_history_and_latest_explicitly_represent_missing_data() {
    let (app, _) = setup().await;
    assert_eq!(
        get(&app, "/api/measurements").await,
        (
            StatusCode::OK,
            json!({"items":[],"hasMore":false,"nextCursor":null})
        )
    );
    let (status, latest) = get(&app, "/api/measurements/latest").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(latest["astronautId"], "demo-astronaut");
    let metrics = latest["metrics"].as_array().unwrap();
    assert_eq!(
        metrics
            .iter()
            .map(|row| row["metric"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "heartRate",
            "systolicPressure",
            "diastolicPressure",
            "temperature",
            "sleepHours",
            "mood",
            "spo2",
            "glucose",
            "visionBlur",
            "exerciseMinutes",
            "headache",
            "backPain"
        ]
    );
    for row in metrics {
        for field in [
            "latestMeasurement",
            "latestKnownMeasurement",
            "latestReceivedMeasurement",
        ] {
            assert_eq!(row.get(field), Some(&Value::Null), "{field}");
        }
    }
}

#[tokio::test]
async fn history_filters_metric_and_half_open_utc_range_with_unknown_time_fallback() {
    let (app, pool) = setup().await;
    seed(
        &pool,
        "start",
        "heartRate",
        Some("2020-01-01T05:00:00+05:00"),
        "2020-01-02T00:00:00Z",
    )
    .await;
    seed(
        &pool,
        "unknown",
        "heartRate",
        None,
        "2020-01-01T00:00:00.001Z",
    )
    .await;
    seed(
        &pool,
        "end",
        "heartRate",
        Some("2020-01-01T00:00:00.002Z"),
        "2020-01-02T00:00:00Z",
    )
    .await;
    seed(
        &pool,
        "sleep",
        "sleepHours",
        Some("2020-01-01T00:00:00.001Z"),
        "2020-01-02T00:00:00Z",
    )
    .await;
    let (status, page) = get(&app, "/api/measurements?metric=heartRate&from=2020-01-01T05:00:00%2B05:00&to=2020-01-01T00:00:00.002Z").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&page), ["unknown", "start"]);
    assert_eq!(page["items"][0]["timeBasis"], "receivedAt");
    assert!(page["items"][0]["measuredAt"].is_null());
    assert_eq!(page["hasMore"], false);
    assert!(page["nextCursor"].is_null());
    let (_, from_only) = get(
        &app,
        "/api/measurements?metric=heartRate&from=2020-01-01T00:00:00.002Z",
    )
    .await;
    assert_eq!(ids(&from_only), ["end"]);
    let (_, to_only) = get(
        &app,
        "/api/measurements?metric=heartRate&to=2020-01-01T00:00:00.001Z",
    )
    .await;
    assert_eq!(ids(&to_only), ["start"]);
}

#[tokio::test]
async fn keyset_pages_do_not_duplicate_ties_when_a_newer_row_arrives() {
    let (app, pool) = setup().await;
    for id in ["a", "b", "c"] {
        seed(
            &pool,
            id,
            "heartRate",
            Some("2020-01-02T00:00:00Z"),
            "2020-01-03T00:00:00Z",
        )
        .await;
    }
    seed(
        &pool,
        "older",
        "heartRate",
        Some("2020-01-01T00:00:00Z"),
        "2020-01-04T00:00:00Z",
    )
    .await;
    let (_, first) = get(&app, "/api/measurements?metric=heartRate&limit=2").await;
    assert_eq!(ids(&first), ["c", "b"]);
    assert_eq!(first["hasMore"], true);
    seed(
        &pool,
        "new-arrival",
        "heartRate",
        Some("2020-01-05T00:00:00Z"),
        "2020-01-05T00:00:00Z",
    )
    .await;
    let (status, second) = get(
        &app,
        &format!(
            "/api/measurements?metric=heartRate&limit=2&cursor={}",
            first["nextCursor"].as_str().unwrap()
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&second), ["a", "older"]);
    assert_eq!(
        second["hasMore"], false,
        "a full final page is not truncated"
    );
    assert!(second["nextCursor"].is_null());
}

#[tokio::test]
async fn frequent_pulse_does_not_evict_sleep_from_latest_or_hide_truncation() {
    let (app, pool) = setup().await;
    seed(
        &pool,
        "sleep",
        "sleepHours",
        Some("2020-01-01T00:00:00Z"),
        "2020-01-01T00:00:00Z",
    )
    .await;
    sqlx::query("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<510) INSERT INTO measurements(id,device_id,astronaut_id,session_id,sequence,metric,value,unit,measured_at,received_at,source) SELECT printf('pulse-%04d',x),'simulator-01','demo-astronaut','many-pulses',x,'heartRate',72,'bpm','2020-01-02T00:00:00Z','2020-01-02T00:00:00Z','simulator' FROM n")
        .execute(&pool).await.unwrap();
    let (_, first) = get(&app, "/api/measurements").await;
    assert_eq!(ids(&first).len(), 100);
    assert_eq!(first["hasMore"], true);
    let (_, max_page) = get(&app, "/api/measurements?limit=500").await;
    assert_eq!(ids(&max_page).len(), 500);
    assert_eq!(max_page["hasMore"], true);
    let (_, rest) = get(
        &app,
        &format!(
            "/api/measurements?limit=500&cursor={}",
            max_page["nextCursor"].as_str().unwrap()
        ),
    )
    .await;
    assert_eq!(ids(&rest).len(), 11);
    assert_eq!(ids(&rest).last(), Some(&"sleep"));
    assert_eq!(rest["hasMore"], false);
    let (_, latest) = get(&app, "/api/measurements/latest").await;
    assert_eq!(
        latest["metrics"][4]["latestKnownMeasurement"]["id"],
        "sleep"
    );
    assert_eq!(
        latest["metrics"][0]["latestKnownMeasurement"]["id"],
        "pulse-0510"
    );
}

#[tokio::test]
async fn latest_keeps_known_measurement_when_delayed_or_unknown_messages_arrive() {
    let (app, pool) = setup().await;
    seed(
        &pool,
        "known-a",
        "heartRate",
        Some("2020-01-03T00:00:00Z"),
        "2020-01-03T00:00:00Z",
    )
    .await;
    seed(
        &pool,
        "delayed",
        "heartRate",
        Some("2020-01-01T00:00:00Z"),
        "2020-01-04T00:00:00Z",
    )
    .await;
    let (_, latest) = get(&app, "/api/measurements/latest").await;
    let pulse = &latest["metrics"][0];
    assert_eq!(pulse["latestMeasurement"]["id"], "known-a");
    assert_eq!(pulse["latestKnownMeasurement"]["id"], "known-a");
    assert_eq!(pulse["latestReceivedMeasurement"]["id"], "delayed");
    seed(
        &pool,
        "known-z",
        "heartRate",
        Some("2020-01-03T05:00:00+05:00"),
        "2020-01-03T00:00:00Z",
    )
    .await;
    for id in ["unknown-a", "unknown-z"] {
        seed(&pool, id, "heartRate", None, "2020-01-05T00:00:00Z").await;
    }
    let (_, latest) = get(&app, "/api/measurements/latest").await;
    let pulse = &latest["metrics"][0];
    assert_eq!(pulse["latestKnownMeasurement"]["id"], "known-z");
    assert_eq!(pulse["latestKnownMeasurement"]["timeBasis"], "measuredAt");
    assert_eq!(pulse["latestMeasurement"]["id"], "unknown-z");
    assert_eq!(pulse["latestReceivedMeasurement"]["id"], "unknown-z");
    assert_eq!(
        pulse["latestReceivedMeasurement"]["timeBasis"],
        "receivedAt"
    );
}

#[tokio::test]
async fn invalid_history_parameters_return_field_specific_problems() {
    let (app, _) = setup().await;
    for (query, field) in [
        ("metric=unknown", "metric"),
        ("metric=", "metric"),
        ("limit=0", "limit"),
        ("limit=501", "limit"),
        ("limit=-1", "limit"),
        ("limit=1.5", "limit"),
        ("from=2020-01-01T00:00:00", "from"),
        ("to=2020-02-31T00:00:00Z", "to"),
        ("from=2020-01-01T00:00:00.0000000001Z", "from"),
        ("from=2020-01-02T00:00:00Z&to=2020-01-01T00:00:00Z", "to"),
        (
            "from=2020-01-01T00:00:00Z&to=2020-01-01T05:00:00%2B05:00",
            "to",
        ),
        ("cursor=", "cursor"),
        ("cursor=v2.0.id", "cursor"),
        ("cursor=v1.bad.id", "cursor"),
        ("cursor=v1.0.", "cursor"),
    ] {
        let (status, problem) = get(&app, &format!("/api/measurements?{query}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {problem}");
        assert_eq!(problem["code"], "VALIDATION_ERROR", "{query}");
        assert_eq!(problem["field"], field, "{query}");
    }
}

#[tokio::test]
async fn unsupported_or_duplicate_query_parameters_are_not_silently_ignored() {
    let (app, _) = setup().await;
    for path in [
        "/api/measurements?astronautId=another-person",
        "/api/measurements?offset=10",
        "/api/measurements?limit=1&limit=2",
        "/api/measurements?metric=heartRate&metric=sleepHours",
        "/api/measurements/latest?metric=heartRate",
    ] {
        let (status, problem) = get(&app, path).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}: {problem}");
        assert_eq!(problem["code"], "INVALID_QUERY");
    }
}

#[tokio::test]
async fn query_failures_are_not_disguised_as_missing_measurements() {
    let (app, pool) = setup().await;
    pool.close().await;
    for path in ["/api/measurements", "/api/measurements/latest"] {
        let (status, error) = get(&app, path).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error["code"], "DATABASE_ERROR");
    }
}
