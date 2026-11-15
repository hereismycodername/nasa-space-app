//! Offline research context shared with the browser. These mappings are not clinical rules.
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub version: String,
    pub reviewed_at: String,
    pub notice: String,
    pub sources: Vec<Source>,
    pub entries: Vec<Entry>,
}
#[derive(Debug, Deserialize)]
pub struct Source {
    pub agency: String,
    pub id: String,
    pub title: String,
    pub url: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub source_section: String,
    pub source_id: String,
    pub summary: String,
    pub metrics: Vec<String>,
    pub project_use: String,
    pub limitation: String,
}

pub fn catalog() -> Result<&'static Catalog, &'static str> {
    static CATALOG: OnceLock<Result<Catalog, serde_json::Error>> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            serde_json::from_str(include_str!("../../../../resources/nasa/catalog.json"))
        })
        .as_ref()
        .map_err(|_| "NASA catalogue unavailable: invalid embedded data")
}

/// Native historical IDs translated to the common API metric IDs.
pub fn api_metric(id: &str) -> &str {
    match id {
        "hr" => "heartRate",
        "sbp" => "systolicPressure",
        "dbp" => "diastolicPressure",
        "temp" => "temperature",
        "sleep" => "sleepHours",
        other => other,
    }
}
impl Catalog {
    pub fn entries_for<'a>(&'a self, metric: Option<&'a str>) -> impl Iterator<Item = &'a Entry> {
        self.entries.iter().filter(move |entry| {
            metric.is_none_or(|id| entry.metrics.iter().any(|m| m == api_metric(id)))
        })
    }
    pub fn source(&self, entry: &Entry) -> Option<&Source> {
        self.sources
            .iter()
            .find(|source| source.id == entry.source_id)
    }
    pub fn text(&self, metric: Option<&str>) -> String {
        let mut text = format!("NASA / CSA · {} · {}\n{}\nСимулятор: синтетические показания. Не диагноз и не пороги NASA.\n\n", self.version, self.reviewed_at, self.notice);
        let entries: Vec<_> = self.entries_for(metric).collect();
        if entries.is_empty() {
            text.push_str("Прямой связи с показателем в каталоге нет.\n");
        }
        for entry in entries {
            text.push_str(&format!(
                "{} ({})\n{}\nПоказатели: {}\nКак используем: {}\nОграничение: {}\n",
                entry.title,
                entry.kind,
                entry.summary,
                entry.metrics.join(", "),
                entry.project_use,
                entry.limitation
            ));
            if let Some(source) = self.source(entry) {
                text.push_str(&format!(
                    "{}: {} — {}\n{}\n\n",
                    source.agency, source.title, entry.source_section, source.url
                ));
            }
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_catalog_has_resolved_sources_and_native_metric_links() {
        let catalog = catalog().expect("valid shared JSON");
        assert_eq!(catalog.entries.len(), 10);
        for entry in &catalog.entries {
            let source = catalog.source(entry).unwrap();
            let prefix = if source.agency == "NASA" {
                "https://www.nasa.gov/"
            } else {
                "https://www.asc-csa.gc.ca/"
            };
            assert!(source.url.starts_with(prefix));
        }
        assert_eq!(catalog.entries_for(Some("hr")).count(), 2);
        assert_eq!(catalog.entries_for(Some("sleep")).count(), 3);
        assert_eq!(catalog.entries_for(Some("glucose")).count(), 0);
        assert!(catalog
            .text(Some("hr"))
            .contains("https://www.nasa.gov/hrp/risks/"));
        assert!(catalog.text(Some("headache")).contains("Прямой связи"));
    }
}
