use crate::domain::tasks::Task;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

fn decode(row: SqliteRow) -> Result<Task, sqlx::Error> {
    Ok(Task {
        id: row.try_get("id")?,
        title: row.try_get("title")?,
        completed_at: row.try_get("completed_at")?,
    })
}
pub(crate) async fn list(pool: &SqlitePool) -> Result<Vec<Task>, sqlx::Error> {
    sqlx::query("SELECT * FROM tasks ORDER BY id")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(decode)
        .collect()
}
pub(crate) async fn set_completion(
    pool: &SqlitePool,
    id: &str,
    completed_at: Option<DateTime<Utc>>,
) -> Result<Option<Task>, sqlx::Error> {
    // Serialize read + write so concurrent PATCH requests cannot diverge between
    // the legacy task view and dated completions. The dated HTTP API is BE-07.
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
    let previous = sqlx::query("SELECT * FROM tasks WHERE id=?")
        .bind(id)
        .fetch_optional(&mut *transaction)
        .await?
        .map(decode)
        .transpose()?;
    let Some(previous) = previous else {
        transaction.commit().await?;
        return Ok(None);
    };

    if let Some(time) = completed_at {
        sqlx::query(
            "INSERT INTO task_completions (astronaut_id,task_id,mission_date,completed_at,completed_at_ms)
             VALUES ('demo-astronaut',?,?,?,?)
             ON CONFLICT(astronaut_id,task_id,mission_date)
             DO UPDATE SET completed_at=excluded.completed_at, completed_at_ms=excluded.completed_at_ms",
        )
        .bind(id)
        .bind(time.date_naive().to_string())
        .bind(time.to_rfc3339())
        .bind(time.timestamp_millis())
        .execute(&mut *transaction)
        .await?;
    } else if let Some(time) = previous
        .completed_at
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
    {
        // An undated legacy PATCH clears the mark it displayed, never an
        // unrelated day's history. Unparseable legacy marks remain in the audit.
        sqlx::query(
            "DELETE FROM task_completions WHERE astronaut_id='demo-astronaut' AND task_id=? AND mission_date=?",
        )
        .bind(id)
        .bind(time.with_timezone(&Utc).date_naive().to_string())
        .execute(&mut *transaction)
        .await?;
    }

    let task = decode(
        sqlx::query("UPDATE tasks SET completed_at=? WHERE id=? RETURNING *")
            .bind(completed_at.map(|time| time.to_rfc3339()))
            .bind(id)
            .fetch_one(&mut *transaction)
            .await?,
    )?;
    transaction.commit().await?;
    Ok(Some(task))
}

// Used once the BE-07 HTTP API switches over.
#[allow(dead_code)]
pub(crate) async fn list_for_date(
    pool: &SqlitePool,
    astronaut_id: &str,
    mission_date: NaiveDate,
) -> Result<Vec<Task>, sqlx::Error> {
    sqlx::query(
        "SELECT t.id, t.title, c.completed_at
         FROM task_templates AS t
         LEFT JOIN task_completions AS c
           ON c.astronaut_id=? AND c.task_id=t.id AND c.mission_date=?
         ORDER BY t.id",
    )
    .bind(astronaut_id)
    .bind(mission_date.to_string())
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(decode)
    .collect()
}

