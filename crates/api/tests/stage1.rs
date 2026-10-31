//! M1 stage 1 end-to-end API tests. Need PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use base64::Engine;
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
    ) -> (StatusCode, Vec<u8>, String) {
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
        let ct = res
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
        (status, bytes, ct)
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let (status, bytes, _) = self.raw(method, path, token, body).await;
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        (status, value)
    }

    async fn get(&self, path: &str, token: &str) -> (StatusCode, Value) {
        self.call(Method::GET, path, Some(token), None).await
    }
    async fn post(&self, path: &str, token: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::POST, path, Some(token), Some(body)).await
    }
    async fn patch(&self, path: &str, token: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::PATCH, path, Some(token), Some(body))
            .await
    }
    async fn put(&self, path: &str, token: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::PUT, path, Some(token), Some(body)).await
    }

    /// Setup + admin login. Returns (admin token, school id).
    async fn setup(&self) -> (String, String) {
        let (status, body) = self
            .call(
                Method::POST,
                "/setup",
                None,
                Some(json!({
                    "school_name": "Gulshan High", "school_code": "KHI-MC", "city": "Karachi",
                    "academic_year_start_month": 4, "currency": "PKR",
                    "admin_full_name": "Ayesha Khan", "admin_username": "ayesha",
                    "admin_password": "admin-password-1", "backup_password": "backup-password-1"
                })),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        (
            self.login("ayesha", "admin-password-1").await,
            body["school_id"].as_str().unwrap().into(),
        )
    }

    async fn login(&self, username: &str, password: &str) -> String {
        let (status, body) = self
            .call(
                Method::POST,
                "/auth/login",
                None,
                Some(json!({ "username": username, "password": password })),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["token"].as_str().unwrap().to_string()
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
        u.id.to_string()
    }

    /// Academic year 2026-27 (active), classes 1 and 2 with sections A, subjects. Returns ids.
    async fn academics(&self, admin: &str) -> Value {
        let (s, year) = self
            .post(
                "/academic-years",
                admin,
                json!({ "start_year": 2026, "start_month": 4, "activate": true }),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{year}");
        let (_, c1) = self
            .post("/classes", admin, json!({ "name": "1", "level": 1 }))
            .await;
        let (_, c2) = self
            .post("/classes", admin, json!({ "name": "2", "level": 2 }))
            .await;
        let (_, eng) = self
            .post(
                "/subjects",
                admin,
                json!({ "name": "English", "code": "ENG" }),
            )
            .await;
        let (_, math) = self
            .post(
                "/subjects",
                admin,
                json!({ "name": "Mathematics", "code": "MATH" }),
            )
            .await;
        let (s, s1a) = self
            .post(
                "/sections",
                admin,
                json!({ "class_id": c1["id"], "academic_year_id": year["id"], "name": "A" }),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{s1a}");
        let (_, s2a) = self
            .post(
                "/sections",
                admin,
                json!({ "class_id": c2["id"], "academic_year_id": year["id"], "name": "A" }),
            )
            .await;
        json!({ "year": year["id"], "c1": c1["id"], "c2": c2["id"], "eng": eng["id"], "math": math["id"], "s1a": s1a["id"], "s2a": s2a["id"] })
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

fn student_body(name: &str, father: &str, dob: &str) -> Value {
    json!({ "full_name": name, "father_name": father, "date_of_birth": dob, "gender": "male", "admission_date": "2026-04-10" })
}

#[tokio::test]
async fn academic_structure_crud_and_validation() {
    let app = app!();
    let (admin, _) = app.setup().await;
    let (s, b) = app
        .post(
            "/academic-years",
            &admin,
            json!({ "start_year": 2026, "start_month": 13 }),
        )
        .await;
    assert_eq!(
        (s, b["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("validation"))
    );
    let (s, y1) = app
        .post(
            "/academic-years",
            &admin,
            json!({ "start_year": 2025, "start_month": 4, "activate": true }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{y1}");
    assert_eq!(y1["name"], "2025-26");
    assert_eq!(y1["is_active"], true);
    let (_, y2) = app
        .post(
            "/academic-years",
            &admin,
            json!({ "start_year": 2026, "start_month": 8, "name": "Session 2026" }),
        )
        .await;
    assert_eq!(
        (y2["name"].as_str(), y2["start_date"].as_str()),
        (Some("Session 2026"), Some("2026-08-01"))
    );
    let (s, act) = app
        .post(
            &format!("/academic-years/{}/activate", y2["id"].as_str().unwrap()),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(
        (s, act["is_active"].as_bool()),
        (StatusCode::OK, Some(true))
    );
    let (_, list) = app.get("/academic-years", &admin).await;
    let active: Vec<_> = list
        .as_array()
        .unwrap()
        .iter()
        .filter(|y| y["is_active"] == true)
        .collect();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0]["id"], y2["id"]);

    let (s, _) = app.post("/classes", &admin, json!({ "name": "  " })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, kg) = app
        .post("/classes", &admin, json!({ "name": "KG", "level": 0 }))
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app.post("/classes", &admin, json!({ "name": "kg" })).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, renamed) = app
        .patch(
            &format!("/classes/{}", kg["id"].as_str().unwrap()),
            &admin,
            json!({ "name": "Nursery" }),
        )
        .await;
    assert_eq!(
        (s, renamed["name"].as_str()),
        (StatusCode::OK, Some("Nursery"))
    );

    let (_, eng) = app
        .post(
            "/subjects",
            &admin,
            json!({ "name": "English", "code": "eng" }),
        )
        .await;
    assert_eq!(eng["code"], "ENG");
    let (s, _) = app
        .post("/subjects", &admin, json!({ "name": "X", "code": "ENG" }))
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, set) = app
        .put(
            &format!("/classes/{}/subjects", kg["id"].as_str().unwrap()),
            &admin,
            json!({ "subject_ids": [eng["id"]] }),
        )
        .await;
    assert_eq!(
        (s, set["subject_ids"].as_array().unwrap().len()),
        (StatusCode::OK, 1)
    );
    let (s, _) = app
        .put(
            &format!("/classes/{}/subjects", kg["id"].as_str().unwrap()),
            &admin,
            json!({ "subject_ids": [uuid::Uuid::now_v7()] }),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let (s, sec) = app.post("/sections", &admin, json!({ "class_id": kg["id"], "academic_year_id": y2["id"], "name": "a", "capacity": 30 })).await;
    assert_eq!((s, sec["name"].as_str()), (StatusCode::OK, Some("A")));
    let (s, _) = app
        .post(
            "/sections",
            &admin,
            json!({ "class_id": kg["id"], "academic_year_id": y2["id"], "name": "A" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, list) = app
        .get(
            &format!("/sections?academic_year_id={}", y2["id"].as_str().unwrap()),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(list[0]["class_name"], "Nursery");
    assert_eq!(list[0]["student_count"], 0);
    let (s, upd) = app
        .patch(
            &format!("/sections/{}", sec["id"].as_str().unwrap()),
            &admin,
            json!({ "capacity": null }),
        )
        .await;
    assert_eq!((s, upd["capacity"].is_null()), (StatusCode::OK, true));
    app.finish().await;
}

#[tokio::test]
async fn staff_profiles_assignments_and_class_teacher() {
    let app = app!();
    let (admin, school_id) = app.setup().await;
    let ids = app.academics(&admin).await;
    let teacher_user = app
        .user_with_role(&school_id, "sana", RoleCode::Teacher)
        .await;

    let (s, b) = app.post("/staff", &admin, json!({ "full_name": "Sana Malik", "phone": "+92 300 1234567", "cnic": "4210112345671", "designation": "Teacher", "user_id": teacher_user, "gender": "female" })).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(
        (
            b["phone"].as_str(),
            b["cnic"].as_str(),
            b["staff_number"].as_str()
        ),
        (Some("03001234567"), Some("42101-1234567-1"), Some("S-0001"))
    );
    assert_eq!(b["roles"], json!(["teacher"]));
    assert_eq!(b["username"], "sana");
    let staff_id = b["id"].as_str().unwrap().to_string();
    let (s, e) = app
        .post(
            "/staff",
            &admin,
            json!({ "full_name": "Bad Phone", "phone": "123" }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("validation"))
    );
    let (s, _) = app
        .post(
            "/staff",
            &admin,
            json!({ "full_name": "Second profile", "user_id": teacher_user }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "one profile per login");

    let (s, a) = app
        .put(
            &format!("/staff/{staff_id}/assignments"),
            &admin,
            json!({ "assignments": [{ "section_id": ids["s1a"], "subject_id": ids["math"] }] }),
        )
        .await;
    assert_eq!(
        (s, a["assignments"].as_array().unwrap().len()),
        (StatusCode::OK, 1)
    );
    let (s, _) = app
        .put(
            &format!("/staff/{staff_id}/assignments"),
            &admin,
            json!({ "assignments": [{ "section_id": uuid::Uuid::now_v7(), "subject_id": null }] }),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, sec) = app
        .patch(
            &format!("/sections/{}", ids["s2a"].as_str().unwrap()),
            &admin,
            json!({ "class_teacher_id": staff_id }),
        )
        .await;
    assert_eq!(
        (s, sec["class_teacher_id"].as_str()),
        (StatusCode::OK, Some(staff_id.as_str()))
    );
    let (_, list) = app.get("/sections", &admin).await;
    let s2a = list
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == ids["s2a"])
        .unwrap();
    assert_eq!(s2a["class_teacher_name"], "Sana Malik");

    // Teachers can read staff, not manage.
    let teacher = app.login("sana", "user-password-1").await;
    let (s, _) = app.get("/staff", &teacher).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app
        .patch(
            &format!("/staff/{staff_id}"),
            &teacher,
            json!({ "designation": "Head" }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, upd) = app
        .patch(
            &format!("/staff/{staff_id}"),
            &admin,
            json!({ "designation": "Head", "status": "left", "phone": null }),
        )
        .await;
    assert_eq!(
        (
            s,
            upd["designation"].as_str(),
            upd["status"].as_str(),
            upd["phone"].is_null()
        ),
        (StatusCode::OK, Some("Head"), Some("left"), true)
    );
    app.finish().await;
}

#[tokio::test]
async fn students_create_duplicates_directory_and_withdraw() {
    let app = app!();
    let (admin, _) = app.setup().await;
    let ids = app.academics(&admin).await;

    let mut ali = student_body("Ali Khan", "Ahmed Khan", "2015-03-04");
    ali["cnic"] = json!("42101 1234567 1");
    ali["section_id"] = ids["s2a"].clone();
    ali["roll_no"] = json!(7);
    ali["custom"] = json!({ "house": "Red" });
    ali["guardians"] = json!([
        { "full_name": "Ahmed Khan", "relationship": "father", "phone": "0300-1111111", "cnic": "42101-7654321-1", "occupation": "Trader", "is_primary": true },
        { "full_name": "Nadia Khan", "relationship": "mother", "phone": "0300-2222222" }
    ]);
    let (s, a) = app.post("/students", &admin, ali.clone()).await;
    assert_eq!(s, StatusCode::CREATED, "{a}");
    assert_eq!(a["student_number"], "KHI-MC-2026-00001");
    assert_eq!(a["cnic"], "42101-1234567-1");
    assert_eq!(a["custom"]["house"], "Red");
    assert_eq!(a["current"]["class_name"], "2");
    assert_eq!(a["current"]["section_name"], "A");
    assert_eq!(a["current"]["roll_no"], 7);
    assert_eq!(a["guardians"].as_array().unwrap().len(), 2);
    assert_eq!(a["guardians"][0]["full_name"], "Ahmed Khan");
    assert_eq!(a["guardians"][0]["is_primary"], true);
    assert_eq!(a["guardians"][0]["phone"], "03001111111");
    let ali_id = a["id"].as_str().unwrap().to_string();

    // Duplicate by CNIC → 409 with matches; force creates anyway.
    let mut dup = student_body("Someone Else", "Other", "2010-01-01");
    dup["cnic"] = json!("42101-1234567-1");
    let (s, e) = app.post("/students", &admin, dup.clone()).await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("possible_duplicate"))
    );
    assert_eq!(e["details"][0]["reason"], "same_cnic");
    assert_eq!(e["details"][0]["student"]["id"], ali_id);
    // Duplicate by identity.
    let (s, e) = app
        .post(
            "/students",
            &admin,
            student_body(" ali  khan ", "AHMED khan", "2015-03-04"),
        )
        .await;
    assert_eq!(
        (s, e["details"][0]["reason"].as_str()),
        (StatusCode::CONFLICT, Some("same_name_dob_father"))
    );
    let (s, m) = app.post("/students/check-duplicates", &admin, json!({ "full_name": "Ali Khan", "father_name": "Ahmed Khan", "date_of_birth": "2015-03-04" })).await;
    assert_eq!((s, m.as_array().unwrap().len()), (StatusCode::OK, 1));
    let (s, m) = app.post("/students/check-duplicates", &admin, json!({ "full_name": "Ali Khan", "father_name": "Ahmed Khan", "date_of_birth": "2015-03-04", "exclude_id": ali_id })).await;
    assert_eq!((s, m.as_array().unwrap().len()), (StatusCode::OK, 0));
    dup["force"] = json!(true);
    let (s, forced) = app.post("/students", &admin, dup).await;
    assert_eq!(s, StatusCode::CREATED, "{forced}");
    assert_eq!(forced["student_number"], "KHI-MC-2026-00002");

    // Sibling: same father by CNIC → shares the guardian record.
    let mut sara = student_body("Sara Khan", "Ahmed Khan", "2017-05-06");
    sara["gender"] = json!("female");
    sara["section_id"] = ids["s1a"].clone();
    sara["guardians"] = json!([{ "full_name": "Ahmed Khan", "relationship": "father", "cnic": "4210176543211", "is_primary": true }]);
    let (s, sr) = app.post("/students", &admin, sara).await;
    assert_eq!(s, StatusCode::CREATED, "{sr}");
    assert_eq!(
        sr["guardians"][0]["phone"], "03001111111",
        "existing guardian reused"
    );
    assert_eq!(sr["siblings"][0]["full_name"], "Ali Khan");
    let (_, ali_detail) = app.get(&format!("/students/{ali_id}"), &admin).await;
    assert_eq!(ali_detail["siblings"].as_array().unwrap().len(), 1);
    assert_eq!(ali_detail["siblings"][0]["class_name"], "1");

    // Validation.
    let (s, _) = app.post("/students", &admin, json!({ "full_name": "", "father_name": "x", "date_of_birth": "2015-01-01", "gender": "male" })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = app.post("/students", &admin, json!({ "full_name": "A", "father_name": "x", "date_of_birth": "2015-01-01", "gender": "robot" })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let mut bad = student_body("A", "B", "2015-01-01");
    bad["cnic"] = json!("123");
    assert_eq!(
        app.post("/students", &admin, bad).await.0,
        StatusCode::BAD_REQUEST
    );

    // Directory: search, filter, sort, paginate.
    let (s, page) = app.get("/students?sort=name&page_size=2", &admin).await;
    assert_eq!(s, StatusCode::OK, "{page}");
    assert_eq!(
        (
            page["total"].clone(),
            page["items"].as_array().unwrap().len(),
            page["page"].clone()
        ),
        (json!(3), 2, json!(1))
    );
    assert_eq!(page["items"][0]["full_name"], "Ali Khan");
    assert_eq!(page["items"][0]["class_name"], "2");
    let (_, p2) = app
        .get("/students?sort=name&page_size=2&page=2", &admin)
        .await;
    assert_eq!(p2["items"][0]["full_name"], "Someone Else");
    let (_, q) = app.get("/students?q=sara", &admin).await;
    assert_eq!(q["total"], 1);
    let (_, q) = app.get("/students?q=00002", &admin).await;
    assert_eq!(q["items"][0]["full_name"], "Someone Else");
    let (_, c) = app
        .get(
            &format!("/students?class_id={}", ids["c1"].as_str().unwrap()),
            &admin,
        )
        .await;
    assert_eq!(c["items"][0]["full_name"], "Sara Khan");
    let (_, d) = app
        .get("/students?sort=admission_date&order=desc", &admin)
        .await;
    assert_eq!(d["total"], 3);
    let (s, _) = app.get("/students?sort=bogus", &admin).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Edit, withdraw (needs a reason), re-admit, move section.
    let (s, u) = app
        .patch(
            &format!("/students/{ali_id}"),
            &admin,
            json!({ "address": " Gulshan ", "board_roll_no": "B-9", "cnic": null }),
        )
        .await;
    assert_eq!(
        (
            s,
            u["address"].as_str(),
            u["board_roll_no"].as_str(),
            u["cnic"].is_null()
        ),
        (StatusCode::OK, Some("Gulshan"), Some("B-9"), true)
    );
    let (s, _) = app
        .post(
            &format!("/students/{ali_id}/withdraw"),
            &admin,
            json!({ "status": "left" }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, w) = app
        .post(
            &format!("/students/{ali_id}/withdraw"),
            &admin,
            json!({ "status": "left", "left_on": "2026-06-30", "reason": "Moved" }),
        )
        .await;
    assert_eq!(
        (s, w["status"].as_str(), w["left_reason"].as_str()),
        (StatusCode::OK, Some("left"), Some("Moved"))
    );
    let (_, active) = app.get("/students?status=active", &admin).await;
    assert_eq!(active["total"], 2);
    let (s, w) = app
        .post(
            &format!("/students/{ali_id}/withdraw"),
            &admin,
            json!({ "status": "active" }),
        )
        .await;
    assert_eq!((s, w["status"].as_str()), (StatusCode::OK, Some("active")));
    let (s, mv) = app
        .post(
            &format!("/students/{ali_id}/enrol"),
            &admin,
            json!({ "section_id": ids["s1a"], "roll_no": 12 }),
        )
        .await;
    assert_eq!(
        (
            s,
            mv["current"]["class_name"].as_str(),
            mv["enrollments"].as_array().unwrap().len()
        ),
        (StatusCode::OK, Some("1"), 1)
    );

    // Guardians: link existing, unlink, search, edit.
    let (_, gs) = app.get("/guardians?q=0300%20222", &admin).await;
    assert_eq!(gs.as_array().unwrap().len(), 1);
    assert_eq!(gs[0]["full_name"], "Nadia Khan");
    assert_eq!(gs[0]["students"][0]["full_name"], "Ali Khan");
    let nadia = gs[0]["id"].as_str().unwrap().to_string();
    let sara_id = sr["id"].as_str().unwrap();
    let (s, links) = app
        .post(
            &format!("/students/{sara_id}/guardians"),
            &admin,
            json!({ "guardian_id": nadia, "relationship": "mother" }),
        )
        .await;
    assert_eq!((s, links.as_array().unwrap().len()), (StatusCode::OK, 2));
    let (s, links) = app
        .call(
            Method::DELETE,
            &format!("/students/{sara_id}/guardians/{nadia}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!((s, links.as_array().unwrap().len()), (StatusCode::OK, 1));
    let (s, g) = app
        .patch(
            &format!("/guardians/{nadia}"),
            &admin,
            json!({ "occupation": "Doctor", "phone": "0300-3333333" }),
        )
        .await;
    assert_eq!(
        (s, g["occupation"].as_str(), g["phone"].as_str()),
        (StatusCode::OK, Some("Doctor"), Some("03003333333"))
    );
    let (s, _) = app.get("/guardians?q=x", &admin).await;
    assert_eq!(s, StatusCode::OK);

    // Student number prefix setting.
    let (s, st) = app.get("/settings/school", &admin).await;
    assert_eq!(
        (s, st["student_number_prefix"].as_str()),
        (StatusCode::OK, Some("KHI-MC"))
    );
    let (s, st) = app
        .put(
            "/settings/school",
            &admin,
            json!({ "student_number_prefix": "ghs 1" }),
        )
        .await;
    assert_eq!(
        (s, st["student_number_prefix"].as_str()),
        (StatusCode::OK, Some("GHS-1"))
    );
    let (s, n) = app
        .post(
            "/students",
            &admin,
            student_body("New Kid", "Dad", "2018-01-01"),
        )
        .await;
    assert_eq!(
        (s, n["student_number"].as_str()),
        (StatusCode::CREATED, Some("GHS-1-2026-00004"))
    );
    app.finish().await;
}

#[tokio::test]
async fn photo_round_trip_and_limits() {
    let app = app!();
    let (admin, _) = app.setup().await;
    let (_, a) = app
        .post(
            "/students",
            &admin,
            student_body("Photo Kid", "Dad", "2015-01-01"),
        )
        .await;
    let id = a["id"].as_str().unwrap();
    let (s, _, _) = app
        .raw(
            Method::GET,
            &format!("/students/{id}/photo"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let png = base64::engine::general_purpose::STANDARD.encode([137u8, 80, 78, 71, 1, 2, 3]);
    let (s, _) = app
        .put(
            &format!("/students/{id}/photo"),
            &admin,
            json!({ "mime": "image/png", "data": png }),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, bytes, ct) = app
        .raw(
            Method::GET,
            &format!("/students/{id}/photo"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(
        (s, ct.as_str(), bytes.len()),
        (StatusCode::OK, "image/png", 7)
    );
    let (_, d) = app.get(&format!("/students/{id}"), &admin).await;
    assert_eq!(d["has_photo"], true);
    let (s, _) = app
        .put(
            &format!("/students/{id}/photo"),
            &admin,
            json!({ "mime": "image/gif", "data": png }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let big = base64::engine::general_purpose::STANDARD.encode(vec![0u8; 600 * 1024]);
    let (s, _) = app
        .put(
            &format!("/students/{id}/photo"),
            &admin,
            json!({ "mime": "image/jpeg", "data": big }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = app
        .call(
            Method::DELETE,
            &format!("/students/{id}/photo"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, d) = app.get(&format!("/students/{id}"), &admin).await;
    assert_eq!(d["has_photo"], false);
    app.finish().await;
}

#[tokio::test]
async fn teachers_read_only_their_own_sections() {
    let app = app!();
    let (admin, school_id) = app.setup().await;
    let ids = app.academics(&admin).await;
    let teacher_user = app
        .user_with_role(&school_id, "sana", RoleCode::Teacher)
        .await;
    let unlinked_user = app
        .user_with_role(&school_id, "nolink", RoleCode::Teacher)
        .await;
    let _ = unlinked_user;
    let accountant = app
        .user_with_role(&school_id, "acc", RoleCode::Accountant)
        .await;
    let _ = accountant;
    let (_, st) = app
        .post(
            "/staff",
            &admin,
            json!({ "full_name": "Sana", "user_id": teacher_user }),
        )
        .await;
    let staff_id = st["id"].as_str().unwrap().to_string();

    let mut a = student_body("In Section 1A", "F", "2015-01-01");
    a["section_id"] = ids["s1a"].clone();
    let (_, a) = app.post("/students", &admin, a).await;
    let mut b = student_body("In Section 2A", "F", "2015-01-02");
    b["section_id"] = ids["s2a"].clone();
    let (_, b) = app.post("/students", &admin, b).await;
    let (_, c) = app
        .post(
            "/students",
            &admin,
            student_body("Unenrolled", "F", "2015-01-03"),
        )
        .await;
    let a_id = a["id"].as_str().unwrap();
    let b_id = b["id"].as_str().unwrap();
    let c_id = c["id"].as_str().unwrap();

    let teacher = app.login("sana", "user-password-1").await;
    // No assignments yet: sees nobody.
    let (s, page) = app.get("/students", &teacher).await;
    assert_eq!((s, page["total"].as_i64()), (StatusCode::OK, Some(0)));
    let (s, _) = app.get(&format!("/students/{a_id}"), &teacher).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Subject teacher in 1A → sees 1A only.
    app.put(
        &format!("/staff/{staff_id}/assignments"),
        &admin,
        json!({ "assignments": [{ "section_id": ids["s1a"], "subject_id": ids["eng"] }] }),
    )
    .await;
    let (_, page) = app.get("/students", &teacher).await;
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["full_name"], "In Section 1A");
    let (s, _) = app.get(&format!("/students/{a_id}"), &teacher).await;
    assert_eq!(s, StatusCode::OK);
    let (s, e) = app.get(&format!("/students/{b_id}"), &teacher).await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("forbidden"))
    );
    let (s, _) = app.get(&format!("/students/{c_id}"), &teacher).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // Filters cannot escape the scope.
    let (_, page) = app
        .get(
            &format!("/students?section_id={}", ids["s2a"].as_str().unwrap()),
            &teacher,
        )
        .await;
    assert_eq!(page["total"], 0);
    // Class teacher of 2A → both sections.
    app.patch(
        &format!("/sections/{}", ids["s2a"].as_str().unwrap()),
        &admin,
        json!({ "class_teacher_id": staff_id }),
    )
    .await;
    let (_, page) = app.get("/students", &teacher).await;
    assert_eq!(page["total"], 2);
    let (s, _) = app.get(&format!("/students/{b_id}"), &teacher).await;
    assert_eq!(s, StatusCode::OK);
    // Teachers never write students or academics.
    let (s, _) = app
        .post("/students", &teacher, student_body("X", "Y", "2015-01-01"))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app
        .patch(
            &format!("/students/{a_id}"),
            &teacher,
            json!({ "address": "x" }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app
        .post(
            &format!("/students/{a_id}/withdraw"),
            &teacher,
            json!({ "status": "left", "reason": "x" }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.post("/classes", &teacher, json!({ "name": "9" })).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.get("/classes", &teacher).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app.get("/settings/school", &teacher).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // A teacher login without a staff profile sees nobody; non-teacher readers see everyone.
    let nolink = app.login("nolink", "user-password-1").await;
    let (_, page) = app.get("/students", &nolink).await;
    assert_eq!(page["total"], 0);
    let acc = app.login("acc", "user-password-1").await;
    let (_, page) = app.get("/students", &acc).await;
    assert_eq!(page["total"], 3);
    let (s, _) = app.get(&format!("/students/{c_id}"), &acc).await;
    assert_eq!(s, StatusCode::OK);
    app.finish().await;
}
