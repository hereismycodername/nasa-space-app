use chrono::{DateTime, Utc};

#[derive(Clone, Copy)]
pub(crate) enum Aggregation {
    Mean,
    Last,
}

impl Aggregation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mean => "mean",
            Self::Last => "last",
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct MetricDefinition {
    pub metric: &'static str,
    pub unit: &'static str,
    pub aggregation: Aggregation,
}

pub(crate) const METRICS: [MetricDefinition; 6] = [
    MetricDefinition {
        metric: "heartRate",
        unit: "bpm",
        aggregation: Aggregation::Mean,
    },
    MetricDefinition {
        metric: "systolicPressure",
        unit: "mmHg",
        aggregation: Aggregation::Mean,
    },
    MetricDefinition {
        metric: "diastolicPressure",
        unit: "mmHg",
        aggregation: Aggregation::Mean,
    },
    MetricDefinition {
        metric: "temperature",
        unit: "°C",
        aggregation: Aggregation::Mean,
    },
    MetricDefinition {
        metric: "sleepHours",
        unit: "h",
        aggregation: Aggregation::Last,
    },
    MetricDefinition {
        metric: "mood",
        unit: "score",
        aggregation: Aggregation::Last,
    },
];

pub(crate) struct SeriesFilter {
    pub astronaut_id: &'static str,
    pub metric: MetricDefinition,
    pub from_ms: i64,
    pub to_ms: i64,
    pub bucket_seconds: i64,
}

/// Select the smallest UTC-epoch-aligned bucket covering [from_ms, to_ms).
pub(crate) fn series_bucket_seconds(
    from_ms: i64,
    to_ms: i64,
    max_points: usize,
) -> Result<i64, ValidationError> {
    if from_ms >= to_ms {
        return Err(ValidationError {
            field: "to",
            message: "to must be after from in UTC milliseconds",
        });
    }
    if !(1..=500).contains(&max_points) {
        return Err(ValidationError {
            field: "maxPoints",
            message: "maxPoints must be an integer from 1 to 500",
        });
    }
    [10, 30, 60, 300, 900, 1800, 3600, 10800, 21600, 43200, 86400]
        .into_iter()
        .find(|seconds| {
            let bucket_ms = seconds * 1000;
            let count = (to_ms - 1).div_euclid(bucket_ms) - from_ms.div_euclid(bucket_ms) + 1;
            count <= max_points as i64
        })
        .ok_or(ValidationError {
            field: "to",
            message: "Range requires more than maxPoints even with 1-day buckets; shorten the range or increase maxPoints",
        })
}

pub(crate) struct BucketAggregate {
    pub start_ms: i64,
    pub mean: f64,
    pub last: f64,
    pub min: f64,
    pub max: f64,
    pub count: i64,
}

pub(crate) struct SeriesData {
    pub buckets: Vec<BucketAggregate>,
    pub excluded_unknown_time_count: i64,
}

pub(crate) struct SeriesPoint {
    pub start_ms: i64,
    pub value: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub count: i64,
}

/// Storage supplies nonempty buckets in ascending order. Missing buckets stay explicit.
pub(crate) fn fill_series(
    filter: &SeriesFilter,
    buckets: Vec<BucketAggregate>,
) -> Vec<SeriesPoint> {
    let bucket_ms = filter.bucket_seconds * 1000;
    let mut start_ms = filter.from_ms.div_euclid(bucket_ms) * bucket_ms;
    let mut buckets = buckets.into_iter().peekable();
    let mut points = Vec::new();
    while start_ms < filter.to_ms {
        let bucket = if buckets
            .peek()
            .is_some_and(|bucket| bucket.start_ms == start_ms)
        {
            buckets.next()
        } else {
            None
        };
        points.push(match bucket {
            Some(bucket) => SeriesPoint {
                start_ms,
                value: Some(match filter.metric.aggregation {
                    Aggregation::Mean => bucket.mean,
                    Aggregation::Last => bucket.last,
                }),
                min: Some(bucket.min),
                max: Some(bucket.max),
                count: bucket.count,
            },
            None => SeriesPoint {
                start_ms,
                value: None,
                min: None,
                max: None,
                count: 0,
            },
        });
        start_ms += bucket_ms;
    }
    points
}

