// Taken from the team backend (branch codex/team-integration, commit 2c3d1db, server/src/storage/series.rs) unless noted below.
//! Aggregate the full requested range in SQLite without loading raw rows into Rust.
use crate::be::domain::{
    metric_config::Aggregation,
    series::{BucketAggregate, SeriesQuery},
};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

const MEAN_SQL: &str = "
    WITH selected AS (
        SELECT value, measured_at IS NOT NULL AS known_time,
               (COALESCE(measured_at_ms, received_at_ms) - ?) / ? AS bucket_index
        FROM measurements
        WHERE astronaut_id = ? AND metric = ?
          AND COALESCE(measured_at_ms, received_at_ms) >= ?
          AND COALESCE(measured_at_ms, received_at_ms) < ?
    ), stats AS (
        SELECT bucket_index, MIN(value) AS minimum, MAX(value) AS maximum,
               COUNT(*) AS sample_count, SUM(known_time) AS measured_count,
               SUM(1 - known_time) AS received_count
        FROM selected GROUP BY bucket_index
    ), means AS (
        SELECT selected.bucket_index,
               MIN(1.0, MAX(0.0, AVG(CASE WHEN stats.maximum = 0 THEN 0.0
                   ELSE selected.value / stats.maximum END))) * stats.maximum AS value
        FROM selected JOIN stats USING (bucket_index)
        GROUP BY selected.bucket_index
    )
    SELECT stats.bucket_index, means.value, stats.minimum, stats.maximum,
           stats.sample_count, stats.measured_count, stats.received_count
    FROM stats JOIN means USING (bucket_index)
    ORDER BY stats.bucket_index";

const LAST_SQL: &str = "
    WITH selected AS (
        SELECT id, value, measured_at IS NOT NULL AS known_time,
               COALESCE(measured_at_ms, received_at_ms) AS effective_at_ms,
               (COALESCE(measured_at_ms, received_at_ms) - ?) / ? AS bucket_index
        FROM measurements
        WHERE astronaut_id = ? AND metric = ?
          AND COALESCE(measured_at_ms, received_at_ms) >= ?
          AND COALESCE(measured_at_ms, received_at_ms) < ?
    ), ranked AS (
        SELECT *, ROW_NUMBER() OVER (
            PARTITION BY bucket_index ORDER BY effective_at_ms DESC, id DESC
        ) AS position
        FROM selected
    )
    SELECT bucket_index, MAX(CASE WHEN position = 1 THEN value END) AS value,
           NULL AS minimum, NULL AS maximum, COUNT(*) AS sample_count,
           SUM(known_time) AS measured_count, SUM(1 - known_time) AS received_count
    FROM ranked GROUP BY bucket_index ORDER BY bucket_index";

fn decode(row: SqliteRow) -> Result<BucketAggregate, sqlx::Error> {
    let value: f64 = row.try_get("value")?;
    let min: Option<f64> = row.try_get("minimum")?;
    let max: Option<f64> = row.try_get("maximum")?;
    if !value.is_finite()
        || min.is_some_and(|value| !value.is_finite())
        || max.is_some_and(|value| !value.is_finite())
    {
        return Err(sqlx::Error::Decode(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Non-finite series aggregate",
        ))));
    }
    Ok(BucketAggregate {
        index: row.try_get("bucket_index")?,
        value,
        min,
        max,
        count: row.try_get("sample_count")?,
        measured_count: row.try_get("measured_count")?,
        received_count: row.try_get("received_count")?,
    })
}

