//! Standard stage 4: library. Lite refuses the module; Standard runs the catalogue, the
//! desk (issue, renew, return, lost), fines (compute, waive, settle to a voucher or at the
//! desk), rules, the stock take, the librarian role and the reports. Needs PostgreSQL via
//! `MADRASA_TEST_DATABASE_URL`.

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
                "school_name": "Gulshan High", "school_code": "KHI-LB", "academic_year_start_month": 4,
                "admin_full_name": "Ayesha", "admin_username": "ayesha", "admin_password": "admin-password-1",
                "backup_password": "backup-password-1"
            })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    app.login("ayesha", "admin-password-1").await
}

#[tokio::test]
async fn lite_refuses_the_library_and_the_librarian_role() {
    let app = app!();
    let admin = setup_school(&app).await;
    let (s, b) = app.get("/library/books", &admin).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    assert_eq!(b["error"]["code"], "feature_unavailable");
    let (_, staff) = app
        .post("/staff", &admin, json!({ "full_name": "Zara Qureshi" }))
        .await;
    let (s, b) = app
        .post(
            &format!("/staff/{}/login", id(&staff["id"])),
            &admin,
            json!({ "username": "zara", "password": "lib-password-1", "role": "librarian" }),
        )
        .await;
    assert!(!s.is_success(), "Lite has no librarian role: {b}");
    let (s, b) = app.get("/reports", &admin).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(!b
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["module"] == "library"));
    app.finish().await;
}

