//! Versioned, conservative screening of the already evaluated BE-06 snapshot.
//! Rules describe individual observations, never diagnose a person or an organ.
mod content;

use super::{
    assessments::InvalidStoredTimestamp,
    freshness::{FreshnessStatus, MetricQuality},
    history::LatestMetric,
    measurements::Measurement,
    metric_config::{MetricDefinition, METRICS},
};
use chrono::{DateTime, Duration, SecondsFormat};

pub(crate) const RULE_SET_VERSION: &str = "be08-demo-1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuleStatus {
    NotEvaluated,
    InsufficientData,
    NoAlert,
    Attention,
    Critical,
}

impl RuleStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotEvaluated => "notEvaluated",
            Self::InsufficientData => "insufficientData",
            Self::NoAlert => "noAlert",
            Self::Attention => "attention",
            Self::Critical => "critical",
        }
    }

    fn is_evaluated(self) -> bool {
        matches!(self, Self::NoAlert | Self::Attention | Self::Critical)
    }
}

#[derive(Debug)]
pub(crate) struct HealthEvaluation {
    pub status: RuleStatus,
    pub explanation: String,
    pub disclaimer: &'static str,
    pub emergency_guidance: &'static str,
    pub evaluated_metric_count: usize,
    pub total_metric_count: usize,
    pub rules: Vec<HealthRule>,
    pub education: Vec<FlightEducation>,
}

#[derive(Debug)]
pub(crate) struct HealthRule {
    pub rule_id: String,
    pub metric: &'static str,
    pub title: &'static str,
    pub status: RuleStatus,
    pub explanation: String,
    pub measurement_ids: Vec<String>,
    pub valid_until: Option<String>,
    pub suggested_action: &'static str,
    pub escalation: &'static str,
    pub flight_context: &'static str,
    pub source_urls: &'static [&'static str],
}

#[derive(Debug)]
pub(crate) struct FlightEducation {
    pub id: &'static str,
    pub title: &'static str,
    pub explanation: &'static str,
    pub suggested_action: &'static str,
    pub source_urls: &'static [&'static str],
}

