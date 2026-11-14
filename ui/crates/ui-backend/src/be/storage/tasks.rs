// Taken unchanged from the team backend (branch main, commit 8c53fce, server/src/storage/tasks.rs) unless noted below.
use crate::be::domain::tasks::Task;
use chrono::{DateTime, Utc};
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
