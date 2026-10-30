//! M1 stage 4 exams API tests. Need PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

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

#[tokio::test]
async fn setup_marks_workflow_results_and_report_cards() {
    let app = app!();
    let (admin, school_id, ids) = app.seed().await;
    let ali = app.student(&admin, "Ali", &ids["s1"]).await;
    let sara = app.student(&admin, "Sara", &ids["s1"]).await;
    let omar = app.student(&admin, "Omar", &ids["s1"]).await;
    let zain = app.student(&admin, "Zain", &ids["s2"]).await;
    let year = ids["year"].as_str().unwrap();
    let s1 = ids["s1"].as_str().unwrap();
    let c1 = ids["c1"].as_str().unwrap();

    // Assessments: defaults, weights must total 100.
    let (s, list) = app
        .post(
            "/exams/assessments/defaults",
            &admin,
            json!({ "academic_year_id": year }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{list}");
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 4);
    assert_eq!(list[3]["name"], "Final");
    let test1 = list[0]["id"].as_str().unwrap().to_string();
    let weights = |w: [u32; 4]| json!({ "academic_year_id": year, "weights": list.iter().zip(w).map(|(a, w)| json!({ "id": a["id"], "weight": w.to_string() })).collect::<Vec<_>>() });
    let (s, e) = app
        .put(
            "/exams/assessments/weights",
            &admin,
            weights([10, 10, 30, 49]),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("validation"))
    );
    let (s, w) = app
        .put(
            "/exams/assessments/weights",
            &admin,
            weights([10, 10, 30, 50]),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{w}");
    assert_eq!(w[3]["weight"], "50.00");

    // Grading scheme: default, then a custom one; invalid refused.
    let (s, g) = app.get("/exams/grading", &admin).await;
    assert_eq!(
        (s, g["pass_percent"].as_str()),
        (StatusCode::OK, Some("40"))
    );
    let (s, _) = app
        .put(
            "/exams/grading",
            &admin,
            json!({ "bands": [{ "grade": "A", "min_percent": "50" }], "pass_percent": "40" }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "no band starts at 0");

    // Maximum marks per subject for class 1.
    let (s, mm) = app
        .put("/exams/max-marks", &admin, json!({ "assessment_id": test1, "class_id": c1, "lines": [{ "subject_id": ids["eng"], "max_marks": "50" }, { "subject_id": ids["mth"], "max_marks": "50" }] }))
        .await;
    assert_eq!(s, StatusCode::OK, "{mm}");
    assert_eq!(mm.as_array().unwrap().len(), 2);
    assert!(mm
        .as_array()
        .unwrap()
        .iter()
        .all(|l| l["max_marks"] == "50.00"));

    // Teacher assigned to English in 1A.
    let (teacher_user, teacher) = app
        .user_with_role(&school_id, "sana", RoleCode::Teacher)
        .await;
    let (s, st) = app
        .post(
            "/staff",
            &admin,
            json!({ "full_name": "Sana", "user_id": teacher_user }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{st}");
    let staff_id = st["id"].as_str().unwrap().to_string();
    app.put(
        &format!("/staff/{staff_id}/assignments"),
        &admin,
        json!({ "assignments": [{ "section_id": s1, "subject_id": ids["eng"] }] }),
    )
    .await;
    let (s, sheets) = app
        .get(
            &format!("/exams/sheets?assessment_id={test1}&section_id={s1}"),
            &teacher,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{sheets}");
    let sheets = sheets["sheets"].as_array().unwrap();
    let eng_row = sheets
        .iter()
        .find(|r| r["subject_id"] == ids["eng"])
        .unwrap();
    let mth_row = sheets
        .iter()
        .find(|r| r["subject_id"] == ids["mth"])
        .unwrap();
    assert_eq!(
        (eng_row["can_enter"].clone(), mth_row["can_enter"].clone()),
        (json!(true), json!(false))
    );
    assert_eq!(eng_row["students"], 3);

    // Teacher enters English; Maths is out of scope; validation.
    let (s, _) = app
        .put("/exams/sheet", &teacher, json!({ "assessment_id": test1, "section_id": s1, "subject_id": ids["mth"], "marks": [{ "student_id": ali, "obtained": "10" }] }))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, e) = app
        .put("/exams/sheet", &teacher, json!({ "assessment_id": test1, "section_id": s1, "subject_id": ids["eng"], "marks": [{ "student_id": ali, "obtained": "50.5" }] }))
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("conflict")),
        "over the maximum"
    );
    let (s, _) = app
        .put("/exams/sheet", &teacher, json!({ "assessment_id": test1, "section_id": s1, "subject_id": ids["eng"], "marks": [{ "student_id": zain, "obtained": "10" }] }))
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "student of another section");
    let (s, v) = app
        .put("/exams/sheet", &teacher, json!({ "assessment_id": test1, "section_id": s1, "subject_id": ids["eng"], "marks": [{ "student_id": ali, "obtained": "45" }, { "student_id": sara, "is_absent": true }] }))
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(
        (
            v["status"].as_str(),
            v["can_edit"].as_bool(),
            v["can_submit"].as_bool(),
            v["can_approve"].as_bool()
        ),
        (Some("draft"), Some(true), Some(true), Some(false))
    );
    let sheet_id = v["sheet"]["id"].as_str().unwrap().to_string();
    let (s, e) = app
        .post(
            &format!("/exams/sheets/{sheet_id}/submit"),
            &teacher,
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "Omar has no marks: {e}");
    app.put("/exams/sheet", &teacher, json!({ "assessment_id": test1, "section_id": s1, "subject_id": ids["eng"], "marks": [{ "student_id": omar, "obtained": "30" }] })).await;
    let (s, v) = app
        .post(
            &format!("/exams/sheets/{sheet_id}/submit"),
            &teacher,
            json!({}),
        )
        .await;
    assert_eq!(
        (s, v["status"].as_str(), v["submitted_by_name"].as_str()),
        (StatusCode::OK, Some("submitted"), Some("sana"))
    );
    assert_eq!(v["can_edit"], false);
    let (s, _) = app
        .put("/exams/sheet", &teacher, json!({ "assessment_id": test1, "section_id": s1, "subject_id": ids["eng"], "marks": [{ "student_id": ali, "obtained": "46" }] }))
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "locked once submitted");
    let (s, _) = app
        .post(
            &format!("/exams/sheets/{sheet_id}/approve"),
            &teacher,
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "teachers cannot approve");
    let (s, v) = app
        .post(
            &format!("/exams/sheets/{sheet_id}/approve"),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(
        (s, v["status"].as_str(), v["approved_by_name"].as_str()),
        (StatusCode::OK, Some("approved"), Some("Ayesha"))
    );
    // Maximum marks are locked once marks exist.
    let (s, _) = app
        .put("/exams/max-marks", &admin, json!({ "assessment_id": test1, "class_id": c1, "lines": [{ "subject_id": ids["eng"], "max_marks": "60" }] }))
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, v) = app
        .post(
            &format!("/exams/sheets/{sheet_id}/reopen"),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!((s, v["status"].as_str()), (StatusCode::OK, Some("draft")));
    app.post(
        &format!("/exams/sheets/{sheet_id}/submit"),
        &admin,
        json!({}),
    )
    .await;
    app.post(
        &format!("/exams/sheets/{sheet_id}/approve"),
        &admin,
        json!({}),
    )
    .await;

    // Admin enters and approves Maths: Ali 40, Sara 25, Omar 30.
    let (s, v) = app
        .put("/exams/sheet", &admin, json!({ "assessment_id": test1, "section_id": s1, "subject_id": ids["mth"], "marks": [{ "student_id": ali, "obtained": "40" }, { "student_id": sara, "obtained": "25" }, { "student_id": omar, "obtained": "30" }] }))
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let mth_sheet = v["sheet"]["id"].as_str().unwrap().to_string();
    app.post(
        &format!("/exams/sheets/{mth_sheet}/submit"),
        &admin,
        json!({}),
    )
    .await;
    let (s, _) = app
        .post(
            &format!("/exams/sheets/{mth_sheet}/approve"),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);

    // Result sheet: totals, percentage, grade, position, analysis.
    let (s, r) = app
        .get(
            &format!("/exams/results/section?assessment_id={test1}&section_id={s1}"),
            &teacher,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["complete"], true);
    let rows = r["rows"].as_array().unwrap();
    let row = |id: &str| rows.iter().find(|x| x["student_id"] == id).unwrap();
    assert_eq!(
        (
            row(&ali)["total"].as_str(),
            row(&ali)["percent"].as_str(),
            row(&ali)["grade"].as_str(),
            row(&ali)["position"].as_i64()
        ),
        (Some("85.00"), Some("85.00"), Some("A"), Some(1))
    );
    assert_eq!(
        (
            row(&omar)["percent"].as_str(),
            row(&omar)["grade"].as_str(),
            row(&omar)["position"].as_i64()
        ),
        (Some("60.00"), Some("C"), Some(2))
    );
    assert_eq!(
        (
            row(&sara)["percent"].as_str(),
            row(&sara)["grade"].as_str(),
            row(&sara)["pass"].as_bool(),
            row(&sara)["position"].as_i64()
        ),
        (Some("25.00"), Some("F"), Some(false), Some(3))
    );
    let sara_eng = row(&sara)["marks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["subject_id"] == ids["eng"])
        .unwrap();
    assert_eq!(
        (
            sara_eng["is_absent"].as_bool(),
            sara_eng["obtained"].clone()
        ),
        (Some(true), Value::Null)
    );
    let eng_an = r["analysis"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["subject_id"] == ids["eng"])
        .unwrap();
    assert_eq!(
        (
            eng_an["appeared"].as_i64(),
            eng_an["absent"].as_i64(),
            eng_an["average_percent"].as_str(),
            eng_an["highest"].as_str(),
            eng_an["lowest"].as_str(),
            eng_an["passed"].as_i64(),
            eng_an["failed"].as_i64()
        ),
        (
            Some(2),
            Some(1),
            Some("75.00"),
            Some("45.00"),
            Some("30.00"),
            Some(2),
            Some(0)
        )
    );

    // Report card for Ali: only Test 1 held, so weighted = Test 1; position 1 of 3.
    let (s, card) = app
        .get(&format!("/exams/results/student/{ali}"), &teacher)
        .await;
    assert_eq!(s, StatusCode::OK, "{card}");
    assert_eq!(card["full_name"], "Ali");
    assert_eq!(
        (card["class_name"].as_str(), card["section_name"].as_str()),
        (Some("1"), Some("A"))
    );
    assert_eq!(card["assessments"].as_array().unwrap().len(), 4);
    assert_eq!(card["assessments"][0]["held"], true);
    assert_eq!(card["assessments"][1]["held"], false);
    let eng_subject = card["subjects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["subject_id"] == ids["eng"])
        .unwrap();
    assert_eq!(
        (
            eng_subject["cells"][0]["obtained"].as_str(),
            eng_subject["cells"][0]["percent"].as_str(),
            eng_subject["cells"][1]["max_marks"].clone()
        ),
        (Some("45.00"), Some("90.00"), Value::Null)
    );
    assert_eq!(
        (
            eng_subject["weighted_percent"].as_str(),
            eng_subject["grade"].as_str()
        ),
        (Some("90.00"), Some("A+"))
    );
    assert_eq!(
        (
            card["totals"][0]["percent"].as_str(),
            card["totals"][0]["position"].as_i64(),
            card["totals"][0]["students"].as_i64()
        ),
        (Some("85.00"), Some(1), Some(3))
    );
    assert_eq!(card["totals"][1]["percent"], Value::Null);
    assert_eq!(
        (
            card["overall_percent"].as_str(),
            card["overall_grade"].as_str(),
            card["pass"].as_bool()
        ),
        (Some("85.00"), Some("A"), Some(true))
    );
    assert_eq!(card["attendance"]["total"], 0);
    // Bulk cards for the section; teacher scope on another section's student.
    let (s, cards) = app
        .get(
            &format!("/exams/results/section/report-cards?section_id={s1}"),
            &admin,
        )
        .await;
    assert_eq!((s, cards.as_array().unwrap().len()), (StatusCode::OK, 3));
    let (s, _) = app
        .get(&format!("/exams/results/student/{zain}"), &teacher)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app
        .get(
            &format!(
                "/exams/results/section?assessment_id={test1}&section_id={}",
                ids["s2"].as_str().unwrap()
            ),
            &teacher,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // Setup is not for teachers; accountants see nothing.
    let (s, _) = app
        .put(
            "/exams/grading",
            &teacher,
            json!({ "bands": [{ "grade": "A", "min_percent": "0" }], "pass_percent": "40" }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (_, accountant) = app
        .user_with_role(&school_id, "acc", RoleCode::Accountant)
        .await;
    let (s, _) = app
        .get(
            &format!("/exams/assessments?academic_year_id={year}"),
            &accountant,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    app.finish().await;
}
