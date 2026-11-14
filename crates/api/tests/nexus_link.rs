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

    /// Row counts of these tables in the only school's copy.
    async fn counts(&self, tables: &[&str]) -> Vec<i64> {
        let mut conn = self.state.pool.acquire().await.unwrap();
        let school = tenants::list(&mut conn).await.unwrap().remove(0);
        drop(conn);
        let mut tx = tenants::begin(&self.state.pool, &school).await.unwrap();
        let mut out = Vec::new();
        for t in tables {
            let n: i64 =
                sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM \"{t}\"")))
                    .fetch_one(&mut *tx)
                    .await
                    .unwrap();
            out.push(n);
        }
        tx.commit().await.unwrap();
        out
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
                json!({ "path": path, "released_key": key, "confirm_network": true }),
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
            json!({ "path": path, "released_key": released_key.to_lowercase().replace('-', " "), "confirm_network": true }),
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
            json!({ "path": path, "password": "backup-password-1", "confirm_network": true }),
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

/// Parent app stage 2, end to end: the school gives a guardian an access code through its
/// hub, the parent activates it at the hub, the school sees that access is open and ends it.
#[tokio::test]
async fn a_school_gives_a_guardian_an_access_code_and_ends_access_again() {
    let (app, hub) = start!();
    let admin = app.setup("KHI-PA").await;
    let hub_admin = hub.setup().await;
    let ali = app.student_id("Ali Khan").await;
    let (s, b) = app
        .post(
            &format!("/students/{ali}/guardians"),
            &admin,
            json!({ "full_name": "Imran Khan", "relationship": "father", "phone": "0300-1111111", "is_primary": true }),
        )
        .await;
    assert!(s.is_success(), "{b}");
    let imran: Uuid = sqlx::query_scalar("SELECT id FROM guardians WHERE full_name = 'Imran Khan'")
        .fetch_one(&app.db.pool)
        .await
        .unwrap();

    // Not joined yet: there is no hub to ask.
    let (s, e) = app
        .post(
            &format!("/parent-app/access/{imran}/code"),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("not_joined")),
        "{e}"
    );

    let code = hub.invite(&hub_admin).await;
    let (s, b) = app
        .post(
            "/nexus/link/join",
            &admin,
            json!({ "hub_url": hub.url, "join_code": code }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");

    // Only someone who manages parent access; a teacher does not.
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
    let (s, _) = app.get("/parent-app/access", &teacher).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app
        .post(
            &format!("/parent-app/access/{imran}/code"),
            &teacher,
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Nothing has been uploaded yet: asking for a code syncs first, so the hub knows the
    // guardian by the time it is asked.
    let (_, st) = app.get("/nexus/link", &admin).await;
    assert!(st["pending"].as_i64().unwrap() > 0);
    let (s, issued) = app
        .post(
            &format!("/parent-app/access/{imran}/code"),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{issued}");
    let access_code = issued["code"].as_str().unwrap().to_string();
    assert_eq!(
        (
            issued["guardian_name"].as_str(),
            issued["hub_url"].as_str(),
            issued["hub_name"].as_str()
        ),
        (
            Some("Imran Khan"),
            Some(hub.url.as_str()),
            Some("Gulshan Education Network")
        )
    );
    let (s, _) = app
        .post(
            &format!("/parent-app/access/{}/code", Uuid::now_v7()),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, list) = app.get("/parent-app/access", &admin).await;
    assert_eq!(s, StatusCode::OK, "{list}");
    assert_eq!(
        (
            list[0]["guardian_name"].as_str(),
            list[0]["children"].clone(),
            list[0]["state"].as_str()
        ),
        (Some("Imran Khan"), json!(["Ali Khan"]), Some("code"))
    );

    // The parent, in the app: checks the code, then activates it.
    let (s, check) = hub
        .call(
            reqwest::Method::POST,
            "/parent/activate/check",
            None,
            Some(json!({ "code": access_code })),
        )
        .await;
    assert_eq!(s, 200, "{check}");
    assert_eq!(
        (
            check["guardian_name"].as_str(),
            check["school_name"].as_str(),
            check["children"].clone()
        ),
        (
            Some("Imran Khan"),
            Some("Gulshan High"),
            json!(["Ali Khan"])
        )
    );
    let (s, session) = hub
        .call(reqwest::Method::POST, "/parent/activate", None, Some(json!({ "code": access_code, "phone": "0300-1234567", "password": "parent-password-1" })))
        .await;
    assert_eq!(s, 200, "{session}");
    let parent = session["token"].as_str().unwrap().to_string();
    let (_, me) = hub
        .call(reqwest::Method::GET, "/parent/me", Some(&parent), None)
        .await;
    assert_eq!(
        (me["links"].as_i64(), me["hub_name"].as_str()),
        (Some(1), Some("Gulshan Education Network"))
    );

    // The school sees it, with only a hint of the phone number.
    let (_, list) = app.get("/parent-app/access", &admin).await;
    assert_eq!(
        (list[0]["state"].as_str(), list[0]["phone_hint"].as_str()),
        (Some("active"), Some("•••• 4567"))
    );
    // The school's own log has the code being issued, not the code.
    let (_, audit) = app.get("/audit?limit=200", &admin).await;
    assert!(!audit.to_string().contains(&access_code));
    let entries = audit["items"]
        .as_array()
        .or_else(|| audit.as_array())
        .unwrap();
    assert!(entries
        .iter()
        .any(|e| e["entity"] == "parent_access" && e["action"] == "create"));

    // The school ends access: the parent's account no longer reaches anything here.
    let (s, ended) = app
        .call(
            Method::DELETE,
            &format!("/parent-app/access/{imran}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(
        (s, ended["ended"].as_bool()),
        (StatusCode::OK, Some(true)),
        "{ended}"
    );
    let (_, me) = hub
        .call(reqwest::Method::GET, "/parent/me", Some(&parent), None)
        .await;
    assert_eq!(me["links"], 0);
    let (_, list) = app.get("/parent-app/access", &admin).await;
    assert_eq!(list, json!([]));

    app.finish().await;
    hub.finish().await;
}

/// Parent app stage 3: what a parent sees in the app is what the school's own screens say.
/// A school with attendance, marks, fees, a notice and a class teacher joins a hub; a
/// guardian activates access; then every figure the hub gives the parent is compared with
/// the school's own API. Marks the principal has not approved are not shown, and once they
/// are approved the parent's report card is the school's, field for field.
#[tokio::test]
async fn a_parent_sees_the_schools_own_figures_and_only_approved_marks() {
    let (app, hub) = start!();
    let admin = app.setup("KHI-PD").await;
    let hub_admin = hub.setup().await;
    let ok = |(s, b): (StatusCode, Value), what: &str| {
        assert!(s.is_success(), "{what}: {s} {b}");
        b
    };
    let one = |sql: &'static str| {
        let pool = app.db.pool.clone();
        async move {
            sqlx::query_scalar::<_, Uuid>(sql)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    let year = one("SELECT id FROM academic_years").await;
    let class = one("SELECT id FROM classes").await;
    let section = one("SELECT id FROM sections").await;
    let ali = app.student_id("Ali Khan").await;
    let sara = app.student_id("Sara Noor").await;

    // ---- the school's records ------------------------------------------------------------------
    let eng = ok(
        app.post(
            "/subjects",
            &admin,
            json!({ "name": "English", "code": "ENG" }),
        )
        .await,
        "subject",
    );
    let mth = ok(
        app.post(
            "/subjects",
            &admin,
            json!({ "name": "Mathematics", "code": "MTH" }),
        )
        .await,
        "subject",
    );
    ok(
        app.call(
            Method::PUT,
            &format!("/classes/{class}/subjects"),
            Some(&admin),
            Some(json!({ "subject_ids": [eng["id"], mth["id"]] })),
        )
        .await,
        "class subjects",
    );
    let staff = ok(
        app.post("/staff", &admin, json!({ "full_name": "Sana Malik" }))
            .await,
        "staff",
    );
    ok(
        app.post(
            &format!("/staff/{}/login", staff["id"].as_str().unwrap()),
            &admin,
            json!({ "username": "sana", "password": "teacher-password-1", "role": "teacher" }),
        )
        .await,
        "teacher login",
    );
    ok(
        app.call(
            Method::PATCH,
            &format!("/sections/{section}"),
            Some(&admin),
            Some(json!({ "class_teacher_id": staff["id"] })),
        )
        .await,
        "class teacher",
    );
    let teacher = app.login("sana", "teacher-password-1").await;
    ok(
        app.post(
            &format!("/students/{ali}/guardians"),
            &admin,
            json!({ "full_name": "Imran Khan", "relationship": "father", "phone": "0300-1111111", "is_primary": true }),
        )
        .await,
        "guardian",
    );
    let imran = one("SELECT id FROM guardians WHERE full_name = 'Imran Khan'").await;

    // Attendance: absent, late, present, and a day of leave the month after.
    for (date, entries) in [
        (
            "2026-04-06",
            json!([{ "student_id": ali, "status": "absent", "note": "sick" }]),
        ),
        (
            "2026-04-07",
            json!([{ "student_id": ali, "status": "late" }]),
        ),
        ("2026-04-08", json!([])),
        (
            "2026-05-04",
            json!([{ "student_id": ali, "status": "leave" }]),
        ),
    ] {
        ok(
            app.call(
                Method::PUT,
                "/attendance/sheet",
                Some(&admin),
                Some(json!({ "section_id": section, "date": date, "entries": entries })),
            )
            .await,
            "attendance",
        );
    }

    // Marks: English approved, Mathematics entered but not approved.
    let list = ok(
        app.post(
            "/exams/assessments/defaults",
            &admin,
            json!({ "academic_year_id": year }),
        )
        .await,
        "assessments",
    );
    let test1 = list[0]["id"].clone();
    ok(
        app.call(
            Method::PUT,
            "/exams/max-marks",
            Some(&admin),
            Some(json!({ "assessment_id": test1, "class_id": class, "lines": [
                { "subject_id": eng["id"], "max_marks": "50" }, { "subject_id": mth["id"], "max_marks": "50" }] })),
        )
        .await,
        "max marks",
    );
    let mut sheets = Vec::new();
    for (subject, a, s) in [(&eng, "45", "30"), (&mth, "37.5", "25.5")] {
        let v = ok(
            app.call(
                Method::PUT,
                "/exams/sheet",
                Some(&admin),
                Some(json!({ "assessment_id": test1, "section_id": section, "subject_id": subject["id"], "marks": [
                    { "student_id": ali, "obtained": a }, { "student_id": sara, "obtained": s }] })),
            )
            .await,
            "marks",
        );
        sheets.push(v["sheet"]["id"].as_str().unwrap().to_string());
    }
    for step in ["submit", "approve"] {
        ok(
            app.post(
                &format!("/exams/sheets/{}/{step}", sheets[0]),
                &admin,
                json!({}),
            )
            .await,
            step,
        );
    }

    // Fees: two months billed (the first is carried into the second), part of it paid.
    let tuition = ok(
        app.post(
            "/fees/heads",
            &admin,
            json!({ "name": "Tuition", "code": "TUI" }),
        )
        .await,
        "fee head",
    );
    ok(
        app.call(
            Method::PUT,
            "/fees/structure",
            Some(&admin),
            Some(json!({ "academic_year_id": year, "class_id": class, "lines": [{ "fee_head_id": tuition["id"], "amount": "2500" }] })),
        )
        .await,
        "fee structure",
    );
    for month in ["2026-04-01", "2026-05-01"] {
        ok(
            app.post("/fees/vouchers/generate", &admin, json!({ "month": month }))
                .await,
            "vouchers",
        );
    }
    let ledger_path = format!("/fees/students/{ali}/ledger");
    let ledger = ok(app.get(&ledger_path, &admin).await, "ledger");
    let latest = ledger["vouchers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["billing_month"] == "2026-05-01")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    ok(
        app.post(
            &format!("/fees/vouchers/{latest}/payments"),
            &admin,
            json!({ "amount": "1000", "method": "cash" }),
        )
        .await,
        "payment",
    );

    // ---- join, a notice, access for the guardian ----------------------------------------------
    let code = hub.invite(&hub_admin).await;
    ok(
        app.post(
            "/nexus/link/join",
            &admin,
            json!({ "hub_url": hub.url, "join_code": code }),
        )
        .await,
        "join",
    );
    ok(
        app.post(
            "/parent-app/notices",
            &admin,
            json!({ "title": "Sports day", "body": "Friday. Bring water.", "publish": true }),
        )
        .await,
        "notice",
    );
    let issued = ok(
        app.post(
            &format!("/parent-app/access/{imran}/code"),
            &admin,
            json!({}),
        )
        .await,
        "access code",
    );
    let (s, session) = hub
        .call(
            reqwest::Method::POST,
            "/parent/activate",
            None,
            Some(json!({ "code": issued["code"], "phone": "0300-1111111", "password": "parent-password-1" })),
        )
        .await;
    assert_eq!(s, 200, "{session}");
    let parent = session["token"].as_str().unwrap().to_string();
    let sync = || async {
        let (s, st) = app.post("/nexus/link/sync", &admin, json!({})).await;
        assert_eq!(s, StatusCode::OK, "{st}");
    };
    sync().await;
    let seen = |path: String| {
        let (hub, parent) = (&hub, parent.clone());
        async move {
            let (s, b) = hub
                .call(reqwest::Method::GET, &path, Some(&parent), None)
                .await;
            assert_eq!(s, 200, "{path}: {b}");
            b
        }
    };

    let kids = seen("/parent/children".into()).await;
    assert_eq!(kids["children"].as_array().unwrap().len(), 1, "{kids}");
    assert_eq!(
        (
            kids["children"][0]["full_name"].as_str(),
            kids["children"][0]["class_name"].as_str(),
            kids["children"][0]["section_name"].as_str(),
            kids["children"][0]["roll_no"].as_i64()
        ),
        (Some("Ali Khan"), Some("5"), Some("A"), Some(1))
    );
    let school_ref = kids["children"][0]["school_ref"]
        .as_str()
        .unwrap()
        .to_string();
    let child = format!("/parent/children/{school_ref}/{ali}");

    // ---- attendance ----------------------------------------------------------------------------
    let theirs = seen(format!("{child}/attendance?month=2026-04")).await;
    let ours = ok(
        app.get(
            &format!("/attendance/students/{ali}?from=2026-04-01&to=2026-04-30"),
            &admin,
        )
        .await,
        "school attendance",
    );
    for key in ["present", "absent", "late", "leave"] {
        assert_eq!(theirs["summary"][key], ours["summary"][key], "April {key}");
    }
    assert_eq!(theirs["summary"]["percent"], ours["percent"]);
    assert_eq!(
        (
            theirs["summary"]["present"].as_i64(),
            theirs["summary"]["absent"].as_i64(),
            theirs["summary"]["late"].as_i64()
        ),
        (Some(1), Some(1), Some(1))
    );
    let mut school_days: Vec<(String, String)> = ours["days"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["date"].as_str().unwrap().into(),
                d["status"].as_str().unwrap().into(),
            )
        })
        .collect();
    school_days.sort();
    let app_days: Vec<(String, String)> = theirs["days"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["date"].as_str().unwrap().into(),
                d["status"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(app_days, school_days, "the same days, oldest first");
    assert!(
        !theirs.to_string().contains("sick"),
        "the teacher's note on an absence is not sent to the app"
    );
    let whole_year = ok(
        app.get(
            &format!("/attendance/students/{ali}?from=2026-04-01&to=2027-03-31"),
            &admin,
        )
        .await,
        "school attendance, year",
    );
    for key in ["present", "absent", "late", "leave"] {
        assert_eq!(
            theirs["year"]["summary"][key], whole_year["summary"][key],
            "year {key}"
        );
    }
    assert_eq!(theirs["year"]["summary"]["percent"], whole_year["percent"]);
    assert_eq!(theirs["year"]["summary"]["leave"], 1);
    let overview = seen(child.clone()).await;
    assert_eq!(overview["attendance"]["percent"], whole_year["percent"]);
    assert_eq!(overview["class_teacher"], "Sana Malik");

    // ---- fees ----------------------------------------------------------------------------------
    let ledger = ok(app.get(&ledger_path, &admin).await, "ledger");
    let fees = seen(format!("{child}/fees")).await;
    assert_eq!(fees["balance"], ledger["totals"]["balance"], "{fees}");
    // April with its late fee carried (2,750) + May (2,500) + May's late fee (250) − 1,000 paid.
    assert_eq!(fees["balance"], "4500.00");
    assert_eq!(overview["balance"], ledger["totals"]["balance"]);
    assert_eq!(
        (
            fees["currency"].as_str(),
            overview["open_vouchers"].as_i64()
        ),
        (Some("PKR"), Some(1))
    );
    let school_vouchers = ledger["vouchers"].as_array().unwrap();
    let app_vouchers = fees["vouchers"].as_array().unwrap();
    assert_eq!(app_vouchers.len(), school_vouchers.len());
    for (mine, school) in app_vouchers.iter().zip(school_vouchers) {
        for key in [
            "id",
            "voucher_no",
            "billing_month",
            "issue_date",
            "due_date",
            "gross",
            "sibling_discount",
            "concession_discount",
            "net_monthly",
            "arrears",
            "total_due",
            "late_fee",
            "net_paid",
            "status",
        ] {
            assert_eq!(mine[key], school[key], "voucher {key}: {mine} / {school}");
        }
        if mine["status"] == "carried" {
            // Its amount is in the next voucher's arrears; the app does not show it twice.
            assert_eq!(mine["balance"], "0", "{mine}");
        } else {
            assert_eq!(mine["balance"], school["balance"], "{mine} / {school}");
        }
        assert!(
            mine["total_due"].is_string(),
            "money is decimal text, never a float"
        );
        assert!(!mine["lines"].as_array().unwrap().is_empty(), "{mine}");
    }
    let paid = app_vouchers
        .iter()
        .find(|v| v["billing_month"] == "2026-05-01")
        .unwrap();
    assert_eq!(
        (
            paid["status"].as_str(),
            paid["arrears"].as_str(),
            paid["late_fee"].as_str(),
            paid["net_paid"].as_str(),
            paid["balance"].as_str(),
            paid["lines"][0]["description"].as_str()
        ),
        (
            Some("partial"),
            Some("2750.00"),
            Some("250.00"),
            Some("1000.00"),
            Some("4500.00"),
            Some("Tuition")
        )
    );
    assert_eq!(app_vouchers[1]["status"], "carried");
    let school_receipts = ledger["receipts"].as_array().unwrap();
    let app_receipts = fees["receipts"].as_array().unwrap();
    assert_eq!((app_receipts.len(), school_receipts.len()), (1, 1));
    for key in ["id", "receipt_no", "paid_on", "amount", "method"] {
        assert_eq!(
            app_receipts[0][key], school_receipts[0][key],
            "receipt {key}"
        );
    }
    assert_eq!(app_receipts[0]["voucher_no"], paid["voucher_no"]);
    assert!(app_receipts[0].get("received_by").is_none());

    // ---- results: approved marks only ----------------------------------------------------------
    let card_path = format!("/exams/results/student/{ali}");
    let school_card = ok(app.get(&card_path, &admin).await, "school card");
    let results = seen(format!("{child}/results")).await;
    let card = &results["card"];
    let cell = |card: &Value, subject: &str| {
        card["subjects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["subject_name"] == subject)
            .unwrap_or_else(|| panic!("{subject} missing: {card}"))["cells"][0]
            .clone()
    };
    // The school sees both subjects, and which is approved.
    assert_eq!(cell(&school_card, "Mathematics")["obtained"], "37.50");
    assert_eq!(cell(&school_card, "Mathematics")["approved"], false);
    // The parent sees English exactly as the school does, and nothing of Mathematics.
    assert_eq!(cell(card, "English"), cell(&school_card, "English"));
    assert_eq!(cell(card, "English")["obtained"], "45.00");
    let maths = cell(card, "Mathematics");
    assert!(
        maths["obtained"].is_null() && maths["max_marks"].is_null() && maths["percent"].is_null(),
        "unapproved marks are not shown: {maths}"
    );
    assert_eq!(
        card["totals"][0]["total"], "45.00",
        "the total counts approved marks only"
    );
    assert_eq!(card["totals"][0]["max"], "50.00");
    assert!(
        card["totals"][0]["position"].is_null(),
        "no position until every sheet is approved"
    );
    assert!(!results.to_string().contains("37.50"), "{results}");
    assert_ne!(card, &school_card);

    // Once the principal approves Mathematics, the parent's card is the school's card.
    for step in ["submit", "approve"] {
        ok(
            app.post(
                &format!("/exams/sheets/{}/{step}", sheets[1]),
                &admin,
                json!({}),
            )
            .await,
            step,
        );
    }
    let results = seen(format!("{child}/results")).await;
    assert!(
        cell(&results["card"], "Mathematics")["obtained"].is_null(),
        "not before the school syncs"
    );
    sync().await;
    let school_card = ok(app.get(&card_path, &admin).await, "school card");
    let results = seen(format!("{child}/results")).await;
    assert_eq!(
        results["card"], school_card,
        "the same report card, field for field"
    );
    assert_eq!(results["card"]["totals"][0]["total"], "82.50");
    assert_eq!(results["card"]["totals"][0]["position"], 1);
    assert!(results["synced_at"].is_string());

    // ---- notices and messages ------------------------------------------------------------------
    let notices = seen(format!("{child}/notices")).await;
    assert_eq!(
        (
            notices["notices"][0]["title"].as_str(),
            notices["notices"][0]["whole_school"].as_bool()
        ),
        (Some("Sports day"), Some(true)),
        "{notices}"
    );
    let (s, sent) = hub
        .call(
            reqwest::Method::POST,
            &format!("{child}/messages"),
            Some(&parent),
            Some(json!({ "body": "Ali has a fever today." })),
        )
        .await;
    assert_eq!(s, 201, "{sent}");
    sync().await;
    let list = ok(
        app.get("/parent-app/conversations", &teacher).await,
        "conversations",
    );
    assert_eq!(
        (
            list[0]["guardian_name"].as_str(),
            list[0]["last_preview"].as_str(),
            list[0]["unread"].as_i64()
        ),
        (Some("Imran Khan"), Some("Ali has a fever today."), Some(1)),
        "{list}"
    );
    ok(
        app.post(
            &format!("/parent-app/conversations/{ali}/{imran}/messages"),
            &teacher,
            json!({ "body": "Thank you. I hope he is better soon." }),
        )
        .await,
        "reply",
    );
    sync().await;
    let thread = seen(format!("{child}/messages")).await;
    let last = &thread["messages"][1];
    assert_eq!(
        (
            last["sender"].as_str(),
            last["sender_name"].as_str(),
            last["body"].as_str(),
            thread["unread"].as_i64()
        ),
        (
            Some("staff"),
            Some("Sana Malik"),
            Some("Thank you. I hope he is better soon."),
            Some(1)
        ),
        "{thread}"
    );

    // Another child of the same school stays out of reach.
    let (s, _) = hub
        .call(
            reqwest::Method::GET,
            &format!("/parent/children/{school_ref}/{sara}/results"),
            Some(&parent),
            None,
        )
        .await;
    assert_eq!(s, 404);

    hub.finish().await;
    app.finish().await;
}

/// Row counts of these tables in the school's own database: the school's records. Rows that
/// belong to no school (instance-wide log entries such as an edition switch) are never sent
/// to a hub, by design, so they are not counted.
async fn school_counts(app: &App, tables: &[&str]) -> Vec<i64> {
    let mut out = Vec::new();
    for t in tables {
        let owner = if *t == "schools" { "id" } else { "school_id" };
        let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM \"{t}\" WHERE {owner} IS NOT NULL"
        )))
        .fetch_one(&app.db.pool)
        .await
        .unwrap();
        out.push(n);
    }
    out
}

/// v0.4.3: restoring an older backup while joined. Before, the school said "in step" while
/// head office held records the school no longer had, and the restored school issued the
/// same student numbers again, so new records waited at the hub for ever. Now the restore
/// asks for a confirmation, head office sets its copy aside at the next sync and starts a
/// fresh one from what the school holds, and both copies agree record for record.
#[tokio::test]
async fn restoring_an_older_backup_while_joined_resets_head_offices_copy() {
    let (app, hub) = start!();
    let admin = app.setup("KHI-RS").await;
    let hub_admin = hub.setup().await;
    let section: Uuid = sqlx::query_scalar("SELECT id FROM sections")
        .fetch_one(&app.db.pool)
        .await
        .unwrap();
    let add_student = |name: &'static str| {
        let (app, admin) = (&app, admin.clone());
        async move {
            let (s, b) = app
                .post(
                    "/students",
                    &admin,
                    json!({ "full_name": name, "father_name": "Father", "date_of_birth": "2015-01-01",
                            "gender": "male", "admission_date": "2026-04-01", "section_id": section }),
                )
                .await;
            assert_eq!(s, StatusCode::CREATED, "{b}");
            b["student_number"].as_str().unwrap().to_string()
        }
    };
    let sync = || async {
        let (s, st) = app.post("/nexus/link/sync", &admin, json!({})).await;
        assert_eq!(s, StatusCode::OK, "{st}");
        st
    };
    let dir = std::env::temp_dir().join(format!("madrasa-restore-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let (s, b) = app
        .call(
            Method::PUT,
            "/backup/settings",
            Some(&admin),
            Some(json!({ "enabled": false, "time": "02:00", "directory": dir.display().to_string(), "keep": 10 })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let backup = || async {
        let (s, run) = app.post("/backup/run", &admin, json!({})).await;
        assert_eq!(s, StatusCode::OK, "{run}");
        run["file_path"].as_str().unwrap().to_string()
    };

    // ---- not joined: no warning, nothing to confirm -------------------------------------------
    let before_joining = backup().await;
    let (s, p) = app
        .post("/backup/preview", &admin, json!({ "path": before_joining }))
        .await;
    assert_eq!((s, &p["network"]), (StatusCode::OK, &Value::Null), "{p}");
    let (s, r) = app
        .post(
            "/backup/restore",
            &admin,
            json!({ "path": before_joining, "password": "backup-password-1" }),
        )
        .await;
    assert_eq!(
        s,
        StatusCode::OK,
        "a school that is not joined restores as before: {r}"
    );
    assert_eq!(r["network"], Value::Null);
    let admin = app.login("ayesha", "admin-password-1").await;

    // ---- join, sync, back up, add records, sync ------------------------------------------------
    let code = hub.invite(&hub_admin).await;
    let (s, b) = app
        .post(
            "/nexus/link/join",
            &admin,
            json!({ "hub_url": hub.url, "join_code": code }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let st = sync().await;
    assert_eq!(
        (st["pending"].as_i64(), st["in_step"].as_bool()),
        (Some(0), Some(true)),
        "{st}"
    );
    let older = backup().await;
    let bilal = add_student("Bilal Ahmed").await;
    let hamza = add_student("Hamza Iqbal").await;
    sync().await;
    assert_eq!(
        hub.in_school("SELECT full_name FROM students ORDER BY full_name")
            .await,
        ["Ali Khan", "Bilal Ahmed", "Hamza Iqbal", "Sara Noor"]
    );

    // ---- restoring while joined: a warning, and a confirmation is required -------------------
    let (s, p) = app
        .post("/backup/preview", &admin, json!({ "path": older }))
        .await;
    assert_eq!(
        (s, p["network"].as_str()),
        (StatusCode::OK, Some("Gulshan Education Network")),
        "{p}"
    );
    let (s, e) = app
        .post(
            "/backup/restore",
            &admin,
            json!({ "path": older, "password": "backup-password-1" }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str(), e["error"]["message"].as_str()),
        (
            StatusCode::CONFLICT,
            Some("restore_while_joined"),
            Some("This school is joined to a network. Restoring an older backup will make your data differ from head office's copy. Madrasa will ask head office to reset its copy.")
        )
    );
    assert_eq!(
        app.names().await.len(),
        4,
        "nothing was restored without the confirmation"
    );
    let (s, r) = app
        .post(
            "/backup/restore",
            &admin,
            json!({ "path": older, "password": "backup-password-1", "confirm_network": true }),
        )
        .await;
    assert_eq!(
        (s, r["network"].as_str()),
        (StatusCode::OK, Some("Gulshan Education Network")),
        "{r}"
    );
    let admin = app.login("ayesha", "admin-password-1").await;
    assert_eq!(
        app.names().await,
        ["Ali Khan", "Sara Noor"],
        "the older backup is back"
    );

    // Still joined, with the key it had; everything queued again; head office not told yet,
    // so the screen does not say "in step".
    let (_, st) = app.get("/nexus/link", &admin).await;
    assert_eq!(st["linked"], true, "{st}");
    assert!(st["pending"].as_i64().unwrap() > 0, "{st}");
    assert!(st["restored"]["at"].is_string(), "{st}");
    assert!(st["restored"]["hub_reset_at"].is_null(), "{st}");
    assert_eq!(st["in_step"], false);

    // ---- the next sync: head office sets its copy aside and starts afresh ---------------------
    let st = sync().await;
    assert!(st["last_error"].is_null(), "{st}");
    assert!(st["restored"]["hub_reset_at"].is_string(), "{st}");
    let archive = st["restored"]["archive"].as_str().unwrap().to_string();
    assert!(archive.starts_with("a_"), "{archive}");
    // The round wrote the school's own log entry about it; one more round sends that too.
    let st = sync().await;
    assert_eq!(
        (
            st["pending"].as_i64(),
            st["parked"].as_array().map(Vec::len),
            st["parked_at_hub"].as_array().map(Vec::len)
        ),
        (Some(0), Some(0), Some(0)),
        "{st}"
    );
    assert_eq!(st["in_step"], true, "{st}");

    // Record for record, every synced table holds the same number of rows on both sides.
    let tables: Vec<&str> = madrasa_core::sync::TABLES
        .iter()
        .filter(|p| p.is_synced())
        .map(|p| p.table)
        .collect();
    let unsent: Vec<String> = sqlx::query_scalar(
        "SELECT action || ' ' || entity FROM audit_log WHERE school_id IS NULL ORDER BY at",
    )
    .fetch_all(&app.db.pool)
    .await
    .unwrap();
    assert!(
        unsent.iter().all(|a| a.ends_with(" setting")),
        "only instance-wide settings are left out: {unsent:?}"
    );
    let here = school_counts(&app, &tables).await;
    let there = hub.counts(&tables).await;
    let differ: Vec<(&str, i64, i64)> = tables
        .iter()
        .zip(here.iter().zip(&there))
        .filter(|(_, (a, b))| a != b)
        .map(|(t, (a, b))| (*t, *a, *b))
        .collect();
    assert!(
        differ.is_empty(),
        "school and head office differ: {differ:?}"
    );
    assert_eq!(
        hub.in_school("SELECT full_name FROM students ORDER BY full_name")
            .await,
        ["Ali Khan", "Sara Noor"]
    );

    // Head office kept its old copy as an archive and logged it.
    let archived: Vec<(String, i64)> = sqlx::query_as(
        "SELECT schema_name, (SELECT count(*) FROM hub_school_archives) FROM hub_school_archives",
    )
    .fetch_all(&hub.state.pool)
    .await
    .unwrap();
    assert_eq!(archived, [(archive.clone(), 1)]);
    let old_names: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT full_name FROM \"{archive}\".students ORDER BY full_name"
    )))
    .fetch_all(&hub.state.pool)
    .await
    .unwrap();
    assert_eq!(
        old_names,
        ["Ali Khan", "Bilal Ahmed", "Hamza Iqbal", "Sara Noor"]
    );
    let logged: i64 =
        sqlx::query_scalar("SELECT count(*) FROM hub_audit_log WHERE action = 'school_restored'")
            .fetch_one(&hub.state.pool)
            .await
            .unwrap();
    assert_eq!(logged, 1);
    // And the school logged both steps: the restore that kept the link, and head office's reset.
    let school_log: Vec<String> = sqlx::query_scalar(
        "SELECT new_value::text FROM audit_log WHERE entity = 'nexus_link' AND action = 'update' ORDER BY at, id",
    )
    .fetch_all(&app.db.pool)
    .await
    .unwrap();
    assert!(
        school_log.iter().any(|v| v.contains("\"restored\": true")),
        "{school_log:?}"
    );
    assert!(
        school_log.iter().any(|v| v.contains("\"hub_reset\": true")),
        "{school_log:?}"
    );

    // ---- no clash: the restored school issues the same student numbers again --------------------
    let again = add_student("Danish Malik").await;
    assert_eq!(
        again, bilal,
        "the restored counter gives out the same number again"
    );
    let again2 = add_student("Eman Tariq").await;
    assert_eq!(again2, hamza);
    let st = sync().await;
    let st = if st["pending"].as_i64() == Some(0) {
        st
    } else {
        sync().await
    };
    assert_eq!(
        (
            st["pending"].as_i64(),
            st["parked_at_hub"].as_array().map(Vec::len),
            st["in_step"].as_bool()
        ),
        (Some(0), Some(0), Some(true)),
        "{st}"
    );
    assert_eq!(
        hub.in_school("SELECT full_name FROM students ORDER BY full_name")
            .await,
        ["Ali Khan", "Danish Malik", "Eman Tariq", "Sara Noor"]
    );
    let here = school_counts(&app, &tables).await;
    let there = hub.counts(&tables).await;
    assert_eq!(here, there, "{tables:?}");

    std::fs::remove_dir_all(&dir).ok();
    hub.finish().await;
    app.finish().await;
}
