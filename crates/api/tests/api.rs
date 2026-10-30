//! End-to-end API tests through the router. Need PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use madrasa_api::{router, AppState};
use madrasa_core::edition::Edition;
use madrasa_core::password::hash_password;
use madrasa_core::permissions::{Permission, RoleCode};
use madrasa_db::testing::TestDb;
use madrasa_db::{roles, settings, users, Actor};
use serde_json::{json, Value};
use tower::ServiceExt;

struct TestApp {
    db: Option<TestDb>,
    state: AppState,
    app: Router,
}

impl TestApp {
    async fn new() -> Option<TestApp> {
        let db = TestDb::new().await?;
        let state = AppState::new(db.pool.clone()).await.unwrap();
        let app = router(state.clone());
        Some(TestApp {
            db: Some(db),
            state,
            app,
        })
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
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, value)
    }

    async fn get(&self, path: &str, token: Option<&str>) -> (StatusCode, Value) {
        self.call(Method::GET, path, token, None).await
    }

    async fn post(&self, path: &str, token: Option<&str>, body: Value) -> (StatusCode, Value) {
        self.call(Method::POST, path, token, Some(body)).await
    }

    fn setup_body() -> Value {
        json!({
            "school_name": "Gulshan High School",
            "school_code": "ghs01",
            "city": "Karachi",
            "academic_year_start_month": 4,
            "currency": "PKR",
            "admin_full_name": "Ayesha Khan",
            "admin_username": "ayesha",
            "admin_password": "admin-password-1",
            "backup_password": "backup-password-1"
        })
    }

    /// Run setup and log the admin in; returns (admin token, school id).
    async fn setup_and_login(&self) -> (String, String) {
        let (status, body) = self.post("/setup", None, Self::setup_body()).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let school_id = body["school_id"].as_str().unwrap().to_string();
        let token = self.login("ayesha", "admin-password-1").await;
        (token, school_id)
    }

    async fn login(&self, username: &str, password: &str) -> String {
        let (status, body) = self
            .post(
                "/auth/login",
                None,
                json!({ "username": username, "password": password }),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["token"].as_str().unwrap().to_string()
    }

    /// Create a user with one built-in role directly in the database.
    async fn create_user_with_role(
        &self,
        school_id: &str,
        username: &str,
        password: &str,
        role: RoleCode,
    ) {
        let school_id: uuid::Uuid = school_id.parse().unwrap();
        let mut conn = self.state.pool.acquire().await.unwrap();
        let user = users::create(
            &mut conn,
            &self.state.hlc,
            Actor::system_for(school_id),
            users::NewUser {
                school_id,
                username: username.into(),
                full_name: username.into(),
                email: None,
                password_hash: hash_password(password).unwrap(),
            },
        )
        .await
        .unwrap();
        let role = roles::find_by_code(&mut conn, school_id, role.code())
            .await
            .unwrap();
        roles::assign(
            &mut conn,
            &self.state.hlc,
            Actor::system_for(school_id),
            user.id,
            &role,
        )
        .await
        .unwrap();
    }

    async fn finish(mut self) {
        if let Some(db) = self.db.take() {
            db.cleanup().await;
        }
    }
}

macro_rules! app {
    () => {
        match TestApp::new().await {
            Some(a) => a,
            None => return,
        }
    };
}

#[tokio::test]
async fn health_reports_ok_and_database() {
    let app = app!();
    let (status, body) = app.get("/health", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["database"], "ok");
    assert!(body["version"].as_str().is_some());
    app.finish().await;
}

