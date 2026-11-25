//! A limit on attempts at the hub's doors that take a secret from someone not yet signed in:
//! the console login, the parent login, parent access codes, school join codes and the
//! first-run setup token.
//!
//! Accounts already lock after five wrong passwords, and join and access codes are long
//! random strings, so this is a second line: it stops one address from trying thousands of
//! names or codes, and caps the whole door so a crowd of addresses cannot either. Counted in
//! memory, per door and per client address, in fixed windows.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, MatchedPath, Request, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::{HubError, HubState, API_PREFIX};

/// The doors, by route. Each is counted by itself.
const DOORS: &[&str] = &[
    "/auth/login",
    "/parent/login",
    "/parent/activate",
    "/parent/activate/check",
    "/join",
    "/setup",
];

/// How many attempts one client address may make at one door per window.
pub const PER_CLIENT: u32 = 60;
/// How many attempts all clients together may make at one door per window.
pub const OVERALL: u32 = 1200;
pub const WINDOW: Duration = Duration::from_secs(5 * 60);

#[derive(Debug)]
pub struct Throttle {
    per_client: u32,
    overall: u32,
    window: Duration,
    seen: Mutex<HashMap<(String, String), (Instant, u32)>>,
}

impl Default for Throttle {
    fn default() -> Self {
        Self::new(PER_CLIENT, OVERALL, WINDOW)
    }
}

impl Throttle {
    pub fn new(per_client: u32, overall: u32, window: Duration) -> Self {
        Self {
            per_client,
            overall,
            window,
            seen: Mutex::new(HashMap::new()),
        }
    }

    /// Count one attempt. `Err(seconds)` when the client, or the door as a whole, has used
    /// its attempts for this window: try again after that many seconds.
    pub fn attempt(&self, door: &str, client: &str, now: Instant) -> Result<(), u64> {
        let mut seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
        // Forget finished windows now and then, so the map stays small.
        if seen.len() > 20_000 {
            let window = self.window;
            seen.retain(|_, (start, _)| now.duration_since(*start) < window);
        }
        let mut count = |key: (String, String), limit: u32| -> Result<(), u64> {
            let entry = seen.entry(key).or_insert((now, 0));
            if now.duration_since(entry.0) >= self.window {
                *entry = (now, 0);
            }
            if entry.1 >= limit {
                let left = self.window.saturating_sub(now.duration_since(entry.0));
                return Err(left.as_secs().max(1));
            }
            entry.1 += 1;
            Ok(())
        };
        count((door.to_string(), String::new()), self.overall)?;
        count((door.to_string(), client.to_string()), self.per_client)
    }
}

/// The address the request came from. Behind a reverse proxy (Caddy, Cloud Run, Container
/// Apps) the connection comes from the proxy, and the client is named in `X-Forwarded-For`;
/// `NEXUS_TRUSTED_PROXIES` says how many proxies stand in front, so that many entries from
/// the right are the proxies' own and the one before them is the client. With no proxy
/// configured the header is ignored: anyone could write anything in it.
pub fn client_of(
    peer: Option<SocketAddr>,
    forwarded_for: Option<&str>,
    trusted_proxies: usize,
) -> String {
    if trusted_proxies > 0 {
        if let Some(list) = forwarded_for {
            let hops: Vec<&str> = list
                .split(',')
                .map(str::trim)
                .filter(|h| !h.is_empty())
                .collect();
            if hops.len() >= trusted_proxies {
                return hops[hops.len() - trusted_proxies].to_string();
            }
        }
    }
    peer.map(|p| p.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// The middleware: counts `POST`s to the doors and answers `429` with `Retry-After` when
/// there have been too many.
pub async fn layer(State(state): State<HubState>, req: Request, next: Next) -> Response {
    if req.method() != Method::POST {
        return next.run(req).await;
    }
    let door = req
        .extensions()
        .get::<MatchedPath>()
        .map(|m| {
            m.as_str()
                .strip_prefix(API_PREFIX)
                .unwrap_or(m.as_str())
                .to_string()
        })
        .filter(|path| DOORS.contains(&path.as_str()));
    let Some(door) = door else {
        return next.run(req).await;
    };
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0);
    let forwarded = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok());
    let client = client_of(peer, forwarded, state.trusted_proxies);
    match state.throttle.attempt(&door, &client, Instant::now()) {
        Ok(()) => next.run(req).await,
        Err(seconds) => {
            tracing::warn!(door, "too many attempts; refused for {seconds} s");
            let mut res = HubError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_attempts",
                "Too many attempts. Wait a few minutes and try again.",
            )
            .into_response();
            if let Ok(v) = HeaderValue::from_str(&seconds.to_string()) {
                res.headers_mut().insert(header::RETRY_AFTER, v);
            }
            res
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_client_is_stopped_at_its_limit_and_let_in_again_after_the_window() {
        let t = Throttle::new(3, 100, Duration::from_secs(60));
        let start = Instant::now();
        for _ in 0..3 {
            assert!(t.attempt("/auth/login", "10.0.0.1", start).is_ok());
        }
        let wait = t.attempt("/auth/login", "10.0.0.1", start).unwrap_err();
        assert!((1..=60).contains(&wait));
        // Another address and another door are not affected.
        assert!(t.attempt("/auth/login", "10.0.0.2", start).is_ok());
        assert!(t.attempt("/parent/login", "10.0.0.1", start).is_ok());
        // The window passes.
        let later = start + Duration::from_secs(61);
        assert!(t.attempt("/auth/login", "10.0.0.1", later).is_ok());
    }

    #[test]
    fn a_door_as_a_whole_has_a_cap_however_many_addresses_knock() {
        let t = Throttle::new(2, 5, Duration::from_secs(60));
        let now = Instant::now();
        for i in 0..5 {
            assert!(t.attempt("/join", &format!("10.0.0.{i}"), now).is_ok());
        }
        assert!(t.attempt("/join", "10.0.0.99", now).is_err());
        assert!(t.attempt("/setup", "10.0.0.99", now).is_ok());
    }

    #[test]
    fn the_forwarded_header_counts_only_behind_a_configured_proxy() {
        let peer: SocketAddr = "172.18.0.5:40000".parse().unwrap();
        // No proxy configured: the header is whatever the caller wrote; ignored.
        assert_eq!(client_of(Some(peer), Some("1.2.3.4"), 0), "172.18.0.5");
        // One proxy: the last entry is what the proxy saw.
        assert_eq!(
            client_of(Some(peer), Some("9.9.9.9, 203.0.113.7"), 1),
            "203.0.113.7"
        );
        // Two proxies (a load balancer and Caddy): the one before the last.
        assert_eq!(
            client_of(Some(peer), Some("9.9.9.9, 203.0.113.7, 10.0.0.2"), 2),
            "203.0.113.7"
        );
        // Fewer entries than proxies, or no header: fall back to the connection.
        assert_eq!(client_of(Some(peer), Some("203.0.113.7"), 2), "172.18.0.5");
        assert_eq!(client_of(Some(peer), None, 1), "172.18.0.5");
        assert_eq!(client_of(None, None, 0), "unknown");
    }
}
