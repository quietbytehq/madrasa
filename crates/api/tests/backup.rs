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

    // Status before any backup on a school set up a moment ago: no backup yet, and none
    // overdue (v0.5.0 warned "older than 2 days" on a brand-new install); unattended ready
    // (setup stored the public key).
    let (s, status) = app.get("/backup/status", &admin).await;
    assert_eq!(
        (
            s,
            status["never"].as_bool(),
            status["stale"].as_bool(),
            status["unattended_ready"].as_bool(),
            status["last_backup_at"].clone()
        ),
        (
            StatusCode::OK,
            Some(true),
            Some(false),
            Some(true),
            Value::Null
        )
    );
    let (_, dash) = app.get("/dashboard", &admin).await;
    assert_eq!(dash["backup"]["never"], true);
    assert_eq!(dash["backup"]["stale"], false);
    // The same school three days after it was set up, still without a backup: overdue.
    sqlx::query("UPDATE schools SET created_at = now() - interval '3 days'")
        .execute(&app.state.pool)
        .await
        .unwrap();
    let (_, status) = app.get("/backup/status", &admin).await;
    assert_eq!(
        (status["never"].as_bool(), status["stale"].as_bool()),
        (Some(true), Some(true))
    );

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
            status["never"].as_bool(),
            status["stale"].as_bool(),
            status["last_backup_file"].as_str()
        ),
        (Some(false), Some(false), Some(path.as_str()))
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
    // Permissions: a teacher cannot touch backups, nor read the status (v0.2.0-alpha B1).
    let (_, teacher) = app
        .user_with_role(&school_id, "sana", RoleCode::Teacher)
        .await;
    let (s, _) = app.post("/backup/run", &teacher, json!({})).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.get("/backup/files", &teacher).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.get("/backup/status", &teacher).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.get("/backup/status", &admin).await;
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

