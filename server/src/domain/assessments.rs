use super::{
    freshness::{evaluate, FreshnessStatus},
    history::LatestMetric,
    measurements::Measurement,
    metric_config::definition_for,
};
use chrono::{DateTime, Utc};

#[derive(Debug)]
pub(crate) struct Assessment {
    pub status: &'static str,
    pub explanation: &'static str,
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid stored timestamp")]
pub(crate) struct InvalidStoredTimestamp;

/// Compatibility summary for one last-received record, using the common policy.
pub(crate) fn assess(
    latest: Option<&Measurement>,
    now: DateTime<Utc>,
) -> Result<Assessment, InvalidStoredTimestamp> {
    let Some(row) = latest else {
        return Ok(Assessment {
            status: "no-data",
            explanation: "Нет измерений. Запустите симулятор или введите показатель.",
        });
    };
    let policy = definition_for(&row.metric).ok_or(InvalidStoredTimestamp)?;
    let quality = evaluate(Some(row), policy, now)?;
    Ok(match quality.status {
        FreshnessStatus::Missing => Assessment {
            status: "no-data",
            explanation: "Нет измерений. Запустите симулятор или введите показатель.",
        },
        FreshnessStatus::UnknownTime => Assessment {
            status: "unknown-time",
            explanation:
                "Время измерения неизвестно. Время получения не подтверждает свежесть показателя.",
        },
        FreshnessStatus::ClockSkew => Assessment {
            status: "clock-skew",
            explanation: "Время измерения находится в будущем относительно сервера. Проверьте часы источника; свежесть не подтверждена.",
        },
        FreshnessStatus::Stale => Assessment {
            status: "stale",
            explanation: "Последнее принятое измерение устарело по демонстрационной политике показателя. Это не медицинская оценка.",
        },
        FreshnessStatus::Fresh => Assessment {
            status: "not-evaluated",
            explanation: "Последнее принятое измерение достаточно свежее по демонстрационной политике показателя. Медицинские правила пока не подключены.",
        },
    })
}

/// Select the last receipt across all metrics from the same read snapshot.
pub(crate) fn assess_snapshot(
    metrics: &[LatestMetric],
    now: DateTime<Utc>,
) -> Result<Assessment, InvalidStoredTimestamp> {
    let mut latest: Option<(i64, &str, &Measurement)> = None;
    for row in metrics
        .iter()
        .filter_map(|metric| metric.latest_received_measurement.as_ref())
    {
        let received_ms = DateTime::parse_from_rfc3339(&row.received_at)
            .map_err(|_| InvalidStoredTimestamp)?
            .timestamp_millis();
        if latest
            .as_ref()
            .is_none_or(|(time, id, _)| (received_ms, row.id.as_str()) > (*time, *id))
        {
            latest = Some((received_ms, &row.id, row));
        }
    }
    assess(latest.map(|(_, _, row)| row), now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-14T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn sample(
        id: &str,
        metric: &str,
        measured_at: Option<String>,
        received_at: &str,
    ) -> Measurement {
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
            received_at: received_at.into(),
            source: "manual".into(),
        }
    }

    fn entry(metric: &'static str, received: Measurement) -> LatestMetric {
        LatestMetric {
            metric,
            latest_measurement: None,
            latest_known_measurement: None,
            latest_received_measurement: Some(received),
        }
    }

    #[test]
    fn compatibility_summary_uses_the_selected_metrics_policy() {
        let measured = Some((now() - Duration::hours(1)).to_rfc3339());
        let sleep = sample(
            "sleep",
            "sleepHours",
            measured.clone(),
            "2026-11-14T00:00:00Z",
        );
        let pulse = sample("pulse", "heartRate", measured, "2026-11-14T00:00:00Z");
        assert_eq!(assess(Some(&sleep), now()).unwrap().status, "not-evaluated");
        assert_eq!(assess(Some(&pulse), now()).unwrap().status, "stale");
    }

    #[test]
    fn compatibility_summary_reports_missing_unknown_and_future_separately() {
        assert_eq!(assess(None, now()).unwrap().status, "no-data");
        let unknown = sample("unknown", "heartRate", None, "2026-11-14T00:00:00Z");
        assert_eq!(
            assess(Some(&unknown), now()).unwrap().status,
            "unknown-time"
        );
        let future = sample(
            "future",
            "heartRate",
            Some((now() + Duration::nanoseconds(1)).to_rfc3339()),
            "2026-11-14T00:00:00Z",
        );
        assert_eq!(assess(Some(&future), now()).unwrap().status, "clock-skew");
    }

    #[test]
    fn snapshot_summary_selects_receipt_by_utc_milliseconds_then_id() {
        let snapshot = vec![
            entry(
                "heartRate",
                sample(
                    "a",
                    "heartRate",
                    Some(now().to_rfc3339()),
                    "2026-11-14T00:00:00Z",
                ),
            ),
            entry(
                "headache",
                sample("z", "headache", None, "2026-11-14T05:00:00+05:00"),
            ),
            entry(
                "mood",
                sample(
                    "zz",
                    "mood",
                    Some(now().to_rfc3339()),
                    "2026-11-13T23:59:59.999Z",
                ),
            ),
        ];
        assert_eq!(
            assess_snapshot(&snapshot, now()).unwrap().status,
            "unknown-time"
        );
    }

    #[test]
    fn snapshot_summary_rejects_any_corrupt_receipt_instead_of_silently_ignoring_it() {
        let snapshot = vec![
            entry(
                "heartRate",
                sample("valid", "heartRate", None, "2026-11-14T00:00:00Z"),
            ),
            entry(
                "sleepHours",
                sample("invalid", "sleepHours", None, "corrupt"),
            ),
        ];
        assert!(assess_snapshot(&snapshot, now()).is_err());
        assert_eq!(assess_snapshot(&[], now()).unwrap().status, "no-data");
    }
}
