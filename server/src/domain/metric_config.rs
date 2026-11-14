//! Shared metric definitions and demonstration policies, not clinical thresholds.

pub(crate) const MAX_FUTURE_SKEW_SECONDS: i64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum InputBounds {
    Positive,
    Inclusive(f64, f64),
}

impl InputBounds {
    pub fn contains(self, value: f64) -> bool {
        value.is_finite()
            && match self {
                Self::Positive => value > 0.0,
                Self::Inclusive(min, max) => (min..=max).contains(&value),
            }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MetricDefinition {
    pub metric: &'static str,
    pub unit: &'static str,
    pub bounds: InputBounds,
    pub aggregation: Aggregation,
    pub fresh_for_seconds: u32,
}

pub(crate) const METRICS: &[MetricDefinition] = &[
    MetricDefinition {
        metric: "heartRate",
        unit: "bpm",
        bounds: InputBounds::Positive,
        aggregation: Aggregation::Mean,
        fresh_for_seconds: 15,
    },
    MetricDefinition {
        metric: "systolicPressure",
        unit: "mmHg",
        bounds: InputBounds::Positive,
        aggregation: Aggregation::Mean,
        fresh_for_seconds: 3600,
    },
    MetricDefinition {
        metric: "diastolicPressure",
        unit: "mmHg",
        bounds: InputBounds::Positive,
        aggregation: Aggregation::Mean,
        fresh_for_seconds: 3600,
    },
    MetricDefinition {
        metric: "temperature",
        unit: "°C",
        bounds: InputBounds::Positive,
        aggregation: Aggregation::Mean,
        fresh_for_seconds: 3600,
    },
    MetricDefinition {
        metric: "sleepHours",
        unit: "h",
        bounds: InputBounds::Inclusive(0.0, 24.0),
        aggregation: Aggregation::Last,
        fresh_for_seconds: 86_400,
    },
    MetricDefinition {
        metric: "mood",
        unit: "score",
        bounds: InputBounds::Inclusive(1.0, 5.0),
        aggregation: Aggregation::Last,
        fresh_for_seconds: 86_400,
    },
    MetricDefinition {
        metric: "spo2",
        unit: "%",
        bounds: InputBounds::Inclusive(0.0, 100.0),
        aggregation: Aggregation::Mean,
        fresh_for_seconds: 60,
    },
    MetricDefinition {
        metric: "glucose",
        unit: "mg/dL",
        bounds: InputBounds::Positive,
        aggregation: Aggregation::Mean,
        fresh_for_seconds: 3600,
    },
    MetricDefinition {
        metric: "visionBlur",
        unit: "score",
        bounds: InputBounds::Inclusive(0.0, 10.0),
        aggregation: Aggregation::Last,
        fresh_for_seconds: 86400,
    },
    MetricDefinition {
        metric: "exerciseMinutes",
        unit: "min",
        bounds: InputBounds::Inclusive(0.0, 1440.0),
        aggregation: Aggregation::Last,
        fresh_for_seconds: 86400,
    },
    MetricDefinition {
        metric: "headache",
        unit: "score",
        bounds: InputBounds::Inclusive(0.0, 10.0),
        aggregation: Aggregation::Last,
        fresh_for_seconds: 3600,
    },
    MetricDefinition {
        metric: "backPain",
        unit: "score",
        bounds: InputBounds::Inclusive(0.0, 10.0),
        aggregation: Aggregation::Last,
        fresh_for_seconds: 3600,
    },
];

pub(crate) fn definition_for(metric: &str) -> Option<&'static MetricDefinition> {
    METRICS.iter().find(|policy| policy.metric == metric)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_supported_metric_has_one_explicit_freshness_policy() {
        assert_eq!(
            METRICS
                .iter()
                .map(|policy| policy.metric)
                .collect::<Vec<_>>(),
            [
                "heartRate",
                "systolicPressure",
                "diastolicPressure",
                "temperature",
                "sleepHours",
                "mood",
                "spo2",
                "glucose",
                "visionBlur",
                "exerciseMinutes",
                "headache",
                "backPain"
            ]
        );
        assert_eq!(
            METRICS
                .iter()
                .map(|policy| policy.fresh_for_seconds)
                .collect::<Vec<_>>(),
            [15, 3600, 3600, 3600, 86_400, 86_400, 60, 3600, 86_400, 86_400, 3600, 3600]
        );
        assert_eq!(definition_for("not-a-metric"), None);
    }
}
