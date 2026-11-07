//! M2 stage 8: consent manager. Types with class ranges and yearly renewal, consents per
//! student with history, bulk recording per section, and the three consent reports.
//! Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

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

#[tokio::test]
async fn consents_are_recorded_renewed_and_reported() {
    let app = app!();
    let (s, b) = app.call(Method::POST, "/setup", None, Some(json!({ "school_name": "Gulshan High", "school_code": "KHI-MC", "academic_year_start_month": 4, "admin_full_name": "Ayesha", "admin_username": "ayesha", "admin_password": "admin-password-1", "backup_password": "backup-password-1" }))).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let admin = app.login("ayesha", "admin-password-1").await;
    // Two years: last year (for an expired yearly consent) and the active one.
    let (_, last) = app
        .post(
            "/academic-years",
            &admin,
            json!({ "start_year": 2025, "start_month": 4, "activate": true }),
        )
        .await;
    let (_, year) = app
        .post(
            "/academic-years",
            &admin,
            json!({ "start_year": 2026, "start_month": 4, "activate": true }),
        )
        .await;
    let (last_id, year_id) = (id(&last["id"]), id(&year["id"]));
    let (_, c3) = app
        .post("/classes", &admin, json!({ "name": "3", "level": 3 }))
        .await;
    let (_, c9) = app
        .post("/classes", &admin, json!({ "name": "9", "level": 9 }))
        .await;
    let (_, s3a) = app
        .post(
            "/sections",
            &admin,
            json!({ "class_id": c3["id"], "academic_year_id": year_id, "name": "A" }),
        )
        .await;
    let (_, s9a) = app
        .post(
            "/sections",
            &admin,
            json!({ "class_id": c9["id"], "academic_year_id": year_id, "name": "A" }),
        )
        .await;
    let (s3a, s9a) = (id(&s3a["id"]), id(&s9a["id"]));
    let student = |name: &str, section: &str, roll: i32| json!({ "full_name": name, "father_name": "Father", "date_of_birth": "2015-01-01", "gender": "male", "admission_date": "2026-04-01", "section_id": section, "roll_no": roll, "guardians": [{ "full_name": format!("Parent of {name}"), "relationship": "father", "phone": "0300 1234567", "is_primary": true }] });
    let (s, ali) = app
        .post("/students", &admin, student("Ali Khan", &s3a, 1))
        .await;
    assert_eq!(s, StatusCode::CREATED, "{ali}");
    let (_, sara) = app
        .post("/students", &admin, student("Sara Khan", &s3a, 2))
        .await;
    let (_, omar) = app
        .post("/students", &admin, student("Omar Khan", &s9a, 1))
        .await;
    let (ali, sara, omar) = (id(&ali["id"]), id(&sara["id"]), id(&omar["id"]));

    // Types: photos (everyone, once), trips (everyone, yearly), lab (classes 9–12, either).
    let (s, e) = app
        .post(
            "/consent-types",
            &admin,
            json!({ "name": "Lab", "given_by": "either", "required": "class_range", "renewal": "none" }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "class range needs levels: {e}");
    let mut types = Vec::new();
    for body in [
        json!({ "name": "Photos", "description": "School publications", "given_by": "guardian", "required": "all", "renewal": "none", "position": 0 }),
        json!({ "name": "Trips", "given_by": "guardian", "required": "all", "renewal": "yearly", "position": 1 }),
        json!({ "name": "Lab safety", "given_by": "either", "required": "class_range", "min_level": 9, "max_level": 12, "renewal": "none", "position": 2 }),
    ] {
        let (s, t) = app.post("/consent-types", &admin, body).await;
        assert_eq!(s, StatusCode::CREATED, "{t}");
        types.push(id(&t["id"]));
    }
    let (photos, trips, lab) = (types[0].clone(), types[1].clone(), types[2].clone());
    let (s, e) = app
        .post(
            "/consent-types",
            &admin,
            json!({ "name": "photos", "given_by": "guardian", "required": "all", "renewal": "none" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "duplicate name: {e}");

    // Ali (class 3) needs Photos and Trips, not Lab; Omar (class 9) needs all three.
    let (s, mine) = app.get(&format!("/students/{ali}/consents"), &admin).await;
    assert_eq!(s, StatusCode::OK, "{mine}");
    let needed = |v: &Value, t: &str| {
        v["types"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["id"] == t)
            .unwrap()
            .clone()
    };
    assert_eq!(needed(&mine, &photos)["needed"], true);
    assert_eq!(needed(&mine, &lab)["needed"], false);
    assert_eq!(needed(&mine, &photos)["current"], "missing");
    assert_eq!(mine["guardians"].as_array().unwrap().len(), 1);
    let ali_guardian = id(&mine["guardians"][0]["id"]);
    let (_, omar_view) = app.get(&format!("/students/{omar}/consents"), &admin).await;
    assert_eq!(needed(&omar_view, &lab)["needed"], true);
    // A guardian consent marked given needs the guardian; a linked one is required.
    let (s, e) = app
        .post(
            &format!("/students/{ali}/consents"),
            &admin,
            json!({ "consent_type_id": photos, "status": "given" }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    let (s, e) = app
        .post(
            &format!("/students/{ali}/consents"),
            &admin,
            json!({ "consent_type_id": photos, "status": "given", "guardian_id": omar }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "unlinked guardian: {e}");
    let (s, mine) = app
        .post(
            &format!("/students/{ali}/consents"),
            &admin,
            json!({ "consent_type_id": photos, "status": "given", "guardian_id": ali_guardian, "recorded_on": "2026-04-05", "note": "form 1" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{mine}");
    assert_eq!(needed(&mine, &photos)["current"], "given");
    assert_eq!(mine["history"].as_array().unwrap().len(), 1);
    assert_eq!(mine["history"][0]["recorded_by_name"], "Ayesha");
    // Trips given last year: expired this year; given again this year: given. History keeps
    // both rows.
    let (s, mine) = app
        .post(
            &format!("/students/{ali}/consents"),
            &admin,
            json!({ "consent_type_id": trips, "status": "given", "guardian_id": ali_guardian, "recorded_on": "2025-06-01", "academic_year_id": last_id }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{mine}");
    let (_, mine) = app.get(&format!("/students/{ali}/consents"), &admin).await;
    assert_eq!(needed(&mine, &trips)["current"], "expired");
    let (_, mine) = app
        .post(
            &format!("/students/{ali}/consents"),
            &admin,
            json!({ "consent_type_id": trips, "status": "given", "guardian_id": ali_guardian }),
        )
        .await;
    assert_eq!(needed(&mine, &trips)["current"], "given");
    assert_eq!(mine["history"].as_array().unwrap().len(), 3);
    // Withdrawn: a new row, the old one stays.
    let (_, mine) = app
        .post(
            &format!("/students/{ali}/consents"),
            &admin,
            json!({ "consent_type_id": photos, "status": "withdrawn", "note": "called on 10 April" }),
        )
        .await;
    assert_eq!(needed(&mine, &photos)["current"], "withdrawn");
    assert_eq!(mine["history"].as_array().unwrap().len(), 4);

    // Section matrix and bulk recording.
    let (s, sec) = app.get(&format!("/sections/{s3a}/consents"), &admin).await;
    assert_eq!(s, StatusCode::OK, "{sec}");
    assert_eq!(sec["types"].as_array().unwrap().len(), 3);
    // Ali: photos withdrawn → 1; Sara: photos + trips missing → 2. Lab is not needed in class 3.
    assert_eq!(sec["missing"], 3);
    let sara_row = sec["students"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["student_id"] == sara)
        .unwrap()
        .clone();
    let sara_guardian = id(&sara_row["guardians"][0]["id"]);
    assert_eq!(sara_row["cells"][2]["required"], false);
    let (s, sec) = app
        .post(
            &format!("/sections/{s3a}/consents"),
            &admin,
            json!({ "rows": [
                { "student_id": sara, "consent_type_id": photos, "status": "given", "guardian_id": sara_guardian },
                { "student_id": sara, "consent_type_id": trips, "status": "refused" },
                { "student_id": ali, "consent_type_id": photos, "status": "given", "guardian_id": ali_guardian }
            ] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{sec}");
    assert_eq!(sec["missing"], 0);
    let (s, _) = app
        .post(
            &format!("/sections/{s3a}/consents"),
            &admin,
            json!({ "rows": [{ "student_id": omar, "consent_type_id": photos, "status": "given" }] }),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "a student of another section");

    // Reports: missing (Omar: photos, trips, lab), expiring (Ali's trips), forms (Omar).
    let (s, r) = app
        .get(
            &format!("/reports/students.consents_missing?academic_year_id={year_id}"),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["row_count"], 3);
    let (s, r) = app
        .get(
            &format!("/reports/students.consents_missing?academic_year_id={year_id}&consent_type_id={lab}"),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["row_count"], 1);
    assert_eq!(r["groups"][0]["rows"][0][2], "Omar Khan");
    let (s, r) = app
        .get(
            &format!("/reports/students.consents_expiring?academic_year_id={year_id}"),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["row_count"], 1);
    assert_eq!(r["groups"][0]["rows"][0][3], "Ali Khan");
    let (s, r) = app
        .get(&format!("/reports/students.consent_forms?academic_year_id={year_id}&section_id={s9a}&lang=ur&format=pdf"), &admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    let (s, r) = app
        .get(
            &format!("/reports/students.consent_forms?academic_year_id={year_id}"),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["row_count"], 1);
    assert!(r["groups"][0]["rows"][0][6]
        .as_str()
        .unwrap()
        .contains("Lab safety"));

    // Permissions: a teacher sees their own section but cannot record; audit entries exist.
    let (_, staff) = app
        .post("/staff", &admin, json!({ "full_name": "Sana Malik" }))
        .await;
    let staff_id = id(&staff["id"]);
    app.put(
        "/allocation/teachers/cell",
        &admin,
        json!({ "section_id": s3a, "subject_id": null, "staff_id": staff_id }),
    )
    .await;
    let (s, _) = app
        .call(
            Method::PATCH,
            &format!("/sections/{s3a}"),
            Some(&admin),
            Some(json!({ "class_teacher_id": staff_id })),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, login) = app
        .post(
            &format!("/staff/{staff_id}/login"),
            &admin,
            json!({ "username": "sana", "password": "teacher-password-1", "role": "teacher" }),
        )
        .await;
    assert!(s.is_success(), "{login}");
    let teacher = app.login("sana", "teacher-password-1").await;
    let (s, _) = app
        .get(&format!("/sections/{s3a}/consents"), &teacher)
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app
        .get(&format!("/sections/{s9a}/consents"), &teacher)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = app
        .post(
            &format!("/students/{ali}/consents"),
            &teacher,
            json!({ "consent_type_id": photos, "status": "refused" }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app
        .post(
            "/consent-types",
            &teacher,
            json!({ "name": "X", "given_by": "guardian", "required": "all", "renewal": "none" }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (_, audit) = app.get("/audit?limit=500", &admin).await;
    let count = |entity: &str| {
        audit
            .as_array()
            .map(|a| a.iter().filter(|e| e["entity"] == entity).count())
            .unwrap_or(0)
    };
    assert_eq!(count("consent_type"), 3);
    assert_eq!(count("consent"), 7);
    // Removing a type hides it everywhere.
    let (s, _) = app
        .call(
            Method::DELETE,
            &format!("/consent-types/{lab}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, sec) = app.get(&format!("/sections/{s9a}/consents"), &admin).await;
    assert_eq!(sec["types"].as_array().unwrap().len(), 2);
    app.finish().await;
}
