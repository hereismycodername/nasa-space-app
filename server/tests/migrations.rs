use chrono::DateTime;
use sqlx::{migrate::Migrate, sqlite::SqlitePoolOptions, Row, SqlitePool};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

async fn legacy_database() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let mut connection = pool.acquire().await.unwrap();
    connection.ensure_migrations_table().await.unwrap();
    // Apply the actual 0001 migration, including its checksum and migration record.
    connection
        .apply(
            MIGRATOR
                .iter()
                .find(|migration| migration.version == 1)
                .unwrap(),
        )
        .await
        .unwrap();
    drop(connection);
    pool
}

async fn insert_measurement(
    pool: &SqlitePool,
    id: &str,
    sequence: i64,
    measured_at: Option<&str>,
    received_at: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO measurements(id,device_id,astronaut_id,session_id,sequence,metric,value,unit,measured_at,received_at,source) VALUES (?,'simulator-01','demo-astronaut','migration-test',?,'heartRate',72,'bpm',?,?,'simulator')")
        .bind(id)
        .bind(sequence)
        .bind(measured_at)
        .bind(received_at)
        .execute(pool)
        .await?;
    Ok(())
}

#[tokio::test]
async fn empty_database_migrates_and_seeds_only_task_templates() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    let counts: (i64, i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM task_templates),(SELECT count(*) FROM task_completions),(SELECT count(*) FROM task_completion_migration_issues),(SELECT count(*) FROM measurements)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 0, 0, 0));
    MIGRATOR.run(&pool).await.unwrap();
    let versions: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(versions, vec![1, 2, 3]);
}

#[tokio::test]
async fn migration_and_new_writes_match_chrono_milliseconds_without_rewriting_timestamps() {
    let pool = legacy_database().await;
    let timestamps = [
        "2026-11-14T08:00:00.123456789Z",
        "2026-11-14T13:00:00.123456789+05:00",
        "2026-11-14t08:00:00.1z",
        "2026-11-14 08:00:00.999999999999Z",
        "2026-11-14T10:00:00.123+23:59",
        "2026-11-14T10:00:00-23:59",
        "1969-12-31T19:00:00−05:00",
        "1969-12-31T23:59:59.999999Z",
        "2016-12-31T23:59:60.999999999Z",
        "0000-01-01T00:00:00+23:59",
    ];
    for (sequence, timestamp) in timestamps.iter().enumerate() {
        insert_measurement(
            &pool,
            &format!("old-{sequence}"),
            sequence as i64,
            Some(timestamp),
            timestamp,
        )
        .await
        .unwrap();
    }
    insert_measurement(&pool, "unknown-time", 100, None, timestamps[0])
        .await
        .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    for (sequence, timestamp) in timestamps.iter().enumerate() {
        insert_measurement(
            &pool,
            &format!("new-{sequence}"),
            200 + sequence as i64,
            Some(timestamp),
            timestamp,
        )
        .await
        .unwrap();
        let expected = DateTime::parse_from_rfc3339(timestamp)
            .unwrap()
            .timestamp_millis();
        let rows: Vec<(String, String, i64, i64)> = sqlx::query_as("SELECT measured_at,received_at,measured_at_ms,received_at_ms FROM measurements WHERE id IN (?,?) ORDER BY id")
            .bind(format!("old-{sequence}")).bind(format!("new-{sequence}"))
            .fetch_all(&pool).await.unwrap();
        assert_eq!(
            rows,
            vec![
                (
                    timestamp.to_string(),
                    timestamp.to_string(),
                    expected,
                    expected
                );
                2
            ],
            "{timestamp}"
        );
    }
    let unknown: (Option<String>, Option<i64>, i64) = sqlx::query_as("SELECT measured_at,measured_at_ms,received_at_ms FROM measurements WHERE id='unknown-time'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(
        unknown,
        (
            None,
            None,
            DateTime::parse_from_rfc3339(timestamps[0])
                .unwrap()
                .timestamp_millis()
        )
    );
    let devices: i64 = sqlx::query_scalar("SELECT count(*) FROM devices")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(devices, 2);
    assert!(insert_measurement(
        &pool,
        "duplicate-message",
        0,
        Some(timestamps[0]),
        timestamps[0]
    )
    .await
    .is_err());
}

#[tokio::test]
async fn numeric_time_ranges_and_order_are_independent_of_offsets() {
    let pool = legacy_database().await;
    for (sequence, (id, time)) in [
        ("a", Some("2026-11-14T09:00:00+01:00")),
        ("b", Some("2026-11-14T07:00:00-01:00")),
        ("c", Some("2026-11-14T08:00:01Z")),
        ("d", None),
    ]
    .into_iter()
    .enumerate()
    {
        insert_measurement(&pool, id, sequence as i64, time, "2026-11-14T08:00:00Z")
            .await
            .unwrap();
    }
    MIGRATOR.run(&pool).await.unwrap();
    let start = DateTime::parse_from_rfc3339("2026-11-14T08:00:00Z")
        .unwrap()
        .timestamp_millis();
    let exact_second: Vec<String> = sqlx::query_scalar("SELECT id FROM measurements WHERE coalesce(measured_at_ms,received_at_ms)>=? AND coalesce(measured_at_ms,received_at_ms)<? ORDER BY coalesce(measured_at_ms,received_at_ms) DESC,id ASC")
        .bind(start).bind(start + 1000).fetch_all(&pool).await.unwrap();
    assert_eq!(exact_second, ["a", "b", "d"]);
    let ordered: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM measurements ORDER BY coalesce(measured_at_ms,received_at_ms) DESC,id ASC",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(ordered, ["c", "a", "b", "d"]);
}