#[tokio::test]
async fn standard_runs_the_desk_fines_and_reports() {
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
    let (_, section) = app
        .post(
            "/sections",
            &admin,
            json!({ "class_id": class["id"], "academic_year_id": year_id, "name": "A" }),
        )
        .await;
    let section_id = id(&section["id"]);
    let (s, ali) = app
        .post(
            "/students",
            &admin,
            json!({ "full_name": "Ali Khan", "father_name": "Father", "date_of_birth": "2015-01-01", "gender": "male",
                    "admission_date": "2026-04-01", "section_id": section_id, "roll_no": 1 }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{ali}");
    let ali = id(&ali["id"]);
    // The librarian role exists on Standard; Zara runs the desk.
    let (_, zara) = app
        .post("/staff", &admin, json!({ "full_name": "Zara Qureshi" }))
        .await;
    let zara = id(&zara["id"]);
    let (s, b) = app
        .post(
            &format!("/staff/{zara}/login"),
            &admin,
            json!({ "username": "zara", "password": "lib-password-1", "role": "librarian" }),
        )
        .await;
    assert!(s.is_success(), "{b}");
    let lib = app.login("zara", "lib-password-1").await;
    let (_, me) = app.get("/auth/me", &lib).await;
    assert!(
        me["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p == "library.manage"),
        "{me}"
    );
    assert!(
        !me["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p == "fees.read"),
        "{me}"
    );
    // A teacher login reads only.
    let (_, sana) = app
        .post("/staff", &admin, json!({ "full_name": "Sana Malik" }))
        .await;
    let sana = id(&sana["id"]);
    let (s, b) = app
        .post(
            &format!("/staff/{sana}/login"),
            &admin,
            json!({ "username": "sana", "password": "teacher-password-1", "role": "teacher" }),
        )
        .await;
    assert!(s.is_success(), "{b}");
    let teacher = app.login("sana", "teacher-password-1").await;

    // Rules: students 2 books, 14 days, 5 per day, 1 renewal; staff fine-free.
    let (s, settings) = app.get("/library/settings", &lib).await;
    assert_eq!(s, StatusCode::OK, "{settings}");
    assert_eq!(settings["rules"]["student"]["max_books"], 2);
    let (s, b) = app
        .put(
            "/library/settings",
            &lib,
            json!({ "rules": { "student": { "max_books": 2, "loan_days": 14, "fine_per_day": 5, "max_renewals": 1 },
                                "staff": { "max_books": 5, "loan_days": 30, "fine_per_day": 0, "max_renewals": 2 } },
                    "fine_fee_head_id": null }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = app
        .put(
            "/library/settings",
            &teacher,
            json!({ "rules": settings["rules"], "fine_fee_head_id": null }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");

    // Catalogue: a book with three copies, one more added later; duplicates refused.
    let (s, physics) = app
        .post(
            "/library/books",
            &lib,
            json!({ "title": "Physics", "author": "Halliday", "category": "Science", "language": "English",
                    "accession_nos": ["A-001", "A-002", "A-003"], "shelf": "S1" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{physics}");
    let physics_id = id(&physics["id"]);
    assert_eq!(physics["copy_list"].as_array().unwrap().len(), 3);
    let (s, b) = app
        .post(
            &format!("/library/books/{physics_id}/copies"),
            &lib,
            json!({ "accession_nos": ["A-004"] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["available"], 4);
    let (s, b) = app
        .post(
            &format!("/library/books/{physics_id}/copies"),
            &lib,
            json!({ "accession_nos": ["a-001"] }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    let (s, urdu) = app
        .post(
            "/library/books",
            &lib,
            json!({ "title": "Urdu Reader", "category": "Languages", "accession_nos": ["U-001"] }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{urdu}");
    let (s, b) = app.get("/library/books?q=A-002", &teacher).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b.as_array().unwrap().len(), 1);
    let (s, b) = app
        .post("/library/books", &teacher, json!({ "title": "X" }))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");

    // Members: search finds Ali and Sana.
    let (s, m) = app.get("/library/members?q=a", &lib).await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert!(m
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x["kind"] == "student" && x["name"] == "Ali Khan"));
    assert!(m
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x["kind"] == "staff" && x["name"] == "Sana Malik"));

    // Desk: issue A-001 and A-002 to Ali on 1 April (due 15 April); a third is refused.
    let (s, p) = app
        .post("/library/loans", &lib, json!({ "accession_no": "A-001", "member_kind": "student", "member_id": ali, "issued_on": "2026-04-01" }))
        .await;
    assert_eq!(s, StatusCode::CREATED, "{p}");
    assert_eq!(p["open"], 1);
    assert_eq!(p["loans"][0]["due_on"], "2026-04-15");
    let loan1 = id(&p["loans"][0]["id"]);
    let (s, p) = app
        .post("/library/loans", &lib, json!({ "accession_no": "A-002", "member_kind": "student", "member_id": ali, "issued_on": "2026-04-01" }))
        .await;
    assert_eq!(s, StatusCode::CREATED, "{p}");
    let (s, b) = app
        .post("/library/loans", &lib, json!({ "accession_no": "A-003", "member_kind": "student", "member_id": ali, "issued_on": "2026-04-01" }))
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "limit: {b}");
    let (s, b) = app
        .post("/library/loans", &lib, json!({ "accession_no": "A-001", "member_kind": "staff", "member_id": sana, "issued_on": "2026-04-02" }))
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "already out: {b}");
    let (s, b) = app
        .post(
            "/library/loans",
            &teacher,
            json!({ "accession_no": "A-003", "member_kind": "staff", "member_id": sana }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    let (s, look) = app
        .get("/library/copies/lookup?accession_no=a-001", &teacher)
        .await;
    assert_eq!(s, StatusCode::OK, "{look}");
    assert_eq!(look["status"], "on_loan");
    assert_eq!(look["loan"]["member_name"], "Ali Khan");
    assert_eq!(look["loan"]["member_detail"], "9-A");
    let (s, b) = app.get(&format!("/library/books/{physics_id}"), &lib).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["on_loan"], 2);
    assert_eq!(b["loans"].as_array().unwrap().len(), 2);

    // Overdue list as of 20 April; return A-001 five days late → fine 25; renew is refused
    // when overdue, allowed otherwise once.
    let (s, overdue) = app
        .get("/reports/library.overdue?as_of=2026-04-20", &lib)
        .await;
    assert_eq!(s, StatusCode::OK, "{overdue}");
    assert_eq!(overdue["groups"][0]["rows"].as_array().unwrap().len(), 2);
    assert_eq!(overdue["groups"][0]["rows"][0][6], "5");
    let (s, r) = app
        .post(
            &format!("/library/loans/{loan1}/return"),
            &lib,
            json!({ "returned_on": "2026-04-20" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(num(&r["fine"]), 25.0);
    let (s, b) = app
        .post(&format!("/library/loans/{loan1}/return"), &lib, json!({}))
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    let loan2 = id(&p["loans"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["accession_no"] == "A-002")
        .unwrap()["id"]);
    // Today is long after 15 April 2026, so the second loan is overdue: renewal refused.
    let (s, b) = app
        .post(&format!("/library/loans/{loan2}/renew"), &lib, json!({}))
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    // A fresh loan to Sana (staff: 30 days, 2 renewals) renews.
    let (s, p) = app
        .post(
            "/library/loans",
            &lib,
            json!({ "accession_no": "A-003", "member_kind": "staff", "member_id": sana }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{p}");
    let loan3 = id(&p["loans"][0]["id"]);
    let (s, r) = app
        .post(&format!("/library/loans/{loan3}/renew"), &lib, json!({}))
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["renewals"], 1);

    // Fines: waive 10 with a reason, settle the rest at the desk; then a fee head makes the
    // next student fine a voucher.
    let (s, b) = app
        .post(
            &format!("/library/loans/{loan1}/waive"),
            &lib,
            json!({ "amount": 30, "reason": "x" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    let (s, b) = app
        .post(
            &format!("/library/loans/{loan1}/waive"),
            &lib,
            json!({ "amount": 10, "reason": "First time" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(num(&b["fine_waived"]), 10.0);
    let (s, page) = app
        .get(&format!("/library/members/student/{ali}"), &lib)
        .await;
    assert_eq!(s, StatusCode::OK, "{page}");
    assert_eq!(num(&page["fines_due"]), 15.0);
    let (s, st) = app
        .post(&format!("/library/loans/{loan1}/settle"), &lib, json!({}))
        .await;
    assert_eq!(s, StatusCode::OK, "{st}");
    assert!(st["voucher_id"].is_null());
    assert!(st["loan"]["fine_settled_on"].is_string());
    let (s, head) = app
        .post(
            "/fees/heads",
            &admin,
            json!({ "name": "Library fine", "code": "LIB", "is_recurring": false }),
        )
        .await;
    assert!(s.is_success(), "{head}");
    let (s, b) = app
        .put(
            "/library/settings",
            &lib,
            json!({ "rules": settings["rules"], "fine_fee_head_id": head["id"] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, r) = app
        .post(
            &format!("/library/loans/{loan2}/return"),
            &lib,
            json!({ "returned_on": "2026-04-17" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(num(&r["fine"]), 10.0);
    let (s, st) = app
        .post(&format!("/library/loans/{loan2}/settle"), &lib, json!({}))
        .await;
    assert_eq!(s, StatusCode::OK, "{st}");
    assert!(st["voucher_id"].is_string());
    let (s, v) = app
        .get(&format!("/fees/vouchers?student_id={ali}"), &admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v.as_array().unwrap().len(), 1);
    assert_eq!(num(&v[0]["total_due"]), 10.0);

    // Lost copy: Sana loses A-003, charged 500; the copy is lost in the stock take.
    let (s, r) = app
        .post(
            &format!("/library/loans/{loan3}/return"),
            &lib,
            json!({ "lost": true, "lost_charge": 500 }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["lost"], true);
    assert_eq!(num(&r["fine"]), 500.0);
    let (s, stock) = app.get("/library/stock", &teacher).await;
    assert_eq!(s, StatusCode::OK, "{stock}");
    let science = stock
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["category"] == "Science")
        .unwrap();
    assert_eq!(science["copies"], 4);
    assert_eq!(science["lost"], 1);
    assert_eq!(science["available"], 3);
    let (s, copies) = app.get("/library/copies?status=lost", &lib).await;
    assert_eq!(s, StatusCode::OK, "{copies}");
    assert_eq!(copies[0]["accession_no"], "A-003");
    let (s, b) = app
        .put(
            &format!("/library/copies/{}", id(&copies[0]["id"])),
            &lib,
            json!({ "shelf": "S1", "condition": "fair", "status": "available" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["status"], "available");
    let (s, b) = app
        .call(
            Method::DELETE,
            &format!("/library/books/{physics_id}"),
            Some(&lib),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{b}");

    // Loans list states and the reports.
    let (s, fines) = app.get("/library/loans?state=fines", &lib).await;
    assert_eq!(s, StatusCode::OK, "{fines}");
    assert_eq!(
        fines.as_array().unwrap().len(),
        1,
        "Sana's lost charge: {fines}"
    );
    let (s, r) = app
        .get("/reports/library.fines?from=2026-04-01&to=2026-04-30", &lib)
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["groups"][0]["rows"].as_array().unwrap().len(), 2);
    assert_eq!(r["total"][6], "25.00");
    let (s, r) = app.get("/reports/library.most_borrowed", &lib).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["groups"][0]["rows"][0][3], "3");
    let (s, cat) = app.get("/reports", &teacher).await;
    assert_eq!(s, StatusCode::OK, "{cat}");
    assert!(cat
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["module"] == "library"));
    for path in [
        "/reports/library.stock?format=pdf&lang=ur",
        "/reports/library.overdue?format=pdf",
        "/reports/library.most_borrowed?format=pdf",
    ] {
        let (s, ctype, bytes) = app.get_bytes(path, &lib).await;
        assert_eq!(s, StatusCode::OK, "{path}");
        assert_eq!(ctype, "application/pdf");
        assert!(bytes.starts_with(b"%PDF"));
    }

    // Back on Lite the module is refused.
    let (s, _) = app
        .put("/settings/edition", &admin, json!({ "edition": "lite" }))
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app.get("/library/loans", &lib).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    app.finish().await;
}
