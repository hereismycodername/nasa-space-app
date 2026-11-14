use crate::domain::{
    history::{HistoryCursor, HistoryPage, HistoryQuery, LatestMetric},
    measurements::{Device, Measurement, NewMeasurement},
    metric_config::METRICS,
};
use sqlx::{sqlite::SqliteRow, QueryBuilder, Row, Sqlite, SqlitePool};
use uuid::Uuid;

fn decode(row: SqliteRow) -> Result<Measurement, sqlx::Error> {
    Ok(Measurement {
        id: row.try_get("id")?,
        device_id: row.try_get("device_id")?,
        astronaut_id: row.try_get("astronaut_id")?,
        session_id: row.try_get("session_id")?,
        sequence: row.try_get("sequence")?,
        metric: row.try_get("metric")?,
        value: row.try_get("value")?,
        unit: row.try_get("unit")?,
        measured_at: row.try_get("measured_at")?,
        received_at: row.try_get("received_at")?,
        source: row.try_get("source")?,
    })
}

/// Read one bounded page using a validated query and an explicit astronaut scope.
pub(crate) async fn history(
    pool: &SqlitePool,
    astronaut_id: &str,
    query: &HistoryQuery,
) -> Result<HistoryPage, sqlx::Error> {
    let mut statement = QueryBuilder::<Sqlite>::new(
        "SELECT *, COALESCE(measured_at_ms, received_at_ms) AS effective_at_ms
         FROM measurements WHERE astronaut_id = ",
    );
    statement.push_bind(astronaut_id);
    if let Some(metric) = &query.metric {
        statement.push(" AND metric = ").push_bind(metric);
    }
    if let Some(from) = query.from_ms {
        statement
            .push(" AND COALESCE(measured_at_ms, received_at_ms) >= ")
            .push_bind(from);
    }
    if let Some(to) = query.to_ms {
        statement
            .push(" AND COALESCE(measured_at_ms, received_at_ms) < ")
            .push_bind(to);
    }
    if let Some(cursor) = &query.cursor {
        statement
            .push(" AND (COALESCE(measured_at_ms, received_at_ms), id) < (")
            .push_bind(cursor.time_ms)
            .push(", ")
            .push_bind(&cursor.id)
            .push(")");
    }
    statement
        .push(" ORDER BY COALESCE(measured_at_ms, received_at_ms) DESC, id DESC LIMIT ")
        .push_bind(i64::from(query.limit) + 1);
    let mut rows = statement.build().fetch_all(pool).await?;
    let limit = query.limit as usize;
    let has_more = rows.len() > limit;
    rows.truncate(limit);
    let next_cursor = if has_more {
        rows.last()
            .map(|row| {
                Ok::<_, sqlx::Error>(HistoryCursor {
                    time_ms: row.try_get("effective_at_ms")?,
                    id: row.try_get("id")?,
                })
            })
            .transpose()?
    } else {
        None
    };
    let items = rows.into_iter().map(decode).collect::<Result<_, _>>()?;
    Ok(HistoryPage {
        items,
        has_more,
        next_cursor,
    })
}

/// Read all three latest views in one SQLite statement and therefore one snapshot.
/// Each correlated lookup uses the existing metric/time index, without scanning history.
pub(crate) async fn latest_by_metric(
    pool: &SqlitePool,
    astronaut_id: &str,
) -> Result<Vec<LatestMetric>, sqlx::Error> {
    let mut query = QueryBuilder::<Sqlite>::new("WITH metrics(metric) AS (");
    query.push_values(METRICS, |mut row, definition| {
        row.push_bind(definition.metric);
    });
    query
        .push(
            "), latest(selection, id) AS (
         SELECT 'effective', (SELECT id FROM measurements WHERE astronaut_id = ",
        )
        .push_bind(astronaut_id)
        .push(
            " AND metric = metrics.metric
          ORDER BY COALESCE(measured_at_ms, received_at_ms) DESC, id DESC LIMIT 1)
         FROM metrics
         UNION ALL
         SELECT 'known', (SELECT id FROM measurements WHERE astronaut_id = ",
        )
        .push_bind(astronaut_id)
        .push(
            " AND metric = metrics.metric AND measured_at IS NOT NULL
          ORDER BY measured_at_ms DESC, id DESC LIMIT 1)
         FROM metrics
         UNION ALL
         SELECT 'received', (SELECT id FROM measurements WHERE astronaut_id = ",
        )
        .push_bind(astronaut_id)
        .push(
            " AND metric = metrics.metric
          ORDER BY received_at_ms DESC, id DESC LIMIT 1)
         FROM metrics)
         SELECT latest.selection, m.* FROM latest JOIN measurements m ON m.id = latest.id",
        );
    let rows = query.build().fetch_all(pool).await?;
    let mut metrics: Vec<_> = METRICS
        .into_iter()
        .map(|definition| LatestMetric {
            metric: definition.metric,
            latest_measurement: None,
            latest_known_measurement: None,
            latest_received_measurement: None,
        })
        .collect();
    for row in rows {
        let selection: String = row.try_get("selection")?;
        let measurement = decode(row)?;
        let metric = metrics
            .iter_mut()
            .find(|entry| entry.metric == measurement.metric)
            .ok_or_else(|| sqlx::Error::Decode("Unknown latest metric".into()))?;
        match selection.as_str() {
            "effective" => metric.latest_measurement = Some(measurement),
            "known" => metric.latest_known_measurement = Some(measurement),
            "received" => metric.latest_received_measurement = Some(measurement),
            _ => return Err(sqlx::Error::Decode("Unknown latest selection".into())),
        }
    }
    Ok(metrics)
}

