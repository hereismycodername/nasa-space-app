use super::*;
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
        ("spo2", 98.0),
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
        assert_eq!(body, json!({"completed":true}));
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
fn lost_task_response_is_not_replayed_because_legacy_patch_changes_the_timestamp() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&attempts);
    let server = MockServer::start(move |method, path, body| {
        assert_eq!(method, "PATCH");
        assert_eq!(path, "/api/tasks/daily-check");
        assert_eq!(body, json!({"completed":true}));
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
    let server = MockServer::start(move |_method, path, _body| {
        if !availability.load(Ordering::Relaxed) {
            return Some((503, json!({"message":"maintenance"})));
        }
        if path == "/api/health" {
            Some((200, json!({"status":"ok","storage":"sqlite"})))
        } else if path == "/api/measurements/latest" {
            Some((200, latest_json()))
        } else if path == "/api/tasks" {
            Some((
                200,
                json!([{"id":"daily-check","title":"Daily check","completedAt":null}]),
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
