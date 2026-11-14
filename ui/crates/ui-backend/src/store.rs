//! SQLite access for the interface: opening, migrations, mission clock, ingestion through the team backend's
//! domain rules and storage functions, and snapshots for the in-memory engine.

use crate::be::domain::measurements::NewMeasurement;
use crate::be::storage;
use crate::metrics;
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Row, SqlitePool,
};
use std::{collections::HashMap, path::Path, time::Duration};
use ui_model::{Message, Snapshot};

pub const DAY_MS: i64 = 86_400_000;

fn es(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// One connection only: writes are serialised by the worker, and `PRAGMA data_version` then reports
/// changes made by *other* processes (the other interfaces on the same database file) and nothing else.
pub async fn open_pool(path: &Path) -> Result<SqlitePool, String> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5))
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal);
    SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect_with(options)
        .await
        .map_err(es)
}

/// Several interfaces may start at once on a new file; the loser of the race retries.
pub async fn migrate(pool: &SqlitePool) -> Result<(), String> {
    let mut last = String::new();
    for attempt in 0..6u64 {
        match sqlx::migrate!("./migrations").run(pool).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                last = e.to_string();
                tokio::time::sleep(Duration::from_millis(120 * (attempt + 1))).await;
            }
        }
    }
    Err(last)
}

