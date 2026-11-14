use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    process::{Command, Output},
};
use tower::ServiceExt;

struct Database {
    directory: PathBuf,
}

impl Database {
    fn new() -> Self {
        let directory =
            std::env::temp_dir().join(format!("space-health-admin-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        Self { directory }
    }

    fn url(&self) -> String {
        format!("sqlite://{}", self.directory.join("devices.db").display())
    }

    fn command(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_space-health-admin"))
            .env("DATABASE_URL", self.url())
            .args(args)
            .output()
            .unwrap()
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn success(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}

fn failure(output: Output, exit: i32, code: &str) {
    assert_eq!(output.status.code(), Some(exit));
    assert!(output.stdout.is_empty());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["error"]["code"], code);
    assert!(!error["error"]["message"].as_str().unwrap().is_empty());
}

#[test]
fn registration_is_persistent_idempotent_and_listed_as_a_sensor() {
    let database = Database::new();
    let first = success(database.command(&["register-device", "esp32-cabin-01"]));
    assert_eq!(
        first,
        json!({
            "created": true,
            "device": {"deviceId":"esp32-cabin-01", "astronautId":"demo-astronaut", "source":"sensor"}
        })
    );
    let repeated = success(database.command(&["register-device", "esp32-cabin-01"]));
    assert_eq!(repeated["created"], false);
    assert_eq!(repeated["device"], first["device"]);
    let listed = success(database.command(&["list-devices"]));
    assert_eq!(
        listed["devices"],
        json!([
            first["device"],
            {"deviceId":"manual-01", "astronautId":"demo-astronaut", "source":"manual"},
            {"deviceId":"simulator-01", "astronautId":"demo-astronaut", "source":"simulator"}
        ])
    );
}

#[tokio::test]
async fn conflicting_registrations_preserve_the_original_source_and_owner() {
    let database = Database::new();
    let pool = space_health_server::database::connect(&database.url())
        .await
        .unwrap();
    sqlx::query("INSERT INTO devices VALUES ('another-crew-sensor','another-astronaut','sensor')")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    let before = success(database.command(&["list-devices"]));
    for id in ["manual-01", "simulator-01", "another-crew-sensor"] {
        failure(
            database.command(&["register-device", id]),
            1,
            "DEVICE_CONFLICT",
        );
    }
    assert_eq!(success(database.command(&["list-devices"])), before);
}

#[test]
fn invalid_arguments_and_help_do_not_create_a_database() {
    let database = Database::new();
    for args in [
        vec![],
        vec!["register-device"],
        vec!["list-devices", "extra"],
        vec!["register-device", "esp32", "--source", "manual"],
        vec!["register-device", "esp32", "--astronaut", "another"],
        vec!["unknown-command"],
    ] {
        failure(database.command(&args), 2, "INVALID_ARGUMENTS");
    }
    for id in [String::new(), "a".repeat(129), "я".repeat(65)] {
        failure(
            database.command(&["register-device", &id]),
            2,
            "INVALID_DEVICE_ID",
        );
    }
    assert!(success(database.command(&["--help"]))["commands"].is_array());
    assert!(!database.directory.join("devices.db").exists());
}

#[test]
fn concurrent_registrations_create_exactly_one_device() {
    let database = Database::new();
    // Initialize before racing registrations so this tests the write arbitration.
    success(database.command(&["list-devices"]));
    let children: Vec<_> = (0..3)
        .map(|_| {
            Command::new(env!("CARGO_BIN_EXE_space-health-admin"))
                .env("DATABASE_URL", database.url())
                .args(["register-device", "rpi-01"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    let results: Vec<_> = children
        .into_iter()
        .map(|child| success(child.wait_with_output().unwrap()))
        .collect();
    assert_eq!(
        results
            .iter()
            .filter(|result| result["created"] == true)
            .count(),
        1
    );
    assert!(results
        .iter()
        .all(|result| result["device"] == results[0]["device"]));
}

#[test]
fn default_database_and_database_errors_have_predictable_cli_behavior() {
    let database = Database::new();
    let output = Command::new(env!("CARGO_BIN_EXE_space-health-admin"))
        .env_remove("DATABASE_URL")
        .current_dir(&database.directory)
        .arg("list-devices")
        .output()
        .unwrap();
    assert_eq!(success(output)["devices"].as_array().unwrap().len(), 2);
    assert!(database.directory.join("space-health.db").exists());
    let output = Command::new(env!("CARGO_BIN_EXE_space-health-admin"))
        .env(
            "DATABASE_URL",
            format!(
                "sqlite://{}/missing/devices.db",
                database.directory.display()
            ),
        )
        .arg("list-devices")
        .output()
        .unwrap();
    failure(output, 1, "DATABASE_ERROR");
}

#[tokio::test]
async fn a_registered_device_ingests_sensor_data_with_server_owned_identity() {
    let database = Database::new();
    success(database.command(&["register-device", "esp32-real-01"]));
    let pool = space_health_server::database::connect(&database.url())
        .await
        .unwrap();
    let app = space_health_server::app(pool.clone(), "dist");
    let input = json!({
        "deviceId":"esp32-real-01", "sessionId":"boot-1", "sequence":0,
        "metric":"heartRate", "value":72, "unit":"bpm", "measuredAt":null
    });
    let request = |input: &Value| {
        Request::builder()
            .method("POST")
            .uri("/api/measurements")
            .header("content-type", "application/json")
            .body(Body::from(input.to_string()))
            .unwrap()
    };
    let response = app.clone().oneshot(request(&input)).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let saved: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(saved["deviceId"], "esp32-real-01");
    assert_eq!(saved["source"], "sensor");
    assert_eq!(saved["astronautId"], "demo-astronaut");
    assert_eq!(saved["timeBasis"], "receivedAt");
    assert!(saved["measuredAt"].is_null());
    for field in ["source", "astronautId"] {
        let mut forged = input.clone();
        forged[field] = json!("client-owned");
        assert_eq!(
            app.clone()
                .oneshot(request(&forged))
                .await
                .unwrap()
                .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM measurements WHERE device_id='esp32-real-01'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    drop(app);
    pool.close().await;
}
