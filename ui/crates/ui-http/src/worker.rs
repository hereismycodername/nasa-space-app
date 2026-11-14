use crate::{catalog::METRICS, dto, lock, Command, State};
use chrono::{DateTime, SecondsFormat, Utc};
use reqwest::blocking::{Client, RequestBuilder};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::io::Read;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, RecvTimeoutError},
    Arc, Mutex,
};
use std::time::{Duration, Instant};
use ui_model::TimedSeries;

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const HISTORY_INTERVAL: Duration = Duration::from_secs(10);
const BODY_LIMIT: u64 = 2 * 1024 * 1024;

struct Failure {
    message: String,
    retryable: bool,
}

impl Failure {
    fn protocol(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: false,
        }
    }
}

fn response<T: DeserializeOwned>(request: RequestBuilder) -> Result<T, Failure> {
    let response = request.send().map_err(|error| Failure {
        message: error.to_string(),
        retryable: true,
    })?;
    let status = response.status();
    let mut bytes = Vec::new();
    response
        .take(BODY_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| Failure {
            message: format!("Cannot read API response: {error}"),
            retryable: true,
        })?;
    if bytes.len() as u64 > BODY_LIMIT {
        return Err(Failure::protocol("API response exceeds 2 MiB"));
    }
    if !status.is_success() {
        let description = serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|v| v.get("message").and_then(|s| s.as_str()).map(str::to_owned))
            .unwrap_or_else(|| "API request failed".into());
        return Err(Failure {
            message: format!("HTTP {status}: {description}"),
            retryable: status.is_server_error(),
        });
    }
    serde_json::from_slice(&bytes).map_err(|error| Failure {
        message: format!("Invalid API response: {error}"),
        retryable: true,
    })
}

fn validate_latest(latest: &dto::Latest) -> Result<(), Failure> {
    if latest.astronaut_id.is_empty() || latest.metrics.len() != METRICS.len() {
        return Err(Failure::protocol("Unexpected latest-measurements schema"));
    }
    dto::timestamp_ms(&latest.evaluated_at).map_err(Failure::protocol)?;
    for metric in &METRICS {
        let entries: Vec<_> = latest
            .metrics
            .iter()
            .filter(|m| m.metric == metric.api)
            .collect();
        if entries.len() != 1 {
            return Err(Failure::protocol(
                "Missing or repeated metric in latest response",
            ));
        }
        let entry = entries[0];
        for measurement in [
            &entry.latest_known_measurement,
            &entry.latest_received_measurement,
        ]
        .into_iter()
        .flatten()
        {
            if measurement.id.is_empty()
                || measurement.metric != metric.api
                || measurement.unit != metric.unit
                || metric.validate(measurement.value).is_err()
            {
                return Err(Failure::protocol(
                    "Latest value does not match the metric contract",
                ));
            }
            dto::timestamp_ms(&measurement.received_at).map_err(Failure::protocol)?;
            if let Some(time) = &measurement.measured_at {
                dto::timestamp_ms(time).map_err(Failure::protocol)?;
            }
        }
        if entry
            .latest_known_measurement
            .as_ref()
            .is_some_and(|v| v.measured_at.is_none())
        {
            return Err(Failure::protocol("Known measurement is missing measuredAt"));
        }
        for quality in [&entry.quality, &entry.latest_received_quality] {
            if !matches!(
                quality.status.as_str(),
                "fresh" | "stale" | "missing" | "unknownTime" | "clockSkew"
            ) {
                return Err(Failure::protocol("Unknown quality status from server"));
            }
        }
    }
    Ok(())
}

