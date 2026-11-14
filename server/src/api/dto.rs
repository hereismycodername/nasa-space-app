//! HTTP serialization stays separate from domain rules and SQLite rows.
use crate::domain::{
    assessments::Assessment,
    measurements::{Measurement, NewMeasurement},
    tasks::Task,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NewMeasurementRequest {
    device_id: String,
    session_id: String,
    sequence: i64,
    metric: String,
    value: f64,
    unit: String,
    measured_at: Option<String>,
}
impl From<NewMeasurementRequest> for NewMeasurement {
    fn from(input: NewMeasurementRequest) -> Self {
        Self {
            device_id: input.device_id,
            session_id: input.session_id,
            sequence: input.sequence,
            metric: input.metric,
            value: input.value,
            unit: input.unit,
            measured_at: input.measured_at,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MeasurementResponse {
    id: String,
    device_id: String,
    astronaut_id: String,
    session_id: String,
    sequence: i64,
    metric: String,
    value: f64,
    unit: String,
    measured_at: Option<String>,
    received_at: String,
    time_basis: &'static str,
    source: String,
}
impl From<Measurement> for MeasurementResponse {
    fn from(row: Measurement) -> Self {
        let time_basis = if row.measured_at.is_some() {
            "measuredAt"
        } else {
            "receivedAt"
        };
        Self {
            id: row.id,
            device_id: row.device_id,
            astronaut_id: row.astronaut_id,
            session_id: row.session_id,
            sequence: row.sequence,
            metric: row.metric,
            value: row.value,
            unit: row.unit,
            measured_at: row.measured_at,
            received_at: row.received_at,
            time_basis,
            source: row.source,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct AssessmentResponse {
    status: &'static str,
    explanation: &'static str,
}
impl From<Assessment> for AssessmentResponse {
    fn from(value: Assessment) -> Self {
        Self {
            status: value.status,
            explanation: value.explanation,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskResponse {
    id: String,
    title: String,
    completed_at: Option<String>,
}
impl From<Task> for TaskResponse {
    fn from(value: Task) -> Self {
        Self {
            id: value.id,
            title: value.title,
            completed_at: value.completed_at,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskUpdateRequest {
    pub completed: bool,
}

#[derive(Serialize)]
pub(crate) struct HealthResponse {
    pub status: &'static str,
    pub storage: &'static str,
}