// Used once the BE-07 HTTP API switches over.
#[allow(dead_code)]
pub(crate) async fn set_completion_for_date(
    pool: &SqlitePool,
    astronaut_id: &str,
    task_id: &str,
    mission_date: NaiveDate,
    completed: bool,
    now: DateTime<Utc>,
) -> Result<Option<Task>, sqlx::Error> {
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
    let template = sqlx::query("SELECT id FROM task_templates WHERE id=?")
        .bind(task_id)
        .fetch_optional(&mut *transaction)
        .await?;
    if template.is_none() {
        transaction.commit().await?;
        return Ok(None);
    }

    let mission_date = mission_date.to_string();
    if completed {
        sqlx::query(
            "INSERT INTO task_completions (astronaut_id,task_id,mission_date,completed_at,completed_at_ms)
             VALUES (?,?,?,?,?)
             ON CONFLICT(astronaut_id,task_id,mission_date) DO NOTHING",
        )
        .bind(astronaut_id)
        .bind(task_id)
        .bind(&mission_date)
        .bind(now.to_rfc3339())
        .bind(now.timestamp_millis())
        .execute(&mut *transaction)
        .await?;
    } else {
        sqlx::query(
            "DELETE FROM task_completions WHERE astronaut_id=? AND task_id=? AND mission_date=?",
        )
        .bind(astronaut_id)
        .bind(task_id)
        .bind(&mission_date)
        .execute(&mut *transaction)
        .await?;
    }

    let task = decode(
        sqlx::query(
            "SELECT t.id, t.title, c.completed_at
             FROM task_templates AS t
             LEFT JOIN task_completions AS c
               ON c.astronaut_id=? AND c.task_id=t.id AND c.mission_date=?
             WHERE t.id=?",
        )
        .bind(astronaut_id)
        .bind(&mission_date)
        .bind(task_id)
        .fetch_one(&mut *transaction)
        .await?,
    )?;
    transaction.commit().await?;
    Ok(Some(task))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        pool
    }

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[tokio::test]
    async fn dated_completions_are_idempotent_and_isolated_across_midnight() {
        let pool = setup().await;
        sqlx::query(
            "INSERT INTO task_templates (id,title) VALUES ('z-check','Last'),('a-check','First')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let first = utc("2026-11-13T23:59:59.123456Z");
        let later = utc("2026-11-14T00:00:01.987654Z");
        let date = first.date_naive();
        let next_date = later.date_naive();
        let empty = list_for_date(&pool, "crew-a", date).await.unwrap();
        let templates: Vec<(String, String)> =
            sqlx::query_as("SELECT id, title FROM task_templates ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            empty
                .iter()
                .map(|task| (task.id.clone(), task.title.clone()))
                .collect::<Vec<_>>(),
            templates
        );
        assert_eq!(empty.len(), 3);
        assert!(empty.iter().all(|task| task.completed_at.is_none()));

        for (astronaut, day) in [("crew-a", next_date), ("crew-b", date)] {
            set_completion_for_date(&pool, astronaut, "daily-check", day, true, first)
                .await
                .unwrap()
                .unwrap();
        }
        for now in [first, later] {
            let task = set_completion_for_date(&pool, "crew-a", "daily-check", date, true, now)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(task.id, "daily-check");
            assert_eq!(task.title, empty[1].title);
            assert_eq!(task.completed_at, Some(first.to_rfc3339()));
            let stored: (String, i64) = sqlx::query_as(
                "SELECT completed_at, completed_at_ms FROM task_completions
                 WHERE astronaut_id=? AND task_id=? AND mission_date=?",
            )
            .bind("crew-a")
            .bind("daily-check")
            .bind(date.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(stored, (first.to_rfc3339(), first.timestamp_millis()));
            assert_eq!(
                list_for_date(&pool, "crew-a", date).await.unwrap()[1].completed_at,
                task.completed_at
            );
        }

        for _ in 0..2 {
            let task = set_completion_for_date(&pool, "crew-a", "daily-check", date, false, later)
                .await
                .unwrap()
                .unwrap();
            assert!(task.completed_at.is_none());
            assert!(list_for_date(&pool, "crew-a", date)
                .await
                .unwrap()
                .iter()
                .all(|task| task.completed_at.is_none()));
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM task_completions")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(count, 2);
        }

        // The form's selected day survives UTC midnight; the new mark uses actual now.
        let task = set_completion_for_date(&pool, "crew-a", "daily-check", date, true, later)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(task.completed_at, Some(later.to_rfc3339()));
        let stored: (String, String, i64) = sqlx::query_as(
            "SELECT mission_date, completed_at, completed_at_ms FROM task_completions
             WHERE astronaut_id=? AND task_id=? AND mission_date=?",
        )
        .bind("crew-a")
        .bind("daily-check")
        .bind(date.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            stored,
            (
                date.to_string(),
                later.to_rfc3339(),
                later.timestamp_millis()
            )
        );
        for (astronaut, day) in [("crew-a", next_date), ("crew-b", date)] {
            let tasks = list_for_date(&pool, astronaut, day).await.unwrap();
            assert_eq!(tasks[1].completed_at, Some(first.to_rfc3339()));
            assert!(tasks[0].completed_at.is_none());
            assert!(tasks[2].completed_at.is_none());
        }
        assert!(list_for_date(&pool, "crew-b", next_date)
            .await
            .unwrap()
            .iter()
            .all(|task| task.completed_at.is_none()));

        for completed in [true, false] {
            assert!(
                set_completion_for_date(&pool, "crew-a", "missing", date, completed, later)
                    .await
                    .unwrap()
                    .is_none()
            );
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM task_completions")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(count, 3);
        }
        assert!(list(&pool)
            .await
            .unwrap()
            .iter()
            .all(|task| task.completed_at.is_none()));
    }

    #[tokio::test]
    async fn concurrent_dated_completions_keep_one_timestamp_after_reopening() {
        use std::sync::Arc;
        use tokio::{sync::Barrier, task::JoinSet};

        let path =
            std::env::temp_dir().join(format!("space-health-tasks-{}.db", uuid::Uuid::new_v4()));
        let url = format!("sqlite://{}", path.display());
        let pool = crate::database::connect(&url).await.unwrap();
        let journal: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(journal, "wal");
        let first = utc("2026-11-13T23:59:59.123456Z");
        let date = first.date_naive();
        let barrier = Arc::new(Barrier::new(16));
        let mut pending = JoinSet::new();
        for offset in 0..16 {
            let pool = pool.clone();
            let barrier = Arc::clone(&barrier);
            pending.spawn(async move {
                let now = first + chrono::Duration::seconds(offset);
                barrier.wait().await;
                set_completion_for_date(&pool, "crew-a", "daily-check", date, true, now)
                    .await
                    .unwrap()
                    .unwrap()
            });
        }
        let mut returned = Vec::new();
        while let Some(task) = pending.join_next().await {
            returned.push(task.unwrap().completed_at.unwrap());
        }
        assert!(pool.size() > 1);
        let rows: Vec<(String, i64)> =
            sqlx::query_as("SELECT completed_at, completed_at_ms FROM task_completions")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(rows.len(), 1);
        let (saved, saved_ms) = &rows[0];
        assert!((0..16).any(|offset| {
            let candidate = first + chrono::Duration::seconds(offset);
            *saved == candidate.to_rfc3339() && *saved_ms == candidate.timestamp_millis()
        }));
        assert_eq!(returned.len(), 16);
        assert!(returned.iter().all(|time| time == saved));
        let before = list_for_date(&pool, "crew-a", date).await.unwrap();
        assert_eq!(before[0].completed_at.as_ref(), Some(saved));
        pool.close().await;

        let reopened = crate::database::connect(&url).await.unwrap();
        let after = list_for_date(&reopened, "crew-a", date).await.unwrap();
        assert_eq!(
            before
                .into_iter()
                .map(|task| (task.id, task.title, task.completed_at))
                .collect::<Vec<_>>(),
            after
                .into_iter()
                .map(|task| (task.id, task.title, task.completed_at))
                .collect::<Vec<_>>()
        );
        reopened.close().await;
        // Windows may retain SQLite worker handles briefly after the pool closes.
        for _ in 0..20 {
            if std::fs::remove_file(&path).is_ok() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn legacy_task_updates_keep_dated_history_and_clear_only_the_displayed_mark() {
        let pool = setup().await;
        let first = utc("2026-11-14T04:59:59.9999+05:00");
        let next = utc("2026-11-14T05:00:00+05:00");
        for time in [first, next, next + chrono::Duration::seconds(1)] {
            set_completion(&pool, "daily-check", Some(time))
                .await
                .unwrap();
        }
        let dates: Vec<(String, i64)> = sqlx::query_as(
            "SELECT mission_date, completed_at_ms FROM task_completions ORDER BY mission_date",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            dates,
            vec![
                ("2026-11-13".to_owned(), first.timestamp_millis()),
                ("2026-11-14".to_owned(), next.timestamp_millis() + 1000),
            ]
        );
        let cleared = set_completion(&pool, "daily-check", None)
            .await
            .unwrap()
            .unwrap();
        assert!(cleared.completed_at.is_none());
        let remaining: Vec<String> =
            sqlx::query_scalar("SELECT mission_date FROM task_completions")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(remaining, ["2026-11-13"]);
        set_completion(&pool, "daily-check", None).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_completions")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn failure_updating_legacy_task_rolls_back_dated_completion() {
        let pool = setup().await;
        sqlx::query("CREATE TRIGGER reject_task_update BEFORE UPDATE ON tasks BEGIN SELECT RAISE(ABORT, 'test write failure'); END")
            .execute(&pool).await.unwrap();
        assert!(
            set_completion(&pool, "daily-check", Some(utc("2026-11-14T00:00:00Z")))
                .await
                .is_err()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_completions")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
        assert!(list(&pool).await.unwrap()[0].completed_at.is_none());
    }

    #[tokio::test]
    async fn missing_task_does_not_create_a_dated_completion() {
        let pool = setup().await;
        assert!(
            set_completion(&pool, "missing", Some(utc("2026-11-14T00:00:00Z")))
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_completions")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
    }
}
