//! Persistent backend for the AstroCheck interfaces.
//!
//! * Storage and rules of ingestion are the team backend's (`be/`: validation, idempotent insert keyed by
//!   device/session/sequence, SQLite schema and migrations 0001/0002 and the query indexes as 0004), extended by migration 0003 (crew, acknowledged
//!   findings, messages, settings) and by the metrics of the interface (`metrics.rs`).
//! * Status, thresholds and advice come from the rules engine of `ui-model` (the team backend has no medical rules yet).
//!   The downlink queue and self-test shown by the interfaces are still simulated by that engine.
//! * The interface never waits for the disk: reads are plain memory, writes are a channel send, one database thread does
//!   the SQLite work (`worker.rs`) and notices changes made by other processes on the same file.
//!
//! ```no_run
//! let mut b = ui_backend::SqliteBackend::open(ui_backend::Options::new("astrocheck.db")).unwrap();
//! use ui_model::Backend;
//! b.log("hr", 72.0);
//! ```

#![allow(clippy::too_many_arguments)]

mod be;
pub mod metrics;
mod store;
mod worker;

use std::path::PathBuf;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use ui_model::*;
use worker::{Cmd, Evt, Handle, Open, Shared};

pub struct Options {
    pub path: PathBuf,
    /// fill an empty database with the demo crew's history (through the normal ingestion path)
    pub seed: bool,
}

/// Where the database lives unless told otherwise: `ASTROCHECK_DB`, else the user's data directory.
pub fn default_db_path() -> PathBuf {
    if let Some(p) = std::env::var_os("ASTROCHECK_DB") {
        return PathBuf::from(p);
    }
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_DATA_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("AstroCheck");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("astrocheck.db")
}

impl Options {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Options { path: path.into(), seed: true }
    }
    pub fn without_seed(mut self) -> Self {
        self.seed = false;
        self
    }
}

/// Counters for tests and the health screen.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub writes: u64,
    pub duplicates: u64,
    pub failures: u64,
    pub reloads: u64,
    pub pending: u64,
    pub p50_us: u64,
    pub p95_us: u64,
    pub max_us: u64,
    pub last_reload_us: u64,
}

pub struct SqliteBackend {
    inner: DemoBackend,
    h: Handle,
    shared: Arc<Shared>,
    notes: Vec<String>,
    path: PathBuf,
}

impl SqliteBackend {
    pub fn open(opts: Options) -> Result<Self, String> {
        let path = opts.path.clone();
        let (h, snap, crew) = worker::spawn(Open { path: opts.path, seed: opts.seed })?;
        let mut inner = DemoBackend::empty(&crew);
        inner.apply_snapshot(&snap);
        inner.note(&format!("DB: {} opened", path.display()));
        let shared = h.shared.clone();
        Ok(SqliteBackend { inner, h, shared, notes: vec![], path })
    }

    pub fn stats(&self) -> Stats {
        let s = &self.shared;
        Stats {
            writes: s.writes.load(Relaxed),
            duplicates: s.duplicates.load(Relaxed),
            failures: s.failures.load(Relaxed),
            reloads: s.reloads.load(Relaxed),
            pending: s.pending.load(Relaxed),
            p50_us: s.p50_us.load(Relaxed),
            p95_us: s.p95_us.load(Relaxed),
            max_us: s.max_us.load(Relaxed),
            last_reload_us: s.last_reload_us.load(Relaxed),
        }
    }

