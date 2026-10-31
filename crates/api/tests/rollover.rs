//! M1 stage 7 rollover and dashboard API tests. Need PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

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
        (
            admin,
            school_id,
            json!({ "year": year["id"], "c1": c1["id"], "c2": c2["id"], "s1": s1["id"], "s2": s2["id"] }),
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

#[tokio::test]
async fn dashboard_figures_and_year_rollover() {
    let app = app!();
    let (admin, school_id, ids) = app.seed().await;
    let ali = app.student(&admin, "Ali", &ids["s1"]).await;
    let _sara = app.student(&admin, "Sara", &ids["s1"]).await;
    let zain = app.student(&admin, "Zain", &ids["s2"]).await;
    let (_, st) = app
        .post("/staff", &admin, json!({ "full_name": "Sana" }))
        .await;
    assert!(st["id"].is_string());

    // Fees: a structure, April vouchers, one payment today.
    let (_, tuition) = app
        .post(
            "/fees/heads",
            &admin,
            json!({ "name": "Tuition", "code": "TUI" }),
        )
        .await;
    app.put("/fees/structure", &admin, json!({ "academic_year_id": ids["year"], "class_id": ids["c1"], "lines": [{ "fee_head_id": tuition["id"], "amount": "2500" }] })).await;
    app.put("/fees/structure", &admin, json!({ "academic_year_id": ids["year"], "class_id": ids["c2"], "lines": [{ "fee_head_id": tuition["id"], "amount": "3000" }] })).await;
    let (s, rep) = app
        .post(
            "/fees/vouchers/generate",
            &admin,
            json!({ "month": "2026-04-01" }),
        )
        .await;
    assert_eq!((s, rep["created"].clone()), (StatusCode::OK, json!(3)));
    let (_, ledger) = app
        .get(&format!("/fees/students/{ali}/ledger"), &admin)
        .await;
    let voucher_id = ledger["vouchers"][0]["id"].as_str().unwrap().to_string();
    let today = chrono::Utc::now().date_naive().to_string();
    let (s, _) = app
        .post(
            &format!("/fees/vouchers/{voucher_id}/payments"),
            &admin,
            json!({ "amount": "1000", "method": "cash", "paid_on": today }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);

    let (s, dash) = app.get("/dashboard", &admin).await;
    assert_eq!(s, StatusCode::OK, "{dash}");
    assert_eq!(
        (
            dash["students_active"].clone(),
            dash["staff_active"].clone()
        ),
        (json!(3), json!(1))
    );
    assert_eq!(dash["fees"]["collected_this_month"], "1000.00");
    assert_eq!(
        dash["fees"]["defaulters"], 3,
        "April vouchers are overdue today"
    );
    // Late fees are assessed when a voucher is touched (Stage 2 rule): Ali's voucher got its
    // 250 at payment time; Sara's and Zain's have not been touched yet.
    // Ali 1,500 + 250, Sara 2,500, Zain 3,000 = 7,250 outstanding.
    assert_eq!(dash["fees"]["outstanding"], "7250.00");
    let (_, teacher) = app
        .user_with_role(&school_id, "sana", RoleCode::Teacher)
        .await;
    let (s, dash) = app.get("/dashboard", &teacher).await;
    assert_eq!(
        (s, dash["fees"].clone(), dash["staff_active"].clone()),
        (StatusCode::OK, Value::Null, json!(1)),
        "teachers read staff but not fees"
    );

    // Rollover preview and apply.
    let (s, p) = app.get("/academic-years/rollover/preview", &admin).await;
    assert_eq!(s, StatusCode::OK, "{p}");
    assert_eq!(
        (
            p["new_year_name"].as_str(),
            p["promoted"].clone(),
            p["graduating"].clone()
        ),
        (Some("2027-28"), json!(2), json!(1))
    );
    assert_eq!(p["students_with_balance"], 3);
    let (s, _) = app
        .post("/academic-years/rollover", &teacher, json!({}))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, r) = app
        .post(
            "/academic-years/rollover",
            &admin,
            json!({ "retain": [], "graduate": [] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(
        (
            r["promoted"].clone(),
            r["graduated"].clone(),
            r["sections_created"].clone(),
            r["structure_lines_copied"].clone()
        ),
        (json!(2), json!(1), json!(1), json!(2))
    );
    assert_eq!(r["new_year"]["is_active"], true);
    let (_, years) = app.get("/academic-years", &admin).await;
    assert_eq!(years.as_array().unwrap().len(), 2);
    let (_, zain_detail) = app.get(&format!("/students/{zain}"), &admin).await;
    assert_eq!(zain_detail["status"], "graduated");
    let (_, ali_detail) = app.get(&format!("/students/{ali}"), &admin).await;
    assert_eq!(ali_detail["current"]["class_name"], "2");
    let (s, e) = app
        .post("/academic-years/rollover", &admin, json!({}))
        .await;
    // Rolling the brand-new year over again opens yet another year; it is allowed but the
    // preview says the year is young. Here the second call simply succeeds.
    assert!(s == StatusCode::OK || s == StatusCode::CONFLICT, "{e}");
    app.finish().await;
}
