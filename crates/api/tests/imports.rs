//! M1 stage 6 import and export API tests. Need PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

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
    async fn get(&self, p: &str, t: &str) -> (StatusCode, Value) {
        self.call(Method::GET, p, Some(t), None).await
    }
    async fn post(&self, p: &str, t: &str, b: Value) -> (StatusCode, Value) {
        self.call(Method::POST, p, Some(t), Some(b)).await
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

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

/// A messy students workbook: Urdu digits, day-first dates, odd headers, a blank row, a
/// within-file duplicate, an unknown class and a bad B-Form.
fn messy_students_xlsx() -> Vec<u8> {
    let mut wb = rust_xlsxwriter::Workbook::new();
    let ws = wb.add_worksheet();
    let headers = [
        "Sr.",
        "Student Name",
        "Father's Name",
        "DOB",
        "Sex",
        "B-Form No",
        "Class",
        "Section",
        "Mobile",
        "Roll No",
    ];
    for (c, hname) in headers.iter().enumerate() {
        ws.write_string(0, c as u16, *hname).unwrap();
    }
    let rows: Vec<Vec<&str>> = vec![
        vec![
            "1",
            "  Ali   Khan",
            "Ahmed Khan",
            "04/03/2015",
            "M",
            "42101-1234567-1",
            "Class 1",
            "A",
            "۰۳۰۰۱۲۳۴۵۶۷",
            "7",
        ],
        vec![
            "2",
            "Sara Khan",
            "Ahmed Khan",
            "2016-05-06",
            "Female",
            "",
            "1",
            "a",
            "0300 1234567",
            "8",
        ],
        vec!["", "", "", "", "", "", "", "", "", ""],
        vec![
            "3",
            "Omar Tariq",
            "Tariq",
            "01.01.2015",
            "boy",
            "4210199999999",
            "Grade 1",
            "A",
            "",
            "",
        ],
        vec![
            "4",
            "Ali Khan",
            "Ahmed Khan",
            "04/03/2015",
            "M",
            "42101-1234567-1",
            "1",
            "A",
            "",
            "",
        ],
        vec![
            "5",
            "Zain Malik",
            "Malik",
            "2015-02-02",
            "M",
            "123",
            "9",
            "A",
            "",
            "",
        ],
        vec![
            "6",
            "Hira Noor",
            "Noor",
            "not a date",
            "F",
            "",
            "2",
            "A",
            "03331234567",
            "",
        ],
    ];
    for (r, row) in rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            ws.write_string((r + 1) as u32, c as u16, *v).unwrap();
        }
    }
    wb.save_to_buffer().unwrap()
}