fn poll(
    client: &Client,
    base: &str,
    stop: &AtomicBool,
) -> Result<(dto::Latest, Vec<dto::Task>), Failure> {
    let health: dto::HealthResponse = response(client.get(format!("{base}/api/health")))?;
    if health.status != "ok" || health.storage != "sqlite" {
        return Err(Failure::protocol("Server storage is not ready"));
    }
    if stop.load(Ordering::Relaxed) {
        return Err(Failure::protocol("Client closed"));
    }
    let latest: dto::Latest = response(client.get(format!("{base}/api/measurements/latest")))?;
    validate_latest(&latest)?;
    if stop.load(Ordering::Relaxed) {
        return Err(Failure::protocol("Client closed"));
    }
    let tasks: Vec<dto::Task> = response(client.get(format!("{base}/api/tasks")))?;
    Ok((latest, tasks))
}

fn write(client: &Client, base: &str, command: &Command) -> Result<String, Failure> {
    match command {
        Command::Measurement(payload) => {
            let saved: dto::Measurement = response(
                client
                    .post(format!("{base}/api/measurements"))
                    .json(payload),
            )?;
            if saved.id.is_empty()
                || saved.device_id != payload.device_id
                || saved.session_id != payload.session_id
                || saved.sequence != payload.sequence
                || saved.metric != payload.metric
                || saved.value != payload.value
                || saved.unit != payload.unit
                || saved.measured_at.as_deref() != Some(payload.measured_at.as_str())
            {
                return Err(Failure::protocol(
                    "Server confirmation does not match the submitted measurement",
                ));
            }
            Ok(format!(
                "Saved to server: {} {} ({})",
                saved.value, saved.unit, saved.metric
            ))
        }
        Command::Task { id, completed } => {
            let mut url = reqwest::Url::parse(&format!("{base}/api/tasks/"))
                .map_err(|_| Failure::protocol("Invalid API origin"))?;
            url.path_segments_mut()
                .map_err(|_| Failure::protocol("Invalid task URL"))?
                .pop_if_empty()
                .push(id);
            let saved: dto::Task = response(
                client
                    .patch(url)
                    .json(&serde_json::json!({"completed": completed})),
            )?;
            if saved.id != *id || saved.completed_at.is_some() != *completed {
                return Err(Failure::protocol(
                    "Server confirmation does not match the task update",
                ));
            }
            Ok(format!(
                "Task saved to server: {} ({})",
                saved.title,
                if *completed {
                    "completed"
                } else {
                    "not completed"
                }
            ))
        }
        Command::Refresh => Ok(String::new()),
    }
}

