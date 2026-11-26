//! Authentication and sessions (security review of v0.5.1). Needs PostgreSQL via
//! `MADRASA_TEST_DATABASE_URL`.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use madrasa_api::{router, AppState};
use madrasa_core::password::hash_password;
use madrasa_core::permissions::RoleCode;
use madrasa_db::testing::TestDb;
use madrasa_db::{roles, users, Actor};
use serde_json::{json, Value};
use tower::ServiceExt;

struct App {
    db: TestDb,
    state: AppState,
    app: Router,
    school_id: uuid::Uuid,
    admin: String,
}

impl App {
    /// A school with its administrator `ayesha` signed in.
    async fn new() -> Option<App> {
        let db = TestDb::new().await?;
        let state = AppState::new(db.pool.clone()).await.unwrap();
        let mut app = App {
            app: router(state.clone()),
            state,
            db,
            school_id: uuid::Uuid::nil(),
            admin: String::new(),
        };
        let (s, b) = app
            .call(Method::POST, "/setup", None, Some(json!({
                "school_name": "Gulshan", "school_code": "KHI-MC", "academic_year_start_month": 4,
                "admin_full_name": "Ayesha", "admin_username": "ayesha",
                "admin_password": "admin-password-1", "backup_password": "backup-password-1"
            })))
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        app.school_id = b["school_id"].as_str().unwrap().parse().unwrap();
        app.admin = app.token("ayesha", "admin-password-1").await;
        Some(app)
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/v1{path}"));
        if let Some(t) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// `(status, error code, message)` of a login attempt.
    async fn try_login(&self, username: &str, password: &str) -> (StatusCode, String, String) {
        let (s, b) = self
            .call(
                Method::POST,
                "/auth/login",
                None,
                Some(json!({ "username": username, "password": password })),
            )
            .await;
        (
            s,
            b["error"]["code"].as_str().unwrap_or("").to_string(),
            b["error"]["message"].as_str().unwrap_or("").to_string(),
        )
    }

    async fn token(&self, username: &str, password: &str) -> String {
        let (s, b) = self
            .call(
                Method::POST,
                "/auth/login",
                None,
                Some(json!({ "username": username, "password": password })),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        b["token"].as_str().unwrap().to_string()
    }

    async fn me(&self, token: &str) -> StatusCode {
        self.call(Method::GET, "/auth/me", Some(token), None)
            .await
            .0
    }

    async fn user(&self, username: &str, role: RoleCode) -> uuid::Uuid {
        let mut c = self.state.pool.acquire().await.unwrap();
        let u = users::create(
            &mut c,
            &self.state.hlc,
            Actor::system_for(self.school_id),
            users::NewUser {
                school_id: self.school_id,
                username: username.into(),
                full_name: username.into(),
                email: None,
                password_hash: hash_password("user-password-1").unwrap(),
            },
        )
        .await
        .unwrap();
        let r = roles::find_by_code(&mut c, self.school_id, role.code())
            .await
            .unwrap();
        roles::assign(
            &mut c,
            &self.state.hlc,
            Actor::system_for(self.school_id),
            u.id,
            &r,
        )
        .await
        .unwrap();
        u.id
    }

    async fn finish(self) {
        self.state.pool.close().await;
        self.db.cleanup().await;
    }
}

macro_rules! app {
    () => {
        match App::new().await {
            Some(app) => app,
            None => {
                eprintln!("skipped: MADRASA_TEST_DATABASE_URL is not set");
                return;
            }
        }
    };
}

/// Nothing the login says tells whether a username is an account: a name that is nobody's
/// gets the same answers, attempt by attempt, lock included; a disabled account answers like
/// any other until the password is right.
#[tokio::test]
async fn login_answers_do_not_tell_which_usernames_exist() {
    let app = app!();
    app.user("bilal", RoleCode::Teacher).await;
    let max = app.state.lockout.max_attempts;
    for attempt in 1..=max + 1 {
        let real = app.try_login("bilal", "wrong-password-1").await;
        let nobody = app
            .try_login("nobody-by-this-name", "wrong-password-1")
            .await;
        assert_eq!(real, nobody, "attempt {attempt}");
        let expected = if attempt < max {
            (StatusCode::UNAUTHORIZED, "invalid_credentials")
        } else {
            (StatusCode::LOCKED, "account_locked")
        };
        assert_eq!((real.0, real.1.as_str()), expected, "attempt {attempt}");
    }
    // Capitals and spaces do not make it a new name with five fresh tries.
    assert_eq!(
        app.try_login(" Nobody-By-This-Name ", "x").await.1,
        "account_locked"
    );

    // A disabled account: "disabled" only once the password is right.
    let off = app.user("retired", RoleCode::Teacher).await;
    let (s, b) = app
        .call(
            Method::PATCH,
            &format!("/users/{off}"),
            Some(&app.admin),
            Some(json!({ "is_active": false })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let wrong = app.try_login("retired", "wrong-password-1").await;
    assert_eq!(
        wrong,
        app.try_login("another-nobody", "wrong-password-1").await
    );
    assert_eq!(wrong.1, "invalid_credentials");
    assert_eq!(
        app.try_login("retired", "user-password-1").await.1,
        "account_inactive"
    );
    app.finish().await;
}

/// A new password ends the sessions opened with the old one: a reset by the administrator
/// signs the user out everywhere, and a user changing their own password keeps only the
/// session they are using.
#[tokio::test]
async fn a_new_password_ends_the_sessions_of_the_old_one() {
    let app = app!();
    let bilal = app.user("bilal", RoleCode::Teacher).await;
    let laptop = app.token("bilal", "user-password-1").await;
    let phone = app.token("bilal", "user-password-1").await;
    assert_eq!(app.me(&laptop).await, StatusCode::OK);

    // The administrator resets it (bilal forgot it, or someone else knows it).
    let (s, b) = app
        .call(
            Method::POST,
            &format!("/users/{bilal}/password"),
            Some(&app.admin),
            Some(json!({ "password": "reset-password-22" })),
        )
        .await;
    assert!(s.is_success(), "{s} {b}");
    assert_eq!(
        app.me(&laptop).await,
        StatusCode::UNAUTHORIZED,
        "old sessions are over"
    );
    assert_eq!(app.me(&phone).await, StatusCode::UNAUTHORIZED);
    assert_eq!(
        app.me(&app.admin).await,
        StatusCode::OK,
        "nobody else is signed out"
    );
    assert_eq!(
        app.try_login("bilal", "user-password-1").await.1,
        "invalid_credentials"
    );

    // bilal signs in with the password he was given and sets his own.
    let laptop = app.token("bilal", "reset-password-22").await;
    let phone = app.token("bilal", "reset-password-22").await;
    let change = |token: String, current: &'static str, new: &'static str| {
        let app = &app;
        async move {
            app.call(
                Method::POST,
                "/auth/password",
                Some(&token),
                Some(json!({ "current_password": current, "new_password": new })),
            )
            .await
        }
    };
    let (s, b) = change(laptop.clone(), "not-the-password", "my-own-password-33").await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
    assert!(
        b["error"]["message"]
            .as_str()
            .unwrap()
            .contains("current password"),
        "{b}"
    );
    let (s, b) = change(laptop.clone(), "reset-password-22", "short").await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
    let (s, b) = change(laptop.clone(), "reset-password-22", "reset-password-22").await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "the same password again: {b}");
    let (s, b) = change(laptop.clone(), "reset-password-22", "my-own-password-33").await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{b}");
    assert_eq!(
        app.me(&laptop).await,
        StatusCode::OK,
        "the session that asked goes on"
    );
    assert_eq!(
        app.me(&phone).await,
        StatusCode::UNAUTHORIZED,
        "the other one is over"
    );
    assert_eq!(
        app.try_login("bilal", "reset-password-22").await.1,
        "invalid_credentials"
    );
    app.token("bilal", "my-own-password-33").await;

    // The administrator no longer knows it, and the change is in the audit log.
    let audited: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action = 'password_reset' AND entity_id = $1",
    )
    .bind(bilal)
    .fetch_one(&app.state.pool)
    .await
    .unwrap();
    assert_eq!(audited, 2);
    // Nobody changes a password without a session.
    let (s, _) = app
        .call(
            Method::POST,
            "/auth/password",
            None,
            Some(json!({ "current_password": "a", "new_password": "b" })),
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // Guessing the current password through an open session locks the account like the
    // login does.
    for _ in 0..app.state.lockout.max_attempts {
        change(laptop.clone(), "a-guess-at-it-00", "whatever-else-44").await;
    }
    let (s, b) = change(laptop.clone(), "my-own-password-33", "whatever-else-44").await;
    assert_eq!(s, StatusCode::LOCKED, "{b}");
    app.finish().await;
}

/// Logging out ends the session; a session not used for twelve hours is over; and however
/// much it is used, a session ends seven days after it was started.
#[tokio::test]
async fn sessions_end_on_logout_when_idle_and_after_seven_days() {
    let app = app!();
    let token = app.token("ayesha", "admin-password-1").await;
    assert_eq!(app.me(&token).await, StatusCode::OK);
    let (s, _) = app
        .call(Method::POST, "/auth/logout", Some(&token), None)
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(app.me(&token).await, StatusCode::UNAUTHORIZED);

    // Idle: the sliding twelve hours ran out.
    let idle = app.token("ayesha", "admin-password-1").await;
    let hash = madrasa_core::session_token::hash(&idle);
    sqlx::query(
        "UPDATE sessions SET expires_at = now() - interval '1 minute' WHERE token_hash = $1",
    )
    .bind(&hash)
    .execute(&app.state.pool)
    .await
    .unwrap();
    assert_eq!(app.me(&idle).await, StatusCode::UNAUTHORIZED);

    // In constant use, started just under and just over seven days ago.
    let busy = app.token("ayesha", "admin-password-1").await;
    let hash = madrasa_core::session_token::hash(&busy);
    let started = |minutes_ago: i32| {
        let pool = app.state.pool.clone();
        let hash = hash.clone();
        async move {
            sqlx::query(
                "UPDATE sessions SET created_at = now() - make_interval(mins => $2),
                    expires_at = now() + interval '12 hours' WHERE token_hash = $1",
            )
            .bind(hash)
            .bind(minutes_ago)
            .execute(&pool)
            .await
            .unwrap();
        }
    };
    started(7 * 24 * 60 - 60).await;
    assert_eq!(app.me(&busy).await, StatusCode::OK);
    started(7 * 24 * 60 + 1).await;
    assert_eq!(app.me(&busy).await, StatusCode::UNAUTHORIZED);

    // Tokens are kept as hashes only: nothing in the table is a token someone could use.
    let stored: Vec<String> = sqlx::query_scalar("SELECT token_hash FROM sessions")
        .fetch_all(&app.state.pool)
        .await
        .unwrap();
    assert!(stored
        .iter()
        .all(|h| h.len() == 64 && h != &busy && h != &idle));
    app.finish().await;
}

/// Only the app itself (packaged, or its development server on this computer) is told it may
/// read answers across origins.
#[tokio::test]
async fn only_the_app_may_read_answers_from_a_browser() {
    let app = app!();
    let allowed = |origin: &'static str| {
        let router = app.app.clone();
        async move {
            let res = router
                .oneshot(
                    Request::builder()
                        .uri("/api/v1/health")
                        .header(header::ORIGIN, origin)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            res.headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        }
    };
    for origin in [
        "tauri://localhost",
        "http://tauri.localhost",
        "http://localhost:1420",
    ] {
        assert_eq!(allowed(origin).await.as_deref(), Some(origin));
    }
    for origin in [
        "https://example.com",
        "http://192.168.1.20",
        "http://localhost.example.com",
    ] {
        assert_eq!(allowed(origin).await, None, "{origin}");
    }
    // The app may also read the name a download gives itself (else every file it saves is
    // called "download").
    let res = app
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .header(header::ORIGIN, "tauri://localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.headers()
            .get(header::ACCESS_CONTROL_EXPOSE_HEADERS)
            .and_then(|v| v.to_str().ok()),
        Some("content-disposition")
    );
    app.finish().await;
}