/// Wait for the check that follows "Back up now" and return the status once it is in.
async fn checked(app: &App, admin: &str) -> Value {
    for _ in 0..150 {
        let (_, status) = app.get("/backup/status", admin).await;
        if !status["check"].is_null() {
            return status;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    panic!("the backup was never checked");
}

async fn scratch_databases(app: &App) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM pg_database WHERE datname LIKE current_database() || '\\_check\\_%'",
    )
    .fetch_one(&app.state.pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn a_backup_is_checked_by_restoring_it_into_a_temporary_database() {
    let app = app!();
    let (admin, school_id, ids) = app.seed().await;
    let dir = temp_dir("check");
    app.put("/backup/settings", &admin, json!({ "enabled": false, "time": "02:00", "directory": dir.display().to_string(), "keep": 5 })).await;
    app.student(&admin, "Ali", &ids["s1"]).await;
    app.student(&admin, "Sara", &ids["s1"]).await;

    // "Back up now" answers at once; the check follows and is stored with the run.
    let (s, run) = app.post("/backup/run", &admin, json!({})).await;
    assert_eq!(s, StatusCode::OK, "{run}");
    let status = checked(&app, &admin).await;
    assert_eq!(status["check"], "verified", "{status}");
    let message = status["check_message"].as_str().unwrap();
    assert!(
        message.starts_with("Restored into a temporary database and counted: all "),
        "{message}"
    );
    assert!(status["last_error"].is_null() && status["last_failed_at"].is_null());
    let (_, runs) = app.get("/backup/runs", &admin).await;
    assert_eq!(runs[0]["verify_status"], "verified");
    assert!(!runs[0]["verified_at"].is_null());
    // The temporary database is gone, and the school's own records are untouched.
    assert_eq!(scratch_databases(&app).await, 0);
    let (_, list) = app.get("/students", &admin).await;
    assert_eq!(list["total"], 2);
    let (_, dash) = app.get("/dashboard", &admin).await;
    assert_eq!(dash["backup"]["check"], "verified");

    // A scheduled backup waits for its check: the run comes back with the verdict.
    let sid: uuid::Uuid = school_id.parse().unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let run = madrasa_api::backup::run_backup(&app.state, Actor::system_for(sid), sid, "scheduled")
        .await
        .unwrap();
    assert_eq!(
        (run.status.as_str(), run.verify_status.as_deref()),
        ("ok", Some("verified"))
    );
    assert_eq!(scratch_databases(&app).await, 0);
    std::fs::remove_dir_all(&dir).ok();
    app.finish().await;
}

#[tokio::test]
async fn a_backup_that_cannot_be_restored_whole_fails_its_check() {
    use madrasa_api::backup::{file_matches, verify_dump, Check};
    let app = app!();
    let (admin, _school_id, ids) = app.seed().await;
    app.student(&admin, "Ali", &ids["s1"]).await;
    use madrasa_core::backup::archive;
    // The records as a backup seals them (compressed), and the rows of every table.
    let (plain, counts) = {
        let mut c = app.state.pool.acquire().await.unwrap();
        let mut plain = Vec::new();
        let counts = madrasa_db::backup::dump_into(&mut c, &mut plain)
            .await
            .unwrap();
        (plain, counts)
    };
    assert!(
        counts.len() > 50 && counts["students"] == 1,
        "every table is counted: {counts:?}"
    );
    let dump = archive::compress(&plain).unwrap();

    // As it should be.
    let good = verify_dump(app.state.clone(), dump.clone(), counts.clone()).await;
    assert!(matches!(good, Check::Verified(_)), "{good:?}");

    // The heading promises a student the contents do not hold.
    let mut promised = counts.clone();
    *promised.get_mut("students").unwrap() += 1;
    match verify_dump(app.state.clone(), dump.clone(), promised).await {
        Check::Failed(why) => {
            assert!(why.contains("students: 1 of 2"), "{why}");
            assert!(why.contains("Do not rely on this backup"), "{why}");
        }
        other => panic!("{other:?}"),
    }

    // Contents cut short: the records end half-way, or the compressed stream itself does.
    for cut in [
        archive::compress(&plain[..plain.len() / 2]).unwrap(),
        dump[..dump.len() / 2].to_vec(),
    ] {
        match verify_dump(app.state.clone(), cut, counts.clone()).await {
            Check::Failed(why) => assert!(
                why.contains("could not be restored") || why.contains("missing"),
                "{why}"
            ),
            other => panic!("{other:?}"),
        }
    }
    // A table that is in the school and not in the backup at all.
    let mut extra = counts.clone();
    extra.insert("students_of_another_kind".into(), 3);
    match verify_dump(app.state.clone(), dump.clone(), extra).await {
        Check::Failed(why) => assert!(why.contains("students_of_another_kind: 0 of 3"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        scratch_databases(&app).await,
        0,
        "every temporary database is removed, also after a failure"
    );

    // The file on the disk is not what was written.
    use sha2::{Digest, Sha256};
    let dir = temp_dir("readback");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("x.mbk");
    std::fs::write(&file, b"what was written").unwrap();
    let digest: [u8; 32] = Sha256::digest(b"what was written").into();
    assert!(file_matches(&file, &digest).await.is_ok());
    std::fs::write(&file, b"what was writteN").unwrap();
    let why = file_matches(&file, &digest).await.unwrap_err();
    assert!(
        why.contains("is not what was written") && why.contains("back up to another one"),
        "{why}"
    );
    std::fs::remove_file(&file).unwrap();
    assert!(file_matches(&file, &digest)
        .await
        .unwrap_err()
        .contains("could not be read back"));
    std::fs::remove_dir_all(&dir).ok();
    app.finish().await;
}

#[tokio::test]
async fn a_failed_backup_is_reported_until_the_next_good_one() {
    let app = app!();
    let (admin, _school_id, _ids) = app.seed().await;
    let good = temp_dir("alert-good");
    app.put("/backup/settings", &admin, json!({ "enabled": true, "time": "02:00", "directory": good.display().to_string(), "keep": 5 })).await;
    let (s, _) = app.post("/backup/run", &admin, json!({})).await;
    assert_eq!(s, StatusCode::OK);
    checked(&app, &admin).await;

    // The folder goes away (a USB drive that was pulled): a file stands where it was.
    let blocked = temp_dir("alert-blocked");
    let (s, body) = app.put("/backup/settings", &admin, json!({ "enabled": true, "time": "02:00", "directory": blocked.display().to_string(), "keep": 5 })).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    std::fs::remove_dir_all(&blocked).unwrap();
    std::fs::write(&blocked, b"not a folder").unwrap();
    let (s, body) = app.post("/backup/run", &admin, json!({})).await;
    assert_eq!(s, StatusCode::CONFLICT, "{body}");
    let (_, status) = app.get("/backup/status", &admin).await;
    let why = status["last_error"].as_str().unwrap();
    assert!(why.contains("The backup folder cannot be used"), "{why}");
    assert!(!status["last_failed_at"].is_null());
    // The last good backup is still named, so the alert can say how old it is.
    assert!(!status["last_backup_at"].is_null());
    let (_, dash) = app.get("/dashboard", &admin).await;
    assert_eq!(dash["backup"]["last_error"], status["last_error"]);

    // Fixed: the alert is gone with the next good backup.
    app.put("/backup/settings", &admin, json!({ "enabled": true, "time": "02:00", "directory": good.display().to_string(), "keep": 5 })).await;
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let (s, _) = app.post("/backup/run", &admin, json!({})).await;
    assert_eq!(s, StatusCode::OK);
    let (_, status) = app.get("/backup/status", &admin).await;
    assert!(
        status["last_error"].is_null() && status["last_failed_at"].is_null(),
        "{status}"
    );
    std::fs::remove_file(&blocked).ok();
    std::fs::remove_dir_all(&good).ok();
    app.finish().await;
}
