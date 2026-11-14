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
            "{} Оценено показателей: {} из {}.",
            match status {
                RuleStatus::Critical => "Есть сигнал для срочного привлечения экипажа и медицинской помощи. Следуйте подсказке конкретного показателя.",
                RuleStatus::Attention => "Есть показатели, требующие внимания и повторной проверки. См. причины и следующие действия.",
                RuleStatus::NoAlert => "Заданные правила не сработали. Это не подтверждает здоровье всех органов и не исключает проблему при симптомах.",
                RuleStatus::NotEvaluated | RuleStatus::InsufficientData => "Полная оценка недоступна: нужны свежие измерения с известным временем и правила для всех показателей.",
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
            explanation: "Для этого показателя правила оценки ещё не подключены. По его значению нельзя получить статус отсутствия предупреждений.".into(),
            measurement_ids: Vec::new(),
            valid_until: None,
            suggested_action: "Сохраняйте измерения по плану миссии; для интерпретации обратитесь к ответственному за медицинскую помощь.",
            escalation: "none",
            flight_context: "Показатель доступен для наблюдения; автоматическая медицинская интерпретация отсутствует.",
            source_urls: &[],
        });
    };
    let mut result = HealthRule {
        rule_id: format!("be08-{}", policy.metric),
        metric: policy.metric,
        title: definition.title,
        status: RuleStatus::InsufficientData,
        explanation: "Нет пригодного измерения с подтверждённой свежестью. Значение не оценено.".into(),
        measurement_ids: Vec::new(),
        valid_until: None,
        suggested_action: "Получите новое измерение по инструкции прибора и сохраните время измерения. При симптомах не ждите оценки приложения.",
        escalation: "self",
        flight_context: definition.flight_context,
        source_urls: definition.source_urls,
    };
    let Some(quality) = quality.map(|entry| &entry.quality) else {
        return Ok(result);
    };
    let unavailable = match quality.status {
        FreshnessStatus::Missing => Some("Измерение отсутствует. Состояние по этому показателю не оценено."),
        FreshnessStatus::Stale => Some("Измерение устарело по сроку свежести этого показателя. Недавнее получение не подтверждает свежесть; текущее состояние не оценено."),
        FreshnessStatus::UnknownTime => Some("Время измерения неизвестно. Время доставки не заменяет его; состояние не оценено."),
        FreshnessStatus::ClockSkew => Some("Время измерения находится в будущем. Проверьте часы источника и повторите измерение; состояние не оценено."),
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
            "Измерение не совпадает со снимком качества. Обновите данные; состояние не оценено."
                .into();
        return Ok(result);
    }
    if !valid_value(row, policy) {
        result.explanation = "Единица или значение измерения не соответствует входному контракту. Проверьте источник; состояние не оценено.".into();
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
        .replace('.', ",")
}

fn explanation_unit(policy: &MetricDefinition) -> &str {
    match policy.unit {
        "score" if policy.metric == "mood" => "из 5",
        "score" => "из 10",
        "h" => "ч",
        "bpm" => "уд/мин",
        "mmHg" => "мм рт. ст.",
        "min" => "мин",
        "mg/dL" => "мг/дл",
        unit => unit,
    }
}

#[cfg(test)]
mod tests;
