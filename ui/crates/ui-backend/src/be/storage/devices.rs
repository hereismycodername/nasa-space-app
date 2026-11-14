// Taken from the team backend (branch codex/team-integration, commit 2c3d1db, server/src/storage/devices.rs) unless noted below.
use sqlx::SqlitePool;

#[derive(sqlx::FromRow)]
pub(crate) struct DeviceRecord {
    pub id: String,
    pub astronaut_id: String,
    pub source: String,
}

pub(crate) struct Registration {
    pub device: DeviceRecord,
    pub created: bool,
}

/// Register once without modifying an existing device's owner or source.
pub(crate) async fn register_sensor(
    pool: &SqlitePool,
    id: &str,
    astronaut_id: &str,
) -> Result<Registration, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let inserted = sqlx::query(
        "INSERT INTO devices(id, astronaut_id, source) VALUES (?, ?, 'sensor')
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(id)
    .bind(astronaut_id)
    .execute(&mut *transaction)
    .await?;
    let device = sqlx::query_as("SELECT id, astronaut_id, source FROM devices WHERE id = ?")
        .bind(id)
        .fetch_one(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(Registration {
        device,
        created: inserted.rows_affected() == 1,
    })
}

pub(crate) async fn list(pool: &SqlitePool) -> Result<Vec<DeviceRecord>, sqlx::Error> {
    sqlx::query_as("SELECT id, astronaut_id, source FROM devices ORDER BY id")
        .fetch_all(pool)
        .await
}
