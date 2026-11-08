//! LAN mode for Lite: one PC is the school server, the others connect to it.
//!
//! - The server serves the same router over TLS on the local network with a **self-signed
//!   certificate** it generates once (`tls/` in the data directory). Its SHA-256 fingerprint is
//!   shown on the server and confirmed by each client on first connect (pinning): afterwards a
//!   client refuses any other certificate.
//! - A client runs a **loopback proxy**: the UI keeps talking to `127.0.0.1`, and the proxy
//!   forwards every request to the server over TLS with the pinned fingerprint. No browser
//!   trust store is involved.
//! - **Discovery**: the server answers UDP broadcasts on [`DISCOVERY_PORT`] with its name,
//!   port and fingerprint; a client can also type the address by hand.
//!
//! Logins and permissions are the API's own: each user signs in with their account on their PC
//! and the backend enforces every permission, exactly as in standalone mode.

use crate::{router, AppState};
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{header, HeaderName, HeaderValue, Request, Response, StatusCode};
use axum::Router;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::WebPkiSupportedAlgorithms;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::UdpSocket;

/// UDP port the server answers discovery requests on.
pub const DISCOVERY_PORT: u16 = 47121;
/// Default TLS port for the school server.
pub const DEFAULT_PORT: u16 = 8471;
const DISCOVER_REQUEST: &[u8] = b"MADRASA_DISCOVER v1";
/// Request bodies larger than this are refused by the proxy (imports are capped at 30 MB).
const MAX_BODY: usize = 48 * 1024 * 1024;

/// Install the `ring` crypto provider once (sqlx uses ring too). Safe to call repeatedly.
pub fn init() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

// ---- certificate --------------------------------------------------------------------------

/// The server's self-signed certificate and key (DER).
#[derive(Clone)]
pub struct Identity {
    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
    /// SHA-256 of the certificate, `AB:CD:…`, as shown to users.
    pub fingerprint: String,
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity")
            .field("fingerprint", &self.fingerprint)
            .finish()
    }
}

/// `AB:CD:…` SHA-256 of a DER certificate.
pub fn fingerprint_of(cert_der: &[u8]) -> String {
    let digest = Sha256::digest(cert_der);
    digest
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// Compare two fingerprints as users type them (case, separators and spaces do not matter).
pub fn same_fingerprint(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        s.chars()
            .filter(|c| c.is_ascii_hexdigit())
            .map(|c| c.to_ascii_uppercase())
            .collect::<String>()
    };
    let (a, b) = (norm(a), norm(b));
    !a.is_empty() && a == b
}

/// Load the certificate from `<dir>/tls/`, or create a self-signed one valid for the host
/// name and the usual LAN addresses and save it there (user-only permissions on Unix).
pub fn load_or_create_identity(dir: &Path) -> anyhow::Result<Identity> {
    let tls = dir.join("tls");
    let cert_path = tls.join("cert.der");
    let key_path = tls.join("key.der");
    if cert_path.exists() && key_path.exists() {
        let cert_der = std::fs::read(&cert_path)?;
        let key_der = std::fs::read(&key_path)?;
        return Ok(Identity {
            fingerprint: fingerprint_of(&cert_der),
            cert_der,
            key_der,
        });
    }
    std::fs::create_dir_all(&tls)?;
    let mut names = vec!["madrasa-server".to_string(), "localhost".to_string()];
    if let Ok(h) = hostname::get() {
        if let Some(h) = h.to_str() {
            names.push(h.to_string());
        }
    }
    for ip in local_addresses() {
        names.push(ip.to_string());
    }
    let certified = rcgen::generate_simple_self_signed(names)?;
    let cert_der = certified.cert.der().to_vec();
    let key_der = certified.key_pair.serialize_der();
    write_private(&cert_path, &cert_der)?;
    write_private(&key_path, &key_der)?;
    Ok(Identity {
        fingerprint: fingerprint_of(&cert_der),
        cert_der,
        key_der,
    })
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// The PC's LAN addresses (best effort): the address used to reach the network, plus any
/// others the OS reports, without loopback.
pub fn local_addresses() -> Vec<IpAddr> {
    let mut out = Vec::new();
    if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if sock.connect("10.254.254.254:1").is_ok() {
            if let Ok(a) = sock.local_addr() {
                if !a.ip().is_loopback() && !a.ip().is_unspecified() {
                    out.push(a.ip());
                }
            }
        }
    }
    out
}