/// Return populated buckets from one SELECT snapshot; the domain layer supplies gaps.
///
/// Validated input bounds make every bucket index nonnegative. Accepted measurement
/// values are nonnegative, so scaling by the bucket maximum keeps AVG's running sum
/// finite even when individually valid readings approach f64::MAX.
pub(crate) async fn aggregate(
    pool: &SqlitePool,
    astronaut_id: &str,
    query: &SeriesQuery,
) -> Result<Vec<BucketAggregate>, sqlx::Error> {
    let statement = match &query.aggregation {
        Aggregation::Mean => MEAN_SQL,
        Aggregation::Last => LAST_SQL,
    };
    sqlx::query(statement)
        .bind(query.from_ms)
        .bind(query.interval_ms)
        .bind(astronaut_id)
        .bind(&query.metric)
        .bind(query.from_ms)
        .bind(query.to_ms)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(decode)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn database() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        pool
    }

    async fn insert(
        pool: &SqlitePool,
        id: &str,
        metric: &str,
        value: f64,
        measured_at: Option<&str>,
        received_at: &str,
    ) {
        sqlx::query(
            "INSERT INTO measurements
             (id, device_id, astronaut_id, session_id, sequence, metric, value,
              unit, measured_at, received_at, source)
             VALUES (?, 'manual-01', 'demo-astronaut', ?, 0, ?, ?, ?, ?, ?, 'manual')",
        )
        .bind(id)
        .bind(id)
        .bind(metric)
        .bind(value)
        .bind(if metric == "mood" { "score" } else { "bpm" })
        .bind(measured_at)
        .bind(received_at)
        .execute(pool)
        .await
        .unwrap();
    }

    fn one_bucket(metric: &str) -> SeriesQuery {
        SeriesQuery::parse(
            Some(metric.into()),
            Some("1970-01-01T00:00:00Z"),
            Some("1970-01-01T00:00:10Z"),
            Some("1"),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn mean_remains_finite_when_raw_sum_would_overflow() {
        let pool = database().await;
        for (id, value) in [("a", f64::MAX), ("b", f64::MAX), ("c", f64::MAX / 2.0)] {
            insert(
                &pool,
                id,
                "heartRate",
                value,
                Some("1970-01-01T00:00:01Z"),
                "1970-01-01T00:00:02Z",
            )
            .await;
        }

        let buckets = aggregate(&pool, "demo-astronaut", &one_bucket("heartRate"))
            .await
            .unwrap();

        assert_eq!(buckets.len(), 1);
        let bucket = &buckets[0];
        assert_eq!(bucket.index, 0);
        assert!(bucket.value.is_finite());
        assert!((bucket.value / f64::MAX - 5.0 / 6.0).abs() < f64::EPSILON);
        assert_eq!(bucket.min, Some(f64::MAX / 2.0));
        assert_eq!(bucket.max, Some(f64::MAX));
        assert_eq!(bucket.count, 3);
        assert_eq!((bucket.measured_count, bucket.received_count), (3, 0));
    }

    #[tokio::test]
    async fn last_uses_effective_time_then_id_and_counts_every_sample() {
        let pool = database().await;
        for (id, value, measured_at, received_at) in [
            (
                "a",
                1.0,
                Some("1970-01-01T00:00:01Z"),
                "1970-01-01T00:00:01Z",
            ),
            (
                "z",
                3.0,
                Some("1970-01-01T00:00:02Z"),
                "1970-01-01T00:00:01Z",
            ),
            ("y", 4.0, None, "1970-01-01T00:00:02Z"),
            (
                "zz",
                5.0,
                Some("1970-01-01T00:00:00Z"),
                "1970-01-01T00:00:09Z",
            ),
        ] {
            insert(&pool, id, "mood", value, measured_at, received_at).await;
        }

        let buckets = aggregate(&pool, "demo-astronaut", &one_bucket("mood"))
            .await
            .unwrap();

        assert_eq!(buckets.len(), 1);
        let bucket = &buckets[0];
        assert_eq!(bucket.value, 3.0);
        assert_eq!(bucket.min, None);
        assert_eq!(bucket.max, None);
        assert_eq!(bucket.count, 4);
        assert_eq!((bucket.measured_count, bucket.received_count), (3, 1));
    }
}