#[tokio::test]
async fn legacy_task_dates_use_utc_without_fractional_rounding_or_invented_dates() {
    let pool = legacy_database().await;
    let completed = [
        ("offset", "2026-11-14T00:30:00+05:00", "2026-11-13"),
        ("fraction", "2026-11-13T23:59:59.999999999Z", "2026-11-13"),
        ("preepoch", "1969-12-31T23:59:59.999999999Z", "1969-12-31"),
        ("leap", "2016-12-31T23:59:60.5Z", "2016-12-31"),
    ];
    for (id, time, _) in completed {
        sqlx::query("INSERT INTO tasks(id,title,completed_at) VALUES (?, 'Legacy task', ?)")
            .bind(id)
            .bind(time)
            .execute(&pool)
            .await
            .unwrap();
    }
    for (id, time) in [
        ("invalid", "not-a-date"),
        ("relative", "now"),
        ("no-zone", "2026-11-14T12:00:00"),
        ("invalid-date", "2026-02-31T00:00:00Z"),
    ] {
        sqlx::query("INSERT INTO tasks(id,title,completed_at) VALUES (?, 'Legacy task', ?)")
            .bind(id)
            .bind(time)
            .execute(&pool)
            .await
            .unwrap();
    }
    MIGRATOR.run(&pool).await.unwrap();
    for (id, time, day) in completed {
        let actual: (String, String, String, i64) = sqlx::query_as("SELECT astronaut_id,mission_date,completed_at,completed_at_ms FROM task_completions WHERE task_id=?")
            .bind(id).fetch_one(&pool).await.unwrap();
        assert_eq!(
            actual,
            (
                "demo-astronaut".into(),
                day.into(),
                time.into(),
                DateTime::parse_from_rfc3339(time)
                    .unwrap()
                    .timestamp_millis()
            )
        );
    }
    let issues: Vec<String> =
        sqlx::query_scalar("SELECT task_id FROM task_completion_migration_issues ORDER BY task_id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(issues, ["invalid", "invalid-date", "no-zone", "relative"]);
    let unchanged: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT id,completed_at FROM tasks ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(unchanged.len(), 9);
    assert!(unchanged.contains(&("daily-check".into(), None)));
    let templates: i64 = sqlx::query_scalar("SELECT count(*) FROM task_templates")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(templates, 9);
    MIGRATOR.run(&pool).await.unwrap();
    let dated_count: i64 = sqlx::query_scalar("SELECT count(*) FROM task_completions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(dated_count, 4);
}

#[tokio::test]
async fn daily_completion_key_and_calendar_constraints_reject_invalid_rows() {
    let pool = legacy_database().await;
    MIGRATOR.run(&pool).await.unwrap();
    let insert = "INSERT INTO task_completions(astronaut_id,task_id,mission_date,completed_at,completed_at_ms) VALUES ('demo-astronaut',?,?,'2026-11-14T08:00:00Z',1794643200000)";
    sqlx::query(insert)
        .bind("daily-check")
        .bind("2026-11-14")
        .execute(&pool)
        .await
        .unwrap();
    assert!(sqlx::query(insert)
        .bind("daily-check")
        .bind("2026-11-14")
        .execute(&pool)
        .await
        .is_err());
    sqlx::query(insert)
        .bind("daily-check")
        .bind("2026-11-15")
        .execute(&pool)
        .await
        .unwrap();
    for day in ["2026-02-31", "yesterday", "2026-1-1"] {
        assert!(
            sqlx::query(insert)
                .bind("daily-check")
                .bind(day)
                .execute(&pool)
                .await
                .is_err(),
            "{day}"
        );
    }
    assert!(sqlx::query(insert)
        .bind("missing-template")
        .bind("2026-11-14")
        .execute(&pool)
        .await
        .is_err());
}

#[tokio::test]
async fn invalid_legacy_measurement_timestamp_rolls_back_migration_without_data_loss() {
    for (measured_at, received_at) in [(Some("invalid"), "2026-11-14T00:00:00Z"), (None, "invalid")]
    {
        let pool = legacy_database().await;
        insert_measurement(&pool, "preserved", 0, measured_at, received_at)
            .await
            .unwrap();
        assert!(MIGRATOR.run(&pool).await.is_err());
        let columns = sqlx::query("PRAGMA table_xinfo(measurements)")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert!(!columns
            .iter()
            .any(|row| row.get::<String, _>("name") == "measured_at_ms"));
        let data: (Option<String>, String) =
            sqlx::query_as("SELECT measured_at,received_at FROM measurements WHERE id='preserved'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            data,
            (measured_at.map(str::to_string), received_at.to_string())
        );
        let versions: Vec<i64> =
            sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(versions, [1]);
        sqlx::query("UPDATE measurements SET measured_at=NULL,received_at='2026-11-14T00:00:00Z'")
            .execute(&pool)
            .await
            .unwrap();
        MIGRATOR.run(&pool).await.unwrap();
    }
}
