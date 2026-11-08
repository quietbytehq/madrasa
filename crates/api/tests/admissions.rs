//! Standard stage 3: admissions. Lite refuses the module; Standard runs the pipeline:
//! default stages, applications, moves with history, bulk moves, the offer letter, the
//! conversion into a student with guardian, enrolment and admission fee voucher, the
//! enquiry import with rollback, and the reports. Needs PostgreSQL via
//! `MADRASA_TEST_DATABASE_URL`.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use base64::Engine;
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

#[tokio::test]
async fn lite_refuses_admissions() {
    let app = app!();
    let admin = setup_school(&app).await;
    let (s, b) = app.get("/admissions/board", &admin).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    assert_eq!(b["error"]["code"], "feature_unavailable");
    let (s, b) = app.get("/imports/fields/enquiries", &admin).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    let (s, b) = app.get("/reports", &admin).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(!b
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["module"] == "admissions"));
    app.finish().await;
}

#[tokio::test]
async fn standard_runs_the_pipeline_and_converts() {
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
    let (_, c6) = app
        .post("/classes", &admin, json!({ "name": "6", "level": 6 }))
        .await;
    let class_id = id(&c6["id"]);
    let (s, sec) = app
        .post(
            "/sections",
            &admin,
            json!({ "class_id": class_id, "academic_year_id": year_id, "name": "A" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{sec}");
    let section_id = id(&sec["id"]);
    // An accountant login reads the pipeline but cannot change it.
    let (_, acc_staff) = app
        .post("/staff", &admin, json!({ "full_name": "Kamran Ali" }))
        .await;
    let (s, b) = app
        .post(
            &format!("/staff/{}/login", id(&acc_staff["id"])),
            &admin,
            json!({ "username": "kamran", "password": "office-password-1", "role": "accountant" }),
        )
        .await;
    assert!(s.is_success(), "{b}");
    let accountant = app.login("kamran", "office-password-1").await;

    // Default stages appear on first use.
    let (s, stages) = app.get("/admissions/stages", &admin).await;
    assert_eq!(s, StatusCode::OK, "{stages}");
    let stages = stages.as_array().unwrap().clone();
    assert_eq!(stages.len(), 8);
    assert_eq!(stages[0]["code"], "enquiry");
    let stage = |code: &str| id(&stages.iter().find(|s| s["code"] == code).unwrap()["id"]);
    // A custom stage, renamed and removed again while empty.
    let (s, extra) = app
        .post(
            "/admissions/stages",
            &admin,
            json!({ "name": "Document check", "kind": "open", "position": 2 }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{extra}");
    assert_eq!(extra["code"], "document_check");
    let (s, _) = app
        .put(
            &format!("/admissions/stages/{}", id(&extra["id"])),
            &admin,
            json!({ "name": "Documents", "kind": "open", "position": 2 }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app
        .call(
            Method::DELETE,
            &format!("/admissions/stages/{}", id(&extra["id"])),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, b) = app
        .post(
            "/admissions/stages",
            &accountant,
            json!({ "name": "X", "kind": "open" }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");

    // Two applications.
    let ali = json!({
        "class_id": class_id, "enquiry_date": "2026-02-10", "source": "Walk-in",
        "full_name": "Ali Raza", "father_name": "Imran Raza", "date_of_birth": "2015-03-14", "gender": "male",
        "phone": "0300 1234567", "previous_school": "Sunrise", "guardian_name": "Imran Raza",
        "guardian_relationship": "father", "guardian_phone": "0300 1234567", "guardian_cnic": "42101-1234567-1"
    });
    let (s, a1) = app
        .post("/admissions/applications", &admin, ali.clone())
        .await;
    assert_eq!(s, StatusCode::CREATED, "{a1}");
    assert_eq!(a1["application_no"], "APP-2026-0001");
    assert_eq!(a1["stage_name"], "Enquiry");
    assert_eq!(a1["events"].as_array().unwrap().len(), 1);
    assert_eq!(a1["guardian_cnic"], "42101-1234567-1");
    let a1_id = id(&a1["id"]);
    let (s, a2) = app
        .post(
            "/admissions/applications",
            &admin,
            json!({ "class_id": class_id, "full_name": "Sara Noor", "gender": "female" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{a2}");
    let a2_id = id(&a2["id"]);
    let (s, b) = app
        .post(
            "/admissions/applications",
            &admin,
            json!({ "full_name": "  " }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
    let (s, b) = app
        .post("/admissions/applications", &accountant, ali.clone())
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");

    // Edit test marks; marks above the total are refused.
    let mut edit = ali.clone();
    edit["test_date"] = json!("2026-03-01");
    edit["test_marks"] = json!(72);
    edit["test_total"] = json!(100);
    edit["interview_notes"] = json!("Confident, reads well");
    let (s, b) = app
        .put(
            &format!("/admissions/applications/{a1_id}"),
            &admin,
            edit.clone(),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(
        b["test_marks"].as_str().unwrap().parse::<f64>().unwrap(),
        72.0
    );
    edit["test_marks"] = json!(120);
    let (s, b) = app
        .put(
            &format!("/admissions/applications/{a1_id}"),
            &admin,
            edit.clone(),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");

    // Move through the pipeline; the history grows; the board counts.
    for code in ["applied", "test_scheduled", "interviewed"] {
        let (s, b) = app
            .post(
                &format!("/admissions/applications/{a1_id}/move"),
                &admin,
                json!({ "stage_id": stage(code), "note": code }),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
    }
    let (s, d) = app
        .get(&format!("/admissions/applications/{a1_id}"), &accountant)
        .await;
    assert_eq!(s, StatusCode::OK, "{d}");
    assert_eq!(d["stage_name"], "Interviewed");
    assert_eq!(d["events"].as_array().unwrap().len(), 4);
    assert_eq!(d["events"][3]["note"], "interviewed");
    // Bulk move both to "offered" with an offer fee set on Ali.
    let mut offered = edit.clone();
    offered["test_marks"] = json!(72);
    offered["offer_date"] = json!("2026-03-20");
    offered["offer_fee"] = json!("5000.00");
    offered["offer_expires"] = json!("2026-04-10");
    let (s, _) = app
        .put(
            &format!("/admissions/applications/{a1_id}"),
            &admin,
            offered,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, b) = app
        .post(
            "/admissions/applications/move",
            &admin,
            json!({ "ids": [a1_id, a2_id], "stage_id": stage("offered") }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["moved"], 2);
    let (s, board) = app.get("/admissions/board", &accountant).await;
    assert_eq!(s, StatusCode::OK, "{board}");
    assert_eq!(board["applications"].as_array().unwrap().len(), 2);
    let funnel = board["funnel"].as_array().unwrap();
    let row = |code: &str| {
        funnel
            .iter()
            .find(|f| f["stage_id"] == stage(code))
            .unwrap()
            .clone()
    };
    assert_eq!(row("enquiry")["reached"], 2);
    assert_eq!(row("enquiry")["current"], 0);
    assert_eq!(row("offered")["current"], 2);
    assert_eq!(row("interviewed")["reached"], 1);
    // The offer letter is a PDF in both languages.
    for lang in ["en", "ur"] {
        let (s, ctype, bytes) = app
            .get_bytes(
                &format!("/admissions/applications/{a1_id}/offer-letter?lang={lang}"),
                &accountant,
            )
            .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(ctype, "application/pdf");
        assert!(bytes.starts_with(b"%PDF"));
    }

    // Conversion needs an admitted stage; the admission fee needs a fee head.
    let (s, b) = app
        .post(
            &format!("/admissions/applications/{a1_id}/convert"),
            &admin,
            json!({ "section_id": section_id }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
    let (s, _) = app
        .post(
            &format!("/admissions/applications/{a1_id}/move"),
            &admin,
            json!({ "stage_id": stage("admitted") }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, head) = app
        .post(
            "/fees/heads",
            &admin,
            json!({ "name": "Admission fee", "code": "ADM", "is_recurring": false }),
        )
        .await;
    assert!(s.is_success(), "{head}");
    let (s, b) = app
        .put(
            "/admissions/settings",
            &admin,
            json!({ "fee_head_id": head["id"] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["fee_head_name"], "Admission fee");
    let (s, conv) = app
        .post(
            &format!("/admissions/applications/{a1_id}/convert"),
            &admin,
            json!({ "section_id": section_id, "roll_no": 7, "admission_date": "2026-04-01" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{conv}");
    let student_id = id(&conv["student_id"]);
    assert!(conv["guardian_id"].is_string());
    assert!(conv["voucher_id"].is_string());
    assert_eq!(conv["application"]["student_id"], student_id);
    // The student exists with guardian, enrolment and the voucher.
    let (s, st) = app.get(&format!("/students/{student_id}"), &admin).await;
    assert_eq!(s, StatusCode::OK, "{st}");
    assert_eq!(st["full_name"], "Ali Raza");
    assert_eq!(st["guardians"][0]["full_name"], "Imran Raza");
    assert_eq!(st["current"]["section_id"], section_id);
    assert_eq!(st["current"]["roll_no"], 7);
    let (s, v) = app
        .get(&format!("/fees/vouchers?student_id={student_id}"), &admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let vouchers = v
        .as_array()
        .cloned()
        .or_else(|| v["items"].as_array().cloned())
        .unwrap();
    assert_eq!(vouchers.len(), 1, "{v}");
    assert_eq!(vouchers[0]["total_due"], "5000.00");
    // Converting twice, deleting or moving a converted application is refused.
    let (s, b) = app
        .post(
            &format!("/admissions/applications/{a1_id}/convert"),
            &admin,
            json!({ "section_id": section_id }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    let (s, b) = app
        .post(
            &format!("/admissions/applications/{a1_id}/move"),
            &admin,
            json!({ "stage_id": stage("rejected") }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    let (s, b) = app
        .call(
            Method::DELETE,
            &format!("/admissions/applications/{a1_id}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    // Sara: a duplicate of an existing student is flagged unless forced.
    let (s, _) = app
        .post(
            "/students",
            &admin,
            json!({ "full_name": "Sara Noor", "father_name": "Noor Ahmed", "date_of_birth": "2015-06-01", "gender": "female",
                    "admission_date": "2026-04-01", "section_id": section_id, "roll_no": 1 }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, _) = app
        .put(
            &format!("/admissions/applications/{a2_id}"),
            &admin,
            json!({ "class_id": class_id, "full_name": "Sara Noor", "father_name": "Noor Ahmed", "date_of_birth": "2015-06-01", "gender": "female" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app
        .post(
            &format!("/admissions/applications/{a2_id}/move"),
            &admin,
            json!({ "stage_id": stage("admitted") }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, b) = app
        .post(
            &format!("/admissions/applications/{a2_id}/convert"),
            &admin,
            json!({ "section_id": section_id }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    assert_eq!(b["error"]["code"], "duplicate");
    let (s, b) = app
        .post(
            &format!("/admissions/applications/{a2_id}/convert"),
            &admin,
            json!({ "section_id": section_id, "force": true }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(b["voucher_id"].is_null(), "no offer fee, no voucher");

    // Enquiry import through the import engine, then rolled back.
    let csv = "Applicant name,Father,Class applied for,Phone,Guardian name,Date\nHamza Khan,Tariq Khan,6,0301 1111111,Tariq Khan,2026-03-05\nNoor Fatima,,9,0302 2222222,Bilquis,2026-03-06\n";
    let content = base64::engine::general_purpose::STANDARD.encode(csv);
    let (s, parsed) = app
        .post(
            "/imports/parse",
            &admin,
            json!({ "kind": "enquiries", "file_name": "enquiries.csv", "content_base64": content }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{parsed}");
    assert_eq!(parsed["mapping"]["full_name"], 0);
    assert_eq!(parsed["mapping"]["class"], 2);
    let req = json!({
        "kind": "enquiries", "file_name": "enquiries.csv", "headers": parsed["headers"], "rows": parsed["rows"],
        "mapping": parsed["mapping"]
    });
    let (s, preview) = app.post("/imports/preview", &admin, req.clone()).await;
    assert_eq!(s, StatusCode::OK, "{preview}");
    assert_eq!(preview["summary"]["create"], 1, "{preview}");
    assert_eq!(
        preview["summary"]["error"], 1,
        "class 9 is not set up: {preview}"
    );
    let (s, result) = app.post("/imports/apply", &admin, req).await;
    assert_eq!(s, StatusCode::OK, "{result}");
    assert_eq!(result["summary"]["create"], 1);
    let (_, list) = app.get("/admissions/applications?q=Hamza", &admin).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["source"], "import");
    assert_eq!(list[0]["enquiry_date"], "2026-03-05");
    let batch_id = id(&result["batch"]["id"]);
    let (s, b) = app
        .post(
            &format!("/imports/batches/{batch_id}/rollback"),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (_, list) = app.get("/admissions/applications?q=Hamza", &admin).await;
    assert!(list.as_array().unwrap().is_empty());

    // Reports, as JSON and PDF.
    let (s, cat) = app.get("/reports", &accountant).await;
    assert_eq!(s, StatusCode::OK, "{cat}");
    assert!(cat
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["module"] == "admissions"));
    let (s, r) = app.get("/reports/admissions.funnel", &admin).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["groups"][0]["rows"][0][1], "2");
    let (s, r) = app.get("/reports/admissions.by_class", &admin).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["groups"][0]["rows"][0][0], "6");
    assert_eq!(r["groups"][0]["rows"][0][5], "2", "{r}");
    for path in [
        "/reports/admissions.funnel?format=pdf&lang=ur",
        "/reports/admissions.by_class?format=pdf",
        "/reports/admissions.pending_tests?format=pdf&as_of=2026-01-01",
    ] {
        let (s, ctype, bytes) = app.get_bytes(path, &admin).await;
        assert_eq!(s, StatusCode::OK, "{path}");
        assert_eq!(ctype, "application/pdf");
        assert!(bytes.starts_with(b"%PDF"));
    }

    // Back on Lite the module is refused and the student stays.
    let (s, _) = app
        .put("/settings/edition", &admin, json!({ "edition": "lite" }))
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app.get("/admissions/board", &admin).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.get(&format!("/students/{student_id}"), &admin).await;
    assert_eq!(s, StatusCode::OK);
    app.finish().await;
}
