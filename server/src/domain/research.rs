//! Uses curated NASA content to explain actual snapshot measurements without inventing diagnoses.
use super::{assessments::InvalidStoredTimestamp, freshness::MetricQuality, history::LatestMetric};
use serde::Deserialize;
#[path = "../../../resources/nasa/insight_types.rs"]
mod types;
pub(crate) use types::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Catalog {
    version: String,
    sources: Vec<Source>,
    entries: Vec<Entry>,
}
#[derive(Deserialize)]
struct Source {
    agency: String,
    id: String,
    url: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    title: String,
    summary: String,
    project_use: String,
    limitation: String,
    source_id: String,
    source_section: String,
    metrics: Vec<String>,
}

pub(crate) fn insights(
    metrics: &[LatestMetric],
    qualities: &[MetricQuality],
) -> Result<Vec<ResearchInsight>, InvalidStoredTimestamp> {
    let catalog: Catalog =
        serde_json::from_str(include_str!("../../../resources/nasa/catalog.json"))
            .map_err(|_| InvalidStoredTimestamp)?;
    metrics.iter().filter(|m| matches!(m.metric, "sleepHours" | "heartRate" | "systolicPressure" | "diastolicPressure" | "exerciseMinutes")).map(|m| {
        let quality = &qualities.iter().find(|q| q.metric == m.metric).ok_or(InvalidStoredTimestamp)?.quality;
        let measurement = m.latest_known_measurement.as_ref().or(m.latest_received_measurement.as_ref());
        let quality_label = match quality.status.as_str() {
            "fresh" => "свежее по политике данных",
            "stale" => "устарело; нужно новое измерение",
            "unknownTime" => "время неизвестно; свежесть не подтверждена",
            "clockSkew" => "время в будущем; проверьте часы источника",
            _ => "нет данных",
        };
        let observation = match measurement {
            Some(row) => format!("Записано: {} {}. Время измерения: {}. Источник: {}. Качество на момент ответа: {}. Это одно наблюдение, а не вывод о тенденции или диагноз.", row.value, row.unit, row.measured_at.as_deref().unwrap_or("неизвестно"), row.source, quality_label),
            None => "Измерений пока нет. Заполните показатель, чтобы наблюдать его вместе с научным контекстом.".into(),
        };
        let label = match m.metric {
            "sleepHours" => "Записать длительность сна",
            "exerciseMinutes" => "Записать минуты активности",
            _ => "Записать повторное измерение",
        };
        let contexts = catalog.entries.iter().filter(|e| e.metrics.iter().any(|id| id == m.metric)).map(|e| {
            let source = catalog.sources.iter().find(|s| s.id == e.source_id).ok_or(InvalidStoredTimestamp)?;
            Ok(ResearchContext { title: e.title.clone(), explanation: e.summary.clone(), project_use: e.project_use.clone(), limitation: e.limitation.clone(), source_url: source.url.clone(),
                            source_agency: source.agency.clone(), source_section: e.source_section.clone() })
        }).collect::<Result<Vec<_>, InvalidStoredTimestamp>>()?;
        Ok(ResearchInsight { metric: m.metric.into(), catalog_version: catalog.version.clone(), observation, measurement_id: quality.measurement_id.clone(), quality: quality.status.as_str().into(), contexts, actions: vec![
            ResearchAction { kind: ActionKind::Record, label: label.into() },
            ResearchAction { kind: ActionKind::History, label: "Посмотреть историю показателя".into() },
            ResearchAction { kind: ActionKind::Guidance, label: "Открыть оценку BE-08 и рекомендации".into() },
        ] })
    }).collect()
}
