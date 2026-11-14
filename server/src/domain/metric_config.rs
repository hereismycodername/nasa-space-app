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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MetricDefinition {
    pub metric: &'static str,
    pub unit: &'static str,
    pub aggregation: Aggregation,
    pub fresh_for_seconds: u32,
}

pub(crate) const METRICS: [MetricDefinition; 6] = [
    MetricDefinition {
        metric: "heartRate",
        unit: "bpm",
        aggregation: Aggregation::Mean,
        fresh_for_seconds: 15,
    },
    MetricDefinition {
        metric: "systolicPressure",
        unit: "mmHg",
        aggregation: Aggregation::Mean,
        fresh_for_seconds: 3600,
    },
    MetricDefinition {
        metric: "diastolicPressure",
        unit: "mmHg",
        aggregation: Aggregation::Mean,
        fresh_for_seconds: 3600,
    },
    MetricDefinition {
        metric: "temperature",
        unit: "°C",
        aggregation: Aggregation::Mean,
        fresh_for_seconds: 3600,
    },
    MetricDefinition {
        metric: "sleepHours",
        unit: "h",
        aggregation: Aggregation::Last,
        fresh_for_seconds: 86_400,
    },
    MetricDefinition {
        metric: "mood",
        unit: "score",
        aggregation: Aggregation::Last,
        fresh_for_seconds: 86_400,
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
            METRICS.map(|policy| policy.metric),
            [
                "heartRate",
                "systolicPressure",
                "diastolicPressure",
                "temperature",
                "sleepHours",
                "mood"
            ]
        );
        assert_eq!(
            METRICS.map(|policy| policy.fresh_for_seconds),
            [15, 3600, 3600, 3600, 86_400, 86_400]
        );
        assert_eq!(definition_for("not-a-metric"), None);
    }
}
