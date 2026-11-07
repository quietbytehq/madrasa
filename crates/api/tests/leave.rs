//! Standard stage 2: leave. Lite refuses the module; Standard runs staff leave (types,
//! balances, requests, approval marking staff attendance, withdrawal unmarking it,
//! carry-over, the approver setting), student leave (approval marking sheets, corrections
//! keeping it, withdrawal), the staff register with late marks, and the reports. Needs
//! PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use madrasa_api::{router, AppState};
use madrasa_db::testing::TestDb;
use serde_json::{json, Value};
use tower::ServiceExt;

struct App {
    db: Option<TestDb>,
    app: Router,
}

impl App {
    async fn new() -> Option<App> {
        let db = TestDb::new().await?;
        let state = AppState::new(db.pool.clone()).await.unwrap();
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
    async fn get_bytes(&self, p: &str, t: &str) -> (StatusCode, String, Vec<u8>) {
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/api/v1{p}"))
            .header(header::AUTHORIZATION, format!("Bearer {t}"))
            .body(Body::empty())
            .unwrap();
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let ctype = res
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
        (status, ctype, bytes)
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

fn id(v: &Value) -> String {
    v.as_str().unwrap().to_string()
}

fn num(v: &Value) -> f64 {
    match v {
        Value::String(s) => s.parse().unwrap(),
        Value::Number(n) => n.as_f64().unwrap(),
        _ => panic!("not a number: {v}"),
    }
}

async fn setup_school(app: &App) -> String {
    let (s, b) = app
        .call(
            Method::POST,
            "/setup",
            None,
            Some(json!({
                "school_name": "Gulshan High", "school_code": "KHI-LV", "academic_year_start_month": 4,
                "admin_full_name": "Ayesha", "admin_username": "ayesha", "admin_password": "admin-password-1",
                "backup_password": "backup-password-1"
            })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    app.login("ayesha", "admin-password-1").await
}

#[tokio::test]
async fn lite_refuses_leave_and_late_marks() {
    let app = app!();
    let admin = setup_school(&app).await;
    let (s, b) = app.get("/leave/types", &admin).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    assert_eq!(b["error"]["code"], "feature_unavailable");
    let (s, b) = app.get("/leave/staff-register", &admin).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    let (_, staff) = app
        .post("/staff", &admin, json!({ "full_name": "Sana Malik" }))
        .await;
    let (s, b) = app
        .put(
            "/attendance/staff",
            &admin,
            json!({ "date": "2026-04-06", "entries": [{ "staff_id": staff["id"], "status": "late", "arrived_at": "08:30" }] }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
    let (s, b) = app.get("/reports", &admin).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(!b.as_array().unwrap().iter().any(|m| m["module"] == "leave"));
    app.finish().await;
}

#[tokio::test]
async fn standard_runs_staff_and_student_leave() {
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
    let (_, year) = app
        .post(
            "/academic-years",
            &admin,
            json!({ "start_year": 2026, "start_month": 4, "activate": true }),
        )
        .await;
    let year_id = id(&year["id"]);
    let (_, class) = app
        .post("/classes", &admin, json!({ "name": "9", "level": 9 }))
        .await;
    // Staff: Sana (teacher login, class teacher of 9 A), Bilal (login; later the approver).
    let (_, sana) = app
        .post("/staff", &admin, json!({ "full_name": "Sana Malik" }))
        .await;
    let (_, bilal) = app
        .post("/staff", &admin, json!({ "full_name": "Bilal Ahmed" }))
        .await;
    let (sana, bilal) = (id(&sana["id"]), id(&bilal["id"]));
    for (st, u) in [(&sana, "sana"), (&bilal, "bilal")] {
        let (s, b) = app
            .post(
                &format!("/staff/{st}/login"),
                &admin,
                json!({ "username": u, "password": "teacher-password-1", "role": "teacher" }),
            )
            .await;
        assert!(s.is_success(), "{b}");
    }
    let teacher = app.login("sana", "teacher-password-1").await;
    let bilal_login = app.login("bilal", "teacher-password-1").await;
    let (s, section) = app
        .post(
            "/sections",
            &admin,
            json!({ "class_id": class["id"], "academic_year_id": year_id, "name": "A", "class_teacher_id": sana }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{section}");
    let section_id = id(&section["id"]);
    let student = |name: &str, roll: i32| {
        json!({ "full_name": name, "father_name": "Father", "date_of_birth": "2015-01-01", "gender": "male", "admission_date": "2026-04-01",
                "section_id": section_id, "roll_no": roll,
                "guardians": [{ "full_name": format!("Parent of {name}"), "relationship": "father", "phone": "0300 1234567", "is_primary": true }] })
    };
    let (s, ali) = app.post("/students", &admin, student("Ali Khan", 1)).await;
    assert_eq!(s, StatusCode::CREATED, "{ali}");
    let (_, sara) = app.post("/students", &admin, student("Sara Khan", 2)).await;
    let (ali, sara) = (id(&ali["id"]), id(&sara["id"]));

    // Leave types: annual (20, carry 5, approval) and casual (10, no approval).
    let (s, annual) = app
        .post(
            "/leave/types",
            &admin,
            json!({ "name": "Annual", "code": "AL", "entitlement_days": 20, "carry_over_max": 5, "needs_approval": true }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{annual}");
    let (s, casual) = app
        .post(
            "/leave/types",
            &admin,
            json!({ "name": "Casual", "code": "CL", "entitlement_days": 10, "needs_approval": false }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{casual}");
    let (annual, casual) = (id(&annual["id"]), id(&casual["id"]));
    let (s, b) = app
        .post(
            "/leave/types",
            &teacher,
            json!({ "name": "X", "entitlement_days": 1 }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    let (s, b) = app
        .post(
            "/leave/types",
            &admin,
            json!({ "name": "annual", "entitlement_days": 1 }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");

    // Balances: every active staff member × type.
    let (s, bal) = app.get("/leave/balances", &admin).await;
    assert_eq!(s, StatusCode::OK, "{bal}");
    assert_eq!(bal.as_array().unwrap().len(), 4);
    let sana_annual = bal
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["staff_id"] == sana && b["leave_type_id"] == annual)
        .unwrap();
    assert_eq!(num(&sana_annual["entitlement"]), 20.0);
    assert_eq!(num(&sana_annual["available"]), 20.0);

    // Sana requests her own annual leave Mon 6 – Wed 8 April (3 days); a teacher sees only herself.
    let (s, req) = app
        .post(
            "/leave/requests",
            &teacher,
            json!({ "leave_type_id": annual, "from_date": "2026-04-06", "to_date": "2026-04-08", "reason": "Family" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{req}");
    assert_eq!(req["status"], "pending");
    assert_eq!(num(&req["days"]), 3.0);
    let req_id = id(&req["id"]);
    let (s, b) = app
        .post(
            "/leave/requests",
            &teacher,
            json!({ "leave_type_id": annual, "from_date": "2026-04-08", "to_date": "2026-04-09" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "overlap: {b}");
    let (s, b) = app
        .post(
            "/leave/requests",
            &teacher,
            json!({ "staff_id": bilal, "leave_type_id": annual, "from_date": "2026-04-20" }),
        )
        .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "a teacher cannot request for others: {b}"
    );
    let (s, mine) = app.get("/leave/balances", &teacher).await;
    assert_eq!(s, StatusCode::OK, "{mine}");
    assert_eq!(mine.as_array().unwrap().len(), 2);
    let m = mine
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["leave_type_id"] == annual)
        .unwrap();
    assert_eq!(num(&m["pending"]), 3.0);
    assert_eq!(num(&m["remaining"]), 17.0);
    let (s, list) = app.get("/leave/requests", &teacher).await;
    assert_eq!(s, StatusCode::OK, "{list}");
    assert_eq!(list["requests"].as_array().unwrap().len(), 1);
    assert_eq!(list["can_decide"], false);
    assert_eq!(list["own_staff_id"], sana);

    // Deciding: a teacher may not; the admin approves and the days are marked as leave.
    let (s, b) = app
        .post(
            &format!("/leave/requests/{req_id}/decide"),
            &teacher,
            json!({ "decision": "approved" }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    let (s, b) = app
        .post(
            &format!("/leave/requests/{req_id}/decide"),
            &admin,
            json!({ "decision": "approved", "note": "ok" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["status"], "approved");
    let (s, day) = app.get("/attendance/staff?date=2026-04-07", &admin).await;
    assert_eq!(s, StatusCode::OK, "{day}");
    let row = day["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["staff_id"] == sana)
        .unwrap();
    assert_eq!(row["status"], "leave");
    assert_eq!(row["leave_request_id"], req_id);
    // Hand marking cannot overwrite an approved leave day.
    let (s, b) = app
        .put(
            "/attendance/staff",
            &admin,
            json!({ "date": "2026-04-07", "entries": [{ "staff_id": sana, "status": "present" }] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let row = b["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["staff_id"] == sana)
        .unwrap();
    assert_eq!(row["status"], "leave");
    let (_, bal) = app
        .get(&format!("/leave/balances?staff_id={sana}"), &admin)
        .await;
    let m = bal
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["leave_type_id"] == annual)
        .unwrap();
    assert_eq!(num(&m["used"]), 3.0);
    assert_eq!(num(&m["available"]), 17.0);
    // Withdrawal unmarks the days.
    let (s, b) = app
        .post(
            &format!("/leave/requests/{req_id}/withdraw"),
            &teacher,
            json!({ "note": "plans changed" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["status"], "withdrawn");
    let (_, day) = app.get("/attendance/staff?date=2026-04-07", &admin).await;
    let row = day["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["staff_id"] == sana)
        .unwrap();
    assert!(row["status"].is_null(), "{row}");
    let (s, b) = app
        .post(
            &format!("/leave/requests/{req_id}/decide"),
            &admin,
            json!({ "decision": "approved" }),
        )
        .await;
    assert_eq!(
        s,
        StatusCode::CONFLICT,
        "a withdrawn request cannot be approved: {b}"
    );

    // Casual leave needs no approval: a half day for Bilal is approved at once.
    let (s, half) = app
        .post(
            "/leave/requests",
            &admin,
            json!({ "staff_id": bilal, "leave_type_id": casual, "from_date": "2026-04-13", "half_day": "am", "reason": "Bank" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{half}");
    assert_eq!(half["status"], "approved");
    assert_eq!(num(&half["days"]), 0.5);
    let (s, b) = app
        .post("/leave/requests", &admin, json!({ "staff_id": bilal, "leave_type_id": annual, "from_date": "2026-05-04", "to_date": "2026-06-05" }))
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "more than the balance: {b}");
    let (s, b) = app
        .post("/leave/requests", &admin, json!({ "staff_id": bilal, "leave_type_id": annual, "from_date": "2026-04-12", "to_date": "2026-04-12" }))
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "weekend only: {b}");

    // The configured approver (Bilal) may decide Sana's next request, but not his own.
    let (s, b) = app
        .put(
            "/leave/settings",
            &admin,
            json!({ "approver_staff_id": bilal }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["approver_name"], "Bilal Ahmed");
    let (_, req2) = app
        .post(
            "/leave/requests",
            &teacher,
            json!({ "leave_type_id": annual, "from_date": "2026-04-27", "to_date": "2026-04-28" }),
        )
        .await;
    let (s, b) = app
        .post(
            &format!("/leave/requests/{}/decide", id(&req2["id"])),
            &bilal_login,
            json!({ "decision": "rejected", "note": "exam week" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["status"], "rejected");
    let (_, own) = app
        .post(
            "/leave/requests",
            &bilal_login,
            json!({ "leave_type_id": annual, "from_date": "2026-04-29" }),
        )
        .await;
    let (s, b) = app
        .post(
            &format!("/leave/requests/{}/decide", id(&own["id"])),
            &bilal_login,
            json!({ "decision": "approved" }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");

    // Student leave: Ali away Mon 6 – Tue 7 April. A sheet already exists for Monday.
    let (s, b) = app
        .put("/attendance/sheet", &admin, json!({ "section_id": section_id, "date": "2026-04-06", "entries": [{ "student_id": sara, "status": "absent" }] }))
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, sreq) = app
        .post("/leave/students", &admin, json!({ "student_id": ali, "from_date": "2026-04-06", "to_date": "2026-04-07", "reason": "Wedding" }))
        .await;
    assert_eq!(s, StatusCode::CREATED, "{sreq}");
    let sreq_id = id(&sreq["id"]);
    let (s, b) = app
        .post(
            "/leave/students",
            &admin,
            json!({ "student_id": ali, "from_date": "2026-04-07", "to_date": "2026-04-08" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "overlap: {b}");
    let (s, b) = app
        .post(
            "/leave/students",
            &teacher,
            json!({ "student_id": sara, "from_date": "2026-04-09" }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    // The class teacher approves; Monday's entry becomes leave, the sheet is recounted.
    let (s, lst) = app.get("/leave/students", &teacher).await;
    assert_eq!(s, StatusCode::OK, "{lst}");
    assert_eq!(lst["requests"].as_array().unwrap().len(), 1);
    assert_eq!(lst["requests"][0]["days"], 2);
    assert_eq!(lst["decide_sections"][0], section_id);
    let (s, b) = app
        .post(
            &format!("/leave/students/{sreq_id}/decide"),
            &teacher,
            json!({ "decision": "approved" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, sheet) = app
        .get(
            &format!("/attendance/sheet?section_id={section_id}&date=2026-04-06"),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{sheet}");
    let ali_entry = sheet["students"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["student_id"] == ali)
        .unwrap();
    assert_eq!(ali_entry["status"], "leave");
    assert_eq!(ali_entry["note"], "Leave (approved)");
    assert_eq!(sheet["sheet"]["leave"], 1);
    // A sheet saved afterwards for Tuesday marks Ali as leave whatever it says; a correction
    // of Monday keeps the leave.
    let (s, tue) = app
        .put("/attendance/sheet", &admin, json!({ "section_id": section_id, "date": "2026-04-07", "entries": [{ "student_id": ali, "status": "absent" }] }))
        .await;
    assert_eq!(s, StatusCode::OK, "{tue}");
    let e = tue["students"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["student_id"] == ali)
        .unwrap();
    assert_eq!(e["status"], "leave");
    let (s, mon) = app
        .put("/attendance/sheet", &admin, json!({ "section_id": section_id, "date": "2026-04-06", "entries": [{ "student_id": ali, "status": "present" }] }))
        .await;
    assert_eq!(s, StatusCode::OK, "{mon}");
    let e = mon["students"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["student_id"] == ali)
        .unwrap();
    assert_eq!(e["status"], "leave");
    // The absentee list names the approved leave.
    let (s, abs) = app
        .get("/attendance/absentees?date=2026-04-06", &admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{abs}");
    assert!(abs.to_string().contains("Leave (approved)"), "{abs}");
    // Withdrawal turns the marked days into absences.
    let (s, b) = app
        .post(
            &format!("/leave/students/{sreq_id}/withdraw"),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (_, mon) = app
        .get(
            &format!("/attendance/sheet?section_id={section_id}&date=2026-04-06"),
            &admin,
        )
        .await;
    let e = mon["students"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["student_id"] == ali)
        .unwrap();
    assert_eq!(e["status"], "absent");
    assert_eq!(mon["sheet"]["leave"], 0);
    // The student profile list.
    let (s, b) = app
        .get(&format!("/leave/students?student_id={ali}"), &admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["requests"][0]["status"], "withdrawn");

    // Staff register with a late mark and its time.
    let (s, b) = app
        .put("/attendance/staff", &admin, json!({ "date": "2026-04-14", "entries": [{ "staff_id": bilal, "status": "late", "arrived_at": "08:30" }] }))
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, reg) = app.get("/leave/staff-register?month=2026-04", &admin).await;
    assert_eq!(s, StatusCode::OK, "{reg}");
    assert_eq!(reg["days"].as_array().unwrap().len(), 30);
    let brow = reg["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["staff_id"] == bilal)
        .unwrap();
    assert_eq!(
        brow["cells"][12]["status"], "leave",
        "13 April: {}",
        brow["cells"][12]
    );
    assert_eq!(brow["cells"][12]["approved_leave"], true);
    assert_eq!(brow["cells"][13]["status"], "late");
    assert_eq!(brow["cells"][13]["arrived_at"], "08:30:00");
    assert_eq!(brow["late"], 1);
    assert_eq!(brow["leave"], 1);
    let (s, reg) = app
        .get("/leave/staff-register?month=2026-04", &teacher)
        .await;
    assert_eq!(s, StatusCode::OK, "{reg}");
    assert_eq!(
        reg["rows"].as_array().unwrap().len(),
        1,
        "a teacher sees her own row"
    );

    // Carry-over into the next year: Sana used nothing (withdrawn, rejected) → min(20, 5) = 5.
    let (_, next) = app
        .post(
            "/academic-years",
            &admin,
            json!({ "start_year": 2027, "start_month": 4, "activate": false }),
        )
        .await;
    let next_id = id(&next["id"]);
    let (s, b) = app
        .post(
            "/leave/balances/carry-over",
            &admin,
            json!({ "from_academic_year_id": year_id, "to_academic_year_id": next_id }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["written"], 2);
    let (_, nb) = app
        .get(
            &format!("/leave/balances?academic_year_id={next_id}&staff_id={sana}"),
            &admin,
        )
        .await;
    let a = nb
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["leave_type_id"] == annual)
        .unwrap();
    assert_eq!(num(&a["carried_over"]), 5.0);
    assert_eq!(num(&a["granted"]), 25.0);
    let c = nb
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["leave_type_id"] == casual)
        .unwrap();
    assert_eq!(num(&c["carried_over"]), 0.0);
    // An adjustment with a note.
    let (s, b) = app
        .put("/leave/balances", &admin, json!({ "staff_id": sana, "leave_type_id": annual, "adjustment": -2, "note": "Unpaid days" }))
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let a = b
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["leave_type_id"] == annual)
        .unwrap();
    assert_eq!(num(&a["granted"]), 18.0);

    // Reports: in the catalogue for office and teachers, as JSON and PDF.
    let (s, cat) = app.get("/reports", &teacher).await;
    assert_eq!(s, StatusCode::OK, "{cat}");
    assert!(cat
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["module"] == "leave"));
    let (s, r) = app.get("/reports/leave.balances", &admin).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["groups"][0]["rows"].as_array().unwrap().len(), 4);
    let (s, r) = app
        .get("/reports/leave.register?status=approved", &admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["groups"][0]["rows"].as_array().unwrap().len(), 1);
    assert_eq!(r["groups"][0]["rows"][0][1], "Bilal Ahmed");
    for path in [
        "/reports/leave.register?format=pdf&lang=ur",
        "/reports/leave.students?format=pdf",
        "/reports/leave.balances?format=pdf",
    ] {
        let (s, ctype, bytes) = app.get_bytes(path, &admin).await;
        assert_eq!(s, StatusCode::OK, "{path}");
        assert_eq!(ctype, "application/pdf");
        assert!(bytes.starts_with(b"%PDF"));
    }
    let (s, r) = app.get("/reports/leave.students", &teacher).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["groups"][0]["rows"].as_array().unwrap().len(), 1);

    // A type with live requests cannot be removed.
    let (s, b) = app
        .call(
            Method::DELETE,
            &format!("/leave/types/{casual}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");

    // Back on Lite the module is refused and attendance still reads.
    let (s, b) = app
        .put("/settings/edition", &admin, json!({ "edition": "lite" }))
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = app.get("/leave/requests", &admin).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    let (s, b) = app.get("/attendance/staff?date=2026-04-14", &admin).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    app.finish().await;
}