#[tokio::test]
async fn students_import_preview_apply_update_and_rollback() {
    let app = app!();
    let (admin, school_id, ids) = app.seed().await;
    // Class 2 has no section: rows for it must be refused with a clear message.
    let (_, c2b) = app
        .post("/classes", &admin, json!({ "name": "9", "level": 9 }))
        .await;
    let _ = c2b;

    // Template downloads.
    let (s, ctype, bytes) = app
        .get_bytes("/imports/templates/students?format=xlsx", &admin)
        .await;
    assert_eq!(s, StatusCode::OK);
    assert!(ctype.contains("spreadsheetml"));
    assert!(bytes.len() > 1000);
    let (s, ctype, bytes) = app
        .get_bytes("/imports/templates/staff?format=csv", &admin)
        .await;
    assert_eq!((s, ctype.starts_with("text/csv")), (StatusCode::OK, true));
    assert!(String::from_utf8_lossy(&bytes).contains("Staff number"));

    // Parse: headers auto-mapped, blank row dropped.
    let content = STANDARD.encode(messy_students_xlsx());
    let (s, parsed) = app
        .post(
            "/imports/parse",
            &admin,
            json!({ "kind": "students", "file_name": "students.xlsx", "content_base64": content }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{parsed}");
    assert_eq!(parsed["rows"].as_array().unwrap().len(), 6);
    assert_eq!(parsed["mapping"]["full_name"].clone(), 1);
    assert_eq!(parsed["mapping"]["guardian_phone"].clone(), 8);
    assert_eq!(parsed["mapping_saved"], false);
    assert!(parsed["mapping"].get("student_number").is_none());

    let body = |policy: &str, save: bool| json!({ "kind": "students", "file_name": "students.xlsx", "headers": parsed["headers"], "rows": parsed["rows"], "mapping": parsed["mapping"], "options": { "duplicates": policy }, "save_mapping": save });
    // Mapping must cover required fields.
    let mut incomplete = body("skip", false);
    incomplete["mapping"]
        .as_object_mut()
        .unwrap()
        .remove("date_of_birth");
    let (s, e) = app.post("/imports/preview", &admin, incomplete).await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("validation"))
    );

    // Preview: 3 creates, 3 errors (duplicate row, unknown class, bad date / bad B-Form).
    let (s, pv) = app
        .post("/imports/preview", &admin, body("skip", false))
        .await;
    assert_eq!(s, StatusCode::OK, "{pv}");
    assert_eq!(
        (
            pv["summary"]["create"].clone(),
            pv["summary"]["error"].clone(),
            pv["summary"]["total"].clone()
        ),
        (json!(3), json!(3), json!(6))
    );
    let rows = pv["rows"].as_array().unwrap();
    assert_eq!(rows[0]["values"]["full_name"], "Ali Khan");
    assert_eq!(rows[0]["values"]["date_of_birth"], "2015-03-04");
    assert_eq!(rows[0]["values"]["guardian_phone"], "03001234567");
    assert_eq!(rows[0]["values"]["guardian_name"], "Ahmed Khan");
    assert_eq!(rows[1]["values"]["section"], "A");
    assert_eq!(rows[1]["action"], "create");
    assert_eq!(rows[2]["values"]["gender"], "male");
    assert_eq!(rows[3]["action"], "error");
    assert!(
        rows[3]["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("row 2"),
        "{}",
        rows[3]
    );
    assert!(rows[4]["errors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["field"] == "cnic"));
    assert!(rows[4]["errors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["message"].as_str().unwrap().contains("is not set up")));
    assert_eq!(rows[5]["errors"][0]["field"], "date_of_birth");

    // Apply: valid rows imported, mapping saved; shared guardian linked once.
    let (s, result) = app.post("/imports/apply", &admin, body("skip", true)).await;
    assert_eq!(s, StatusCode::OK, "{result}");
    assert_eq!(
        (
            result["batch"]["created_rows"].clone(),
            result["batch"]["skipped_rows"].clone(),
            result["batch"]["status"].as_str()
        ),
        (json!(3), json!(3), Some("applied"))
    );
    let batch_id = result["batch"]["id"].as_str().unwrap().to_string();
    let (_, page) = app.get("/students?sort=name", &admin).await;
    assert_eq!(page["total"], 3);
    let ali_id = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["full_name"] == "Ali Khan")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, ali) = app.get(&format!("/students/{ali_id}"), &admin).await;
    assert_eq!(ali["current"]["section_name"], "A");
    assert_eq!(ali["enrollments"][0]["roll_no"], 7);
    assert_eq!(ali["guardians"].as_array().unwrap().len(), 1);
    assert_eq!(ali["guardians"][0]["phone"], "03001234567");
    assert_eq!(
        ali["siblings"].as_array().unwrap().len(),
        1,
        "Sara shares the guardian"
    );
    // Mapping remembered for the same headings.
    let (_, parsed2) = app
        .post(
            "/imports/parse",
            &admin,
            json!({ "kind": "students", "file_name": "again.xlsx", "content_base64": content }),
        )
        .await;
    assert_eq!(parsed2["mapping_saved"], true);
    let (_, saved) = app.get("/imports/mappings/students", &admin).await;
    assert_eq!(saved["mapping"]["full_name"], 1);

    // Second import of the same file: everyone exists → skip / update per policy.
    let (_, pv) = app
        .post("/imports/preview", &admin, body("skip", false))
        .await;
    assert_eq!(
        (
            pv["summary"]["skip"].clone(),
            pv["summary"]["create"].clone()
        ),
        (json!(3), json!(0))
    );
    assert!(pv["rows"][0]["matched"]
        .as_str()
        .unwrap()
        .starts_with("Ali Khan"));
    let mut update = body("update", false);
    // Change Omar's address via the file: add a column mapped to address.
    update["headers"]
        .as_array_mut()
        .unwrap()
        .push(json!("Address"));
    for r in update["rows"].as_array_mut().unwrap() {
        r.as_array_mut().unwrap().push(json!("New Street"));
    }
    update["mapping"]["address"] = json!(10);
    let (s, result) = app.post("/imports/apply", &admin, update).await;
    assert_eq!(s, StatusCode::OK, "{result}");
    assert_eq!(
        (
            result["batch"]["updated_rows"].clone(),
            result["batch"]["created_rows"].clone()
        ),
        (json!(3), json!(0))
    );
    let (_, ali) = app.get(&format!("/students/{ali_id}"), &admin).await;
    assert_eq!(ali["address"], "New Street");
    let update_batch = result["batch"]["id"].as_str().unwrap().to_string();

    // Roll back the update: the address is empty again. Roll back the first import: gone.
    let (s, b) = app
        .post(
            &format!("/imports/batches/{update_batch}/rollback"),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(
        (s, b["status"].as_str()),
        (StatusCode::OK, Some("rolled_back"))
    );
    let (_, ali) = app.get(&format!("/students/{ali_id}"), &admin).await;
    assert_eq!(ali["address"], "");
    let (s, _) = app
        .post(
            &format!("/imports/batches/{batch_id}/rollback"),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (_, page) = app.get("/students", &admin).await;
    assert_eq!(page["total"], 0);
    let (s, _) = app
        .post(
            &format!("/imports/batches/{batch_id}/rollback"),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "already rolled back");
    let (_, batches) = app.get("/imports/batches", &admin).await;
    assert_eq!(batches.as_array().unwrap().len(), 2);
    let (_, audit) = app.get("/audit?limit=200", &admin).await;
    let entities: Vec<&str> = audit["items"]
        .as_array()
        .or_else(|| audit.as_array())
        .unwrap()
        .iter()
        .filter_map(|e| e["entity"].as_str())
        .collect();
    assert!(entities.contains(&"import_batch"));

    // Teachers cannot import.
    let (_, teacher) = app
        .user_with_role(&school_id, "sana", RoleCode::Teacher)
        .await;
    let (s, _) = app.post("/imports/parse", &teacher, json!({ "kind": "students", "file_name": "x.csv", "content_base64": STANDARD.encode("Name\nAli") })).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let _ = ids;
    app.finish().await;
}

#[tokio::test]
async fn staff_csv_and_opening_balances_and_exports() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("madrasa_api=error,madrasa_db=error,sqlx=warn")
        .with_test_writer()
        .try_init();
    let app = app!();
    let (admin, _school_id, ids) = app.seed().await;
    // Staff from a semicolon CSV with a BOM and Urdu heading.
    let csv = "\u{feff}Employee ID;Name;Designation;Joining Date;فون\nS-0001;Sana Malik;Teacher;01/08/2020;0300-1111111\n;Hira Noor;Teacher;2021-03-01;12\nS-0001;Sana Malik;Head;2020-08-01;\n";
    let (s, parsed) = app
        .post("/imports/parse", &admin, json!({ "kind": "staff", "file_name": "staff.csv", "content_base64": STANDARD.encode(csv) }))
        .await;
    assert_eq!(s, StatusCode::OK, "{parsed}");
    assert_eq!(
        (
            parsed["mapping"]["staff_number"].clone(),
            parsed["mapping"]["phone"].clone()
        ),
        (json!(0), json!(4))
    );
    let body = json!({ "kind": "staff", "file_name": "staff.csv", "headers": parsed["headers"], "rows": parsed["rows"], "mapping": parsed["mapping"], "options": { "duplicates": "skip" } });
    let (s, pv) = app.post("/imports/preview", &admin, body.clone()).await;
    assert_eq!(s, StatusCode::OK, "{pv}");
    assert_eq!(pv["rows"][0]["action"], "create");
    assert_eq!(pv["rows"][1]["errors"][0]["field"], "phone");
    assert!(
        pv["rows"][2]["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("row 2"),
        "duplicate staff number in file"
    );
    let (s, result) = app.post("/imports/apply", &admin, body).await;
    assert_eq!(
        (s, result["batch"]["created_rows"].clone()),
        (StatusCode::OK, json!(1))
    );
    let (_, list) = app.get("/staff", &admin).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["joined_on"], "2020-08-01");

    // Opening balances for two students, one by number and one by legacy ID.
    let ali = app.student(&admin, "Ali", &ids["s1"]).await;
    let (_, ali_detail) = app.get(&format!("/students/{ali}"), &admin).await;
    let number = ali_detail["student_number"].as_str().unwrap().to_string();
    let (s, _) = app
        .call(
            Method::PATCH,
            &format!("/students/{ali}"),
            Some(&admin),
            Some(json!({ "legacy_id": "OLD-7" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let csv = format!("Student number,Legacy ID,Amount,Note\n{number},,\"Rs. 4,500.50\",old register\n,OLD-7,100,dup\n,NOPE,50,\n");
    let (_, parsed) = app
        .post("/imports/parse", &admin, json!({ "kind": "opening_balances", "file_name": "balances.csv", "content_base64": STANDARD.encode(csv) }))
        .await;
    let body = json!({ "kind": "opening_balances", "file_name": "balances.csv", "headers": parsed["headers"], "rows": parsed["rows"], "mapping": parsed["mapping"] });
    let (s, pv) = app.post("/imports/preview", &admin, body.clone()).await;
    assert_eq!(s, StatusCode::OK, "{pv}");
    assert_eq!(pv["rows"][0]["action"], "create");
    assert_eq!(pv["rows"][0]["values"]["amount"], "4500.50");
    assert!(
        pv["rows"][1]["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("row 2"),
        "same student twice"
    );
    assert_eq!(pv["rows"][2]["errors"][0]["field"], "student_number");
    let (s, result) = app.post("/imports/apply", &admin, body).await;
    assert_eq!(
        (s, result["batch"]["created_rows"].clone()),
        (StatusCode::OK, json!(1))
    );
    let (_, ledger) = app
        .get(&format!("/fees/students/{ali}/ledger"), &admin)
        .await;
    assert_eq!(ledger["totals"]["balance"], "4500.50");
    assert_eq!(ledger["vouchers"][0]["gross"], "4500.50");
    let batch = result["batch"]["id"].as_str().unwrap();
    let (s, _) = app
        .post(
            &format!("/imports/batches/{batch}/rollback"),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (_, ledger) = app
        .get(&format!("/fees/students/{ali}/ledger"), &admin)
        .await;
    // Zero decimals serialize without a scale.
    assert_eq!(ledger["totals"]["balance"], "0");

    // Exports: every list in both formats.
    for path in [
        "/export/students?format=xlsx",
        "/export/students?format=csv&status=active",
        "/export/staff?format=csv",
        "/export/fees/vouchers?format=xlsx",
        "/export/fees/defaulters?format=csv",
        "/export/attendance/absentees?format=xlsx",
        &format!(
            "/export/attendance/register?format=csv&section_id={}&month=2026-04-01",
            ids["s1"].as_str().unwrap()
        ),
    ] {
        let (s, ctype, bytes) = app.get_bytes(path, &admin).await;
        assert_eq!(s, StatusCode::OK, "{path}");
        if path.contains("csv") {
            assert!(ctype.starts_with("text/csv"), "{path}: {ctype}");
            assert!(bytes.starts_with(&[0xEF, 0xBB, 0xBF]), "BOM for Excel");
        } else {
            assert!(ctype.contains("spreadsheetml"), "{path}: {ctype}");
            assert!(bytes.starts_with(b"PK"), "zip container");
        }
    }
    let (_, _, bytes) = app.get_bytes("/export/students?format=csv", &admin).await;
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("Student number,Name"));
    assert!(text.contains("Ali"));
    let (s, _, _) = app.get_bytes("/export/students?format=pdf", &admin).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    app.finish().await;
}
