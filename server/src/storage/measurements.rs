use crate::domain::measurements::{
    Device, HistoryFilter, LatestMetric, Measurement, MeasurementPage, NewMeasurement, METRICS,
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

/// Order the display timeline by measurement time, or explicitly unknown-time receipt.
pub(crate) async fn history(
    pool: &SqlitePool,
    filter: &HistoryFilter,
) -> Result<MeasurementPage, sqlx::Error> {
    let mut query = QueryBuilder::<Sqlite>::new("SELECT * FROM measurements WHERE astronaut_id = ");
    query.push_bind(&filter.astronaut_id);
    if let Some(metric) = &filter.metric {
        query.push(" AND metric = ").push_bind(metric);
    }
    if let Some(from) = filter.from_ms {
        query
            .push(" AND COALESCE(measured_at_ms, received_at_ms) >= ")
            .push_bind(from);
    }
    if let Some(to) = filter.to_ms {
        query
            .push(" AND COALESCE(measured_at_ms, received_at_ms) < ")
            .push_bind(to);
    }
    query
        .push(" ORDER BY COALESCE(measured_at_ms, received_at_ms) DESC, id DESC LIMIT ")
        .push_bind((filter.limit + 1) as i64);
    let mut items = query
        .build()
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(decode)
        .collect::<Result<Vec<_>, _>>()?;
    let has_more = items.len() > filter.limit;
    items.truncate(filter.limit);
    Ok(MeasurementPage { items, has_more })
}

/// Each scalar lookup uses its metric/time index; one statement gives a consistent snapshot.
pub(crate) async fn latest_per_metric(
    pool: &SqlitePool,
    astronaut_id: &str,
) -> Result<Vec<LatestMetric>, sqlx::Error> {
    let mut query = QueryBuilder::<Sqlite>::new("WITH metrics(metric) AS (");
    query.push_values(&METRICS, |mut row, metric| {
        row.push_bind(metric.metric);
    });
    query.push(
        ")
        SELECT m.* FROM metrics JOIN measurements m ON m.id = (
            SELECT id FROM measurements WHERE astronaut_id = ",
    );
    query.push_bind(astronaut_id).push(
        " AND metric = metrics.metric
                AND measured_at IS NOT NULL ORDER BY measured_at_ms DESC, id DESC LIMIT 1
        )
        UNION ALL
        SELECT m.* FROM metrics JOIN measurements m ON m.id = (
            SELECT id FROM measurements WHERE astronaut_id = ",
    );
    query.push_bind(astronaut_id).push(
        " AND metric = metrics.metric
                ORDER BY received_at_ms DESC, id DESC LIMIT 1
        ) WHERE m.measured_at IS NULL",
    );
    let rows = query.build().fetch_all(pool).await?;
    let mut metrics: Vec<_> = METRICS
        .into_iter()
        .map(|metric| LatestMetric {
            metric: metric.metric,
            unit: metric.unit,
            latest_known_measurement: None,
            latest_unknown_time_measurement: None,
        })
        .collect();
    for row in rows {
        let measurement = decode(row)?;
        if let Some(metric) = metrics
            .iter_mut()
            .find(|metric| metric.metric == measurement.metric)
        {
            if measurement.measured_at.is_some() {
                metric.latest_known_measurement = Some(measurement);
            } else {
                metric.latest_unknown_time_measurement = Some(measurement);
            }
        }
    }
    Ok(metrics)
}

pub(crate) async fn latest_received(pool: &SqlitePool) -> Result<Option<Measurement>, sqlx::Error> {
    sqlx::query("SELECT * FROM measurements ORDER BY received_at_ms DESC, id DESC LIMIT 1")
        .fetch_optional(pool)
        .await?
        .map(decode)
        .transpose()
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
