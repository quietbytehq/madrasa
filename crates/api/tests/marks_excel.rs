//! Marks via Excel: template download and upload through the same validation and approval
//! workflow as on-screen entry. Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

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

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use calamine::Reader;

impl App {
    async fn get_bytes(&self, p: &str, t: &str) -> (StatusCode, Vec<u8>) {
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/api/v1{p}"))
            .header(header::AUTHORIZATION, format!("Bearer {t}"))
            .body(Body::empty())
            .unwrap();
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
        (status, bytes)
    }
}

async fn upload(
    app: &App,
    token: &str,
    assessment_id: &Value,
    section_id: &Value,
    subject_id: &Value,
    content: String,
) -> (StatusCode, Value) {
    app.post(
        "/exams/sheet/upload",
        token,
        json!({ "assessment_id": assessment_id, "section_id": section_id, "subject_id": subject_id, "file_name": "marks.xlsx", "content_base64": content }),
    )
    .await
}

/// A filled template: the same headings, one row per (number, marks, absent).
fn filled(rows: &[(&str, &str, &str)]) -> String {
    let mut wb = rust_xlsxwriter::Workbook::new();
    let ws = wb.add_worksheet();
    for (c, h) in [
        "Student number",
        "Student name",
        "Roll no",
        "Marks (max 50)",
        "Absent (Y)",
    ]
    .iter()
    .enumerate()
    {
        ws.write_string(0, c as u16, *h).unwrap();
    }
    for (i, (number, marks, absent)) in rows.iter().enumerate() {
        let r = (i + 1) as u32;
        ws.write_string(r, 0, *number).unwrap();
        ws.write_string(r, 1, "name ignored").unwrap();
        ws.write_string(r, 3, *marks).unwrap();
        ws.write_string(r, 4, *absent).unwrap();
    }
    STANDARD.encode(wb.save_to_buffer().unwrap())
}

