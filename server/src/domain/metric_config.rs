//! Demonstration collection policies, separate from clinical health thresholds.

pub(crate) const MAX_FUTURE_SKEW_SECONDS: i64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MetricPolicy {
    pub metric: &'static str,
    pub fresh_for_seconds: u32,
}

pub(crate) const METRIC_POLICIES: [MetricPolicy; 6] = [
    MetricPolicy {
        metric: "heartRate",
        fresh_for_seconds: 15,
    },
    MetricPolicy {
        metric: "systolicPressure",
        fresh_for_seconds: 3600,
    },
    MetricPolicy {
        metric: "diastolicPressure",
        fresh_for_seconds: 3600,
    },
    MetricPolicy {
        metric: "temperature",
        fresh_for_seconds: 3600,
    },
    MetricPolicy {
        metric: "sleepHours",
        fresh_for_seconds: 86_400,
    },
    MetricPolicy {
        metric: "mood",
        fresh_for_seconds: 86_400,
    },
];

pub(crate) fn policy_for(metric: &str) -> Option<&'static MetricPolicy> {
    METRIC_POLICIES
        .iter()
        .find(|policy| policy.metric == metric)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::measurements::METRICS;

    #[test]
    fn every_supported_metric_has_one_explicit_freshness_policy() {
        assert_eq!(METRIC_POLICIES.map(|policy| policy.metric), METRICS);
        assert_eq!(
            METRIC_POLICIES.map(|policy| policy.fresh_for_seconds),
            [15, 3600, 3600, 3600, 86_400, 86_400]
        );
        assert_eq!(policy_for("not-a-metric"), None);
    }
}
