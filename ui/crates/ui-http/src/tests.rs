use super::*;
use crate::catalog::METRICS;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::AtomicUsize;
use std::thread::{self, JoinHandle};
use std::time::Duration;

fn record(id: &str, value: f64, measured_at: Option<&str>) -> Value {
    json!({"id":id,"deviceId":"manual-01","sessionId":"test-session","sequence":0,
        "metric":"heartRate","value":value,"unit":"bpm","measuredAt":measured_at,
        "receivedAt":"2026-11-15T01:00:00Z","source":"manual"})
}

fn quality(status: &str) -> Value {
    json!({"status":status,"freshForSeconds":15,"ageSeconds":if status == "fresh" { Some(1.0) } else { None },"clockSkew":false})
}

fn latest_json() -> Value {
    json!({"astronautId":"demo-astronaut","evaluatedAt":"2026-11-15T01:00:00Z",
        "metrics":METRICS.iter().map(|metric| json!({"metric":metric.api,
            "latestKnownMeasurement":null,"latestReceivedMeasurement":null,
            "quality":quality("missing"),"latestReceivedQuality":quality("missing")})).collect::<Vec<_>>()})
}

fn cached_backend(latest: Value) -> (HttpBackend, mpsc::Receiver<Command>) {
    let mut state = State::new();
    state.online = true;
    state.latest = Some(serde_json::from_value(latest).unwrap());
    state.snapshot_at = Some(Instant::now());
    let (tx, rx) = mpsc::sync_channel(1);
    (
        HttpBackend {
            api_url: "http://127.0.0.1:8080".into(),
            state: Arc::new(Mutex::new(state)),
            commands: tx,
            stop: Arc::new(AtomicBool::new(false)),
            session: "test-session".into(),
            next_sequence: 0,
            observed_generation: 0,
        },
        rx,
    )
}

