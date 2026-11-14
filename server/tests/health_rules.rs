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
    // The six new registered metrics have no clinical rules and prevent an all-clear.
    assert_eq!(health["totalMetricCount"], 12);
    assert_eq!(health["status"], "insufficientData");
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
async fn unconfigured_metrics_preserve_independent_quality_without_rule_evidence() {
    let app = setup().await;
    let now = Utc::now() - Duration::seconds(1);
    let cases = [
        ("spo2", 97.0, "%", Some(now), "fresh"),
        ("glucose", 90.0, "mg/dL", None, "unknownTime"),
        (
            "visionBlur",
            1.5,
            "score",
            Some(now - Duration::days(2)),
            "stale",
        ),
        (
            "exerciseMinutes",
            45.0,
            "min",
            Some(now + Duration::minutes(2)),
            "clockSkew",
        ),
        ("headache", 0.0, "score", Some(now), "fresh"),
        ("backPain", 3.5, "score", Some(now), "fresh"),
    ];
    let mut saved_ids = Vec::new();
    for (sequence, (metric, value, unit, at, _)) in cases.iter().enumerate() {
        saved_ids
            .push(insert(&app, sequence as i64, metric, *value, unit, *at).await["id"].clone());
    }
    let (status, assessment) = call(&app, "GET", "/api/assessments", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(assessment["metrics"].as_array().unwrap().len(), 12);
    assert_eq!(assessment["health"]["rules"].as_array().unwrap().len(), 12);
    assert_eq!(assessment["health"]["evaluatedMetricCount"], 0);
    assert_eq!(assessment["health"]["status"], "insufficientData");
    for ((metric, _, _, _, quality_status), saved_id) in cases.iter().zip(saved_ids) {
        let quality = &by_metric(&assessment["metrics"], metric)["quality"];
        assert_eq!(quality["measurementId"], saved_id);
        assert_eq!(quality["status"], *quality_status);
        let rule = by_metric(&assessment["health"]["rules"], metric);
        assert_eq!(rule["status"], "notEvaluated");
        assert_eq!(rule["measurementIds"], json!([]));
        assert_eq!(rule["validUntil"], Value::Null);
        assert_eq!(rule["sourceUrls"], json!([]));
    }
}

#[tokio::test]
async fn all_twelve_fresh_observations_keep_unconfigured_rules_out_of_the_evaluated_count() {
    let app = setup().await;
    let now = Utc::now() - Duration::seconds(1);
    let configured = [
        ("heartRate", 72.0, "bpm"),
        ("systolicPressure", 115.0, "mmHg"),
        ("diastolicPressure", 75.0, "mmHg"),
        ("temperature", 36.6, "°C"),
        ("sleepHours", 8.0, "h"),
        ("mood", 4.0, "score"),
    ];
    let unconfigured = [
        ("spo2", 0.0, "%"),
        ("glucose", 500.0, "mg/dL"),
        ("visionBlur", 10.0, "score"),
        ("exerciseMinutes", 0.0, "min"),
        ("headache", 10.0, "score"),
        ("backPain", 10.0, "score"),
    ];
    for (sequence, (metric, value, unit)) in configured.iter().chain(&unconfigured).enumerate() {
        insert(&app, sequence as i64, metric, *value, unit, Some(now)).await;
    }
    let (status, assessment) = call(&app, "GET", "/api/assessments", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(assessment["health"]["status"], "insufficientData");
    assert_eq!(assessment["health"]["evaluatedMetricCount"], 6);
    assert_eq!(assessment["health"]["totalMetricCount"], 12);
    for (metric, _, _) in configured {
        assert_eq!(
            by_metric(&assessment["health"]["rules"], metric)["status"],
            "noAlert"
        );
    }
    for (metric, _, _) in unconfigured {
        assert_eq!(
            by_metric(&assessment["metrics"], metric)["quality"]["status"],
            "fresh"
        );
        assert_eq!(
            by_metric(&assessment["health"]["rules"], metric)["status"],
            "notEvaluated"
        );
    }
}

#[tokio::test]
async fn new_metric_input_bounds_accept_endpoints_and_fractions_but_reject_outside_values() {
    let app = setup().await;
    let now = Utc::now() - Duration::seconds(1);
    let accepted = [
        ("spo2", "%", 0.0),
        ("spo2", "%", 100.0),
        ("spo2", "%", 97.5),
        ("glucose", "mg/dL", 0.01),
        ("glucose", "mg/dL", 90.5),
        ("visionBlur", "score", 0.0),
        ("visionBlur", "score", 10.0),
        ("visionBlur", "score", 1.5),
        ("exerciseMinutes", "min", 0.0),
        ("exerciseMinutes", "min", 1440.0),
        ("exerciseMinutes", "min", 30.5),
        ("headache", "score", 0.0),
        ("headache", "score", 10.0),
        ("headache", "score", 2.5),
        ("backPain", "score", 0.0),
        ("backPain", "score", 10.0),
        ("backPain", "score", 3.5),
    ];
    for (sequence, (metric, unit, value)) in accepted.into_iter().enumerate() {
        let saved = insert(&app, sequence as i64, metric, value, unit, Some(now)).await;
        assert_eq!(saved["value"], value);
    }
    for (sequence, (metric, unit, value)) in [
        ("spo2", "%", -0.01),
        ("spo2", "%", 100.01),
        ("glucose", "mg/dL", 0.0),
        ("glucose", "mg/dL", -0.01),
        ("visionBlur", "score", -0.01),
        ("visionBlur", "score", 10.01),
        ("exerciseMinutes", "min", -0.01),
        ("exerciseMinutes", "min", 1440.01),
        ("headache", "score", -0.01),
        ("headache", "score", 10.01),
        ("backPain", "score", -0.01),
        ("backPain", "score", 10.01),
    ]
    .into_iter()
    .enumerate()
    {
        let (status, problem) = call(
            &app,
            "POST",
            "/api/measurements",
            json!({
                "deviceId":"manual-01", "sessionId":"invalid-bc-bounds", "sequence":sequence,
                "metric":metric, "unit":unit, "value":value, "measuredAt":now.to_rfc3339()
            }),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "{metric} {value}: {problem}"
        );
        assert_eq!(problem["code"], "VALIDATION_ERROR");
        assert_eq!(problem["field"], "value");
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
