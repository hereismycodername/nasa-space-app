use super::measurements::Measurement;
use chrono::{DateTime, Utc};

#[derive(Debug)]
pub(crate) struct Assessment {
    pub status: &'static str,
    pub explanation: &'static str,
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid stored timestamp")]
pub(crate) struct InvalidStoredTimestamp;

/// Reception proves delivery, not measurement freshness. Per-metric policy is BE-06.
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
    let Some(measured_at) = &row.measured_at else {
        return Ok(Assessment {
            status: "unknown-time",
            explanation:
                "Время измерения неизвестно. Время получения не подтверждает свежесть показателя.",
        });
    };
    let time = DateTime::parse_from_rfc3339(measured_at).map_err(|_| InvalidStoredTimestamp)?;
    if now.signed_duration_since(time).num_seconds() > 15 {
        Ok(Assessment { status: "stale", explanation: "Последнее измерение старше 15 секунд. Это демонстрационный порог свежести, не медицинская оценка." })
    } else {
        Ok(Assessment {
            status: "not-evaluated",
            explanation: "Данные поступают. Медицинские правила пока не подключены.",
        })
    }
}