#[test]
fn origin_normalization_prevents_duplicate_api_and_rejects_credentials_or_hidden_queries() {
    assert_eq!(
        normalize_url("http://127.0.0.1:8080/api/").unwrap(),
        "http://127.0.0.1:8080"
    );
    assert_eq!(
        normalize_url("https://example.test/").unwrap(),
        "https://example.test"
    );
    for invalid in [
        "ftp://host",
        "http://user:secret@host",
        "http://host/api?token=a",
        "http://host/api#fragment",
        "http://host/other",
    ] {
        assert!(normalize_url(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn manual_submission_uses_api_units_and_unique_keys_without_optimistic_values() {
    let (mut backend, rx) = cached_backend(latest_json());
    assert!(backend.submit_measurement("mood", 3.5).is_ok());
    assert_eq!(backend.pending_writes(), 1);
    assert!(backend.latest("mood").is_none());
    assert!(backend.submit_measurement("temp", 36.8).is_err());
    let Command::Measurement(first) = rx.try_recv().unwrap() else {
        panic!("measurement expected");
    };
    assert_eq!(first.metric, "mood");
    assert_eq!(first.unit, "score");
    assert_eq!(first.device_id, "manual-01");
    lock(&backend.state).pending = 0;
    backend.submit_measurement("temp", 36.8).unwrap();
    let Command::Measurement(second) = rx.try_recv().unwrap() else {
        panic!("measurement expected");
    };
    assert_eq!(second.metric, "temperature");
    assert_eq!(second.unit, "°C");
    assert_eq!(second.session_id, first.session_id);
    assert_eq!(second.sequence, first.sequence + 1);
    for (id, value) in [
        ("hr", 0.0),
        ("sleep", 24.1),
        ("mood", 0.5),
        ("temp", f64::NAN),
        ("spo2", 100.1),
    ] {
        assert!(backend.submit_measurement(id, value).is_err());
    }
}

#[test]
fn newest_unknown_receipt_does_not_replace_known_value_or_hide_its_provenance() {
    let mut data = latest_json();
    data["metrics"][0]["latestKnownMeasurement"] =
        record("known", 72.5, Some("2026-11-15T00:00:00Z"));
    data["metrics"][0]["latestReceivedMeasurement"] = record("unknown", 99.0, None);
    data["metrics"][0]["quality"] = quality("stale");
    data["metrics"][0]["latestReceivedQuality"] = quality("unknownTime");
    let (backend, _) = cached_backend(data.clone());
    assert_eq!(backend.latest("hr").unwrap().value, 72.5);
    assert_eq!(backend.indicator_status("hr"), Status::Unassessed);
    let detail = backend.measurement_detail("hr");
    assert!(detail.contains("stale"));
    assert!(detail.contains("newest receipt: 99"));
    assert!(detail.contains("unknownTime"));
    data["metrics"][0]["latestKnownMeasurement"] = Value::Null;
    let (backend, _) = cached_backend(data);
    assert_eq!(backend.latest("hr").unwrap().value, 99.0);
    assert!(backend
        .measurement_detail("hr")
        .contains("receipt time is not measurement time"));
}

#[test]
fn freshness_ages_and_offline_cache_cannot_be_mistaken_for_a_live_confirmation() {
    let mut data = latest_json();
    data["metrics"][0]["latestKnownMeasurement"] =
        record("known", 72.0, Some("2026-11-15T00:59:59Z"));
    data["metrics"][0]["quality"] = quality("fresh");
    let (mut backend, _) = cached_backend(data);
    {
        let mut state = lock(&backend.state);
        state.online = false;
        state.error = Some("connection refused".into());
        state.snapshot_at = Some(Instant::now() - Duration::from_secs(20));
    }
    assert_eq!(backend.latest("hr").unwrap().value, 72.0);
    assert!(backend
        .measurement_detail("hr")
        .contains("Offline / cached; stale"));
    assert!(backend.connection_label().contains("Offline"));
    assert!(backend.submit_measurement("hr", 80.0).is_err());
    let q: dto::Quality = serde_json::from_value(quality("fresh")).unwrap();
    assert_eq!(q.label(14.0), "fresh");
    assert_eq!(q.label(14.001), "stale");
}

#[test]
fn accepted_future_measurement_preserves_clock_skew_without_rejecting_the_snapshot() {
    let mut data = latest_json();
    let future = record("future", 75.0, Some("2026-11-15T01:02:00Z"));
    data["metrics"][0]["latestKnownMeasurement"] = future.clone();
    data["metrics"][0]["latestReceivedMeasurement"] = future;
    let mut skew = quality("clockSkew");
    skew["clockSkew"] = json!(true);
    data["metrics"][0]["quality"] = skew.clone();
    data["metrics"][0]["latestReceivedQuality"] = skew;
    let snapshot: dto::Latest = serde_json::from_value(data.clone()).unwrap();
    assert!(worker::test_validate_latest(&snapshot));
    let (backend, _) = cached_backend(data);
    assert!(backend.overview().link_up);
    assert_eq!(backend.latest("hr").unwrap().value, 75.0);
    assert_eq!(backend.indicator_status("hr"), Status::Unassessed);
    assert!(backend.measurement_detail("hr").contains("clockSkew"));
    assert!(backend
        .measurement_detail("hr")
        .contains("clock skew reported by server"));
}

#[test]
fn real_time_series_keeps_gaps_and_unknown_time_sample_count() {
    let series: dto::Series = serde_json::from_value(
        json!({"metric":"heartRate","unit":"bpm","aggregation":"mean",
        "from":"2026-11-15T00:00:00Z","to":"2026-11-15T03:00:00Z","timeBasis":"mixed",
        "points":[{"startAt":"2026-11-15T00:00:00Z","value":71.5,"receivedCount":1},
            {"startAt":"2026-11-15T01:00:00Z","value":null,"receivedCount":0},
            {"startAt":"2026-11-15T02:00:00Z","value":75.0,"receivedCount":0}]}),
    )
    .unwrap();
    let timed = series.into_timed().unwrap();
    assert_eq!(timed.points.len(), 3);
    assert!(timed.points[1].value.is_none());
    assert_eq!(timed.points[2].at_ms - timed.points[0].at_ms, 7_200_000);
    assert!(timed.note.contains("1 samples use receipt time"));
}

struct MockServer {
    url: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl MockServer {
    fn start(handler: impl Fn(&str, &str, Value) -> Option<(u16, Value)> + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let quit = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            while !quit.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        if let Some((method, path, body)) = read_request(&mut stream) {
                            if let Some((status, json)) = handler(&method, &path, body) {
                                let body = json.to_string();
                                let header = format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                                let _ = stream.write_all(header.as_bytes());
                                let _ = stream.write_all(body.as_bytes());
                            }
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("{error}"),
                }
            }
        });
        Self {
            url,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

fn read_request(stream: &mut TcpStream) -> Option<(String, String, Value)> {
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    let header_end = loop {
        let n = stream.read(&mut buffer).ok()?;
        if n == 0 {
            return None;
        }
        bytes.extend_from_slice(&buffer[..n]);
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
    let mut first = headers.lines().next()?.split_whitespace();
    let method = first.next()?.to_string();
    let path = first.next()?.to_string();
    let length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    while bytes.len() < header_end + length {
        let n = stream.read(&mut buffer).ok()?;
        if n == 0 {
            return None;
        }
        bytes.extend_from_slice(&buffer[..n]);
    }
    let body = if length == 0 {
        Value::Null
    } else {
        serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap()
    };
    Some((method, path, body))
}

#[test]
fn lost_post_response_replays_identical_payload_and_only_then_reports_saved() {
    let attempts = Arc::new(Mutex::new(Vec::<Value>::new()));
    let seen = Arc::clone(&attempts);
    let server = MockServer::start(move |method, path, body| {
        assert_eq!(method, "POST");
        assert_eq!(path, "/api/measurements");
        let mut seen = seen.lock().unwrap();
        seen.push(body.clone());
        if seen.len() == 1 {
            return None;
        }
        let mut saved = body;
        saved["id"] = json!("already-saved");
        saved["source"] = json!("manual");
        saved["receivedAt"] = json!("2026-11-15T01:00:00Z");
        Some((200, saved))
    });
    let state = Mutex::new(State::new());
    lock(&state).pending = 1;
    let payload = dto::NewMeasurement {
        device_id: "manual-01",
        session_id: "lost-response".into(),
        sequence: 7,
        metric: "heartRate",
        value: 73.25,
        unit: "bpm",
        measured_at: "2026-11-15T01:00:00Z".into(),
    };
    worker::test_write(&server.url, Command::Measurement(payload), &state);
    let attempts = attempts.lock().unwrap();
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0], attempts[1]);
    assert_eq!(lock(&state).pending, 0);
    assert_eq!(lock(&state).messages.len(), 1);
    assert!(lock(&state).messages[0].starts_with("Saved to server:"));
}

#[test]
fn task_validation_error_is_not_retried_or_reported_as_saved() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&attempts);
    let server = MockServer::start(move |method, path, body| {
        assert_eq!(method, "PATCH");
        assert_eq!(path, "/api/tasks/daily-check");
        assert_eq!(body, json!({"missionDate":"2026-11-14","completed":true}));
        count.fetch_add(1, Ordering::Relaxed);
        Some((
            404,
            json!({"code":"TASK_NOT_FOUND","message":"Unknown task"}),
        ))
    });
    let state = Mutex::new(State::new());
    lock(&state).pending = 1;
    worker::test_write(
        &server.url,
        Command::Task {
            id: "daily-check".into(),
            mission_date: "2026-11-14".into(),
            completed: true,
        },
        &state,
    );
    assert_eq!(attempts.load(Ordering::Relaxed), 1);
    assert_eq!(lock(&state).pending, 0);
    assert!(lock(&state).messages[0].contains("Save not confirmed"));
    assert!(!lock(&state).messages[0].starts_with("Task saved"));
}

#[test]
fn lost_task_response_refreshes_state_without_reporting_an_unconfirmed_save() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&attempts);
    let server = MockServer::start(move |method, path, body| {
        assert_eq!(method, "PATCH");
        assert_eq!(path, "/api/tasks/daily-check");
        assert_eq!(body, json!({"missionDate":"2026-11-14","completed":true}));
        count.fetch_add(1, Ordering::Relaxed);
        // The server applied the change, but the acknowledgement was lost.
        None
    });
    let state = Mutex::new(State::new());
    lock(&state).pending = 1;
    worker::test_write(
        &server.url,
        Command::Task {
            id: "daily-check".into(),
            mission_date: "2026-11-14".into(),
            completed: true,
        },
        &state,
    );
    assert_eq!(attempts.load(Ordering::Relaxed), 1);
    assert_eq!(lock(&state).pending, 0);
    assert!(lock(&state).messages[0].starts_with("Save not confirmed"));
    assert!(!lock(&state).messages[0].starts_with("Task saved"));
}