pub(crate) async fn device(
    pool: &SqlitePool,
    device_id: &str,
) -> Result<Option<Device>, sqlx::Error> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT astronaut_id, source FROM devices WHERE id = ?")
            .bind(device_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(astronaut_id, source)| Device {
        astronaut_id,
        source,
    }))
}

pub(crate) struct InsertOutcome {
    pub measurement: Measurement,
    pub created: bool,
}

/// Atomically insert and read the row, or read an existing message without changing it.
/// The caller checks whether the validated payload matches the stored record.
pub(crate) async fn insert_or_get(
    pool: &SqlitePool,
    input: &NewMeasurement,
    device: &Device,
    received_at: &str,
) -> Result<InsertOutcome, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    // The first statement writes, avoiding a read-to-write lock upgrade under
    // concurrent requests. The unique key arbitrates the winning insertion.
    let result = sqlx::query("INSERT INTO measurements (id,device_id,astronaut_id,session_id,sequence,metric,value,unit,measured_at,received_at,source) VALUES (?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(device_id,session_id,sequence) DO NOTHING")
        .bind(Uuid::new_v4().to_string()).bind(&input.device_id).bind(&device.astronaut_id)
        .bind(&input.session_id).bind(input.sequence).bind(&input.metric).bind(input.value)
        .bind(&input.unit).bind(&input.measured_at).bind(received_at).bind(&device.source).execute(&mut *transaction).await?;
    let row =
        sqlx::query("SELECT * FROM measurements WHERE device_id=? AND session_id=? AND sequence=?")
            .bind(&input.device_id)
            .bind(&input.session_id)
            .bind(input.sequence)
            .fetch_one(&mut *transaction)
            .await?;
    let measurement = decode(row)?;
    transaction.commit().await?;
    Ok(InsertOutcome {
        measurement,
        created: result.rows_affected() == 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn measurement_queries_isolate_the_requested_astronaut() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        sqlx::query("INSERT INTO devices VALUES ('other-device', 'other-astronaut', 'sensor')")
            .execute(&pool)
            .await
            .unwrap();
        for (id, device, astronaut, time, source) in [
            (
                "demo",
                "simulator-01",
                "demo-astronaut",
                "2020-01-01T00:00:00Z",
                "simulator",
            ),
            (
                "other",
                "other-device",
                "other-astronaut",
                "2020-01-02T00:00:00Z",
                "sensor",
            ),
        ] {
            sqlx::query("INSERT INTO measurements(id,device_id,astronaut_id,session_id,sequence,metric,value,unit,measured_at,received_at,source) VALUES (?,?,?,'scope-test',0,'heartRate',72,'bpm',?,?,?)")
                .bind(id).bind(device).bind(astronaut).bind(time).bind(time).bind(source)
                .execute(&pool).await.unwrap();
        }
        let query = HistoryQuery {
            metric: Some("heartRate".into()),
            from_ms: None,
            to_ms: None,
            limit: 100,
            cursor: None,
        };
        for (astronaut, expected_id) in [("demo-astronaut", "demo"), ("other-astronaut", "other")] {
            let page = history(&pool, astronaut, &query).await.unwrap();
            let ids: Vec<_> = page.items.iter().map(|row| row.id.as_str()).collect();
            assert_eq!(ids, [expected_id]);
            assert!(!page.has_more);
            assert!(page.next_cursor.is_none());
            let metrics = latest_by_metric(&pool, astronaut).await.unwrap();
            let pulse = metrics
                .iter()
                .find(|row| row.metric == "heartRate")
                .unwrap();
            assert_eq!(pulse.latest_measurement.as_ref().unwrap().id, expected_id);
            assert_eq!(
                pulse.latest_known_measurement.as_ref().unwrap().id,
                expected_id
            );
            assert_eq!(
                pulse.latest_received_measurement.as_ref().unwrap().id,
                expected_id
            );
        }
    }
}
