//! M2 stage 2: fee, attendance and exam reports checked against the modules' own endpoints,
//! teacher scope and Urdu. Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

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
    async fn raw(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, String, Vec<u8>) {
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
        let ctype = res
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, ctype, bytes.to_vec())
    }
    async fn call(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let (status, _, bytes) = self.raw(method, path, token, body).await;
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
    /// Report JSON, asserting 200.
    async fn report(&self, path: &str, t: &str) -> Value {
        let (s, b) = self.get(path, t).await;
        assert_eq!(s, StatusCode::OK, "{path}: {b}");
        b
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

fn rows(report: &Value, group: usize) -> &Vec<Value> {
    report["groups"][group]["rows"].as_array().unwrap()
}

#[tokio::test]
async fn fee_attendance_and_exam_reports_match_the_modules_and_respect_teacher_scope() {
    let app = app!();
    // School, year, classes 1 and 2 with section A, English in class 1, tuition fee for class 1.
    let (s, b) = app.call(Method::POST, "/setup", None, Some(json!({ "school_name": "Gulshan High", "school_code": "KHI-MC", "academic_year_start_month": 4, "admin_full_name": "Ayesha", "admin_username": "ayesha", "admin_password": "admin-password-1", "backup_password": "backup-password-1" }))).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let admin = app.login("ayesha", "admin-password-1").await;
    let (_, year) = app
        .post(
            "/academic-years",
            &admin,
            json!({ "start_year": 2026, "start_month": 4, "activate": true }),
        )
        .await;
    let year_id = id(&year["id"]);
    let (_, c1) = app
        .post("/classes", &admin, json!({ "name": "1", "level": 1 }))
        .await;
    let (_, c2) = app
        .post("/classes", &admin, json!({ "name": "2", "level": 2 }))
        .await;
    let (_, s1) = app
        .post(
            "/sections",
            &admin,
            json!({ "class_id": c1["id"], "academic_year_id": year["id"], "name": "A" }),
        )
        .await;
    let (_, s2) = app
        .post(
            "/sections",
            &admin,
            json!({ "class_id": c2["id"], "academic_year_id": year["id"], "name": "A" }),
        )
        .await;
    let (c1, s1, s2) = (id(&c1["id"]), id(&s1["id"]), id(&s2["id"]));
    let (_, eng) = app
        .post(
            "/subjects",
            &admin,
            json!({ "name": "English", "code": "ENG" }),
        )
        .await;
    let eng = id(&eng["id"]);
    let (s, _) = app
        .put(
            &format!("/classes/{c1}/subjects"),
            &admin,
            json!({ "subject_ids": [eng] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (_, tuition) = app
        .post(
            "/fees/heads",
            &admin,
            json!({ "name": "Tuition", "code": "TUI" }),
        )
        .await;
    let (s, _) = app.put("/fees/structure", &admin, json!({ "academic_year_id": year_id, "class_id": c1, "lines": [{ "fee_head_id": tuition["id"], "amount": "2500" }] })).await;
    assert_eq!(s, StatusCode::OK);
    // Different fathers and phones, so nobody is a sibling (no sibling discount).
    let student = |name: &str, father: &str, phone: &str, section: &str, roll: i32| {
        json!({ "full_name": name, "father_name": father, "date_of_birth": "2015-01-01", "gender": "male", "admission_date": "2026-04-01", "section_id": section, "roll_no": roll,
                "guardians": [{ "full_name": father, "relationship": "father", "phone": phone, "is_primary": true }] })
    };
    let (_, ali) = app
        .post(
            "/students",
            &admin,
            student("Ali Khan", "Ahmed Khan", "0300-1111111", &s1, 1),
        )
        .await;
    let (_, sara) = app
        .post(
            "/students",
            &admin,
            student("Sara Khan", "Imran Khan", "0300-2222222", &s1, 2),
        )
        .await;
    let (_, zain) = app
        .post(
            "/students",
            &admin,
            student("Zain Ahmed", "Rashid Ahmed", "0300-3333333", &s2, 1),
        )
        .await;
    let (ali, sara, zain) = (id(&ali["id"]), id(&sara["id"]), id(&zain["id"]));

    // ---- Fees: April vouchers (long overdue), late fees, one full and one partial payment.
    let (s, g) = app
        .post(
            "/fees/vouchers/generate",
            &admin,
            json!({ "month": "2026-04-01" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{g}");
    let (s, _) = app.post("/fees/late-fees/run", &admin, json!({})).await;
    assert_eq!(s, StatusCode::OK);
    let (_, vouchers) = app
        .get("/fees/vouchers?billing_month=2026-04-01", &admin)
        .await;
    let vouchers = vouchers.as_array().unwrap();
    assert_eq!(vouchers.len(), 2, "{vouchers:?}");
    let voucher_of = |sid: &str| {
        vouchers
            .iter()
            .find(|v| v["student_id"] == sid)
            .unwrap()
            .clone()
    };
    let (v_ali, v_sara) = (voucher_of(&ali), voucher_of(&sara));
    let late: String = v_ali["late_fee"].as_str().unwrap().into();
    assert_eq!(late, "250.00", "10% late fee on 2500");
    let (s, _) = app
        .post(
            &format!("/fees/vouchers/{}/payments", id(&v_ali["id"])),
            &admin,
            json!({ "amount": "2750", "method": "cash" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, _) = app
        .post(
            &format!("/fees/vouchers/{}/payments", id(&v_sara["id"])),
            &admin,
            json!({ "amount": "1000", "method": "cash" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, _) = app
        .post(
            &format!("/fees/students/{zain}/concessions"),
            &admin,
            json!({ "kind": "scholarship", "percent": "50", "reason": "Merit" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    let today = madrasa_api::clock::today().format("%Y-%m-%d").to_string();

    // Collection: two receipts today, the same total as /fees/collections.
    let (_, coll) = app
        .get(&format!("/fees/collections?date={today}"), &admin)
        .await;
    let r = app
        .report(
            &format!("/reports/fees.collection?from={today}&to={today}"),
            &admin,
        )
        .await;
    assert_eq!(r["row_count"], 2, "{r}");
    assert_eq!(r["columns"][9]["label"], "Amount (PKR)");
    assert_eq!(r["groups"][0]["subtotal"][9], "3,750.00");
    assert_eq!(r["total"][9], "3,750.00");
    assert_eq!(coll["total"], "3750.00", "{coll}");
    assert_eq!(rows(&r, 0)[0][7], "Cash");
    let r = app
        .report(
            &format!("/reports/fees.monthly?from=2026-04-01&to={today}"),
            &admin,
        )
        .await;
    assert_eq!(r["row_count"], 1);
    assert_eq!(r["total"][3], "3,750.00");

    // Defaulters: Sara only, balance 1,750, in the over-90-days bucket; matches /fees/defaulters.
    let (_, api_def) = app.get("/fees/defaulters", &admin).await;
    assert_eq!(api_def.as_array().unwrap().len(), 1, "{api_def}");
    let r = app.report("/reports/fees.defaulters", &admin).await;
    assert_eq!(r["row_count"], 1, "{r}");
    assert_eq!(r["groups"][0]["heading"], "Class 1");
    let row = &rows(&r, 0)[0];
    assert_eq!(row[1], "Sara Khan");
    assert_eq!(row[7], "Over 90 days");
    assert_eq!(row[9], "1,750.00");
    assert_eq!(api_def[0]["balance"], "1750.00");
    assert_eq!(r["total"][9], "1,750.00");
    assert!(
        r["notes"][3]
            .as_str()
            .unwrap()
            .starts_with("Over 90 days: 1 · 1,750.00"),
        "{r}"
    );

    // Ledger for Ali: voucher then receipt, balance back to zero.
    let r = app
        .report(
            "/reports/fees.ledger?student_number=KHI-MC-2026-00001",
            &admin,
        )
        .await;
    assert_eq!(r["row_count"], 2, "{r}");
    assert_eq!(rows(&r, 0)[0][4], "2,500.00");
    assert_eq!(rows(&r, 0)[1][7], "2,750.00");
    assert_eq!(r["total"][8], "0.00");
    let (s, _) = app
        .get("/reports/fees.ledger?student_number=NOPE", &admin)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Voucher register, late fees, concessions, heads.
    let r = app
        .report("/reports/fees.vouchers?month=2026-04", &admin)
        .await;
    assert_eq!(r["row_count"], 2);
    assert_eq!(r["total"][4], "5,000.00");
    assert_eq!(r["total"][10], "1,750.00");
    let r = app
        .report(
            "/reports/fees.late_fees?from=2026-04-01&to=2026-04-30",
            &admin,
        )
        .await;
    assert_eq!(r["row_count"], 2);
    assert_eq!(r["total"][6], "500.00");
    let r = app
        .report(
            &format!("/reports/fees.concessions?from={today}&to={today}"),
            &admin,
        )
        .await;
    assert_eq!(r["row_count"], 1);
    assert_eq!(rows(&r, 0)[0][1], "Zain Ahmed");
    assert_eq!(rows(&r, 0)[0][3], "Scholarship");
    let r = app.report("/reports/fees.heads", &admin).await;
    assert_eq!(rows(&r, 0)[0][0], "Tuition");
    assert_eq!(rows(&r, 0)[0][2], "5,000.00");
    assert_eq!(r["total"][3], "3,750.00");

    // ---- Attendance: two days in April for section 1-A, one staff leave.
    for (date, ali_st, sara_st) in [
        ("2026-04-06", "present", "absent"),
        ("2026-04-07", "late", "present"),
    ] {
        let (s, v) = app.put("/attendance/sheet", &admin, json!({ "section_id": s1, "date": date, "entries": [{ "student_id": ali, "status": ali_st }, { "student_id": sara, "status": sara_st }] })).await;
        assert_eq!(s, StatusCode::OK, "{v}");
    }
    let (_, hira) = app
        .post("/staff", &admin, json!({ "full_name": "Hira" }))
        .await;
    let (s, _) = app.put("/attendance/staff", &admin, json!({ "date": "2026-04-06", "entries": [{ "staff_id": hira["id"], "status": "leave" }] })).await;
    assert_eq!(s, StatusCode::OK);

    let (_, api_reg) = app
        .get(
            &format!("/attendance/register?section_id={s1}&month=2026-04-01"),
            &admin,
        )
        .await;
    let r = app
        .report(
            &format!("/reports/attendance.register?class_id={c1}&section_id={s1}&month=2026-04"),
            &admin,
        )
        .await;
    assert_eq!(r["row_count"], 2, "{r}");
    let ali_row = &rows(&r, 0)[0];
    assert_eq!(ali_row[1], "Ali Khan");
    assert_eq!(ali_row[2 + 5], "P", "6 April");
    assert_eq!(ali_row[2 + 6], "L", "7 April");
    assert_eq!(ali_row[2 + 4], "H", "5 April is a Sunday");
    let n = r["columns"].as_array().unwrap().len();
    assert_eq!(
        (
            &ali_row[n - 5],
            &ali_row[n - 4],
            &ali_row[n - 3],
            &ali_row[n - 1]
        ),
        (&json!("1"), &json!("0"), &json!("1"), &json!("100.0"))
    );
    let sara_row = &rows(&r, 0)[1];
    assert_eq!(sara_row[n - 1], "50.0");
    assert_eq!(api_reg["rows"][0]["summary"]["present"], 1, "{api_reg}");
    assert_eq!(api_reg["rows"][0]["summary"]["late"], 1);
    let r = app
        .report(
            "/reports/attendance.percent?from=2026-04-01&to=2026-04-30",
            &admin,
        )
        .await;
    assert_eq!(
        rows(&r, 0)[0],
        json!(["1", "A", "2", "2", "2", "1", "1", "0", "75.0"]),
        "{r}"
    );
    assert_eq!(rows(&r, 0)[1][8], "—", "section 2-A has nothing marked");
    let r = app
        .report(
            "/reports/attendance.absentees?from=2026-04-06&to=2026-04-07",
            &admin,
        )
        .await;
    assert_eq!(r["row_count"], 2, "{r}");
    assert_eq!(r["groups"][0]["heading"], "06-04-2026");
    assert_eq!(rows(&r, 0)[0][2], "Sara Khan");
    assert_eq!(rows(&r, 1)[0][6], "Late");
    let r = app
        .report(
            "/reports/attendance.below?from=2026-04-01&to=2026-04-30&threshold=60",
            &admin,
        )
        .await;
    assert_eq!(r["row_count"], 1, "{r}");
    assert_eq!(rows(&r, 0)[0][1], "Sara Khan");
    let r = app
        .report("/reports/attendance.staff?month=2026-04", &admin)
        .await;
    let hira_row = &rows(&r, 0)[0];
    assert_eq!(
        (&hira_row[1], &hira_row[6], &hira_row[7]),
        (&json!("Hira"), &json!("1"), &json!("0.0")),
        "{r}"
    );

    // ---- Exams: default assessments, max marks 50, Ali 45 and Sara 15 in English.
    let (_, list) = app
        .post(
            "/exams/assessments/defaults",
            &admin,
            json!({ "academic_year_id": year_id }),
        )
        .await;
    let test1 = id(&list[0]["id"]);
    let (s, _) = app.put("/exams/max-marks", &admin, json!({ "assessment_id": test1, "class_id": c1, "lines": [{ "subject_id": eng, "max_marks": "50" }] })).await;
    assert_eq!(s, StatusCode::OK);
    let (s, sh) = app.put("/exams/sheet", &admin, json!({ "assessment_id": test1, "section_id": s1, "subject_id": eng, "marks": [{ "student_id": ali, "obtained": "45" }, { "student_id": sara, "obtained": "15" }] })).await;
    assert_eq!(s, StatusCode::OK, "{sh}");
    let (_, api_res) = app
        .get(
            &format!("/exams/results/section?assessment_id={test1}&section_id={s1}"),
            &admin,
        )
        .await;
    let r = app
        .report(
            &format!(
                "/reports/exams.result_sheet?assessment_id={test1}&class_id={c1}&section_id={s1}"
            ),
            &admin,
        )
        .await;
    assert_eq!(r["row_count"], 2, "{r}");
    assert_eq!(r["columns"][3]["label"], "English /50");
    assert_eq!(
        rows(&r, 0)[0],
        json!([
            "1",
            "KHI-MC-2026-00001",
            "Ali Khan",
            "45",
            "45/50",
            "90.0",
            "A+",
            "Pass",
            "1"
        ])
    );
    assert_eq!(rows(&r, 0)[1][7], "Fail");
    assert_eq!(api_res["rows"][0]["percent"], "90.00", "{api_res}");
    assert_eq!(
        r["notes"][0],
        "Some sheets are not yet approved; figures may change."
    );
    let r = app
        .report(
            &format!("/reports/exams.merit?assessment_id={test1}&class_id={c1}&top=1"),
            &admin,
        )
        .await;
    assert_eq!(r["row_count"], 1);
    assert_eq!(rows(&r, 0)[0][2], "Ali Khan");
    let r = app
        .report(
            &format!("/reports/exams.analysis?assessment_id={test1}&class_id={c1}"),
            &admin,
        )
        .await;
    assert_eq!(
        rows(&r, 0)[0],
        json!(["English", "50", "2", "0", "60.0", "45", "15", "1", "50.0"]),
        "{r}"
    );
    let r = app
        .report(
            &format!("/reports/exams.grades?assessment_id={test1}&class_id={c1}"),
            &admin,
        )
        .await;
    let gr = &rows(&r, 0)[0];
    assert_eq!((&gr[0], &gr[1]), (&json!("A"), &json!("2")));
    assert_eq!(r["columns"][2]["label"], "A+");
    assert_eq!(gr[2], "1");
    let n = r["columns"].as_array().unwrap().len();
    assert_eq!(
        (&gr[n - 3], &gr[n - 2], &gr[n - 1]),
        (&json!("1"), &json!("1"), &json!("50.0"))
    );
    let r = app
        .report(
            &format!("/reports/exams.failing?assessment_id={test1}&class_id={c1}"),
            &admin,
        )
        .await;
    assert_eq!(r["groups"][0]["heading"], "English (1)");
    assert_eq!(rows(&r, 0)[0][3], "Sara Khan");
    let r = app
        .report(
            &format!("/reports/exams.report_cards?class_id={c1}&section_id={s1}"),
            &admin,
        )
        .await;
    assert_eq!(r["row_count"], 2);
    assert_eq!(rows(&r, 0)[0][2], "Ali Khan");
    assert_eq!(rows(&r, 0)[0][5], "Pass");
    let (s, ctype, bytes) = app
        .raw(
            Method::GET,
            &format!(
                "/reports/exams.report_cards?class_id={c1}&section_id={s1}&format=pdf&lang=ur"
            ),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!((s, ctype.as_str()), (StatusCode::OK, "application/pdf"));
    assert!(bytes.starts_with(b"%PDF"));

    // ---- Urdu labels on a fee report.
    let r = app
        .report(
            &format!("/reports/fees.collection?from={today}&to={today}&lang=ur"),
            &admin,
        )
        .await;
    assert_eq!(r["title"], "وصولی (رسیدیں)");
    assert_eq!(rows(&r, 0)[0][7], "نقد");

    // ---- Teacher scope: Sana teaches English in 1-A. Catalogue: attendance and exams only;
    // fees forbidden; other sections forbidden; her own section fine.
    let (_, st) = app
        .post("/staff", &admin, json!({ "full_name": "Sana Malik" }))
        .await;
    let staff_id = id(&st["id"]);
    app.put(
        &format!("/staff/{staff_id}/assignments"),
        &admin,
        json!({ "assignments": [{ "section_id": s1, "subject_id": eng }] }),
    )
    .await;
    let (s, _) = app
        .post(
            &format!("/staff/{staff_id}/login"),
            &admin,
            json!({ "username": "sana", "password": "teacher-password-1", "role": "teacher" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let teacher = app.login("sana", "teacher-password-1").await;
    let (s, cat) = app.get("/reports", &teacher).await;
    assert_eq!(s, StatusCode::OK, "{cat}");
    let modules: Vec<&str> = cat
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["module"].as_str().unwrap())
        .collect();
    assert_eq!(modules, ["attendance", "exams"]);
    assert!(!cat[0]["reports"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["code"] == "attendance.staff"));
    let (s, _) = app
        .get(
            &format!("/reports/fees.collection?from={today}&to={today}"),
            &teacher,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.get("/reports/students.list", &teacher).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app
        .get(
            &format!(
                "/reports/attendance.register?class_id={}&section_id={s2}&month=2026-04",
                id(&c2["id"])
            ),
            &teacher,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let r = app
        .report(
            &format!("/reports/attendance.register?class_id={c1}&section_id={s1}&month=2026-04"),
            &teacher,
        )
        .await;
    assert_eq!(r["row_count"], 2);
    let r = app
        .report(
            &format!(
                "/reports/exams.result_sheet?assessment_id={test1}&class_id={c1}&section_id={s1}"
            ),
            &teacher,
        )
        .await;
    assert_eq!(r["row_count"], 2);
    let r = app
        .report(
            "/reports/attendance.percent?from=2026-04-01&to=2026-04-30",
            &teacher,
        )
        .await;
    assert_eq!(r["row_count"], 1, "only her section: {r}");
    let (s, _) = app
        .get(
            &format!(
                "/reports/exams.result_sheet?assessment_id={test1}&class_id={}&section_id={s2}",
                id(&c2["id"])
            ),
            &teacher,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    app.finish().await;
}
