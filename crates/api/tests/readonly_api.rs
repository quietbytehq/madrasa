//! Standard stage 9: the read-only API. Lite has no `/api/ro`; Standard issues keys with
//! scopes, an IP allow-list and an expiry, serves the endpoints with ETags, enforces scopes,
//! the allow-list, revocation and the rate limit, and logs every call. Needs PostgreSQL via
//! `MADRASA_TEST_DATABASE_URL`.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use madrasa_api::{router, AppState};
use madrasa_db::testing::TestDb;
use serde_json::{json, Value};
use std::net::SocketAddr;
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
    /// A read-only API call with a key, from a given address, with extra headers.
    async fn ro(
        &self,
        path: &str,
        key: Option<&str>,
        from: &str,
        extra: &[(&str, &str)],
    ) -> (StatusCode, HeaderMap, Value) {
        let mut req = Request::builder()
            .method(Method::GET)
            .uri(format!("/api/ro/v1{path}"));
        if let Some(k) = key {
            req = req.header(header::AUTHORIZATION, format!("Bearer {k}"));
        }
        for (n, v) in extra {
            req = req.header(*n, *v);
        }
        let mut req = req.body(Body::empty()).unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(from.parse::<SocketAddr>().unwrap()));
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        (status, headers, value)
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
                "school_name": "Gulshan High", "school_code": "KHI-RO", "academic_year_start_month": 4,
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
async fn lite_has_no_readonly_api() {
    let app = app!();
    let admin = setup_school(&app).await;
    let (s, b) = app.get("/settings/api-keys", &admin).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    assert_eq!(b["error"]["code"], "feature_unavailable");
    let (s, _, _) = app.ro("/students", None, "127.0.0.1:5000", &[]).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "no /api/ro on Lite");
    let (s, _, _) = app.ro("/openapi.json", None, "127.0.0.1:5000", &[]).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    app.finish().await;
}

