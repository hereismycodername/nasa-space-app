use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use std::{env, str::FromStr, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url =
        env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://space-health.db".into());
    let options = SqliteConnectOptions::from_str(&database_url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5))
        .journal_mode(SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    let address = env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let dist = env::var("FRONTEND_DIST").unwrap_or_else(|_| "dist".into());
    let listener = tokio::net::TcpListener::bind(&address).await?;
    println!("Space Health: http://{address}");
    axum::serve(listener, space_health_server::app(pool, &dist))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
