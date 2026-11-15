use serde::{Deserialize, Serialize};

/// Server-produced research context, independent of clinical rule status.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchInsight {
    pub metric: String,
    pub catalog_version: String,
    pub observation: String,
    pub measurement_id: Option<String>,
    pub quality: String,
    pub contexts: Vec<ResearchContext>,
    pub actions: Vec<ResearchAction>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchContext {
    pub title: String,
    pub explanation: String,
    pub project_use: String,
    pub limitation: String,
    pub source_url: String,
    pub source_agency: String,
    pub source_section: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResearchAction {
    pub kind: ActionKind,
    pub label: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ActionKind {
    Record,
    History,
    Guidance,
}
