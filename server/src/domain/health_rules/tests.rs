use super::*;
use crate::domain::{
    freshness,
    metric_config::{Aggregation, InputBounds},
};
use chrono::Utc;

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-15T00:00:00Z")
        .unwrap()
        .to_utc()
}

fn sample(id: &str, metric: &str, value: f64, at: Option<DateTime<Utc>>) -> Measurement {
    let policy = super::super::metric_config::definition_for(metric).unwrap();
    Measurement {
        id: id.into(),
        device_id: "manual-01".into(),
        astronaut_id: "demo-astronaut".into(),
        session_id: id.into(),
        sequence: 0,
        metric: metric.into(),
        value,
        unit: policy.unit.into(),
        measured_at: at.map(|time| time.to_rfc3339()),
        received_at: now().to_rfc3339(),
        source: "manual".into(),
    }
}

fn entry(metric: &'static str, value: f64, at: Option<DateTime<Utc>>) -> LatestMetric {
    LatestMetric {
        metric,
        latest_measurement: Some(sample("observed", metric, value, at)),
        latest_known_measurement: at.map(|_| sample("observed", metric, value, at)),
        latest_received_measurement: Some(sample("observed", metric, value, at)),
    }
}

fn assess(rows: &[LatestMetric], at: DateTime<Utc>) -> HealthEvaluation {
    let quality = freshness::evaluate_metrics(rows, at).unwrap();
    evaluate(rows, &quality).unwrap()
}

fn find_rule<'a>(health: &'a HealthEvaluation, metric: &str) -> &'a HealthRule {
    health
        .rules
        .iter()
        .find(|rule| rule.metric == metric)
        .unwrap()
}

#[test]
fn explanations_use_russian_units_and_compact_decimal_values() {
    for (metric, value, expected) in [
        ("mood", 2.0, "2 из 5."),
        ("sleepHours", 4.14, "4,14 ч."),
        ("heartRate", 72.0, "72 уд/мин."),
        ("systolicPressure", 115.0, "115 мм рт. ст."),
    ] {
        let result = assess(&[entry(metric, value, Some(now()))], now());
        let explanation = &find_rule(&result, metric).explanation;
        assert!(explanation.starts_with(expected), "{explanation}");
        assert!(!explanation.contains("score"));
    }
}

#[test]
fn every_numeric_boundary_has_the_documented_inclusive_or_strict_behavior() {
    use RuleStatus::{Attention, Critical, NoAlert};
    let cases = [
        ("heartRate", 59.999, Attention),
        ("heartRate", 60.0, NoAlert),
        ("heartRate", 100.0, NoAlert),
        ("heartRate", 100.001, Attention),
        ("heartRate", 250.0, Attention),
        ("systolicPressure", 89.999, Attention),
        ("systolicPressure", 90.0, NoAlert),
        ("systolicPressure", 119.999, NoAlert),
        ("systolicPressure", 120.0, Attention),
        ("systolicPressure", 129.999, Attention),
        ("systolicPressure", 130.0, Attention),
        ("systolicPressure", 180.0, Attention),
        ("systolicPressure", 180.001, Critical),
        ("diastolicPressure", 59.999, Attention),
        ("diastolicPressure", 60.0, NoAlert),
        ("diastolicPressure", 79.999, NoAlert),
        ("diastolicPressure", 80.0, Attention),
        ("diastolicPressure", 120.0, Attention),
        ("diastolicPressure", 120.001, Critical),
        ("temperature", 34.999, Critical),
        ("temperature", 35.0, NoAlert),
        ("temperature", 37.999, NoAlert),
        ("temperature", 38.0, Attention),
        ("temperature", 41.0, Attention),
        ("sleepHours", 0.0, Attention),
        ("sleepHours", 6.999, Attention),
        ("sleepHours", 7.0, NoAlert),
        ("mood", 1.0, Attention),
        ("mood", 2.999, Attention),
        ("mood", 3.0, NoAlert),
        ("mood", 5.0, NoAlert),
    ];
    for (metric, value, expected) in cases {
        let result = assess(&[entry(metric, value, Some(now()))], now());
        assert_eq!(
            find_rule(&result, metric).status,
            expected,
            "{metric}: {value}"
        );
    }
}

