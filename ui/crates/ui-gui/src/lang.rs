//! Interface language of the shared-server window (Russian or English).
//!
//! Only what the window itself says is translated, plus the fixed status messages of `ui-http`.
//! Texts written by the server (rule explanations, actions, flight context, task titles) are shown
//! exactly as received. Russian terms follow the React client (`src/features/dashboard`).

use chrono::{DateTime, Utc};
use ui_model::{fmt_num, Escalation, HealthStatus};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Ru,
    En,
}

impl Lang {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "ru" => Some(Lang::Ru),
            "en" => Some(Lang::En),
            _ => None,
        }
    }

    /// Pick the text for this language.
    pub fn t(self, en: &'static str, ru: &'static str) -> &'static str {
        match self {
            Lang::En => en,
            Lang::Ru => ru,
        }
    }

    /// A measured value for display (Russian uses a decimal comma, as the React client does).
    pub fn num(self, value: f64) -> String {
        self.decimal(fmt_num(value))
    }

    pub fn decimal(self, text: String) -> String {
        match self {
            Lang::En => text,
            Lang::Ru => text.replace('.', ","),
        }
    }

    pub fn metric(self, id: &str, fallback: &str) -> String {
        if self == Lang::En {
            return fallback.to_string();
        }
        match id {
            "hr" => "Пульс",
            "sbp" => "Систолическое давление",
            "dbp" => "Диастолическое давление",
            "temp" => "Температура",
            "sleep" => "Сон",
            "mood" => "Самочувствие",
            "spo2" => "Насыщение крови кислородом",
            "glucose" => "Глюкоза крови",
            "visionBlur" => "Нечёткость зрения",
            "exerciseMinutes" => "Активность за 24 часа",
            "headache" => "Головная боль",
            "backPain" => "Боль в спине",
            _ => fallback,
        }
        .to_string()
    }

    pub fn system(self, id: &str, fallback: &str) -> String {
        if self == Lang::En {
            return fallback.to_string();
        }
        match id {
            "brain" => "Сон, самочувствие и голова",
            "heart" => "Сердце и кровообращение",
            "immune" => "Температура тела",
            "lungs" => "Дыхательная система",
            "eyes" => "Зрение",
            "gut" => "Пищеварение и обмен веществ",
            "msk" => "Кости, мышцы и спина",
            _ => fallback,
        }
        .to_string()
    }

    /// Unit for display; the API unit is unchanged in requests.
    pub fn unit(self, unit: &str, metric: &str) -> String {
        match (self, unit) {
            (_, "score") => match (self, metric) {
                (Lang::Ru, "mood") => "из 5",
                (Lang::Ru, _) => "из 10",
                (Lang::En, "mood") => "of 5",
                (Lang::En, _) => "of 10",
            }
            .to_string(),
            (Lang::En, _) => unit.to_string(),
            (Lang::Ru, "bpm") => "уд/мин".into(),
            (Lang::Ru, "mmHg") => "мм рт. ст.".into(),
            (Lang::Ru, "h") => "ч".into(),
            (Lang::Ru, "mg/dL") => "мг/дл".into(),
            (Lang::Ru, "min") => "мин".into(),
            (Lang::Ru, _) => unit.to_string(),
        }
    }

    /// Full rule result, as in the React client.
    pub fn health(self, status: HealthStatus) -> &'static str {
        match self {
            Lang::En => status.label(),
            Lang::Ru => match status {
                HealthStatus::NotEvaluated => "Не оценено",
                HealthStatus::InsufficientData => "Недостаточно данных",
                HealthStatus::NoAlert => "Нет срабатываний",
                HealthStatus::Attention => "Требует внимания",
                HealthStatus::Critical => "Срочная помощь",
            },
        }
    }

    /// Short rule result for tags, tiles and the figure. Alerts, not organ diagnoses.
    pub fn short(self, status: HealthStatus) -> &'static str {
        match status {
            HealthStatus::NoAlert => self.t("No alerts", "Нет сигналов"),
            HealthStatus::Attention => self.t("Attention", "Внимание"),
            HealthStatus::Critical => self.t("Urgent", "Срочно"),
            HealthStatus::InsufficientData | HealthStatus::NotEvaluated => self.t("Not assessed", "Не оценено"),
        }
    }

    pub fn escalation(self, escalation: Escalation) -> &'static str {
        match self {
            Lang::En => escalation.label(),
            Lang::Ru => match escalation {
                Escalation::None => "Плановое наблюдение",
                Escalation::SelfCare => "Самопроверка",
                Escalation::Crew => "Поддержка экипажа",
                Escalation::Ground => "Консультация с медиком",
                Escalation::Urgent => "Экипаж и срочная медицинская помощь",
            },
        }
    }

    /// Data quality of a value ("fresh", "stale", ...).
    pub fn quality(self, word: &str) -> String {
        let (en, ru) = match word {
            "fresh" => ("fresh", "свежее"),
            "stale" => ("stale", "устарело"),
            "missing" => ("missing", "нет замера"),
            "unknownTime" => ("unknown time", "время неизвестно"),
            "clockSkew" => ("clock skew", "расхождение часов"),
            "No data loaded" => ("no data loaded", "данные не загружены"),
            "No measurements recorded" => ("no measurements", "нет измерений"),
            other => return other.to_string(),
        };
        self.t(en, ru).to_string()
    }

    /// Date and time of a UTC instant.
    pub fn date_time(self, milliseconds: i64) -> String {
        DateTime::<Utc>::from_timestamp_millis(milliseconds)
            .map(|time| time.format(self.t("%m-%d %H:%M UTC", "%d.%m %H:%M UTC")).to_string())
            .unwrap_or_else(|| self.t("Unknown time", "Время неизвестно").into())
    }

    pub fn day(self, milliseconds: i64) -> String {
        DateTime::<Utc>::from_timestamp_millis(milliseconds)
            .map(|time| time.format(self.t("%m-%d", "%d.%m")).to_string())
            .unwrap_or_default()
    }

    /// An RFC 3339 timestamp from the server, shortened; unparsable text is returned unchanged.
    pub fn timestamp(self, rfc3339: &str) -> String {
        DateTime::parse_from_rfc3339(rfc3339)
            .map(|time| self.date_time(time.with_timezone(&Utc).timestamp_millis()))
            .unwrap_or_else(|_| rfc3339.to_string())
    }

    /// Measurement provenance ("source: x; measured: t; received: t") without the leading quality
    /// word and the closing disclaimer, with readable times and translated keys.
    pub fn provenance(self, detail: &str) -> String {
        detail
            .split(';')
            .map(str::trim)
            .filter(|part| !part.is_empty() && *part != "Offline / cached" && *part != "data quality is separate from health guidance")
            .skip(1)
            .map(|part| match part.split_once(": ") {
                Some((key, value)) => {
                    let key = match key {
                        "source" => self.t("source", "источник"),
                        "measured" => self.t("measured", "измерено"),
                        "received" => self.t("received", "получено"),
                        "newest receipt" => self.t("newest receipt", "последнее получение"),
                        other => return format!("{other}: {value}"),
                    };
                    let value = match value {
                        "unknown (receipt time is not measurement time)" => {
                            self.t("unknown (receipt time is not measurement time)", "неизвестно (время получения не равно времени измерения)").to_string()
                        }
                        _ => self.timestamp(value),
                    };
                    format!("{key}: {value}")
                }
                None if part == "clock skew reported by server" => self.t("clock skew reported by server", "сервер сообщил о расхождении часов").to_string(),
                None => part.to_string(),
            })
            .collect::<Vec<_>>()
            .join("  \u{00B7}  ")
    }

    /// Fixed status messages of `ui-http` and the server link. Unknown text is returned unchanged.
    pub fn message(self, text: &str) -> String {
        if self == Lang::En {
            return text.to_string();
        }
        if let Some(count) = text.strip_suffix(RULES_SUFFIX) {
            return format!("Актуальных оценок правил: {count}. Отсутствие срабатываний не означает, что все органы здоровы.");
        }
        let mut out = text.to_string();
        if let Some(at) = out.find(NOTE_PREFIX) {
            if let Some(note) = series_note_ru(&out[at + NOTE_PREFIX.len()..]) {
                out = format!("{}{note}", &out[..at]);
            }
        }
        for (en, ru) in FRAGMENTS {
            out = out.replace(en, ru);
        }
        out
    }
}

