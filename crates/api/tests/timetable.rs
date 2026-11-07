//! Standard stage 1: the timetable. Lite refuses the module; Standard sets up shifts, rooms
//! and lessons, generates a timetable in the background, lets the office move and lock
//! cards, publishes, prints, and handles substitutions with cover suggestions. Needs
//! PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

use std::time::Duration;

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
    async fn delete(&self, p: &str, t: &str) -> (StatusCode, Value) {
        self.call(Method::DELETE, p, Some(t), None).await
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

async fn setup_school(app: &App) -> String {
    let (s, b) = app
        .call(
            Method::POST,
            "/setup",
            None,
            Some(json!({
                "school_name": "Gulshan High", "school_code": "KHI-TT", "academic_year_start_month": 4,
                "admin_full_name": "Ayesha", "admin_username": "ayesha", "admin_password": "admin-password-1",
                "backup_password": "backup-password-1"
            })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    app.login("ayesha", "admin-password-1").await
}

#[tokio::test]
async fn lite_refuses_the_timetable_and_hides_its_reports() {
    let app = app!();
    let admin = setup_school(&app).await;
    let (s, b) = app.get("/timetable/setup", &admin).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    assert_eq!(b["error"]["code"], "feature_unavailable");
    let (s, b) = app.get("/timetable/rooms", &admin).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    let (s, b) = app.get("/reports", &admin).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(
        !b.as_array()
            .unwrap()
            .iter()
            .any(|m| m["module"] == "timetable"),
        "Lite shows no timetable reports: {b}"
    );
    let (s, b) = app.get("/reports/timetable.teacher_load", &admin).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    app.finish().await;
}

/// Builds a small Standard school: one year, two classes with two sections each, four
/// subjects, four teachers (one per subject) and two rooms. Returns (year id, section ids,
/// subject ids, teacher ids).
async fn standard_school(
    app: &App,
    admin: &str,
) -> (String, Vec<String>, Vec<String>, Vec<String>) {
    let (s, b) = app
        .put("/settings/edition", admin, json!({ "edition": "standard" }))
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (_, year) = app
        .post(
            "/academic-years",
            admin,
            json!({ "start_year": 2026, "start_month": 4, "activate": true }),
        )
        .await;
    let year_id = id(&year["id"]);
    let mut sections = Vec::new();
    for (name, level) in [("9", 9), ("10", 10)] {
        let (_, c) = app
            .post("/classes", admin, json!({ "name": name, "level": level }))
            .await;
        for sec in ["A", "B"] {
            let (s, b) = app
                .post(
                    "/sections",
                    admin,
                    json!({ "class_id": c["id"], "academic_year_id": year_id, "name": sec }),
                )
                .await;
            assert_eq!(s, StatusCode::OK, "{b}");
            sections.push(id(&b["id"]));
        }
    }
    let mut subjects = Vec::new();
    for (name, code) in [
        ("Mathematics", "MATH"),
        ("English", "ENG"),
        ("Urdu", "URD"),
        ("Science", "SCI"),
    ] {
        let (s, b) = app
            .post("/subjects", admin, json!({ "name": name, "code": code }))
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        subjects.push(id(&b["id"]));
    }
    // Zara teaches no lesson, so she is always free to cover.
    let mut teachers = Vec::new();
    for name in [
        "Sana Malik",
        "Bilal Ahmed",
        "Hina Raza",
        "Omar Farooq",
        "Zara Qureshi",
    ] {
        let (s, b) = app
            .post("/staff", admin, json!({ "full_name": name }))
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        teachers.push(id(&b["id"]));
    }
    for (name, short) in [("Room 1", "R1"), ("Science lab", "LAB")] {
        let (s, b) = app
            .post(
                "/timetable/rooms",
                admin,
                json!({ "name": name, "short_name": short, "building": "Main" }),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
    }
    (year_id, sections, subjects, teachers)
}

async fn wait_for_job(app: &App, admin: &str, job_id: &str) -> Value {
    for _ in 0..600 {
        let (s, b) = app
            .get(&format!("/timetable/generate/{job_id}"), admin)
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        if b["status"] != "running" {
            return b;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("the generator did not finish in time");
}

#[tokio::test]
async fn standard_generates_edits_publishes_prints_and_covers() {
    let app = app!();
    let admin = setup_school(&app).await;
    let (year_id, sections, subjects, teachers) = standard_school(&app, &admin).await;

    // A teacher login (Sana) sees the module read-only.
    let (s, b) = app
        .post(
            &format!("/staff/{}/login", teachers[0]),
            &admin,
            json!({ "username": "sana", "password": "teacher-password-1", "role": "teacher" }),
        )
        .await;
    assert!(s.is_success(), "{b}");
    let teacher = app.login("sana", "teacher-password-1").await;

    // Setup is empty until a shift exists; the pre-check says so.
    let (s, b) = app.get("/timetable/setup", &admin).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(b["shifts"].as_array().unwrap().is_empty());
    assert!(b["check"]["errors"].as_u64().unwrap() > 0, "{}", b["check"]);
    let (s, b) = app.get("/timetable/setup", &teacher).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = app
        .post("/timetable/shifts/default", &teacher, json!({}))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");

    // The default shift: Mon–Thu 7 periods, Fri 5.
    let (s, shift) = app
        .post("/timetable/shifts/default", &admin, json!({}))
        .await;
    assert_eq!(s, StatusCode::CREATED, "{shift}");
    let shift_id = id(&shift["id"]);
    assert_eq!(shift["days"][4]["periods"], 5);
    assert_eq!(shift["period_times"].as_array().unwrap().len(), 7);
    // Bad shifts are refused with a plain message.
    let (s, b) = app
        .post(
            "/timetable/shifts",
            &admin,
            json!({ "name": "Odd", "days": shift["days"], "period_times": [{ "start": "8am", "end": "09:00" }] }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");

    // Lessons: every section takes every subject from its subject's teacher, 6 sessions a
    // week (one double for science).
    let mut lesson_ids = Vec::new();
    for sec in &sections {
        for (i, subj) in subjects.iter().enumerate() {
            let (s, b) = app
                .post(
                    "/timetable/lessons",
                    &admin,
                    json!({
                        "subject_id": subj, "teacher_ids": [teachers[i]], "section_ids": [sec],
                        "periods_per_card": if i == 3 { 2 } else { 1 },
                        "cards_per_week": if i == 3 { 3 } else { 6 }
                    }),
                )
                .await;
            assert_eq!(s, StatusCode::CREATED, "{b}");
            lesson_ids.push(id(&b["id"]));
        }
    }
    // A lesson for a section of another year is refused.
    let (s, b) = app
        .post(
            "/timetable/lessons",
            &admin,
            json!({ "subject_id": subjects[0], "section_ids": [uuid::Uuid::nil()], "cards_per_week": 1 }),
        )
        .await;
    assert!(
        s == StatusCode::NOT_FOUND || s == StatusCode::BAD_REQUEST,
        "{b}"
    );

    // Rules: Bilal never teaches period 1 on Monday; 9 A's home room is Room 1.
    let (s, b) = app
        .put(
            "/timetable/rules",
            &admin,
            json!({
                "kind": "teacher", "entity_id": teachers[1],
                "time_off": [{ "shift_id": shift_id, "day": 0, "period": 1, "state": 2 }],
                "limits": { "max_per_day": 6 }
            }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (_, rooms) = app.get("/timetable/rooms", &admin).await;
    let room1 = id(&rooms[0]["id"]);
    let (s, b) = app
        .put(
            "/timetable/rules",
            &admin,
            json!({ "kind": "section", "entity_id": sections[0], "shift_ids": [shift_id], "home_room_id": room1 }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");

    // The pre-check is clean and the load adds up: 4 sections × (3×6 + 3×2) = 96 periods.
    let (s, setup) = app.get("/timetable/setup", &admin).await;
    assert_eq!(s, StatusCode::OK, "{setup}");
    assert_eq!(setup["check"]["errors"], 0, "{}", setup["check"]);
    assert_eq!(setup["lessons"].as_array().unwrap().len(), 16);
    assert_eq!(setup["lessons"][0]["teachers"][0], "Sana Malik");
    assert_eq!(setup["load"]["classes"][0]["used"], 24);
    assert_eq!(setup["load"]["classes"][0]["available"], 33);
    assert_eq!(setup["load"]["cards"], 84);

    // Generate in the background and wait.
    let (s, job) = app
        .post(
            "/timetable/generate",
            &admin,
            json!({ "time_limit_seconds": 30, "name": "Draft 1" }),
        )
        .await;
    assert_eq!(s, StatusCode::ACCEPTED, "{job}");
    let timetable_id = id(&job["timetable_id"]);
    let (s, b) = app
        .post(
            "/timetable/generate",
            &admin,
            json!({ "timetable_id": timetable_id }),
        )
        .await;
    assert!(
        s == StatusCode::CONFLICT || s == StatusCode::ACCEPTED,
        "{b}"
    );
    let done = wait_for_job(&app, &admin, &id(&job["id"])).await;
    assert_eq!(done["status"], "done", "{done}");
    assert_eq!(
        done["report"]["placed"], done["report"]["total"],
        "{}",
        done["report"]["unplaced"]
    );
    assert_eq!(done["report"]["total"], 84);

    // The timetable has every session, no problems, and Bilal is free Monday period 1.
    let (s, view) = app
        .get(&format!("/timetable/timetables/{timetable_id}"), &admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{view}");
    let cards = view["cards"].as_array().unwrap();
    assert_eq!(cards.len(), 84);
    assert!(
        view["problems"].as_array().unwrap().is_empty(),
        "{}",
        view["problems"]
    );
    let bilal_lessons: Vec<&Value> = view["lessons"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["teacher_ids"][0] == teachers[1])
        .collect();
    assert_eq!(bilal_lessons.len(), 4);
    assert!(!cards.iter().any(|c| {
        c["day"] == 0 && c["period"] == 1 && bilal_lessons.iter().any(|l| l["id"] == c["lesson_id"])
    }));
    // The same slot never holds two lessons of one section.
    for a in cards {
        for b in cards {
            if a["id"] != b["id"] && a["day"] == b["day"] && a["period"] == b["period"] {
                let la = &view["lessons"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|l| l["id"] == a["lesson_id"])
                    .unwrap();
                let lb = &view["lessons"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|l| l["id"] == b["lesson_id"])
                    .unwrap();
                assert_ne!(
                    la["section_ids"][0], lb["section_ids"][0],
                    "section clash at {} {}",
                    a["day"], a["period"]
                );
                assert_ne!(
                    la["teacher_ids"][0], lb["teacher_ids"][0],
                    "teacher clash at {} {}",
                    a["day"], a["period"]
                );
            }
        }
    }

    // Manual edit: move the first card to a slot the checker marks ok; a blocked slot is refused.
    let card = &cards[0];
    let (s, options) = app
        .get(
            &format!(
                "/timetable/timetables/{timetable_id}/options?lesson_id={}&number={}",
                id(&card["lesson_id"]),
                card["number"]
            ),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{options}");
    let ok = options
        .as_array()
        .unwrap()
        .iter()
        .find(|o| {
            o["status"] == "ok" && !(o["day"] == card["day"] && o["period"] == card["period"])
        })
        .expect("a free slot");
    let (s, moved) = app
        .post(
            &format!("/timetable/timetables/{timetable_id}/move"),
            &admin,
            json!({ "lesson_id": card["lesson_id"], "number": card["number"], "day": ok["day"], "period": ok["period"] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{moved}");
    let moved_card = moved["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["lesson_id"] == card["lesson_id"] && c["number"] == card["number"])
        .unwrap();
    assert_eq!(moved_card["day"], ok["day"]);
    assert_eq!(moved_card["period"], ok["period"]);
    assert!(
        moved["problems"].as_array().unwrap().is_empty(),
        "{}",
        moved["problems"]
    );
    if let Some(blocked) = options
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["status"] == "blocked")
    {
        let (s, b) = app
            .post(
                &format!("/timetable/timetables/{timetable_id}/move"),
                &admin,
                json!({ "lesson_id": card["lesson_id"], "number": card["number"], "day": blocked["day"], "period": blocked["period"] }),
            )
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
    }
    // Lock it; a locked card cannot be moved; the teacher cannot edit at all.
    let (s, locked) = app
        .post(
            &format!("/timetable/timetables/{timetable_id}/lock"),
            &admin,
            json!({ "lesson_id": card["lesson_id"], "number": card["number"], "locked": true }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{locked}");
    assert_eq!(locked["locked"], true);
    let (s, b) = app
        .post(
            &format!("/timetable/timetables/{timetable_id}/move"),
            &admin,
            json!({ "lesson_id": card["lesson_id"], "number": card["number"], "day": card["day"], "period": card["period"] }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
    let (s, b) = app
        .post(
            &format!("/timetable/timetables/{timetable_id}/lock"),
            &teacher,
            json!({ "lesson_id": card["lesson_id"], "number": card["number"], "locked": false }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");

    // Publish: the current timetable is this one; Sana's own week lists only her lessons.
    let (s, b) = app.get("/timetable/current", &admin).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(b["timetable"].is_null());
    let (s, b) = app
        .post(
            &format!("/timetable/timetables/{timetable_id}/publish"),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["status"], "published");
    let (s, b) = app.get("/timetable/current", &admin).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["timetable"]["id"], timetable_id);
    let (s, b) = app
        .delete(&format!("/timetable/timetables/{timetable_id}"), &admin)
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    let (s, mine) = app.get("/timetable/mine", &teacher).await;
    assert_eq!(s, StatusCode::OK, "{mine}");
    assert_eq!(mine["lessons"].as_array().unwrap().len(), 4);
    assert_eq!(mine["cards"].as_array().unwrap().len(), 24);
    assert!(mine["today"].is_number());
    let (s, b) = app.get("/timetable/mine", &admin).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{b}");

    // Printouts and reports (en and ur) are PDFs.
    for (path, who) in [
        (
            format!("/timetable/print?kind=section&id={}&lang=ur", sections[0]),
            &admin,
        ),
        (
            format!("/timetable/print?kind=teacher&id={}", teachers[0]),
            &teacher,
        ),
        (format!("/timetable/print?kind=room&id={room1}"), &admin),
        (
            format!(
                "/reports/timetable.section?format=pdf&class_id=x&section_id={}",
                sections[1]
            ),
            &admin,
        ),
        (
            "/reports/timetable.teacher_load?format=pdf&lang=ur".to_string(),
            &admin,
        ),
        (
            "/reports/timetable.free_rooms?format=pdf&day=1".to_string(),
            &admin,
        ),
    ] {
        let (s, ctype, bytes) = app.get_bytes(&path, who).await;
        assert_eq!(s, StatusCode::OK, "{path}");
        assert_eq!(ctype, "application/pdf", "{path}");
        assert!(bytes.starts_with(b"%PDF"), "{path}");
    }
    let (s, load) = app.get("/reports/timetable.teacher_load", &admin).await;
    assert_eq!(s, StatusCode::OK, "{load}");
    assert_eq!(load["groups"][0]["rows"].as_array().unwrap().len(), 5);
    assert_eq!(load["groups"][0]["rows"][0][0], "Bilal Ahmed");
    assert_eq!(
        load["groups"][0]["rows"][0][3], "24",
        "{}",
        load["groups"][0]["rows"][0]
    );
    let (s, b) = app.get("/reports", &admin).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(b
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["module"] == "timetable"));

    // Substitutions: Sana is away on the first Monday of term; her sessions need cover.
    let monday = "2026-04-06";
    let (s, absence) = app
        .post(
            "/timetable/absences",
            &admin,
            json!({ "staff_id": teachers[0], "from_date": monday, "note": "Sick" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{absence}");
    let (s, day) = app
        .get(&format!("/timetable/substitutions?date={monday}"), &admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{day}");
    assert_eq!(day["day"]["working"], true);
    let needs = day["day"]["needs"].as_array().unwrap();
    let sana_monday = mine["cards"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["day"] == 0)
        .count();
    assert_eq!(needs.len(), sana_monday, "{}", day["day"]);
    assert!(!needs.is_empty());
    let need = &needs[0];
    assert_eq!(need["absentName"], "Sana Malik");
    let candidate = need["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["suitable"] == true)
        .expect("a free teacher");
    let (s, cover) = app
        .put(
            "/timetable/covers",
            &admin,
            json!({
                "absence_id": absence["id"], "date": monday, "lesson_id": need["lessonId"],
                "number": need["number"], "cover_staff_id": candidate["teacherId"]
            }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{cover}");
    // A teacher busy at that time is refused.
    if let Some(busy) = need["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["suitable"] == false)
    {
        let (s, b) = app
            .put(
                "/timetable/covers",
                &admin,
                json!({
                    "absence_id": absence["id"], "date": monday, "lesson_id": need["lessonId"],
                    "number": need["number"], "cover_staff_id": busy["teacherId"]
                }),
            )
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
    }
    let (s, day) = app
        .get(&format!("/timetable/substitutions?date={monday}"), &admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{day}");
    assert_eq!(
        day["day"]["needs"][0]["coverTeacher"],
        candidate["teacherId"]
    );
    assert_eq!(day["counts"]["total"], 1);
    assert_eq!(day["absences"][0]["staff_name"], "Sana Malik");
    // The cover sheet lists the day; as JSON and as a PDF, also through the report catalogue.
    let (s, sheet) = app
        .get(
            &format!("/timetable/cover-sheet?from={monday}&to=2026-04-10"),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{sheet}");
    assert_eq!(sheet.as_array().unwrap().len(), 1);
    assert_eq!(sheet[0]["rows"].as_array().unwrap().len(), needs.len());
    assert!(sheet[0]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["covered"] == true));
    for path in [
        format!("/timetable/cover-sheet/pdf?from={monday}&to={monday}&lang=ur"),
        format!("/reports/timetable.cover_sheet?format=pdf&from={monday}&to=2026-04-10"),
    ] {
        let (s, ctype, bytes) = app.get_bytes(&path, &admin).await;
        assert_eq!(s, StatusCode::OK, "{path}");
        assert_eq!(ctype, "application/pdf");
        assert!(bytes.starts_with(b"%PDF"));
    }
    // Removing the absence removes its covers.
    let (s, _) = app
        .delete(
            &format!("/timetable/absences/{}", id(&absence["id"])),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, day) = app
        .get(&format!("/timetable/substitutions?date={monday}"), &admin)
        .await;
    assert!(day["day"]["needs"].as_array().unwrap().is_empty());
    assert_eq!(day["counts"]["total"], 0);

    // Deleting a lesson removes its cards; the checker then reports nothing wrong.
    let (s, _) = app
        .delete(&format!("/timetable/lessons/{}", lesson_ids[0]), &admin)
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, view) = app
        .get(&format!("/timetable/timetables/{timetable_id}"), &admin)
        .await;
    assert_eq!(view["cards"].as_array().unwrap().len(), 78);

    // Back on Lite the module is refused again and nothing else changed.
    let (s, b) = app
        .put("/settings/edition", &admin, json!({ "edition": "lite" }))
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = app.get("/timetable/current", &admin).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    let (s, b) = app
        .get(&format!("/sections?academic_year_id={year_id}"), &admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b.as_array().unwrap().len(), 4);
    app.finish().await;
}

#[tokio::test]
async fn lessons_can_be_seeded_from_the_allocation_grid() {
    let app = app!();
    let admin = setup_school(&app).await;
    let (_, sections, subjects, teachers) = standard_school(&app, &admin).await;
    // Sana teaches Mathematics to 9 A and 9 B.
    let (s, b) = app
        .put(
            &format!("/staff/{}/assignments", teachers[0]),
            &admin,
            json!({ "assignments": [
                { "section_id": sections[0], "subject_id": subjects[0] },
                { "section_id": sections[1], "subject_id": subjects[0] }
            ] }),
        )
        .await;
    assert!(s.is_success(), "{b}");
    let (s, b) = app
        .post(
            "/timetable/lessons/seed",
            &admin,
            json!({ "cards_per_week": 5 }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["created"], 2);
    let (s, b) = app.post("/timetable/lessons/seed", &admin, json!({})).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["created"], 0, "seeding is idempotent");
    let (_, setup) = app.get("/timetable/setup", &admin).await;
    let lessons = setup["lessons"].as_array().unwrap();
    assert_eq!(lessons.len(), 2);
    assert_eq!(lessons[0]["subject"], "Mathematics");
    assert_eq!(lessons[0]["cards_per_week"], 5);
    assert_eq!(lessons[0]["teachers"][0], "Sana Malik");
    app.finish().await;
}
