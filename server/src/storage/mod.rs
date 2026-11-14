//! Concrete SQLite functions. HTTP status codes and DTOs do not belong here.
pub(crate) mod measurements;
pub(crate) mod series;
pub(crate) mod tasks;

pub(crate) async fn check_connection(pool: &sqlx::SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}
