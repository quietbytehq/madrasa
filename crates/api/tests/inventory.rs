//! Standard stage 7: inventory. Lite refuses the module and the store keeper role;
//! Standard runs categories, locations, items, movements that never take stock negative,
//! transfers, issues to staff and rooms, the reorder report, asset tags and the reports.
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
                "school_name": "Gulshan High", "school_code": "KHI-IN", "academic_year_start_month": 4,
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
async fn lite_refuses_inventory_and_the_store_keeper_role() {
    let app = app!();
    let admin = setup_school(&app).await;
    for path in [
        "/inventory/items",
        "/inventory/locations",
        "/inventory/movements",
    ] {
        let (s, b) = app.get(path, &admin).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{path}: {b}");
        assert_eq!(b["error"]["code"], "feature_unavailable");
    }
    let (_, staff) = app
        .post("/staff", &admin, json!({ "full_name": "Bilal Store" }))
        .await;
    let (s, b) = app
        .post(
            &format!("/staff/{}/login", id(&staff["id"])),
            &admin,
            json!({ "username": "bilal", "password": "store-password-1", "role": "storekeeper" }),
        )
        .await;
    assert!(!s.is_success(), "Lite has no store keeper role: {b}");
    let (s, b) = app.get("/reports", &admin).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(!b
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["module"] == "inventory"));
    app.finish().await;
}

