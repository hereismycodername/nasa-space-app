//! End-to-end probe used by `npm run test:native-api` with an isolated real API.
use chrono::{Duration as ChronoDuration, Utc};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::{
    error::Error,
    thread,
    time::{Duration, Instant},
};
use ui_http::HttpBackend;
use ui_model::{Backend, HealthStatus, Status};

fn wait_for(
    gui: &mut HttpBackend,
    tui: &mut HttpBackend,
    description: &str,
    condition: impl Fn(&HttpBackend, &HttpBackend) -> bool,
) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        gui.tick();
        tui.tick();
        if condition(gui, tui) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Timed out: {description}; GUI: {}; TUI: {}",
                gui.connection_label(),
                tui.connection_label()
            )
            .into());
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn same_value(gui: &HttpBackend, tui: &HttpBackend, metric: &str, value: f64) -> bool {
    [gui, tui].iter().all(|backend| {
        backend
            .latest(metric)
            .is_some_and(|reading| reading.value == value)
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let base = std::env::args().nth(1).ok_or("Pass isolated API URL")?;
    let browser = Client::builder().timeout(Duration::from_secs(5)).build()?;
    let mut gui = HttpBackend::connect(&base)?;
    let mut tui = HttpBackend::connect(&base)?;
    wait_for(&mut gui, &mut tui, "initial server snapshot", |a, b| {
        a.overview().link_up
            && b.overview().link_up
            && !a.tasks().is_empty()
            && !b.tasks().is_empty()
    })?;
    assert!(!gui.is_demo());
    assert_eq!(gui.overview().score, None);
    assert!(gui.latest("hr").is_none());
    gui.submit_measurement("hr", 72.25)?;
    wait_for(
        &mut gui,
        &mut tui,
        "GUI write visible in both clients",
        |a, b| a.pending_writes() == 0 && same_value(a, b, "hr", 72.25),
    )?;
    let latest: Value = browser
        .get(format!("{base}/api/measurements/latest"))
        .send()?
        .error_for_status()?
        .json()?;
    let heart = latest["metrics"]
        .as_array()
        .ok_or("Missing metrics")?
        .iter()
        .find(|m| m["metric"] == "heartRate")
        .ok_or("Missing heart rate")?;
    assert_eq!(heart["latestKnownMeasurement"]["value"], 72.25);
    assert_eq!(heart["latestKnownMeasurement"]["source"], "manual");
    wait_for(
        &mut gui,
        &mut tui,
        "shared fresh health guidance",
        |a, b| {
            [a, b].iter().all(|client| {
                client.health_rule("hr").is_some_and(|rule| {
                    rule.current_status() == HealthStatus::NoAlert
                        && rule.measurement_ids
                            == [heart["latestKnownMeasurement"]["id"]
                                .as_str()
                                .unwrap_or_default()]
                })
            })
        },
    )?;
    assert_eq!(gui.indicator_status("hr"), Status::Normal);
    assert_eq!(
        gui.health_report()
            .evaluation
            .as_ref()
            .map(|value| value.education.len()),
        Some(5)
    );

    let session = format!("native-smoke-{}", uuid::Uuid::new_v4());
    let mut sequence = 0;
    let mut post = |metric: &str,
                    value: f64,
                    unit: &str,
                    measured_at: Option<String>|
     -> Result<Value, Box<dyn Error>> {
        let payload = json!({"deviceId":"manual-01","sessionId":session,"sequence":sequence,
            "metric":metric,"value":value,"unit":unit,"measuredAt":measured_at});
        sequence += 1;
        let first = browser
            .post(format!("{base}/api/measurements"))
            .json(&payload)
            .send()?
            .error_for_status()?;
        assert_eq!(first.status().as_u16(), 201);
        let saved: Value = first.json()?;
        let repeat = browser
            .post(format!("{base}/api/measurements"))
            .json(&payload)
            .send()?
            .error_for_status()?;
        assert_eq!(repeat.status().as_u16(), 200);
        assert_eq!(repeat.json::<Value>()?["id"], saved["id"]);
        Ok(saved)
    };
    post("sleepHours", 7.5, "h", Some(Utc::now().to_rfc3339()))?;
    wait_for(
        &mut gui,
        &mut tui,
        "browser write visible in both clients",
        |a, b| same_value(a, b, "sleep", 7.5),
    )?;
    tui.submit_measurement("mood", 3.5)?;
    wait_for(&mut gui, &mut tui, "fractional TUI mood write", |a, b| {
        b.pending_writes() == 0 && same_value(a, b, "mood", 3.5)
    })?;
    let history: Value = browser
        .get(format!("{base}/api/measurements?metric=mood"))
        .send()?
        .error_for_status()?
        .json()?;
    assert_eq!(history["items"][0]["value"], 3.5);

    gui.set_task_completed("daily-check", true)?;
    wait_for(
        &mut gui,
        &mut tui,
        "task completed in every client",
        |a, b| {
            a.pending_writes() == 0
                && [a, b].iter().all(|v| {
                    v.tasks()
                        .iter()
                        .any(|t| t.id == "daily-check" && t.completed_at.is_some())
                })
        },
    )?;
    let tasks: Value = browser
        .get(format!("{base}/api/tasks"))
        .send()?
        .error_for_status()?
        .json()?;
    assert!(tasks[0]["completedAt"].is_string());
    tui.set_task_completed("daily-check", false)?;
    wait_for(&mut gui, &mut tui, "task undone in every client", |a, b| {
        b.pending_writes() == 0
            && [a, b].iter().all(|v| {
                v.tasks()
                    .iter()
                    .any(|t| t.id == "daily-check" && t.completed_at.is_none())
            })
    })?;

    post(
        "systolicPressure",
        118.5,
        "mmHg",
        Some((Utc::now() - ChronoDuration::hours(2)).to_rfc3339()),
    )?;
    post(
        "diastolicPressure",
        76.25,
        "mmHg",
        Some(Utc::now().to_rfc3339()),
    )?;
    post("temperature", 36.625, "°C", Some(Utc::now().to_rfc3339()))?;
    post("temperature", 37.0, "°C", None)?;
    wait_for(
        &mut gui,
        &mut tui,
        "quality and known-time selection",
        |a, b| {
            same_value(a, b, "sbp", 118.5)
                && same_value(a, b, "dbp", 76.25)
                && same_value(a, b, "temp", 36.625)
                && a.measurement_detail("sbp").to_lowercase().contains("stale")
                && b.measurement_detail("temp")
                    .to_lowercase()
                    .contains("unknown")
        },
    )?;
    wait_for(
        &mut gui,
        &mut tui,
        "server-aggregated UTC history",
        |a, b| {
            [a, b].iter().all(|v| {
                let s = v.data_series("hr");
                s.from_ms < s.to_ms
                    && s.points.iter().any(|p| p.value == Some(72.25))
                    && s.points.iter().any(|p| p.value.is_none())
            })
        },
    )?;
    // Optional capabilities are advertised by the server; old six-metric APIs remain supported.
    let optional = [
        ("spo2", 98.5, "%"),
        ("glucose", 94.25, "mg/dL"),
        ("visionBlur", 0.0, "score"),
        ("exerciseMinutes", 15.5, "min"),
        ("headache", 2.5, "score"),
        ("backPain", 0.0, "score"),
    ];
    let supported: Vec<_> = optional
        .into_iter()
        .filter(|(metric, _, _)| {
            latest["metrics"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item["metric"] == *metric))
        })
        .collect();
    for (metric, value, unit) in &supported {
        post(metric, *value, unit, Some(Utc::now().to_rfc3339()))?;
    }
    if !supported.is_empty() {
        wait_for(
            &mut gui,
            &mut tui,
            "optional metrics shared without invented health rules",
            |a, b| {
                supported
                    .iter()
                    .all(|(metric, value, _)| same_value(a, b, metric, *value))
                    && [a, b].iter().all(|client| {
                        supported.iter().all(|(metric, _, _)| {
                            client.health_rule(metric).is_some_and(|rule| {
                                rule.current_status() == HealthStatus::NotEvaluated
                            })
                        })
                    })
            },
        )?;
        tui.submit_measurement("exerciseMinutes", 20.25)?;
        wait_for(
            &mut gui,
            &mut tui,
            "fractional optional TUI write",
            |a, b| b.pending_writes() == 0 && same_value(a, b, "exerciseMinutes", 20.25),
        )?;
        assert_eq!(gui.indicator_status("spo2"), Status::Unassessed);
        assert_eq!(gui.overview().total_indicators, 6 + supported.len());
    } else {
        assert!(tui.submit_measurement("spo2", 98.0).is_err());
    }
    let critical = post(
        "systolicPressure",
        181.0,
        "mmHg",
        Some(Utc::now().to_rfc3339()),
    )?;
    wait_for(
        &mut gui,
        &mut tui,
        "shared critical rule with matching measurement and actions",
        |a, b| {
            [a, b].iter().all(|client| {
                client.health_rule("sbp").is_some_and(|rule| {
                    rule.current_status() == HealthStatus::Critical
                        && rule.measurement_ids == [critical["id"].as_str().unwrap_or_default()]
                        && !rule.suggested_action.is_empty()
                        && !rule.flight_context.is_empty()
                        && !rule.source_urls.is_empty()
                })
            })
        },
    )?;
    assert_eq!(gui.system_status("heart"), Status::Act);
    assert_eq!(tui.health_report().status, HealthStatus::Critical);
    post(
        "heartRate",
        88.5,
        "bpm",
        Some((Utc::now() + ChronoDuration::seconds(120)).to_rfc3339()),
    )?;
    wait_for(
        &mut gui,
        &mut tui,
        "accepted clock skew stays connected and explicit",
        |a, b| {
            same_value(a, b, "hr", 88.5)
                && a.overview().link_up
                && b.overview().link_up
                && a.measurement_detail("hr").contains("clockSkew")
                && b.measurement_detail("hr").contains("clockSkew")
        },
    )?;
    assert!(gui.submit_measurement("mood", 6.0).is_err());
    assert_eq!(gui.indicator_status("hr"), Status::Unassessed);
    assert!(!gui
        .health_rule("hr")
        .is_some_and(|rule| rule.current_status() == HealthStatus::NoAlert));
    assert_eq!(gui.overview().score, None);
    assert!(gui.alerts().is_empty());
    println!("PASS: two native HTTP clients and browser API share measurements, fractional values, tasks, provenance, quality, UTC history, idempotent storage and shared server health guidance.");
    Ok(())
}
