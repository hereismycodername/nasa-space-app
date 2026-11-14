use chrono::{DateTime, Utc};

#[derive(Debug)]
pub(crate) struct Task {
    pub id: String,
    pub title: String,
    pub completed_at: Option<String>,
}

/// The legacy endpoint still replaces completion time; dated HTTP semantics are BE-07.
pub(crate) fn completion_time(completed: bool, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    completed.then_some(now)
}