#[test]
fn worker_recovers_after_connection_failure_without_inventing_demo_data() {
    let ready = Arc::new(AtomicBool::new(false));
    let availability = Arc::clone(&ready);
    let completed = Arc::new(AtomicBool::new(false));
    let mark = Arc::clone(&completed);
    let server = MockServer::start(move |method, path, body| {
        if !availability.load(Ordering::Relaxed) {
            return Some((503, json!({"message":"maintenance"})));
        }
        if path == "/api/health" {
            Some((200, json!({"status":"ok","storage":"sqlite"})))
        } else if path == "/api/measurements/latest" {
            Some((200, latest_json()))
        } else if path == "/api/tasks/daily-check" {
            assert_eq!(method, "PATCH");
            assert_eq!(body, json!({"missionDate":"2026-11-14","completed":true}));
            mark.store(true, Ordering::Relaxed);
            Some((
                200,
                json!({"missionDate":"2026-11-14","id":"daily-check","title":"Daily check","completedAt":"2026-11-15T00:00:01Z"}),
            ))
        } else if path == "/api/tasks" {
            Some((
                200,
                json!({"missionDate":"2026-11-14","tasks":[{"id":"daily-check","title":"Daily check","completedAt":if mark.load(Ordering::Relaxed) {Some("2026-11-15T00:00:01Z")} else {None}}]}),
            ))
        } else {
            Some((503, json!({"message":"history temporarily unavailable"})))
        }
    });
    let mut backend = HttpBackend::connect(&server.url).unwrap();
    wait_for(|| backend.connection_label().starts_with("Offline:"));
    assert!(backend.latest("hr").is_none());
    assert!(!backend.is_demo());
    assert!(backend.crew().is_empty());
    ready.store(true, Ordering::Relaxed);
    wait_for(|| backend.overview().link_up);
    assert_eq!(backend.selected_crew(), "demo-astronaut");
    assert_eq!(backend.tasks().len(), 1);
    assert!(backend.latest("hr").is_none());
    assert!(backend
        .take_messages()
        .iter()
        .any(|m| m.contains("Connection restored")));
    backend.set_task_completed("daily-check", true).unwrap();
    wait_for(|| backend.pending_writes() == 0 && backend.tasks()[0].completed_at.is_some());
    assert!(completed.load(Ordering::Relaxed));
    assert!(backend
        .take_messages()
        .iter()
        .any(|m| m.starts_with("Task saved to server:")));
    ready.store(false, Ordering::Relaxed);
    wait_for(|| !backend.overview().link_up);
    assert_eq!(
        backend.tasks().len(),
        1,
        "confirmed tasks remain cached while offline"
    );
    assert!(backend.set_task_completed("daily-check", true).is_err());
    let closing = Instant::now();
    drop(backend);
    assert!(closing.elapsed() < Duration::from_millis(100));
}

