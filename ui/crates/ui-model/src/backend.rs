//! THE CONTRACT. Implement this trait on the real Rust backend and both interfaces
//! (terminal and window) work unchanged. All methods are synchronous and cheap; the
//! interfaces call `tick()` about once per second and re-read what they need.

use crate::types::*;

pub trait Backend {
    // ---- read side -----------------------------------------------------------------
    fn overview(&self) -> Overview;
    fn systems(&self) -> Vec<SystemInfo>;
    fn indicators(&self, system: &str) -> Vec<IndicatorInfo>;
    fn system_status(&self, system: &str) -> Status;
    fn indicator_status(&self, indicator: &str) -> Status;
    fn latest(&self, indicator: &str) -> Option<Reading>;
    /// oldest first
    fn history(&self, indicator: &str) -> Vec<Reading>;
    fn thresholds(&self, indicator: &str) -> Thresholds;
    /// Status + advice for any value, including a value the user has typed but not logged yet.
    fn evaluate(&self, indicator: &str, value: f64) -> Evaluation;
    fn is_due(&self, indicator: &str) -> bool;
    fn environments(&self) -> Vec<EnvInfo>;
    fn environment(&self) -> EnvInfo;
    fn hazard_label(&self, key: &str) -> String;
    fn queue(&self, max_rows: usize) -> (QueueSummary, Vec<QueueRow>);
    fn health(&self) -> Health;
    fn blackout(&self) -> bool;

    // ---- write side ----------------------------------------------------------------
    fn log(&mut self, indicator: &str, value: f64) -> LogResult;
    fn set_environment(&mut self, env_id: &str);
    fn set_blackout(&mut self, on: bool);
    fn run_selftest(&mut self);
    /// Writes the downlink bundle and returns where it was written.
    fn export(&mut self) -> Result<String, String>;

    // ---- ground / multi-crew (defaults let a single-astronaut backend work unchanged) ----
    /// Roster. Empty = single-crew backend: the ground view shows one anonymous crew member.
    fn crew(&self) -> Vec<CrewMember> {
        vec![]
    }
    fn selected_crew(&self) -> String {
        String::new()
    }
    /// After this call every read method above answers for that crew member.
    fn select_crew(&mut self, _id: &str) {}
    /// Open findings for the selected crew member, most severe first.
    fn alerts(&self) -> Vec<AlertRow> {
        vec![]
    }
    fn ack_alert(&mut self, _alert_id: u64) {}
    /// Ground -> crew message (uplink). Delivery time is the backend's business.
    fn send_message(&mut self, _text: &str) {}
    fn messages(&self) -> Vec<Message> {
        vec![]
    }

    // ---- housekeeping --------------------------------------------------------------
    /// Call about once per second. Returns true when anything visible may have changed.
    fn tick(&mut self) -> bool;
    /// One-shot notifications ("Data corruption repaired", "Downlink bundle delivered", ...).
    fn take_messages(&mut self) -> Vec<String>;

    // ---- demo only (default: not supported) ---------------------------------------
    fn demo_next_day(&mut self) {}
    fn demo_solar_storm(&mut self) {}
    fn demo_inject_fault(&mut self) -> String {
        "not supported".into()
    }
    fn demo_reset(&mut self) {}
}
