//! Stage 1: first-run setup, login and lockout, hub users and permissions, the audit log.

mod common;

use axum::http::{Method, StatusCode};
use common::{config, ADMIN_PASSWORD, SETUP_TOKEN};
use madrasa_nexus::{store, HubState};
use serde_json::json;

#[tokio::test]
async fn health_and_setup_run_once_with_the_token() {
    let hub = hub!();
    let (s, h) = hub.call(Method::GET, "/health", None, None).await;
    assert_eq!(s, StatusCode::OK, "{h}");
    assert_eq!(h["status"], "ok");
    assert_eq!(h["service"], "madrasa-nexus");
    assert_eq!(h["database"], "ok");
    assert_eq!(h["version"], env!("CARGO_PKG_VERSION"));
    let (_, st) = hub.call(Method::GET, "/setup/status", None, None).await;
    assert_eq!(st["needs_setup"], true);

    let body = |token: &str, password: &str, username: &str| {
        json!({ "setup_token": token, "hub_name": "Gulshan Education Network",
                "admin_username": username, "admin_full_name": "Head Office", "admin_password": password })
    };
    // The token is required, and the usual rules apply to the account.
    let (s, e) = hub
        .call(
            Method::POST,
            "/setup",
            None,
            Some(body("wrong", ADMIN_PASSWORD, "head.office")),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("bad_setup_token")),
        "{e}"
    );
    for (password, username) in [("short", "head.office"), (ADMIN_PASSWORD, "h!")] {
        let (s, e) = hub
            .call(
                Method::POST,
                "/setup",
                None,
                Some(body(SETUP_TOKEN, password, username)),
            )
            .await;
        assert_eq!(
            (s, e["error"]["code"].as_str()),
            (StatusCode::BAD_REQUEST, Some("validation")),
            "{e}"
        );
    }
    // Nothing protected works before setup.
    let (s, _) = hub.call(Method::GET, "/users", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    let (s, b) = hub
        .call(
            Method::POST,
            "/setup",
            None,
            Some(body(SETUP_TOKEN, ADMIN_PASSWORD, "Head.Office")),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(b["admin_user_id"].is_string());
    // Once only: the token is spent even though it was right.
    let (s, e) = hub
        .call(
            Method::POST,
            "/setup",
            None,
            Some(body(SETUP_TOKEN, ADMIN_PASSWORD, "second")),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("setup_already_done")),
        "{e}"
    );
    let (_, st) = hub.call(Method::GET, "/setup/status", None, None).await;
    assert_eq!(st["needs_setup"], false);
    assert_eq!(st["hub_name"], "Gulshan Education Network");

    // The first administrator holds every hub permission and logs in case-insensitively.
    let admin = hub.login("HEAD.OFFICE", ADMIN_PASSWORD).await;
    let (s, me) = hub.get("/auth/me", &admin).await;
    assert_eq!(s, StatusCode::OK, "{me}");
    assert_eq!(me["user"]["username"], "head.office");
    assert_eq!(
        me["user"]["permissions"],
        json!(["nexus.admin", "nexus.reports", "nexus.escrow"])
    );
    assert_eq!(me["hub_name"], "Gulshan Education Network");
    assert!(me["user"].get("password_hash").is_none(), "{me}");
    hub.finish().await;
}

#[tokio::test]
async fn a_generated_setup_token_is_returned_once_and_works() {
    let hub = hub!();
    // Start again as `main` would with no NEXUS_SETUP_TOKEN.
    let (state, generated) = HubState::new(hub.state.pool.clone(), &config(&hub.db.url, None))
        .await
        .unwrap();
    let token = generated.expect("a token is generated while setup is open");
    assert!(token.len() >= 40, "32 random bytes, base64url");
    assert!(state.setup_token_matches(&token));
    assert!(!state.setup_token_matches(SETUP_TOKEN));
    state.close_setup();
    assert!(!state.setup_token_matches(&token), "spent after setup");
    // After setup no token is generated at all.
    hub.setup().await;
    let (_, generated) = HubState::new(hub.state.pool.clone(), &config(&hub.db.url, None))
        .await
        .unwrap();
    assert!(generated.is_none());
    hub.finish().await;
}

