//! M2 stage 7: allocation screens. The teacher grid sets one teacher per section and
//! subject; bulk moves and the balancing proposal move students between sections.
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

fn count_entity(audit: &Value, entity: &str) -> usize {
    audit
        .as_array()
        .map(|a| a.iter().filter(|e| e["entity"] == entity).count())
        .unwrap_or(0)
}

#[tokio::test]
async fn teacher_grid_and_section_moves() {
    let app = app!();
    let (s, b) = app.call(Method::POST, "/setup", None, Some(json!({ "school_name": "Gulshan High", "school_code": "KHI-MC", "academic_year_start_month": 4, "admin_full_name": "Ayesha", "admin_username": "ayesha", "admin_password": "admin-password-1", "backup_password": "backup-password-1" }))).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let admin = app.login("ayesha", "admin-password-1").await;
    let (_, year) = app
        .post(
            "/academic-years",
            &admin,
            json!({ "start_year": 2026, "start_month": 4, "activate": true }),
        )
        .await;
    let year_id = id(&year["id"]);
    let (_, c1) = app
        .post("/classes", &admin, json!({ "name": "1", "level": 1 }))
        .await;
    let c1 = id(&c1["id"]);
    let mut sections = Vec::new();
    for name in ["A", "B", "C"] {
        let (_, sec) = app
            .post(
                "/sections",
                &admin,
                json!({ "class_id": c1, "academic_year_id": year_id, "name": name }),
            )
            .await;
        sections.push(id(&sec["id"]));
    }
    let (sa, sb, sc) = (
        sections[0].clone(),
        sections[1].clone(),
        sections[2].clone(),
    );
    let (_, eng) = app
        .post(
            "/subjects",
            &admin,
            json!({ "name": "English", "code": "ENG" }),
        )
        .await;
    let (_, mth) = app
        .post(
            "/subjects",
            &admin,
            json!({ "name": "Mathematics", "code": "MTH" }),
        )
        .await;
    let (eng, mth) = (id(&eng["id"]), id(&mth["id"]));
    app.put(
        &format!("/classes/{c1}/subjects"),
        &admin,
        json!({ "subject_ids": [eng, mth] }),
    )
    .await;
    let (s, sana) = app
        .post("/staff", &admin, json!({ "full_name": "Sana Malik" }))
        .await;
    assert_eq!(s, StatusCode::OK, "{sana}");
    let (_, bilal) = app
        .post("/staff", &admin, json!({ "full_name": "Bilal Ahmed" }))
        .await;
    let (sana, bilal) = (id(&sana["id"]), id(&bilal["id"]));

    // Teacher grid: sections × subjects, empty at first.
    let (s, grid) = app
        .get(&format!("/allocation/teachers?class_id={c1}"), &admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{grid}");
    assert_eq!(grid["sections"].as_array().unwrap().len(), 3);
    assert_eq!(grid["subjects"].as_array().unwrap().len(), 2);
    assert!(grid["sections"][0]["cells"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["staff_id"].is_null()));
    // Set a cell, then replace its teacher; the load counts follow.
    for (subject, staff) in [(&eng, &sana), (&mth, &sana), (&eng, &bilal)] {
        let (s, _) = app
            .put(
                "/allocation/teachers/cell",
                &admin,
                json!({ "section_id": sa, "subject_id": subject, "staff_id": staff }),
            )
            .await;
        assert_eq!(s, StatusCode::NO_CONTENT);
    }
    let (_, grid) = app
        .get(&format!("/allocation/teachers?class_id={c1}"), &admin)
        .await;
    let row_a = grid["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["section_id"] == sa)
        .unwrap()
        .clone();
    let cell = |subject: &str| {
        row_a["cells"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["subject_id"] == subject)
            .unwrap()
            .clone()
    };
    assert_eq!(cell(&eng)["staff_name"], "Bilal Ahmed");
    assert_eq!(cell(&eng)["extra"], 0);
    assert_eq!(cell(&mth)["staff_name"], "Sana Malik");
    let load = |name: &str| {
        grid["staff"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["full_name"] == name)
            .unwrap()
            .clone()
    };
    assert_eq!(load("Sana Malik")["cells"], 1);
    assert_eq!(load("Bilal Ahmed")["cells"], 1);
    // The staff member's own assignment list agrees.
    let (_, a) = app.get(&format!("/staff/{sana}/assignments"), &admin).await;
    assert_eq!(a["assignments"].as_array().unwrap().len(), 1);
    assert_eq!(a["assignments"][0]["subject_id"], mth);
    // Clearing a cell works; a subject not taught in the class is refused.
    let (s, _) = app
        .put(
            "/allocation/teachers/cell",
            &admin,
            json!({ "section_id": sa, "subject_id": eng, "staff_id": null }),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, other) = app
        .post("/subjects", &admin, json!({ "name": "Art", "code": "ART" }))
        .await;
    let (s, _) = app
        .put(
            "/allocation/teachers/cell",
            &admin,
            json!({ "section_id": sa, "subject_id": other["id"], "staff_id": sana }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, audit) = app.get("/audit?limit=200", &admin).await;
    assert_eq!(s, StatusCode::OK, "{audit}");
    assert_eq!(count_entity(&audit, "section_teacher_cell"), 4);

    // Students: 5 in A, 1 in B, none in C.
    let student = |name: &str, section: &str, roll: i32, gender: &str| json!({ "full_name": name, "father_name": "Father", "date_of_birth": "2019-01-01", "gender": gender, "admission_date": "2026-04-01", "section_id": section, "roll_no": roll });
    let mut in_a = Vec::new();
    for (i, (name, g)) in [
        ("Ali Khan", "male"),
        ("Sara Khan", "female"),
        ("Omar Khan", "male"),
        ("Hina Khan", "female"),
        ("Zain Khan", "male"),
    ]
    .iter()
    .enumerate()
    {
        let (_, st) = app
            .post("/students", &admin, student(name, &sa, i as i32 + 1, g))
            .await;
        in_a.push(id(&st["id"]));
    }
    let (_, st) = app
        .post("/students", &admin, student("Bina Khan", &sb, 1, "female"))
        .await;
    let bina = id(&st["id"]);
    let (s, view) = app
        .get(&format!("/allocation/sections?class_id={c1}"), &admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{view}");
    let sec = |v: &Value, sid: &str| {
        v["sections"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["section_id"] == sid)
            .unwrap()
            .clone()
    };
    assert_eq!(sec(&view, &sa)["size"], 5);
    assert_eq!(sec(&view, &sa)["male"], 3);
    assert_eq!(sec(&view, &sa)["female"], 2);
    assert_eq!(sec(&view, &sc)["size"], 0);
    // Balance preview: 6 students over 3 sections → 2 each; Zain (roll 5) is excluded, so
    // Hina (roll 4) moves first.
    let (s, preview) = app
        .post(
            "/allocation/balance",
            &admin,
            json!({ "class_id": c1, "exclude_student_ids": [in_a[4]] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{preview}");
    let moves = preview["moves"].as_array().unwrap().clone();
    assert_eq!(moves.len(), 3);
    assert!(moves.iter().all(|m| m["student_id"] != in_a[4]));
    assert_eq!(moves[0]["full_name"], "Hina Khan");
    assert_eq!(moves[0]["from_name"], "A");
    assert!(preview["sizes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|x| x["after"] == 2));
    // Apply the proposal with fresh roll numbers.
    let apply: Vec<Value> = moves
        .iter()
        .map(|m| json!({ "student_id": m["student_id"], "to_section_id": m["to_section_id"] }))
        .collect();
    let (s, applied) = app
        .post(
            "/allocation/moves",
            &admin,
            json!({ "class_id": c1, "moves": apply, "roll_numbers": "reassign" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{applied}");
    assert_eq!(applied["moved"], 3);
    let view = applied["view"].clone();
    assert!(view["sections"]
        .as_array()
        .unwrap()
        .iter()
        .all(|x| x["size"] == 2));
    let rolls = |v: &Value, sid: &str| -> Vec<i64> {
        sec(v, sid)["students"]
            .as_array()
            .unwrap()
            .iter()
            .map(|st| st["roll_no"].as_i64().unwrap())
            .collect()
    };
    // C was empty (1, 2); B had roll 1, the newcomer is 2.
    assert_eq!(rolls(&view, &sb), vec![1, 2]);
    assert_eq!(rolls(&view, &sc), vec![1, 2]);
    // Keep the roll number when moving Bina to A.
    let (s, applied) = app
        .post(
            "/allocation/moves",
            &admin,
            json!({ "class_id": c1, "moves": [{ "student_id": bina, "to_section_id": sa }], "roll_numbers": "keep" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{applied}");
    let bina_row = sec(&applied["view"], &sa)["students"]
        .as_array()
        .unwrap()
        .iter()
        .find(|st| st["student_id"] == bina)
        .unwrap()
        .clone();
    assert_eq!(bina_row["roll_no"], 1);
    // A section of another class is refused; a teacher may neither view nor move.
    let (_, c2) = app
        .post("/classes", &admin, json!({ "name": "2", "level": 2 }))
        .await;
    let (_, s2a) = app
        .post(
            "/sections",
            &admin,
            json!({ "class_id": c2["id"], "academic_year_id": year_id, "name": "A" }),
        )
        .await;
    let (s, _) = app
        .post(
            "/allocation/moves",
            &admin,
            json!({ "class_id": c1, "moves": [{ "student_id": bina, "to_section_id": s2a["id"] }] }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, login) = app
        .post(
            &format!("/staff/{sana}/login"),
            &admin,
            json!({ "username": "sana", "password": "teacher-password-1", "role": "teacher" }),
        )
        .await;
    assert!(s.is_success(), "{login}");
    let teacher = app.login("sana", "teacher-password-1").await;
    let (s, _) = app
        .get(&format!("/allocation/sections?class_id={c1}"), &teacher)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app
        .put(
            "/allocation/teachers/cell",
            &teacher,
            json!({ "section_id": sa, "subject_id": eng, "staff_id": sana }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // Every move is an audited enrolment update (6 admissions + 4 moves).
    let (_, audit) = app.get("/audit?limit=500", &admin).await;
    assert!(count_entity(&audit, "enrollment") >= 10, "{audit}");
    app.finish().await;
}
