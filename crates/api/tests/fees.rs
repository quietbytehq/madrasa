//! M1 stage 2 fees API tests. Need PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

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
async fn setup_policy_and_permissions() {
    let app = app!();
    let (admin, school_id, ids) = app.seed().await;
    let (s, p) = app.get("/fees/policy", &admin).await;
    assert_eq!(
        (
            s,
            p["sibling_discount_percent"].as_str(),
            p["late_fee_percent"].as_str(),
            p["due_day"].as_i64()
        ),
        (StatusCode::OK, Some("10"), Some("10"), Some(15))
    );
    let (s, e) = app.put("/fees/policy", &admin, json!({ "sibling_discount_percent": "150", "sibling_rule": "all", "late_fee_percent": "10", "due_day": 15 })).await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("validation"))
    );
    let (s, p) = app.put("/fees/policy", &admin, json!({ "sibling_discount_percent": "15", "sibling_rule": "second_child_onwards", "late_fee_percent": "5", "due_day": 10 })).await;
    assert_eq!(
        (s, p["sibling_rule"].as_str()),
        (StatusCode::OK, Some("second_child_onwards"))
    );

    // Accountant collects but cannot change setup; teacher cannot see fees at all.
    let accountant = app
        .user_with_role(&school_id, "acc", RoleCode::Accountant)
        .await;
    let teacher = app
        .user_with_role(&school_id, "tch", RoleCode::Teacher)
        .await;
    let (s, _) = app.get("/fees/heads", &accountant).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app
        .post(
            "/fees/heads",
            &accountant,
            json!({ "name": "X", "code": "X" }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.put("/fees/policy", &accountant, json!({ "sibling_discount_percent": "10", "sibling_rule": "all", "late_fee_percent": "10", "due_day": 15 })).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.get("/fees/heads", &teacher).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app
        .post(
            "/fees/vouchers/generate",
            &teacher,
            json!({ "month": "2026-04-01" }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app
        .get(
            &format!(
                "/fees/structure?academic_year_id={}",
                ids["year"].as_str().unwrap()
            ),
            &accountant,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    app.finish().await;
}

#[tokio::test]
async fn generate_collect_waive_reverse_ledger_defaulters_collections() {
    let app = app!();
    let (admin, school_id, ids) = app.seed().await;
    let accountant = app
        .user_with_role(&school_id, "acc", RoleCode::Accountant)
        .await;
    let teacher = app
        .user_with_role(&school_id, "tch", RoleCode::Teacher)
        .await;
    // Ali (class 1) and Sara (class 2) are siblings; Omar (class 2) is alone with a 50% scholarship.
    let ali = app
        .student(
            &admin,
            "Ali Khan",
            "Ahmed Khan",
            &ids["s1"],
            Some("42101-7654321-1"),
        )
        .await;
    let sara = app
        .student(
            &admin,
            "Sara Khan",
            "Ahmed Khan",
            &ids["s2"],
            Some("42101-7654321-1"),
        )
        .await;
    let omar = app
        .student(&admin, "Omar Raza", "Tariq Raza", &ids["s2"], None)
        .await;
    let (s, e) = app
        .post(
            &format!("/fees/students/{omar}/concessions"),
            &accountant,
            json!({ "kind": "scholarship", "percent": "50", "reason": "Merit" }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{e}");
    let (s, e) = app
        .post(
            &format!("/fees/students/{omar}/concessions"),
            &admin,
            json!({ "kind": "scholarship", "percent": "50", "reason": "" }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    let (s, conc) = app.post(&format!("/fees/students/{omar}/concessions"), &admin, json!({ "kind": "scholarship", "percent": "50", "reason": "Merit", "valid_from": "2026-04-01" })).await;
    assert_eq!(s, StatusCode::CREATED, "{conc}");
    assert!(conc["approved_by"].is_string());

    // Generation (accountant may), month outside the year refused, idempotent.
    let (s, e) = app
        .post(
            "/fees/vouchers/generate",
            &accountant,
            json!({ "month": "2026-03-01" }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("validation"))
    );
    let (s, rep) = app
        .post(
            "/fees/vouchers/generate",
            &accountant,
            json!({ "month": "2026-04-15", "issue_date": "2026-04-01" }),
        )
        .await;
    assert_eq!(
        (s, rep["created"].as_u64(), rep["skipped_existing"].as_u64()),
        (StatusCode::OK, Some(3), Some(0)),
        "{rep}"
    );
    let (_, rep) = app
        .post(
            "/fees/vouchers/generate",
            &accountant,
            json!({ "month": "2026-04-01" }),
        )
        .await;
    assert_eq!(
        (rep["created"].as_u64(), rep["skipped_existing"].as_u64()),
        (Some(0), Some(3))
    );

    let (s, list) = app
        .get("/fees/vouchers?month=2026-04-01", &accountant)
        .await;
    assert_eq!((s, list.as_array().unwrap().len()), (StatusCode::OK, 3));
    let v_ali = list
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["student_id"] == json!(ali))
        .unwrap()
        .clone();
    assert_eq!(
        (
            v_ali["gross"].as_str(),
            v_ali["sibling_discount"].as_str(),
            v_ali["total_due"].as_str()
        ),
        (Some("8300.00"), Some("830.00"), Some("7470.00"))
    );
    let v_omar = list
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["student_id"] == json!(omar))
        .unwrap()
        .clone();
    assert_eq!(
        (
            v_omar["concession_discount"].as_str(),
            v_omar["total_due"].as_str()
        ),
        (Some("1666.67"), Some("1666.66"))
    );
    let (_, only_c2) = app
        .get(
            &format!(
                "/fees/vouchers?month=2026-04-01&class_id={}",
                ids["c2"].as_str().unwrap()
            ),
            &accountant,
        )
        .await;
    assert_eq!(only_c2.as_array().unwrap().len(), 2);
    let (_, by_q) = app.get("/fees/vouchers?q=sara", &accountant).await;
    assert_eq!(by_q.as_array().unwrap().len(), 1);

    // Voucher detail has print data.
    let vid = v_ali["id"].as_str().unwrap().to_string();
    let (s, d) = app.get(&format!("/fees/vouchers/{vid}"), &accountant).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(d["lines"].as_array().unwrap().len(), 3);
    assert_eq!(d["school"]["code"], "KHI-MC");
    assert_eq!(d["student_name"], "Ali Khan");
    assert_eq!(d["class_name"], "1");
    assert_eq!(d["balance"], "7470.00");
    let (s, _) = app.get(&format!("/fees/vouchers/{vid}"), &teacher).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Payments: validation, partial on time, late with reference, over-payment refused.
    let (s, e) = app
        .post(
            &format!("/fees/vouchers/{vid}/payments"),
            &accountant,
            json!({ "amount": "100", "method": "kuickpay" }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("validation")),
        "reference required"
    );
    let (s, _) = app
        .post(
            &format!("/fees/vouchers/{vid}/payments"),
            &accountant,
            json!({ "amount": "100", "method": "rupees" }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = app
        .post(
            &format!("/fees/vouchers/{vid}/payments"),
            &teacher,
            json!({ "amount": "100", "method": "cash" }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, p1) = app
        .post(
            &format!("/fees/vouchers/{vid}/payments"),
            &accountant,
            json!({ "amount": "1000", "method": "cash", "paid_on": "2026-04-10" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{p1}");
    assert_eq!(p1["receipt"]["receipt_no"], "R2026-000001");
    assert_eq!(
        (
            p1["voucher"]["status"].as_str(),
            p1["voucher"]["balance"].as_str()
        ),
        (Some("partial"), Some("6470.00"))
    );
    let (s, p2) = app.post(&format!("/fees/vouchers/{vid}/payments"), &accountant, json!({ "amount": "470", "method": "one_bill", "reference": "1B-123", "paid_on": "2026-04-20" })).await;
    assert_eq!(s, StatusCode::CREATED, "{p2}");
    assert_eq!(
        p2["voucher"]["late_fee"], "747.00",
        "10% of net monthly 7,470 after the 15th"
    );
    assert_eq!(p2["voucher"]["balance"], "6747.00");
    let (s, e) = app
        .post(
            &format!("/fees/vouchers/{vid}/payments"),
            &accountant,
            json!({ "amount": "99999", "method": "cash", "paid_on": "2026-04-20" }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("conflict"))
    );

    // Note: a zero NUMERIC decodes as Decimal zero with scale 0 and serializes as "0"; the UI
    // formats all money to two places.
    // Waiver needs approval (accountant 403, admin ok, reason required).
    let (s, _) = app
        .post(
            &format!("/fees/vouchers/{vid}/waive-late-fee"),
            &accountant,
            json!({ "reason": "Hardship" }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app
        .post(
            &format!("/fees/vouchers/{vid}/waive-late-fee"),
            &admin,
            json!({ "reason": " " }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, w) = app
        .post(
            &format!("/fees/vouchers/{vid}/waive-late-fee"),
            &admin,
            json!({ "reason": "Hardship" }),
        )
        .await;
    assert_eq!(
        (
            s,
            w["late_fee"].as_str(),
            w["late_fee_waived"].as_bool(),
            w["waiver_reason"].as_str(),
            w["balance"].as_str()
        ),
        (
            StatusCode::OK,
            Some("0"),
            Some(true),
            Some("Hardship"),
            Some("6000.00")
        )
    );
    assert!(w["waived_by"].is_string());

    // Receipt reprint + reversal (approval needed; original untouched).
    let rid = p2["receipt"]["id"].as_str().unwrap().to_string();
    let (s, r) = app.get(&format!("/fees/receipts/{rid}"), &accountant).await;
    assert_eq!(
        (
            s,
            r["amount"].as_str(),
            r["student_name"].as_str(),
            r["received_by_name"].as_str()
        ),
        (
            StatusCode::OK,
            Some("470.00"),
            Some("Ali Khan"),
            Some("acc")
        )
    );
    assert!(r["reversed_by"].is_null());
    let (s, _) = app
        .post(
            &format!("/fees/receipts/{rid}/reverse"),
            &accountant,
            json!({ "reason": "Wrong voucher" }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, rev) = app
        .post(
            &format!("/fees/receipts/{rid}/reverse"),
            &admin,
            json!({ "reason": "Wrong voucher" }),
        )
        .await;
    assert_eq!(
        (
            s,
            rev["amount"].as_str(),
            rev["reverses_id"].as_str(),
            rev["voucher_balance_after"].as_str()
        ),
        (
            StatusCode::CREATED,
            Some("-470.00"),
            Some(rid.as_str()),
            Some("6470.00")
        )
    );
    let (s, _) = app
        .post(
            &format!("/fees/receipts/{rid}/reverse"),
            &admin,
            json!({ "reason": "again" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (_, r) = app.get(&format!("/fees/receipts/{rid}"), &accountant).await;
    assert_eq!(r["amount"], "470.00");
    assert_eq!(r["reversed_by"]["amount"], "-470.00");

    // Ledger: billed 8,300, discounts 830, fines 0 (waived), paid 1,000, balance 6,470.
    let (s, l) = app
        .get(&format!("/fees/students/{ali}/ledger"), &accountant)
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        (
            l["totals"]["billed"].as_str(),
            l["totals"]["discounts"].as_str(),
            l["totals"]["fines"].as_str(),
            l["totals"]["paid"].as_str(),
            l["totals"]["balance"].as_str()
        ),
        (
            Some("8300.00"),
            Some("830.00"),
            Some("0"),
            Some("1000.00"),
            Some("6470.00")
        )
    );
    assert_eq!(l["receipts"].as_array().unwrap().len(), 3);
    assert_eq!(l["payable"].as_array().unwrap().len(), 1);
    let (s, _) = app
        .get(&format!("/fees/students/{ali}/ledger"), &teacher)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // May: carry-forward; Ali's April becomes carried and not payable.
    let (_, rep) = app
        .post(
            "/fees/vouchers/generate",
            &accountant,
            json!({ "month": "2026-05-01", "issue_date": "2026-05-01" }),
        )
        .await;
    assert_eq!(rep["created"], 3);
    let (_, l) = app
        .get(&format!("/fees/students/{ali}/ledger"), &accountant)
        .await;
    assert_eq!(l["vouchers"][0]["arrears"], "6470.00");
    assert_eq!(
        l["vouchers"][0]["total_due"], "9440.00",
        "2,970 net + 6,470 arrears"
    );
    assert_eq!(l["vouchers"][1]["status"], "carried");
    assert_eq!(l["totals"]["balance"], "9440.00");
    let (s, e) = app
        .post(
            &format!("/fees/vouchers/{vid}/payments"),
            &accountant,
            json!({ "amount": "10", "method": "cash", "paid_on": "2026-05-02" }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("conflict"))
    );

    // Defaulters as of 1 June (all three owe; Ali's oldest due date is April).
    let (s, defs) = app
        .get("/fees/defaulters?as_of=2026-06-01", &accountant)
        .await;
    assert_eq!((s, defs.as_array().unwrap().len()), (StatusCode::OK, 3));
    assert_eq!(defs[0]["student_id"], json!(ali));
    assert_eq!(
        (
            defs[0]["days_overdue"].as_i64(),
            defs[0]["balance"].as_str(),
            defs[0]["guardian_phone"].is_null()
        ),
        (Some(47), Some("9440.00"), true)
    );
    let (_, defs) = app
        .get("/fees/defaulters?as_of=2026-06-01&min_days=30", &accountant)
        .await;
    assert_eq!(
        defs.as_array().unwrap().len(),
        3,
        "all April vouchers are 47 days overdue"
    );
    let (_, defs) = app
        .get(
            &format!(
                "/fees/defaulters?as_of=2026-06-01&class_id={}",
                ids["c1"].as_str().unwrap()
            ),
            &accountant,
        )
        .await;
    assert_eq!(defs.as_array().unwrap().len(), 1);

    // Daily collections.
    let (s, col) = app
        .get("/fees/collections?date=2026-04-10", &accountant)
        .await;
    assert_eq!(
        (
            s,
            col["total"].as_str(),
            col["by_method"][0]["method"].as_str(),
            col["receipts"].as_array().unwrap().len()
        ),
        (StatusCode::OK, Some("1000.00"), Some("cash"), 1)
    );
    let (_, col) = app
        .get("/fees/collections?date=2026-04-21", &accountant)
        .await;
    assert_eq!(col["total"], "0");

    // Cancel: refused with payments; ok for an untouched voucher (Sara's May).
    let (s, _) = app
        .post(
            &format!("/fees/vouchers/{vid}/cancel"),
            &admin,
            json!({ "reason": "x" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (_, sara_l) = app
        .get(&format!("/fees/students/{sara}/ledger"), &accountant)
        .await;
    let sara_may = sara_l["vouchers"][0]["id"].as_str().unwrap();
    let (s, cv) = app
        .post(
            &format!("/fees/vouchers/{sara_may}/cancel"),
            &admin,
            json!({ "reason": "Generated by mistake" }),
        )
        .await;
    assert_eq!(
        (s, cv["status"].as_str()),
        (StatusCode::OK, Some("cancelled"))
    );
    app.finish().await;
}
