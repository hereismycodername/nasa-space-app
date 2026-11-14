use std::{env, path::Path};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = space_health_server::database::connect_from_env().await?;
    let address = env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let dist = env::var("FRONTEND_DIST").unwrap_or_else(|_| "dist".into());
    let working_directory = env::current_dir()?;
    let dist_path = working_directory.join(&dist);
    let options = pool.connect_options();
    let database_path = options.get_filename();
    if database_path == Path::new(":memory:") {
        println!("Database: in-memory SQLite");
    } else {
        println!(
            "Database: {}",
            working_directory.join(database_path).display()
        );
    }
    println!("Frontend: {}", dist_path.display());
    if !dist_path.join("index.html").is_file() {
        eprintln!("Frontend build not found at {}. API remains available; run npm run build or set FRONTEND_DIST.", dist_path.display());
    }
    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .map_err(|error| {
            std::io::Error::new(
                error.kind(),
                format!("Cannot listen on BIND_ADDR={address}: {error}"),
            )
        })?;
    println!("Space Health: http://{}", listener.local_addr()?);
    axum::serve(listener, space_health_server::app(pool, &dist))
        .with_graceful_shutdown(async {
            if let Err(error) = shutdown_signal().await {
                eprintln!("Shutdown signal handler failed: {error}");
            }
        })
        .await?;
    Ok(())
}

async fn shutdown_signal() -> Result<(), std::io::Error> {
    // Supervisors opt into stdin because SIGINT from Node forces exit on Windows.
    if env::var("SHUTDOWN_STDIN").as_deref() == Ok("1") {
        let (shutdown, requested) = tokio::sync::oneshot::channel::<()>();
        std::thread::spawn(move || {
            let mut line = String::new();
            let _ = std::io::stdin().read_line(&mut line);
            let _ = shutdown.send(());
        });
        tokio::select! {
            result = os_shutdown_signal() => result,
            _ = requested => Ok(()),
        }
    } else {
        os_shutdown_signal().await
    }
}

async fn os_shutdown_signal() -> Result<(), std::io::Error> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result,
            _ = terminate.recv() => Ok(()),
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await
}
