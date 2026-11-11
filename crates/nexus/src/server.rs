//! Bind and serve the hub.

use std::future::Future;
use std::net::SocketAddr;

use crate::HubState;

/// Bind `addr` and return the actual address plus the serve future. Port 0 picks a free port
/// (tests use this to run a real hub over HTTP).
pub async fn bind(
    addr: SocketAddr,
    state: HubState,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<(SocketAddr, impl Future<Output = std::io::Result<()>>)> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let local = listener.local_addr()?;
    let app = crate::router(state);
    let fut = async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await
    };
    Ok((local, fut))
}

/// `madrasa-nexus healthcheck`: ask the local hub for `/health` and report success.
///
/// The container image has no curl; the binary checks itself. Plain HTTP/1.0 over a TCP
/// socket to the loopback address is all that is needed.
pub fn healthcheck(port: u16) -> bool {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_secs(3)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let request = format!(
        "GET {}/health HTTP/1.0\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        crate::API_PREFIX
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    response.starts_with("HTTP/1.")
        && response.contains(" 200 ")
        && response.contains("\"status\":\"ok\"")
}
