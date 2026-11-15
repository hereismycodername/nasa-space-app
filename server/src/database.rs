//! Shared SQLite configuration and migrations for the server and local admin CLI.
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::{env, str::FromStr, time::Duration};

/// Default database path, resolved against the process working directory.
pub const DEFAULT_DATABASE_URL: &str = "sqlite://space-health.db";

/// Failure to configure, open, or migrate the local database.
#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    /// DATABASE_URL cannot be represented as a UTF-8 connection URL.
    #[error(
        "DATABASE_URL must contain valid UTF-8; set a SQLite URL such as sqlite://space-health.db"
    )]
    InvalidEnvironment,
    /// A connection URL for another database is not supported.
    #[error("Invalid DATABASE_URL; use sqlite://space-health.db or an absolute sqlite:///path/file.db URL")]
    InvalidScheme,
    /// SQLite rejected the connection URL syntax or options.
    #[error("Invalid DATABASE_URL: {0}; use a valid SQLite URL such as sqlite://space-health.db")]
    InvalidUrl(#[source] sqlx::Error),
    /// SQLite could not parse its URL or establish the connection.
    #[error("Could not open DATABASE_URL database: {0}; create its parent directory and grant write access to the directory and database file")]
    Connection(#[from] sqlx::Error),
    /// The current database cannot be migrated by this application version.
    #[error("Could not migrate SQLite database: {0}; check database write permissions and use a database compatible with this release")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

/// Open DATABASE_URL (or the default path) and apply the server's migrations.
pub async fn connect_from_env() -> Result<SqlitePool, DatabaseError> {
    let url = match env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(env::VarError::NotPresent) => DEFAULT_DATABASE_URL.into(),
        Err(env::VarError::NotUnicode(_)) => return Err(DatabaseError::InvalidEnvironment),
    };
    connect(&url).await
}

/// Open a SQLite URL with foreign keys, WAL and a five-second write-lock timeout.
///
/// A missing database file is created; its parent directory must already exist.
/// The returned pool has completed every migration, just like server startup.
pub async fn connect(database_url: &str) -> Result<SqlitePool, DatabaseError> {
    if !database_url.starts_with("sqlite:") {
        return Err(DatabaseError::InvalidScheme);
    }
    let options = SqliteConnectOptions::from_str(database_url)
        .map_err(DatabaseError::InvalidUrl)?
        .create_if_missing(true)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5))
        .journal_mode(SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}