    /// Ask the database thread to reload and wait until it has been applied (for tests; the UI never needs this).
    pub fn sync(&mut self, timeout: std::time::Duration) -> bool {
        let before = self.shared.reloads.load(Relaxed);
        self.send(Cmd::Resync);
        let t = std::time::Instant::now();
        while t.elapsed() < timeout {
            self.tick();
            if self.shared.reloads.load(Relaxed) > before && self.shared.pending.load(Relaxed) == 0 {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        false
    }

    fn send(&self, c: Cmd) {
        self.shared.pending.fetch_add(1, Relaxed);
        if self.h.tx.send(c).is_err() {
            self.shared.pending.fetch_sub(1, Relaxed);
        }
    }

    fn crew_id(&self) -> String {
        self.inner.selected_crew()
    }
}

impl Drop for SqliteBackend {
    fn drop(&mut self) {
        let _ = self.h.tx.send(Cmd::Stop);
        if let Some(j) = self.h.join.take() {
            let _ = j.join();
        }
    }
}

impl Backend for SqliteBackend {
    fn overview(&self) -> Overview {
        self.inner.overview()
    }
    fn systems(&self) -> Vec<SystemInfo> {
        self.inner.systems()
    }
    fn indicators(&self, system: &str) -> Vec<IndicatorInfo> {
        self.inner.indicators(system)
    }
    fn system_status(&self, system: &str) -> Status {
        self.inner.system_status(system)
    }
    fn indicator_status(&self, indicator: &str) -> Status {
        self.inner.indicator_status(indicator)
    }
    fn latest(&self, indicator: &str) -> Option<Reading> {
        self.inner.latest(indicator)
    }
    fn history(&self, indicator: &str) -> Vec<Reading> {
        self.inner.history(indicator)
    }
    fn thresholds(&self, indicator: &str) -> Thresholds {
        self.inner.thresholds(indicator)
    }
    fn evaluate(&self, indicator: &str, value: f64) -> Evaluation {
        self.inner.evaluate(indicator, value)
    }
    fn is_due(&self, indicator: &str) -> bool {
        self.inner.is_due(indicator)
    }
    fn environments(&self) -> Vec<EnvInfo> {
        self.inner.environments()
    }
    fn environment(&self) -> EnvInfo {
        self.inner.environment()
    }
    fn hazard_label(&self, key: &str) -> String {
        self.inner.hazard_label(key)
    }
    fn queue(&self, max_rows: usize) -> (QueueSummary, Vec<QueueRow>) {
        self.inner.queue(max_rows)
    }

    fn health(&self) -> Health {
        let mut h = self.inner.health();
        // the simulated database checks of the engine are replaced by real ones
        h.checks.retain(|c| !c.name.starts_with("Database integrity") && !c.name.starts_with("Row CRC32"));
        let s = self.stats();
        let integ = self.shared.integrity_ok.load(Relaxed) == 1;
        let ck = |name: &str, ok: bool, d: String| Check { name: name.into(), ok, detail: d };
        h.checks.insert(0, ck("Database integrity (SQLite quick_check)", integ, format!("{} - WAL mode", self.path.display())));
        h.checks.insert(1, ck("CRC: WAL frame checksums", integ, "checked by SQLite on every read".into()));
        h.checks.push(ck("Storage writes", s.failures == 0, format!("{} stored, {} failed, p95 {} us, max {} us", s.writes, s.failures, s.p95_us, s.max_us)));
        h.checks.push(ck("Write queue", s.pending < 100, format!("{} waiting for the database thread", s.pending)));
        h
    }
    fn blackout(&self) -> bool {
        self.inner.blackout()
    }

    fn log(&mut self, indicator: &str, value: f64) -> LogResult {
        let r = self.inner.log(indicator, value);
        self.send(Cmd::Log { crew: self.crew_id(), ind: indicator.to_string(), value: r.value });
        r
    }
    fn set_environment(&mut self, env_id: &str) {
        self.inner.select_environment(env_id);
        self.send(Cmd::Env(env_id.to_string()));
    }
    fn set_blackout(&mut self, on: bool) {
        self.inner.set_blackout(on);
        self.send(Cmd::Blackout(on));
    }
    fn run_selftest(&mut self) {
        self.inner.run_selftest();
        self.send(Cmd::Resync); // also verifies that the database answers
    }
    fn export(&mut self) -> Result<String, String> {
        self.inner.export()
    }

    fn crew(&self) -> Vec<CrewMember> {
        self.inner.crew()
    }
    fn selected_crew(&self) -> String {
        self.inner.selected_crew()
    }
    fn select_crew(&mut self, id: &str) {
        self.inner.select_crew(id);
    }
    fn alerts(&self) -> Vec<AlertRow> {
        self.inner.alerts()
    }
    fn ack_alert(&mut self, alert_id: u64) {
        let ind = self.inner.alerts().into_iter().find(|a| a.id == alert_id).map(|a| a.indicator);
        self.inner.ack_alert(alert_id);
        if let Some(ind) = ind {
            self.send(Cmd::Ack { crew: self.crew_id(), ind });
        }
    }
    fn send_message(&mut self, text: &str) {
        self.inner.send_message(text);
        let day = self.inner.overview().mission_day;
        self.send(Cmd::Msg { crew: self.crew_id(), text: text.to_string(), day });
    }
    fn messages(&self) -> Vec<Message> {
        self.inner.messages()
    }

    fn tick(&mut self) -> bool {
        let mut changed = self.inner.tick();
        while let Ok(e) = self.h.rx.try_recv() {
            match e {
                Evt::Snapshot(s) => {
                    self.inner.apply_snapshot(&s);
                    changed = true;
                }
                Evt::Failed(m) => {
                    self.inner.note(&format!("DB ERROR: {m}"));
                    self.notes.push(format!("Database: {m}"));
                    changed = true;
                }
            }
        }
        changed
    }
    fn take_messages(&mut self) -> Vec<String> {
        let mut v = self.inner.take_messages();
        v.append(&mut self.notes);
        v
    }

    fn demo_next_day(&mut self) {
        self.inner.demo_next_day();
        self.send(Cmd::SkipDay);
    }
    fn demo_solar_storm(&mut self) {
        let v = self.inner.environment().typical_dose * 14.0;
        self.inner.note("TEST: solar particle event simulated");
        self.log("dose", v);
    }
    fn demo_inject_fault(&mut self) -> String {
        self.inner.demo_inject_fault()
    }
    fn demo_reset(&mut self) {
        self.send(Cmd::Reset { seed: true });
    }
}