fn wait_for(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for background update"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn health_latest() -> Value {
    let mut latest = latest_json();
    for (index, metric) in METRICS.iter().enumerate() {
        let value = match metric.id {
            "sleep" => 8.0,
            "mood" => 4.0,
            "temp" => 36.5,
            "visionBlur" | "headache" | "backPain" => 2.5,
            _ => 75.0,
        };
        let mut measurement = record(
            &format!("known-{}", metric.api),
            value,
            Some("2026-11-15T00:59:59Z"),
        );
        measurement["metric"] = json!(metric.api);
        measurement["unit"] = json!(metric.unit);
        latest["metrics"][index]["latestKnownMeasurement"] = measurement.clone();
        latest["metrics"][index]["latestReceivedMeasurement"] = measurement;
        latest["metrics"][index]["quality"] = quality("fresh");
        latest["metrics"][index]["latestReceivedQuality"] = quality("fresh");
    }
    latest
}

fn assessment_json() -> Value {
    json!({"astronautId":"demo-astronaut","evaluatedAt":"2026-11-15T01:00:00Z", "health":{
        "ruleSetVersion":"be08-demo-1","demonstrationOnly":true,"status":"noAlert",
        "explanation":"No configured rules triggered","disclaimer":"Prototype, not a diagnosis",
        "emergencyGuidance":"Involve crew immediately for urgent symptoms", "evaluatedMetricCount":METRICS.len(),"totalMetricCount":METRICS.len(),
        "rules":METRICS.iter().map(|metric| json!({"ruleId":format!("be08-{}",metric.api),"ruleVersion":"be08-demo-1","metric":metric.api,
            "title":metric.name,"status":"noAlert","explanation":"Reference rule explanation","measurementIds":[format!("known-{}",metric.api)],
            "validUntil":"2026-11-15T01:00:15Z","suggestedAction":"Follow the mission routine","escalation":"none",
            "flightContext":"A flight-context explanation","sourceUrls":["https://www.nasa.gov/hrp/hazards/"],"demonstrationOnly":true})).collect::<Vec<_>>(),
        "education":(["gravity","isolation","radiation","distance","environment"].iter().map(|id| json!({"id":id,"title":id,"explanation":"Flight hazard","suggestedAction":"Follow the mission procedure","sourceUrls":["https://www.nasa.gov/hrp/hazards/"]})).collect::<Vec<_>>())}})
}