pub(crate) struct HistoryFilter {
    pub astronaut_id: String,
    pub metric: Option<String>,
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub limit: usize,
}

pub(crate) struct MeasurementPage {
    pub items: Vec<Measurement>,
    pub has_more: bool,
}

pub(crate) struct LatestMetric {
    pub metric: &'static str,
    pub unit: &'static str,
    pub latest_known_measurement: Option<Measurement>,
    pub latest_unknown_time_measurement: Option<Measurement>,
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub(crate) struct ValidationError {
    pub field: &'static str,
    pub message: &'static str,
}

#[derive(Debug)]
pub(crate) struct NewMeasurement {
    pub device_id: String,
    pub session_id: String,
    pub sequence: i64,
    pub metric: String,
    pub value: f64,
    pub unit: String,
    /// Source-provided measurement instant. Missing means unknown, not "now".
    pub measured_at: Option<String>,
}

impl NewMeasurement {
    /// Validate the v0.1 input bounds, not clinical health thresholds.
    pub fn validate(&self, now: DateTime<Utc>) -> Result<(), ValidationError> {
        let invalid_identity = if self.device_id.is_empty() || self.device_id.len() > 128 {
            Some("deviceId")
        } else if self.session_id.is_empty() || self.session_id.len() > 128 {
            Some("sessionId")
        } else if self.sequence < 0 {
            Some("sequence")
        } else {
            None
        };
        if let Some(field) = invalid_identity {
            return Err(ValidationError {
                field,
                message: "Invalid deviceId, sessionId or sequence",
            });
        }
        let expected = METRICS
            .iter()
            .find(|metric| metric.metric == self.metric)
            .ok_or(ValidationError {
                field: "metric",
                message: "Unknown metric",
            })?
            .unit;
        if self.unit != expected || !self.value.is_finite() {
            return Err(ValidationError {
                field: if self.unit != expected {
                    "unit"
                } else {
                    "value"
                },
                message: "Invalid unit or value",
            });
        }
        if let Some(time) = &self.measured_at {
            let parsed = DateTime::parse_from_rfc3339(time).map_err(|_| ValidationError {
                field: "measuredAt",
                message: "Invalid measuredAt timestamp",
            })?;
            if parsed > now + chrono::Duration::minutes(5) {
                return Err(ValidationError {
                    field: "measuredAt",
                    message: "measuredAt is in the future",
                });
            }
        }
        let valid = match self.metric.as_str() {
            "sleepHours" => (0.0..=24.0).contains(&self.value),
            "mood" => (1.0..=5.0).contains(&self.value),
            _ => self.value > 0.0,
        };
        if !valid {
            return Err(ValidationError {
                field: "value",
                message: "Value outside input bounds",
            });
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct Measurement {
    pub id: String,
    pub device_id: String,
    pub astronaut_id: String,
    pub session_id: String,
    pub sequence: i64,
    pub metric: String,
    pub value: f64,
    pub unit: String,
    /// Original RFC3339 text; numeric UTC columns in storage provide ordering.
    pub measured_at: Option<String>,
    /// Time of first successful receipt, unchanged by a duplicate message.
    pub received_at: String,
    pub source: String,
}

impl Measurement {
    /// Compare instants at Chrono's nanosecond precision, keeping the original text.
    ///
    /// Input must already be validated. A parse error means stored data could not
    /// be compared; it must not be reported as a client conflict or a successful retry.
    pub fn matches_payload(&self, input: &NewMeasurement) -> Result<bool, chrono::ParseError> {
        if self.metric != input.metric || self.value != input.value || self.unit != input.unit {
            return Ok(false);
        }
        match (&self.measured_at, &input.measured_at) {
            (None, None) => Ok(true),
            (Some(stored), Some(incoming)) => {
                let stored = DateTime::parse_from_rfc3339(stored)?.with_timezone(&Utc);
                let incoming = DateTime::parse_from_rfc3339(incoming)?.with_timezone(&Utc);
                Ok(stored == incoming)
            }
            _ => Ok(false),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Device {
    pub astronaut_id: String,
    pub source: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn series_bucket_selection_accounts_for_alignment_and_range_limit() {
        let week = 7 * 86400 * 1000;
        assert_eq!(series_bucket_seconds(0, week, 200).unwrap(), 3600);
        assert_eq!(series_bucket_seconds(0, 20_000, 2).unwrap(), 10);
        assert_eq!(series_bucket_seconds(1, 20_001, 2).unwrap(), 30);
        assert_eq!(series_bucket_seconds(-10_001, -1, 1).unwrap(), 30);
        assert_eq!(
            series_bucket_seconds(0, 501 * 86400 * 1000, 500)
                .unwrap_err()
                .field,
            "to"
        );
    }

    fn input() -> NewMeasurement {
        NewMeasurement {
            device_id: "simulator-01".into(),
            session_id: "domain-test".into(),
            sequence: 0,
            metric: "heartRate".into(),
            value: 72.0,
            unit: "bpm".into(),
            measured_at: None,
        }
    }

    fn stored(time: &str) -> Measurement {
        Measurement {
            id: "original-id".into(),
            device_id: "simulator-01".into(),
            astronaut_id: "demo-astronaut".into(),
            session_id: "domain-test".into(),
            sequence: 0,
            metric: "heartRate".into(),
            value: 72.0,
            unit: "bpm".into(),
            measured_at: Some(time.into()),
            received_at: "2026-11-14T00:00:00Z".into(),
            source: "simulator".into(),
        }
    }

    #[test]
    fn accepts_exactly_five_minutes_ahead_but_rejects_one_nanosecond_more() {
        let now = DateTime::parse_from_rfc3339("2026-11-14T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let boundary = now + chrono::Duration::minutes(5);
        let mut input = input();
        input.measured_at = Some(boundary.to_rfc3339());
        assert!(input.validate(now).is_ok());
        input.measured_at = Some((boundary + chrono::Duration::nanoseconds(1)).to_rfc3339());
        let error = input.validate(now).unwrap_err();
        assert_eq!(error.field, "measuredAt");
        assert_eq!(error.message, "measuredAt is in the future");
    }

    #[test]
    fn rejects_non_finite_values_before_storage() {
        let now = DateTime::parse_from_rfc3339("2026-11-14T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut input = input();
            input.value = value;
            assert_eq!(input.validate(now).unwrap_err().field, "value");
        }
    }

    #[test]
    fn corrupt_stored_time_is_an_error_not_a_successful_retry_or_conflict() {
        let mut input = input();
        input.measured_at = Some("2020-01-01T00:00:00Z".into());
        assert!(stored("corrupt").matches_payload(&input).is_err());
    }

    #[test]
    fn leap_second_is_distinct_from_next_second_despite_equal_unix_milliseconds() {
        let mut input = input();
        input.measured_at = Some("2017-01-01T00:00:00Z".into());
        assert!(!stored("2016-12-31T23:59:60Z")
            .matches_payload(&input)
            .unwrap());
        input.measured_at = Some("2017-01-01T04:59:60+05:00".into());
        assert!(stored("2016-12-31T23:59:60Z")
            .matches_payload(&input)
            .unwrap());
    }

    #[test]
    fn timestamp_comparison_uses_first_nine_fraction_digits() {
        let mut input = input();
        input.measured_at = Some("2020-01-01T05:00:00.123456789999+05:00".into());
        assert!(stored("2020-01-01T00:00:00.123456789Z")
            .matches_payload(&input)
            .unwrap());
    }
}
