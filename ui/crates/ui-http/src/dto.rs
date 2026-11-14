use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ui_model::{TaskInfo, TimePoint, TimedSeries};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Measurement {
    pub id: String,
    pub device_id: String,
    pub session_id: String,
    pub sequence: i64,
    pub metric: String,
    pub value: f64,
    pub unit: String,
    pub measured_at: Option<String>,
    pub received_at: String,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Quality {
    pub status: String,
    pub fresh_for_seconds: u32,
    pub age_seconds: Option<f64>,
    pub clock_skew: bool,
}

impl Quality {
    pub fn label(&self, elapsed: f64) -> &str {
        if self.status == "fresh"
            && self
                .age_seconds
                .is_some_and(|age| age + elapsed > f64::from(self.fresh_for_seconds))
        {
            "stale"
        } else {
            &self.status
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LatestMetric {
    pub metric: String,
    pub latest_known_measurement: Option<Measurement>,
    pub latest_received_measurement: Option<Measurement>,
    pub quality: Quality,
    pub latest_received_quality: Quality,
}

impl LatestMetric {
    pub fn selected(&self) -> Option<&Measurement> {
        self.latest_known_measurement
            .as_ref()
            .or(self.latest_received_measurement.as_ref())
    }

    pub fn selected_quality(&self) -> &Quality {
        if self.latest_known_measurement.is_some() {
            &self.quality
        } else {
            &self.latest_received_quality
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Latest {
    pub astronaut_id: String,
    pub evaluated_at: String,
    pub metrics: Vec<LatestMetric>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Task {
    pub id: String,
    pub title: String,
    pub completed_at: Option<String>,
}

impl From<&Task> for TaskInfo {
    fn from(value: &Task) -> Self {
        Self {
            id: value.id.clone(),
            title: value.title.clone(),
            completed_at: value.completed_at.clone(),
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct HealthResponse {
    pub status: String,
    pub storage: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NewMeasurement {
    pub device_id: &'static str,
    pub session_id: String,
    pub sequence: i64,
    pub metric: &'static str,
    pub value: f64,
    pub unit: &'static str,
    pub measured_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Series {
    pub metric: String,
    pub unit: String,
    pub aggregation: String,
    pub from: String,
    pub to: String,
    pub time_basis: Option<String>,
    pub points: Vec<SeriesPoint>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SeriesPoint {
    pub start_at: String,
    pub value: Option<f64>,
    pub received_count: i64,
}

pub(crate) fn timestamp_ms(value: &str) -> Result<i64, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|v| v.timestamp_millis())
        .map_err(|_| "Server returned an invalid timestamp".into())
}

impl Series {
    pub fn into_timed(self) -> Result<TimedSeries, String> {
        let received: i64 = self.points.iter().map(|p| p.received_count).sum();
        let points = self
            .points
            .into_iter()
            .map(|p| {
                if p.value.is_some_and(|v| !v.is_finite()) {
                    return Err("Server returned a non-finite series value".into());
                }
                Ok(TimePoint {
                    at_ms: timestamp_ms(&p.start_at)?,
                    value: p.value,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(TimedSeries {
            from_ms: timestamp_ms(&self.from)?, to_ms: timestamp_ms(&self.to)?, points,
            note: format!("Last 7 days UTC; {} per interval; time basis: {}; {} samples use receipt time because measurement time is unknown. Gaps mean no samples.",
                self.aggregation, self.time_basis.as_deref().unwrap_or("no samples"), received),
        })
    }
}

pub(crate) fn now_text() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Assessment {
    pub astronaut_id: String,
    pub evaluated_at: String,
    #[serde(default)]
    pub health: Option<ui_model::HealthEvaluation>,
}
