//! Standalone API server (Standard edition and development).
//!
//! Environment:
//! - `MADRASA_DATABASE_URL` PostgreSQL URL (required unless built with `embedded-postgres`)
//! - `MADRASA_BIND` address to bind, default `127.0.0.1:4777`
//! - `MADRASA_DATA_DIR` data directory for the bundled database
//! - `RUST_LOG` tracing filter, default `info`

use madrasa_api::bootstrap::{Database, DatabaseSource};
use madrasa_api::{server, AppState};
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let bind: SocketAddr = std::env::var("MADRASA_BIND")
        .unwrap_or_else(|_| "127.0.0.1:4777".into())
        .parse()?;
    let db = Database::open(DatabaseSource::from_env()).await?;
    let state = AppState::new(db.pool.clone()).await?;
    madrasa_api::backup::spawn_scheduler(state.clone());
    madrasa_api::late_fees::spawn_scheduler(state.clone());
    let (addr, fut) = server::bind(bind, state, shutdown_signal()).await?;
    tracing::info!(%addr, edition = "see /system/features", "madrasa-server listening");
    fut.await?;
    db.shutdown().await;
    Ok(())
}

/// Resolve on Ctrl+C or, on Unix, SIGTERM, so the bundled database is stopped cleanly.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