/// Use the caller's quality snapshot rather than reading a clock or database again.
pub(crate) fn evaluate(
    metrics: &[LatestMetric],
    qualities: &[MetricQuality],
) -> Result<HealthEvaluation, InvalidStoredTimestamp> {
    let rules = METRICS
        .iter()
        .map(|policy| {
            evaluate_metric(
                policy,
                metrics.iter().find(|entry| entry.metric == policy.metric),
                qualities.iter().find(|entry| entry.metric == policy.metric),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let evaluated_metric_count = rules
        .iter()
        .filter(|rule| rule.status.is_evaluated())
        .count();
    let status = summarize(rules.iter().map(|rule| rule.status));
    Ok(HealthEvaluation {
        status,
        explanation: format!(
            "{} Metrics evaluated: {} of {}.",
            match status {
                RuleStatus::Critical => "There is a signal to involve the crew and medical help urgently. Follow the guidance for the specific metric.",
                RuleStatus::Attention => "Some metrics need attention and a repeat check. See the reasons and next steps.",
                RuleStatus::NoAlert => "The configured rules did not trigger. This does not confirm that all organs are healthy and does not rule out a problem if there are symptoms.",
                RuleStatus::NotEvaluated | RuleStatus::InsufficientData => "A full assessment is not available: it needs fresh measurements with a known time and rules for all metrics.",
            },
            evaluated_metric_count,
            METRICS.len(),
        ),
        disclaimer: content::DISCLAIMER,
        emergency_guidance: content::EMERGENCY_GUIDANCE,
        evaluated_metric_count,
        total_metric_count: METRICS.len(),
        rules,
        education: content::education(),
    })
}

fn summarize(statuses: impl Iterator<Item = RuleStatus>) -> RuleStatus {
    let mut result = RuleStatus::NoAlert;
    let mut found = false;
    for status in statuses {
        found = true;
        match status {
            RuleStatus::Critical => return RuleStatus::Critical,
            RuleStatus::Attention => result = RuleStatus::Attention,
            RuleStatus::NotEvaluated | RuleStatus::InsufficientData
                if result != RuleStatus::Attention =>
            {
                result = RuleStatus::InsufficientData;
            }
            _ => {}
        }
    }
    if found {
        result
    } else {
        RuleStatus::InsufficientData
    }
}

fn evaluate_metric(
    policy: &MetricDefinition,
    metric: Option<&LatestMetric>,
    quality: Option<&MetricQuality>,
) -> Result<HealthRule, InvalidStoredTimestamp> {
    let Some(definition) = content::definition(policy.metric) else {
        return Ok(HealthRule {
            rule_id: format!("be08-{}", policy.metric),
            metric: policy.metric,
            title: policy.metric,
            status: RuleStatus::NotEvaluated,
            explanation: "Assessment rules are not connected for this metric yet. Its value cannot produce a no-alert status.".into(),
            measurement_ids: Vec::new(),
            valid_until: None,
            suggested_action: "Keep taking measurements per the mission plan; for interpretation contact the person responsible for medical care.",
            escalation: "none",
            flight_context: "The metric is available for observation; there is no automatic medical interpretation.",
            source_urls: &[],
        });
    };
    let mut result = HealthRule {
        rule_id: format!("be08-{}", policy.metric),
        metric: policy.metric,
        title: definition.title,
        status: RuleStatus::InsufficientData,
        explanation: "There is no usable measurement with confirmed freshness. The value is not assessed.".into(),
        measurement_ids: Vec::new(),
        valid_until: None,
        suggested_action: "Take a new measurement following the device instructions and record the measurement time. If there are symptoms, do not wait for the app's assessment.",
        escalation: "self",
        flight_context: definition.flight_context,
        source_urls: definition.source_urls,
    };
    let Some(quality) = quality.map(|entry| &entry.quality) else {
        return Ok(result);
    };
    let unavailable = match quality.status {
        FreshnessStatus::Missing => Some("The measurement is missing. The state for this metric is not assessed."),
        FreshnessStatus::Stale => Some("The measurement is stale for this metric's freshness window. Recent receipt does not confirm freshness; the current state is not assessed."),
        FreshnessStatus::UnknownTime => Some("Measurement time is unknown. Delivery time does not replace it; the state is not assessed."),
        FreshnessStatus::ClockSkew => Some("Measurement time is in the future. Check the source clock and measure again; the state is not assessed."),
        FreshnessStatus::Fresh => None,
    };
    if let Some(explanation) = unavailable {
        result.explanation = explanation.into();
        return Ok(result);
    }
    // Never substitute the last delivery for the known observation selected by BE-06.
    let Some(row) = metric.and_then(|entry| entry.latest_known_measurement.as_ref()) else {
        return Ok(result);
    };
    if quality.measurement_id.as_deref() != Some(row.id.as_str()) {
        result.explanation =
            "The measurement does not match the quality snapshot. Refresh the data; the state is not assessed."
                .into();
        return Ok(result);
    }
    if !valid_value(row, policy) {
        result.explanation = "The measurement unit or value does not match the input contract. Check the source; the state is not assessed.".into();
        return Ok(result);
    }
    let Some(measured_at) = row.measured_at.as_deref() else {
        return Ok(result);
    };
    let measured_at =
        DateTime::parse_from_rfc3339(measured_at).map_err(|_| InvalidStoredTimestamp)?;
    let valid_until = measured_at
        .checked_add_signed(Duration::seconds(i64::from(policy.fresh_for_seconds)))
        .ok_or(InvalidStoredTimestamp)?;
    let outcome = definition.classify(row.value);
    result.status = outcome.status;
    result.explanation = format!(
        "{} {}. {} {}",
        format_value(row.value),
        explanation_unit(policy).trim_end_matches('.'),
        outcome
            .criterion
            .replace(policy.unit, explanation_unit(policy)),
        definition.limitation,
    );
    result.measurement_ids.push(row.id.clone());
    result.valid_until = Some(
        valid_until
            .to_utc()
            .to_rfc3339_opts(SecondsFormat::AutoSi, true),
    );
    result.suggested_action = outcome.suggested_action;
    result.escalation = outcome.escalation;
    Ok(result)
}

fn valid_value(row: &Measurement, policy: &MetricDefinition) -> bool {
    row.metric == policy.metric && row.unit == policy.unit && policy.bounds.contains(row.value)
}

fn format_value(value: f64) -> String {
    format!("{value:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn explanation_unit(policy: &MetricDefinition) -> &str {
    match policy.unit {
        "score" if policy.metric == "mood" => "of 5",
        "score" => "of 10",
        "h" => "h",
        "bpm" => "bpm",
        "mmHg" => "mmHg",
        "min" => "min",
        "mg/dL" => "mg/dL",
        unit => unit,
    }
}

#[cfg(test)]
mod tests;
