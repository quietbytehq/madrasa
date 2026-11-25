//! The hub's doors (security review of v0.5.1): sign-in answers that do not tell which names
//! exist, a limit on attempts, and the headers every answer carries. Tenant isolation is in
//! `schools.rs` (`two_schools_with_identical_data_never_see_each_other`), parent isolation in
//! `parent_data.rs` (`a_parent_reaches_their_own_children_and_nobody_elses`).
//! Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

mod common;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use common::Hub;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

macro_rules! hub {
    () => {
        match Hub::new().await {
            Some(hub) => hub,
            None => {
                eprintln!("skipped: MADRASA_TEST_DATABASE_URL is not set");
                return;
            }
        }
    };
}

/// A username that is nobody's is answered, counted and locked like a wrong password on a
/// real account: before, only real accounts ever said "locked", so five tries told an
/// attacker which usernames exist.
#[tokio::test]
async fn sign_in_answers_do_not_tell_which_usernames_exist() {
    let hub = hub!();
    let admin = hub.setup().await;
    let (s, u) = hub
        .post(
            "/users",
            &admin,
            json!({ "username": "sana", "full_name": "Sana Malik", "password": "viewer-password-1",
                    "permissions": ["nexus.reports"] }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{u}");
    let max = hub.state.lockout.max_attempts;
    for attempt in 1..=max + 1 {
        let mut answers = Vec::new();
        for name in ["sana", "nobody-by-this-name"] {
            let (s, e) = hub
                .call(
                    Method::POST,
                    "/auth/login",
                    None,
                    Some(json!({ "username": name, "password": "wrong-password" })),
                )
                .await;
            answers.push((
                s,
                e["error"]["code"].as_str().unwrap_or("").to_string(),
                e["error"]["message"].as_str().unwrap_or("").to_string(),
            ));
        }
        assert_eq!(answers[0], answers[1], "attempt {attempt}");
        let expected = if attempt < max {
            (StatusCode::UNAUTHORIZED, "invalid_credentials")
        } else {
            (StatusCode::LOCKED, "account_locked")
        };
        assert_eq!(
            (answers[0].0, answers[0].1.as_str()),
            expected,
            "attempt {attempt}"
        );
    }
    hub.finish().await;
}

/// One address may not keep trying: after its attempts for the window the door answers 429
/// with `Retry-After`, for the login, the parent login, access codes, join codes and setup.
#[tokio::test]
async fn too_many_attempts_from_one_address_are_refused_for_a_while() {
    let mut hub = hub!();
    hub.setup().await;
    // A small limit so the test stays quick; the default is 60 per five minutes.
    hub.state.throttle = Arc::new(madrasa_nexus::throttle::Throttle::new(
        4,
        1000,
        Duration::from_secs(300),
    ));
    hub.router = madrasa_nexus::router(hub.state.clone());

    let knock = |path: &'static str, client: &'static str, body: serde_json::Value| {
        let router = hub.router.clone();
        async move {
            let mut req = Request::builder()
                .method(Method::POST)
                .uri(format!("{}{path}", madrasa_nexus::API_PREFIX))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap();
            let peer: std::net::SocketAddr = client.parse().unwrap();
            req.extensions_mut()
                .insert(axum::extract::ConnectInfo(peer));
            let res = router.oneshot(req).await.unwrap();
            (
                res.status(),
                res.headers().get(header::RETRY_AFTER).cloned(),
            )
        }
    };
    let doors: [(&str, serde_json::Value); 5] = [
        (
            "/auth/login",
            json!({ "username": "x", "password": "wrong-password" }),
        ),
        (
            "/parent/login",
            json!({ "phone": "03001234567", "password": "wrong-password" }),
        ),
        (
            "/parent/activate/check",
            json!({ "code": "AAAA-BBBB-CCCC" }),
        ),
        ("/join", json!({})),
        ("/setup", json!({})),
    ];
    for (door, body) in doors {
        for attempt in 1..=4 {
            let (status, _) = knock(door, "203.0.113.7:50000", body.clone()).await;
            assert_ne!(
                status,
                StatusCode::TOO_MANY_REQUESTS,
                "{door} attempt {attempt}"
            );
        }
        let (status, retry) = knock(door, "203.0.113.7:50001", body.clone()).await;
        assert_eq!(
            status,
            StatusCode::TOO_MANY_REQUESTS,
            "{door}: the fifth attempt"
        );
        let wait: u64 = retry.unwrap().to_str().unwrap().parse().unwrap();
        assert!((1..=300).contains(&wait), "{door}: Retry-After {wait}");
        // Another address is not held up by it.
        let (status, _) = knock(door, "198.51.100.9:50000", body.clone()).await;
        assert_ne!(
            status,
            StatusCode::TOO_MANY_REQUESTS,
            "{door}: another address"
        );
    }
    // Reading is never counted.
    for _ in 0..10 {
        let res = hub
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("{}/health", madrasa_nexus::API_PREFIX))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
    hub.finish().await;
}

/// Every answer of the hub carries the browser-side protections; over HTTPS (the public
/// address says so) it also tells browsers never to fall back to HTTP.
#[tokio::test]
async fn every_answer_carries_the_security_headers() {
    let hub = hub!();
    let res = hub
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("{}/health", madrasa_nexus::API_PREFIX))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let h = res.headers();
    let get = |name: &str| {
        h.get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    };
    assert!(
        get("content-security-policy").contains("default-src 'self'"),
        "{}",
        get("content-security-policy")
    );
    assert!(get("content-security-policy").contains("frame-ancestors 'none'"));
    assert_eq!(get("x-content-type-options"), "nosniff");
    assert_eq!(get("x-frame-options"), "DENY");
    assert_eq!(get("referrer-policy"), "no-referrer");
    assert!(get("permissions-policy").contains("camera=()"));
    assert_eq!(get("cross-origin-opener-policy"), "same-origin");
    assert_eq!(get("strict-transport-security"), "max-age=31536000");
    // No cross-origin access to the console's API (only the parent app's routes allow it).
    assert_eq!(get("access-control-allow-origin"), "");

    // A hub reached over plain HTTP (a local test) must not send HSTS.
    let mut state = hub.state.clone();
    state.public_url = Some("http://localhost:4780".into());
    let res = madrasa_nexus::router(state)
        .oneshot(
            Request::builder()
                .uri(format!("{}/health", madrasa_nexus::API_PREFIX))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(res.headers().get("strict-transport-security").is_none());
    hub.finish().await;
}