pub async fn meta_get(pool: &SqlitePool, key: &str) -> Result<Option<String>, String> {
    sqlx::query_scalar("SELECT value FROM meta WHERE key = ?").bind(key).fetch_optional(pool).await.map_err(es)
}
pub async fn meta_set(pool: &SqlitePool, key: &str, value: &str) -> Result<(), String> {
    sqlx::query("INSERT INTO meta(key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
        .bind(key)
        .bind(value)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(es)
}
pub async fn meta_init(pool: &SqlitePool, key: &str, value: &str) -> Result<(), String> {
    sqlx::query("INSERT OR IGNORE INTO meta(key, value) VALUES (?, ?)").bind(key).bind(value).execute(pool).await.map(|_| ()).map_err(es)
}
async fn meta_i64(pool: &SqlitePool, key: &str) -> Result<i64, String> {
    Ok(meta_get(pool, key).await?.and_then(|v| v.parse().ok()).unwrap_or(0))
}

/// Registers the roster and one manual-entry device per crew member (the team schema keys data by device).
pub async fn ensure_crew(pool: &SqlitePool, crew: &[(String, String, String)]) -> Result<(), String> {
    for (i, (id, name, title)) in crew.iter().enumerate() {
        sqlx::query("INSERT OR IGNORE INTO crew(id, name, title, sort) VALUES (?, ?, ?, ?)").bind(id).bind(name).bind(title).bind(i as i64).execute(pool).await.map_err(es)?;
        sqlx::query("INSERT OR IGNORE INTO devices(id, astronaut_id, source) VALUES (?, ?, 'manual')").bind(format!("ui-{id}")).bind(id).execute(pool).await.map_err(es)?;
    }
    Ok(())
}

pub async fn crew(pool: &SqlitePool) -> Result<Vec<(String, String, String)>, String> {
    sqlx::query("SELECT id, name, title FROM crew ORDER BY sort, id")
        .fetch_all(pool)
        .await
        .map_err(es)?
        .into_iter()
        .map(|r| Ok((r.try_get("id").map_err(es)?, r.try_get("name").map_err(es)?, r.try_get("title").map_err(es)?)))
        .collect()
}

/// Mission clock: day 1 starts at `mission_start_ms`; `clock_skew_ms` is the demo time travel ("next day").
pub async fn init_clock(pool: &SqlitePool, days_before_today: i64) -> Result<(), String> {
    let today = Utc::now().timestamp_millis().div_euclid(DAY_MS) * DAY_MS;
    meta_init(pool, "mission_start_ms", &(today - days_before_today * DAY_MS).to_string()).await?;
    meta_init(pool, "clock_skew_ms", "0").await?;
    meta_init(pool, "env", "iss").await?;
    meta_init(pool, "blackout", "0").await
}

pub async fn skew(pool: &SqlitePool) -> Result<i64, String> {
    meta_i64(pool, "clock_skew_ms").await
}

/// The moment the mission clock shows now: real UTC plus the demo skew.
pub fn virtual_now(skew_ms: i64) -> DateTime<Utc> {
    Utc::now() + chrono::Duration::milliseconds(skew_ms)
}

pub fn day_of(ms: i64, start_ms: i64) -> u32 {
    ((ms - start_ms).div_euclid(DAY_MS) + 1).clamp(1, 100_000) as u32
}

fn stamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Stores one reading through the team backend's validation and idempotent insert.
/// Returns whether a new row was created (false = the same message was already stored).
#[allow(clippy::too_many_arguments)]
pub async fn ingest(pool: &SqlitePool, crew: &str, session: &str, seq: i64, indicator: &str, value: f64, measured_at: DateTime<Utc>, now: DateTime<Utc>) -> Result<bool, String> {
    let metric = metrics::metric_of(indicator).ok_or_else(|| format!("indicator {indicator} has no storage metric"))?;
    let unit = metrics::unit(metric).ok_or_else(|| format!("metric {metric} has no unit"))?;
    let input = NewMeasurement {
        device_id: format!("ui-{crew}"),
        session_id: session.to_string(),
        sequence: seq,
        metric: metric.to_string(),
        value,
        unit: unit.to_string(),
        measured_at: Some(stamp(measured_at)),
    };
    input.validate(now).map_err(|e| format!("{}: {}", e.field, e.message))?;
    let device = storage::measurements::device(pool, &input.device_id).await.map_err(es)?.ok_or_else(|| format!("unknown device {}", input.device_id))?;
    let out = storage::measurements::insert_or_get(pool, &input, &device, &stamp(now)).await.map_err(es)?;
    if !out.created && !out.measurement.matches_payload(&input).map_err(es)? {
        return Err(format!("sequence conflict for {}/{}", input.session_id, input.sequence));
    }
    Ok(out.created)
}

/// Fills an empty database with the demo crew's history, through the same ingestion path as live data.
/// Deterministic keys make concurrent seeding by several interfaces harmless.
pub async fn seed(pool: &SqlitePool, rows: &[(String, String, u32, f64)]) -> Result<usize, String> {
    if meta_get(pool, "seeded").await?.is_some() {
        return Ok(0);
    }
    let start = meta_i64(pool, "mission_start_ms").await?;
    let now = Utc::now();
    let mut n = 0;
    for (i, (crew, ind, day, value)) in rows.iter().enumerate() {
        let ms = start + (*day as i64 - 1) * DAY_MS + 8 * 3_600_000;
        let at = Utc.timestamp_millis_opt(ms).single().ok_or("bad timestamp")?;
        if ingest(pool, crew, "seed-v1", i as i64, ind, *value, at, now).await? {
            n += 1;
        }
    }
    meta_init(pool, "seeded", "1").await?;
    Ok(n)
}

pub async fn ack(pool: &SqlitePool, crew: &str, indicator: &str, now: DateTime<Utc>) -> Result<(), String> {
    sqlx::query("INSERT OR IGNORE INTO alert_acks(crew_id, indicator, acked_at) VALUES (?, ?, ?)").bind(crew).bind(indicator).bind(stamp(now)).execute(pool).await.map(|_| ()).map_err(es)
}

pub async fn message(pool: &SqlitePool, crew: &str, from_ground: bool, text: &str, day: u32, now: DateTime<Utc>) -> Result<(), String> {
    sqlx::query("INSERT INTO messages(crew_id, from_ground, text, day, created_at) VALUES (?, ?, ?, ?, ?)").bind(crew).bind(from_ground as i64).bind(text).bind(day as i64).bind(stamp(now)).execute(pool).await.map(|_| ()).map_err(es)
}

pub async fn wipe(pool: &SqlitePool) -> Result<(), String> {
    for q in [
        "DELETE FROM measurements",
        "DELETE FROM alert_acks",
        "DELETE FROM messages",
        "DELETE FROM meta",
    ] {
        sqlx::query(q).execute(pool).await.map_err(es)?;
    }
    Ok(())
}

/// Everything the engine needs, read in one pass: latest value per crew member, indicator and mission day.
pub async fn snapshot(pool: &SqlitePool) -> Result<Snapshot, String> {
    let start = meta_i64(pool, "mission_start_ms").await?;
    let skew_ms = skew(pool).await?;
    let mut latest: HashMap<(String, &'static str, u32), f64> = HashMap::new();
    let rows = sqlx::query("SELECT astronaut_id, metric, value, COALESCE(measured_at_ms, received_at_ms) AS t FROM measurements ORDER BY t, id").fetch_all(pool).await.map_err(es)?;
    for r in rows {
        let metric: String = r.try_get("metric").map_err(es)?;
        let Some(ind) = metrics::indicator_of(&metric) else { continue };
        let crew: String = r.try_get("astronaut_id").map_err(es)?;
        let t: i64 = r.try_get("t").map_err(es)?;
        let v: f64 = r.try_get("value").map_err(es)?;
        latest.insert((crew, ind, day_of(t, start)), v); // ascending time: the last write per day wins
    }
    let mut readings: Vec<(String, String, u32, f64)> = latest.into_iter().map(|((c, i, d), v)| (c, i.to_string(), d, v)).collect();
    readings.sort_by(|a, b| a.2.cmp(&b.2).then(a.0.cmp(&b.0)).then(a.1.cmp(&b.1)));
    let acks = sqlx::query("SELECT crew_id, indicator FROM alert_acks").fetch_all(pool).await.map_err(es)?.into_iter().map(|r| (r.get::<String, _>("crew_id"), r.get::<String, _>("indicator"))).collect();
    let messages = sqlx::query("SELECT crew_id, from_ground, text, day FROM messages ORDER BY id")
        .fetch_all(pool)
        .await
        .map_err(es)?
        .into_iter()
        .map(|r| (r.get::<String, _>("crew_id"), Message { from_ground: r.get::<i64, _>("from_ground") != 0, text: r.get("text"), day: r.get::<i64, _>("day") as u32 }))
        .collect();
    Ok(Snapshot {
        day: day_of(virtual_now(skew_ms).timestamp_millis(), start),
        env_id: meta_get(pool, "env").await?.unwrap_or_else(|| "iss".into()),
        blackout: meta_get(pool, "blackout").await?.as_deref() == Some("1"),
        readings,
        acks,
        messages,
    })
}

pub async fn data_version(pool: &SqlitePool) -> Result<i64, String> {
    sqlx::query_scalar("PRAGMA data_version").fetch_one(pool).await.map_err(es)
}

pub async fn quick_check(pool: &SqlitePool) -> Result<bool, String> {
    let r: String = sqlx::query_scalar("PRAGMA quick_check").fetch_one(pool).await.map_err(es)?;
    Ok(r == "ok")
}
