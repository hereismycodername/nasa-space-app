//! Validated history filters and separate views of the latest measurement.
use super::measurements::{Measurement, ValidationError, METRICS};
use chrono::DateTime;

pub(crate) const DEMO_ASTRONAUT_ID: &str = "demo-astronaut";

#[derive(Debug)]
pub(crate) struct HistoryCursor {
    pub time_ms: i64,
    pub id: String,
}

#[derive(Debug)]
pub(crate) struct HistoryQuery {
    pub metric: Option<String>,
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub limit: u32,
    pub cursor: Option<HistoryCursor>,
}

impl HistoryQuery {
    pub fn parse(
        metric: Option<String>,
        from: Option<&str>,
        to: Option<&str>,
        limit: Option<&str>,
        cursor: Option<HistoryCursor>,
    ) -> Result<Self, ValidationError> {
        if metric
            .as_deref()
            .is_some_and(|metric| !METRICS.contains(&metric))
        {
            return Err(ValidationError {
                field: "metric",
                message: "Unknown metric",
            });
        }
        let from_ms = from.map(|time| parse_boundary(time, "from")).transpose()?;
        let to_ms = to.map(|time| parse_boundary(time, "to")).transpose()?;
        if matches!((from_ms, to_ms), (Some(from), Some(to)) if from >= to) {
            return Err(ValidationError {
                field: "to",
                message: "to must be later than from",
            });
        }
        let limit = match limit {
            None => 100,
            Some(value) => value
                .parse::<u32>()
                .ok()
                .filter(|value| (1..=500).contains(value))
                .ok_or(ValidationError {
                    field: "limit",
                    message: "limit must be an integer from 1 to 500",
                })?,
        };
        Ok(Self {
            metric,
            from_ms,
            to_ms,
            limit,
            cursor,
        })
    }
}

pub(super) fn parse_boundary(value: &str, field: &'static str) -> Result<i64, ValidationError> {
    let time = DateTime::parse_from_rfc3339(value).map_err(|_| ValidationError {
        field,
        message: "Expected an RFC3339 timestamp with timezone",
    })?;
    // Storage compares whole milliseconds. Reject finer boundaries instead of
    // silently moving an inclusive/exclusive edge to a different instant.
    let fraction = value
        .get(20..)
        .filter(|_| value.as_bytes().get(19) == Some(&b'.'))
        .unwrap_or("");
    if fraction
        .bytes()
        .take_while(u8::is_ascii_digit)
        .skip(3)
        .any(|digit| digit != b'0')
    {
        return Err(ValidationError {
            field,
            message: "Time boundaries must align to whole milliseconds",
        });
    }
    Ok(time.timestamp_millis())
}

pub(crate) struct HistoryPage {
    pub items: Vec<Measurement>,
    pub has_more: bool,
    pub next_cursor: Option<HistoryCursor>,
}

pub(crate) struct LatestMetric {
    pub metric: &'static str,
    pub latest_measurement: Option<Measurement>,
    pub latest_known_measurement: Option<Measurement>,
    pub latest_received_measurement: Option<Measurement>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equivalent_offset_boundaries_produce_the_same_utc_range() {
        let query = HistoryQuery::parse(
            None,
            Some("2020-01-01T05:00:00.001+05:00"),
            Some("2020-01-01T00:00:00.002Z"),
            None,
            None,
        )
        .unwrap();
        assert_eq!(query.to_ms.unwrap() - query.from_ms.unwrap(), 1);
        assert_eq!(query.limit, 100);
    }

    #[test]
    fn rejects_sub_millisecond_and_empty_ranges() {
        assert_eq!(
            HistoryQuery::parse(
                None,
                Some("2020-01-01T00:00:00.0000000001Z"),
                None,
                None,
                None
            )
            .unwrap_err()
            .field,
            "from"
        );
        assert_eq!(
            HistoryQuery::parse(None, Some("2020-01-01T00:00:00.000001Z"), None, None, None)
                .unwrap_err()
                .field,
            "from"
        );
        assert_eq!(
            HistoryQuery::parse(
                None,
                Some("2020-01-01T05:00:00+05:00"),
                Some("2020-01-01T00:00:00Z"),
                None,
                None
            )
            .unwrap_err()
            .field,
            "to"
        );
    }
}
