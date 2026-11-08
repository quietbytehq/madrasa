//! Standard stage 8: directory (LDAP) login against an in-process stub directory. Lite
//! refuses the settings and keeps local logins; Standard saves and tests the settings,
//! creates accounts on first directory login with the mapped role, refuses wrong
//! passwords (with the lockout), disabled accounts and unmapped users, keeps local
//! accounts working, and refuses directory logins again back on Lite. Needs PostgreSQL via
//! `MADRASA_TEST_DATABASE_URL`.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use madrasa_api::directory::StubDirectory;
use madrasa_api::{router, AppState};
use madrasa_core::directory::DirectoryUser;
use madrasa_db::testing::TestDb;
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

struct App {
    db: Option<TestDb>,
    app: Router,
}

impl App {
    async fn new() -> Option<App> {
        let db = TestDb::new().await?;
        let state = AppState::new(db.pool.clone())
            .await
            .unwrap()
            .with_directory(Arc::new(stub()));
        let app = router(state.clone());
        Some(App { db: Some(db), app })
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
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        (status, value)
    }
    async fn get(&self, p: &str, t: &str) -> (StatusCode, Value) {
        self.call(Method::GET, p, Some(t), None).await
    }
    async fn post(&self, p: &str, t: &str, b: Value) -> (StatusCode, Value) {
        self.call(Method::POST, p, Some(t), Some(b)).await
    }
    async fn put(&self, p: &str, t: &str, b: Value) -> (StatusCode, Value) {
        self.call(Method::PUT, p, Some(t), Some(b)).await
    }
    async fn login(&self, u: &str, p: &str) -> String {
        let (s, b) = self
            .call(
                Method::POST,
                "/auth/login",
                None,
                Some(json!({ "username": u, "password": p })),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        b["token"].as_str().unwrap().into()
    }
    async fn finish(mut self) {
        if let Some(db) = self.db.take() {
            db.cleanup().await;
        }
    }
}

macro_rules! app {
    () => {
        match App::new().await {
            Some(a) => a,
            None => return,
        }
    };
}

const BIND_DN: &str = "CN=madrasa,OU=Service,DC=school,DC=local";
const TEACHERS: &str = "CN=Teachers,OU=Groups,DC=school,DC=local";
const PRINCIPALS: &str = "CN=Principals,OU=Groups,DC=school,DC=local";

fn person(username: &str, name: &str, groups: &[&str], disabled: bool) -> DirectoryUser {
    DirectoryUser {
        dn: format!("CN={name},OU=Staff,DC=school,DC=local"),
        username: username.into(),
        full_name: name.into(),
        email: Some(format!("{username}@school.local")),
        groups: groups.iter().map(|g| g.to_string()).collect(),
        disabled,
    }
}

/// The stub directory every test talks to.
fn stub() -> StubDirectory {
    StubDirectory::new(BIND_DN, "bind-secret")
        .with_user(
            "zara-pass-1",
            person("zara", "Zara Qureshi", &[TEACHERS], false),
        )
        .with_user(
            "head-pass-1",
            person("sadia", "Sadia Malik", &[TEACHERS, PRINCIPALS], false),
        )
        .with_user(
            "gone-pass-1",
            person("gone", "Gone Person", &[TEACHERS], true),
        )
        .with_user(
            "none-pass-1",
            person(
                "nobody",
                "No Group",
                &["CN=Other,DC=school,DC=local"],
                false,
            ),
        )
}

fn settings_body(enabled: bool, bind_password: Option<&str>) -> Value {
    let mut v = json!({
        "enabled": enabled,
        "url": "ldaps://dc1.school.local:636",
        "bind_dn": BIND_DN,
        "user_base_dn": "OU=Staff,DC=school,DC=local",
        "group_roles": [
            { "group": PRINCIPALS, "role": "principal" },
            { "group": TEACHERS, "role": "teacher" }
        ],
        "login_hint": "Use your school network account."
    });
    if let Some(p) = bind_password {
        v["bind_password"] = json!(p);
    }
    v
}

async fn setup_school(app: &App) -> String {
    let (s, b) = app
        .call(
            Method::POST,
            "/setup",
            None,
            Some(json!({
                "school_name": "Gulshan High", "school_code": "KHI-AD", "academic_year_start_month": 4,
                "admin_full_name": "Ayesha", "admin_username": "ayesha", "admin_password": "admin-password-1",
                "backup_password": "backup-password-1"
            })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    app.login("ayesha", "admin-password-1").await
}

async fn try_login(app: &App, u: &str, p: &str) -> (StatusCode, Value) {
    app.call(
        Method::POST,
        "/auth/login",
        None,
        Some(json!({ "username": u, "password": p })),
    )
    .await
}

#[tokio::test]
async fn lite_has_no_directory_login() {
    let app = app!();
    let admin = setup_school(&app).await;
    let (s, b) = app.get("/settings/directory", &admin).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    assert_eq!(b["error"]["code"], "feature_unavailable");
    let (s, b) = app.call(Method::GET, "/auth/options", None, None).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["directory_login"], false);
    let (s, _) = try_login(&app, "zara", "zara-pass-1").await;
    assert_eq!(
        s,
        StatusCode::UNAUTHORIZED,
        "directory users are unknown on Lite"
    );
    app.finish().await;
}

#[tokio::test]
async fn standard_logs_directory_users_in_with_mapped_roles() {
    let app = app!();
    let admin = setup_school(&app).await;
    let (s, b) = app
        .put(
            "/settings/edition",
            &admin,
            json!({ "edition": "standard" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");

    // Settings: TLS required, the bind password before enabling, unknown roles refused.
    let mut plain = settings_body(true, Some("bind-secret"));
    plain["url"] = json!("ldap://dc1.school.local");
    let (s, b) = app.put("/settings/directory", &admin, plain).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
    let (s, b) = app
        .put("/settings/directory", &admin, settings_body(true, None))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "no bind password yet: {b}");
    let mut bad_role = settings_body(true, Some("bind-secret"));
    bad_role["default_role"] = json!("wizard");
    let (s, b) = app.put("/settings/directory", &admin, bad_role).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
    // Test connection with a wrong bind password, then the right one and a user lookup.
    let mut t = settings_body(true, Some("wrong"));
    t["username"] = json!("sadia");
    let (s, b) = app.post("/settings/directory/test", &admin, t).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["bind_ok"], false);
    let mut t = settings_body(true, Some("bind-secret"));
    t["username"] = json!("sadia");
    let (s, b) = app.post("/settings/directory/test", &admin, t).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["bind_ok"], true);
    assert_eq!(b["role"], "principal", "the first mapping wins: {b}");
    assert_eq!(b["user"]["full_name"], "Sadia Malik");
    // Save for real; the password is stored sealed and never echoed.
    let (s, b) = app
        .put(
            "/settings/directory",
            &admin,
            settings_body(true, Some("bind-secret")),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["enabled"], true);
    assert_eq!(b["has_bind_password"], true);
    assert!(b.get("bind_password").is_none());
    assert!(b["roles"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r == "librarian"));
    let (s, b) = app.get("/settings/directory", &admin).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["url"], "ldaps://dc1.school.local:636");
    // The test endpoint reuses the stored password when none is sent.
    let (s, b) = app
        .post(
            "/settings/directory/test",
            &admin,
            settings_body(true, None),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["bind_ok"], true);
    // The login screen learns about it without a token.
    let (s, b) = app.call(Method::GET, "/auth/options", None, None).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["directory_login"], true);
    assert_eq!(b["directory_hint"], "Use your school network account.");

