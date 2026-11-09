//! Standard stage 6: hostel. Lite refuses the module; Standard runs hostels with rooms and
//! beds, allocations (no double booking of a bed, moves that keep the history), the
//! occupancy board, the leave-out and visitor register with consent, the monthly charge on
//! the fee voucher and the reports. Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

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

fn num(v: &Value) -> f64 {
    match v {
        Value::String(s) => s.parse().unwrap(),
        Value::Number(n) => n.as_f64().unwrap(),
        _ => panic!("not a number: {v}"),
    }
}

async fn setup_school(app: &App) -> String {
    let (s, b) = app
        .call(
            Method::POST,
            "/setup",
            None,
            Some(json!({
                "school_name": "Gulshan High", "school_code": "KHI-HS", "academic_year_start_month": 4,
                "admin_full_name": "Ayesha", "admin_username": "ayesha", "admin_password": "admin-password-1",
                "backup_password": "backup-password-1"
            })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    app.login("ayesha", "admin-password-1").await
}

async fn student(app: &App, admin: &str, name: &str, section: &str, roll: i32) -> String {
    let (s, b) = app
        .post(
            "/students",
            admin,
            json!({ "full_name": name, "father_name": "Father", "date_of_birth": "2015-01-01", "gender": "male",
                    "admission_date": "2026-04-01", "section_id": section, "roll_no": roll }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    id(&b["id"])
}

#[tokio::test]
async fn lite_refuses_the_hostel() {
    let app = app!();
    let admin = setup_school(&app).await;
    for path in [
        "/hostel/hostels",
        "/hostel/residents",
        "/hostel/register",
        // A bad query string must not turn the edition refusal into a validation error.
        "/hostel/residents?academic_year_id=not-a-uuid",
        "/hostel/board?academic_year_id=nope",
        "/hostel/allocations?student_id=x",
        "/hostel/register?from=yesterday",
    ] {
        let (s, b) = app.get(path, &admin).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{path}: {b}");
        assert_eq!(b["error"]["code"], "feature_unavailable", "{path}: {b}");
    }
    let (s, b) = app.get("/reports", &admin).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(!b
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["module"] == "hostel"));
    app.finish().await;
}

#[tokio::test]
async fn standard_runs_rooms_allocations_register_fees_and_reports() {
    let app = app!();
    let admin = setup_school(&app).await;
    let (s, b) = app
        .put(
            "/settings/edition",
            &admin,
            json!({ "edition": "standard" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (_, year) = app
        .post(
            "/academic-years",
            &admin,
            json!({ "start_year": 2026, "start_month": 4, "activate": true }),
        )
        .await;
    let year_id = id(&year["id"]);
    let (_, class) = app
        .post("/classes", &admin, json!({ "name": "9", "level": 9 }))
        .await;
    let (_, section) = app
        .post(
            "/sections",
            &admin,
            json!({ "class_id": class["id"], "academic_year_id": year_id, "name": "A" }),
        )
        .await;
    let section_id = id(&section["id"]);
    let ali = student(&app, &admin, "Ali Khan", &section_id, 1).await;
    let sara = student(&app, &admin, "Sara Noor", &section_id, 2).await;
    let omar = student(&app, &admin, "Omar Shah", &section_id, 3).await;

    // A hostel with a fee head and two rooms.
    let (s, head) = app
        .post(
            "/fees/heads",
            &admin,
            json!({ "name": "Hostel", "code": "HS", "is_recurring": true }),
        )
        .await;
    assert!(s.is_success(), "{head}");
    let (s, b) = app
        .post("/hostel/hostels", &admin, json!({ "name": "" }))
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    let (s, hostel) = app
        .post(
            "/hostel/hostels",
            &admin,
            json!({ "name": "Iqbal House", "kind": "boys", "warden_name": "Mr Rashid", "monthly_fee": "8000",
                    "fee_head_id": head["id"], "requires_consent": true }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{hostel}");
    let hostel_id = id(&hostel["id"]);
    assert_eq!(hostel["fee_head_name"], "Hostel");
    let (s, r1) = app
        .post(
            &format!("/hostel/hostels/{hostel_id}/rooms"),
            &admin,
            json!({ "floor": "Ground", "name": "G-1", "beds": 2 }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{r1}");
    let r1 = id(&r1["id"]);
    let (s, r2) = app
        .post(
            &format!("/hostel/hostels/{hostel_id}/rooms"),
            &admin,
            json!({ "floor": "First", "name": "F-1", "beds": 1, "monthly_fee": "9500" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{r2}");
    let r2 = id(&r2["id"]);
    let (_, d) = app
        .get(&format!("/hostel/hostels/{hostel_id}"), &admin)
        .await;
    assert_eq!(d["room_list"].as_array().unwrap().len(), 2);
    assert_eq!(d["beds"], 3);

    // Allocate: a bed must exist and be free; one bed per student.
    let (s, a1) = app
        .post(
            "/hostel/allocations",
            &admin,
            json!({ "student_id": ali, "academic_year_id": year_id, "room_id": r1, "bed_no": 1, "from_date": "2026-04-01" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{a1}");
    assert_eq!(num(&a1["charge"]), 8000.0);
    let (s, b) = app
        .post(
            "/hostel/allocations",
            &admin,
            json!({ "student_id": sara, "academic_year_id": year_id, "room_id": r1, "bed_no": 1, "from_date": "2026-04-01" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "double booking: {b}");
    let (s, b) = app
        .post(
            "/hostel/allocations",
            &admin,
            json!({ "student_id": sara, "academic_year_id": year_id, "room_id": r1, "bed_no": 3, "from_date": "2026-04-01" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "no such bed: {b}");
    let (s, a2) = app
        .post(
            "/hostel/allocations",
            &admin,
            json!({ "student_id": sara, "academic_year_id": year_id, "room_id": r1, "bed_no": 2, "from_date": "2026-04-01" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{a2}");
    let (s, a3) = app
        .post(
            "/hostel/allocations",
            &admin,
            json!({ "student_id": omar, "academic_year_id": year_id, "room_id": r2, "bed_no": 1, "from_date": "2026-04-01", "amount": "5000" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{a3}");
    assert_eq!(num(&a3["charge"]), 5000.0);
    // The room cannot shrink under a resident, nor be removed.
    let (s, _) = app
        .put(
            &format!("/hostel/rooms/{r1}"),
            &admin,
            json!({ "floor": "Ground", "name": "G-1", "beds": 1 }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = app.delete(&format!("/hostel/rooms/{r1}"), &admin).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = app
        .delete(&format!("/hostel/hostels/{hostel_id}"), &admin)
        .await;
    assert_eq!(s, StatusCode::CONFLICT);

    // The board shows every bed.
    let (s, board) = app
        .get(
            &format!("/hostel/board?hostel_id={hostel_id}&academic_year_id={year_id}"),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{board}");
    assert_eq!(board["occupancy"]["beds"], 3);
    assert_eq!(board["occupancy"]["occupied"], 3);
    assert_eq!(board["occupancy"]["free"], 0);
    let g1 = board["by_room"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["room"]["name"] == "G-1")
        .unwrap();
    assert_eq!(g1["beds"][0]["resident"]["student_name"], "Ali Khan");
    assert_eq!(g1["beds"][1]["resident"]["student_name"], "Sara Noor");

    // Sara leaves in May: her bed frees, the history stays. Omar moves into it in June.
    let (s, b) = app
        .put(
            &format!("/hostel/allocations/{}", id(&a2["id"])),
            &admin,
            json!({ "to_date": "2026-05-15" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, a4) = app
        .post(
            "/hostel/allocations",
            &admin,
            json!({ "student_id": omar, "academic_year_id": year_id, "room_id": r1, "bed_no": 2, "from_date": "2026-06-01" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{a4}");
    assert_eq!(
        num(&a4["charge"]),
        8000.0,
        "the own amount does not follow a move"
    );
    let (_, history) = app
        .get(
            &format!("/hostel/allocations?student_id={omar}&academic_year_id={year_id}"),
            &admin,
        )
        .await;
    let h = history.as_array().unwrap();
    assert_eq!(h.len(), 2);
    assert!(h
        .iter()
        .any(|a| a["room_name"] == "F-1" && a["to_date"] == "2026-05-31"));
    let (_, residents) = app
        .get(
            &format!("/hostel/residents?academic_year_id={year_id}"),
            &admin,
        )
        .await;
    assert_eq!(residents.as_array().unwrap().len(), 2);

    // Register: a leave-out needs consent here; a visitor does not.
    let (s, b) = app
        .post(
            "/hostel/register",
            &admin,
            json!({ "student_id": ali, "kind": "leave_out", "expected_back": "2026-06-07T18:00:00Z" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "consent: {b}");
    let (s, out) = app
        .post(
            "/hostel/register",
            &admin,
            json!({ "student_id": ali, "kind": "leave_out", "started_at": "2026-06-05T09:00:00Z", "expected_back": "2026-06-07T18:00:00Z",
                    "guardian_consent": true, "consent_note": "Father phoned" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{out}");
    assert_eq!(out["student_name"], "Ali Khan");
    assert_eq!(out["room_name"], "G-1");
    let (s, b) = app
        .post(
            "/hostel/register",
            &admin,
            json!({ "student_id": ali, "kind": "leave_out", "guardian_consent": true }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "already out: {b}");
    let (s, b) = app
        .post(
            "/hostel/register",
            &admin,
            json!({ "student_id": sara, "kind": "visitor", "visitor_name": "Mrs Noor" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "not a resident any more: {b}");
    let (s, visit) = app
        .post(
            "/hostel/register",
            &admin,
            json!({ "student_id": omar, "kind": "visitor", "visitor_name": "Mr Shah", "visitor_relationship": "father" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{visit}");
    let (_, open) = app.get("/hostel/register?open=true", &admin).await;
    assert_eq!(open.as_array().unwrap().len(), 2);
    let (s, b) = app
        .post(
            &format!("/hostel/register/{}/close", id(&out["id"])),
            &admin,
            json!({ "at": "2026-06-07T17:30:00Z" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["ended_at"], "2026-06-07T17:30:00Z");
    let (s, _) = app
        .post(
            &format!("/hostel/register/{}/close", id(&out["id"])),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (_, open) = app.get("/hostel/register?open=true", &admin).await;
    assert_eq!(open.as_array().unwrap().len(), 1);

    // The hostel charge joins the voucher: Ali the hostel fee, Omar the room fee in June
    // (his own amount ended with the F-1 allocation), Sara nothing after she left.
    let (s, gen) = app
        .post(
            "/fees/vouchers/generate",
            &admin,
            json!({ "month": "2026-06-01", "academic_year_id": year_id }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{gen}");
    assert_eq!(gen["created"], 2, "{gen}");
    let (_, v) = app
        .get(&format!("/fees/vouchers?student_id={ali}"), &admin)
        .await;
    assert_eq!(num(&v[0]["total_due"]), 8000.0);
    let (_, v) = app
        .get(&format!("/fees/vouchers?student_id={omar}"), &admin)
        .await;
    assert_eq!(num(&v[0]["total_due"]), 8000.0);
    let (_, v) = app
        .get(&format!("/fees/vouchers?student_id={sara}"), &admin)
        .await;
    assert_eq!(v.as_array().unwrap().len(), 0);

    // Reports.
    for path in [
        &format!("/reports/hostel.occupancy?academic_year_id={year_id}&format=pdf"),
        &format!("/reports/hostel.residents?academic_year_id={year_id}&format=pdf&lang=ur"),
        &format!("/reports/hostel.outstanding?academic_year_id={year_id}&format=pdf"),
    ] {
        let (s, ctype, bytes) = app.get_bytes(path, &admin).await;
        assert_eq!(s, StatusCode::OK, "{path}");
        assert_eq!(ctype, "application/pdf");
        assert!(bytes.starts_with(b"%PDF"));
    }
    let (s, out) = app
        .get(
            &format!("/reports/hostel.outstanding?academic_year_id={year_id}"),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{out}");
    assert_eq!(out["groups"][0]["rows"].as_array().unwrap().len(), 2);

    // A boys' hostel takes boys only; a girls' hostel takes girls only; mixed takes anyone.
    let (s, hina) = app
        .post(
            "/students",
            &admin,
            json!({ "full_name": "Hina Baig", "father_name": "Father", "date_of_birth": "2015-01-01", "gender": "female",
                    "admission_date": "2026-04-01", "section_id": section_id, "roll_no": 9 }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{hina}");
    let hina = id(&hina["id"]);
    let (s, r3) = app
        .post(
            &format!("/hostel/hostels/{hostel_id}/rooms"),
            &admin,
            json!({ "floor": "Ground", "name": "G-2", "beds": 2 }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{r3}");
    let r3 = id(&r3["id"]);
    let (s, b) = app
        .post(
            "/hostel/allocations",
            &admin,
            json!({ "student_id": hina, "academic_year_id": year_id, "room_id": r3, "bed_no": 1, "from_date": "2026-04-01" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "girl in a boys' hostel: {b}");
    assert!(
        b["error"]["message"]
            .as_str()
            .unwrap()
            .contains("boys' hostel"),
        "{b}"
    );
    // The empty room goes again, so the board below counts the three original beds.
    let (s, _) = app.delete(&format!("/hostel/rooms/{r3}"), &admin).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, girls) = app
        .post(
            "/hostel/hostels",
            &admin,
            json!({ "name": "Fatima House", "kind": "girls", "warden_name": "Ms Rubina", "monthly_fee": "8000" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{girls}");
    let (s, gr) = app
        .post(
            &format!("/hostel/hostels/{}/rooms", id(&girls["id"])),
            &admin,
            json!({ "floor": "Ground", "name": "G-1", "beds": 2 }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{gr}");
    let gr = id(&gr["id"]);
    let (s, b) = app
        .post(
            "/hostel/allocations",
            &admin,
            json!({ "student_id": hina, "academic_year_id": year_id, "room_id": gr, "bed_no": 1, "from_date": "2026-04-01" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let (s, b) = app
        .post(
            "/hostel/allocations",
            &admin,
            json!({ "student_id": omar, "academic_year_id": year_id, "room_id": gr, "bed_no": 2, "from_date": "2026-04-01" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "boy in a girls' hostel: {b}");
    assert!(
        b["error"]["message"]
            .as_str()
            .unwrap()
            .contains("girls' hostel"),
        "{b}"
    );

    // Back on Lite the module is refused.
    let (s, _) = app
        .put("/settings/edition", &admin, json!({ "edition": "lite" }))
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app.get("/hostel/hostels", &admin).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    app.finish().await;
}
