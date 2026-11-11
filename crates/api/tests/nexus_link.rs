//! Nexus stage 4, end to end: a school's API joins a real hub over HTTP, uploads, syncs both
//! ways, sees what head office overwrote, copes with the hub being away, is told when it is
//! removed, and leaves with its data intact. Needs PostgreSQL via
//! `MADRASA_TEST_DATABASE_URL`.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use madrasa_api::{router, AppState};
use madrasa_db::testing::TestDb;
use madrasa_nexus::{tenants, Config, HubState};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

/// A school install: the API router on its own database.
struct App {
    db: TestDb,
    state: AppState,
    app: Router,
}

impl App {
    async fn new() -> Option<App> {
        let db = TestDb::new().await?;
        let state = AppState::new(db.pool.clone()).await.unwrap();
        let app = router(state.clone());
        Some(App { db, state, app })
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
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
    async fn get(&self, p: &str, t: &str) -> (StatusCode, Value) {
        self.call(Method::GET, p, Some(t), None).await
    }
    async fn post(&self, p: &str, t: &str, b: Value) -> (StatusCode, Value) {
        self.call(Method::POST, p, Some(t), Some(b)).await
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

    /// Set the school up with one section and two students; returns the admin's token.
    async fn setup(&self, code: &str) -> String {
        let (s, b) = self
            .call(
                Method::POST,
                "/setup",
                None,
                Some(json!({
                    "school_name": "Gulshan High", "school_code": code, "academic_year_start_month": 4,
                    "admin_full_name": "Ayesha", "admin_username": "ayesha", "admin_password": "admin-password-1",
                    "backup_password": "backup-password-1"
                })),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        let admin = self.login("ayesha", "admin-password-1").await;
        let (_, year) = self
            .post(
                "/academic-years",
                &admin,
                json!({ "start_year": 2026, "start_month": 4, "activate": true }),
            )
            .await;
        let (_, class) = self
            .post("/classes", &admin, json!({ "name": "5", "level": 5 }))
            .await;
        let (_, section) = self
            .post(
                "/sections",
                &admin,
                json!({ "class_id": class["id"], "academic_year_id": year["id"], "name": "A" }),
            )
            .await;
        for (i, name) in ["Ali Khan", "Sara Noor"].iter().enumerate() {
            let (s, b) = self
                .post(
                    "/students",
                    &admin,
                    json!({ "full_name": name, "father_name": "Father", "date_of_birth": "2015-01-01", "gender": "male",
                            "admission_date": "2026-04-01", "section_id": section["id"], "roll_no": i + 1 }),
                )
                .await;
            assert_eq!(s, StatusCode::CREATED, "{b}");
        }
        admin
    }

    async fn student_id(&self, name: &str) -> Uuid {
        sqlx::query_scalar("SELECT id FROM students WHERE full_name = $1")
            .bind(name)
            .fetch_one(&self.db.pool)
            .await
            .unwrap()
    }

    async fn names(&self) -> Vec<String> {
        sqlx::query_scalar("SELECT full_name FROM students ORDER BY full_name")
            .fetch_all(&self.db.pool)
            .await
            .unwrap()
    }

    async fn triggers(&self) -> i64 {
        let mut c = self.db.pool.acquire().await.unwrap();
        madrasa_db::sync::triggers_installed(&mut c).await.unwrap()
    }

    async fn rename(&self, admin: &str, from: &str, to: &str) {
        let id = self.student_id(from).await;
        let (s, b) = self
            .call(
                Method::PATCH,
                &format!("/students/{id}"),
                Some(admin),
                Some(json!({ "full_name": to })),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
    }

    async fn finish(self) {
        self.db.cleanup().await;
    }
}

/// A real hub listening on a loopback port.
struct LiveHub {
    db: TestDb,
    state: HubState,
    url: String,
    http: reqwest::Client,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
}

const HUB_ADMIN_PASSWORD: &str = "admin-password-1";

impl LiveHub {
    async fn start() -> Option<LiveHub> {
        let db = TestDb::empty().await?;
        let pool = madrasa_nexus::store::connect(&db.url).await.unwrap();
        madrasa_nexus::store::migrate(&pool).await.unwrap();
        let config = Config {
            database_url: db.url.clone(),
            bind: "127.0.0.1:0".parse().unwrap(),
            public_url: None,
            setup_token: Some("test-setup-token".into()),
            console_dir: None,
        };
        let (state, _) = HubState::new(pool, &config).await.unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let (addr, serve) = madrasa_nexus::server::bind(config.bind, state.clone(), async {
            let _ = rx.await;
        })
        .await
        .unwrap();
        tokio::spawn(serve);
        madrasa_api::lan::init();
        Some(LiveHub {
            db,
            state,
            url: format!("http://{addr}"),
            http: reqwest::Client::new(),
            stop: Some(tx),
        })
    }

    async fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (u16, Value) {
        let mut req = self
            .http
            .request(method, format!("{}/api/nexus/v1{path}", self.url));
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        if let Some(b) = body {
            req = req.json(&b);
        }
        let res = req.send().await.unwrap();
        let status = res.status().as_u16();
        (status, res.json().await.unwrap_or(Value::Null))
    }

    /// First-run setup; returns the hub administrator's token.
    async fn setup(&self) -> String {
        let (s, b) = self
            .call(
                reqwest::Method::POST,
                "/setup",
                None,
                Some(json!({ "setup_token": "test-setup-token", "hub_name": "Gulshan Education Network",
                             "admin_username": "head.office", "admin_full_name": "Head Office", "admin_password": HUB_ADMIN_PASSWORD })),
            )
            .await;
        assert_eq!(s, 200, "{b}");
        let (s, b) = self
            .call(
                reqwest::Method::POST,
                "/auth/login",
                None,
                Some(json!({ "username": "head.office", "password": HUB_ADMIN_PASSWORD })),
            )
            .await;
        assert_eq!(s, 200, "{b}");
        b["token"].as_str().unwrap().to_string()
    }

    async fn invite(&self, admin: &str) -> String {
        let (s, b) = self
            .call(
                reqwest::Method::POST,
                "/invitations",
                Some(admin),
                Some(json!({ "school_name": "Gulshan High" })),
            )
            .await;
        assert_eq!(s, 201, "{b}");
        b["join_code"].as_str().unwrap().to_string()
    }

    /// The hub's register row of the only school.
    async fn school(&self, admin: &str) -> Value {
        let (s, b) = self
            .call(reqwest::Method::GET, "/schools", Some(admin), None)
            .await;
        assert_eq!(s, 200, "{b}");
        b[0].clone()
    }

    /// Run SQL in the only school's schema, as head office would edit its copy.
    async fn in_school(&self, sql: &'static str) -> Vec<String> {
        let mut conn = self.state.pool.acquire().await.unwrap();
        let school = tenants::list(&mut conn).await.unwrap().remove(0);
        drop(conn);
        let mut tx = tenants::begin(&self.state.pool, &school).await.unwrap();
        let rows = sqlx::query_scalar::<_, String>(sql)
            .fetch_all(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        rows
    }

    /// Stop listening (the hub is "away"); the database stays.
    fn stop(&mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
    }

    async fn finish(mut self) {
        self.stop();
        self.state.pool.close().await;
        self.db.cleanup().await;
    }
}

macro_rules! start {
    () => {{
        let (Some(app), Some(hub)) = (App::new().await, LiveHub::start().await) else {
            return;
        };
        (app, hub)
    }};
}

#[tokio::test]
async fn a_school_joins_syncs_both_ways_and_leaves_with_its_data() {
    let (app, hub) = start!();
    let admin = app.setup("KHI-NX").await;
    let hub_admin = hub.setup().await;

    // Before joining: the feature is there on Lite, nothing is switched on, nothing queued.
    let (_, features) = app.call(Method::GET, "/system/features", None, None).await;
    assert_eq!(features["edition"], "lite");
    assert!(features["features"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f == "nexus_link"));
    let (s, st) = app.get("/nexus/link", &admin).await;
    assert_eq!(s, StatusCode::OK, "{st}");
    assert_eq!(
        (
            st["linked"].as_bool(),
            st["pending"].as_i64(),
            st["hub_url"].is_null()
        ),
        (Some(false), Some(0), true)
    );
    assert_eq!(
        app.triggers().await,
        0,
        "an unjoined school has no sync triggers"
    );
    let (s, e) = app.post("/nexus/link/sync", &admin, json!({})).await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("conflict")),
        "{e}"
    );

    // Joining needs a proper address, a hub that answers and a valid code.
    let code = hub.invite(&hub_admin).await;
    for bad in ["http://nexus.example.org", "nexus.example.org", ""] {
        let (s, e) = app
            .post(
                "/nexus/link/join",
                &admin,
                json!({ "hub_url": bad, "join_code": code }),
            )
            .await;
        assert_eq!(
            (s, e["error"]["code"].as_str()),
            (StatusCode::BAD_REQUEST, Some("validation")),
            "{bad}: {e}"
        );
    }
    let (s, e) = app
        .post(
            "/nexus/link/join",
            &admin,
            json!({ "hub_url": hub.url, "join_code": " " }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    let (s, e) = app
        .post(
            "/nexus/link/join",
            &admin,
            json!({ "hub_url": "http://127.0.0.1:9", "join_code": code }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::SERVICE_UNAVAILABLE, Some("hub_unreachable")),
        "{e}"
    );
    let (s, e) = app
        .post(
            "/nexus/link/join",
            &admin,
            json!({ "hub_url": hub.url, "join_code": "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH" }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("hub_refused")),
        "{e}"
    );
    assert!(
        e["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("This join code is not valid"),
        "{e}"
    );
    assert_eq!(app.triggers().await, 0, "a failed join changes nothing");

    let (s, joined) = app
        .post(
            "/nexus/link/join",
            &admin,
            json!({ "hub_url": format!("{}/", hub.url), "join_code": code }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{joined}");
    assert_eq!(joined["linked"], true);
    assert_eq!(joined["hub_name"], "Gulshan Education Network");
    assert_eq!(joined["hub_url"], hub.url);
    assert!(
        joined["pending"].as_i64().unwrap() > 10,
        "everything is queued for the first upload: {joined}"
    );
    assert!(app.triggers().await > 60);
    // The key for the hub is stored sealed and shown nowhere.
    assert!(!joined.to_string().contains("token"));
    let sealed: Value = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'nexus.token'")
        .fetch_one(&app.db.pool)
        .await
        .unwrap();
    let hub_hash: String = sqlx::query_scalar("SELECT token_hash FROM hub_schools")
        .fetch_one(&hub.state.pool)
        .await
        .unwrap();
    assert_eq!(hub_hash.len(), 64, "the hub keeps a hash only");
    assert!(sealed.as_str().unwrap().len() > 60);
    let (_, audit) = app.get("/audit", &admin).await;
    assert!(
        !audit.to_string().contains(sealed.as_str().unwrap()),
        "the sealed key is not in the audit log"
    );
    // Joining twice is refused.
    let (s, e) = app
        .post(
            "/nexus/link/join",
            &admin,
            json!({ "hub_url": hub.url, "join_code": code }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("conflict")),
        "{e}"
    );

    // First sync: the upload.
    let (s, st) = app.post("/nexus/link/sync", &admin, json!({})).await;
    assert_eq!(s, StatusCode::OK, "{st}");
    assert_eq!(
        (st["pending"].as_i64(), st["last_error"].is_null()),
        (Some(0), true),
        "{st}"
    );
    assert!(st["last_sync_at"].is_string());
    assert!(st["last_report"]["sent"].as_i64().unwrap() > 10);
    assert_eq!(
        hub.in_school("SELECT full_name FROM students ORDER BY 1")
            .await,
        ["Ali Khan", "Sara Noor"]
    );
    assert_eq!(
        hub.in_school("SELECT password_hash FROM users").await,
        ["!"]
    );
    let row = hub.school(&hub_admin).await;
    assert_eq!(
        (
            row["status"].as_str(),
            row["code"].as_str(),
            row["edition"].as_str()
        ),
        (Some("active"), Some("KHI-NX"), Some("lite"))
    );

    // The school edits through its API; head office corrects another field in its copy.
    app.rename(&admin, "Ali Khan", "Ali Ahmed Khan").await;
    hub.in_school("UPDATE students SET father_name = 'Corrected by head office', updated_at = now(), hlc = '9999999999999-00000-00000000004b' WHERE full_name = 'Sara Noor' RETURNING full_name").await;
    let (_, st) = app.post("/nexus/link/sync", &admin, json!({})).await;
    assert_eq!(st["last_report"]["received"], 1, "{st}");
    assert_eq!(
        hub.in_school("SELECT full_name FROM students ORDER BY 1")
            .await,
        ["Ali Ahmed Khan", "Sara Noor"]
    );
    let father: String =
        sqlx::query_scalar("SELECT father_name FROM students WHERE full_name = 'Sara Noor'")
            .fetch_one(&app.db.pool)
            .await
            .unwrap();
    assert_eq!(father, "Corrected by head office");

    // The same field on both sides: head office wins and the school can see what it lost.
    hub.in_school("UPDATE students SET full_name = 'Sara Noor (head office)', updated_at = now(), hlc = '9999999999999-00001-00000000004b' WHERE full_name = 'Sara Noor' RETURNING full_name").await;
    app.rename(&admin, "Sara Noor", "Sara Noor (school)").await;
    let (_, st) = app.post("/nexus/link/sync", &admin, json!({})).await;
    assert_eq!(
        (
            st["overwrites"].as_i64(),
            st["last_report"]["overwrites"].as_i64()
        ),
        (Some(1), Some(1)),
        "{st}"
    );
    assert_eq!(
        app.names().await,
        ["Ali Ahmed Khan", "Sara Noor (head office)"]
    );
    let (s, log) = app.get("/nexus/link/overwrites", &admin).await;
    assert_eq!(s, StatusCode::OK, "{log}");
    assert_eq!(log.as_array().unwrap().len(), 1);
    assert_eq!(
        (log[0]["table"].as_str(), log[0]["field"].as_str()),
        (Some("students"), Some("full_name"))
    );
    assert_eq!(
        (log[0]["kept"].as_str(), log[0]["discarded"].as_str()),
        (Some("Sara Noor (head office)"), Some("Sara Noor (school)"))
    );
    assert_eq!(log[0]["reason"], "head_office_wins");

    // Only someone who manages settings may see or change the link.
    let (_, staff) = app
        .post("/staff", &admin, json!({ "full_name": "Sana Malik" }))
        .await;
    let (s, b) = app
        .post(
            &format!("/staff/{}/login", staff["id"].as_str().unwrap()),
            &admin,
            json!({ "username": "sana", "password": "teacher-password-1", "role": "teacher" }),
        )
        .await;
    assert!(s.is_success(), "{b}");
    let teacher = app.login("sana", "teacher-password-1").await;
    for (method, path) in [
        (Method::GET, "/nexus/link"),
        (Method::POST, "/nexus/link/sync"),
        (Method::POST, "/nexus/link/leave"),
        (Method::GET, "/nexus/link/overwrites"),
    ] {
        let (s, _) = app
            .call(method, path, Some(&teacher), Some(json!({})))
            .await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{path}");
    }

    // Leaving: the hub is told, capture stops, every record stays.
    let before = app.names().await;
    let (s, left) = app
        .post(
            "/nexus/link/leave",
            &admin,
            json!({ "reason": "Moving to another network" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{left}");
    assert_eq!(left["hub_told"], true);
    assert_eq!(left["status"]["linked"], false);
    assert_eq!(
        (
            left["status"]["ended"]["by"].as_str(),
            left["status"]["ended"]["reason"].as_str()
        ),
        (Some("school"), Some("Moving to another network"))
    );
    assert_eq!(app.triggers().await, 0);
    assert_eq!(app.names().await, before);
    app.rename(&admin, "Ali Ahmed Khan", "After Leaving").await;
    let (_, st) = app.get("/nexus/link", &admin).await;
    assert_eq!(
        st["pending"], 0,
        "nothing is queued once the school has left"
    );
    let (s, _) = app.post("/nexus/link/sync", &admin, json!({})).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let row = hub.school(&hub_admin).await;
    assert_eq!(
        (row["status"].as_str(), row["leave_reason"].as_str()),
        (Some("left"), Some("Moving to another network"))
    );
    assert_eq!(
        hub.in_school("SELECT full_name FROM students ORDER BY 1")
            .await,
        ["Ali Ahmed Khan", "Sara Noor (head office)"],
        "the hub keeps its archive as it was"
    );
    let token: Value = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'nexus.token'")
        .fetch_one(&app.db.pool)
        .await
        .unwrap();
    assert_eq!(token, "", "the key is forgotten");

    // The school can join again with a new invitation.
    let code = hub.invite(&hub_admin).await;
    let (s, again) = app
        .post(
            "/nexus/link/join",
            &admin,
            json!({ "hub_url": hub.url, "join_code": code }),
        )
        .await;
    assert_eq!(
        (s, again["linked"].as_bool(), again["ended"].is_null()),
        (StatusCode::OK, Some(true), true),
        "{again}"
    );
    app.finish().await;
    hub.finish().await;
}

#[tokio::test]
async fn a_school_keeps_working_while_the_hub_is_away_and_is_told_when_removed() {
    let (app, mut hub) = start!();
    let admin = app.setup("KHI-OF").await;
    let hub_admin = hub.setup().await;
    let code = hub.invite(&hub_admin).await;
    let (s, _) = app
        .post(
            "/nexus/link/join",
            &admin,
            json!({ "hub_url": hub.url, "join_code": code }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (_, st) = app.post("/nexus/link/sync", &admin, json!({})).await;
    assert_eq!(st["pending"], 0);

    // Head office removes the school. The school is told at its next sync, unlinks itself and
    // keeps everything.
    let id = hub.school(&hub_admin).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (s, b) = hub
        .call(
            reqwest::Method::POST,
            &format!("/schools/{id}/remove"),
            Some(&hub_admin),
            Some(json!({ "reason": "Fees to the network are overdue." })),
        )
        .await;
    assert_eq!(s, 200, "{b}");
    app.rename(&admin, "Ali Khan", "Changed Before Being Told")
        .await;
    let (s, st) = app.post("/nexus/link/sync", &admin, json!({})).await;
    assert_eq!(s, StatusCode::OK, "{st}");
    assert_eq!(st["linked"], false);
    assert_eq!(st["ended"]["by"], "head_office");
    assert_eq!(
        st["ended"]["reason"],
        "Head office removed this school from the network: Fees to the network are overdue."
    );
    assert_eq!(app.triggers().await, 0);
    assert_eq!(
        app.names().await,
        ["Changed Before Being Told", "Sara Noor"]
    );
    assert_eq!(
        hub.in_school("SELECT full_name FROM students ORDER BY 1")
            .await,
        ["Ali Khan", "Sara Noor"],
        "nothing was accepted after the removal"
    );

    // Join again, then the hub goes away: syncing reports the problem and loses nothing.
    let code = hub.invite(&hub_admin).await;
    let (s, _) = app
        .post(
            "/nexus/link/join",
            &admin,
            json!({ "hub_url": hub.url, "join_code": code }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    app.post("/nexus/link/sync", &admin, json!({})).await;
    hub.stop();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    app.rename(&admin, "Sara Noor", "Edited While Offline")
        .await;
    let (s, st) = app.post("/nexus/link/sync", &admin, json!({})).await;
    assert_eq!(
        s,
        StatusCode::OK,
        "being offline is a status, not a failure: {st}"
    );
    assert_eq!(st["linked"], true);
    assert!(
        st["last_error"]
            .as_str()
            .unwrap()
            .starts_with("The hub could not be reached"),
        "{st}"
    );
    assert!(
        st["pending"].as_i64().unwrap() >= 1,
        "the edit waits for the hub: {st}"
    );
    assert_eq!(
        app.names().await,
        ["Changed Before Being Told", "Edited While Offline"]
    );
    // Leaving while the hub is away still works; the school says the hub was not told.
    let (s, left) = app.post("/nexus/link/leave", &admin, json!({})).await;
    assert_eq!(
        (
            s,
            left["hub_told"].as_bool(),
            left["status"]["linked"].as_bool()
        ),
        (StatusCode::OK, Some(false), Some(false)),
        "{left}"
    );
    assert_eq!(app.triggers().await, 0);
    app.finish().await;
    hub.finish().await;
}

#[tokio::test]
async fn the_link_survives_a_restart_and_follows_the_edition() {
    let (app, hub) = start!();
    let admin = app.setup("KHI-RS").await;
    let hub_admin = hub.setup().await;
    // Standard schools join the same way.
    let (s, _) = app
        .call(
            Method::PUT,
            "/settings/edition",
            Some(&admin),
            Some(json!({ "edition": "standard" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let code = hub.invite(&hub_admin).await;
    let (s, _) = app
        .post(
            "/nexus/link/join",
            &admin,
            json!({ "hub_url": hub.url, "join_code": code }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    app.post("/nexus/link/sync", &admin, json!({})).await;
    assert_eq!(hub.school(&hub_admin).await["edition"], "standard");

    // A restart with the triggers missing (as after a version that added tables): the check
    // at start puts them back. A restart of an unjoined school removes stray ones.
    let installed = app.triggers().await;
    sqlx::query("DROP TRIGGER sync_capture ON students")
        .execute(&app.db.pool)
        .await
        .unwrap();
    assert_eq!(app.triggers().await, installed - 1);
    let restarted = AppState::new(app.db.pool.clone()).await.unwrap();
    madrasa_api::nexus_link::reconcile(&restarted)
        .await
        .unwrap();
    assert_eq!(app.triggers().await, installed);
    app.rename(&admin, "Ali Khan", "After Restart").await;
    let (_, st) = app.post("/nexus/link/sync", &admin, json!({})).await;
    assert_eq!(st["pending"], 0);
    assert_eq!(
        hub.in_school("SELECT full_name FROM students ORDER BY 1")
            .await,
        ["After Restart", "Sara Noor"]
    );
    let _ = &app.state;
    app.finish().await;
    hub.finish().await;
}

/// Stage 6, end to end: the school leaves its backup key with head office, head office
/// releases it, the school is told, and the released key opens a real backup.
#[tokio::test]
async fn a_backup_key_left_with_head_office_is_released_and_opens_a_real_backup() {
    let (app, hub) = start!();
    let admin = app.setup("KHI-ES").await;
    let hub_admin = hub.setup().await;
    let delete = |path: &'static str| app.call(Method::DELETE, path, Some(&admin), None);

    // A school that has not joined has nobody to leave a key with.
    let (s, e) = app.get("/nexus/link/escrow", &admin).await;
    assert_eq!(s, StatusCode::CONFLICT, "{e}");
    let code = hub.invite(&hub_admin).await;
    let (s, b) = app
        .post(
            "/nexus/link/join",
            &admin,
            json!({ "hub_url": hub.url, "join_code": code }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, b) = app.post("/nexus/link/sync", &admin, json!({})).await;
    assert_eq!((s, &b["last_error"]), (StatusCode::OK, &Value::Null), "{b}");

    // Head office has not set escrow up yet: the school is told so, plainly.
    let (s, view) = app.get("/nexus/link/escrow", &admin).await;
    assert_eq!(s, StatusCode::OK, "{view}");
    assert_eq!(
        (view["available"].as_bool(), view["deposited"].as_bool()),
        (Some(false), Some(false))
    );
    let (s, e) = app
        .post(
            "/nexus/link/escrow",
            &admin,
            json!({ "password": "backup-password-1" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{e}");
    assert_eq!(
        e["error"]["message"],
        "Head office has not set up key escrow yet. Ask head office, then try again."
    );
    let passphrase = "head office escrow passphrase";
    let (s, _) = hub
        .call(
            reqwest::Method::POST,
            "/escrow/setup",
            Some(&hub_admin),
            Some(json!({ "passphrase": passphrase })),
        )
        .await;
    assert_eq!(s, 204);

    // Leaving the key needs the backup password (or the recovery key); a wrong one is
    // refused and audited; a "released key" proves nothing here.
    let (s, e) = app
        .post(
            "/nexus/link/escrow",
            &admin,
            json!({ "password": "wrong-password-1" }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::UNAUTHORIZED, Some("backup_secret_invalid"))
    );
    let (s, _) = app
        .post(
            "/nexus/link/escrow",
            &admin,
            json!({ "released_key": "00000-00000-00000-00000-00000-00000-00000-00000-00000-00000-00000" }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, view) = app
        .post(
            "/nexus/link/escrow",
            &admin,
            json!({ "password": "backup-password-1" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{view}");
    assert_eq!(
        (
            view["available"].as_bool(),
            view["deposited"].as_bool(),
            view["current"].as_bool(),
            view["releases"].as_array().map(Vec::len)
        ),
        (Some(true), Some(true), Some(true), Some(0))
    );
    // What the hub holds is sealed: the school's key set is not in it.
    let held: String = sqlx::query_scalar("SELECT sealed::text FROM hub_escrow_deposits")
        .fetch_one(&hub.state.pool)
        .await
        .unwrap();
    let key_set: String =
        sqlx::query_scalar("SELECT value::text FROM settings WHERE key = 'backup.keys'")
            .fetch_one(&app.db.pool)
            .await
            .unwrap();
    let key_set: Value = serde_json::from_str(&key_set).unwrap();
    assert!(held.contains(key_set["public_key"].as_str().unwrap()));
    assert!(!held.contains(key_set["by_password"]["ciphertext"].as_str().unwrap()));

    // Only people who manage settings and backups may do any of this.
    let (_, staff) = app
        .post("/staff", &admin, json!({ "full_name": "Sana Malik" }))
        .await;
    let (s, b) = app
        .post(
            &format!("/staff/{}/login", staff["id"].as_str().unwrap()),
            &admin,
            json!({ "username": "sana", "password": "teacher-password-1", "role": "teacher" }),
        )
        .await;
    assert!(s.is_success(), "{b}");
    let teacher = app.login("sana", "teacher-password-1").await;
    for method in [Method::GET, Method::POST, Method::DELETE] {
        let (s, _) = app
            .call(
                method.clone(),
                "/nexus/link/escrow",
                Some(&teacher),
                Some(json!({ "password": "backup-password-1" })),
            )
            .await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{method}");
    }

    // A real backup, made while the key is with head office.
    let dir = std::env::temp_dir().join(format!("madrasa-escrow-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let (s, b) = app
        .call(
            Method::PUT,
            "/backup/settings",
            Some(&admin),
            Some(json!({ "enabled": false, "time": "02:00", "directory": dir.display().to_string(), "keep": 5 })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, run) = app.post("/backup/run", &admin, json!({})).await;
    assert_eq!(s, StatusCode::OK, "{run}");
    let path = run["file_path"].as_str().unwrap().to_string();

    // The school "loses" its password and recovery key. Head office releases the key.
    let school_ref = hub.school(&hub_admin).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let reason = "Principal lost the backup password and the recovery key";
    let (s, released) = hub
        .call(
            reqwest::Method::POST,
            &format!("/schools/{school_ref}/escrow/release"),
            Some(&hub_admin),
            Some(json!({ "passphrase": passphrase, "reason": reason })),
        )
        .await;
    assert_eq!(s, 200, "{released}");
    let released_key = released["released_key"].as_str().unwrap().to_string();

    // At its next sync the school records the release in its own audit log and confirms.
    let (s, b) = app.post("/nexus/link/sync", &admin, json!({})).await;
    assert_eq!((s, &b["last_error"]), (StatusCode::OK, &Value::Null), "{b}");
    let (_, audit) = app.get("/audit?limit=200", &admin).await;
    let entries = audit["items"]
        .as_array()
        .or_else(|| audit.as_array())
        .unwrap();
    let noted: Vec<&Value> = entries
        .iter()
        .filter(|e| e["entity"] == "nexus_escrow" && e["action"] == "update")
        .collect();
    assert_eq!(noted.len(), 1, "{audit}");
    assert_eq!(
        (
            noted[0]["new_value"]["reason"].as_str(),
            noted[0]["new_value"]["released_by"].as_str()
        ),
        (Some(reason), Some("Head Office"))
    );
    assert!(!audit.to_string().contains(&released_key));
    let (_, view) = app.get("/nexus/link/escrow", &admin).await;
    assert_eq!(
        (
            view["releases"][0]["reason"].as_str(),
            view["releases"][0]["seen"].as_bool()
        ),
        (Some(reason), Some(true)),
        "{view}"
    );
    // A second sync does not record it again.
    app.post("/nexus/link/sync", &admin, json!({})).await;
    let (_, audit) = app.get("/audit?limit=200", &admin).await;
    let again = audit["items"]
        .as_array()
        .or_else(|| audit.as_array())
        .unwrap()
        .iter()
        .filter(|e| e["entity"] == "nexus_escrow" && e["action"] == "update")
        .count();
    assert_eq!(again, 1);

    // Restore with the released key. A mistyped character and another school's key are
    // refused before anything is touched.
    app.rename(&admin, "Ali Khan", "Ali Changed").await;
    let mut typo: Vec<char> = released_key.chars().collect();
    typo[3] = if typo[3] == '7' { '8' } else { '7' };
    let typo: String = typo.into_iter().collect();
    let other = madrasa_core::escrow::format_released_key(&[0x42; 32]);
    for (key, message) in [
        (typo.as_str(), "The key released by head office is incomplete or has a wrong character. Check it against the code you were given."),
        (other.as_str(), "This key does not open this backup. It was released for another school or for an older key."),
    ] {
        let (s, e) = app
            .post(
                "/backup/restore",
                &admin,
                json!({ "path": path, "released_key": key }),
            )
            .await;
        assert_eq!(
            (s, e["error"]["code"].as_str(), e["error"]["message"].as_str()),
            (StatusCode::UNAUTHORIZED, Some("backup_secret_invalid"), Some(message))
        );
    }
    assert_eq!(app.names().await, ["Ali Changed", "Sara Noor"]);
    let (s, r) = app
        .post(
            "/backup/restore",
            &admin,
            json!({ "path": path, "released_key": released_key.to_lowercase().replace('-', " ") }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(
        app.names().await,
        ["Ali Khan", "Sara Noor"],
        "the backup is back"
    );

    // The released key also lets the school set a new backup password, which then works.
    let (s, e) = app
        .post(
            "/backup/password",
            &admin,
            json!({ "released_key": other, "new_password": "new-backup-password-1" }),
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "{e}");
    let (s, e) = app
        .post(
            "/backup/password",
            &admin,
            json!({ "released_key": released_key, "new_password": "new-backup-password-1" }),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{e}");
    let (s, r) = app
        .post(
            "/backup/restore",
            &admin,
            json!({ "path": path, "password": "backup-password-1" }),
        )
        .await;
    assert_eq!(
        s,
        StatusCode::OK,
        "the old file still opens with its old password: {r}"
    );
    let (s, _) = app
        .post(
            "/backup/password",
            &admin,
            json!({ "current_password": "backup-password-1", "new_password": "new-backup-password-2" }),
        )
        .await;
    assert_eq!(
        s,
        StatusCode::NO_CONTENT,
        "restore brought the old key set back"
    );

    // The school takes its key back: nothing sealed is left at the hub.
    let (s, view) = delete("/nexus/link/escrow").await;
    assert_eq!(s, StatusCode::OK, "{view}");
    assert_eq!(view["deposited"], false);
    assert_eq!(view["releases"].as_array().unwrap().len(), 1);
    let sealed: i64 =
        sqlx::query_scalar("SELECT count(*) FROM hub_escrow_deposits WHERE sealed IS NOT NULL")
            .fetch_one(&hub.state.pool)
            .await
            .unwrap();
    assert_eq!(sealed, 0);
    let (s, e) = hub
        .call(
            reqwest::Method::POST,
            &format!("/schools/{school_ref}/escrow/release"),
            Some(&hub_admin),
            Some(json!({ "passphrase": passphrase, "reason": "After it was taken back" })),
        )
        .await;
    assert_eq!((s, e["error"]["code"].as_str()), (409, Some("no_deposit")));

    let _ = std::fs::remove_dir_all(&dir);
    app.finish().await;
    hub.finish().await;
}
