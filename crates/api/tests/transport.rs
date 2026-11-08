//! Standard stage 5: transport. Lite refuses the module; Standard runs vehicles, routes
//! with stops, assignments with capacity warnings, rosters, the monthly charge on the fee
//! voucher and the reports. Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

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
                "school_name": "Gulshan High", "school_code": "KHI-TR", "academic_year_start_month": 4,
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
async fn lite_refuses_transport() {
    let app = app!();
    let admin = setup_school(&app).await;
    for path in [
        "/transport/routes",
        "/transport/vehicles",
        "/transport/students",
    ] {
        let (s, b) = app.get(path, &admin).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{path}: {b}");
        assert_eq!(b["error"]["code"], "feature_unavailable");
    }
    let (s, b) = app.get("/reports", &admin).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(!b
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["module"] == "transport"));
    app.finish().await;
}

#[tokio::test]
async fn standard_runs_routes_assignments_fees_and_reports() {
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
    let class_id = id(&class["id"]);
    let (_, section) = app
        .post(
            "/sections",
            &admin,
            json!({ "class_id": class_id, "academic_year_id": year_id, "name": "A" }),
        )
        .await;
    let section_id = id(&section["id"]);
    let ali = student(&app, &admin, "Ali Khan", &section_id, 1).await;
    let sara = student(&app, &admin, "Sara Noor", &section_id, 2).await;
    let omar = student(&app, &admin, "Omar Shah", &section_id, 3).await;

    // Vehicles: a registration number is required and unique.
    let (s, b) = app
        .post(
            "/transport/vehicles",
            &admin,
            json!({ "registration_no": "" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    let (s, van) = app
        .post(
            "/transport/vehicles",
            &admin,
            json!({ "registration_no": "KHI-1234", "make_model": "Hiace", "capacity": 2, "driver_name": "Rashid", "driver_phone": "0300 1111111" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{van}");
    let van_id = id(&van["id"]);
    let (s, b) = app
        .post(
            "/transport/vehicles",
            &admin,
            json!({ "registration_no": "khi-1234" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");

    // A route with ordered stops and the van.
    let (s, route) = app
        .post(
            "/transport/routes",
            &admin,
            json!({ "name": "North", "code": "N1", "vehicle_id": van_id, "monthly_fee": "1500", "active": true, "notes": "",
                    "stops": [
                        { "name": "Gulshan Chowrangi", "pickup_time": "07:10:00", "monthly_fee": null },
                        { "name": "NIPA", "pickup_time": "07:25:00", "monthly_fee": "1200" }
                    ] }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{route}");
    let route_id = id(&route["id"]);
    assert_eq!(route["stops"], 2);
    assert_eq!(route["vehicle_registration"], "KHI-1234");
    let stops = route["stop_list"].as_array().unwrap();
    let gulshan = id(&stops[0]["id"]);
    let nipa = id(&stops[1]["id"]);
    assert_eq!(stops[1]["position"], 1);
    // The van cannot go while the route uses it.
    let (s, _) = app
        .delete(&format!("/transport/vehicles/{van_id}"), &admin)
        .await;
    assert_eq!(s, StatusCode::CONFLICT);

    // Assign Ali to Gulshan (route fee), Sara to NIPA (stop fee), Omar at his own amount.
    let (s, a1) = app
        .post(
            "/transport/assignments",
            &admin,
            json!({ "student_id": ali, "academic_year_id": year_id, "stop_id": gulshan, "from_date": "2026-04-01" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{a1}");
    assert_eq!(num(&a1["assignment"]["charge"]), 1500.0);
    assert_eq!(a1["route_students"], 1);
    assert_eq!(a1["seats_left"], 1);
    assert!(a1["warning"].is_null());
    let (s, b) = app
        .post(
            "/transport/assignments/bulk",
            &admin,
            json!({ "student_ids": [sara, omar, ali], "academic_year_id": year_id, "stop_id": nipa, "from_date": "2026-04-01" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    // Ali moved stops (his old assignment had not started later, so it was replaced).
    assert_eq!(b["assigned"], 3);
    assert_eq!(b["route_students"], 3);
    assert_eq!(b["warning"]["kind"], "over");
    assert_eq!(b["warning"]["by"], 1);
    let (_, students) = app
        .get(
            &format!("/transport/students?academic_year_id={year_id}&section_id={section_id}"),
            &admin,
        )
        .await;
    let rows = students.as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|r| r["stop_name"] == "NIPA"));
    let omar_row = rows.iter().find(|r| r["student_id"] == omar).unwrap();
    let omar_assignment = id(&omar_row["assignment_id"]);
    let (s, b) = app
        .put(
            &format!("/transport/assignments/{omar_assignment}"),
            &admin,
            json!({ "amount": "900" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(num(&b["charge"]), 900.0);
    // Ali goes back to Gulshan from May: the NIPA assignment ends on 30 April.
    let (s, a2) = app
        .post(
            "/transport/assignments",
            &admin,
            json!({ "student_id": ali, "academic_year_id": year_id, "stop_id": gulshan, "from_date": "2026-05-01" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{a2}");
    let (_, history) = app
        .get(
            &format!("/transport/assignments?student_id={ali}&academic_year_id={year_id}"),
            &admin,
        )
        .await;
    let h = history.as_array().unwrap();
    assert_eq!(h.len(), 2);
    let ended = h.iter().find(|a| a["stop_name"] == "NIPA").unwrap();
    assert_eq!(ended["to_date"], "2026-04-30");
    // Nobody is without transport; a fourth student is.
    let (_, none) = app
        .get(
            &format!("/transport/students?academic_year_id={year_id}&without=true"),
            &admin,
        )
        .await;
    assert_eq!(none.as_array().unwrap().len(), 0);
    let zain = student(&app, &admin, "Zain Ahmed", &section_id, 4).await;
    let (_, none) = app
        .get(
            &format!("/transport/students?academic_year_id={year_id}&without=true"),
            &admin,
        )
        .await;
    assert_eq!(none.as_array().unwrap()[0]["student_id"], zain);

    // Roster by stop with the capacity warning.
    let (s, roster) = app
        .get(
            &format!("/transport/routes/{route_id}/roster?academic_year_id={year_id}"),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{roster}");
    assert_eq!(roster["total"], 3);
    assert_eq!(roster["warning"]["kind"], "over");
    let rs = roster["by_stop"].as_array().unwrap();
    assert_eq!(rs[0]["stop"]["name"], "Gulshan Chowrangi");
    assert_eq!(rs[0]["students"].as_array().unwrap().len(), 1);
    assert_eq!(rs[1]["students"].as_array().unwrap().len(), 2);
    // A stop with students cannot be dropped from the route; renaming keeps it.
    let (s, b) = app
        .put(
            &format!("/transport/routes/{route_id}"),
            &admin,
            json!({ "name": "North", "code": "N1", "vehicle_id": van_id, "monthly_fee": "1500", "active": true, "notes": "",
                    "stops": [{ "id": gulshan, "name": "Gulshan", "pickup_time": "07:05:00", "monthly_fee": null }] }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    let (s, b) = app
        .put(
            &format!("/transport/routes/{route_id}"),
            &admin,
            json!({ "name": "North", "code": "N1", "vehicle_id": van_id, "monthly_fee": "1500", "active": true, "notes": "",
                    "stops": [
                        { "id": nipa, "name": "NIPA", "pickup_time": "07:00:00", "monthly_fee": "1200" },
                        { "id": gulshan, "name": "Gulshan", "pickup_time": "07:15:00", "monthly_fee": null },
                        { "name": "Sohrab Goth", "pickup_time": "07:30:00", "monthly_fee": null }
                    ] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["stop_list"][0]["name"], "NIPA");
    assert_eq!(b["stop_list"][1]["name"], "Gulshan");
    assert_eq!(b["stops"], 3);

    // The monthly charge joins the voucher once a fee head is chosen.
    let (s, head) = app
        .post(
            "/fees/heads",
            &admin,
            json!({ "name": "Transport", "code": "TR", "is_recurring": true }),
        )
        .await;
    assert!(s.is_success(), "{head}");
    let (s, b) = app
        .put(
            "/transport/settings",
            &admin,
            json!({ "fee_head_id": head["id"] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["fee_head_name"], "Transport");
    let (s, gen) = app
        .post(
            "/fees/vouchers/generate",
            &admin,
            json!({ "month": "2026-05-01", "academic_year_id": year_id }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{gen}");
    assert_eq!(
        gen["created"], 3,
        "Zain has no structure and no transport: {gen}"
    );
    let (_, vouchers) = app
        .get(&format!("/fees/vouchers?student_id={omar}"), &admin)
        .await;
    let v = &vouchers.as_array().unwrap()[0];
    let (_, detail) = app
        .get(&format!("/fees/vouchers/{}", id(&v["id"])), &admin)
        .await;
    let lines = detail["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 1, "{detail}");
    assert_eq!(num(&lines[0]["amount"]), 900.0);
    assert_eq!(num(&v["total_due"]), 900.0);
    let (_, vouchers) = app
        .get(&format!("/fees/vouchers?student_id={ali}"), &admin)
        .await;
    assert_eq!(num(&vouchers.as_array().unwrap()[0]["total_due"]), 1500.0);

    // Reports.
    for path in [
        &format!("/reports/transport.manifest?academic_year_id={year_id}&format=pdf"),
        &format!("/reports/transport.without_stop?academic_year_id={year_id}&format=pdf&lang=ur"),
        &format!(
            "/reports/transport.fee_by_route?academic_year_id={year_id}&month=2026-05&format=pdf"
        ),
    ] {
        let (s, ctype, bytes) = app.get_bytes(path, &admin).await;
        assert_eq!(s, StatusCode::OK, "{path}");
        assert_eq!(ctype, "application/pdf");
        assert!(bytes.starts_with(b"%PDF"));
    }
    let (s, fee) = app
        .get(
            &format!("/reports/transport.fee_by_route?academic_year_id={year_id}&month=2026-05"),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{fee}");
    let row = &fee["groups"][0]["rows"][0];
    assert_eq!(row[0], "North");
    assert_eq!(row[2], "3");

    // Back on Lite the module is refused.
    let (s, _) = app
        .put("/settings/edition", &admin, json!({ "edition": "lite" }))
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app.get("/transport/routes", &admin).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    app.finish().await;
}