    // First directory login creates the account with the mapped role.
    let (s, b) = try_login(&app, "Zara", "zara-pass-1").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["user"]["full_name"], "Zara Qureshi");
    assert_eq!(b["user"]["auth_source"], "directory");
    assert_eq!(b["user"]["roles"], json!(["teacher"]));
    let zara = b["token"].as_str().unwrap().to_string();
    let (s, me) = app.get("/auth/me", &zara).await;
    assert_eq!(s, StatusCode::OK, "{me}");
    assert!(me["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p == "attendance.manage"));
    let (s, b) = try_login(&app, "sadia", "head-pass-1").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["user"]["roles"], json!(["principal"]));
    // The account list shows the source; a directory account has no local password reset.
    let (s, users) = app.get("/users", &admin).await;
    assert_eq!(s, StatusCode::OK, "{users}");
    let z = users
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["username"] == "zara")
        .unwrap();
    assert_eq!(z["auth_source"], "directory");
    assert_eq!(z["full_name"], "Zara Qureshi");

    // Wrong password, disabled account, unmapped groups, unknown user.
    let (s, b) = try_login(&app, "zara", "wrong").await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "{b}");
    let (s, b) = try_login(&app, "gone", "gone-pass-1").await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    assert_eq!(b["error"]["code"], "account_inactive");
    let (s, b) = try_login(&app, "nobody", "none-pass-1").await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    let (s, b) = try_login(&app, "stranger", "x").await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "{b}");
    // Lockout after repeated wrong passwords on a known directory account.
    let mut last = StatusCode::OK;
    for _ in 0..12 {
        let (s, _) = try_login(&app, "zara", "wrong").await;
        last = s;
        if s == StatusCode::LOCKED {
            break;
        }
    }
    assert_eq!(last, StatusCode::LOCKED);
    let (s, _) = try_login(&app, "zara", "zara-pass-1").await;
    assert_eq!(s, StatusCode::LOCKED, "the right password does not unlock");
    // The local administrator still logs in with a password.
    let (s, b) = try_login(&app, "ayesha", "admin-password-1").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["user"]["auth_source"], "local");
    // A default role lets unmapped users in.
    let mut with_default = settings_body(true, None);
    with_default["default_role"] = json!("teacher");
    let (s, b) = app.put("/settings/directory", &admin, with_default).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = try_login(&app, "nobody", "none-pass-1").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["user"]["roles"], json!(["teacher"]));
    // Switched off: directory accounts cannot log in, local ones can.
    let (s, b) = app
        .put("/settings/directory", &admin, settings_body(false, None))
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, _) = try_login(&app, "sadia", "head-pass-1").await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = try_login(&app, "ayesha", "admin-password-1").await;
    assert_eq!(s, StatusCode::OK);

    // Back on Lite the settings are refused and directory logins stay refused.
    let (s, b) = app
        .put("/settings/directory", &admin, settings_body(true, None))
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, _) = app
        .put("/settings/edition", &admin, json!({ "edition": "lite" }))
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app.get("/settings/directory", &admin).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = try_login(&app, "sadia", "head-pass-1").await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = try_login(&app, "ayesha", "admin-password-1").await;
    assert_eq!(s, StatusCode::OK);
    app.finish().await;
}
