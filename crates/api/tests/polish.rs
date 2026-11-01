//! Pre-pilot polish: version reporting, Settings → Users and the demo-school setup. Needs
//! PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

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
    db: Option<TestDb>,
    state: AppState,
    app: Router,
}

impl App {
    async fn new() -> Option<App> {
        let db = TestDb::new().await?;
        let state = AppState::new(db.pool.clone()).await.unwrap();
        let app = router(state.clone());
        Some(App {
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
    /// Returns (user id, login token).
    async fn patch(&self, p: &str, t: &str, b: Value) -> (StatusCode, Value) {
        self.call(Method::PATCH, p, Some(t), Some(b)).await
    }
    async fn user_with_role(
        &self,
        school_id: &str,
        username: &str,
        role: RoleCode,
    ) -> (String, String) {
        let sid: uuid::Uuid = school_id.parse().unwrap();
        let mut c = self.state.pool.acquire().await.unwrap();
        let u = users::create(
            &mut c,
            &self.state.hlc,
            Actor::system_for(sid),
            users::NewUser {
                school_id: sid,
                username: username.into(),
                full_name: username.into(),
                email: None,
                password_hash: hash_password("user-password-1").unwrap(),
            },
        )
        .await
        .unwrap();
        let r = roles::find_by_code(&mut c, sid, role.code()).await.unwrap();
        roles::assign(&mut c, &self.state.hlc, Actor::system_for(sid), u.id, &r)
            .await
            .unwrap();
        (
            u.id.to_string(),
            self.login(username, "user-password-1").await,
        )
    }

    /// Setup, admin login, year 2026-27 active, class 1 section A, class 2 section A.
    async fn seed(&self) -> (String, String, Value) {
        let (s, b) = self.call(Method::POST, "/setup", None, Some(json!({ "school_name": "Gulshan", "school_code": "KHI-MC", "academic_year_start_month": 4, "admin_full_name": "Ayesha", "admin_username": "ayesha", "admin_password": "admin-password-1", "backup_password": "backup-password-1" }))).await;
        assert_eq!(s, StatusCode::OK, "{b}");
        let school_id = b["school_id"].as_str().unwrap().to_string();
        let admin = self.login("ayesha", "admin-password-1").await;
        let (_, year) = self
            .post(
                "/academic-years",
                &admin,
                json!({ "start_year": 2026, "start_month": 4, "activate": true }),
            )
            .await;
        let (_, c1) = self
            .post("/classes", &admin, json!({ "name": "1", "level": 1 }))
            .await;
        let (_, c2) = self
            .post("/classes", &admin, json!({ "name": "2", "level": 2 }))
            .await;
        let (_, s1) = self
            .post(
                "/sections",
                &admin,
                json!({ "class_id": c1["id"], "academic_year_id": year["id"], "name": "A" }),
            )
            .await;
        let (_, s2) = self
            .post(
                "/sections",
                &admin,
                json!({ "class_id": c2["id"], "academic_year_id": year["id"], "name": "A" }),
            )
            .await;
        let (_, eng) = self
            .post(
                "/subjects",
                &admin,
                json!({ "name": "English", "code": "ENG" }),
            )
            .await;
        let (_, mth) = self
            .post(
                "/subjects",
                &admin,
                json!({ "name": "Mathematics", "code": "MTH" }),
            )
            .await;
        let (s, _) = self
            .put(
                &format!("/classes/{}/subjects", c1["id"].as_str().unwrap()),
                &admin,
                json!({ "subject_ids": [eng["id"], mth["id"]] }),
            )
            .await;
        assert_eq!(s, StatusCode::OK);
        self.put(
            &format!("/classes/{}/subjects", c2["id"].as_str().unwrap()),
            &admin,
            json!({ "subject_ids": [eng["id"]] }),
        )
        .await;
        (
            admin,
            school_id,
            json!({ "year": year["id"], "c1": c1["id"], "c2": c2["id"], "s1": s1["id"], "s2": s2["id"], "eng": eng["id"], "mth": mth["id"] }),
        )
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

#[tokio::test]
async fn health_and_features_report_the_release_version() {
    let app = app!();
    let (s, h) = app.call(Method::GET, "/health", None, None).await;
    assert_eq!(s, StatusCode::OK, "{h}");
    assert_eq!(h["version"], madrasa_api::VERSION);
    assert!(madrasa_api::VERSION.starts_with(env!("CARGO_PKG_VERSION")));
    let (s, f) = app.call(Method::GET, "/system/features", None, None).await;
    assert_eq!(s, StatusCode::OK, "{f}");
    assert_eq!(f["version"], madrasa_api::VERSION);
    app.finish().await;
}

#[tokio::test]
async fn users_page_lists_logins_with_staff_links_and_manages_them() {
    let app = app!();
    let (admin, school_id, _ids) = app.seed().await;
    let (s, st) = app
        .post(
            "/staff",
            &admin,
            json!({ "full_name": "Sana Malik", "email": "sana@example.com", "designation": "Teacher" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{st}");
    let staff_id = st["id"].as_str().unwrap().to_string();

    // Create a login from the Users page (same rules as the staff record).
    let accountant = app
        .user_with_role(&school_id, "acc", RoleCode::Accountant)
        .await
        .1;
    let (s, e) = app
        .post(
            "/users",
            &accountant,
            json!({ "staff_id": staff_id, "username": "sana", "password": "teacher-password-1", "role": "teacher" }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{e}");
    let (s, created) = app
        .post(
            "/users",
            &admin,
            json!({ "staff_id": staff_id, "username": "sana", "password": "teacher-password-1", "role": "teacher" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{created}");
    assert_eq!(
        (
            created["username"].as_str(),
            created["staff_name"].as_str(),
            created["roles"].clone()
        ),
        (Some("sana"), Some("Sana Malik"), json!(["teacher"]))
    );
    assert!(created["last_login_at"].is_null());
    let _teacher = app.login("sana", "teacher-password-1").await;

    // The list: the setup administrator (no staff record, logged in), the accountant, Sana.
    let (s, list) = app.get("/users", &admin).await;
    assert_eq!(s, StatusCode::OK, "{list}");
    let list = list.as_array().unwrap();
    let by = |u: &str| list.iter().find(|x| x["username"] == u).cloned().unwrap();
    let ayesha = by("ayesha");
    assert!(
        ayesha["staff_id"].is_null() && ayesha["last_login_at"].is_string(),
        "{ayesha}"
    );
    assert_eq!(ayesha["roles"], json!(["admin", "principal"]));
    let sana = by("sana");
    assert_eq!(sana["staff_id"], staff_id);
    assert!(sana["last_login_at"].is_string(), "{sana}");
    assert_eq!(sana["is_active"], true);
    let (s, _) = app.get("/users", &accountant).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Role change, disable, self-disable refused, reset.
    let uid = sana["id"].as_str().unwrap().to_string();
    let (s, u) = app
        .patch(
            &format!("/users/{uid}"),
            &admin,
            json!({ "role": "accountant", "is_active": false }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{u}");
    assert_eq!(
        (u["roles"].clone(), u["is_active"].as_bool()),
        (json!(["accountant"]), Some(false))
    );
    let (s, _) = app
        .call(
            Method::POST,
            "/auth/login",
            None,
            Some(json!({ "username": "sana", "password": "teacher-password-1" })),
        )
        .await;
    assert_ne!(s, StatusCode::OK, "disabled");
    let (s, e) = app
        .patch(
            &format!("/users/{}", ayesha["id"].as_str().unwrap()),
            &admin,
            json!({ "is_active": false }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    let (s, _) = app
        .patch(
            &format!("/users/{uid}"),
            &admin,
            json!({ "is_active": true }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app
        .post(
            &format!("/users/{uid}/password"),
            &admin,
            json!({ "password": "sana-new-pass-22" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let _ = app.login("sana", "sana-new-pass-22").await;
    // A user of another school (or an unknown id) is not found.
    let (s, _) = app
        .patch(
            &format!("/users/{}", uuid::Uuid::now_v7()),
            &admin,
            json!({ "is_active": true }),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    app.finish().await;
}

#[tokio::test]
async fn demo_school_setup_loads_sample_data_and_finishes_setup() {
    let app = app!();
    let (s, e) = app
        .call(
            Method::POST,
            "/setup/demo",
            None,
            Some(json!({ "admin_username": "eval", "admin_password": "short", "backup_password": "backup-password-1", "size": "small" })),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    let (s, b) = app
        .call(
            Method::POST,
            "/setup/demo",
            None,
            Some(json!({ "admin_username": "eval", "admin_password": "eval-password-1", "backup_password": "backup-password-1", "size": "small" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(b["recovery_key"].as_str().unwrap().len() > 20);
    let (_, status) = app.call(Method::GET, "/setup/status", None, None).await;
    assert_eq!(status["needs_setup"], false);
    let admin = app.login("eval", "eval-password-1").await;
    let (s, school) = app.get("/schools/current", &admin).await;
    assert_eq!(s, StatusCode::OK, "{school}");
    assert_eq!(school["is_demo"], true, "{school}");
    assert!(
        school["name"].as_str().unwrap().contains("DEMO"),
        "{school}"
    );
    let (_, page) = app.get("/students?page_size=1", &admin).await;
    assert_eq!(page["total"].as_i64(), Some(60), "{page}");
    let (_, staff) = app.get("/staff", &admin).await;
    assert_eq!(staff.as_array().unwrap().len(), 12);
    let (_, years) = app.get("/academic-years", &admin).await;
    assert_eq!(
        years
            .as_array()
            .unwrap()
            .iter()
            .filter(|y| y["is_active"] == true)
            .count(),
        1
    );
    let (_, users) = app.get("/users", &admin).await;
    assert!(users.as_array().unwrap().len() >= 13, "{users}");
    // Done once.
    let (s, _) = app
        .call(
            Method::POST,
            "/setup/demo",
            None,
            Some(json!({ "admin_username": "eval2", "admin_password": "eval-password-1", "backup_password": "backup-password-1", "size": "small" })),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    app.finish().await;
}
