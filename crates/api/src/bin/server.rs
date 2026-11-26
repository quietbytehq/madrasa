//! Standalone API server (Standard edition and development).
//!
//! Environment:
//! - `MADRASA_DATABASE_URL` PostgreSQL URL (required unless built with `embedded-postgres`)
//! - `MADRASA_BIND` address to bind, default `127.0.0.1:4777`
//! - `MADRASA_DATA_DIR` data directory for the bundled database
//! - `MADRASA_LAN_BIND` also serve the API over TLS on this address, as the desktop app does
//!   in *school server* mode (the certificate is kept under the data directory, and its
//!   fingerprint is logged and written to `<data dir>/tls/fingerprint.txt`)
//! - `RUST_LOG` tracing filter, default `info`
//!
//! `madrasa-server proxy` runs what the desktop app runs in *client* mode instead: a
//! loopback proxy to a school server, with the offline queue (ADR 0009). The automated GUI
//! tests use it (scripts/e2e.sh). Environment:
//! - `MADRASA_SERVER_URL` the school server, for example `https://192.168.1.10:8471`
//! - `MADRASA_SERVER_FINGERPRINT` its certificate fingerprint, or `probe` to trust whatever
//!   the server presents now (tests only: nobody compared it)
//! - `MADRASA_PROXY_BIND` address to bind, default `127.0.0.1:4778`
//! - `MADRASA_DATA_DIR` where the offline queue is kept

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

    if std::env::args().nth(1).as_deref() == Some("proxy") {
        return proxy().await;
    }
    let bind: SocketAddr = std::env::var("MADRASA_BIND")
        .unwrap_or_else(|_| "127.0.0.1:4777".into())
        .parse()?;
    let db = Database::open(DatabaseSource::from_env()).await?;
    let state = AppState::new(db.pool.clone()).await?;
    madrasa_api::backup::spawn_scheduler(state.clone());
    madrasa_api::late_fees::spawn_scheduler(state.clone());
    madrasa_api::nexus_link::spawn_scheduler(state.clone());
    if let Ok(lan_bind) = std::env::var("MADRASA_LAN_BIND") {
        let tls_dir = madrasa_api::bootstrap::default_data_dir().join("tls");
        let identity = madrasa_api::lan::load_or_create_identity(&tls_dir)?;
        let (lan_addr, lan) = madrasa_api::lan::serve_tls(
            lan_bind.parse()?,
            state.clone(),
            &identity,
            std::future::pending(),
        )
        .await?;
        madrasa_api::private::write_private(
            &tls_dir.join("fingerprint.txt"),
            identity.fingerprint.as_bytes(),
        )?;
        tracing::info!(addr = %lan_addr, fingerprint = %identity.fingerprint, "also listening over TLS");
        tokio::spawn(lan);
    }
    let (addr, fut) = server::bind(bind, state, shutdown_signal()).await?;
    tracing::info!(%addr, edition = "see /system/features", "madrasa-server listening");
    fut.await?;
    db.shutdown().await;
    Ok(())
}

/// `madrasa-server proxy`: the client side of a school network, without a window.
async fn proxy() -> anyhow::Result<()> {
    use madrasa_api::lan;
    let upstream = std::env::var("MADRASA_SERVER_URL")
        .map_err(|_| anyhow::anyhow!("MADRASA_SERVER_URL is not set"))?;
    let fingerprint = match std::env::var("MADRASA_SERVER_FINGERPRINT").as_deref() {
        Ok("probe") => lan::probe(&upstream).await?.fingerprint,
        Ok(f) if !f.trim().is_empty() => f.trim().to_string(),
        _ => anyhow::bail!("MADRASA_SERVER_FINGERPRINT is not set"),
    };
    let bind: SocketAddr = std::env::var("MADRASA_PROXY_BIND")
        .unwrap_or_else(|_| "127.0.0.1:4778".into())
        .parse()?;
    let folder: String = fingerprint
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(32)
        .collect::<String>()
        .to_lowercase();
    let outbox = madrasa_api::bootstrap::default_data_dir()
        .join("outbox")
        .join(folder);
    let (addr, fut) = lan::serve_proxy(
        bind,
        &upstream,
        &fingerprint,
        Some(&outbox),
        shutdown_signal(),
    )
    .await?;
    tracing::info!(%addr, %upstream, "client proxy listening");
    fut.await?;
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