const RULES_SUFFIX: &str = " current rule evaluations. No rule alerts does not mean that all organs are healthy.";
const NOTE_PREFIX: &str = "Last 7 days UTC; ";

/// "{agg} per interval; time basis: {basis}; {n} samples use receipt time because measurement time is unknown. Gaps mean no samples."
fn series_note_ru(rest: &str) -> Option<String> {
    let mut parts = rest.split("; ");
    let aggregation = parts.next()?.strip_suffix(" per interval")?;
    let basis = parts.next()?.strip_prefix("time basis: ")?;
    let received = parts.next()?.split(' ').next()?;
    let aggregation = match aggregation {
        "mean" => "среднее",
        "last" => "последнее",
        "sum" => "сумма",
        "max" => "максимум",
        "min" => "минимум",
        other => other,
    };
    let basis = match basis {
        "measuredAt" => "время измерения",
        "receivedAt" => "время получения",
        "no samples" => "нет замеров",
        other => other,
    };
    Some(format!("7 дней, UTC; {aggregation} за интервал; шкала: {basis}; по времени получения: {received}. Пропуск означает, что замеров не было."))
}

const FRAGMENTS: &[(&str, &str)] = &[
    ("Waiting for server health guidance", "Ожидается оценка сервера"),
    ("This server does not publish health rules. Measurements remain available.", "Сервер не публикует правила оценки. Измерения доступны."),
    ("Offline: previous evaluation is not current", "Нет связи: предыдущая оценка не актуальна"),
    ("Health guidance refresh failed: ", "Не удалось обновить оценку: "),
    ("Assessment profile differs from current measurements", "Профиль оценки не совпадает с текущими измерениями"),
    ("This client does not display this metric", "Этот клиент не показывает этот показатель"),
    ("No latest measurement to match", "Нет последнего измерения для сопоставления"),
    ("No measurement with a known time", "Нет измерения с известным временем"),
    ("Measurement changed; waiting for matching assessment", "Измерение изменилось; ожидается новая оценка"),
    ("Latest measurement is no longer eligible for evaluation", "Последнее измерение больше не подходит для оценки"),
    ("Evaluation expired; obtain a current measurement", "Срок оценки истёк; нужен новый замер"),
    ("Connection restored; shared server data refreshed", "Связь восстановлена; данные сервера обновлены"),
    ("Connection lost; displaying cached values. Writes are unavailable.", "Связь потеряна; показаны сохранённые значения. Запись недоступна."),
    ("Server unavailable. This value has not been sent or saved.", "Сервер недоступен. Значение не отправлено и не сохранено."),
    ("Client is busy; try again shortly", "Клиент занят; повторите чуть позже"),
    ("This measurement is not available on the connected server", "Этот показатель недоступен на подключённом сервере"),
    ("Check current server data before trying again.", "Перед повтором проверьте данные на сервере."),
    ("Save not confirmed: ", "Сохранение не подтверждено: "),
    ("Task change sent; waiting for confirmation.", "Изменение задачи отправлено; ждём подтверждения."),
    (" (not completed)", " (не выполнено)"),
    (" (completed)", " (выполнено)"),
    ("Task saved to server: ", "Задача сохранена на сервере: "),
    ("Saved to server: ", "Сохранено на сервере: "),
    ("Waiting for seven-day history", "Ожидается история за 7 дней"),
    ("History refresh failed: ", "Не удалось обновить историю: "),
    ("Offline / cached. ", "Нет связи, сохранённые данные. "),
    ("; showing cached data", "; показаны сохранённые данные"),
    ("; no data loaded", "; данные не загружены"),
    ("; history refresh: ", "; обновление истории: "),
    ("Connecting to ", "Подключение к "),
    ("Connected: ", "Подключено: "),
    ("Offline: ", "Нет связи: "),
    ("Invalid API response: ", "Некорректный ответ API: "),
    ("Cannot read API response: ", "Не удалось прочитать ответ API: "),
    ("Shared API and SQLite", "Общий API и SQLite"),
    ("Server health and measurements loaded", "Состояние сервера и измерения загружены"),
    ("HTTP client; storage is owned by the Rust server", "HTTP-клиент; данные хранит Rust-сервер"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_unchanged_and_russian_follows_the_react_terms() {
        assert_eq!(Lang::En.message("Connected: http://pi:8080"), "Connected: http://pi:8080");
        assert_eq!(Lang::Ru.message("Connected: http://pi:8080"), "Подключено: http://pi:8080");
        assert_eq!(Lang::Ru.message("Offline: previous evaluation is not current"), "Нет связи: предыдущая оценка не актуальна");
        assert_eq!(Lang::Ru.health(HealthStatus::Attention), "Требует внимания");
        assert_eq!(Lang::Ru.metric("sbp", "Systolic pressure"), "Систолическое давление");
        assert_eq!(Lang::Ru.unit("mmHg", "sbp"), "мм рт. ст.");
        assert_eq!(Lang::Ru.unit("score", "mood"), "из 5");
        assert_eq!(Lang::Ru.num(129.4), "129,4");
        assert_eq!(Lang::En.num(129.4), "129.4");
    }

    #[test]
    fn structured_messages_are_rebuilt_in_russian() {
        assert_eq!(
            Lang::Ru.message("5/12 current rule evaluations. No rule alerts does not mean that all organs are healthy."),
            "Актуальных оценок правил: 5/12. Отсутствие срабатываний не означает, что все органы здоровы."
        );
        let note = "Offline / cached. Last 7 days UTC; mean per interval; time basis: measuredAt; 0 samples use receipt time because measurement time is unknown. Gaps mean no samples.";
        assert_eq!(
            Lang::Ru.message(note),
            "Нет связи, сохранённые данные. 7 дней, UTC; среднее за интервал; шкала: время измерения; по времени получения: 0. Пропуск означает, что замеров не было."
        );
    }

    #[test]
    fn provenance_has_readable_times_in_both_languages() {
        let detail = "stale; source: simulator; measured: 2026-11-14T19:30:00.000Z; received: 2026-11-15T02:15:10.365060200+00:00; data quality is separate from health guidance";
        assert_eq!(Lang::En.provenance(detail), "source: simulator  \u{00B7}  measured: 11-14 19:30 UTC  \u{00B7}  received: 11-15 02:15 UTC");
        assert_eq!(Lang::Ru.provenance(detail), "источник: simulator  \u{00B7}  измерено: 14.11 19:30 UTC  \u{00B7}  получено: 15.11 02:15 UTC");
        assert_eq!(Lang::Ru.provenance("Offline / cached; fresh; source: manual-01"), "источник: manual-01");
    }
}
