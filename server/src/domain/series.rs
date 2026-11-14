//! Bounded UTC intervals and explicit gaps for measurement charts.
use super::{
    history::parse_boundary,
    measurements::ValidationError,
    metric_config::{definition_for, Aggregation},
};
use chrono::{DateTime, Datelike};

#[derive(Debug)]
pub(crate) struct SeriesQuery {
    pub metric: String,
    pub unit: &'static str,
    pub aggregation: Aggregation,
    pub from_ms: i64,
    pub to_ms: i64,
    pub interval_ms: i64,
    pub max_points: u32,
}

impl SeriesQuery {
    pub fn parse(
        metric: Option<String>,
        from: Option<&str>,
        to: Option<&str>,
        max_points: Option<&str>,
    ) -> Result<Self, ValidationError> {
        let metric = metric.ok_or(ValidationError {
            field: "metric",
            message: "metric is required",
        })?;
        let definition = definition_for(&metric).ok_or(ValidationError {
            field: "metric",
            message: "Unknown metric",
        })?;
        let from_ms = boundary(from, "from")?;
        let to_ms = boundary(to, "to")?;
        if from_ms >= to_ms {
            return Err(ValidationError {
                field: "to",
                message: "to must be later than from",
            });
        }
        let max_points = match max_points {
            None => 200,
            Some(value) => value
                .parse::<u32>()
                .ok()
                .filter(|value| (1..=500).contains(value))
                .ok_or(ValidationError {
                    field: "maxPoints",
                    message: "maxPoints must be an integer from 1 to 500",
                })?,
        };
        let interval_ms = (to_ms - from_ms - 1) / i64::from(max_points) + 1;
        Ok(Self {
            metric,
            unit: definition.unit,
            aggregation: definition.aggregation,
            from_ms,
            to_ms,
            interval_ms,
            max_points,
        })
    }

    pub fn bucket_count(&self) -> i64 {
        (self.to_ms - self.from_ms - 1) / self.interval_ms + 1
    }
}

fn boundary(value: Option<&str>, field: &'static str) -> Result<i64, ValidationError> {
    let value = value.ok_or(ValidationError {
        field,
        message: "Time boundary is required",
    })?;
    let milliseconds = parse_boundary(value, field)?;
    // Responses use ordinary four-digit RFC3339 UTC timestamps, including every bucket edge.
    if !DateTime::from_timestamp_millis(milliseconds)
        .is_some_and(|time| (1..=9999).contains(&time.year()))
    {
        return Err(ValidationError {
            field,
            message: "UTC boundary year must be from 0001 to 9999",
        });
    }
    Ok(milliseconds)
}

#[derive(Debug)]
pub(crate) struct BucketAggregate {
    pub index: i64,
    pub value: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub count: i64,
    pub measured_count: i64,
    pub received_count: i64,
}

#[derive(Debug)]
pub(crate) struct SeriesPoint {
    pub start_ms: i64,
    pub end_ms: i64,
    pub value: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub count: i64,
    pub measured_count: i64,
    pub received_count: i64,
}

pub(crate) fn time_basis(measured: bool, received: bool) -> Option<&'static str> {
    match (measured, received) {
        (true, false) => Some("measuredAt"),
        (false, true) => Some("receivedAt"),
        (true, true) => Some("mixed"),
        (false, false) => None,
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid series aggregate")]
pub(crate) struct InvalidSeriesData;

/// Materialize only the bounded interval grid; empty intervals carry no value.
pub(crate) fn fill_gaps(
    query: &SeriesQuery,
    buckets: Vec<BucketAggregate>,
) -> Result<Vec<SeriesPoint>, InvalidSeriesData> {
    let count = query.bucket_count();
    if count > i64::from(query.max_points) {
        return Err(InvalidSeriesData);
    }
    let mut previous = None;
    for bucket in &buckets {
        if bucket.index < 0
            || bucket.index >= count
            || previous.is_some_and(|index| index >= bucket.index)
            || bucket.count < 1
            || bucket.measured_count < 0
            || bucket.received_count < 0
            || bucket.measured_count.checked_add(bucket.received_count) != Some(bucket.count)
            || !bucket.value.is_finite()
            || bucket.min.is_some_and(|value| !value.is_finite())
            || bucket.max.is_some_and(|value| !value.is_finite())
        {
            return Err(InvalidSeriesData);
        }
        previous = Some(bucket.index);
    }
    let mut populated = buckets.into_iter().peekable();
    let mut points = Vec::with_capacity(count as usize);
    for index in 0..count {
        let start_ms = query.from_ms + index * query.interval_ms;
        let mut point = SeriesPoint {
            start_ms,
            end_ms: (start_ms + query.interval_ms).min(query.to_ms),
            value: None,
            min: None,
            max: None,
            count: 0,
            measured_count: 0,
            received_count: 0,
        };
        if let Some(bucket) = populated.next_if(|bucket| bucket.index == index) {
            point.value = Some(bucket.value);
            point.min = bucket.min;
            point.max = bucket.max;
            point.count = bucket.count;
            point.measured_count = bucket.measured_count;
            point.received_count = bucket.received_count;
        }
        points.push(point);
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_covers_unaligned_request_without_exceeding_point_limit() {
        let query = SeriesQuery::parse(
            Some("heartRate".into()),
            Some("1969-12-31T23:59:59.999Z"),
            Some("1970-01-01T00:00:00.004Z"),
            Some("2"),
        )
        .unwrap();
        assert_eq!(query.interval_ms, 3);
        let points = fill_gaps(&query, vec![]).unwrap();
        assert_eq!(
            points
                .iter()
                .map(|p| (p.start_ms, p.end_ms, p.value))
                .collect::<Vec<_>>(),
            [(-1, 2, None), (2, 4, None)]
        );
    }

    #[test]
    fn millisecond_range_has_one_point_even_with_larger_requested_limit() {
        let query = SeriesQuery::parse(
            Some("mood".into()),
            Some("2020-01-01T00:00:00Z"),
            Some("2020-01-01T00:00:00.001Z"),
            Some("500"),
        )
        .unwrap();
        assert_eq!(query.bucket_count(), 1);
        assert_eq!(query.aggregation, Aggregation::Last);
    }

    #[test]
    fn non_finite_aggregate_is_an_error_instead_of_a_fake_gap() {
        let query = SeriesQuery::parse(
            Some("heartRate".into()),
            Some("2020-01-01T00:00:00Z"),
            Some("2020-01-02T00:00:00Z"),
            Some("1"),
        )
        .unwrap();
        assert!(fill_gaps(
            &query,
            vec![BucketAggregate {
                index: 0,
                value: f64::INFINITY,
                min: Some(1.0),
                max: Some(1.0),
                count: 1,
                measured_count: 1,
                received_count: 0
            }]
        )
        .is_err());
    }
}
