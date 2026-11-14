//! Explainable server-owned health guidance shared by the native interfaces.
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum HealthStatus {
    #[default]
    NotEvaluated,
    InsufficientData,
    NoAlert,
    Attention,
    Critical,
}

impl HealthStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::NotEvaluated => "Not evaluated",
            Self::InsufficientData => "Insufficient data",
            Self::NoAlert => "No rule alerts",
            Self::Attention => "Needs attention",
            Self::Critical => "Urgent action",
        }
    }

    pub fn is_evaluated(self) -> bool {
        matches!(self, Self::NoAlert | Self::Attention | Self::Critical)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Escalation {
    None,
    #[serde(rename = "self")]
    SelfCare,
    Crew,
    Ground,
    Urgent,
}

impl Escalation {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "Routine observation",
            Self::SelfCare => "Personal next step",
            Self::Crew => "Involve the crew",
            Self::Ground => "Contact medical support",
            Self::Urgent => "Urgent crew / medical assistance",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthRule {
    pub rule_id: String,
    pub rule_version: String,
    pub metric: String,
    pub title: String,
    pub status: HealthStatus,
    pub explanation: String,
    pub measurement_ids: Vec<String>,
    pub valid_until: Option<String>,
    pub suggested_action: String,
    pub escalation: Escalation,
    pub flight_context: String,
    pub source_urls: Vec<String>,
    pub demonstration_only: bool,
    /// Client validity metadata; never received from the wire.
    #[serde(skip)]
    pub current: bool,
    #[serde(skip)]
    pub unavailable_reason: Option<String>,
}

impl HealthRule {
    pub fn current_status(&self) -> HealthStatus {
        if self.current {
            self.status
        } else {
            HealthStatus::NotEvaluated
        }
    }

    pub fn detail_text(&self) -> String {
        let validity = if self.current {
            "Current server result".into()
        } else {
            format!(
                "Historical / not current: {}",
                self.unavailable_reason
                    .as_deref()
                    .unwrap_or("waiting for a valid assessment")
            )
        };
        format!("{} — {}\n{}\n{}\n\nNext step: {}\nEscalation: {}\nFlight context: {}\n\nRule: {} / {}\nValid until: {}\nSources:\n{}",
            self.title, self.status.label(), validity, self.explanation, self.suggested_action,
            self.escalation.label(), self.flight_context, self.rule_id, self.rule_version,
            self.valid_until.as_deref().unwrap_or("not evaluated"), self.source_urls.join("\n"))
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlightEducation {
    pub id: String,
    pub title: String,
    pub explanation: String,
    pub suggested_action: String,
    pub source_urls: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthEvaluation {
    pub rule_set_version: String,
    pub demonstration_only: bool,
    pub status: HealthStatus,
    pub explanation: String,
    pub disclaimer: String,
    pub emergency_guidance: String,
    pub evaluated_metric_count: usize,
    pub total_metric_count: usize,
    pub rules: Vec<HealthRule>,
    pub education: Vec<FlightEducation>,
}

#[derive(Clone, Debug, Default)]
pub struct HealthReport {
    pub evaluation: Option<HealthEvaluation>,
    pub evaluated_at: Option<String>,
    pub status: HealthStatus,
    pub message: String,
}
