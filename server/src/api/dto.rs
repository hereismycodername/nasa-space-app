//! HTTP serialization stays separate from domain rules and SQLite rows.
use crate::domain::{
    assessments::{self, InvalidStoredTimestamp},
    freshness::{self, MetricQuality, Quality},
    health_rules::{self, FlightEducation, HealthEvaluation, HealthRule},
    history::{HistoryCursor, HistoryPage, HistoryQuery, LatestMetric},
    measurements::{Measurement, NewMeasurement, ValidationError},
    series::{self, BucketAggregate, InvalidSeriesData, SeriesPoint, SeriesQuery},
    tasks::Task,
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SeriesRequest {
    metric: Option<String>,
    from: Option<String>,
    to: Option<String>,
    max_points: Option<String>,
}

impl TryFrom<SeriesRequest> for SeriesQuery {
    type Error = ValidationError;
    fn try_from(input: SeriesRequest) -> Result<Self, Self::Error> {
        Self::parse(
            input.metric,
            input.from.as_deref(),
            input.to.as_deref(),
            input.max_points.as_deref(),
        )
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SeriesResponse {
    astronaut_id: &'static str,
    metric: String,
    unit: &'static str,
    aggregation: &'static str,
    from: String,
    to: String,
    interval_ms: i64,
    time_basis: Option<&'static str>,
    points: Vec<SeriesPointResponse>,
}

impl SeriesResponse {
    pub fn from_data(
        astronaut_id: &'static str,
        query: SeriesQuery,
        buckets: Vec<BucketAggregate>,
    ) -> Result<Self, InvalidSeriesData> {
        let points = series::fill_gaps(&query, buckets)?;
        let time_basis = series::time_basis(
            points.iter().any(|p| p.measured_count > 0),
            points.iter().any(|p| p.received_count > 0),
        );
        Ok(Self {
            astronaut_id,
            metric: query.metric,
            unit: query.unit,
            aggregation: query.aggregation.as_str(),
            from: utc_timestamp(query.from_ms)?,
            to: utc_timestamp(query.to_ms)?,
            interval_ms: query.interval_ms,
            time_basis,
            points: points
                .into_iter()
                .map(SeriesPointResponse::try_from)
                .collect::<Result<_, _>>()?,
        })
    }
}

fn utc_timestamp(milliseconds: i64) -> Result<String, InvalidSeriesData> {
    DateTime::from_timestamp_millis(milliseconds)
        .map(|time| time.to_rfc3339_opts(SecondsFormat::Millis, true))
        .ok_or(InvalidSeriesData)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SeriesPointResponse {
    start_at: String,
    end_at: String,
    value: Option<f64>,
    min: Option<f64>,
    max: Option<f64>,
    count: i64,
    measured_count: i64,
    received_count: i64,
    time_basis: Option<&'static str>,
}

impl TryFrom<SeriesPoint> for SeriesPointResponse {
    type Error = InvalidSeriesData;
    fn try_from(point: SeriesPoint) -> Result<Self, Self::Error> {
        Ok(Self {
            start_at: utc_timestamp(point.start_ms)?,
            end_at: utc_timestamp(point.end_ms)?,
            value: point.value,
            min: point.min,
            max: point.max,
            count: point.count,
            measured_count: point.measured_count,
            received_count: point.received_count,
            time_basis: series::time_basis(point.measured_count > 0, point.received_count > 0),
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryRequest {
    metric: Option<String>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<String>,
    cursor: Option<String>,
}

impl TryFrom<HistoryRequest> for HistoryQuery {
    type Error = ValidationError;

    fn try_from(input: HistoryRequest) -> Result<Self, Self::Error> {
        let cursor = input.cursor.as_deref().map(parse_cursor).transpose()?;
        Self::parse(
            input.metric,
            input.from.as_deref(),
            input.to.as_deref(),
            input.limit.as_deref(),
            cursor,
        )
    }
}

fn parse_cursor(value: &str) -> Result<HistoryCursor, ValidationError> {
    let invalid = || ValidationError {
        field: "cursor",
        message: "Invalid history cursor",
    };
    if value.len() > 160 {
        return Err(invalid());
    }
    let mut parts = value.splitn(3, '.');
    if parts.next() != Some("v1") {
        return Err(invalid());
    }
    let time_ms = parts
        .next()
        .ok_or_else(invalid)?
        .parse::<i64>()
        .map_err(|_| invalid())?;
    let id = parts
        .next()
        .filter(|id| !id.is_empty() && id.len() <= 128)
        .ok_or_else(invalid)?;
    Ok(HistoryCursor {
        time_ms,
        id: id.to_owned(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HistoryResponse {
    items: Vec<MeasurementResponse>,
    has_more: bool,
    next_cursor: Option<String>,
}

impl From<HistoryPage> for HistoryResponse {
    fn from(page: HistoryPage) -> Self {
        Self {
            items: page.items.into_iter().map(Into::into).collect(),
            has_more: page.has_more,
            next_cursor: page
                .next_cursor
                .map(|cursor| format!("v1.{}.{}", cursor.time_ms, cursor.id)),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LatestRequest {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LatestResponse {
    astronaut_id: &'static str,
    evaluated_at: String,
    metrics: Vec<LatestMetricResponse>,
}

impl LatestResponse {
    pub fn from_data(
        astronaut_id: &'static str,
        metrics: Vec<LatestMetric>,
        now: DateTime<Utc>,
    ) -> Result<Self, InvalidStoredTimestamp> {
        let qualities = freshness::evaluate_metrics(&metrics, now)?;
        Ok(Self {
            astronaut_id,
            evaluated_at: now.to_rfc3339_opts(SecondsFormat::AutoSi, true),
            metrics: metrics.into_iter().zip(qualities).map(Into::into).collect(),
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LatestMetricResponse {
    metric: &'static str,
    latest_measurement: Option<MeasurementResponse>,
    latest_known_measurement: Option<MeasurementResponse>,
    latest_received_measurement: Option<MeasurementResponse>,
    quality: QualityResponse,
    latest_received_quality: QualityResponse,
}

impl From<(LatestMetric, MetricQuality)> for LatestMetricResponse {
    fn from((value, quality): (LatestMetric, MetricQuality)) -> Self {
        Self {
            metric: value.metric,
            latest_measurement: value.latest_measurement.map(Into::into),
            latest_known_measurement: value.latest_known_measurement.map(Into::into),
            latest_received_measurement: value.latest_received_measurement.map(Into::into),
            quality: quality.quality.into(),
            latest_received_quality: quality.latest_received_quality.into(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QualityResponse {
    status: &'static str,
    measurement_id: Option<String>,
    fresh_for_seconds: u32,
    age_seconds: Option<f64>,
    clock_skew: bool,
}

impl From<Quality> for QualityResponse {
    fn from(quality: Quality) -> Self {
        Self {
            status: quality.status.as_str(),
            measurement_id: quality.measurement_id,
            fresh_for_seconds: quality.fresh_for_seconds,
            age_seconds: quality.age_seconds,
            clock_skew: quality.clock_skew,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MetricQualityResponse {
    metric: &'static str,
    quality: QualityResponse,
    latest_received_quality: QualityResponse,
}

impl From<MetricQuality> for MetricQualityResponse {
    fn from(metric: MetricQuality) -> Self {
        Self {
            metric: metric.metric,
            quality: metric.quality.into(),
            latest_received_quality: metric.latest_received_quality.into(),
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
#[serde(rename_all = "camelCase")]
pub(crate) struct AssessmentResponse {
    status: &'static str,
    explanation: &'static str,
    astronaut_id: &'static str,
    evaluated_at: String,
    metrics: Vec<MetricQualityResponse>,
    health: HealthEvaluationResponse,
}
impl AssessmentResponse {
    pub fn from_data(
        astronaut_id: &'static str,
        metrics: &[LatestMetric],
        now: DateTime<Utc>,
    ) -> Result<Self, InvalidStoredTimestamp> {
        let summary = assessments::assess_snapshot(metrics, now)?;
        let qualities = freshness::evaluate_metrics(metrics, now)?;
        let health = health_rules::evaluate(metrics, &qualities)?;
        Ok(Self {
            status: summary.status,
            explanation: summary.explanation,
            astronaut_id,
            evaluated_at: now.to_rfc3339_opts(SecondsFormat::AutoSi, true),
            metrics: qualities.into_iter().map(Into::into).collect(),
            health: health.into(),
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthEvaluationResponse {
    rule_set_version: &'static str,
    demonstration_only: bool,
    status: &'static str,
    explanation: String,
    disclaimer: &'static str,
    emergency_guidance: &'static str,
    evaluated_metric_count: usize,
    total_metric_count: usize,
    rules: Vec<HealthRuleResponse>,
    education: Vec<FlightEducationResponse>,
}

impl From<HealthEvaluation> for HealthEvaluationResponse {
    fn from(value: HealthEvaluation) -> Self {
        Self {
            rule_set_version: health_rules::RULE_SET_VERSION,
            demonstration_only: true,
            status: value.status.as_str(),
            explanation: value.explanation,
            disclaimer: value.disclaimer,
            emergency_guidance: value.emergency_guidance,
            evaluated_metric_count: value.evaluated_metric_count,
            total_metric_count: value.total_metric_count,
            rules: value.rules.into_iter().map(Into::into).collect(),
            education: value.education.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthRuleResponse {
    rule_id: String,
    rule_version: &'static str,
    metric: &'static str,
    title: &'static str,
    status: &'static str,
    explanation: String,
    measurement_ids: Vec<String>,
    valid_until: Option<String>,
    suggested_action: &'static str,
    escalation: &'static str,
    flight_context: &'static str,
    source_urls: &'static [&'static str],
    demonstration_only: bool,
}

impl From<HealthRule> for HealthRuleResponse {
    fn from(value: HealthRule) -> Self {
        Self {
            rule_id: value.rule_id,
            rule_version: health_rules::RULE_SET_VERSION,
            metric: value.metric,
            title: value.title,
            status: value.status.as_str(),
            explanation: value.explanation,
            measurement_ids: value.measurement_ids,
            valid_until: value.valid_until,
            suggested_action: value.suggested_action,
            escalation: value.escalation,
            flight_context: value.flight_context,
            source_urls: value.source_urls,
            demonstration_only: true,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FlightEducationResponse {
    id: &'static str,
    title: &'static str,
    explanation: &'static str,
    suggested_action: &'static str,
    source_urls: &'static [&'static str],
}

impl From<FlightEducation> for FlightEducationResponse {
    fn from(value: FlightEducation) -> Self {
        Self {
            id: value.id,
            title: value.title,
            explanation: value.explanation,
            suggested_action: value.suggested_action,
            source_urls: value.source_urls,
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