// ---- TLS server ---------------------------------------------------------------------------

/// Serve the API over TLS on `addr` (port 0 for any) and return the bound address plus the
/// server future, which ends after `shutdown` resolves (connections get five seconds).
pub async fn serve_tls(
    addr: SocketAddr,
    state: AppState,
    identity: &Identity,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> anyhow::Result<(SocketAddr, impl Future<Output = std::io::Result<()>>)> {
    init();
    let config = axum_server::tls_rustls::RustlsConfig::from_der(
        vec![identity.cert_der.clone()],
        identity.key_der.clone(),
    )
    .await?;
    let handle = axum_server::Handle::new();
    let server = axum_server::bind_rustls(addr, config)
        .handle(handle.clone())
        .serve(router(state).into_make_service_with_connect_info::<SocketAddr>());
    let server = tokio::spawn(server);
    let local = handle
        .listening()
        .await
        .ok_or_else(|| anyhow::anyhow!("TLS listener did not start on {addr}"))?;
    tokio::spawn(async move {
        shutdown.await;
        handle.graceful_shutdown(Some(Duration::from_secs(5)));
    });
    let fut = async move {
        match server.await {
            Ok(r) => r,
            Err(e) => Err(std::io::Error::other(e)),
        }
    };
    Ok((local, fut))
}

// ---- pinned client ------------------------------------------------------------------------

/// Records the fingerprint it saw and, when one is expected, refuses any other certificate.
#[derive(Debug)]
pub struct PinnedVerifier {
    expected: Option<String>,
    seen: Mutex<Option<String>>,
    algorithms: WebPkiSupportedAlgorithms,
}

impl PinnedVerifier {
    pub fn new(expected: Option<&str>) -> Arc<Self> {
        Arc::new(Self {
            expected: expected.map(str::to_owned),
            seen: Mutex::new(None),
            algorithms: rustls::crypto::ring::default_provider().signature_verification_algorithms,
        })
    }
    /// The server certificate's fingerprint from the last handshake.
    pub fn seen(&self) -> Option<String> {
        self.seen.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let fp = fingerprint_of(end_entity.as_ref());
        *self.seen.lock().unwrap_or_else(|e| e.into_inner()) = Some(fp.clone());
        match &self.expected {
            Some(expected) if !same_fingerprint(expected, &fp) => Err(rustls::Error::General(
                "the server's certificate does not match the pinned fingerprint".into(),
            )),
            _ => Ok(ServerCertVerified::assertion()),
        }
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

/// An HTTPS client that only talks to the server with this fingerprint (`None`: any server,
/// recording what it saw, for the first-connect probe).
pub fn pinned_client(
    expected: Option<&str>,
) -> anyhow::Result<(reqwest::Client, Arc<PinnedVerifier>)> {
    init();
    let verifier = PinnedVerifier::new(expected);
    let tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier.clone())
        .with_no_client_auth();
    let client = reqwest::Client::builder()
        .use_preconfigured_tls(tls)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(120))
        .build()?;
    Ok((client, verifier))
}

/// What a client learns about a server before trusting it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Probe {
    pub url: String,
    pub fingerprint: String,
    pub version: String,
    pub needs_setup: bool,
}

