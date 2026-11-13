//! In-process API: owns a tokio runtime thread that opens the database (bundled or external),
//! migrates it, and serves the Axum router on a random loopback port.

use crate::mode::{Mode, ModeConfig, NetworkInfo};
use madrasa_api::bootstrap::{default_data_dir, Database, DatabaseSource};
use madrasa_api::{lan, server, AppState};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{oneshot, watch, Mutex};

#[derive(Debug, Clone)]
enum Status {
    Starting,
    Ready(String),
    Failed(String),
}

pub struct BackendHandle {
    status: watch::Receiver<Status>,
}

impl BackendHandle {
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
}

pub struct Backend {
    status: watch::Receiver<Status>,
    shutdown: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    network: watch::Receiver<Option<NetworkInfo>>,
    /// `true` once the backend thread has finished (database stopped).
    stopped: watch::Receiver<bool>,
}

impl Backend {
    pub fn start(config: ModeConfig) -> Backend {
        let (status_tx, status_rx) = watch::channel(Status::Starting);
        let (network_tx, network_rx) = watch::channel(None);
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let (stopped_tx, stopped_rx) = watch::channel(false);

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
                    match run(config, shutdown_rx, &status_tx, &network_tx).await {
                        Ok(()) => tracing::info!("backend stopped"),
                        Err(e) => {
                            tracing::error!(error = ?e, "backend failed");
                            let _ = status_tx.send(Status::Failed(friendly(&e)));
                        }
                    }
                });
                let _ = stopped_tx.send(true);
            })
            .expect("spawn backend thread");

        Backend {
            status: status_rx,
            shutdown: Arc::new(Mutex::new(Some(shutdown_tx))),
            network: network_rx,
            stopped: stopped_rx,
        }
    }

    /// Addresses, port and fingerprint once the server (or proxy) is up.
    pub fn network_info(&self) -> Option<NetworkInfo> {
        self.network.borrow().clone()
    }

    /// A handle that can wait for readiness without holding the shell's lock.
    pub fn handle(&self) -> BackendHandle {
        BackendHandle {
            status: self.status.clone(),
        }
    }

    pub fn shutdown(&self) {
        if let Ok(mut guard) = self.shutdown.try_lock() {
            if let Some(tx) = guard.take() {
                let _ = tx.send(());
            }
        }
    }

    /// Resolves once the backend thread has finished (after `shutdown`), or at once if it
    /// has already gone.
    pub async fn stopped(&self) {
        let mut rx = self.stopped.clone();
        while !*rx.borrow_and_update() {
            if rx.changed().await.is_err() {
                return;
            }
        }
    }
}

/// Client mode: no database here, only a loopback proxy to the school server with the pinned
/// certificate fingerprint.
async fn run_client(
    config: ModeConfig,
    shutdown_rx: oneshot::Receiver<()>,
    status: &watch::Sender<Status>,
    network: &watch::Sender<Option<NetworkInfo>>,
) -> anyhow::Result<()> {
    let upstream = config
        .server_url
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no school server configured"))?;
    let fingerprint = config
        .fingerprint
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no server fingerprint configured"))?;
    let bind: SocketAddr = "127.0.0.1:0".parse()?;
    // Changes made while the server is out of reach wait here (ADR 0009), one folder per
    // server so a PC moved to another school never sends them to the wrong one.
    let outbox = madrasa_api::bootstrap::default_data_dir()
        .join("outbox")
        .join(outbox_folder(&fingerprint));
    let (addr, fut) = lan::serve_proxy(bind, &upstream, &fingerprint, Some(&outbox), async {
        let _ = shutdown_rx.await;
    })
    .await?;
    let url = format!("http://{addr}");
    tracing::info!(%url, %upstream, "client proxy listening");
    let _ = network.send(Some(NetworkInfo {
        mode: Mode::Client,
        addresses: Vec::new(),
        port: 0,
        fingerprint: Some(fingerprint),
        server_url: Some(lan::normalise_url(&upstream)?),
    }));
    let _ = status.send(Status::Ready(url));
    fut.await?;
    Ok(())
}

