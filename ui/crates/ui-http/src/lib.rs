//! Shared asynchronous BE-06 adapter for the window and terminal applications.
//!
//! The UI reads a small cached snapshot. A single worker owns HTTP I/O and writes;
//! no network call or database access happens on the drawing thread.

mod catalog;
mod dto;
mod worker;

use catalog::{metric, METRICS};
use std::collections::{HashMap, VecDeque};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, SyncSender},
    Arc, Mutex, MutexGuard,
};
use std::time::Instant;
use ui_model::*;

pub(crate) struct State {
    online: bool,
    error: Option<String>,
    latest: Option<dto::Latest>,
    snapshot_at: Option<Instant>,
    tasks: Vec<dto::Task>,
    series: HashMap<String, TimedSeries>,
    series_error: Option<String>,
    pending: usize,
    generation: u64,
    messages: VecDeque<String>,
}

impl State {
    fn new() -> Self {
        Self {
            online: false,
            error: None,
            latest: None,
            snapshot_at: None,
            tasks: vec![],
            series: HashMap::new(),
            series_error: None,
            pending: 0,
            generation: 0,
            messages: VecDeque::new(),
        }
    }

    fn message(&mut self, message: String) {
        if self.messages.len() == 32 {
            self.messages.pop_front();
        }
        self.messages.push_back(message);
        self.generation = self.generation.wrapping_add(1);
    }

    fn elapsed(&self) -> f64 {
        self.snapshot_at
            .map_or(0.0, |at| at.elapsed().as_secs_f64())
    }

    fn entry(&self, id: &str) -> Option<&dto::LatestMetric> {
        let api = metric(id)?.api;
        self.latest
            .as_ref()?
            .metrics
            .iter()
            .find(|item| item.metric == api)
    }
}

pub(crate) fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    // Recovery keeps the UI usable if a worker unexpectedly panics.
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) enum Command {
    Measurement(dto::NewMeasurement),
    Task { id: String, completed: bool },
    Refresh,
}

/// A single-astronaut client for the existing BE-06 service.
pub struct HttpBackend {
    api_url: String,
    state: Arc<Mutex<State>>,
    commands: SyncSender<Command>,
    stop: Arc<AtomicBool>,
    session: String,
    next_sequence: i64,
    observed_generation: u64,
}

