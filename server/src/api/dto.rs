//! HTTP serialization stays separate from domain rules and SQLite rows.
use crate::domain::{
    assessments::Assessment,
    measurements::{
        HistoryFilter, LatestMetric, Measurement, MeasurementPage, NewMeasurement, ValidationError,
        METRICS,
    },
    tasks::Task,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryQuery {
    metric: Option<String>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<String>,
}

impl TryFrom<HistoryQuery> for HistoryFilter {
    type Error = ValidationError;

    fn try_from(query: HistoryQuery) -> Result<Self, Self::Error> {
        if query
            .metric
            .as_ref()
            .is_some_and(|value| !METRICS.iter().any(|metric| metric.metric == value))
        {
            return Err(ValidationError {
                field: "metric",
                message: "Unknown metric",
            });
        }
        let timestamp = |value: Option<String>, field| -> Result<Option<i64>, ValidationError> {
            value
                .map(|value| {
                    chrono::DateTime::parse_from_rfc3339(&value)
                        .map(|time| time.timestamp_millis())
                        .map_err(|_| ValidationError {
                            field,
                            message: "Expected an RFC3339 timestamp with offset",
                        })
                })
                .transpose()
        };
        let from_ms = timestamp(query.from, "from")?;
        let to_ms = timestamp(query.to, "to")?;
        if matches!((from_ms, to_ms), (Some(from), Some(to)) if from >= to) {
            return Err(ValidationError {
                field: "to",
                message: "to must be after from",
            });
        }
        let limit = query
            .limit
            .map(|value| {
                value
                    .parse::<usize>()
                    .ok()
                    .filter(|limit| (1..=500).contains(limit))
                    .ok_or(ValidationError {
                        field: "limit",
                        message: "limit must be an integer from 1 to 500",
                    })
            })
            .transpose()?
            .unwrap_or(100);
        Ok(Self {
            astronaut_id: "demo-astronaut".into(),
            metric: query.metric,
            from_ms,
            to_ms,
            limit,
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MeasurementPageResponse {
    items: Vec<MeasurementResponse>,
    has_more: bool,
}
impl From<MeasurementPage> for MeasurementPageResponse {
    fn from(page: MeasurementPage) -> Self {
        Self {
            items: page.items.into_iter().map(Into::into).collect(),
            has_more: page.has_more,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct LatestMeasurementsResponse {
    pub metrics: Vec<LatestMetricResponse>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LatestMetricResponse {
    metric: &'static str,
    unit: &'static str,
    latest_known_measurement: Option<MeasurementResponse>,
    latest_unknown_time_measurement: Option<MeasurementResponse>,
}
impl From<LatestMetric> for LatestMetricResponse {
    fn from(metric: LatestMetric) -> Self {
        Self {
            metric: metric.metric,
            unit: metric.unit,
            latest_known_measurement: metric.latest_known_measurement.map(Into::into),
            latest_unknown_time_measurement: metric.latest_unknown_time_measurement.map(Into::into),
        }
    }
}

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
