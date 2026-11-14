use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use chrono::{DateTime, Duration, Utc};
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

async fn call(app: &Router, method: &str, path: &str, input: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(input.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    if status.is_client_error() || status.is_server_error() {
        assert_eq!(
            response.headers()["content-type"],
            "application/problem+json"
        );
    }
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn seed(pool: &SqlitePool, id: &str, metric: &str, measured: Option<&str>, received: &str) {
    let (value, unit) = match metric {
        "sleepHours" => (7.5, "h"),
        "mood" => (4.0, "score"),
        "temperature" => (36.7, "°C"),
        "systolicPressure" | "diastolicPressure" => (100.0, "mmHg"),
        _ => (72.0, "bpm"),
    };
    sqlx::query("INSERT INTO measurements(id,device_id,astronaut_id,session_id,sequence,metric,value,unit,measured_at,received_at,source) VALUES (?,'manual-01','demo-astronaut',?,0,?,?,?,?,?,'manual')")
        .bind(id).bind(id).bind(metric).bind(value).bind(unit).bind(measured).bind(received)
        .execute(pool).await.unwrap();
}

fn metric<'a>(response: &'a Value, name: &str) -> &'a Value {
    response["metrics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["metric"] == name)
        .unwrap()
}

#[tokio::test]
async fn empty_snapshot_exposes_six_missing_metrics_with_explicit_policy() {
    let (app, _) = setup().await;
    for path in ["/api/measurements/latest", "/api/assessments"] {
        let (status, result) = call(&app, "GET", path, Value::Null).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(result["astronautId"], "demo-astronaut");
        DateTime::parse_from_rfc3339(result["evaluatedAt"].as_str().unwrap()).unwrap();
        assert_eq!(result["metrics"].as_array().unwrap().len(), 6);
        for (name, ttl) in [
            ("heartRate", 15),
            ("systolicPressure", 3600),
            ("diastolicPressure", 3600),
            ("temperature", 3600),
            ("sleepHours", 86400),
            ("mood", 86400),
        ] {
            let entry = metric(&result, name);
            let expected = json!({"status":"missing","measurementId":null,"freshForSeconds":ttl,"ageSeconds":null,"clockSkew":false});
            assert_eq!(entry["quality"], expected);
            assert_eq!(entry["latestReceivedQuality"], expected);
        }
        if path == "/api/assessments" {
            assert_eq!(result["status"], "no-data");
        }
    }
}

#[tokio::test]
async fn each_metric_uses_its_own_measured_time_and_window_in_both_resources() {
    let (app, pool) = setup().await;
    let now = Utc::now();
    let cases = [
        (
            "systolicPressure",
            Some(now - Duration::minutes(30)),
            "fresh",
            3600,
        ),
        (
            "diastolicPressure",
            Some(now + Duration::minutes(2)),
            "clockSkew",
            3600,
        ),
        ("temperature", Some(now - Duration::hours(2)), "stale", 3600),
        (
            "sleepHours",
            Some(now - Duration::hours(25)),
            "stale",
            86400,
        ),
        ("mood", None, "unknownTime", 86400),
        ("heartRate", Some(now - Duration::seconds(1)), "fresh", 15),
    ];
    for (name, measured, _, _) in cases {
        seed(
            &pool,
            name,
            name,
            measured.map(|time| time.to_rfc3339()).as_deref(),
            &now.to_rfc3339(),
        )
        .await;
    }
    for path in ["/api/measurements/latest", "/api/assessments"] {
        let (status, result) = call(&app, "GET", path, Value::Null).await;
        assert_eq!(status, StatusCode::OK);
        let evaluated =
            DateTime::parse_from_rfc3339(result["evaluatedAt"].as_str().unwrap()).unwrap();
        for (name, measured, expected, ttl) in cases {
            let quality = &metric(&result, name)["quality"];
            assert_eq!(quality["status"], expected, "{name}: {quality}");
            assert_eq!(quality["measurementId"], name);
            assert_eq!(quality["freshForSeconds"], ttl);
            assert_eq!(quality["clockSkew"], expected == "clockSkew");
            assert_eq!(*quality, metric(&result, name)["latestReceivedQuality"]);
            if let Some(time) = measured.filter(|_| expected != "clockSkew") {
                let expected_age = evaluated.signed_duration_since(time).as_seconds_f64();
                assert!((quality["ageSeconds"].as_f64().unwrap() - expected_age).abs() < 1e-8);
            } else {
                assert!(quality["ageSeconds"].is_null());
            }
        }
    }
}

#[tokio::test]
async fn unknown_delivery_is_reported_separately_without_hiding_a_known_measurement() {
    let (app, pool) = setup().await;
    let now = Utc::now();
    let known_time = (now - Duration::seconds(2)).to_rfc3339();
    seed(&pool, "known", "heartRate", Some(&known_time), &known_time).await;
    seed(&pool, "unknown", "heartRate", None, &now.to_rfc3339()).await;
    let (_, latest) = call(&app, "GET", "/api/measurements/latest", Value::Null).await;
    let pulse = metric(&latest, "heartRate");
    assert_eq!(pulse["latestMeasurement"]["id"], "unknown");
    assert_eq!(pulse["latestKnownMeasurement"]["id"], "known");
    assert_eq!(pulse["quality"]["measurementId"], "known");
    assert_eq!(pulse["quality"]["status"], "fresh");
    assert_eq!(pulse["latestReceivedQuality"]["measurementId"], "unknown");
    assert_eq!(pulse["latestReceivedQuality"]["status"], "unknownTime");
    assert!(pulse["latestReceivedQuality"]["ageSeconds"].is_null());
    let (_, assessment) = call(&app, "GET", "/api/assessments", Value::Null).await;
    assert_eq!(
        assessment["status"], "unknown-time",
        "deprecated summary remains scoped to the last delivery"
    );
    assert_eq!(
        metric(&assessment, "heartRate")["quality"]["status"],
        "fresh"
    );
}

#[tokio::test]
async fn delayed_and_retried_samples_do_not_refresh_or_replace_newer_measurements() {
    let (app, pool) = setup().await;
    let now = Utc::now();
    let known = (now - Duration::seconds(2)).to_rfc3339();
    seed(&pool, "known", "heartRate", Some(&known), &known).await;
    let input = json!({"deviceId":"manual-01","sessionId":"late-pulse","sequence":0,"metric":"heartRate","value":71,"unit":"bpm","measuredAt":(now-Duration::hours(1)).to_rfc3339()});
    let (status, saved) = call(&app, "POST", "/api/measurements", input.clone()).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, repeated) = call(&app, "POST", "/api/measurements", input).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved, repeated);
    let (_, result) = call(&app, "GET", "/api/measurements/latest", Value::Null).await;
    let pulse = metric(&result, "heartRate");
    assert_eq!(pulse["quality"]["measurementId"], "known");
    assert_eq!(pulse["quality"]["status"], "fresh");
    assert_eq!(pulse["latestReceivedQuality"]["measurementId"], saved["id"]);
    assert_eq!(pulse["latestReceivedQuality"]["status"], "stale");
    let (_, assessment) = call(&app, "GET", "/api/assessments", Value::Null).await;
    assert_eq!(assessment["status"], "stale");
    assert_eq!(
        metric(&assessment, "heartRate")["quality"]["status"],
        "fresh"
    );
}