impl HttpBackend {
    /// Validate the origin and start the worker. Unavailable servers stay visibly
    /// disconnected; they never cause a switch to synthetic demo data.
    pub fn connect(api_url: &str) -> Result<Self, String> {
        let api_url = normalize_url(api_url)?;
        let state = Arc::new(Mutex::new(State::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (commands, receiver) = mpsc::sync_channel(1);
        let thread_state = Arc::clone(&state);
        let thread_stop = Arc::clone(&stop);
        let thread_url = api_url.clone();
        std::thread::Builder::new()
            .name("astrocheck-api".into())
            .spawn(move || worker::run(thread_url, thread_state, thread_stop, receiver))
            .map_err(|error| format!("Cannot start API client: {error}"))?;
        Ok(Self {
            api_url,
            state,
            commands,
            stop,
            session: uuid::Uuid::new_v4().to_string(),
            next_sequence: 0,
            observed_generation: 0,
        })
    }

    fn enqueue(&mut self, command: Command) -> Result<(), String> {
        let mut state = lock(&self.state);
        if state.pending != 0 {
            return Err("A write is still awaiting server confirmation".into());
        }
        if !state.online {
            return Err("Server unavailable. This value has not been sent or saved.".into());
        }
        // Mark pending before sending, so a quick worker cannot finish before the flag is set.
        state.pending = 1;
        if let Err(error) = self.commands.try_send(command) {
            state.pending = 0;
            return Err(match error {
                mpsc::TrySendError::Full(_) => "Client is busy; try again shortly".into(),
                mpsc::TrySendError::Disconnected(_) => {
                    "API client stopped; restart the application".into()
                }
            });
        }
        state.generation = state.generation.wrapping_add(1);
        Ok(())
    }

    fn unsupported(&mut self, feature: &str) {
        lock(&self.state).message(format!("{feature} is not provided by the shared API"));
    }
}

impl Drop for HttpBackend {
    fn drop(&mut self) {
        // A closing window never waits for I/O. Requests have a short timeout and
        // the worker checks this flag between every request and retry.
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.commands.try_send(Command::Refresh);
    }
}

fn normalize_url(input: &str) -> Result<String, String> {
    let mut url = reqwest::Url::parse(input)
        .map_err(|_| "API URL must be an absolute HTTP(S) origin".to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "/" | "/api" | "/api/")
    {
        return Err("API URL must be an HTTP(S) origin (optional /api), without credentials, query or fragment".into());
    }
    url.set_path("");
    Ok(url.as_str().trim_end_matches('/').to_string())
}

impl Backend for HttpBackend {
    fn is_demo(&self) -> bool {
        false
    }

    fn connection_label(&self) -> String {
        let state = lock(&self.state);
        if state.online {
            format!(
                "Connected: {}{}",
                self.api_url,
                state
                    .series_error
                    .as_ref()
                    .map(|e| format!("; history refresh: {e}"))
                    .unwrap_or_default()
            )
        } else if let Some(error) = &state.error {
            format!(
                "Offline: {error}; {}",
                if state.latest.is_some() {
                    "showing cached data"
                } else {
                    "no data loaded"
                }
            )
        } else {
            format!("Connecting to {}", self.api_url)
        }
    }

    fn measurement_detail(&self, indicator: &str) -> String {
        let state = lock(&self.state);
        let Some(entry) = state.entry(indicator) else {
            return "No data loaded".into();
        };
        let Some(value) = entry.selected() else {
            return "No measurements recorded".into();
        };
        let quality = entry.selected_quality();
        let time = value
            .measured_at
            .as_deref()
            .unwrap_or("unknown (receipt time is not measurement time)");
        let mut label = format!(
            "{}{}; source: {}; measured: {}; received: {}{}",
            if state.online {
                ""
            } else {
                "Offline / cached; "
            },
            quality.label(state.elapsed()),
            value.source,
            time,
            value.received_at,
            if quality.clock_skew {
                "; clock skew reported by server"
            } else {
                ""
            }
        );
        if let Some(received) = &entry.latest_received_measurement {
            if received.id != value.id {
                label.push_str(&format!(
                    "; newest receipt: {} {} at {} ({}, source {}, measured {})",
                    received.value,
                    received.unit,
                    received.received_at,
                    entry.latest_received_quality.label(state.elapsed()),
                    received.source,
                    received.measured_at.as_deref().unwrap_or("unknown")
                ));
            }
        }
        label.push_str("; no clinical assessment");
        label
    }

    fn data_series(&self, indicator: &str) -> TimedSeries {
        let state = lock(&self.state);
        let mut result = state
            .series
            .get(indicator)
            .cloned()
            .unwrap_or_else(|| TimedSeries {
                note: "Waiting for seven-day history".into(),
                ..TimedSeries::default()
            });
        if !state.online {
            result.note = format!("Offline / cached. {}", result.note);
        }
        if let Some(error) = &state.series_error {
            result.note = format!("History refresh failed: {error}. {}", result.note);
        }
        result
    }

    fn submit_measurement(&mut self, indicator: &str, value: f64) -> Result<(), String> {
        let metric = metric(indicator).ok_or_else(|| "Unsupported measurement".to_string())?;
        metric.validate(value)?;
        let next = self
            .next_sequence
            .checked_add(1)
            .ok_or_else(|| "Session sequence exhausted; restart the application".to_string())?;
        let command = Command::Measurement(dto::NewMeasurement {
            device_id: "manual-01",
            session_id: self.session.clone(),
            sequence: self.next_sequence,
            metric: metric.api,
            value,
            unit: metric.unit,
            measured_at: dto::now_text(),
        });
        self.enqueue(command)?;
        self.next_sequence = next;
        Ok(())
    }

    fn pending_writes(&self) -> usize {
        lock(&self.state).pending
    }
    fn tasks(&self) -> Vec<TaskInfo> {
        lock(&self.state).tasks.iter().map(Into::into).collect()
    }
    fn set_task_completed(&mut self, id: &str, completed: bool) -> Result<(), String> {
        if !lock(&self.state).tasks.iter().any(|task| task.id == id) {
            return Err("Unknown task".into());
        }
        self.enqueue(Command::Task {
            id: id.into(),
            completed,
        })
    }

    fn overview(&self) -> Overview {
        let state = lock(&self.state);
        Overview {
            mission_day: 0,
            env_id: "shared-api".into(),
            env_name: "Shared server".into(),
            score: None,
            link_up: state.online,
            autonomous: false,
            rtt_s: 0.0,
            due_count: 0,
            total_indicators: METRICS.len(),
        }
    }
    fn systems(&self) -> Vec<SystemInfo> {
        catalog::systems()
    }
    fn indicators(&self, system: &str) -> Vec<IndicatorInfo> {
        METRICS
            .iter()
            .filter(|item| item.system == system)
            .map(|item| item.info())
            .collect()
    }
    fn system_status(&self, system: &str) -> Status {
        if self
            .indicators(system)
            .iter()
            .any(|item| self.latest(&item.id).is_some())
        {
            Status::Unassessed
        } else {
            Status::NoData
        }
    }
    fn indicator_status(&self, indicator: &str) -> Status {
        if self.latest(indicator).is_some() {
            Status::Unassessed
        } else {
            Status::NoData
        }
    }
    fn latest(&self, indicator: &str) -> Option<Reading> {
        lock(&self.state)
            .entry(indicator)?
            .selected()
            .map(|v| Reading {
                day: 0,
                value: v.value,
            })
    }
    // The demo trait's integer mission-day chart cannot express UTC times or gaps.
    // Live views deliberately use data_series() instead.
    fn history(&self, _indicator: &str) -> Vec<Reading> {
        vec![]
    }
    fn thresholds(&self, _indicator: &str) -> Thresholds {
        Thresholds {
            ok: (f64::NAN, f64::NAN),
            warn: (f64::NAN, f64::NAN),
        }
    }
    fn evaluate(&self, _indicator: &str, _value: f64) -> Evaluation {
        Evaluation {
            status: Status::Unassessed,
            target: "No clinical assessment".into(),
            advice: Advice {
                headline: "Medical rules are not connected".into(),
                actions: vec![],
                escalate: None,
            },
        }
    }
    fn is_due(&self, _indicator: &str) -> bool {
        false
    }
    fn environments(&self) -> Vec<EnvInfo> {
        vec![self.environment()]
    }
    fn environment(&self) -> EnvInfo {
        EnvInfo {
            id: "shared-api".into(),
            name: "Shared server".into(),
            gravity: "Not configured".into(),
            comms: "Local HTTP API".into(),
            rtt_s: 0.0,
            typical_dose: 0.0,
            escalation: String::new(),
            notes: HashMap::new(),
        }
    }
    fn hazard_label(&self, key: &str) -> String {
        key.into()
    }
    fn queue(&self, _max_rows: usize) -> (QueueSummary, Vec<QueueRow>) {
        (QueueSummary::default(), vec![])
    }
    fn health(&self) -> Health {
        let state = lock(&self.state);
        Health {
            checks: vec![Check {
                name: "Shared API and SQLite".into(),
                ok: state.online,
                detail: if state.online {
                    "Server health and measurements loaded".into()
                } else {
                    state.error.clone().unwrap_or_else(|| "Connecting".into())
                },
            }],
            events: state.messages.iter().rev().cloned().collect(),
            platform: "HTTP client; storage is owned by the Rust server".into(),
        }
    }
    fn blackout(&self) -> bool {
        false
    }
    fn log(&mut self, indicator: &str, value: f64) -> LogResult {
        if let Err(error) = self.submit_measurement(indicator, value) {
            lock(&self.state).message(error);
        }
        LogResult {
            status: Status::Unassessed,
            value,
        }
    }
    fn set_environment(&mut self, _env_id: &str) {
        self.unsupported("Mission environment selection");
    }
    fn set_blackout(&mut self, _on: bool) {
        self.unsupported("Simulated blackout");
    }
    fn run_selftest(&mut self) {
        let _ = self.commands.try_send(Command::Refresh);
    }
    fn export(&mut self) -> Result<String, String> {
        Err("Downlink export is not provided by the shared API".into())
    }
    fn crew(&self) -> Vec<CrewMember> {
        let state = lock(&self.state);
        let Some(latest) = &state.latest else {
            return vec![];
        };
        vec![CrewMember {
            id: latest.astronaut_id.clone(),
            name: latest.astronaut_id.clone(),
            title: "Shared profile".into(),
            status: if latest.metrics.iter().any(|m| m.selected().is_some()) {
                Status::Unassessed
            } else {
                Status::NoData
            },
            score: None,
            last_contact_s: state.elapsed() as u64,
            open_alerts: 0,
        }]
    }
    fn selected_crew(&self) -> String {
        lock(&self.state)
            .latest
            .as_ref()
            .map(|v| v.astronaut_id.clone())
            .unwrap_or_default()
    }
    fn ack_alert(&mut self, _alert_id: u64) {
        self.unsupported("Finding acknowledgement");
    }
    fn send_message(&mut self, _text: &str) {
        self.unsupported("Uplink messages");
    }
    fn tick(&mut self) -> bool {
        let generation = lock(&self.state).generation;
        let changed = generation != self.observed_generation;
        self.observed_generation = generation;
        changed
    }
    fn take_messages(&mut self) -> Vec<String> {
        lock(&self.state).messages.drain(..).collect()
    }
}

#[cfg(test)]
mod tests;