/// The outbox folder for a server: its certificate fingerprint, letters and digits only.
fn outbox_folder(fingerprint: &str) -> String {
    fingerprint
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(32)
        .collect::<String>()
        .to_ascii_lowercase()
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
    config: ModeConfig,
    shutdown_rx: oneshot::Receiver<()>,
    status: &watch::Sender<Status>,
    network: &watch::Sender<Option<NetworkInfo>>,
) -> anyhow::Result<()> {
    lan::init();
    if config.mode == Mode::Client {
        return run_client(config, shutdown_rx, status, network).await;
    }
    if matches!(DatabaseSource::from_env(), DatabaseSource::Embedded { .. }) {
        root_check(effective_uid()).map_err(anyhow::Error::msg)?;
    }
    let db = Database::open(DatabaseSource::from_env()).await?;
    let state = AppState::new(db.pool.clone()).await?;
    madrasa_api::backup::spawn_scheduler(state.clone());
    madrasa_api::late_fees::spawn_scheduler(state.clone());
    madrasa_api::nexus_link::spawn_scheduler(state.clone());
    // One shutdown signal fans out to the loopback server and the LAN server.
    let (stop_tx, _) = tokio::sync::broadcast::channel::<()>(2);
    let mut stop_local = stop_tx.subscribe();
    let bind: SocketAddr = "127.0.0.1:0".parse()?;
    let (addr, local_fut) = server::bind(bind, state.clone(), async move {
        let _ = stop_local.recv().await;
    })
    .await?;
    let url = format!("http://{addr}");
    tracing::info!(%url, "api listening");

    let lan_fut = if config.mode == Mode::Server {
        let identity = lan::load_or_create_identity(&default_data_dir())?;
        let mut stop_lan = stop_tx.subscribe();
        let lan_bind: SocketAddr = format!("0.0.0.0:{}", config.listen_port).parse()?;
        let (lan_addr, fut) = lan::serve_tls(lan_bind, state.clone(), &identity, async move {
            let _ = stop_lan.recv().await;
        })
        .await?;
        let name = {
            let mut c = state.pool.acquire().await?;
            madrasa_db::schools::current(&mut c)
                .await
                .ok()
                .flatten()
                .map(|s| s.name)
                .unwrap_or_else(|| "Madrasa server".into())
        };
        let beacon_bind: SocketAddr = format!("0.0.0.0:{}", lan::DISCOVERY_PORT).parse()?;
        if let Err(e) = lan::spawn_beacon(
            beacon_bind,
            lan::ServerInfo {
                name,
                version: madrasa_api::VERSION.into(),
                port: lan_addr.port(),
                fingerprint: identity.fingerprint.clone(),
            },
        )
        .await
        {
            tracing::warn!(error = %e, "discovery beacon not started; clients must type the address");
        }
        tracing::info!(port = lan_addr.port(), fingerprint = %identity.fingerprint, "school server listening on the LAN");
        let _ = network.send(Some(NetworkInfo {
            mode: Mode::Server,
            addresses: lan::local_addresses()
                .iter()
                .map(|a| a.to_string())
                .collect(),
            port: lan_addr.port(),
            fingerprint: Some(identity.fingerprint),
            server_url: None,
        }));
        Some(fut)
    } else {
        let _ = network.send(Some(NetworkInfo {
            mode: Mode::Standalone,
            addresses: Vec::new(),
            port: 0,
            fingerprint: None,
            server_url: None,
        }));
        None
    };
    let _ = status.send(Status::Ready(url));
    tokio::spawn(async move {
        let _ = shutdown_rx.await;
        let _ = stop_tx.send(());
    });
    match lan_fut {
        Some(lan_fut) => {
            let (a, b) = tokio::join!(local_fut, lan_fut);
            a?;
            b?;
        }
        None => local_fut.await?,
    }
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

    #[test]
    fn each_server_has_its_own_outbox_folder() {
        let a = outbox_folder("AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD");
        assert_eq!(a, "abcdef0123456789abcdef0123456789");
        assert_ne!(a, outbox_folder("AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:88:AB:CD"));
        assert!(!outbox_folder("../../etc:passwd").contains('/'));
    }
}