#[tokio::test]
async fn setup_runs_once_and_validates_input() {
    let app = app!();
    let (status, body) = app.get("/setup/status", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["needs_setup"], true);

    // Validation.
    let mut bad = TestApp::setup_body();
    bad["admin_password"] = json!("short");
    let (status, body) = app.post("/setup", None, bad).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "validation");
    let mut bad = TestApp::setup_body();
    bad["academic_year_start_month"] = json!(9);
    assert_eq!(
        app.post("/setup", None, bad).await.0,
        StatusCode::BAD_REQUEST
    );
    let mut bad = TestApp::setup_body();
    bad["school_code"] = json!("not a code!");
    assert_eq!(
        app.post("/setup", None, bad).await.0,
        StatusCode::BAD_REQUEST
    );
    let (status, body) = app.post("/setup", None, json!({ "nope": 1 })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    // Success returns a recovery key once.
    let (status, body) = app.post("/setup", None, TestApp::setup_body()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let key = body["recovery_key"].as_str().unwrap();
    assert_eq!(key.split('-').count(), 8);
    assert!(madrasa_core::backup::normalize_recovery_key(key).is_some());

    let (_, body) = app.get("/setup/status", None).await;
    assert_eq!(body["needs_setup"], false);
    let (status, body) = app.post("/setup", None, TestApp::setup_body()).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "setup_already_done");

    // Stored key material unlocks with the recovery key and never contains it.
    let mut conn = app.state.pool.acquire().await.unwrap();
    let school = madrasa_db::schools::current(&mut conn)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(school.code, "GHS01");
    assert!(!school.is_demo);
    let stored = settings::get(&mut conn, Some(school.id), settings::KEY_BACKUP_KEYS)
        .await
        .unwrap()
        .unwrap();
    assert!(!stored.to_string().contains(key));
    let set: madrasa_core::backup::BackupKeySet = serde_json::from_value(stored).unwrap();
    madrasa_core::backup::unlock_with_recovery_key(&set, key).unwrap();
    madrasa_core::backup::unlock_with_password(&set, "backup-password-1").unwrap();
    drop(conn);
    app.finish().await;
}