#[test]
fn summary_prioritizes_urgent_then_attention_over_missing_and_unconfigured_rules() {
    use RuleStatus::{Attention, Critical, InsufficientData, NoAlert, NotEvaluated};
    for (statuses, expected) in [
        (vec![NoAlert, NoAlert], NoAlert),
        (vec![NoAlert, InsufficientData], InsufficientData),
        (vec![NoAlert, NotEvaluated], InsufficientData),
        (vec![NotEvaluated, NotEvaluated], InsufficientData),
        (vec![NotEvaluated, Attention], Attention),
        (vec![Attention, InsufficientData], Attention),
        (vec![Attention, Critical, NotEvaluated], Critical),
        (vec![Critical, Attention, InsufficientData], Critical),
        (vec![], InsufficientData),
    ] {
        assert_eq!(summarize(statuses.into_iter()), expected);
    }
}

#[test]
fn missing_metrics_prevent_an_all_clear_summary() {
    let result = assess(&[entry("heartRate", 72.0, Some(now()))], now());
    assert_eq!(result.status, RuleStatus::InsufficientData);
    assert_eq!(result.evaluated_metric_count, 1);
    assert_eq!(result.total_metric_count, METRICS.len());
    assert_eq!(result.rules.len(), METRICS.len());
}

#[test]
fn severe_pressure_is_critical_without_the_other_component() {
    for metric in ["systolicPressure", "diastolicPressure"] {
        let result = assess(&[entry(metric, 200.0, Some(now()))], now());
        assert_eq!(result.status, RuleStatus::Critical, "{metric}");
        assert_eq!(find_rule(&result, metric).escalation, "urgent");
    }
}

#[test]
fn systolic_attention_uses_self_follow_up_below_130_and_ground_for_other_alerts() {
    for (value, expected) in [
        (89.0, "ground"),
        (120.0, "self"),
        (129.999, "self"),
        (130.0, "ground"),
    ] {
        let result = assess(&[entry("systolicPressure", value, Some(now()))], now());
        assert_eq!(find_rule(&result, "systolicPressure").escalation, expected);
    }
}

#[test]
fn stale_unknown_and_future_severe_values_never_produce_a_current_alert() {
    for at in [
        None,
        Some(now() - Duration::hours(14)),
        Some(now() + Duration::nanoseconds(1)),
    ] {
        let result = assess(&[entry("systolicPressure", 200.0, at)], now());
        let rule = find_rule(&result, "systolicPressure");
        assert_eq!(rule.status, RuleStatus::InsufficientData, "{at:?}");
        assert!(rule.measurement_ids.is_empty());
        assert_eq!(rule.valid_until, None);
        assert_eq!(result.evaluated_metric_count, 0);
    }
}

#[test]
fn newest_unknown_delivery_does_not_override_known_fresh_observation() {
    let mut row = entry(
        "systolicPressure",
        115.0,
        Some(now() - Duration::minutes(5)),
    );
    row.latest_received_measurement = Some(sample("unknown", "systolicPressure", 200.0, None));
    let result = assess(&[row], now());
    let rule = find_rule(&result, "systolicPressure");
    assert_eq!(rule.status, RuleStatus::NoAlert);
    assert_eq!(rule.measurement_ids, ["observed"]);
    assert!(rule.explanation.starts_with("115 мм рт. ст."));
}

#[test]
fn delayed_receipt_does_not_refresh_an_old_severe_observation() {
    let result = assess(
        &[entry(
            "heartRate",
            140.0,
            Some(now() - Duration::minutes(1)),
        )],
        now(),
    );
    assert_eq!(
        find_rule(&result, "heartRate").status,
        RuleStatus::InsufficientData
    );
}

#[test]
fn valid_until_uses_measurement_time_and_each_policy_including_the_exact_boundary() {
    for (metric, value) in [
        ("heartRate", 72.0),
        ("systolicPressure", 115.0),
        ("sleepHours", 8.0),
    ] {
        let policy = super::super::metric_config::definition_for(metric).unwrap();
        let observed = now() - Duration::seconds(i64::from(policy.fresh_for_seconds));
        let rows = [entry(metric, value, Some(observed))];
        let boundary = assess(&rows, now());
        let rule = find_rule(&boundary, metric);
        assert_eq!(rule.status, RuleStatus::NoAlert);
        assert_eq!(rule.valid_until.as_deref(), Some("2026-11-15T00:00:00Z"));
        let expired = assess(&rows, now() + Duration::nanoseconds(1));
        assert_eq!(
            find_rule(&expired, metric).status,
            RuleStatus::InsufficientData
        );
    }
}

#[test]
fn mismatched_quality_id_does_not_authorize_a_rule() {
    let rows = [entry("heartRate", 72.0, Some(now()))];
    let mut qualities = freshness::evaluate_metrics(&rows, now()).unwrap();
    qualities[0].quality.measurement_id = Some("another-observation".into());
    let result = evaluate(&rows, &qualities).unwrap();
    assert_eq!(
        find_rule(&result, "heartRate").status,
        RuleStatus::InsufficientData
    );
}