fn assessed_backend(assessment: Value) -> HttpBackend {
    let (backend, _) = cached_backend(health_latest());
    let value = serde_json::from_value(assessment).unwrap();
    assert!(health::validate(&value).is_ok());
    {
        let mut state = lock(&backend.state);
        state.assessment = Some(value);
        state.assessment_at = Some(Instant::now());
    }
    backend
}

#[test]
fn health_statuses_and_actions_come_from_the_server_with_sources_and_all_hazards() {
    let mut assessment = assessment_json();
    // The client deliberately does not re-implement a numeric threshold for 75 bpm.
    assessment["health"]["rules"][0]["status"] = json!("critical");
    assessment["health"]["rules"][0]["escalation"] = json!("urgent");
    assessment["health"]["status"] = json!("critical");
    let backend = assessed_backend(assessment);
    assert_eq!(backend.health_report().status, HealthStatus::Critical);
    assert_eq!(backend.indicator_status("hr"), Status::Act);
    assert_eq!(backend.system_status("heart"), Status::Act);
    let rule = backend.health_rule("hr").unwrap();
    assert!(rule.current);
    assert_eq!(rule.escalation, Escalation::Urgent);
    assert!(rule.detail_text().contains("Flight context"));
    assert!(rule.detail_text().contains("https://www.nasa.gov"));
    assert_eq!(
        backend.health_report().evaluation.unwrap().education.len(),
        5
    );
    assert_eq!(backend.overview().score, None);
}

