use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use chrono::{DateTime, Duration, Utc};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::sqlite::SqlitePoolOptions;
use tower::ServiceExt;

async fn setup() -> Router {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    space_health_server::app(pool, "dist")
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
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

async fn insert(
    app: &Router,
    sequence: i64,
    metric: &str,
    value: f64,
    unit: &str,
    at: Option<DateTime<Utc>>,
) -> Value {
    let (status, saved) = call(app, "POST", "/api/measurements", json!({
        "deviceId":"manual-01", "sessionId":"be08-test", "sequence":sequence,
        "metric":metric, "value":value, "unit":unit, "measuredAt":at.map(|time| time.to_rfc3339())
    })).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    saved
}

fn by_metric<'a>(items: &'a Value, metric: &str) -> &'a Value {
    items
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["metric"] == metric)
        .unwrap()
}

#[tokio::test]
async fn empty_assessment_preserves_be06_and_appends_versioned_health_contract() {
    let app = setup().await;
    let (status, assessment) = call(&app, "GET", "/api/assessments", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(assessment["status"], "no-data");
    assert_eq!(assessment["astronautId"], "demo-astronaut");
    assert_eq!(
        assessment["explanation"],
        "Нет измерений. Запустите симулятор или введите показатель."
    );
    let health = &assessment["health"];
    assert_eq!(health["ruleSetVersion"], "be08-demo-1");
    assert_eq!(health["demonstrationOnly"], true);
    assert_eq!(health["status"], "insufficientData");
    assert_eq!(health["evaluatedMetricCount"], 0);
    assert_eq!(
        health["totalMetricCount"].as_u64().unwrap() as usize,
        assessment["metrics"].as_array().unwrap().len()
    );
    assert_eq!(health["education"].as_array().unwrap().len(), 5);
    let rule = by_metric(&health["rules"], "heartRate");
    assert_eq!(rule["ruleId"], "be08-heartRate");
    assert_eq!(rule["ruleVersion"], "be08-demo-1");
    assert_eq!(rule["demonstrationOnly"], true);
    assert_eq!(rule["status"], "insufficientData");
    assert_eq!(rule["measurementIds"], json!([]));
    assert_eq!(rule["validUntil"], Value::Null);
    for field in [
        "title",
        "explanation",
        "suggestedAction",
        "escalation",
        "flightContext",
    ] {
        assert!(!rule[field].as_str().unwrap().is_empty(), "{field}");
    }
    assert!(!rule["sourceUrls"].as_array().unwrap().is_empty());
    let (_, latest) = call(&app, "GET", "/api/measurements/latest", Value::Null).await;
    assert!(latest.get("health").is_none());
}

#[tokio::test]
async fn six_fresh_observations_use_the_same_ids_times_and_policy_as_assessment_quality() {
    let app = setup().await;
    let now = Utc::now() - Duration::seconds(1);
    let cases = [
        ("heartRate", 72.0, "bpm"),
        ("systolicPressure", 115.0, "mmHg"),
        ("diastolicPressure", 75.0, "mmHg"),
        ("temperature", 36.6, "°C"),
        ("sleepHours", 8.0, "h"),
        ("mood", 4.0, "score"),
    ];
    for (sequence, (metric, value, unit)) in cases.iter().enumerate() {
        insert(&app, sequence as i64, metric, *value, unit, Some(now)).await;
    }
    let (_, assessment) = call(&app, "GET", "/api/assessments", Value::Null).await;
    assert_eq!(assessment["status"], "not-evaluated");
    let health = &assessment["health"];
    assert_eq!(health["evaluatedMetricCount"], 6);
    // A registry expansion with unconfigured rules must not silently retain an all-clear.
    assert_eq!(
        health["status"],
        if health["totalMetricCount"] == 6 {
            "noAlert"
        } else {
            "insufficientData"
        }
    );
    for (metric, _, _) in cases {
        let quality = &by_metric(&assessment["metrics"], metric)["quality"];
        let rule = by_metric(&health["rules"], metric);
        assert_eq!(rule["status"], "noAlert");
        assert_eq!(rule["measurementIds"], json!([quality["measurementId"]]));
        let valid_until =
            DateTime::parse_from_rfc3339(rule["validUntil"].as_str().unwrap()).unwrap();
        assert_eq!(
            valid_until,
            now + Duration::seconds(quality["freshForSeconds"].as_i64().unwrap())
        );
    }
}

#[tokio::test]
async fn unknown_latest_receipt_does_not_hide_a_fresh_known_critical_measurement() {
    let app = setup().await;
    let known = insert(
        &app,
        0,
        "systolicPressure",
        181.0,
        "mmHg",
        Some(Utc::now() - Duration::seconds(1)),
    )
    .await;
    let unknown = insert(&app, 1, "systolicPressure", 115.0, "mmHg", None).await;
    let (_, assessment) = call(&app, "GET", "/api/assessments", Value::Null).await;
    assert_eq!(assessment["status"], "unknown-time");
    assert_eq!(assessment["health"]["status"], "critical");
    let quality = by_metric(&assessment["metrics"], "systolicPressure");
    assert_eq!(quality["quality"]["measurementId"], known["id"]);
    assert_eq!(
        quality["latestReceivedQuality"]["measurementId"],
        unknown["id"]
    );
    assert_eq!(quality["latestReceivedQuality"]["status"], "unknownTime");
    let rule = by_metric(&assessment["health"]["rules"], "systolicPressure");
    assert_eq!(rule["measurementIds"], json!([known["id"]]));
    assert_eq!(rule["status"], "critical");
    assert_eq!(rule["escalation"], "urgent");
    assert_eq!(
        by_metric(&assessment["health"]["rules"], "diastolicPressure")["status"],
        "insufficientData"
    );
}

#[tokio::test]
async fn future_and_stale_values_remain_unassessed_even_when_severe_and_just_delivered() {
    let app = setup().await;
    let now = Utc::now();
    insert(
        &app,
        0,
        "systolicPressure",
        210.0,
        "mmHg",
        Some(now + Duration::minutes(2)),
    )
    .await;
    insert(
        &app,
        1,
        "temperature",
        34.0,
        "°C",
        Some(now - Duration::hours(2)),
    )
    .await;
    let (_, assessment) = call(&app, "GET", "/api/assessments", Value::Null).await;
    assert_eq!(assessment["health"]["status"], "insufficientData");
    assert_eq!(assessment["health"]["evaluatedMetricCount"], 0);
    for (metric, status) in [("systolicPressure", "clockSkew"), ("temperature", "stale")] {
        assert_eq!(
            by_metric(&assessment["metrics"], metric)["quality"]["status"],
            status
        );
        let rule = by_metric(&assessment["health"]["rules"], metric);
        assert_eq!(rule["status"], "insufficientData");
        assert_eq!(rule["measurementIds"], json!([]));
        assert_eq!(rule["validUntil"], Value::Null);
    }
}