#[test]
fn corrupt_stored_value_or_unit_is_never_shown_as_no_alert() {
    for (metric, value, unit) in [
        ("heartRate", f64::NAN, "bpm"),
        ("heartRate", f64::INFINITY, "bpm"),
        ("heartRate", 72.0, "mmHg"),
        ("heartRate", 0.0, "bpm"),
        ("sleepHours", 25.0, "h"),
        ("mood", 6.0, "score"),
    ] {
        let mut row = entry(metric, value, Some(now()));
        row.latest_known_measurement.as_mut().unwrap().unit = unit.into();
        let result = assess(&[row], now());
        assert_eq!(
            find_rule(&result, metric).status,
            RuleStatus::InsufficientData
        );
    }
}

#[test]
fn corrupt_timestamp_is_an_error_without_an_alert_payload() {
    let mut row = entry("heartRate", 72.0, Some(now()));
    let qualities = freshness::evaluate_metrics(std::slice::from_ref(&row), now()).unwrap();
    row.latest_known_measurement.as_mut().unwrap().measured_at = Some("invalid".into());
    assert!(evaluate(&[row], &qualities).is_err());
}

#[test]
fn newly_registered_metric_without_rules_is_explicitly_not_evaluated() {
    let policy = MetricDefinition {
        metric: "newMetric",
        unit: "test",
        bounds: InputBounds::Positive,
        aggregation: Aggregation::Mean,
        fresh_for_seconds: 60,
    };
    let rule = evaluate_metric(&policy, None, None).unwrap();
    assert_eq!(rule.status, RuleStatus::NotEvaluated);
    assert!(rule.measurement_ids.is_empty());
    assert_eq!(rule.valid_until, None);
}

#[test]
fn all_six_unconfigured_registry_metrics_stay_unevaluated_for_every_quality_state() {
    let metrics = [
        ("spo2", 0.0),
        ("glucose", 500.0),
        ("visionBlur", 10.0),
        ("exerciseMinutes", 0.0),
        ("headache", 10.0),
        ("backPain", 10.0),
    ];
    for (metric, value) in metrics {
        for at in [
            None,
            Some(now()),
            Some(now() - Duration::days(2)),
            Some(now() + Duration::minutes(2)),
        ] {
            let result = assess(&[entry(metric, value, at)], now());
            let rule = find_rule(&result, metric);
            assert_eq!(rule.status, RuleStatus::NotEvaluated, "{metric} {at:?}");
            assert!(rule.measurement_ids.is_empty());
            assert_eq!(rule.valid_until, None);
            assert!(rule.source_urls.is_empty());
            assert_eq!(result.evaluated_metric_count, 0);
            assert_eq!(result.total_metric_count, 12);
            assert_eq!(result.status, RuleStatus::InsufficientData);
        }
    }
}

#[test]
fn critical_pressure_guidance_places_emergency_actions_before_asymptomatic_repeat() {
    let result = assess(&[entry("systolicPressure", 200.0, Some(now()))], now());
    let action = find_rule(&result, "systolicPressure").suggested_action;
    let emergency = action.find("немедленно привлеките экипаж").unwrap();
    let repeat = action.find("не раньше чем через минуту").unwrap();
    assert!(emergency < repeat);
    assert!(action.contains("Если тревожных симптомов нет"));
}

#[test]
fn mood_threshold_is_labeled_as_team_policy_and_not_attributed_to_nasa() {
    let result = assess(&[entry("mood", 2.0, Some(now()))], now());
    let rule = find_rule(&result, "mood");
    assert!(rule.explanation.contains("условный порог команды"));
    assert!(rule.explanation.contains("NASA не задаёт"));
    assert_eq!(rule.escalation, "crew");
}

#[test]
fn educational_hazards_have_actions_and_official_sources_even_without_data() {
    let result = assess(&[], now());
    assert_eq!(
        result
            .education
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>(),
        [
            "gravity",
            "isolation",
            "radiation",
            "distance",
            "environment"
        ]
    );
    for item in &result.education {
        assert!(!item.suggested_action.is_empty());
        assert!(item
            .source_urls
            .iter()
            .all(|url| url.starts_with("https://www.nasa.gov/")));
    }
    for metric in [
        "heartRate",
        "systolicPressure",
        "diastolicPressure",
        "temperature",
        "sleepHours",
        "mood",
    ] {
        assert!(!find_rule(&result, metric).source_urls.is_empty());
    }
}
