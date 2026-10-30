//! M1 stage 5 backup API tests. Need PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

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

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "madrasa-backup-test-{name}-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[tokio::test]
async fn backup_restore_round_trip_wrong_password_tampering_and_password_change() {
    let app = app!();
    let (admin, school_id, ids) = app.seed().await;
    let dir = temp_dir("rt");
    let dir_s = dir.display().to_string();

    // Settings: nightly by default; point the folder at a temp dir.
    let (s, st) = app.get("/backup/settings", &admin).await;
    assert_eq!(
        (
            s,
            st["enabled"].as_bool(),
            st["time"].as_str(),
            st["keep"].as_i64()
        ),
        (StatusCode::OK, Some(true), Some("02:00"), Some(30))
    );
    let (s, _) = app
        .put(
            "/backup/settings",
            &admin,
            json!({ "enabled": true, "time": "25:00", "directory": dir_s, "keep": 5 }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, st) = app
        .put(
            "/backup/settings",
            &admin,
            json!({ "enabled": true, "time": "01:30", "directory": dir_s, "keep": 5 }),
        )
        .await;
    assert_eq!(
        (s, st["directory"].as_str()),
        (StatusCode::OK, Some(dir_s.as_str()))
    );

    // Status before any backup: stale, unattended ready (setup stored the public key).
    let (s, status) = app.get("/backup/status", &admin).await;
    assert_eq!(
        (
            s,
            status["stale"].as_bool(),
            status["unattended_ready"].as_bool(),
            status["last_backup_at"].clone()
        ),
        (StatusCode::OK, Some(true), Some(true), Value::Null)
    );
    let (_, dash) = app.get("/dashboard", &admin).await;
    assert_eq!(dash["backup"]["stale"], true);

    // Manual backup writes a file and records the run.
    let ali = app.student(&admin, "Ali", &ids["s1"]).await;
    let (s, run) = app.post("/backup/run", &admin, json!({})).await;
    assert_eq!(s, StatusCode::OK, "{run}");
    assert_eq!(
        (run["status"].as_str(), run["trigger"].as_str()),
        (Some("ok"), Some("manual"))
    );
    let path = run["file_path"].as_str().unwrap().to_string();
    assert!(path.starts_with(&dir_s) && path.ends_with(".mbk"), "{path}");
    assert!(run["file_size"].as_i64().unwrap() > 500);
    let (_, status) = app.get("/backup/status", &admin).await;
    assert_eq!(
        (
            status["stale"].as_bool(),
            status["last_backup_file"].as_str()
        ),
        (Some(false), Some(path.as_str()))
    );
    let (_, files) = app.get("/backup/files", &admin).await;
    assert_eq!(files.as_array().unwrap().len(), 1);
    assert_eq!(files[0]["path"], path);
    let (_, runs) = app.get("/backup/runs", &admin).await;
    assert_eq!(runs.as_array().unwrap().len(), 1);

    // Preview: readable without a key, shows counts and schema match.
    let (s, pv) = app
        .post("/backup/preview", &admin, json!({ "path": path }))
        .await;
    assert_eq!(s, StatusCode::OK, "{pv}");
    assert_eq!(
        (
            pv["meta"]["school_code"].as_str(),
            pv["schema_matches"].as_bool(),
            pv["meta"]["counts"]["students"].as_i64()
        ),
        (Some("KHI-MC"), Some(true), Some(1))
    );
    assert_eq!(pv["current_counts"]["students"], 1);

    // Change the data: a second student. Then restore with a wrong password (audited, 401),
    // then with the right one (data back to one student, audited).
    app.student(&admin, "Sara", &ids["s1"]).await;
    let (_, page) = app.get("/students", &admin).await;
    assert_eq!(page["total"], 2);
    let (s, e) = app
        .post(
            "/backup/restore",
            &admin,
            json!({ "path": path, "password": "wrong-password-1" }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::UNAUTHORIZED, Some("backup_secret_invalid"))
    );
    let (s, _) = app
        .post("/backup/restore", &admin, json!({ "path": path }))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "no secret given");
    let (_, page) = app.get("/students", &admin).await;
    assert_eq!(page["total"], 2, "nothing restored");
    let (s, r) = app
        .post(
            "/backup/restore",
            &admin,
            json!({ "path": path, "password": "backup-password-1" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert!(r["rows"].as_u64().unwrap() > 0);
    assert_eq!(r["relogin"], true);
    // The admin session existed when the backup was made, so it still works.
    let (s, page) = app.get("/students", &admin).await;
    assert_eq!((s, page["total"].as_i64()), (StatusCode::OK, Some(1)));
    assert_eq!(page["items"][0]["id"], ali);
    let (_, audit) = app.get("/audit?limit=50", &admin).await;
    let actions: Vec<&str> = audit["items"]
        .as_array()
        .or_else(|| audit.as_array())
        .unwrap()
        .iter()
        .filter_map(|e| e["action"].as_str())
        .collect();
    assert!(actions.contains(&"restore"), "{actions:?}");
    assert!(actions.contains(&"backup"), "{actions:?}");

    // Tampering: flip a byte in the file; preview and restore refuse it.
    let mut bytes = std::fs::read(&path).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xff;
    let bad_path = dir.join("madrasa-tampered.mbk");
    std::fs::write(&bad_path, &bytes).unwrap();
    let (s, e) = app
        .post(
            "/backup/preview",
            &admin,
            json!({ "path": bad_path.display().to_string() }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("backup_invalid"))
    );
    let (s, _) = app
        .post(
            "/backup/restore",
            &admin,
            json!({ "path": bad_path.display().to_string(), "password": "backup-password-1" }),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    std::fs::write(dir.join("madrasa-not.mbk"), b"hello").unwrap();
    let (s, _) = app
        .post(
            "/backup/preview",
            &admin,
            json!({ "path": dir.join("madrasa-not.mbk").display().to_string() }),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

    // Password change: wrong current password is refused and audited; new backups use the
    // new password; the old file still opens with the old password.
    let (s, _) = app
        .post(
            "/backup/password",
            &admin,
            json!({ "current_password": "nope-nope-nope", "new_password": "new-backup-pw-1" }),
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = app
        .post(
            "/backup/password",
            &admin,
            json!({ "current_password": "backup-password-1", "new_password": "short" }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // A wrong recovery key after the restore, so both failures are in the log.
    let (s, _) = app
        .post(
            "/backup/restore",
            &admin,
            json!({ "path": path, "recovery_key": "AAAA-AAAA-AAAA-AAAA-AAAA-AAAA-AAAA-AAAA" }),
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (_, audit) = app.get("/audit?limit=100", &admin).await;
    let failed = audit["items"]
        .as_array()
        .or_else(|| audit.as_array())
        .unwrap()
        .iter()
        .filter(|e| e["action"] == "backup_password_failed")
        .count();
    assert!(
        failed >= 2,
        "wrong current password and wrong recovery key: {failed}"
    );

    let (s, _) = app
        .post(
            "/backup/password",
            &admin,
            json!({ "current_password": "backup-password-1", "new_password": "new-backup-pw-1" }),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, run2) = app.post("/backup/run", &admin, json!({})).await;
    assert_eq!(s, StatusCode::OK, "{run2}");
    let path2 = run2["file_path"].as_str().unwrap().to_string();
    let (s, _) = app
        .post(
            "/backup/restore",
            &admin,
            json!({ "path": path2, "password": "backup-password-1" }),
        )
        .await;
    assert_eq!(
        s,
        StatusCode::UNAUTHORIZED,
        "old password does not open new backups"
    );
    let (s, _) = app
        .post(
            "/backup/restore",
            &admin,
            json!({ "path": path2, "password": "new-backup-pw-1" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app
        .post(
            "/backup/restore",
            &admin,
            json!({ "path": path, "password": "backup-password-1" }),
        )
        .await;
    assert_eq!(
        s,
        StatusCode::OK,
        "old backup still opens with the old password"
    );
    // Permissions: a teacher cannot touch backups but can read the status for the dashboard.
    let (_, teacher) = app
        .user_with_role(&school_id, "sana", RoleCode::Teacher)
        .await;
    let (s, _) = app.post("/backup/run", &teacher, json!({})).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.get("/backup/files", &teacher).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.get("/backup/status", &teacher).await;
    assert_eq!(s, StatusCode::OK);
    std::fs::remove_dir_all(&dir).ok();
    app.finish().await;
}

#[tokio::test]
async fn pruning_keeps_the_newest_files() {
    let app = app!();
    let (admin, _school_id, _ids) = app.seed().await;
    let dir = temp_dir("prune");
    app.put("/backup/settings", &admin, json!({ "enabled": false, "time": "02:00", "directory": dir.display().to_string(), "keep": 2 })).await;
    for _ in 0..3 {
        let (s, run) = app.post("/backup/run", &admin, json!({})).await;
        assert_eq!(s, StatusCode::OK, "{run}");
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    }
    let (_, files) = app.get("/backup/files", &admin).await;
    assert_eq!(
        files.as_array().unwrap().len(),
        2,
        "only the newest two are kept"
    );
    let (_, status) = app.get("/backup/status", &admin).await;
    assert_eq!(status["enabled"], false);
    std::fs::remove_dir_all(&dir).ok();
    app.finish().await;
}
