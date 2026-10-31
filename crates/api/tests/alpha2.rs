//! Second alpha test report: guardian matching, setup defaults, teacher redaction and the new
//! import fields. Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

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

fn csv(content: &str) -> String {
    STANDARD.encode(content.trim_start())
}

async fn import_csv(app: &App, token: &str, kind: &str, content: &str) -> (Value, Value) {
    let (s, parsed) = app
        .post(
            "/imports/parse",
            token,
            json!({ "kind": kind, "file_name": format!("{kind}.csv"), "content_base64": csv(content) }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{parsed}");
    let body = json!({ "kind": kind, "file_name": format!("{kind}.csv"), "headers": parsed["headers"], "rows": parsed["rows"], "mapping": parsed["mapping"], "options": { "duplicates": "skip" }, "save_mapping": false });
    let (s, preview) = app.post("/imports/preview", token, body.clone()).await;
    assert_eq!(s, StatusCode::OK, "{preview}");
    let (s, result) = app.post("/imports/apply", token, body).await;
    assert_eq!(s, StatusCode::OK, "{result}");
    (preview, result)
}

async fn voucher_of(app: &App, token: &str, d: &Value) -> Value {
    let (_, list) = app
        .get(
            &format!("/fees/vouchers?student_id={}", d["id"].as_str().unwrap()),
            token,
        )
        .await;
    list[0].clone()
}

async fn detail_by_number(app: &App, token: &str, number: &str) -> Value {
    let (_, page) = app
        .get(&format!("/students?q={number}&page_size=5"), token)
        .await;
    let id = page["items"][0]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("{number}: {page}"));
    app.get(&format!("/students/{id}"), token).await.1
}

#[tokio::test]
async fn guardians_never_merge_on_phone_alone() {
    let app = app!();
    let (admin, _school_id, _ids) = app.seed().await;
    // Four students, one shared phone number: Ali and Sara are a family (FAM-1); Zain's
    // guardian has the same name and phone (so he is their brother too); Omar's guardian
    // only shares the phone and is a different person.
    let (preview, result) = import_csv(
        &app,
        &admin,
        "students",
        r#"
Student Number,Student Name,Father's Name,Date of Birth,Gender,Class,Section,Guardian Name,Guardian Phone,Family ID
S-1,Ali Khan,Ahmed Khan,01/01/2015,M,1,A,Ahmed Khan,0300-1111111,FAM-1
S-2,Sara Khan,Ahmed Khan,02/02/2016,F,2,A,Ahmed Khan,0300-1111111,FAM-1
S-3,Omar Raza,Bilal Raza,03/03/2015,M,1,A,Bilal Raza,0300-1111111,
S-4,Zain Khan,Ahmed Khan,04/04/2014,M,2,A,Ahmed Khan,0300 1111111,
"#,
    )
    .await;
    assert_eq!(
        (
            preview["summary"]["create"].as_u64(),
            preview["summary"]["error"].as_u64()
        ),
        (Some(4), Some(0)),
        "{preview}"
    );
    // Omar's row warns that the phone belongs to someone else; the others do not.
    let rows = preview["rows"].as_array().unwrap();
    let omar = rows
        .iter()
        .find(|r| r["values"]["full_name"] == "Omar Raza")
        .unwrap();
    assert_eq!(omar["warnings"].as_array().unwrap().len(), 1, "{omar}");
    assert!(
        omar["warnings"][0]["message"]
            .as_str()
            .unwrap()
            .contains("Ahmed Khan"),
        "{omar}"
    );
    let ali = rows
        .iter()
        .find(|r| r["values"]["full_name"] == "Ali Khan")
        .unwrap();
    assert!(ali["warnings"].as_array().unwrap().is_empty(), "{ali}");
    assert_eq!(result["summary"]["create"].as_u64(), Some(4));

    let ali = detail_by_number(&app, &admin, "S-1").await;
    let sara = detail_by_number(&app, &admin, "S-2").await;
    let omar = detail_by_number(&app, &admin, "S-3").await;
    let zain = detail_by_number(&app, &admin, "S-4").await;
    let g = |d: &Value| d["guardians"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(g(&ali), g(&sara), "family id");
    assert_eq!(g(&ali), g(&zain), "same name and phone");
    assert_ne!(g(&ali), g(&omar), "same phone, different person");
    assert_eq!(omar["guardians"][0]["full_name"], "Bilal Raza");
    assert_eq!(omar["guardians"][0]["phone"], "03001111111");
    assert_eq!(ali["siblings"].as_array().unwrap().len(), 2, "{ali}");
    assert!(omar["siblings"].as_array().unwrap().is_empty(), "{omar}");

    // The sibling discount follows the corrected guardians.
    let (s, rep) = app
        .post(
            "/fees/vouchers/generate",
            &admin,
            json!({ "month": "2026-04-01", "issue_date": "2026-04-01" }),
        )
        .await;
    assert_eq!(
        (s, rep["created"].as_u64()),
        (StatusCode::OK, Some(4)),
        "{rep}"
    );
    assert_eq!(
        voucher_of(&app, &admin, &omar).await["sibling_discount"],
        "0",
        "no siblings"
    );
    assert_ne!(
        voucher_of(&app, &admin, &ali).await["sibling_discount"],
        "0",
        "siblings get the discount"
    );
    assert_ne!(
        voucher_of(&app, &admin, &zain).await["sibling_discount"],
        "0"
    );

    // The same rule on the student form: a guardian typed with a known phone but another name
    // becomes a new record, not a link to the wrong person.
    let (s, b) = app
        .post(
            "/students",
            &admin,
            json!({ "full_name": "Hira Baig", "father_name": "Kamran Baig", "date_of_birth": "2015-05-05", "gender": "female",
                    "guardians": [{ "full_name": "Kamran Baig", "relationship": "father", "phone": "0300-1111111" }] }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let (_, hira) = app
        .get(&format!("/students/{}", b["id"].as_str().unwrap()), &admin)
        .await;
    assert_eq!(hira["guardians"][0]["full_name"], "Kamran Baig");
    assert!(hira["siblings"].as_array().unwrap().is_empty());
    app.finish().await;
}

#[tokio::test]
async fn setup_creates_the_first_year_and_default_classes_so_import_works_at_once() {
    let app = app!();
    let (s, b) = app
        .call(
            Method::POST,
            "/setup",
            None,
            Some(json!({ "school_name": "Gulshan", "school_code": "KHI-MC", "academic_year_start_month": 4, "country": "PK",
                         "first_year": true, "default_classes": true,
                         "admin_full_name": "Ayesha", "admin_username": "ayesha", "admin_password": "admin-password-1", "backup_password": "backup-password-1" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let admin = app.login("ayesha", "admin-password-1").await;
    let (s, years) = app.get("/academic-years", &admin).await;
    assert_eq!(s, StatusCode::OK, "{years}");
    let years = years.as_array().unwrap();
    assert_eq!(years.len(), 1, "{years:?}");
    assert_eq!(years[0]["is_active"], true);
    assert_eq!(years[0]["start_month"], 4);
    let today = chrono::Local::now().date_naive();
    let expected_start = madrasa_api::routes::setup::first_year_start(today, 4);
    assert_eq!(
        years[0]["name"],
        format!("{expected_start}-{:02}", (expected_start + 1) % 100)
    );
    let (_, classes) = app.get("/classes", &admin).await;
    let names: Vec<&str> = classes
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names[0], "KG");
    assert_eq!(names.len(), 13);
    let (_, sections) = app
        .get(
            &format!(
                "/sections?academic_year_id={}",
                years[0]["id"].as_str().unwrap()
            ),
            &admin,
        )
        .await;
    assert_eq!(sections.as_array().unwrap().len(), 13, "{sections}");
    // Import straight after setup: class 1, section A exist.
    let (preview, result) = import_csv(
        &app,
        &admin,
        "students",
        r#"
Student Name,Father's Name,Date of Birth,Gender,Class,Section
Ali Khan,Ahmed Khan,01/01/2015,M,1,A
Sara Khan,Ahmed Khan,02/02/2016,F,KG,A
"#,
    )
    .await;
    assert_eq!(
        (
            preview["summary"]["create"].as_u64(),
            preview["summary"]["error"].as_u64()
        ),
        (Some(2), Some(0)),
        "{preview}"
    );
    assert_eq!(result["summary"]["create"].as_u64(), Some(2));
    app.finish().await;
}

#[tokio::test]
async fn setup_without_the_flags_creates_no_year() {
    let app = app!();
    let (s, _) = app
        .call(
            Method::POST,
            "/setup",
            None,
            Some(json!({ "school_name": "Gulshan", "school_code": "KHI-MC", "academic_year_start_month": 4,
                         "admin_full_name": "Ayesha", "admin_username": "ayesha", "admin_password": "admin-password-1", "backup_password": "backup-password-1" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let admin = app.login("ayesha", "admin-password-1").await;
    let (_, years) = app.get("/academic-years", &admin).await;
    assert!(years.as_array().unwrap().is_empty());
    let (_, classes) = app.get("/classes", &admin).await;
    assert!(classes.as_array().unwrap().is_empty());
    app.finish().await;
}

#[tokio::test]
async fn teachers_do_not_see_colleagues_contact_details() {
    let app = app!();
    let (admin, school_id, _ids) = app.seed().await;
    let (s, st) = app
        .post(
            "/staff",
            &admin,
            json!({ "full_name": "Sana Malik", "phone": "0300-2222222", "email": "sana@example.com", "cnic": "4210112345671" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{st}");
    assert_eq!(st["phone"], "03002222222");
    let teacher = app
        .user_with_role(&school_id, "tch", RoleCode::Teacher)
        .await;
    let (s, list) = app.get("/staff", &teacher).await;
    assert_eq!(s, StatusCode::OK, "{list}");
    let sana = list
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["full_name"] == "Sana Malik")
        .unwrap();
    assert!(
        sana["phone"].is_null() && sana["email"].is_null() && sana["cnic"].is_null(),
        "{sana}"
    );
    let (_, one) = app
        .get(&format!("/staff/{}", st["id"].as_str().unwrap()), &teacher)
        .await;
    assert!(one["phone"].is_null(), "{one}");
    let (_, one) = app
        .get(&format!("/staff/{}", st["id"].as_str().unwrap()), &admin)
        .await;
    assert_eq!(one["phone"], "03002222222");
    assert_eq!(one["cnic"], "42101-1234567-1");
    app.finish().await;
}

#[tokio::test]
async fn new_import_fields_are_stored_and_unmapped_columns_are_reported() {
    let app = app!();
    let (admin, _school_id, _ids) = app.seed().await;
    let (preview, _) = import_csv(
        &app,
        &admin,
        "students",
        r#"
Student Number,Student Name,Father's Name,Date of Birth,Gender,Class,Section,Status,City,Province,Zip Code,Blood Group
S-1,Ali Khan,Ahmed Khan,01/01/2015,M,1,A,Withdrawn,Karachi,Sindh,75300,O+
S-2,Sara Khan,Ahmed Khan,02/02/2016,F,2,A,Active,Lahore,Punjab,54000,A+
"#,
    )
    .await;
    assert_eq!(preview["summary"]["error"].as_u64(), Some(0), "{preview}");
    assert_eq!(preview["unmapped"], json!(["Blood Group"]), "{preview}");
    let ali = detail_by_number(&app, &admin, "S-1").await;
    assert_eq!(ali["status"], "left");
    assert_eq!(ali["custom"]["city"], "Karachi");
    assert_eq!(ali["custom"]["state"], "Sindh");
    assert_eq!(ali["custom"]["postal_code"], "75300");
    let sara = detail_by_number(&app, &admin, "S-2").await;
    assert_eq!(sara["status"], "active");

    let (preview, _) = import_csv(
        &app,
        &admin,
        "staff",
        r#"
Staff Number,Name,Type,Subject/Department,Grades Taught,Hire Date
T-1,Sana Malik,Teaching,Mathematics,6-8,15/08/2020
"#,
    )
    .await;
    assert_eq!(preview["summary"]["error"].as_u64(), Some(0), "{preview}");
    assert!(preview["unmapped"].as_array().unwrap().is_empty());
    let (_, staff) = app.get("/staff?q=T-1", &admin).await;
    let sana = &staff[0];
    assert_eq!(sana["custom"]["staff_type"], "Teaching");
    assert_eq!(sana["custom"]["department"], "Mathematics");
    assert_eq!(sana["custom"]["grades"], "6-8");
    assert_eq!(sana["joined_on"], "2020-08-15");
    app.finish().await;
}