#[tokio::test]
async fn deprecated_summary_also_uses_the_selected_metrics_policy() {
    let (app, pool) = setup().await;
    let now = Utc::now();
    let hour_ago = (now - Duration::hours(1)).to_rfc3339();
    seed(&pool, "old-pulse", "heartRate", Some(&hour_ago), &hour_ago).await;
    seed(
        &pool,
        "sleep",
        "sleepHours",
        Some(&(now - Duration::hours(2)).to_rfc3339()),
        &now.to_rfc3339(),
    )
    .await;
    let (_, result) = call(&app, "GET", "/api/assessments", Value::Null).await;
    assert_eq!(result["status"], "not-evaluated");
    assert_eq!(metric(&result, "sleepHours")["quality"]["status"], "fresh");
    assert_eq!(metric(&result, "heartRate")["quality"]["status"], "stale");
}

#[tokio::test]
async fn accepted_future_time_is_clock_skew_and_out_of_tolerance_is_rejected() {
    let (app, pool) = setup().await;
    let now = Utc::now();
    let mut input = json!({"deviceId":"manual-01","sessionId":"future","sequence":0,"metric":"heartRate","value":72,"unit":"bpm","measuredAt":(now+Duration::seconds(299)).to_rfc3339()});
    let (status, saved) = call(&app, "POST", "/api/measurements", input.clone()).await;
    assert_eq!(status, StatusCode::CREATED);
    input["sequence"] = json!(1);
    input["measuredAt"] = json!((now + Duration::minutes(10)).to_rfc3339());
    let (status, error) = call(&app, "POST", "/api/measurements", input).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["field"], "measuredAt");
    for path in ["/api/measurements/latest", "/api/assessments"] {
        let (status, result) = call(&app, "GET", path, Value::Null).await;
        assert_eq!(status, StatusCode::OK);
        let pulse = metric(&result, "heartRate");
        assert_eq!(pulse["quality"]["status"], "clockSkew");
        assert_eq!(pulse["quality"]["clockSkew"], true);
        assert_eq!(pulse["quality"]["measurementId"], saved["id"]);
        assert!(pulse["quality"]["ageSeconds"].is_null());
        if path == "/api/assessments" {
            assert_eq!(result["status"], "clock-skew");
        }
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM measurements")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    // A stored timestamp can exceed ingestion tolerance after the server clock rolls back.
    seed(
        &pool,
        "after-clock-rollback",
        "mood",
        Some(&(now + Duration::hours(1)).to_rfc3339()),
        &now.to_rfc3339(),
    )
    .await;
    let (status, result) = call(&app, "GET", "/api/measurements/latest", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(metric(&result, "mood")["quality"]["status"], "clockSkew");
}

#[tokio::test]
async fn qualities_are_scoped_to_the_server_selected_astronaut() {
    let (app, pool) = setup().await;
    sqlx::query("INSERT INTO devices VALUES ('other-device','other-astronaut','sensor')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO measurements(id,device_id,astronaut_id,session_id,sequence,metric,value,unit,measured_at,received_at,source) VALUES ('foreign','other-device','other-astronaut','foreign',0,'heartRate',72,'bpm','9999-01-01T00:00:00Z','9999-01-01T00:00:00Z','sensor')").execute(&pool).await.unwrap();
    for path in ["/api/measurements/latest", "/api/assessments"] {
        let (status, result) = call(&app, "GET", path, Value::Null).await;
        assert_eq!(status, StatusCode::OK);
        assert!(result["metrics"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["quality"]["status"] == "missing"));
        if path == "/api/assessments" {
            assert_eq!(result["status"], "no-data");
        }
    }
}

#[tokio::test]
async fn query_overrides_and_database_failure_are_not_disguised_as_fresh_or_missing() {
    let (app, pool) = setup().await;
    for path in [
        "/api/assessments?astronautId=other",
        "/api/assessments?now=2020-01-01T00:00:00Z",
        "/api/measurements/latest?freshForSeconds=999999",
    ] {
        let (status, problem) = call(&app, "GET", path, Value::Null).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(problem["code"], "INVALID_QUERY");
    }
    pool.close().await;
    for path in ["/api/measurements/latest", "/api/assessments"] {
        let (status, problem) = call(&app, "GET", path, Value::Null).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(problem["code"], "DATABASE_ERROR");
    }
}

#[tokio::test]
async fn corrupt_stored_time_returns_a_generic_server_error() {
    for column in ["measured_at", "received_at"] {
        let (app, pool) = setup().await;
        seed(
            &pool,
            "corrupt",
            "heartRate",
            Some("2020-01-01T00:00:00Z"),
            "2020-01-01T00:00:00Z",
        )
        .await;
        // Simulate a damaged/legacy table: real generated columns correctly prevent
        // these writes. Keep numeric sort keys so the corrupt text reaches the domain.
        for statement in [
            "CREATE TABLE damaged_measurements AS SELECT * FROM measurements",
            "DROP TABLE measurements",
            "ALTER TABLE damaged_measurements RENAME TO measurements",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        sqlx::query(&format!(
            "UPDATE measurements SET {column}='corrupt timestamp' WHERE id='corrupt'"
        ))
        .execute(&pool)
        .await
        .unwrap();
        for path in ["/api/measurements/latest", "/api/assessments"] {
            let (status, error) = call(&app, "GET", path, Value::Null).await;
            assert_eq!(
                status,
                StatusCode::INTERNAL_SERVER_ERROR,
                "{column}: {error}"
            );
            assert_eq!(error["code"], "DATABASE_ERROR");
            assert_eq!(error["message"], "Database operation failed");
        }
    }
}
