use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use tower::ServiceExt;

const RANGE: &str = "from=2020-01-01T00:00:00Z&to=2020-01-08T00:00:00Z";

async fn setup() -> (Router, SqlitePool) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    (space_health_server::app(pool.clone(), "dist"), pool)
}

async fn get(app: &Router, query: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/measurements/series?{query}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    if !status.is_success() {
        assert_eq!(
            response.headers()["content-type"],
            "application/problem+json"
        );
    }
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn seed(
    pool: &SqlitePool,
    id: &str,
    metric: &str,
    value: f64,
    measured: Option<&str>,
    received: &str,
) {
    let unit = match metric {
        "mood" => "score",
        "sleepHours" => "h",
        "temperature" => "°C",
        "systolicPressure" | "diastolicPressure" => "mmHg",
        _ => "bpm",
    };
    sqlx::query("INSERT INTO measurements(id,device_id,astronaut_id,session_id,sequence,metric,value,unit,measured_at,received_at,source) VALUES (?,'manual-01','demo-astronaut',?,0,?,?,?,?,?,'manual')")
        .bind(id).bind(id).bind(metric).bind(value).bind(unit).bind(measured).bind(received)
        .execute(pool).await.unwrap();
}

#[tokio::test]
async fn accepted_large_finite_values_have_a_finite_mean_in_the_json_response() {
    let (app, _) = setup().await;
    for sequence in 0..2 {
        let input = json!({"deviceId":"manual-01", "sessionId":"large-values", "sequence":sequence,
            "metric":"heartRate", "value":1e308, "unit":"bpm", "measuredAt":"2020-01-01T00:00:01Z"});
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/measurements")
                    .header("content-type", "application/json")
                    .body(Body::from(input.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    let (status, result) = get(
        &app,
        "metric=heartRate&from=2020-01-01T00:00:00Z&to=2020-01-01T00:00:10Z&maxPoints=1",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["points"][0]["value"].as_f64(), Some(1e308));
    assert_eq!(result["points"][0]["count"], 2);
    assert_eq!(result["points"][0]["min"].as_f64(), Some(1e308));
    assert_eq!(result["points"][0]["max"].as_f64(), Some(1e308));
}

#[tokio::test]
async fn seven_day_series_aggregates_every_record_beyond_the_raw_history_limit() {
    let (app, pool) = setup().await;
    // Four samples per hour for seven days: 672 rows, exceeding history's 500 cap.
    sqlx::query("WITH RECURSIVE n(x) AS (SELECT 0 UNION ALL SELECT x+1 FROM n WHERE x<671) INSERT INTO measurements(id,device_id,astronaut_id,session_id,sequence,metric,value,unit,measured_at,received_at,source) SELECT printf('pulse-%04d',x),'simulator-01','demo-astronaut','week',x,'heartRate',60+x%8,'bpm',strftime('%Y-%m-%dT%H:%M:%SZ',1577836800+x*900,'unixepoch'),'2020-01-09T00:00:00Z','simulator' FROM n")
        .execute(&pool).await.unwrap();
    let before: Vec<(String, f64, String)> =
        sqlx::query_as("SELECT id,value,measured_at FROM measurements ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    let (status, series) = get(&app, &format!("metric=heartRate&{RANGE}&maxPoints=7")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(series["astronautId"], "demo-astronaut");
    assert_eq!(series["unit"], "bpm");
    assert_eq!(series["aggregation"], "mean");
    assert_eq!(series["intervalMs"], 86_400_000);
    assert_eq!(series["timeBasis"], "measuredAt");
    let points = series["points"].as_array().unwrap();
    assert_eq!(points.len(), 7);
    assert_eq!(points[0]["startAt"], series["from"]);
    assert_eq!(points[6]["endAt"], series["to"]);
    for (day, point) in points.iter().enumerate() {
        assert_eq!(
            point["startAt"],
            format!("2020-01-0{}T00:00:00.000Z", day + 1)
        );
        assert!((point["value"].as_f64().unwrap() - 63.5).abs() < 1e-12);
        assert_eq!(point["min"], 60.0);
        assert_eq!(point["max"], 67.0);
        assert_eq!(point["count"], 96);
        assert_eq!(point["measuredCount"], 96);
        assert_eq!(point["receivedCount"], 0);
    }
    let after: Vec<(String, f64, String)> =
        sqlx::query_as("SELECT id,value,measured_at FROM measurements ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(before, after, "aggregation must not rewrite raw samples");
}

#[tokio::test]
async fn mean_uses_half_open_effective_time_range_and_preserves_gaps_and_provenance() {
    let (app, pool) = setup().await;
    for (id, value, measured, received) in [
        (
            "before",
            500.0,
            Some("2019-12-31T23:59:59.999Z"),
            "2020-01-01T00:00:00Z",
        ),
        (
            "start",
            60.0,
            Some("2020-01-01T05:00:00+05:00"),
            "2020-01-02T00:00:00Z",
        ),
        ("unknown", 100.0, None, "2020-01-01T00:00:00.001Z"),
        ("end-bin", 80.0, None, "2020-01-01T00:00:00.004Z"),
        (
            "exclusive-end",
            500.0,
            Some("2020-01-01T00:00:00.005Z"),
            "2020-01-01T00:00:00Z",
        ),
    ] {
        seed(&pool, id, "heartRate", value, measured, received).await;
    }
    seed(
        &pool,
        "other-metric",
        "temperature",
        37.0,
        Some("2020-01-01T00:00:00Z"),
        "2020-01-01T00:00:00Z",
    )
    .await;
    sqlx::query("INSERT INTO devices VALUES ('other-device','other-astronaut','sensor')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO measurements(id,device_id,astronaut_id,session_id,sequence,metric,value,unit,measured_at,received_at,source) VALUES ('foreign','other-device','other-astronaut','foreign',0,'heartRate',500,'bpm','2020-01-01T00:00:00Z','2020-01-01T00:00:00Z','sensor')")
        .execute(&pool).await.unwrap();
    let query =
        "metric=heartRate&from=2020-01-01T00:00:00Z&to=2020-01-01T00:00:00.005Z&maxPoints=3";
    let (status, series) = get(&app, query).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(series["intervalMs"], 2);
    assert_eq!(series["timeBasis"], "mixed");
    assert_eq!(
        series["points"],
        json!([
            {"startAt":"2020-01-01T00:00:00.000Z","endAt":"2020-01-01T00:00:00.002Z","value":80.0,"min":60.0,"max":100.0,"count":2,"measuredCount":1,"receivedCount":1,"timeBasis":"mixed"},
            {"startAt":"2020-01-01T00:00:00.002Z","endAt":"2020-01-01T00:00:00.004Z","value":null,"min":null,"max":null,"count":0,"measuredCount":0,"receivedCount":0,"timeBasis":null},
            {"startAt":"2020-01-01T00:00:00.004Z","endAt":"2020-01-01T00:00:00.005Z","value":80.0,"min":80.0,"max":80.0,"count":1,"measuredCount":0,"receivedCount":1,"timeBasis":"receivedAt"}
        ])
    );
    let offset_query = query.replace(
        "from=2020-01-01T00:00:00Z",
        "from=2020-01-01T05:00:00%2B05:00",
    );
    assert_eq!(get(&app, &offset_query).await.1, series);
}

#[tokio::test]
async fn discrete_metrics_use_last_effective_value_with_id_ties_and_count_all_rows() {
    let (app, pool) = setup().await;
    for (metric, unit) in [("sleepHours", "h"), ("mood", "score")] {
        for (id, value, measured, received) in [
            (
                "a",
                1.0,
                Some("2020-01-01T00:00:01Z"),
                "2020-01-01T00:00:01Z",
            ),
            (
                "z",
                3.0,
                Some("2020-01-01T00:00:02Z"),
                "2020-01-01T00:00:02Z",
            ),
            ("y", 4.0, None, "2020-01-01T00:00:02Z"),
            (
                "zz-delayed",
                5.0,
                Some("2020-01-01T00:00:00Z"),
                "2020-01-01T00:00:09Z",
            ),
        ] {
            seed(
                &pool,
                &format!("{metric}-{id}"),
                metric,
                value,
                measured,
                received,
            )
            .await;
        }
        let (status, series) = get(&app, &format!("metric={metric}&{RANGE}&maxPoints=1")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(series["aggregation"], "last");
        assert_eq!(series["unit"], unit);
        let point = &series["points"][0];
        assert_eq!(
            point["value"], 3.0,
            "last is not the mean 3.25 or last received 5"
        );
        assert!(point["min"].is_null());
        assert!(point["max"].is_null());
        assert_eq!(point["count"], 4);
        assert_eq!(point["measuredCount"], 3);
        assert_eq!(point["receivedCount"], 1);
        assert_eq!(
            point["timeBasis"], "mixed",
            "provenance describes all contributing rows"
        );
        assert_eq!(series["timeBasis"], "mixed");
    }
}

#[tokio::test]
async fn continuous_metric_units_and_zero_sleep_are_preserved() {
    let (app, pool) = setup().await;
    for (metric, unit) in [
        ("systolicPressure", "mmHg"),
        ("diastolicPressure", "mmHg"),
        ("temperature", "°C"),
        ("sleepHours", "h"),
    ] {
        let value = if metric == "sleepHours" { 0.0 } else { 40.0 };
        seed(
            &pool,
            metric,
            metric,
            value,
            Some("2020-01-01T00:00:00Z"),
            "2020-01-01T00:00:00Z",
        )
        .await;
        let (status, series) = get(&app, &format!("metric={metric}&{RANGE}&maxPoints=1")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(series["unit"], unit);
        assert_eq!(series["points"][0]["value"], value);
        assert_eq!(series["points"][0]["count"], 1);
        if metric != "sleepHours" {
            assert_eq!(series["aggregation"], "mean");
            assert_eq!(series["points"][0]["min"], value);
            assert_eq!(series["points"][0]["max"], value);
        }
    }
}

#[tokio::test]
async fn empty_ranges_keep_bounded_grid_including_short_and_full_calendar_ranges() {
    let (app, _) = setup().await;
    for (range, expected_count) in [
        (RANGE.to_owned(), 200),
        (format!("{RANGE}&maxPoints=500"), 500),
        (
            "from=1969-12-31T23:59:59.999Z&to=1970-01-01T00:00:00.004Z&maxPoints=2".into(),
            2,
        ),
        (
            "from=2020-01-01T00:00:00Z&to=2020-01-01T00:00:00.001Z&maxPoints=500".into(),
            1,
        ),
        (
            "from=0001-01-01T00:00:00Z&to=9999-12-31T23:59:59.999Z&maxPoints=500".into(),
            500,
        ),
    ] {
        let (status, series) = get(&app, &format!("metric=heartRate&{range}")).await;
        assert_eq!(status, StatusCode::OK, "{range}: {series}");
        let points = series["points"].as_array().unwrap();
        assert_eq!(points.len(), expected_count);
        assert_eq!(points.first().unwrap()["startAt"], series["from"]);
        assert_eq!(points.last().unwrap()["endAt"], series["to"]);
        assert!(series["timeBasis"].is_null());
        for point in points {
            for field in ["value", "min", "max", "timeBasis"] {
                assert_eq!(point.get(field), Some(&Value::Null));
            }
            for field in ["count", "measuredCount", "receivedCount"] {
                assert_eq!(point[field], 0);
            }
        }
        for pair in points.windows(2) {
            assert_eq!(pair[0]["endAt"], pair[1]["startAt"]);
        }
    }
}

#[tokio::test]
async fn series_validation_returns_field_specific_problems() {
    let (app, _) = setup().await;
    let valid = format!("metric=heartRate&{RANGE}");
    for (query, field) in [
        (RANGE.to_owned(), "metric"),
        (valid.replace("heartRate", "invalid"), "metric"),
        (valid.replace("heartRate", ""), "metric"),
        ("metric=heartRate&to=2020-01-02T00:00:00Z".into(), "from"),
        ("metric=heartRate&from=2020-01-01T00:00:00Z".into(), "to"),
        (
            valid.replace("2020-01-01T00:00:00Z", "2020-01-01T00:00:00"),
            "from",
        ),
        (
            valid.replace("2020-01-08T00:00:00Z", "2020-02-31T00:00:00Z"),
            "to",
        ),
        (
            valid.replace("2020-01-01T00:00:00Z", "2020-01-01T00:00:00.0000000001Z"),
            "from",
        ),
        (
            valid.replace("2020-01-08T00:00:00Z", "2020-01-08T00:00:00.0011Z"),
            "to",
        ),
        (valid.replace("2020-01-08", "2020-01-01"), "to"),
        (valid.replace("2020-01-08", "2019-01-01"), "to"),
        (valid.replace("2020-01-01", "0000-01-01"), "from"),
        (
            valid.replace("2020-01-01T00:00:00Z", "0001-01-01T00:00:00%2B01:00"),
            "from",
        ),
        (
            valid.replace("2020-01-08T00:00:00Z", "9999-12-31T23:59:59-01:00"),
            "to",
        ),
    ] {
        let (status, problem) = get(&app, &query).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {problem}");
        assert_eq!(problem["code"], "VALIDATION_ERROR");
        assert_eq!(problem["field"], field, "{query}");
    }
    for max_points in ["", "0", "501", "-1", "1.5", "many", "4294967296"] {
        let (status, problem) = get(&app, &format!("{valid}&maxPoints={max_points}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(problem["code"], "VALIDATION_ERROR");
        assert_eq!(problem["field"], "maxPoints");
    }
}

#[tokio::test]
async fn unsupported_and_duplicate_parameters_are_rejected() {
    let (app, _) = setup().await;
    for extra in [
        "astronautId=other",
        "limit=10",
        "cursor=anything",
        "metric=mood",
        "from=2020-01-01T00:00:00Z",
        "maxPoints=1&maxPoints=2",
    ] {
        let (status, problem) = get(&app, &format!("metric=heartRate&{RANGE}&{extra}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{extra}: {problem}");
        assert_eq!(problem["code"], "INVALID_QUERY");
    }
}

#[tokio::test]
async fn database_failure_is_not_reported_as_an_empty_series() {
    let (app, pool) = setup().await;
    pool.close().await;
    let (status, problem) = get(&app, &format!("metric=heartRate&{RANGE}")).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(problem["code"], "DATABASE_ERROR");
    assert!(problem.get("points").is_none());
}