#[tokio::test]
async fn logins_lock_after_repeated_failures_and_logout_ends_the_session() {
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
    let bad = json!({ "username": "sana", "password": "wrong-password" });
    let max = hub.state.lockout.max_attempts;
    for attempt in 1..=max {
        let (s, e) = hub
            .call(Method::POST, "/auth/login", None, Some(bad.clone()))
            .await;
        if attempt < max {
            assert_eq!(
                (s, e["error"]["code"].as_str()),
                (StatusCode::UNAUTHORIZED, Some("invalid_credentials")),
                "attempt {attempt}: {e}"
            );
        } else {
            assert_eq!(
                (s, e["error"]["code"].as_str()),
                (StatusCode::LOCKED, Some("account_locked")),
                "{e}"
            );
        }
    }
    // Locked even with the right password; an unknown user looks like a wrong password.
    let (s, _) = hub
        .call(
            Method::POST,
            "/auth/login",
            None,
            Some(json!({ "username": "sana", "password": "viewer-password-1" })),
        )
        .await;
    assert_eq!(s, StatusCode::LOCKED);
    let (s, e) = hub
        .call(
            Method::POST,
            "/auth/login",
            None,
            Some(json!({ "username": "nobody", "password": "x" })),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::UNAUTHORIZED, Some("invalid_credentials"))
    );
    // A password reset by the administrator clears the lock.
    let id = u["id"].as_str().unwrap();
    let (s, b) = hub
        .post(
            &format!("/users/{id}/password"),
            &admin,
            json!({ "password": "viewer-password-2" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let sana = hub.login("sana", "viewer-password-2").await;
    let (s, _) = hub.get("/auth/me", &sana).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = hub.post("/auth/logout", &sana, json!({})).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = hub.get("/auth/me", &sana).await;
    assert_eq!(
        s,
        StatusCode::UNAUTHORIZED,
        "the token is dead after logout"
    );
    // Only token hashes are stored.
    let stored: Vec<String> = sqlx::query_scalar("SELECT token_hash FROM hub_sessions")
        .fetch_all(&hub.state.pool)
        .await
        .unwrap();
    assert!(!stored.is_empty());
    assert!(stored
        .iter()
        .all(|h| h.len() == 64 && h != &admin && h != &sana));
    hub.finish().await;
}

#[tokio::test]
async fn permissions_are_enforced_and_one_administrator_always_remains() {
    let hub = hub!();
    let admin = hub.setup().await;
    // Validation: permissions, username, password, duplicates.
    for bad in [
        json!({ "username": "sana", "full_name": "Sana", "password": "viewer-password-1", "permissions": ["students.read"] }),
        json!({ "username": "s!", "full_name": "Sana", "password": "viewer-password-1", "permissions": [] }),
        json!({ "username": "sana", "full_name": " ", "password": "viewer-password-1", "permissions": [] }),
        json!({ "username": "sana", "full_name": "Sana", "password": "short", "permissions": [] }),
    ] {
        let (s, e) = hub.post("/users", &admin, bad).await;
        assert_eq!(
            (s, e["error"]["code"].as_str()),
            (StatusCode::BAD_REQUEST, Some("validation")),
            "{e}"
        );
    }
    let (s, sana) = hub
        .post("/users", &admin, json!({ "username": "Sana", "full_name": "Sana Malik", "email": "SANA@example.org",
                                        "password": "viewer-password-1", "permissions": ["nexus.reports"] }))
        .await;
    assert_eq!(s, StatusCode::CREATED, "{sana}");
    assert_eq!(sana["username"], "sana");
    assert_eq!(sana["email"], "sana@example.org");
    let (s, e) = hub
        .post("/users", &admin, json!({ "username": "SANA", "full_name": "Other", "password": "viewer-password-1", "permissions": [] }))
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("conflict")),
        "{e}"
    );
    let sana_id = sana["id"].as_str().unwrap().to_string();

    // A reports-only user cannot manage users or read the audit log.
    let viewer = hub.login("sana", "viewer-password-1").await;
    for (method, path, body) in [
        (Method::GET, "/users".to_string(), None),
        (Method::GET, "/audit".to_string(), None),
        (
            Method::POST,
            "/users".to_string(),
            Some(
                json!({ "username": "x.y.z", "full_name": "X", "password": "viewer-password-1", "permissions": [] }),
            ),
        ),
        (
            Method::PATCH,
            format!("/users/{sana_id}"),
            Some(json!({ "permissions": ["nexus.admin"] })),
        ),
    ] {
        let (s, e) = hub.call(method, &path, Some(&viewer), body).await;
        assert_eq!(
            (s, e["error"]["code"].as_str()),
            (StatusCode::FORBIDDEN, Some("forbidden")),
            "{path}: {e}"
        );
    }
    // ...but may change their own password, not anyone else's.
    let (_, users) = hub.get("/users", &admin).await;
    let admin_id = users
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["username"] == "head.office")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (s, _) = hub
        .post(
            &format!("/users/{admin_id}/password"),
            &viewer,
            json!({ "password": "stolen-password-1" }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = hub
        .post(
            &format!("/users/{sana_id}/password"),
            &viewer,
            json!({ "password": "viewer-password-2" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = hub.get("/auth/me", &viewer).await;
    assert_eq!(
        s,
        StatusCode::UNAUTHORIZED,
        "a password change ends existing sessions"
    );

    // The only administrator cannot be disabled or stripped of nexus.admin.
    for body in [
        json!({ "is_active": false }),
        json!({ "permissions": ["nexus.reports"] }),
    ] {
        let (s, e) = hub.patch(&format!("/users/{admin_id}"), &admin, body).await;
        assert_eq!(
            (s, e["error"]["code"].as_str()),
            (StatusCode::CONFLICT, Some("conflict")),
            "{e}"
        );
    }
    // With a second administrator it can.
    let (s, b) = hub
        .patch(
            &format!("/users/{sana_id}"),
            &admin,
            json!({ "permissions": ["nexus.escrow", "nexus.admin"] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["permissions"], json!(["nexus.admin", "nexus.escrow"]));
    let sana_token = hub.login("sana", "viewer-password-2").await;
    let (s, b) = hub
        .patch(
            &format!("/users/{admin_id}"),
            &sana_token,
            json!({ "is_active": false }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["is_active"], false);
    // A disabled account loses its session at once and cannot log in.
    let (s, _) = hub.get("/users", &admin).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, e) = hub
        .call(
            Method::POST,
            "/auth/login",
            None,
            Some(json!({ "username": "head.office", "password": ADMIN_PASSWORD })),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("account_inactive")),
        "{e}"
    );
    let (s, e) = hub
        .patch(
            "/users/00000000-0000-7000-8000-000000000000",
            &sana_token,
            json!({ "is_active": false }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::NOT_FOUND, Some("not_found")),
        "{e}"
    );
    hub.finish().await;
}

#[tokio::test]
async fn every_change_is_audited_without_secrets_and_the_log_is_append_only() {
    let hub = hub!();
    let admin = hub.setup().await;
    let (_, u) = hub
        .post("/users", &admin, json!({ "username": "sana", "full_name": "Sana Malik", "password": "viewer-password-1", "permissions": ["nexus.reports"] }))
        .await;
    let id = u["id"].as_str().unwrap();
    hub.patch(
        &format!("/users/{id}"),
        &admin,
        json!({ "full_name": "Sana M. Malik" }),
    )
    .await;
    hub.post(
        &format!("/users/{id}/password"),
        &admin,
        json!({ "password": "viewer-password-2" }),
    )
    .await;
    let (s, log) = hub.get("/audit?page_size=50", &admin).await;
    assert_eq!(s, StatusCode::OK, "{log}");
    let rows = log.as_array().unwrap();
    let actions: Vec<(&str, &str)> = rows
        .iter()
        .rev()
        .map(|r| (r["action"].as_str().unwrap(), r["entity"].as_str().unwrap()))
        .collect();
    assert_eq!(
        actions,
        [
            ("create", "hub_user"),
            ("setup", "hub"),
            ("create", "hub_user"),
            ("update", "hub_user"),
            ("password_reset", "hub_user")
        ]
    );
    let update = rows.iter().find(|r| r["action"] == "update").unwrap();
    assert_eq!(update["old_value"]["full_name"], "Sana Malik");
    assert_eq!(update["new_value"]["full_name"], "Sana M. Malik");
    assert!(update["actor_user_id"].is_string());
    let text = log.to_string();
    assert!(
        !text.contains("argon2"),
        "no password hash in the audit log"
    );
    assert!(
        !text.contains("viewer-password"),
        "no password in the audit log"
    );

    // The database refuses to change or delete audit rows.
    for sql in [
        "UPDATE hub_audit_log SET action = 'x'",
        "DELETE FROM hub_audit_log",
    ] {
        let err = sqlx::query(sql).execute(&hub.state.pool).await.unwrap_err();
        assert!(err.to_string().contains("append-only"), "{err}");
    }
    let (s, e) = hub.get("/audit?page=abc", &admin).await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("validation")),
        "{e}"
    );
    hub.finish().await;
}

#[tokio::test]
async fn hub_tables_live_in_their_own_schema() {
    let hub = hub!();
    let schemas: Vec<(String, String)> = sqlx::query_as(
        "SELECT table_schema::text, table_name::text FROM information_schema.tables
         WHERE table_name LIKE 'hub\\_%' OR table_name = '_sqlx_migrations' ORDER BY 2",
    )
    .fetch_all(&hub.state.pool)
    .await
    .unwrap();
    let names: Vec<&str> = schemas.iter().map(|(_, t)| t.as_str()).collect();
    assert_eq!(
        names,
        [
            "_sqlx_migrations",
            "hub_audit_log",
            "hub_escrow_deposits",
            "hub_escrow_releases",
            "hub_invitations",
            "hub_parent_access_codes",
            "hub_parent_links",
            "hub_parent_sessions",
            "hub_parents",
            "hub_schools",
            "hub_sessions",
            "hub_settings",
            "hub_users"
        ]
    );
    assert!(
        schemas.iter().all(|(s, _)| s == store::HUB_SCHEMA),
        "{schemas:?}"
    );
    // Migrating again is a no-op.
    store::migrate(&hub.state.pool).await.unwrap();
    hub.finish().await;
}

/// The hub serves the built web console next to its API (stage 7).
#[tokio::test]
async fn the_console_is_served_beside_the_api_with_safe_headers() {
    use axum::body::Body;
    use axum::http::{header, Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let hub = hub!();
    let dir = std::env::temp_dir().join(format!("madrasa-console-test-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(
        dir.join("index.html"),
        "<!doctype html><title>Madrasa Nexus</title>",
    )
    .unwrap();
    std::fs::write(
        dir.join("assets/console-abc123.js"),
        "console.log('console')",
    )
    .unwrap();

    let get = |router: axum::Router, path: &'static str| async move {
        let res = router
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        (status, headers, String::from_utf8_lossy(&body).into_owned())
    };
    let head = |h: &axum::http::HeaderMap, name: header::HeaderName| {
        h.get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string()
    };

    let mut state = hub.state.clone();
    state.console_dir = Some(dir.clone());
    let with_console = madrasa_nexus::router(state);

    // The page, at the root and at any console address (the browser routes those).
    for path in ["/", "/schools", "/reports/anything"] {
        let (status, headers, body) = get(with_console.clone(), path).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert!(
            body.contains("<title>Madrasa Nexus</title>"),
            "{path}: {body}"
        );
        assert!(
            head(&headers, header::CONTENT_TYPE).starts_with("text/html"),
            "{path}"
        );
        assert_eq!(
            head(&headers, header::CACHE_CONTROL),
            "no-cache",
            "{path}: a new version shows at once"
        );
        let csp = head(&headers, header::CONTENT_SECURITY_POLICY);
        assert!(
            csp.contains("default-src 'self'") && csp.contains("frame-ancestors 'none'"),
            "{csp}"
        );
        assert_eq!(head(&headers, header::X_CONTENT_TYPE_OPTIONS), "nosniff");
        assert_eq!(head(&headers, header::X_FRAME_OPTIONS), "DENY");
        assert_eq!(head(&headers, header::REFERRER_POLICY), "no-referrer");
    }
    // Built files are cached for good (their names carry a hash); a missing one is a 404,
    // not the page.
    let (status, headers, body) = get(with_console.clone(), "/assets/console-abc123.js").await;
    assert_eq!(
        (status, body.as_str()),
        (StatusCode::OK, "console.log('console')")
    );
    assert!(head(&headers, header::CACHE_CONTROL).contains("immutable"));
    let (status, _, _) = get(with_console.clone(), "/assets/nothing.js").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // Files outside the console folder are never served.
    let (status, _, body) = get(with_console.clone(), "/assets/../../etc/passwd").await;
    assert!(
        status == StatusCode::NOT_FOUND || body.contains("<title>Madrasa Nexus</title>"),
        "{status} {body}"
    );
    assert!(!body.contains("root:"));

    // The API is still the API: JSON answers, and an unknown address is a JSON 404.
    let (status, headers, body) = get(with_console.clone(), "/api/nexus/v1/health").await;
    assert_eq!(status, StatusCode::OK);
    assert!(head(&headers, header::CONTENT_TYPE).starts_with("application/json"));
    assert!(body.contains("\"status\":\"ok\""));
    let (status, _, body) = get(with_console.clone(), "/api/nexus/v1/no-such-thing").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("\"code\":\"not_found\""), "{body}");
    let (status, _, _) = get(with_console, "/api/nexus/v1/users").await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "the console adds no way round sign-in"
    );

    // Without the console's files the hub serves the API only and says so.
    let (status, _, body) = get(hub.router.clone(), "/").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        body.contains("The web console is not installed here"),
        "{body}"
    );
    let (status, _, _) = get(hub.router.clone(), "/api/nexus/v1/health").await;
    assert_eq!(status, StatusCode::OK);

    let _ = std::fs::remove_dir_all(&dir);
    hub.finish().await;
}
