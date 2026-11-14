//! Client validity checks only. Numeric health rules remain on the server.
use crate::{catalog::METRICS, dto, State};
use std::collections::HashSet;
use ui_model::{HealthReport, HealthStatus};

pub(crate) fn validate(assessment: &dto::Assessment) -> Result<(), String> {
    dto::timestamp_ms(&assessment.evaluated_at)?;
    if assessment.astronaut_id.is_empty() {
        return Err("Assessment profile is missing".into());
    }
    let Some(health) = &assessment.health else {
        return Ok(());
    };
    if !health.demonstration_only
        || health.rule_set_version.is_empty()
        || health.total_metric_count == 0
        || health.evaluated_metric_count > health.total_metric_count
        || health.rules.len() != health.total_metric_count
    {
        return Err("Invalid health evaluation metadata".into());
    }
    let mut seen = HashSet::new();
    for rule in &health.rules {
        if !seen.insert(&rule.metric)
            || !rule.demonstration_only
            || rule.rule_version != health.rule_set_version
        {
            return Err("Repeated metric or inconsistent health rule version".into());
        }
        if rule.status.is_evaluated() {
            if rule.measurement_ids.len() != 1
                || rule.measurement_ids[0].is_empty()
                || rule.valid_until.is_none()
            {
                return Err("Evaluated health rule has no measurement linkage".into());
            }
        } else if !rule.measurement_ids.is_empty() || rule.valid_until.is_some() {
            return Err("Unevaluated health rule contains evaluated measurement metadata".into());
        }
        if let Some(until) = &rule.valid_until {
            dto::timestamp_ms(until)?;
        }
    }
    Ok(())
}

pub(crate) fn report(state: &State) -> HealthReport {
    let Some(assessment) = &state.assessment else {
        return HealthReport {
            message: state
                .assessment_error
                .clone()
                .unwrap_or_else(|| "Waiting for server health guidance".into()),
            ..HealthReport::default()
        };
    };
    let Some(mut health) = assessment.health.clone() else {
        return HealthReport {
            evaluated_at: Some(assessment.evaluated_at.clone()),
            message: "This server does not publish health rules. Measurements remain available."
                .into(),
            ..HealthReport::default()
        };
    };
    let unavailable = if !state.online {
        Some("Offline: previous evaluation is not current".to_string())
    } else if let Some(error) = &state.assessment_error {
        Some(format!("Health guidance refresh failed: {error}"))
    } else if state
        .latest
        .as_ref()
        .is_none_or(|latest| latest.astronaut_id != assessment.astronaut_id)
    {
        Some("Assessment profile differs from current measurements".into())
    } else {
        None
    };
    let elapsed = state.assessment_at.map_or(i64::MAX, |at| {
        at.elapsed().as_millis().min(i64::MAX as u128) as i64
    });
    let now = dto::timestamp_ms(&assessment.evaluated_at)
        .unwrap_or(i64::MAX)
        .saturating_add(elapsed);
    for rule in &mut health.rules {
        let reason = unavailable.clone().or_else(|| {
            let Some(metric) = METRICS.iter().find(|metric| metric.api == rule.metric) else {
                return Some("This client does not display this metric".into());
            };
            if !rule.status.is_evaluated() {
                return None;
            }
            let Some(entry) = state.entry(metric.id) else {
                return Some("No latest measurement to match".into());
            };
            let Some(measurement) = &entry.latest_known_measurement else {
                return Some("No measurement with a known time".into());
            };
            if rule.measurement_ids.len() != 1 || rule.measurement_ids[0] != measurement.id {
                return Some("Measurement changed; waiting for matching assessment".into());
            }
            if entry.quality.label(state.elapsed()) != "fresh" || entry.quality.clock_skew {
                return Some("Latest measurement is no longer eligible for evaluation".into());
            }
            let until = rule
                .valid_until
                .as_deref()
                .and_then(|value| dto::timestamp_ms(value).ok());
            if until.is_none_or(|until| now > until) {
                return Some("Evaluation expired; obtain a current measurement".into());
            }
            None
        });
        rule.current = reason.is_none();
        rule.unavailable_reason = reason;
    }
    let active: Vec<_> = health
        .rules
        .iter()
        .map(|rule| rule.current_status())
        .collect();
    let status = if unavailable.is_some() {
        HealthStatus::NotEvaluated
    } else if active.contains(&HealthStatus::Critical) {
        HealthStatus::Critical
    } else if active.contains(&HealthStatus::Attention) {
        HealthStatus::Attention
    } else if active.len() == health.total_metric_count
        && active.iter().all(|status| *status == HealthStatus::NoAlert)
    {
        HealthStatus::NoAlert
    } else {
        HealthStatus::InsufficientData
    };
    let current_count = health
        .rules
        .iter()
        .filter(|rule| rule.current && rule.status.is_evaluated())
        .count();
    let message = unavailable.unwrap_or_else(|| format!("{current_count}/{} current rule evaluations. No rule alerts does not mean that all organs are healthy.", health.total_metric_count));
    HealthReport {
        evaluation: Some(health),
        evaluated_at: Some(assessment.evaluated_at.clone()),
        status,
        message,
    }
}
