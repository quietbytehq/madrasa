//! Bind and serve the router.

use std::future::IntoFuture;
use std::net::SocketAddr;
use tokio::net::TcpListener;

use crate::{router, AppState};

/// Bind `addr` (use port 0 for an ephemeral port) and return the bound address plus a future
/// that runs the server until `shutdown` resolves.
pub async fn bind(
    addr: SocketAddr,
    state: AppState,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> anyhow::Result<(
    SocketAddr,
    impl std::future::Future<Output = std::io::Result<()>>,
)> {
    let listener = TcpListener::bind(addr).await?;
    let local = listener.local_addr()?;
    let app = router(state);
    let fut = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .into_future();
    Ok((local, fut))
}
