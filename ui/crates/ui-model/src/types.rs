//! Plain data types exchanged between a backend and the interfaces.
//! Everything is owned data (no lifetimes, no trait objects) so any backend can fill it easily.

use std::collections::HashMap;

/// Health status of one indicator or one body system. Ordered: `Act` is the worst.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Status {
    NoData,
    Normal,
    Watch,
    Act,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::NoData => "No data",
            Status::Normal => "Normal",
            Status::Watch => "Watch",
            Status::Act => "Act now",
        }
    }
    pub fn from_level(l: u8) -> Status {
        match l {
            0 => Status::Normal,
            1 => Status::Watch,
            _ => Status::Act,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// free numeric value (bpm, mg/dL, ...)
    Num,
    /// small integer scale (0-10)
    Scale,
}

#[derive(Clone, Debug)]
pub struct SystemInfo {
    pub id: String,
    pub name: String,
    pub function: String,
    pub hazards: Vec<String>,
    pub effect: String,
    pub why: String,
}

#[derive(Clone, Debug)]
pub struct IndicatorInfo {
    pub id: String,
    pub system: String,
    pub name: String,
    pub unit: String,
    pub kind: Kind,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    /// days between measurements
    pub freq_days: u32,
    pub why: String,
    pub how: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct Reading {
    pub day: u32,
    pub value: f64,
}

/// Normal range and watch range; outside the watch range the status is `Act`.
#[derive(Clone, Copy, Debug)]
pub struct Thresholds {
    pub ok: (f64, f64),
    pub warn: (f64, f64),
}

#[derive(Clone, Debug)]
pub struct EnvInfo {
    pub id: String,
    pub name: String,
    pub gravity: String,
    pub comms: String,
    /// round-trip light time in seconds
    pub rtt_s: f64,
    /// typical radiation dose rate, mSv/day
    pub typical_dose: f64,
    pub escalation: String,
    /// system id -> local hazard note
    pub notes: HashMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct Advice {
    pub headline: String,
    pub actions: Vec<String>,
    /// set when the crew must contact the flight surgeon / follow the onboard protocol
    pub escalate: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Evaluation {
    pub status: Status,
    pub target: String,
    pub advice: Advice,
}

#[derive(Clone, Debug)]
pub struct LogResult {
    pub status: Status,
    pub value: f64,
    pub xp_gained: u32,
    pub bonus: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Priority {
    Routine = 0,
    Watch = 1,
    Urgent = 2,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum QState {
    Queued,
    InFlight,
    Acked,
}

#[derive(Clone, Debug)]
pub struct QueueRow {
    pub id: u64,
    pub label: String,
    pub priority: Priority,
    pub state: QState,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct QueueSummary {
    pub queued: usize,
    pub in_flight: usize,
    pub acked: usize,
}

#[derive(Clone, Debug)]
pub struct Overview {
    pub mission_day: u32,
    pub env_id: String,
    pub env_name: String,
    pub score: Option<u8>,
    pub xp: u32,
    pub level: u32,
    pub streak: u32,
    pub link_up: bool,
    /// no usable link to ground (blackout or very long light delay): onboard protocol is primary
    pub autonomous: bool,
    pub rtt_s: f64,
    pub due_count: usize,
    pub total_indicators: usize,
}

#[derive(Clone, Debug)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Clone, Debug)]
pub struct Health {
    pub checks: Vec<Check>,
    /// newest first
    pub events: Vec<String>,
    pub platform: String,
}

/// Tie-break for findings of equal severity: life-critical signals first (lower = more urgent).
pub fn clinical_rank(indicator_id: &str) -> u8 {
    const ORDER: [&str; 16] = ["spo2", "hr", "sbp", "temp", "glucose", "dose", "rr", "pvt", "headache", "sleep", "stress", "vision", "urine", "mass", "exercise", "back"];
    ORDER.iter().position(|x| *x == indicator_id).map(|p| p as u8).unwrap_or(99)
}

/// Who is looking at the screen. Same data, two workflows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    /// the astronaut: self-assessment, plain-language guidance, daily routine
    Crew,
    /// flight surgeon / mission support on the ground: triage, trends, uplink messages
    Ground,
}

/// One crew member as seen in the ground roster.
#[derive(Clone, Debug)]
pub struct CrewMember {
    pub id: String,
    pub name: String,
    pub title: String,
    /// worst status over all indicators
    pub status: Status,
    pub score: Option<u8>,
    /// seconds since the last data from this crew member reached the viewer
    pub last_contact_s: u64,
    pub open_alerts: usize,
}

/// One open finding for the selected crew member (an indicator outside the normal range).
#[derive(Clone, Debug)]
pub struct AlertRow {
    pub id: u64,
    pub indicator: String,
    pub indicator_name: String,
    pub system: String,
    pub value: f64,
    pub unit: String,
    pub status: Status,
    pub day: u32,
    pub acked: bool,
    pub headline: String,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub from_ground: bool,
    pub text: String,
    pub day: u32,
}

#[derive(Clone, Debug)]
pub struct Badge {
    pub name: String,
    pub unlocked: bool,
}