#[tokio::test]
async fn login_me_logout_and_bad_credentials() {
    let app = app!();
    let (token, _) = app.setup_and_login().await;

    let (status, body) = app.get("/auth/me", Some(&token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["username"], "ayesha");
    assert_eq!(body["user"]["roles"], json!(["admin", "principal"]));
    let perms = body["permissions"].as_array().unwrap();
    assert!(perms.contains(&json!("users.manage")));
    assert!(!perms.contains(&json!("nexus.admin")));
    assert!(body["user"].get("password_hash").is_none());

    let (status, _) = app.get("/auth/me", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = app.get("/auth/me", Some("garbage")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, body) = app
        .post(
            "/auth/login",
            None,
            json!({ "username": "ayesha", "password": "wrong" }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "invalid_credentials");
    let (status, _) = app
        .post(
            "/auth/login",
            None,
            json!({ "username": "nobody", "password": "wrong" }),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = app.post("/auth/logout", Some(&token), json!({})).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = app.get("/auth/me", Some(&token)).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "token must be dead after logout"
    );
    app.finish().await;
}

#[tokio::test]
async fn account_locks_after_repeated_failures() {
    let app = app!();
    app.setup_and_login().await;
    for i in 0..4 {
        let (status, body) = app
            .post(
                "/auth/login",
                None,
                json!({ "username": "AYESHA", "password": "wrong" }),
            )
            .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "attempt {i}: {body}");
    }
    let (status, body) = app
        .post(
            "/auth/login",
            None,
            json!({ "username": "ayesha", "password": "wrong" }),
        )
        .await;
    assert_eq!(status, StatusCode::LOCKED, "{body}");
    assert_eq!(body["error"]["code"], "account_locked");
    // Even the correct password is refused while locked.
    let (status, _) = app
        .post(
            "/auth/login",
            None,
            json!({ "username": "ayesha", "password": "admin-password-1" }),
        )
        .await;
    assert_eq!(status, StatusCode::LOCKED);

    // Lockout is audited.
    let mut conn = app.state.pool.acquire().await.unwrap();
    let school = madrasa_db::schools::current(&mut conn)
        .await
        .unwrap()
        .unwrap();
    let recent = madrasa_db::audit::list_recent(&mut conn, Some(school.id), 50)
        .await
        .unwrap();
    assert!(recent.iter().any(|e| e.action == "lockout"));
    assert_eq!(
        recent.iter().filter(|e| e.action == "login_failed").count(),
        5
    );

    // Expire the lock manually and log in again; counter resets.
    sqlx::query("UPDATE users SET locked_until = now() - interval '1 minute'")
        .execute(&mut *conn)
        .await
        .unwrap();
    drop(conn);
    app.login("ayesha", "admin-password-1").await;
    let mut conn = app.state.pool.acquire().await.unwrap();
    let user = madrasa_db::users::find_by_username(&mut conn, school.id, "ayesha")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(user.failed_login_count, 0);
    assert!(user.locked_until.is_none());
    drop(conn);
    app.finish().await;
}

#[tokio::test]
async fn teacher_cannot_reach_admin_endpoints() {
    let app = app!();
    let (admin_token, school_id) = app.setup_and_login().await;
    app.create_user_with_role(
        &school_id,
        "teacher1",
        "teacher-password",
        RoleCode::Teacher,
    )
    .await;
    let teacher_token = app.login("teacher1", "teacher-password").await;

    let (status, body) = app.get("/users", Some(&teacher_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "forbidden");
    let (status, _) = app.get("/audit", Some(&teacher_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Teachers may read the school; admins may list users and audit.
    let (status, body) = app.get("/schools/current", Some(&teacher_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["code"], "GHS01");
    let (status, body) = app.get("/users", Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 2);
    let (status, body) = app.get("/audit?limit=5", Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 5);

    // Inactive accounts cannot log in.
    let mut conn = app.state.pool.acquire().await.unwrap();
    let sid: uuid::Uuid = school_id.parse().unwrap();
    let teacher = users::find_by_username(&mut conn, sid, "teacher1")
        .await
        .unwrap()
        .unwrap();
    users::update(
        &mut conn,
        &app.state.hlc,
        Actor::system_for(sid),
        teacher.id,
        users::UserUpdate {
            is_active: Some(false),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    drop(conn);
    let (status, body) = app
        .post(
            "/auth/login",
            None,
            json!({ "username": "teacher1", "password": "teacher-password" }),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "account_inactive");
    // ...and an existing session of a deactivated user stops working.
    let (status, _) = app.get("/auth/me", Some(&teacher_token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    app.finish().await;
}

#[tokio::test]
async fn edition_flags_gate_features_and_endpoints() {
    let app = app!();
    let (status, body) = app.get("/system/features", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["edition"], "lite");
    let features = body["features"].as_array().unwrap();
    assert!(features.contains(&json!("students")));
    assert!(!features.contains(&json!("nexus_hub")));

    let (admin_token, school_id) = app.setup_and_login().await;
    let (status, body) = app.get("/nexus/status", Some(&admin_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "feature_unavailable");

    // Switch the instance to Nexus: the feature is on, but the permission is still required.
    let mut conn = app.state.pool.acquire().await.unwrap();
    settings::set_edition(&mut conn, &app.state.hlc, Actor::SYSTEM, Edition::Nexus)
        .await
        .unwrap();
    drop(conn);
    assert_eq!(app.state.reload_edition().await.unwrap(), Edition::Nexus);
    let (_, body) = app.get("/system/features", None).await;
    assert_eq!(body["edition"], "nexus");
    assert!(body["features"]
        .as_array()
        .unwrap()
        .contains(&json!("nexus_hub")));

    let (status, body) = app.get("/nexus/status", Some(&admin_token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "forbidden");

    // Grant nexus.admin to the principal role; a new session sees it.
    let sid: uuid::Uuid = school_id.parse().unwrap();
    let mut conn = app.state.pool.acquire().await.unwrap();
    let principal = roles::find_by_code(&mut conn, sid, "principal")
        .await
        .unwrap();
    roles::grant(
        &mut conn,
        &app.state.hlc,
        Actor::system_for(sid),
        &principal,
        Permission::NexusAdmin,
    )
    .await
    .unwrap();
    drop(conn);
    let (status, body) = app.get("/nexus/status", Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "not_configured");
    app.finish().await;
}
