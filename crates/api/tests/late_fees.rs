//! Automatic late fees API tests. Need PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

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
    async fn user_with_role(&self, school_id: &str, username: &str, role: RoleCode) -> String {
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
        self.login(username, "user-password-1").await
    }

    /// Setup, admin login, year 2026-27 active, class 1 section A, class 2 section A, heads, structure.
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
        let (s, tuition) = self
            .post(
                "/fees/heads",
                &admin,
                json!({ "name": "Tuition", "code": "TUI" }),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{tuition}");
        let (_, transport) = self
            .post(
                "/fees/heads",
                &admin,
                json!({ "name": "Transport", "code": "TRN", "position": 1 }),
            )
            .await;
        let (_, admission) = self
            .post(
                "/fees/heads",
                &admin,
                json!({ "name": "Admission", "code": "ADM", "is_recurring": false, "position": 2 }),
            )
            .await;
        let (s, st) = self.put("/fees/structure", &admin, json!({ "academic_year_id": year["id"], "class_id": c1["id"], "lines": [{ "fee_head_id": tuition["id"], "amount": "2500" }, { "fee_head_id": transport["id"], "amount": "800" }, { "fee_head_id": admission["id"], "amount": "5000" }] })).await;
        assert_eq!((s, st.as_array().unwrap().len()), (StatusCode::OK, 3));
        self.put("/fees/structure", &admin, json!({ "academic_year_id": year["id"], "class_id": c2["id"], "lines": [{ "fee_head_id": tuition["id"], "amount": "3333.33" }] })).await;
        (
            admin,
            school_id,
            json!({ "year": year["id"], "c1": c1["id"], "c2": c2["id"], "s1": s1["id"], "s2": s2["id"], "tuition": tuition["id"] }),
        )
    }

    async fn student(
        &self,
        admin: &str,
        name: &str,
        father: &str,
        section: &Value,
        guardian_cnic: Option<&str>,
    ) -> String {
        let mut body = json!({ "full_name": name, "father_name": father, "date_of_birth": "2015-01-01", "gender": "male", "admission_date": "2026-04-01", "section_id": section });
        if let Some(cnic) = guardian_cnic {
            body["guardians"] =
                json!([{ "full_name": father, "relationship": "father", "cnic": cnic }]);
        }
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
async fn late_fee_pass_adds_fees_once_keeps_waivers_and_is_audited() {
    let app = app!();
    let (admin, school_id, ids) = app.seed().await;
    let accountant = app
        .user_with_role(&school_id, "acc", RoleCode::Accountant)
        .await;
    let teacher = app
        .user_with_role(&school_id, "tch", RoleCode::Teacher)
        .await;
    let omar = app
        .student(&admin, "Omar Raza", "Tariq Raza", &ids["s2"], None)
        .await;
    let zain = app
        .student(&admin, "Zain Bilal", "Bilal Ahmed", &ids["s2"], None)
        .await;
    let (s, rep) = app
        .post(
            "/fees/vouchers/generate",
            &accountant,
            json!({ "month": "2026-04-01", "issue_date": "2026-04-01" }),
        )
        .await;
    assert_eq!(
        (s, rep["created"].as_u64()),
        (StatusCode::OK, Some(2)),
        "{rep}"
    );
    let voucher_of = |student: &str| {
        let app = &app;
        let token = &admin;
        let student = student.to_string();
        async move {
            let (_, list) = app
                .get(&format!("/fees/vouchers?student_id={student}"), token)
                .await;
            list[0].clone()
        }
    };
    let v_omar = voucher_of(&omar).await;
    let v_zain = voucher_of(&zain).await;

    // Needs fees.manage; a future date is refused; the due day itself adds nothing.
    let (s, e) = app.post("/fees/late-fees/run", &teacher, json!({})).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{e}");
    let (s, e) = app
        .post(
            "/fees/late-fees/run",
            &accountant,
            json!({ "as_of": "2999-01-01" }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("validation"))
    );
    let (s, r) = app
        .post(
            "/fees/late-fees/run",
            &accountant,
            json!({ "as_of": "2026-04-15" }),
        )
        .await;
    assert_eq!(
        (s, r["assessed"].as_u64(), r["total"].as_str()),
        (StatusCode::OK, Some(0), Some("0")),
        "{r}"
    );

    // The day after the due date both vouchers get 10%; a second pass adds nothing.
    let (s, r) = app
        .post(
            "/fees/late-fees/run",
            &accountant,
            json!({ "as_of": "2026-04-16" }),
        )
        .await;
    assert_eq!(
        (
            s,
            r["assessed"].as_u64(),
            r["total"].as_str(),
            r["as_of"].as_str()
        ),
        (StatusCode::OK, Some(2), Some("666.66"), Some("2026-04-16")),
        "{r}"
    );
    let (_, r) = app
        .post(
            "/fees/late-fees/run",
            &accountant,
            json!({ "as_of": "2026-04-17" }),
        )
        .await;
    assert_eq!(r["assessed"].as_u64(), Some(0));
    let (_, v) = app
        .get(
            &format!("/fees/vouchers/{}", v_omar["id"].as_str().unwrap()),
            &admin,
        )
        .await;
    assert_eq!(
        (
            v["late_fee"].as_str(),
            v["balance"].as_str(),
            v["status"].as_str()
        ),
        (Some("333.33"), Some("3666.66"), Some("unpaid")),
        "{v}"
    );

    // Defaulters and the ledger show the amount owed with the fee.
    let (_, defs) = app
        .get("/fees/defaulters?as_of=2026-04-17", &accountant)
        .await;
    let defs = defs.as_array().unwrap();
    assert_eq!(defs.len(), 2, "{defs:?}");
    assert!(defs.iter().all(|d| d["balance"] == "3666.66"), "{defs:?}");
    let (_, ledger) = app
        .get(&format!("/fees/students/{zain}/ledger"), &accountant)
        .await;
    assert_eq!(
        (
            ledger["totals"]["fines"].as_str(),
            ledger["totals"]["balance"].as_str()
        ),
        (Some("333.33"), Some("3666.66")),
        "{ledger}"
    );

    // A waiver stands through later passes.
    let (s, w) = app
        .post(
            &format!(
                "/fees/vouchers/{}/waive-late-fee",
                v_omar["id"].as_str().unwrap()
            ),
            &admin,
            json!({ "reason": "Hardship" }),
        )
        .await;
    assert_eq!(
        (s, w["late_fee"].as_str()),
        (StatusCode::OK, Some("0")),
        "{w}"
    );
    let (_, r) = app
        .post(
            "/fees/late-fees/run",
            &accountant,
            json!({ "as_of": "2026-05-01" }),
        )
        .await;
    assert_eq!(r["assessed"].as_u64(), Some(0));
    let (_, v) = app
        .get(
            &format!("/fees/vouchers/{}", v_omar["id"].as_str().unwrap()),
            &admin,
        )
        .await;
    assert_eq!(
        (
            v["late_fee"].as_str(),
            v["late_fee_waived"].as_bool(),
            v["balance"].as_str()
        ),
        (Some("0"), Some(true), Some("3333.33")),
        "{v}"
    );

    // Every automatic fee is in the audit log under its own action with the amount.
    let (s, log) = app.get("/audit?limit=100", &admin).await;
    assert_eq!(s, StatusCode::OK, "{log}");
    let fees: Vec<&Value> = log
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["action"] == "late_fee")
        .collect();
    assert_eq!(fees.len(), 2, "{log}");
    assert!(fees.iter().all(|e| e["entity"] == "fee_voucher"
        && e["new_value"]["late_fee"] == "333.33"
        && e["old_value"]["late_fee"] == "0"));
    assert!(fees.iter().any(|e| e["entity_id"] == v_zain["id"]));
    app.finish().await;
}