#[test]
fn expired_mismatched_offline_and_failed_assessments_never_keep_a_current_green_result() {
    let backend = assessed_backend(assessment_json());
    assert_eq!(backend.health_report().status, HealthStatus::NoAlert);
    assert_eq!(backend.indicator_status("hr"), Status::Normal);
    lock(&backend.state).assessment_at = Some(Instant::now() - Duration::from_secs(16));
    assert_eq!(
        backend.health_report().status,
        HealthStatus::InsufficientData
    );
    assert_eq!(backend.indicator_status("hr"), Status::Unassessed);
    assert!(!backend.health_rule("hr").unwrap().current);

    lock(&backend.state).assessment_at = Some(Instant::now());
    lock(&backend.state).latest.as_mut().unwrap().metrics[0]
        .latest_known_measurement
        .as_mut()
        .unwrap()
        .id = "replacement".into();
    assert_eq!(backend.indicator_status("hr"), Status::Unassessed);
    assert!(backend
        .health_rule("hr")
        .unwrap()
        .unavailable_reason
        .unwrap()
        .contains("Measurement changed"));
    // Other matched current values retain their own valid assessment.
    assert_eq!(backend.indicator_status("sleep"), Status::Normal);

    lock(&backend.state).online = false;
    assert_eq!(backend.health_report().status, HealthStatus::NotEvaluated);
    assert_eq!(backend.indicator_status("sleep"), Status::Unassessed);
    assert!(backend
        .health_rule("sleep")
        .unwrap()
        .detail_text()
        .contains("Historical / not current"));
    lock(&backend.state).online = true;
    lock(&backend.state).assessment_error = Some("HTTP 503".into());
    assert_eq!(backend.health_report().status, HealthStatus::NotEvaluated);
    assert!(backend.health_report().message.contains("refresh failed"));
}

#[test]
fn old_be06_response_without_health_remains_supported_and_unassessed() {
    let assessment: dto::Assessment = serde_json::from_value(json!({"astronautId":"demo-astronaut","evaluatedAt":"2026-11-15T01:00:00Z","metrics":[],"status":"not-evaluated"})).unwrap();
    assert!(health::validate(&assessment).is_ok());
    let (backend, _) = cached_backend(health_latest());
    lock(&backend.state).assessment = Some(assessment);
    assert_eq!(backend.health_report().status, HealthStatus::NotEvaluated);
    assert!(backend
        .health_report()
        .message
        .contains("does not publish health rules"));
    assert_eq!(backend.indicator_status("hr"), Status::Unassessed);
}

#[test]
fn invalid_health_linkage_is_rejected_before_it_can_be_displayed_as_current() {
    let mut value = assessment_json();
    value["health"]["rules"][0]["measurementIds"] = json!([]);
    let invalid: dto::Assessment = serde_json::from_value(value).unwrap();
    assert!(health::validate(&invalid).is_err());
    let mut value = assessment_json();
    value["health"]["rules"][0]["validUntil"] = json!("invalid");
    assert!(health::validate(&serde_json::from_value(value).unwrap()).is_err());
}

#[test]
fn optional_metrics_use_server_capabilities_and_fractional_input_bounds() {
    let mut core = latest_json();
    core["metrics"].as_array_mut().unwrap().retain(|entry| {
        METRICS
            .iter()
            .any(|metric| metric.required && metric.api == entry["metric"])
    });
    assert!(worker::test_validate_latest(
        &serde_json::from_value(core.clone()).unwrap()
    ));
    let (mut old, _) = cached_backend(core);
    assert_eq!(old.overview().total_indicators, 6);
    assert_eq!(old.systems().len(), 3);
    assert!(old.indicators("lungs").is_empty());
    assert!(old
        .submit_measurement("spo2", 98.0)
        .unwrap_err()
        .contains("not available"));

    let full = health_latest();
    assert!(worker::test_validate_latest(
        &serde_json::from_value(full.clone()).unwrap()
    ));
    let (mut backend, receiver) = cached_backend(full);
    assert_eq!(backend.overview().total_indicators, 12);
    assert_eq!(backend.systems().len(), 7);
    for (id, value, unit) in [
        ("spo2", 0.0, "%"),
        ("spo2", 98.5, "%"),
        ("glucose", 94.25, "mg/dL"),
        ("visionBlur", 0.0, "score"),
        ("exerciseMinutes", 15.5, "min"),
        ("headache", 3.5, "score"),
        ("backPain", 0.0, "score"),
    ] {
        backend.submit_measurement(id, value).unwrap();
        let Command::Measurement(payload) = receiver.try_recv().unwrap() else {
            panic!("measurement expected")
        };
        assert_eq!(payload.metric, id);
        assert_eq!(payload.value, value);
        assert_eq!(payload.unit, unit);
        lock(&backend.state).pending = 0;
    }
    for (id, value) in [
        ("spo2", 100.1),
        ("glucose", 0.0),
        ("visionBlur", 10.1),
        ("exerciseMinutes", 1440.1),
        ("headache", -0.1),
        ("backPain", f64::INFINITY),
    ] {
        assert!(backend.submit_measurement(id, value).is_err());
    }
}