#[tokio::test]
async fn marks_template_round_trips_through_validation_and_approval() {
    let app = app!();
    let (admin, school_id, ids) = app.seed().await;
    let year = ids["year"].as_str().unwrap();
    let s1 = ids["s1"].clone();
    let ali = app.student(&admin, "Ali", &s1).await;
    let sara = app.student(&admin, "Sara", &s1).await;
    let (_, list) = app
        .post(
            "/exams/assessments/defaults",
            &admin,
            json!({ "academic_year_id": year }),
        )
        .await;
    let test1 = list[0]["id"].clone();
    let (s, _) = app
        .put("/exams/max-marks", &admin, json!({ "assessment_id": test1, "class_id": ids["c1"], "lines": [{ "subject_id": ids["eng"], "max_marks": "50" }] }))
        .await;
    assert_eq!(s, StatusCode::OK);
    // Teacher of English in 1-A; another teacher in 2-A.
    let (teacher_user, teacher) = app
        .user_with_role(&school_id, "sana", RoleCode::Teacher)
        .await;
    let (_, st) = app
        .post(
            "/staff",
            &admin,
            json!({ "full_name": "Sana", "user_id": teacher_user }),
        )
        .await;
    app.put(
        &format!("/staff/{}/assignments", st["id"].as_str().unwrap()),
        &admin,
        json!({ "assignments": [{ "section_id": s1, "subject_id": ids["eng"] }] }),
    )
    .await;
    let (other_user, other) = app
        .user_with_role(&school_id, "omar", RoleCode::Teacher)
        .await;
    let (_, st2) = app
        .post(
            "/staff",
            &admin,
            json!({ "full_name": "Omar", "user_id": other_user }),
        )
        .await;
    app.put(
        &format!("/staff/{}/assignments", st2["id"].as_str().unwrap()),
        &admin,
        json!({ "assignments": [{ "section_id": ids["s2"], "subject_id": ids["eng"] }] }),
    )
    .await;
    let q = format!(
        "assessment_id={}&section_id={}&subject_id={}",
        test1.as_str().unwrap(),
        s1.as_str().unwrap(),
        ids["eng"].as_str().unwrap()
    );

    // The template carries the roster and the maximum.
    let (s, bytes) = app
        .get_bytes(&format!("/exams/sheet/template?{q}"), &teacher)
        .await;
    assert_eq!(s, StatusCode::OK);
    let mut wb = calamine::open_workbook_auto_from_rs(std::io::Cursor::new(bytes)).unwrap();
    let name = wb.sheet_names()[0].clone();
    let range = wb.worksheet_range(&name).unwrap();
    let cells: Vec<Vec<String>> = range
        .rows()
        .map(|r| r.iter().map(|c| c.to_string()).collect())
        .collect();
    assert_eq!(cells[0][0], "Student number");
    assert_eq!(cells[0][3], "Marks (max 50)");
    assert_eq!(cells.len(), 3, "heading + two students: {cells:?}");
    let number_of = |name: &str| {
        cells[1..]
            .iter()
            .find(|r| r[1] == name)
            .map(|r| r[0].clone())
            .unwrap()
    };
    let (ali_no, sara_no) = (number_of("Ali"), number_of("Sara"));

    // Upload: a wrong mark blocks the whole file with the row number; nothing is saved.
    let (s, r) = upload(
        &app,
        &teacher,
        &test1,
        &s1,
        &ids["eng"],
        filled(&[(&ali_no, "60", ""), (&sara_no, "40", "")]),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["saved"], 0);
    assert_eq!(r["issues"][0]["row_no"], 2, "{r}");
    assert!(r["issues"][0]["message"]
        .as_str()
        .unwrap()
        .contains("between 0 and 50"));
    let (_, r) = upload(
        &app,
        &teacher,
        &test1,
        &s1,
        &ids["eng"],
        filled(&[("NOPE-1", "10", ""), (&sara_no, "abc", "")]),
    )
    .await;
    assert_eq!(r["issues"].as_array().unwrap().len(), 2, "{r}");
    let (_, v) = app.get(&format!("/exams/sheet?{q}"), &teacher).await;
    assert!(
        v["students"]
            .as_array()
            .unwrap()
            .iter()
            .all(|st| st["obtained"].is_null()),
        "nothing saved: {v}"
    );

    // A good file saves a draft: Ali 41, Sara absent.
    let (s, r) = upload(
        &app,
        &teacher,
        &test1,
        &s1,
        &ids["eng"],
        filled(&[(&ali_no, "41", ""), (&sara_no, "", "Y")]),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["saved"], 2);
    let students = r["sheet"]["students"].as_array().unwrap();
    let st = |id: &str| {
        students
            .iter()
            .find(|x| x["student_id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(st(&ali)["obtained"], "41.00");
    assert_eq!(st(&sara)["is_absent"], true);
    assert_eq!(r["sheet"]["status"], "draft");
    // The other section's teacher may not upload here; the same workflow then applies.
    let (s, _) = upload(
        &app,
        &other,
        &test1,
        &s1,
        &ids["eng"],
        filled(&[(&ali_no, "30", "")]),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let sheet_id = r["sheet"]["sheet"]["id"].as_str().unwrap().to_string();
    let (s, v) = app
        .post(
            &format!("/exams/sheets/{sheet_id}/submit"),
            &teacher,
            json!({}),
        )
        .await;
    assert_eq!(
        (s, v["status"].as_str()),
        (StatusCode::OK, Some("submitted")),
        "{v}"
    );
    let (s, r) = upload(
        &app,
        &teacher,
        &test1,
        &s1,
        &ids["eng"],
        filled(&[(&ali_no, "45", "")]),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "closed for entry: {r}");
    let (s, v) = app
        .post(
            &format!("/exams/sheets/{sheet_id}/approve"),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(
        (s, v["status"].as_str()),
        (StatusCode::OK, Some("approved")),
        "{v}"
    );
    // Without the template columns the file is refused with a clear message.
    let mut wb = rust_xlsxwriter::Workbook::new();
    wb.add_worksheet().write_string(0, 0, "Name").unwrap();
    let (s, r) = upload(
        &app,
        &teacher,
        &test1,
        &s1,
        &ids["eng"],
        STANDARD.encode(wb.save_to_buffer().unwrap()),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{r}");
    app.finish().await;
}