fn perform_write(
    client: &Client,
    base: &str,
    command: Command,
    state: &Mutex<State>,
    stop: &AtomicBool,
) {
    if matches!(command, Command::Refresh) {
        return;
    }
    let mut result = Err(Failure::protocol("Client closed before confirmation"));
    // Measurements have an explicit idempotency key. Legacy BE-06 task PATCH
    // rewrites its completion timestamp, so an ambiguous result must not replay.
    let attempts = if matches!(command, Command::Measurement(_)) {
        3
    } else {
        1
    };
    for attempt in 0..attempts {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        result = write(client, base, &command);
        if !result.as_ref().is_err_and(|error| error.retryable) || attempt + 1 == attempts {
            break;
        }
        // Retry preserves the original body, timestamp, session and sequence.
        for _ in 0..5 {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let mut state = lock(state);
    state.pending = 0;
    match result {
        Ok(message) => state.message(message),
        Err(error) => state.message(format!(
            "Save not confirmed: {}. Check current server data before trying again.",
            error.message
        )),
    }
}

struct SeriesCycle {
    from: String,
    to: String,
    next: usize,
    values: HashMap<String, TimedSeries>,
}

impl SeriesCycle {
    fn new(evaluated_at: &str) -> Result<Self, String> {
        let to_ms = dto::timestamp_ms(evaluated_at)?;
        let to = DateTime::<Utc>::from_timestamp_millis(to_ms).ok_or("Invalid server timestamp")?;
        let from = to
            .checked_sub_signed(chrono::Duration::days(7))
            .ok_or("Invalid history range")?;
        Ok(Self {
            from: from.to_rfc3339_opts(SecondsFormat::Millis, true),
            to: to.to_rfc3339_opts(SecondsFormat::Millis, true),
            next: 0,
            values: HashMap::new(),
        })
    }

    fn fetch_next(&mut self, client: &Client, base: &str) -> Result<bool, Failure> {
        let metric = &METRICS[self.next];
        let series: dto::Series = response(
            client
                .get(format!("{base}/api/measurements/series"))
                .query(&[
                    ("metric", metric.api),
                    ("from", &self.from),
                    ("to", &self.to),
                    ("maxPoints", "168"),
                ]),
        )?;
        if series.metric != metric.api
            || series.unit != metric.unit
            || series.from != self.from
            || series.to != self.to
        {
            return Err(Failure::protocol(
                "Series response does not match its requested metric or range",
            ));
        }
        self.values.insert(
            metric.id.into(),
            series.into_timed().map_err(Failure::protocol)?,
        );
        self.next += 1;
        Ok(self.next == METRICS.len())
    }
}

pub(crate) fn run(
    base: String,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    receiver: Receiver<Command>,
) {
    let client = match Client::builder()
        .timeout(Duration::from_secs(3))
        .connect_timeout(Duration::from_secs(1))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            let mut state = lock(&state);
            state.error = Some(format!("Cannot create HTTP client: {error}"));
            state.generation = state.generation.wrapping_add(1);
            return;
        }
    };
    let mut next_poll = Instant::now();
    let mut next_history = Instant::now();
    let mut cycle: Option<SeriesCycle> = None;
    while !stop.load(Ordering::Relaxed) {
        // Prioritise edits between requests, including during a six-metric refresh.
        let wait = if cycle.is_some() || Instant::now() >= next_poll {
            Duration::ZERO
        } else {
            next_poll
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(250))
        };
        match receiver.recv_timeout(wait) {
            Ok(command) => {
                perform_write(&client, &base, command, &state, &stop);
                next_poll = Instant::now();
                next_history = Instant::now();
                cycle = None;
            }
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if Instant::now() >= next_poll {
            let requested_at = Instant::now();
            let result = poll(&client, &base, &stop);
            next_poll = Instant::now() + POLL_INTERVAL;
            let mut state = lock(&state);
            match result {
                Ok((latest, tasks)) => {
                    if state.error.is_some() {
                        state.message("Connection restored; shared server data refreshed".into());
                    }
                    state.online = true;
                    state.error = None;
                    if Instant::now() >= next_history && cycle.is_none() {
                        cycle = SeriesCycle::new(&latest.evaluated_at).ok();
                        next_history = Instant::now() + HISTORY_INTERVAL;
                    }
                    state.latest = Some(latest);
                    state.snapshot_at = Some(requested_at);
                    state.tasks = tasks;
                }
                Err(error) => {
                    if state.online {
                        state.message(
                            "Connection lost; displaying cached values. Writes are unavailable."
                                .into(),
                        );
                    }
                    state.online = false;
                    state.error = Some(error.message);
                    cycle = None;
                }
            }
            state.generation = state.generation.wrapping_add(1);
        }
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if let Some(current) = &mut cycle {
            match current.fetch_next(&client, &base) {
                Ok(true) => {
                    let mut state = lock(&state);
                    state.series = std::mem::take(&mut current.values);
                    state.series_error = None;
                    state.generation = state.generation.wrapping_add(1);
                    cycle = None;
                }
                Ok(false) => {}
                Err(error) => {
                    let mut state = lock(&state);
                    state.series_error = Some(error.message);
                    state.generation = state.generation.wrapping_add(1);
                    cycle = None;
                }
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn test_write(base: &str, command: Command, state: &Mutex<State>) {
    let client = Client::builder()
        .timeout(Duration::from_secs(1))
        .no_proxy()
        .build()
        .unwrap();
    perform_write(&client, base, command, state, &AtomicBool::new(false));
}

#[cfg(test)]
pub(crate) fn test_validate_latest(latest: &dto::Latest) -> bool {
    validate_latest(latest).is_ok()
}