#[tokio::test]
async fn standard_runs_items_movements_assets_and_reports() {
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
    let ali = student(&app, &admin, "Ali Khan", &id(&section["id"]), 1).await;
    // The store keeper runs the store.
    let (_, bilal) = app
        .post(
            "/staff",
            &admin,
            json!({ "full_name": "Bilal Store", "designation": "Store keeper" }),
        )
        .await;
    let bilal_id = id(&bilal["id"]);
    let (s, b) = app
        .post(
            &format!("/staff/{bilal_id}/login"),
            &admin,
            json!({ "username": "bilal", "password": "store-password-1", "role": "storekeeper" }),
        )
        .await;
    assert!(s.is_success(), "{b}");
    let store = app.login("bilal", "store-password-1").await;

    // Setup: a category, two locations.
    let (s, cat) = app
        .post(
            "/inventory/categories",
            &store,
            json!({ "name": "Stationery" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{cat}");
    let (s, main) = app
        .post(
            "/inventory/locations",
            &store,
            json!({ "name": "Main store", "kind": "store" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{main}");
    let main = id(&main["id"]);
    let (s, lab) = app
        .post(
            "/inventory/locations",
            &store,
            json!({ "name": "Science lab", "kind": "lab" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{lab}");
    let lab = id(&lab["id"]);
    let (s, b) = app
        .post(
            "/inventory/locations",
            &store,
            json!({ "name": "main STORE" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");

    // Items.
    let (s, b) = app
        .post("/inventory/items", &store, json!({ "name": "" }))
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    let (s, chalk) = app
        .post(
            "/inventory/items",
            &store,
            json!({ "name": "Chalk (box)", "code": "CH-1", "unit": "box", "category_id": cat["id"], "reorder_level": "10" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{chalk}");
    let chalk = id(&chalk["id"]);
    let (s, proj) = app
        .post(
            "/inventory/items",
            &store,
            json!({ "name": "Projector", "code": "PJ", "unit": "pcs", "is_asset": true }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{proj}");
    let proj = id(&proj["id"]);

    // Stock never goes negative: an issue before any purchase is refused.
    let (s, b) = app
        .post(
            "/inventory/movements",
            &store,
            json!({ "item_id": chalk, "location_id": main, "kind": "issue", "quantity": "1",
                    "recipient_kind": "staff", "recipient_id": bilal_id, "recipient_name": "Bilal Store" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    let (s, b) = app
        .post(
            "/inventory/movements",
            &store,
            json!({ "item_id": chalk, "location_id": main, "kind": "purchase_in", "quantity": "50", "unit_cost": "120", "reference": "Bill 17", "moved_on": "2026-04-02" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(num(&b["on_hand"]), 50.0);
    assert_eq!(num(&b["movement"]["total_cost"]), 6000.0);
    assert!(!b["below_reorder"].as_bool().unwrap());
    // An issue needs a recipient.
    let (s, b) = app
        .post(
            "/inventory/movements",
            &store,
            json!({ "item_id": chalk, "location_id": main, "kind": "issue", "quantity": "5" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    let (_, who) = app.get("/inventory/recipients?q=ali", &store).await;
    assert!(
        who.as_array()
            .unwrap()
            .iter()
            .any(|r| r["kind"] == "student" && r["id"] == ali),
        "{who}"
    );
    let (s, b) = app
        .post(
            "/inventory/movements",
            &store,
            json!({ "item_id": chalk, "location_id": main, "kind": "issue", "quantity": "42", "moved_on": "2026-04-05",
                    "recipient_kind": "staff", "recipient_id": bilal_id, "recipient_name": "Bilal Store" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(num(&b["on_hand"]), 8.0);
    assert!(
        b["below_reorder"].as_bool().unwrap(),
        "8 boxes is under the level of 10"
    );
    // Transfer: not more than there is; the two halves share an id.
    let (s, b) = app
        .post(
            "/inventory/transfers",
            &store,
            json!({ "item_id": chalk, "from_location_id": main, "to_location_id": lab, "quantity": "9" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    let (s, b) = app
        .post(
            "/inventory/transfers",
            &store,
            json!({ "item_id": chalk, "from_location_id": main, "to_location_id": lab, "quantity": "3", "moved_on": "2026-04-06" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(b[0]["transfer_id"], b[1]["transfer_id"]);
    let (_, stock) = app.get("/inventory/stock", &store).await;
    let rows = stock.as_array().unwrap();
    let at = |loc: &str| {
        rows.iter()
            .find(|r| r["location_id"] == loc)
            .map(|r| num(&r["on_hand"]))
    };
    assert_eq!(at(&main), Some(5.0));
    assert_eq!(at(&lab), Some(3.0));
    // A return brings stock back; a write-off removes it.
    let (s, b) = app
        .post(
            "/inventory/movements",
            &store,
            json!({ "item_id": chalk, "location_id": main, "kind": "return", "quantity": "2", "recipient_kind": "staff", "recipient_id": bilal_id, "recipient_name": "Bilal Store" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(num(&b["on_hand"]), 10.0);
    let (s, b) = app
        .post(
            "/inventory/movements",
            &store,
            json!({ "item_id": chalk, "location_id": lab, "kind": "write_off", "quantity": "1", "notes": "broken" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(num(&b["on_hand"]), 9.0);
    // The item detail shows stock by location and the register.
    let (s, d) = app.get(&format!("/inventory/items/{chalk}"), &store).await;
    assert_eq!(s, StatusCode::OK, "{d}");
    assert_eq!(num(&d["on_hand"]), 9.0);
    assert_eq!(d["by_location"].as_array().unwrap().len(), 2);
    assert_eq!(d["recent"].as_array().unwrap().len(), 6);
    let (_, below) = app.get("/inventory/items?below_reorder=true", &store).await;
    assert_eq!(below.as_array().unwrap().len(), 1);
    assert_eq!(below[0]["name"], "Chalk (box)");
    // Removing the purchase now would take the store negative.
    let purchase = d["recent"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["kind"] == "purchase_in")
        .unwrap();
    let (s, b) = app
        .delete(
            &format!("/inventory/movements/{}", id(&purchase["id"])),
            &store,
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    // A location with stock cannot go; an item with stock cannot go.
    let (s, _) = app
        .delete(&format!("/inventory/locations/{lab}"), &store)
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = app
        .delete(&format!("/inventory/items/{chalk}"), &store)
        .await;
    assert_eq!(s, StatusCode::CONFLICT);

    // Assets: tags are unique; assign to a room.
    let (s, b) = app
        .post(
            "/inventory/assets",
            &store,
            json!({ "item_id": proj, "tag": "PJ-001", "serial_no": "SN1", "location_id": lab, "status": "in_use",
                    "assigned_kind": "room", "assigned_name": "Lab 1", "purchased_on": "2025-09-01", "cost": "85000" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let asset_id = id(&b["id"]);
    let (s, b) = app
        .post(
            "/inventory/assets",
            &store,
            json!({ "item_id": proj, "tag": "pj-001" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    let (s, b) = app
        .put(
            &format!("/inventory/assets/{asset_id}"),
            &store,
            json!({ "item_id": proj, "tag": "PJ-001", "status": "under_repair", "location_id": main }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["status"], "under_repair");
    assert_eq!(b["location_name"], "Main store");
    let (_, assets) = app.get("/inventory/assets?q=pj", &store).await;
    assert_eq!(assets.as_array().unwrap().len(), 1);
    let (_, d) = app.get(&format!("/inventory/items/{proj}"), &store).await;
    assert_eq!(d["assets"], 1);

    // Reports (the accountant reads, the teacher cannot).
    for path in [
        "/reports/inventory.stock?format=pdf",
        "/reports/inventory.reorder?format=pdf&lang=ur",
        &format!("/reports/inventory.movements?academic_year_id={year_id}&from=2026-04-01&to=2026-04-30&format=pdf"),
    ] {
        let (s, ctype, bytes) = app.get_bytes(path, &store).await;
        assert_eq!(s, StatusCode::OK, "{path}");
        assert_eq!(ctype, "application/pdf");
        assert!(bytes.starts_with(b"%PDF"));
    }
    let (s, mv) = app
        .get(
            &format!("/reports/inventory.movements?academic_year_id={year_id}&from=2026-04-01&to=2026-04-30"),
            &store,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{mv}");
    let row = &mv["groups"][0]["rows"][0];
    assert_eq!(row[0], "Chalk (box)");
    assert_eq!(row[2], "50");
    assert_eq!(row[3], "42");
    let (s, reorder) = app.get("/reports/inventory.reorder", &store).await;
    assert_eq!(s, StatusCode::OK, "{reorder}");
    assert_eq!(
        reorder["groups"][0]["rows"][0][4], "1",
        "shortfall of one box"
    );

    // Back on Lite the module is refused.
    let (s, _) = app
        .put("/settings/edition", &admin, json!({ "edition": "lite" }))
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app.get("/inventory/items", &store).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    app.finish().await;
}
