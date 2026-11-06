//! M2 stage 3: the custom report builder (catalogue, saved definitions, runs, sharing,
//! teacher scope). Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

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
async fn custom_reports_are_built_from_the_catalogue_saved_shared_and_scoped() {
    let app = app!();
    let (s, b) = app.call(Method::POST, "/setup", None, Some(json!({ "school_name": "Gulshan High", "school_code": "KHI-MC", "academic_year_start_month": 4, "admin_full_name": "Ayesha", "admin_username": "ayesha", "admin_password": "admin-password-1", "backup_password": "backup-password-1" }))).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let school_id = id(&b["school_id"]);
    let admin = app.login("ayesha", "admin-password-1").await;
    let (_, year) = app
        .post(
            "/academic-years",
            &admin,
            json!({ "start_year": 2026, "start_month": 4, "activate": true }),
        )
        .await;
    let (_, c1) = app
        .post("/classes", &admin, json!({ "name": "1", "level": 1 }))
        .await;
    let (_, c2) = app
        .post("/classes", &admin, json!({ "name": "2", "level": 2 }))
        .await;
    let (_, s1) = app
        .post(
            "/sections",
            &admin,
            json!({ "class_id": c1["id"], "academic_year_id": year["id"], "name": "A" }),
        )
        .await;
    let (_, s2) = app
        .post(
            "/sections",
            &admin,
            json!({ "class_id": c2["id"], "academic_year_id": year["id"], "name": "A" }),
        )
        .await;
    let (s1, s2) = (id(&s1["id"]), id(&s2["id"]));
    let (_, eng) = app
        .post(
            "/subjects",
            &admin,
            json!({ "name": "English", "code": "ENG" }),
        )
        .await;
    let student = |name: &str, father: &str, gender: &str, dob: &str, section: &str, roll: i32| {
        json!({ "full_name": name, "father_name": father, "date_of_birth": dob, "gender": gender, "admission_date": "2026-04-01", "section_id": section, "roll_no": roll,
                "guardians": [{ "full_name": father, "relationship": "father", "phone": format!("0300-{}", roll + 1000000), "is_primary": true }] })
    };
    app.post(
        "/students",
        &admin,
        student("Ali Khan", "Ahmed Khan", "male", "2015-03-04", &s1, 1),
    )
    .await;
    app.post(
        "/students",
        &admin,
        student("Sara Khan", "Imran Khan", "female", "2016-11-20", &s1, 2),
    )
    .await;
    app.post(
        "/students",
        &admin,
        student("Zain Ahmed", "Rashid Ahmed", "male", "2014-07-07", &s2, 1),
    )
    .await;

    // Catalogue: sources, typed fields, operators, in Urdu too.
    let (s, cat) = app.get("/reports/custom/sources?lang=ur", &admin).await;
    assert_eq!(s, StatusCode::OK, "{cat}");
    let sources: Vec<&str> = cat["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["key"].as_str().unwrap())
        .collect();
    assert!(
        sources.contains(&"students")
            && sources.contains(&"fee_balances")
            && sources.contains(&"marks"),
        "{sources:?}"
    );
    let students_src = &cat["sources"][0];
    assert_eq!(students_src["label"], "طلبہ");
    let gender = students_src["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["key"] == "gender")
        .unwrap();
    assert_eq!(gender["type"], "choice");
    assert_eq!(
        gender["choices"][0],
        json!({ "value": "male", "label": "مرد" })
    );
    assert!(gender["operators"]
        .as_array()
        .unwrap()
        .contains(&json!("in")));
    assert!(cat["operators"]
        .as_array()
        .unwrap()
        .iter()
        .any(|o| o["value"] == "contains"));

    // Preview without saving: boys of class 1 grouped by section, newest first.
    let definition = json!({
        "source": "students",
        "columns": ["student_number", "full_name", "gender", "date_of_birth", "roll_no", "guardian_phone"],
        "filters": [
            { "field": "gender", "op": "eq", "value": "male" },
            { "field": "class", "op": "eq", "value": "1" }
        ],
        "sort": [{ "field": "date_of_birth", "desc": true }],
        "group_by": "section"
    });
    let (s, r) = app
        .post(
            "/reports/custom/run",
            &admin,
            json!({ "definition": definition, "name": "Boys of class 1" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["row_count"], 1, "{r}");
    assert_eq!(r["title"], "Boys of class 1");
    assert_eq!(r["groups"][0]["heading"], "Section: A");
    assert_eq!(
        r["groups"][0]["rows"][0],
        json!([
            "KHI-MC-2026-00001",
            "Ali Khan",
            "Male",
            "04-03-2015",
            "1",
            "03001000001"
        ])
    );
    assert_eq!(r["groups"][0]["subtotal"][0], "Subtotal (1)");
    assert_eq!(r["groups"][0]["subtotal"][4], "1");
    assert_eq!(r["total"][0], "Total (1)");
    assert_eq!(r["columns"][4]["align"], "end");

    // Refused: unknown field / source / operator, bad value, malicious text harmless.
    let bad = |d: Value| json!({ "definition": d });
    let (s, e) = app
        .post(
            "/reports/custom/run",
            &admin,
            bad(json!({ "source": "students", "columns": ["password_hash"] })),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("validation")),
        "{e}"
    );
    let (s, _) = app
        .post(
            "/reports/custom/run",
            &admin,
            bad(json!({ "source": "users", "columns": ["username"] })),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = app.post("/reports/custom/run", &admin, bad(json!({ "source": "students", "columns": ["full_name"], "filters": [{ "field": "roll_no", "op": "contains", "value": "1" }] }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, r) = app.post("/reports/custom/run", &admin, bad(json!({ "source": "students", "columns": ["full_name"], "filters": [{ "field": "full_name", "op": "contains", "value": "' OR 1=1; --" }] }))).await;
    assert_eq!((s, r["row_count"].as_i64()), (StatusCode::OK, Some(0)));

    // Save: the accountant may save a private report but not a shared one; admin shares.
    let accountant = app
        .user_with_role(&school_id, "acc", RoleCode::Accountant)
        .await;
    let (s, e) = app
        .post(
            "/reports/custom",
            &accountant,
            json!({ "name": "Shared by acc", "definition": definition, "is_shared": true }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{e}");
    let (s, mine) = app
        .post(
            "/reports/custom",
            &accountant,
            json!({ "name": "Acc private", "definition": definition }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{mine}");
    assert_eq!(mine["can_edit"], true);
    let (s, shared) = app.post("/reports/custom", &admin, json!({ "name": "Boys of class 1", "description": "For the sports day list", "definition": definition, "is_shared": true })).await;
    assert_eq!(s, StatusCode::CREATED, "{shared}");
    let (s, e) = app
        .post(
            "/reports/custom",
            &admin,
            json!({ "name": "boys OF class 1", "definition": definition }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("conflict")),
        "names unique: {e}"
    );
    let (s, e) = app
        .post(
            "/reports/custom",
            &admin,
            json!({ "name": "", "definition": definition }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");

    // Lists: the admin sees the shared one, not the accountant's private one; the accountant
    // sees both of theirs.
    let (_, admin_list) = app.get("/reports/custom", &admin).await;
    let names: Vec<&str> = admin_list
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Boys of class 1"]);
    let (_, acc_list) = app.get("/reports/custom", &accountant).await;
    let names: Vec<&str> = acc_list
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Acc private", "Boys of class 1"]);
    let shared_view = acc_list
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "Boys of class 1")
        .unwrap();
    assert_eq!(shared_view["can_edit"], false);
    assert_eq!(shared_view["owner_name"], "Ayesha");

    // Run a saved report as JSON, Excel and PDF; it is audit-logged.
    let sid = id(&shared["id"]);
    let (s, r) = app
        .get(&format!("/reports/custom/{sid}"), &accountant)
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["subtitle"], "For the sports day list");
    assert_eq!(r["row_count"], 1);
    for (format, prefix) in [("xlsx", &b"PK"[..]), ("pdf", &b"%PDF"[..])] {
        let (s, _, bytes) = app
            .raw(
                Method::GET,
                &format!("/reports/custom/{sid}?format={format}&lang=ur"),
                Some(&admin),
                None,
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{format}");
        assert!(bytes.starts_with(prefix), "{format}");
    }
    let (_, log) = app.get("/audit?limit=50", &admin).await;
    assert!(
        log.as_array()
            .unwrap()
            .iter()
            .any(|e| e["action"] == "report"
                && e["entity"] == "report_definition"
                && e["new_value"]["rows"] == 1),
        "{log}"
    );

    // Edit: the accountant cannot edit the admin's report; the admin can; private report
    // of another user is invisible (404).
    let (s, _) = app
        .put(
            &format!("/reports/custom/{sid}"),
            &accountant,
            json!({ "name": "Renamed", "definition": definition, "is_shared": true }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, upd) = app.put(&format!("/reports/custom/{sid}"), &admin, json!({ "name": "Boys of class 1 (sports)", "definition": { "source": "students", "columns": ["full_name"], "filters": [{ "field": "gender", "op": "eq", "value": "male" }] }, "is_shared": true })).await;
    assert_eq!(s, StatusCode::OK, "{upd}");
    assert_eq!(upd["definition"]["columns"], json!(["full_name"]));
    let (s, r) = app.get(&format!("/reports/custom/{sid}"), &admin).await;
    assert_eq!(
        (s, r["row_count"].as_i64()),
        (StatusCode::OK, Some(2)),
        "both boys now: {r}"
    );
    let (s, _) = app
        .get(&format!("/reports/custom/{}", id(&mine["id"])), &admin)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = app
        .call(
            Method::DELETE,
            &format!("/reports/custom/{}", id(&mine["id"])),
            Some(&accountant),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = app
        .get(&format!("/reports/custom/{}", id(&mine["id"])), &accountant)
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Other sources run too: fee balances and marks (empty but well-formed), attendance summary.
    for src in [
        "guardians",
        "fee_vouchers",
        "fee_receipts",
        "fee_balances",
        "attendance_entries",
        "attendance_summary",
        "marks",
        "staff",
    ] {
        let (_, c) = app.get("/reports/custom/sources", &admin).await;
        let fields: Vec<String> = c["sources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["key"] == src)
            .unwrap()["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| id(&f["key"]))
            .collect();
        let (s, r) = app
            .post(
                "/reports/custom/run",
                &admin,
                json!({ "definition": { "source": src, "columns": fields } }),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{src}: {r}");
    }
    let (_, r) = app.post("/reports/custom/run", &admin, json!({ "definition": { "source": "fee_balances", "columns": ["full_name", "balance", "unpaid_vouchers"], "filters": [{ "field": "class", "op": "in", "value": "1,2" }] } })).await;
    assert_eq!(r["row_count"], 3, "{r}");
    assert_eq!(r["groups"][0]["rows"][0][1], "0.00");
    assert_eq!(r["columns"][1]["label"], "Balance (PKR)");

    // A teacher granted reports.read by their school still sees only their own sections.
    let (_, st) = app
        .post("/staff", &admin, json!({ "full_name": "Sana Malik" }))
        .await;
    let staff_id = id(&st["id"]);
    app.put(
        &format!("/staff/{staff_id}/assignments"),
        &admin,
        json!({ "assignments": [{ "section_id": s1, "subject_id": eng["id"] }] }),
    )
    .await;
    app.post(
        &format!("/staff/{staff_id}/login"),
        &admin,
        json!({ "username": "sana", "password": "teacher-password-1", "role": "teacher" }),
    )
    .await;
    let teacher = app.login("sana", "teacher-password-1").await;
    let (s, _) = app.get("/reports/custom", &teacher).await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "teachers hold no reports.read by default"
    );
    {
        let sid_u: uuid::Uuid = school_id.parse().unwrap();
        let mut c = app.state.pool.acquire().await.unwrap();
        let role = roles::find_by_code(&mut c, sid_u, "teacher").await.unwrap();
        roles::grant(
            &mut c,
            &app.state.hlc,
            Actor::system_for(sid_u),
            &role,
            "reports.read".parse().unwrap(),
        )
        .await
        .unwrap();
    }
    let teacher = app.login("sana", "teacher-password-1").await;
    let (s, r) = app
        .post(
            "/reports/custom/run",
            &teacher,
            json!({ "definition": { "source": "students", "columns": ["full_name", "section"] } }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["row_count"], 2, "only section 1-A: {r}");
    let (s, r) = app
        .post(
            "/reports/custom/run",
            &teacher,
            json!({ "definition": { "source": "staff", "columns": ["full_name"] } }),
        )
        .await;
    assert_eq!(
        (s, r["row_count"].as_i64()),
        (StatusCode::OK, Some(0)),
        "staff is never scoped to a teacher: {r}"
    );
    let _ = s2;
    app.finish().await;
}
