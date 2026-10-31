//! In-process API: owns a tokio runtime thread that opens the database (bundled or external),
//! migrates it, and serves the Axum router on a random loopback port.

use madrasa_api::bootstrap::{Database, DatabaseSource};
use madrasa_api::{server, AppState};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{oneshot, watch, Mutex};

#[derive(Debug, Clone)]
enum Status {
    Starting,
    Ready(String),
    Failed(String),
}

pub struct Backend {
    status: watch::Receiver<Status>,
    shutdown: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}

impl Backend {
    pub fn start() -> Backend {
        let (status_tx, status_rx) = watch::channel(Status::Starting);
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        std::thread::Builder::new()
            .name("madrasa-backend".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        let _ = status_tx.send(Status::Failed(format!(
                            "Could not start the service runtime: {e}"
                        )));
                        return;
                    }
                };
                rt.block_on(async move {
                    match run(shutdown_rx, &status_tx).await {
                        Ok(()) => tracing::info!("backend stopped"),
                        Err(e) => {
                            tracing::error!(error = ?e, "backend failed");
                            let _ = status_tx.send(Status::Failed(friendly(&e)));
                        }
                    }
                });
            })
            .expect("spawn backend thread");

        Backend {
            status: status_rx,
            shutdown: Arc::new(Mutex::new(Some(shutdown_tx))),
        }
    }

    /// Wait until the API is listening and return its base URL.
    pub async fn base_url(&self) -> Result<String, String> {
        let mut rx = self.status.clone();
        loop {
            match rx.borrow_and_update().clone() {
                Status::Ready(url) => return Ok(url),
                Status::Failed(msg) => return Err(msg),
                Status::Starting => {}
            }
            if rx.changed().await.is_err() {
                return Err("The Madrasa service stopped unexpectedly.".into());
            }
        }
    }

    pub fn shutdown(&self) {
        if let Ok(mut guard) = self.shutdown.try_lock() {
            if let Some(tx) = guard.take() {
                let _ = tx.send(());
            }
        }
    }
}

/// The bundled PostgreSQL refuses to run as root, so say so plainly instead of failing later
/// with a cryptic database error. `uid` is the effective user id on Unix.
pub fn root_check(uid: Option<u32>) -> Result<(), String> {
    match uid {
        Some(0) => Err(ROOT_MESSAGE.into()),
        _ => Ok(()),
    }
}

pub const ROOT_MESSAGE: &str = "Madrasa cannot run as root: the bundled database refuses to start \
under the root account. Start Madrasa as a normal user (no sudo).";

#[cfg(target_os = "linux")]
fn effective_uid() -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/proc/self").ok().map(|m| m.uid())
}

#[cfg(not(target_os = "linux"))]
fn effective_uid() -> Option<u32> {
    None
}

async fn run(
    shutdown_rx: oneshot::Receiver<()>,
    status: &watch::Sender<Status>,
) -> anyhow::Result<()> {
    if matches!(DatabaseSource::from_env(), DatabaseSource::Embedded { .. }) {
        root_check(effective_uid()).map_err(anyhow::Error::msg)?;
    }
    let db = Database::open(DatabaseSource::from_env()).await?;
    let state = AppState::new(db.pool.clone()).await?;
    madrasa_api::backup::spawn_scheduler(state.clone());
    madrasa_api::late_fees::spawn_scheduler(state.clone());
    let bind: SocketAddr = "127.0.0.1:0".parse()?;
    let (addr, fut) = server::bind(bind, state, async {
        let _ = shutdown_rx.await;
    })
    .await?;
    let url = format!("http://{addr}");
    tracing::info!(%url, "api listening");
    let _ = status.send(Status::Ready(url));
    fut.await?;
    db.shutdown().await;
    Ok(())
}

/// Plain-language message for the UI. Never includes connection strings.
fn friendly(err: &anyhow::Error) -> String {
    let text = err.to_string();
    if text == ROOT_MESSAGE {
        return text;
    }
    if text.contains("no database configured") {
        "No database is configured. Set MADRASA_DATABASE_URL or install the all-in-one build."
            .into()
    } else {
        "The Madrasa service could not start. See the log file for details.".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_is_refused_with_a_plain_message() {
        assert_eq!(root_check(Some(0)), Err(ROOT_MESSAGE.into()));
        assert_eq!(root_check(Some(1000)), Ok(()));
        assert_eq!(root_check(None), Ok(()), "Windows: no uid");
        assert_eq!(friendly(&anyhow::Error::msg(ROOT_MESSAGE)), ROOT_MESSAGE);
    }
}
