// Taken from the team backend (branch codex/team-integration, commit 2c3d1db, server/src/domain/freshness.rs) unless noted below.
//! Per-measurement quality, evaluated at a caller-supplied server instant.
use super::{
    assessments::InvalidStoredTimestamp,
    history::LatestMetric,
    measurements::Measurement,
    metric_config::{definition_for, MetricDefinition},
};
use chrono::{DateTime, Duration, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FreshnessStatus {
    Missing,
    Fresh,
    Stale,
    UnknownTime,
    ClockSkew,
}

impl FreshnessStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Fresh => "fresh",
            Self::Stale => "stale",
            Self::UnknownTime => "unknownTime",
            Self::ClockSkew => "clockSkew",
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct Quality {
    pub status: FreshnessStatus,
    pub measurement_id: Option<String>,
    pub fresh_for_seconds: u32,
    pub age_seconds: Option<f64>,
    pub clock_skew: bool,
}

#[derive(Debug, PartialEq)]
pub(crate) struct MetricQuality {
    pub metric: &'static str,
    pub quality: Quality,
    pub latest_received_quality: Quality,
}

/// Receipt confirms delivery, never the age of the underlying observation.
pub(crate) fn evaluate(
    row: Option<&Measurement>,
    policy: &MetricDefinition,
    now: DateTime<Utc>,
) -> Result<Quality, InvalidStoredTimestamp> {
    let mut quality = Quality {
        status: FreshnessStatus::Missing,
        measurement_id: row.map(|row| row.id.clone()),
        fresh_for_seconds: policy.fresh_for_seconds,
        age_seconds: None,
        clock_skew: false,
    };
    let Some(row) = row else {
        return Ok(quality);
    };
    // A corrupt stored receipt must not be disguised as an unknown measurement time.
    DateTime::parse_from_rfc3339(&row.received_at).map_err(|_| InvalidStoredTimestamp)?;
    let Some(measured_at) = &row.measured_at else {
        quality.status = FreshnessStatus::UnknownTime;
        return Ok(quality);
    };
    let measured_at = DateTime::parse_from_rfc3339(measured_at)
        .map_err(|_| InvalidStoredTimestamp)?
        .with_timezone(&Utc);
    // Compare instants before subtracting: leap seconds can share Unix milliseconds
    // with the following ordinary second, but are distinct ordered Chrono instants.
    if measured_at > now {
        quality.status = FreshnessStatus::ClockSkew;
        quality.clock_skew = true;
        return Ok(quality);
    }
    let age = now.signed_duration_since(measured_at);
    if age < Duration::zero() {
        return Err(InvalidStoredTimestamp);
    }
    quality.age_seconds = Some(age.as_seconds_f64());
    quality.status = if age <= Duration::seconds(i64::from(policy.fresh_for_seconds)) {
        FreshnessStatus::Fresh
    } else {
        FreshnessStatus::Stale
    };
    Ok(quality)
}

/// Preserve snapshot order and evaluate the displayed known sample independently
/// from the last delivery, including when that delivery has an unknown time.
pub(crate) fn evaluate_metrics(
    metrics: &[LatestMetric],
    now: DateTime<Utc>,
) -> Result<Vec<MetricQuality>, InvalidStoredTimestamp> {
    metrics
        .iter()
        .map(|entry| {
            let policy = definition_for(entry.metric).ok_or(InvalidStoredTimestamp)?;
            Ok(MetricQuality {
                metric: entry.metric,
                quality: evaluate(
                    entry
                        .latest_known_measurement
                        .as_ref()
                        .or(entry.latest_received_measurement.as_ref()),
                    policy,
                    now,
                )?,
                latest_received_quality: evaluate(
                    entry.latest_received_measurement.as_ref(),
                    policy,
                    now,
                )?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::be::domain::metric_config::METRICS;

    fn instant(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn now() -> DateTime<Utc> {
        instant("2026-11-14T00:00:00Z")
    }

    fn row(id: &str, metric: &str, measured_at: Option<String>) -> Measurement {
        Measurement {
            id: id.into(),
            device_id: "manual-01".into(),
            astronaut_id: "demo-astronaut".into(),
            session_id: id.into(),
            sequence: 0,
            metric: metric.into(),
            value: 1.0,
            unit: "score".into(),
            measured_at,
            received_at: now().to_rfc3339(),
            source: "manual".into(),
        }
    }

    fn quality(row: Option<&Measurement>, metric: &str, at: DateTime<Utc>) -> Quality {
        evaluate(row, definition_for(metric).unwrap(), at).unwrap()
    }

    #[test]
    fn missing_and_unknown_time_are_distinct_even_when_delivery_is_recent() {
        let missing = quality(None, "heartRate", now());
        assert_eq!(missing.status, FreshnessStatus::Missing);
        assert_eq!(missing.measurement_id, None);
        assert_eq!(missing.age_seconds, None);
        assert!(!missing.clock_skew);

        let sample = row("unknown", "heartRate", None);
        let unknown = quality(Some(&sample), "heartRate", now());
        assert_eq!(unknown.status, FreshnessStatus::UnknownTime);
        assert_eq!(unknown.measurement_id.as_deref(), Some("unknown"));
        assert_eq!(unknown.age_seconds, None);
        assert!(!unknown.clock_skew);
    }

    #[test]
    fn every_policy_is_fresh_at_its_exact_boundary_and_stale_one_nanosecond_later() {
        for policy in METRICS {
            let measured_at = now() - Duration::seconds(i64::from(policy.fresh_for_seconds));
            let sample = row("boundary", policy.metric, Some(measured_at.to_rfc3339()));
            let at_boundary = evaluate(Some(&sample), &policy, now()).unwrap();
            assert_eq!(
                at_boundary.status,
                FreshnessStatus::Fresh,
                "{}",
                policy.metric
            );
            assert_eq!(
                at_boundary.age_seconds,
                Some(f64::from(policy.fresh_for_seconds))
            );
            let after_boundary =
                evaluate(Some(&sample), &policy, now() + Duration::nanoseconds(1)).unwrap();
            assert_eq!(
                after_boundary.status,
                FreshnessStatus::Stale,
                "{}",
                policy.metric
            );
        }
    }

    #[test]
    fn future_measurements_have_clock_skew_at_one_nanosecond_and_five_minutes() {
        for ahead in [
            Duration::nanoseconds(1),
            Duration::minutes(5),
            Duration::hours(1),
        ] {
            let sample = row("future", "heartRate", Some((now() + ahead).to_rfc3339()));
            let result = quality(Some(&sample), "heartRate", now());
            assert_eq!(result.status, FreshnessStatus::ClockSkew);
            assert!(result.clock_skew);
            assert_eq!(result.age_seconds, None);
        }
    }

    #[test]
    fn the_same_sample_transitions_from_clock_skew_to_fresh_then_stale() {
        let measured_at = now() + Duration::minutes(5);
        let sample = row("transition", "heartRate", Some(measured_at.to_rfc3339()));
        assert_eq!(
            quality(Some(&sample), "heartRate", now()).status,
            FreshnessStatus::ClockSkew
        );
        let at_measurement = quality(Some(&sample), "heartRate", measured_at);
        assert_eq!(at_measurement.status, FreshnessStatus::Fresh);
        assert_eq!(at_measurement.age_seconds, Some(0.0));
        assert!(!at_measurement.clock_skew);
        assert_eq!(
            quality(
                Some(&sample),
                "heartRate",
                measured_at + Duration::seconds(15)
            )
            .status,
            FreshnessStatus::Fresh
        );
        assert_eq!(
            quality(
                Some(&sample),
                "heartRate",
                measured_at + Duration::seconds(15) + Duration::nanoseconds(1)
            )
            .status,
            FreshnessStatus::Stale
        );
    }

    #[test]
    fn equivalent_offsets_and_chrono_fraction_precision_produce_identical_quality() {
        let mut sample = row(
            "same",
            "heartRate",
            Some("2026-11-13T23:59:59.123456789Z".into()),
        );
        let original = quality(Some(&sample), "heartRate", now());
        sample.measured_at = Some("2026-11-14T04:59:59.123456789999+05:00".into());
        assert_eq!(quality(Some(&sample), "heartRate", now()), original);
        assert_eq!(original.age_seconds, Some(0.876543211));
    }

    #[test]
    fn future_check_distinguishes_a_leap_second_from_the_next_second() {
        let leap = instant("2016-12-31T23:59:60Z");
        let after_leap = instant("2017-01-01T00:00:00Z");
        assert_eq!(leap.timestamp_millis(), after_leap.timestamp_millis());
        let sample = row(
            "following-second",
            "heartRate",
            Some(after_leap.to_rfc3339()),
        );
        assert_eq!(
            quality(Some(&sample), "heartRate", leap).status,
            FreshnessStatus::ClockSkew
        );
        let leap_sample = row("leap", "heartRate", Some(leap.to_rfc3339()));
        assert_eq!(
            quality(Some(&leap_sample), "heartRate", after_leap).status,
            FreshnessStatus::Fresh
        );
    }

    #[test]
    fn recent_receipt_does_not_refresh_a_delayed_observation() {
        let sample = row(
            "delayed",
            "heartRate",
            Some((now() - Duration::days(2)).to_rfc3339()),
        );
        let result = quality(Some(&sample), "heartRate", now());
        assert_eq!(result.status, FreshnessStatus::Stale);
        assert_eq!(result.age_seconds, Some(172_800.0));
    }

    #[test]
    fn old_known_times_do_not_overflow_nanosecond_age_calculation() {
        let sample = row("ancient", "heartRate", Some("0001-01-01T00:00:00Z".into()));
        let result = quality(Some(&sample), "heartRate", now());
        assert_eq!(result.status, FreshnessStatus::Stale);
        assert!(result.age_seconds.unwrap().is_finite());
        assert!(result.age_seconds.unwrap() > 60_000_000_000.0);
    }

    #[test]
    fn corrupt_stored_measurement_or_receipt_time_is_an_error() {
        let invalid_measured = row("bad-measured", "heartRate", Some("invalid".into()));
        assert!(evaluate(Some(&invalid_measured), &METRICS[0], now()).is_err());
        let mut invalid_received = row("bad-received", "heartRate", None);
        invalid_received.received_at = "invalid".into();
        assert!(evaluate(Some(&invalid_received), &METRICS[0], now()).is_err());
    }

    fn entry(
        metric: &'static str,
        known: Option<Measurement>,
        received: Option<Measurement>,
    ) -> LatestMetric {
        LatestMetric {
            metric,
            latest_measurement: None,
            latest_known_measurement: known,
            latest_received_measurement: received,
        }
    }

    #[test]
    fn fresh_pulse_does_not_refresh_stale_sleep_or_missing_mood() {
        let snapshot = vec![
            entry(
                "heartRate",
                Some(row("pulse", "heartRate", Some(now().to_rfc3339()))),
                None,
            ),
            entry(
                "sleepHours",
                Some(row(
                    "sleep",
                    "sleepHours",
                    Some((now() - Duration::days(2)).to_rfc3339()),
                )),
                None,
            ),
            entry("mood", None, None),
        ];
        let result = evaluate_metrics(&snapshot, now()).unwrap();
        assert_eq!(
            result
                .iter()
                .map(|metric| metric.quality.status)
                .collect::<Vec<_>>(),
            [
                FreshnessStatus::Fresh,
                FreshnessStatus::Stale,
                FreshnessStatus::Missing
            ]
        );
        assert_eq!(result[1].quality.measurement_id.as_deref(), Some("sleep"));
    }

    #[test]
    fn unknown_latest_delivery_does_not_hide_or_refresh_a_known_observation() {
        let snapshot = vec![entry(
            "heartRate",
            Some(row(
                "known",
                "heartRate",
                Some((now() - Duration::seconds(30)).to_rfc3339()),
            )),
            Some(row("unknown", "heartRate", None)),
        )];
        let result = evaluate_metrics(&snapshot, now()).unwrap();
        assert_eq!(result[0].quality.status, FreshnessStatus::Stale);
        assert_eq!(result[0].quality.measurement_id.as_deref(), Some("known"));
        assert_eq!(
            result[0].latest_received_quality.status,
            FreshnessStatus::UnknownTime
        );
        assert_eq!(
            result[0].latest_received_quality.measurement_id.as_deref(),
            Some("unknown")
        );
    }

    #[test]
    fn a_delayed_delivery_does_not_replace_a_fresh_known_observation() {
        let snapshot = vec![entry(
            "heartRate",
            Some(row("known", "heartRate", Some(now().to_rfc3339()))),
            Some(row(
                "delayed",
                "heartRate",
                Some((now() - Duration::hours(1)).to_rfc3339()),
            )),
        )];
        let result = evaluate_metrics(&snapshot, now()).unwrap();
        assert_eq!(result[0].quality.status, FreshnessStatus::Fresh);
        assert_eq!(
            result[0].latest_received_quality.status,
            FreshnessStatus::Stale
        );
    }

    #[test]
    fn unknown_only_metric_reports_the_received_record_in_both_qualities() {
        let snapshot = vec![entry("mood", None, Some(row("only-unknown", "mood", None)))];
        let result = evaluate_metrics(&snapshot, now()).unwrap();
        assert_eq!(result[0].quality.status, FreshnessStatus::UnknownTime);
        assert_eq!(result[0].quality, result[0].latest_received_quality);
    }

    #[test]
    fn unsupported_metric_policy_is_an_error_instead_of_a_default() {
        let snapshot = vec![entry("unsupported", None, None)];
        assert!(evaluate_metrics(&snapshot, now()).is_err());
    }
}