/// Connect to `url` (`https://host:port`), accept whatever certificate it presents once, and
/// report its fingerprint and version so the user can compare them with the server's screen.
pub async fn probe(url: &str) -> anyhow::Result<Probe> {
    let base = normalise_url(url)?;
    let (client, verifier) = pinned_client(None)?;
    let features: serde_json::Value = client
        .get(format!("{base}/api/v1/system/features"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let status: serde_json::Value = client
        .get(format!("{base}/api/v1/setup/status"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let fingerprint = verifier
        .seen()
        .ok_or_else(|| anyhow::anyhow!("no certificate seen"))?;
    Ok(Probe {
        url: base,
        fingerprint,
        version: features["version"].as_str().unwrap_or("").to_string(),
        needs_setup: status["needs_setup"].as_bool().unwrap_or(false),
    })
}

/// `host`, `host:port` or `https://host:port` → `https://host:port` (default port).
pub fn normalise_url(input: &str) -> anyhow::Result<String> {
    let s = input.trim().trim_end_matches('/');
    if s.is_empty() {
        anyhow::bail!("Enter the server address");
    }
    let s = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(s);
    let (host, port) = match s.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) && !h.contains(':') => {
            (h.to_string(), p.parse::<u16>()?)
        }
        _ => (s.to_string(), DEFAULT_PORT),
    };
    if host.is_empty() {
        anyhow::bail!("Enter the server address");
    }
    Ok(format!("https://{host}:{port}"))
}

// ---- loopback proxy -----------------------------------------------------------------------

#[derive(Clone)]
struct ProxyState {
    upstream: String,
    client: reqwest::Client,
}

/// Serve a plain-HTTP proxy on `bind` (loopback) that forwards every request to `upstream`
/// over TLS, trusting only the certificate with `fingerprint`.
pub async fn serve_proxy(
    bind: SocketAddr,
    upstream: &str,
    fingerprint: &str,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> anyhow::Result<(SocketAddr, impl Future<Output = std::io::Result<()>>)> {
    let (client, _) = pinned_client(Some(fingerprint))?;
    let state = ProxyState {
        upstream: normalise_url(upstream)?,
        client,
    };
    let app = Router::new().fallback(forward).with_state(state);
    let app = app.into_make_service_with_connect_info::<SocketAddr>();
    let listener = tokio::net::TcpListener::bind(bind).await?;
    let local = listener.local_addr()?;
    let fut = axum::serve(listener, app).with_graceful_shutdown(shutdown);
    Ok((local, async move { fut.await }))
}

const HOP_BY_HOP: &[HeaderName] = &[
    header::CONNECTION,
    header::TRANSFER_ENCODING,
    header::UPGRADE,
    header::HOST,
    header::CONTENT_LENGTH,
];

async fn forward(State(p): State<ProxyState>, req: Request<Body>) -> Response<Body> {
    let (parts, body) = req.into_parts();
    let path = parts
        .uri
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_else(|| "/".into());
    let bytes = match axum::body::to_bytes(body, MAX_BODY).await {
        Ok(b) => b,
        Err(_) => return proxy_error(StatusCode::PAYLOAD_TOO_LARGE, "The request is too large."),
    };
    let mut rb = p
        .client
        .request(parts.method.clone(), format!("{}{}", p.upstream, path))
        .body(bytes);
    for (name, value) in parts.headers.iter() {
        if !HOP_BY_HOP.contains(name) {
            rb = rb.header(name.clone(), value.clone());
        }
    }
    match rb.send().await {
        Ok(resp) => {
            let status = resp.status();
            let mut out = Response::builder().status(status);
            for (name, value) in resp.headers().iter() {
                if !HOP_BY_HOP.contains(name) {
                    out = out.header(name.clone(), value.clone());
                }
            }
            match resp.bytes().await {
                Ok(b) => out
                    .body(Body::from(b))
                    .unwrap_or_else(|_| proxy_error(StatusCode::BAD_GATEWAY, "Bad reply.")),
                Err(e) => proxy_error(StatusCode::BAD_GATEWAY, &e.to_string()),
            }
        }
        Err(e) => {
            let message = if e.to_string().contains("pinned fingerprint") {
                "The school server's certificate has changed. Ask the administrator for the new fingerprint and connect again.".to_string()
            } else {
                format!("The school server could not be reached: {e}")
            };
            proxy_error(StatusCode::BAD_GATEWAY, &message)
        }
    }
}

fn proxy_error(status: StatusCode, message: &str) -> Response<Body> {
    let body = serde_json::json!({ "error": { "code": "server_unreachable", "message": message } });
    Response::builder()
        .status(status)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )
        .body(Body::from(Bytes::from(body.to_string())))
        .unwrap_or_default()
}

// ---- discovery ----------------------------------------------------------------------------

/// What a server announces on the LAN.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
    pub port: u16,
    pub fingerprint: String,
}

/// A server a client found, with the address it answered from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Found {
    pub host: String,
    pub url: String,
    pub info: ServerInfo,
}

