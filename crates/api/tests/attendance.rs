//! M1 stage 3 attendance API tests. Need PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

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
    async fn delete(&self, p: &str, t: &str) -> (StatusCode, Value) {
        self.call(Method::DELETE, p, Some(t), None).await
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

/// A Monday in the active year, in the past (tests run after April 2026).
const MONDAY: &str = "2026-04-06";

#[tokio::test]
async fn teacher_marks_own_section_only_and_cannot_correct() {
    let app = app!();
    let (admin, school_id, ids) = app.seed().await;
    let ali = app.student(&admin, "Ali", &ids["s1"]).await;
    let sara = app.student(&admin, "Sara", &ids["s1"]).await;
    let zain = app.student(&admin, "Zain", &ids["s2"]).await;
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

    // No assignments yet: the section is out of scope.
    let (s, _) = app
        .get(
            &format!(
                "/attendance/sheet?section_id={}&date={MONDAY}",
                ids["s1"].as_str().unwrap()
            ),
            &teacher,
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app
        .get(
            &format!(
                "/attendance/sheet?section_id={}&date={MONDAY}",
                ids["s1"].as_str().unwrap()
            ),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "admins see every section");

    // Class teacher of 1A.
    let (s, _) = app
        .call(
            Method::PATCH,
            &format!("/sections/{}", ids["s1"].as_str().unwrap()),
            Some(&admin),
            Some(json!({ "class_teacher_id": staff_id })),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, v) = app
        .get(
            &format!(
                "/attendance/sheet?section_id={}&date={MONDAY}",
                ids["s1"].as_str().unwrap()
            ),
            &teacher,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["day_kind"]["kind"], "school");
    assert_eq!(v["sheet"], Value::Null);
    assert_eq!(v["students"].as_array().unwrap().len(), 2);
    assert_eq!(
        (v["can_edit"].clone(), v["is_correction"].clone()),
        (json!(true), json!(false))
    );
    assert_eq!(v["section"]["class_teacher_name"], "Sana");

    // Status lists only her section; the dashboard says it is unmarked.
    let (_, st) = app
        .get(&format!("/attendance/status?date={MONDAY}"), &teacher)
        .await;
    assert_eq!(st["sections"].as_array().unwrap().len(), 1);
    assert_eq!(st["totals"]["marked"], 0);
    assert_eq!(st["totals"]["percent"], Value::Null);
    let (_, st_admin) = app
        .get(&format!("/attendance/status?date={MONDAY}"), &admin)
        .await;
    assert_eq!(st_admin["sections"].as_array().unwrap().len(), 2);

    // Mark: only exceptions are sent.
    let (s, v) = app
        .put(
            "/attendance/sheet",
            &teacher,
            json!({ "section_id": ids["s1"], "date": MONDAY, "entries": [{ "student_id": sara, "status": "absent", "note": "sick" }] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(
        (v["sheet"]["present"].clone(), v["sheet"]["absent"].clone()),
        (json!(1), json!(1))
    );
    assert_eq!(v["marked_by_name"], "sana");
    assert_eq!(
        (v["can_edit"].clone(), v["is_correction"].clone()),
        (json!(false), json!(true))
    );
    let ali_row = v["students"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["student_id"] == ali)
        .unwrap();
    assert_eq!(ali_row["status"], "present");

    // Teacher cannot re-save; the other section and a student of it are refused.
    let (s, e) = app
        .put(
            "/attendance/sheet",
            &teacher,
            json!({ "section_id": ids["s1"], "date": MONDAY, "entries": [] }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("already_submitted"))
    );
    let (s, _) = app
        .put(
            "/attendance/sheet",
            &teacher,
            json!({ "section_id": ids["s2"], "date": MONDAY, "entries": [] }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, e) = app
        .put(
            "/attendance/sheet",
            &admin,
            json!({ "section_id": ids["s2"], "date": "2026-04-07", "entries": [{ "student_id": ali, "status": "absent" }] }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("conflict")),
        "student from another section"
    );
    let (s, _) = app
        .put(
            "/attendance/sheet",
            &admin,
            json!({ "section_id": ids["s2"], "date": "2099-01-05", "entries": [] }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "future date");
    let (s, _) = app
        .put(
            "/attendance/sheet",
            &admin,
            json!({ "section_id": ids["s2"], "date": MONDAY, "entries": [{ "student_id": zain, "status": "sick" }] }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "unknown status");

    // Admin corrects: Sara was present after all. History shows who changed what.
    let (s, v) = app
        .put(
            "/attendance/sheet",
            &admin,
            json!({ "section_id": ids["s1"], "date": MONDAY, "entries": [] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(
        (v["sheet"]["present"].clone(), v["sheet"]["absent"].clone()),
        (json!(2), json!(0))
    );
    assert_eq!(v["can_edit"], true, "admin may correct again");
    let (s, h) = app
        .get(
            &format!(
                "/attendance/sheet/history?section_id={}&date={MONDAY}",
                ids["s1"].as_str().unwrap()
            ),
            &teacher,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let h = h.as_array().unwrap();
    assert_eq!(h.len(), 3);
    assert_eq!(
        (h[0]["entity"].as_str(), h[0]["actor_name"].as_str()),
        (Some("attendance_sheet"), Some("sana"))
    );
    assert_eq!(
        (
            h[1]["entity"].as_str(),
            h[1]["actor_name"].as_str(),
            h[1]["student_name"].as_str()
        ),
        (Some("attendance_entry"), Some("Ayesha"), Some("Sara"))
    );
    assert_eq!(h[1]["old_value"]["status"], "absent");
    assert_eq!(h[1]["new_value"]["status"], "present");

    // Student history respects the teacher scope.
    let (s, v) = app
        .get(
            &format!("/attendance/students/{sara}?from=2026-04-01&to=2026-04-30"),
            &teacher,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["summary"]["present"], 1);
    assert_eq!(v["percent"], 100.0);
    let (s, _) = app
        .get(&format!("/attendance/students/{zain}"), &teacher)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, v) = app
        .get(&format!("/attendance/students/{zain}"), &admin)
        .await;
    assert_eq!(
        (s, v["summary"]["present"].as_i64()),
        (StatusCode::OK, Some(0))
    );

    // Accountants have no attendance permission at all.
    let (_, accountant) = app
        .user_with_role(&school_id, "acc", RoleCode::Accountant)
        .await;
    let (s, _) = app
        .get(&format!("/attendance/status?date={MONDAY}"), &accountant)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    app.finish().await;
}

#[tokio::test]
async fn calendar_blocks_marking_and_register_absentees_dashboard_report() {
    let app = app!();
    let (admin, school_id, ids) = app.seed().await;
    let _ali = app.student(&admin, "Ali", &ids["s1"]).await;
    let sara = app.student(&admin, "Sara", &ids["s1"]).await;
    let s1 = ids["s1"].as_str().unwrap();

    let (s, cal) = app.get("/attendance/calendar", &admin).await;
    assert_eq!(
        (s, cal["weekend_days"].clone()),
        (StatusCode::OK, json!([7]))
    );
    let (s, cal) = app
        .put(
            "/attendance/calendar/weekend",
            &admin,
            json!({ "weekend_days": [6, 7] }),
        )
        .await;
    assert_eq!(
        (s, cal["weekend_days"].clone()),
        (StatusCode::OK, json!([6, 7]))
    );
    let (s, _) = app
        .put(
            "/attendance/calendar/weekend",
            &admin,
            json!({ "weekend_days": [1, 2, 3, 4, 5, 6, 7] }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, h) = app
        .post(
            "/attendance/calendar/holidays",
            &admin,
            json!({ "name": "Founders Day", "starts_on": "2026-04-08" }),
        )
        .await;
    assert_eq!(
        (s, h["ends_on"].as_str()),
        (StatusCode::CREATED, Some("2026-04-08"))
    );
    let (s, _) = app
        .post(
            "/attendance/calendar/holidays",
            &admin,
            json!({ "name": "Bad", "starts_on": "2026-04-09", "ends_on": "2026-04-01" }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Saturday is now a weekend; the holiday refuses too.
    let (s, v) = app
        .get(
            &format!("/attendance/sheet?section_id={s1}&date=2026-04-04"),
            &admin,
        )
        .await;
    assert_eq!(
        (s, v["day_kind"]["kind"].as_str(), v["can_edit"].as_bool()),
        (StatusCode::OK, Some("weekend"), Some(false))
    );
    let (s, e) = app
        .put(
            "/attendance/sheet",
            &admin,
            json!({ "section_id": s1, "date": "2026-04-04", "entries": [] }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("conflict"))
    );
    let (s, v) = app
        .get(
            &format!("/attendance/sheet?section_id={s1}&date=2026-04-08"),
            &admin,
        )
        .await;
    assert_eq!(
        (
            s,
            v["day_kind"]["kind"].as_str(),
            v["day_kind"]["name"].as_str()
        ),
        (StatusCode::OK, Some("holiday"), Some("Founders Day"))
    );
    let (s, _) = app
        .put(
            "/attendance/sheet",
            &admin,
            json!({ "section_id": s1, "date": "2026-04-08", "entries": [] }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);

    // Mark Mon–Tue; Sara absent Monday, late Tuesday.
    for (date, status) in [(MONDAY, "absent"), ("2026-04-07", "late")] {
        let (s, v) = app
            .put("/attendance/sheet", &admin, json!({ "section_id": s1, "date": date, "entries": [{ "student_id": sara, "status": status }] }))
            .await;
        assert_eq!(s, StatusCode::OK, "{v}");
    }
    let (s, abs) = app
        .get(&format!("/attendance/absentees?date={MONDAY}"), &admin)
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(abs.as_array().unwrap().len(), 1);
    assert_eq!(
        (
            abs[0]["full_name"].as_str(),
            abs[0]["status"].as_str(),
            abs[0]["class_name"].as_str()
        ),
        (Some("Sara"), Some("absent"), Some("1"))
    );
    let (_, abs) = app
        .get(
            &format!(
                "/attendance/absentees?date={MONDAY}&class_id={}",
                ids["c2"].as_str().unwrap()
            ),
            &admin,
        )
        .await;
    assert!(abs.as_array().unwrap().is_empty());

    let (s, reg) = app
        .get(
            &format!("/attendance/register?section_id={s1}&month=2026-04-15"),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{reg}");
    let days = reg["days"].as_array().unwrap();
    assert_eq!(days.len(), 30);
    assert_eq!(days[7]["kind"]["kind"], "holiday");
    assert_eq!(days[4]["kind"]["kind"], "weekend", "5 April is a Sunday");
    assert_eq!(
        days[3]["kind"]["kind"], "weekend",
        "4 April is a Saturday (configured)"
    );
    assert_eq!(
        (
            days[5]["marked"].clone(),
            days[6]["marked"].clone(),
            days[8]["marked"].clone()
        ),
        (json!(true), json!(true), json!(false))
    );
    // April 2026 with Sat+Sun off and one holiday: 30 − 8 weekend days − 1 holiday = 21.
    assert_eq!(reg["school_days"], 21);
    let rows = reg["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let sara_row = rows.iter().find(|r| r["student_id"] == sara).unwrap();
    assert_eq!(sara_row["statuses"][5], "absent");
    assert_eq!(sara_row["statuses"][6], "late");
    assert_eq!(sara_row["statuses"][0], Value::Null);
    assert_eq!(
        (
            sara_row["summary"]["absent"].clone(),
            sara_row["summary"]["late"].clone()
        ),
        (json!(1), json!(1))
    );

    // Daily status and dashboard: 1 of 2 sections marked (the other has no students, so it is
    // not listed as unmarked).
    let (_, st) = app
        .get(&format!("/attendance/status?date={MONDAY}"), &admin)
        .await;
    assert_eq!(
        (
            st["totals"]["sections"].clone(),
            st["totals"]["marked"].clone(),
            st["totals"]["percent"].clone()
        ),
        (json!(2), json!(1), json!(50.0))
    );
    let (s, dash) = app.get("/dashboard", &admin).await;
    assert_eq!(s, StatusCode::OK, "{dash}");
    assert!(dash["attendance"].is_object());
    assert_eq!(dash["attendance"]["totals"]["sections"], 2);
    assert_eq!(
        dash["attendance"]["unmarked"].as_array().unwrap().len(),
        1,
        "today: 1A unmarked, 2A empty"
    );
    let (_, accountant) = app
        .user_with_role(&school_id, "acc", RoleCode::Accountant)
        .await;
    let (s, dash) = app.get("/dashboard", &accountant).await;
    assert_eq!(
        (s, dash["attendance"].clone()),
        (StatusCode::OK, Value::Null)
    );

    // Deleting the holiday reopens the day; a foreign id is not found.
    let (s, _) = app
        .delete(
            &format!(
                "/attendance/calendar/holidays/{}",
                h["id"].as_str().unwrap()
            ),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = app
        .delete(
            &format!("/attendance/calendar/holidays/{}", uuid::Uuid::nil()),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (_, v) = app
        .get(
            &format!("/attendance/sheet?section_id={s1}&date=2026-04-08"),
            &admin,
        )
        .await;
    assert_eq!(v["day_kind"]["kind"], "school");
    app.finish().await;
}

#[tokio::test]
async fn staff_attendance_is_for_admins_only() {
    let app = app!();
    let (admin, school_id, _ids) = app.seed().await;
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
    let sana = st["id"].as_str().unwrap().to_string();
    let (_, st) = app
        .post("/staff", &admin, json!({ "full_name": "Hira" }))
        .await;
    let hira = st["id"].as_str().unwrap().to_string();

    let (s, v) = app
        .get(&format!("/attendance/staff?date={MONDAY}"), &admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["rows"].as_array().unwrap().len(), 2);
    assert_eq!(v["can_edit"], true);
    let (s, v) = app
        .put("/attendance/staff", &admin, json!({ "date": MONDAY, "entries": [{ "staff_id": hira, "status": "leave", "note": "casual" }] }))
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let hira_row = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["staff_id"] == hira)
        .unwrap();
    assert_eq!(
        (hira_row["status"].as_str(), hira_row["note"].as_str()),
        (Some("leave"), Some("casual"))
    );
    let sana_row = v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["staff_id"] == sana)
        .unwrap();
    assert_eq!(sana_row["status"], "present");
    let (s, h) = app
        .get(
            &format!("/attendance/staff/{hira}?from=2026-04-01&to=2026-04-30"),
            &admin,
        )
        .await;
    assert_eq!(
        (
            s,
            h["summary"]["leave"].as_i64(),
            h["days"].as_array().unwrap().len()
        ),
        (StatusCode::OK, Some(1), 1)
    );

    // Teachers can see the list but not mark it; a weekend is refused.
    let (s, v) = app
        .get(&format!("/attendance/staff?date={MONDAY}"), &teacher)
        .await;
    assert_eq!((s, v["can_edit"].as_bool()), (StatusCode::OK, Some(false)));
    let (s, _) = app
        .put(
            "/attendance/staff",
            &teacher,
            json!({ "date": MONDAY, "entries": [] }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app
        .put(
            "/attendance/staff",
            &admin,
            json!({ "date": "2026-04-05", "entries": [] }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = app
        .put(
            "/attendance/staff",
            &admin,
            json!({ "date": MONDAY, "entries": [{ "staff_id": hira, "status": "late" }] }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    app.finish().await;
}