#[tokio::test]
async fn standard_serves_the_readonly_api_with_keys() {
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
    let _sara = student(&app, &admin, "Sara Noor", &section_id, 2).await;

    // Keys: validation, creation shows the secret once, the list hides it.
    let (s, b) = app
        .post(
            "/settings/api-keys",
            &admin,
            json!({ "name": "Portal", "scopes": [] }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    let (s, b) = app
        .post(
            "/settings/api-keys",
            &admin,
            json!({ "name": "Portal", "scopes": ["students"], "ip_allow": ["school.local"] }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");
    let (s, created) = app
        .post(
            "/settings/api-keys",
            &admin,
            json!({ "name": "Portal", "scopes": ["students", "attendance", "fees", "timetable"], "ip_allow": ["10.0.0.0/8", "127.0.0.1"] }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{created}");
    let key = id(&created["key"]);
    assert!(key.starts_with("mk_"), "{key}");
    let key_id = id(&created["id"]);
    let (s, list) = app.get("/settings/api-keys", &admin).await;
    assert_eq!(s, StatusCode::OK, "{list}");
    assert_eq!(list["keys"].as_array().unwrap().len(), 1);
    assert!(list["keys"][0].get("key").is_none() && list["keys"][0].get("key_hash").is_none());
    assert_eq!(list["keys"][0]["key_prefix"], &key[..11]);
    assert_eq!(list["base_path"], "/api/ro/v1");
    assert_eq!(list["rate_limit_per_minute"], 120);

    // Calls: no key, wrong key, wrong address, then a good one with an ETag.
    let (s, _, _) = app.ro("/students", None, "127.0.0.1:5000", &[]).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _, _) = app
        .ro("/students", Some("mk_nope"), "127.0.0.1:5000", &[])
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _, b) = app
        .ro("/students", Some(&key), "192.168.5.5:5000", &[])
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    assert_eq!(b["error"]["code"], "ip_not_allowed");
    let (s, h, b) = app
        .ro("/students", Some(&key), "10.20.30.40:5000", &[])
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["total"], 2);
    assert_eq!(b["items"][0]["full_name"], "Ali Khan");
    assert_eq!(b["items"][0]["section_name"], "A");
    let etag = h.get(header::ETAG).unwrap().to_str().unwrap().to_string();
    assert!(etag.starts_with("W/\""));
    let (s, _, _) = app
        .ro(
            "/students",
            Some(&key),
            "127.0.0.1:1",
            &[("If-None-Match", &etag)],
        )
        .await;
    assert_eq!(s, StatusCode::NOT_MODIFIED);
    // The X-Api-Key header works too; paging and filters.
    let (s, _, b) = app
        .ro(
            "/students?page_size=1&page=2",
            None,
            "127.0.0.1:1",
            &[("X-Api-Key", &key)],
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["items"].as_array().unwrap().len(), 1);
    assert_eq!(b["items"][0]["full_name"], "Sara Noor");
    let (s, _, b) = app.ro("/sections", Some(&key), "127.0.0.1:1", &[]).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b[0]["students"], 2);
    // Scopes: exams is not on this key.
    let (s, _, b) = app
        .ro("/exams/assessments", Some(&key), "127.0.0.1:1", &[])
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    assert_eq!(b["error"]["code"], "scope_missing");
    // Attendance for a day, fee balances, the timetable (not published yet).
    let (s, _, b) = app
        .ro(
            "/attendance/daily?date=2026-04-06",
            Some(&key),
            "127.0.0.1:1",
            &[],
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b[0]["marked"], false);
    let (s, _, b) = app
        .ro("/fees/balances", Some(&key), "127.0.0.1:1", &[])
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b.as_array().unwrap().len(), 0);
    let (s, _, b) = app
        .ro(
            &format!("/timetable?section_id={section_id}"),
            Some(&key),
            "127.0.0.1:1",
            &[],
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["published"], false);
    let (s, _, b) = app.ro("", Some(&key), "127.0.0.1:1", &[]).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["key"], "Portal");
    let (s, _, b) = app
        .ro("/openapi.json", Some(&key), "127.0.0.1:1", &[])
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(b["paths"]["/students"]["get"].is_object());
    // Every call was logged with the key.
    let (s, calls) = app
        .get(&format!("/settings/api-keys/{key_id}/calls"), &admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{calls}");
    let calls = calls.as_array().unwrap();
    assert!(calls.len() >= 10, "{}", calls.len());
    assert!(calls
        .iter()
        .any(|c| c["path"] == "/api/ro/v1/students" && c["status"] == 304));
    assert!(calls
        .iter()
        .any(|c| c["status"] == 403 && c["ip"] == "192.168.5.5"));
    assert!(
        !calls.iter().any(|c| c["status"] == 401),
        "unknown keys are not logged against this key"
    );

    // Rate limit: the 121st call in a minute is refused with Retry-After.
    let mut last = StatusCode::OK;
    let mut retry_after = None;
    for _ in 0..130 {
        let (s, h, _) = app.ro("/sections", Some(&key), "127.0.0.1:1", &[]).await;
        last = s;
        if s == StatusCode::TOO_MANY_REQUESTS {
            retry_after = h
                .get(header::RETRY_AFTER)
                .map(|v| v.to_str().unwrap().to_string());
            break;
        }
    }
    assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
    assert!(retry_after.is_some());

    // Expiry and revocation.
    let (s, expired) = app
        .post(
            "/settings/api-keys",
            &admin,
            json!({ "name": "Old", "scopes": ["students"], "expires_at": "2020-01-01T00:00:00Z" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{expired}");
    let (s, _, _) = app
        .ro("/sections", Some(&id(&expired["key"])), "127.0.0.1:1", &[])
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, b) = app
        .delete(&format!("/settings/api-keys/{key_id}"), &admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(b["revoked_at"].is_string());
    let (s, _, _) = app.ro("/sections", Some(&key), "127.0.0.1:1", &[]).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "revoked");
    // A key without an allow-list works from anywhere; results need exams.
    let (_, any) = app
        .post(
            "/settings/api-keys",
            &admin,
            json!({ "name": "BI", "scopes": ["exams", "library"] }),
        )
        .await;
    let any_key = id(&any["key"]);
    let (s, _, b) = app
        .ro("/exams/assessments", Some(&any_key), "203.0.113.9:1", &[])
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, _, b) = app
        .ro(
            &format!(
                "/exams/results?assessment_id={}&section_id={section_id}",
                ali
            ),
            Some(&any_key),
            "203.0.113.9:1",
            &[],
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b.as_array().unwrap().len(), 0);
    let (s, _, b) = app
        .ro("/library/loans", Some(&any_key), "203.0.113.9:1", &[])
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");

    // Back on Lite the prefix disappears.
    let (s, _) = app
        .put("/settings/edition", &admin, json!({ "edition": "lite" }))
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _, _) = app
        .ro("/sections", Some(&any_key), "127.0.0.1:1", &[])
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    app.finish().await;
}
