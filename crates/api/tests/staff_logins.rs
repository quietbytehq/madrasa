//! Staff logins managed from the Staff screen: create, change role, disable, reset password,
//! audited and enforced in the backend. Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

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

    async fn student(&self, admin: &str, name: &str, section: &Value) -> String {
        let body = json!({ "full_name": name, "father_name": "Father", "date_of_birth": "2015-01-01", "gender": "male", "admission_date": "2026-04-01", "section_id": section });
        let (s, b) = self.post("/students", admin, body).await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        b["id"].as_str().unwrap().into()
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

/// What each login can and cannot do; a teacher's marks go through, fees and settings do not.
async fn probe(app: &App, token: &str, path: &str) -> StatusCode {
    app.get(path, token).await.0
}

#[tokio::test]
async fn admin_creates_disables_and_resets_staff_logins_with_roles_enforced() {
    let app = app!();
    let (admin, school_id, ids) = app.seed().await;
    let year = ids["year"].as_str().unwrap();
    let s1 = ids["s1"].clone();
    let ali = app.student(&admin, "Ali", &s1).await;
    let (s, list) = app
        .post(
            "/exams/assessments/defaults",
            &admin,
            json!({ "academic_year_id": year }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{list}");
    let test1 = list[0]["id"].clone();
    let (s, mm) = app
        .put("/exams/max-marks", &admin, json!({ "assessment_id": test1, "class_id": ids["c1"], "lines": [{ "subject_id": ids["eng"], "max_marks": "50" }] }))
        .await;
    assert_eq!(s, StatusCode::OK, "{mm}");

    // A staff member without a login.
    let (s, st) = app
        .post(
            "/staff",
            &admin,
            json!({ "full_name": "Sana Malik", "email": "sana@example.com", "designation": "Teacher" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{st}");
    let staff_id = st["id"].as_str().unwrap().to_string();
    assert!(st["username"].is_null());
    app.put(
        &format!("/staff/{staff_id}/assignments"),
        &admin,
        json!({ "assignments": [{ "section_id": s1, "subject_id": ids["eng"] }] }),
    )
    .await;

    // Only users.manage (principal, admin) may create logins: an accountant is refused.
    let accountant = app
        .user_with_role(&school_id, "acc", RoleCode::Accountant)
        .await
        .1;
    let (s, e) = app
        .post(
            &format!("/staff/{staff_id}/login"),
            &accountant,
            json!({ "username": "sana", "password": "teacher-password-1", "role": "teacher" }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{e}");
    // Validation: role, username, password.
    for bad in [
        json!({ "username": "sana", "password": "teacher-password-1", "role": "owner" }),
        json!({ "username": "s!", "password": "teacher-password-1", "role": "teacher" }),
        json!({ "username": "sana", "password": "short", "role": "teacher" }),
    ] {
        let (s, e) = app
            .post(&format!("/staff/{staff_id}/login"), &admin, bad)
            .await;
        assert_eq!(
            (s, e["error"]["code"].as_str()),
            (StatusCode::BAD_REQUEST, Some("validation")),
            "{e}"
        );
    }
    let (s, v) = app
        .post(
            &format!("/staff/{staff_id}/login"),
            &admin,
            json!({ "username": "Sana", "password": "teacher-password-1", "role": "teacher" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["username"], "sana");
    assert_eq!(v["roles"], json!(["teacher"]));
    assert!(v["user_id"].is_string());
    let (s, e) = app
        .post(
            &format!("/staff/{staff_id}/login"),
            &admin,
            json!({ "username": "sana2", "password": "teacher-password-1", "role": "teacher" }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "already has a login: {e}");

    // The teacher can enter marks for the assigned subject but not touch fees or settings.
    let teacher = app.login("sana", "teacher-password-1").await;
    let (s, sheet) = app
        .put("/exams/sheet", &teacher, json!({ "assessment_id": test1, "section_id": s1, "subject_id": ids["eng"], "marks": [{ "student_id": ali, "obtained": "41" }] }))
        .await;
    assert_eq!(s, StatusCode::OK, "{sheet}");
    assert_eq!(
        probe(&app, &teacher, "/fees/vouchers").await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        probe(&app, &teacher, "/fees/defaulters").await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        probe(&app, &teacher, "/settings/school").await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(probe(&app, &teacher, "/users").await, StatusCode::FORBIDDEN);
    let (s, _) = app
        .put("/fees/policy", &teacher, json!({ "late_fee_percent": "0" }))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app
        .patch("/schools/current", &teacher, json!({ "city": "X" }))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Role change to accountant: fees open up, marks close.
    let (s, v) = app
        .patch(
            &format!("/staff/{staff_id}/login"),
            &admin,
            json!({ "role": "accountant" }),
        )
        .await;
    assert_eq!(
        (s, v["roles"].clone()),
        (StatusCode::OK, json!(["accountant"]))
    );
    let as_acc = app.login("sana", "teacher-password-1").await;
    assert_eq!(probe(&app, &as_acc, "/fees/vouchers").await, StatusCode::OK);
    let (s, _) = app
        .put("/exams/sheet", &as_acc, json!({ "assessment_id": test1, "section_id": s1, "subject_id": ids["eng"], "marks": [{ "student_id": ali, "obtained": "42" }] }))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, v) = app
        .patch(
            &format!("/staff/{staff_id}/login"),
            &admin,
            json!({ "role": "teacher" }),
        )
        .await;
    assert_eq!(
        (s, v["roles"].clone()),
        (StatusCode::OK, json!(["teacher"]))
    );

    // Reset the password (principal may too): the old one stops working, the new one works.
    let principal = app
        .user_with_role(&school_id, "head", RoleCode::Principal)
        .await
        .1;
    let (s, _) = app
        .post(
            &format!("/staff/{staff_id}/login/password"),
            &principal,
            json!({ "password": "new-teacher-pass-2" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app
        .call(
            Method::POST,
            "/auth/login",
            None,
            Some(json!({ "username": "sana", "password": "teacher-password-1" })),
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let teacher = app.login("sana", "new-teacher-pass-2").await;

    // Disable: login refused; the account cannot disable itself; re-enable works.
    let (s, v) = app
        .patch(
            &format!("/staff/{staff_id}/login"),
            &admin,
            json!({ "is_active": false }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (s, e) = app
        .call(
            Method::POST,
            "/auth/login",
            None,
            Some(json!({ "username": "sana", "password": "new-teacher-pass-2" })),
        )
        .await;
    assert_ne!(s, StatusCode::OK, "{e}");
    let (s, _) = app
        .patch(
            &format!("/staff/{staff_id}/login"),
            &admin,
            json!({ "is_active": true }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let _ = teacher;
    let teacher = app.login("sana", "new-teacher-pass-2").await;
    let (s, _) = app
        .patch(
            &format!("/staff/{staff_id}/login"),
            &teacher,
            json!({ "is_active": false }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "teachers cannot manage logins");
    // An admin cannot disable their own account.
    let (_, me) = app.get("/auth/me", &admin).await;
    let (s, own) = app
        .post(
            "/staff",
            &admin,
            json!({ "full_name": "Ayesha", "user_id": me["user"]["id"] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{own}");
    let (s, e) = app
        .patch(
            &format!("/staff/{}/login", own["id"].as_str().unwrap()),
            &admin,
            json!({ "is_active": false }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");

    // Everything is in the audit log: user created, roles, password reset, disable.
    let (_, log) = app.get("/audit?limit=200", &admin).await;
    let actions: Vec<&str> = log
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["action"].as_str().unwrap())
        .collect();
    assert!(actions.contains(&"password_reset"), "{actions:?}");
    let user_entries = log
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["entity"] == "user" && e["action"] == "update")
        .count();
    assert!(user_entries >= 2, "disable and re-enable: {log}");
    assert!(
        log.as_array()
            .unwrap()
            .iter()
            .all(|e| e["new_value"].get("password_hash").is_none()),
        "hashes never reach the log"
    );
    app.finish().await;
}