/// Answer discovery requests on `bind` (normally `0.0.0.0:47121`). Returns the bound address.
pub async fn spawn_beacon(bind: SocketAddr, info: ServerInfo) -> anyhow::Result<SocketAddr> {
    let socket = UdpSocket::bind(bind).await?;
    let local = socket.local_addr()?;
    let reply = serde_json::to_vec(&info)?;
    tokio::spawn(async move {
        let mut buf = [0u8; 512];
        loop {
            match socket.recv_from(&mut buf).await {
                Ok((n, from)) if buf[..n].starts_with(DISCOVER_REQUEST) => {
                    let _ = socket.send_to(&reply, from).await;
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(error = %e, "discovery beacon stopped");
                    break;
                }
            }
        }
    });
    Ok(local)
}

/// Ask the LAN (broadcast) and this PC for Madrasa servers and collect answers for `timeout`.
pub async fn discover(timeout: Duration) -> Vec<Found> {
    let targets = [
        SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), DISCOVERY_PORT),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DISCOVERY_PORT),
    ];
    discover_at(&targets, timeout).await
}

/// [`discover`] against given addresses (tests use a loopback beacon on any port).
pub async fn discover_at(targets: &[SocketAddr], timeout: Duration) -> Vec<Found> {
    let mut found: Vec<Found> = Vec::new();
    let Ok(socket) = UdpSocket::bind("0.0.0.0:0").await else {
        return found;
    };
    let _ = socket.set_broadcast(true);
    for t in targets {
        let _ = socket.send_to(DISCOVER_REQUEST, t).await;
    }
    let deadline = tokio::time::Instant::now() + timeout;
    let mut buf = [0u8; 2048];
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            break;
        }
        match tokio::time::timeout(left, socket.recv_from(&mut buf)).await {
            Ok(Ok((n, from))) => {
                if let Ok(info) = serde_json::from_slice::<ServerInfo>(&buf[..n]) {
                    let host = from.ip().to_string();
                    let url = format!("https://{host}:{}", info.port);
                    if !found.iter().any(|f| f.url == url) {
                        found.push(Found { host, url, info });
                    }
                }
            }
            _ => break,
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_format_and_compare_loosely() {
        let fp = fingerprint_of(b"hello");
        assert_eq!(fp.len(), 32 * 3 - 1);
        assert!(fp.starts_with("2C:F2:4D"));
        assert!(same_fingerprint(&fp, &fp.to_lowercase().replace(':', " ")));
        assert!(!same_fingerprint(&fp, "00:11"));
        assert!(!same_fingerprint("", ""));
    }

    #[test]
    fn addresses_normalise_to_https_with_the_default_port() {
        assert_eq!(
            normalise_url("192.168.1.10").unwrap(),
            "https://192.168.1.10:8471"
        );
        assert_eq!(
            normalise_url(" office-pc:9000/ ").unwrap(),
            "https://office-pc:9000"
        );
        assert_eq!(
            normalise_url("http://192.168.1.10:8471").unwrap(),
            "https://192.168.1.10:8471"
        );
        assert!(normalise_url("").is_err());
    }

    #[test]
    fn identity_is_created_once_and_reloaded() {
        let dir = std::env::temp_dir().join(format!("madrasa-tls-{}", uuid::Uuid::now_v7()));
        let a = load_or_create_identity(&dir).unwrap();
        let b = load_or_create_identity(&dir).unwrap();
        assert_eq!(a.fingerprint, b.fingerprint);
        assert_eq!(a.cert_der, b.cert_der);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn beacon_answers_discovery_requests() {
        let info = ServerInfo {
            name: "Office PC".into(),
            version: "0.1.3".into(),
            port: 8471,
            fingerprint: "AA:BB".into(),
        };
        let addr = spawn_beacon("127.0.0.1:0".parse().unwrap(), info.clone())
            .await
            .unwrap();
        let found = discover_at(&[addr], Duration::from_millis(500)).await;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].info, info);
        assert_eq!(found[0].url, "https://127.0.0.1:8471");
    }
}