#[test]
fn optional_unimplemented_rules_remain_neutral_despite_fresh_data() {
    let mut assessment = assessment_json();
    for rule in assessment["health"]["rules"].as_array_mut().unwrap() {
        if METRICS
            .iter()
            .any(|metric| !metric.required && metric.api == rule["metric"])
        {
            rule["status"] = json!("notEvaluated");
            rule["measurementIds"] = json!([]);
            rule["validUntil"] = Value::Null;
        }
    }
    assessment["health"]["evaluatedMetricCount"] = json!(6);
    assessment["health"]["status"] = json!("insufficientData");
    let backend = assessed_backend(assessment);
    assert_eq!(
        backend.health_report().status,
        HealthStatus::InsufficientData
    );
    for metric in METRICS.iter().filter(|metric| !metric.required) {
        assert_eq!(
            backend.health_rule(metric.id).unwrap().current_status(),
            HealthStatus::NotEvaluated
        );
        assert_eq!(backend.indicator_status(metric.id), Status::Unassessed);
    }
    assert_eq!(backend.system_status("msk"), Status::Unassessed);
}

#[test]
fn old_six_metric_server_receives_only_supported_history_queries() {
    let requests = Arc::new(Mutex::new(Vec::<String>::new()));
    let observed = Arc::clone(&requests);
    let server = MockServer::start(move |_, path, _| match path {
        "/api/health" => Some((200, json!({"status":"ok","storage":"sqlite"}))),
        "/api/measurements/latest" => {
            let mut value = latest_json();
            value["metrics"].as_array_mut().unwrap().retain(|entry| {
                METRICS
                    .iter()
                    .any(|m| m.required && m.api == entry["metric"])
            });
            Some((200, value))
        }
        "/api/tasks" => Some((200, json!({"missionDate":"2026-11-14","tasks":[]}))),
        "/api/assessments" => Some((
            200,
            json!({"astronautId":"demo-astronaut","evaluatedAt":"2026-11-15T01:00:00Z"}),
        )),
        _ if path.starts_with("/api/measurements/series?") => {
            let url = reqwest::Url::parse(&format!("http://local{path}")).unwrap();
            let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
            let metric = METRICS
                .iter()
                .find(|metric| metric.api == query["metric"])
                .unwrap();
            observed.lock().unwrap().push(metric.api.into());
            assert!(metric.required, "Optional query sent to an older server");
            Some((
                200,
                json!({"metric":metric.api,"unit":metric.unit,"aggregation":"mean","from":query["from"],"to":query["to"],"timeBasis":"mixed","points":[]}),
            ))
        }
        _ => Some((404, json!({"message":"not available"}))),
    });
    let backend = HttpBackend::connect(&server.url).unwrap();
    wait_for(|| backend.data_series("mood").to_ms > 0);
    assert!(backend.overview().link_up);
    assert_eq!(backend.overview().total_indicators, 6);
    assert_eq!(requests.lock().unwrap().len(), 6);
    assert_eq!(backend.health_report().status, HealthStatus::NotEvaluated);
    assert!(backend
        .health_report()
        .message
        .contains("does not publish health rules"));
}

#[test]
fn research_insight_preserves_server_content_and_labels_cached_snapshots() {
    let mut value = assessment_json();
    let insight: Value = serde_json::from_str(include_str!("../../../../docs/examples/research-insight.json")).unwrap();
    value["researchInsights"] = json!([insight]);
    let backend = assessed_backend(value);
    let received = backend.research_insight("hr").unwrap();
    assert!(received.observation.contains("72 bpm"));
    assert!(received.observation.contains("Снимок сервера"));
    assert_eq!(received.actions[0].kind, ui_model::research::ActionKind::Record);
    assert!(backend.research_insight("glucose").is_none());
    lock(&backend.state).online = false;
    assert!(backend.research_insight("hr").unwrap().observation.contains("сохранённый снимок"));
}
