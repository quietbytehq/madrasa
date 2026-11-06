//! M2 stage 1: the report library (catalogue, student reports, PDF/Excel, permissions,
//! audit). Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

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

    /// Setup, admin login, year 2026-27 active, class 1 section A, class 2 section A.
    async fn seed(&self) -> (String, String, Value) {
        let (s, b) = self.call(Method::POST, "/setup", None, Some(json!({ "school_name": "Gulshan High", "school_code": "KHI-MC", "academic_year_start_month": 4, "admin_full_name": "Ayesha", "admin_username": "ayesha", "admin_password": "admin-password-1", "backup_password": "backup-password-1" }))).await;
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
        let (s, _) = self
            .put(
                &format!("/classes/{}/subjects", c1["id"].as_str().unwrap()),
                &admin,
                json!({ "subject_ids": [eng["id"]] }),
            )
            .await;
        assert_eq!(s, StatusCode::OK);
        (
            admin,
            school_id,
            json!({ "year": year["id"], "c1": c1["id"], "c2": c2["id"], "s1": s1["id"], "s2": s2["id"], "eng": eng["id"] }),
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

fn student(name: &str, father: &str, dob: &str, gender: &str, section: &Value, roll: i32) -> Value {
    json!({
        "full_name": name, "father_name": father, "date_of_birth": dob, "gender": gender,
        "admission_date": "2026-04-10", "section_id": section, "roll_no": roll,
    })
}

#[tokio::test]
async fn student_reports_run_in_both_languages_as_json_pdf_and_excel() {
    let app = app!();
    let (admin, school_id, ids) = app.seed().await;

    let mut ali = student(
        "Ali Khan",
        "Ahmed Khan",
        "2015-03-04",
        "male",
        &ids["s1"],
        1,
    );
    ali["cnic"] = json!("42101-1234567-1");
    ali["address"] = json!("House 4, Gulshan");
    ali["guardians"] = json!([
        { "full_name": "Ahmed Khan", "relationship": "father", "phone": "0300-1111111", "is_primary": true },
        { "full_name": "Nadia Khan", "relationship": "mother" }
    ]);
    let (s, a) = app.post("/students", &admin, ali).await;
    assert_eq!(s, StatusCode::CREATED, "{a}");
    let mut sara = student(
        "Sara Khan",
        "Imran Khan",
        "2016-11-20",
        "female",
        &ids["s1"],
        2,
    );
    sara["guardians"] = json!([{ "full_name": "Imran Khan", "relationship": "father" }]);
    let (s, sr) = app.post("/students", &admin, sara).await;
    assert_eq!(s, StatusCode::CREATED, "{sr}");
    let (s, b) = app
        .post(
            "/students",
            &admin,
            student(
                "Bilal Ahmed",
                "Rashid Ahmed",
                "2014-07-07",
                "male",
                &ids["s2"],
                1,
            ),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let (s, w) = app
        .post(
            &format!("/students/{}/withdraw", b["id"].as_str().unwrap()),
            &admin,
            json!({ "status": "left", "left_on": "2026-06-30", "reason": "Moved" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{w}");

    // Catalogue, in Urdu.
    let (s, cat) = app.get("/reports?lang=ur", &admin).await;
    assert_eq!(s, StatusCode::OK, "{cat}");
    assert_eq!(cat[0]["label"], "طلبہ", "{cat}");
    let codes: Vec<&str> = cat[0]["reports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["code"].as_str().unwrap())
        .collect();
    assert!(
        codes.contains(&"students.list") && codes.contains(&"students.missing"),
        "{codes:?}"
    );
    assert_eq!(cat[0]["reports"][0]["params"][1]["kind"], "class");

    // Student list: active students grouped by class/section with the primary guardian.
    let (s, r) = app.get("/reports/students.list", &admin).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["row_count"], 2, "{r}");
    assert_eq!(r["school_name"], "Gulshan High");
    assert_eq!(r["groups"][0]["heading"], "Class 1 · A");
    let first = &r["groups"][0]["rows"][0];
    assert_eq!(first[1], "KHI-MC-2026-00001");
    assert_eq!(first[2], "Ali Khan");
    assert_eq!(first[4], "Male");
    assert_eq!(first[5], "04-03-2015");
    assert_eq!(first[6], "1");
    assert_eq!(first[7], "Ahmed Khan");
    assert_eq!(first[8], "03001111111");
    assert_eq!(r["groups"][0]["subtotal"][2], "2");
    assert_eq!(r["total"][2], "2");
    assert!(r["params"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["label"] == "Status" && p["value"] == "Active"));
    let (_, left) = app.get("/reports/students.list?status=left", &admin).await;
    assert_eq!(left["row_count"], 1);
    assert_eq!(left["groups"][0]["rows"][0][2], "Bilal Ahmed");
    let (_, sec) = app
        .get(
            &format!(
                "/reports/students.list?section_id={}",
                ids["s2"].as_str().unwrap()
            ),
            &admin,
        )
        .await;
    assert_eq!(sec["row_count"], 0, "{sec}");
    assert!(sec["params"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["label"] == "Class" && p["value"] == "2"));

    // Class strength.
    let (_, st) = app.get("/reports/students.strength", &admin).await;
    let rows = st["groups"][0]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{st}");
    assert_eq!(
        rows[0],
        json!(["1", "A", "", "1", "1", "0", "2", ""]),
        "{st}"
    );
    assert_eq!(rows[1][6], "0");
    assert_eq!(st["total"][6], "2");

    // Admissions and withdrawals in a period.
    let (s, mv) = app
        .get(
            "/reports/students.movements?from=2026-04-01&to=2026-06-30",
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{mv}");
    assert_eq!(mv["groups"][0]["heading"], "Admission (3)");
    assert_eq!(mv["groups"][1]["heading"], "Withdrawal (1)");
    assert_eq!(mv["groups"][1]["rows"][0][3], "Bilal Ahmed");
    assert_eq!(mv["groups"][1]["rows"][0][8], "Moved");
    let (_, none) = app
        .get(
            "/reports/students.movements?from=2026-07-01&to=2026-07-31",
            &admin,
        )
        .await;
    assert_eq!(none["row_count"], 0);
    let (s, e) = app
        .get("/reports/students.movements?from=2026-07-01", &admin)
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("validation"))
    );
    let (s, _) = app
        .get(
            "/reports/students.movements?from=2026-07-01&to=2026-06-01",
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Guardian contacts: one row per guardian link, primary marked.
    let (_, ct) = app.get("/reports/students.contacts", &admin).await;
    assert_eq!(ct["row_count"], 3, "{ct}");
    let crows = ct["groups"][0]["rows"].as_array().unwrap();
    assert_eq!(
        (
            crows[0][1].as_str(),
            crows[0][3].as_str(),
            crows[0][4].as_str(),
            crows[0][5].as_str()
        ),
        (
            Some("Ali Khan"),
            Some("Ahmed Khan"),
            Some("Father"),
            Some("Yes")
        )
    );
    assert_eq!(
        (
            crows[1][3].as_str(),
            crows[1][4].as_str(),
            crows[1][5].as_str()
        ),
        (Some("Nadia Khan"), Some("Mother"), Some(""))
    );

    // Birthdays in November: Sara, turning 10 in 2026.
    let (_, bd) = app
        .get("/reports/students.birthdays?month=2026-11", &admin)
        .await;
    assert_eq!(bd["row_count"], 1, "{bd}");
    assert_eq!(
        bd["groups"][0]["rows"][0],
        json!([
            "20",
            "KHI-MC-2026-00002",
            "Sara Khan",
            "Imran Khan",
            "1",
            "A",
            "2",
            "20-11-2016",
            "10"
        ])
    );
    let (s, _) = app
        .get("/reports/students.birthdays?month=2026-13", &admin)
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Missing data: Ali lacks a photo only; Sara lacks CNIC, photo, guardian phone and address.
    let (_, md) = app.get("/reports/students.missing", &admin).await;
    let mrows = md["groups"][0]["rows"].as_array().unwrap();
    assert_eq!(mrows.len(), 2, "{md}");
    assert_eq!(mrows[0][7], "photo");
    assert_eq!(mrows[1][7], "CNIC/B-Form, photo, guardian phone, address");

    // Urdu: labels, values and the group heading.
    let (_, ur) = app.get("/reports/students.list?lang=ur", &admin).await;
    assert_eq!(ur["lang"], "ur");
    assert_eq!(ur["title"], "طلبہ کی فہرست");
    assert_eq!(ur["columns"][2]["label"], "نام");
    assert_eq!(ur["groups"][0]["heading"], "جماعت 1 · A");
    assert_eq!(ur["groups"][0]["rows"][0][4], "مرد");

    // Excel and PDF downloads, both languages.
    let (s, ctype, bytes) = app
        .raw(
            Method::GET,
            "/reports/students.list?format=xlsx",
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert!(ctype.contains("spreadsheetml"), "{ctype}");
    assert!(bytes.starts_with(b"PK"));
    for lang in ["en", "ur"] {
        let (s, ctype, bytes) = app
            .raw(
                Method::GET,
                &format!("/reports/students.list?format=pdf&lang={lang}"),
                Some(&admin),
                None,
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{lang}");
        assert_eq!(ctype, "application/pdf");
        assert!(bytes.starts_with(b"%PDF"), "{lang}");
    }
    let (s, _) = app.get("/reports/students.list?format=docx", &admin).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = app.get("/reports/nope", &admin).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Permissions: accountant reads reports; a teacher does not hold reports.read.
    let accountant = app
        .user_with_role(&school_id, "acc", RoleCode::Accountant)
        .await;
    let (s, _) = app.get("/reports/students.strength", &accountant).await;
    assert_eq!(s, StatusCode::OK);
    let teacher = app
        .user_with_role(&school_id, "tch", RoleCode::Teacher)
        .await;
    let (s, _) = app.get("/reports", &teacher).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.get("/reports/students.list", &teacher).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Every run is in the audit log with its code and parameters.
    let (_, log) = app.get("/audit?limit=100", &admin).await;
    let runs: Vec<&Value> = log
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["action"] == "report")
        .collect();
    assert!(runs.len() >= 10, "{}", runs.len());
    assert!(
        runs.iter()
            .any(|e| e["new_value"]["code"] == "students.birthdays"
                && e["new_value"]["params"]["month"] == "2026-11"
                && e["new_value"]["rows"] == 1),
        "{log}"
    );
    app.finish().await;
}
