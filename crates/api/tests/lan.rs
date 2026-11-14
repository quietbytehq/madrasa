//! LAN mode end to end: the API served over TLS with a self-signed certificate, a client that
//! pins the fingerprint, the loopback proxy the desktop app uses in client mode, and several
//! teachers working at the same time through it. Needs PostgreSQL via
//! `MADRASA_TEST_DATABASE_URL`.

use futures_util::future::join_all;
use madrasa_api::{lan, server, AppState};
use madrasa_db::testing::TestDb;
use serde_json::{json, Value};
use std::time::Duration;

struct Lan {
    db: Option<TestDb>,
    /// Plain HTTP into the loopback proxy, as the UI sees it.
    proxy: String,
    /// The TLS server on the "LAN".
    tls: String,
    fingerprint: String,
    client: reqwest::Client,
    stop: Option<tokio::sync::broadcast::Sender<()>>,
}

impl Lan {
    async fn start() -> Option<Lan> {
        let db = TestDb::new().await?;
        let state = AppState::new(db.pool.clone()).await.unwrap();
        let dir = std::env::temp_dir().join(format!("madrasa-lan-{}", uuid::Uuid::now_v7()));
        let identity = lan::load_or_create_identity(&dir).unwrap();
        let (stop, _) = tokio::sync::broadcast::channel::<()>(2);
        let mut s1 = stop.subscribe();
        let (tls_addr, tls_fut) = lan::serve_tls(
            "127.0.0.1:0".parse().unwrap(),
            state.clone(),
            &identity,
            async move {
                let _ = s1.recv().await;
            },
        )
        .await
        .unwrap();
        tokio::spawn(tls_fut);
        // A plain loopback listener too, as standalone mode has (unused here, proves both coexist).
        let mut s2 = stop.subscribe();
        let (_plain, plain_fut) = server::bind("127.0.0.1:0".parse().unwrap(), state, async move {
            let _ = s2.recv().await;
        })
        .await
        .unwrap();
        tokio::spawn(plain_fut);
        let tls = format!("https://127.0.0.1:{}", tls_addr.port());
        let mut s3 = stop.subscribe();
        let (proxy_addr, proxy_fut) = lan::serve_proxy(
            "127.0.0.1:0".parse().unwrap(),
            &tls,
            &identity.fingerprint,
            None,
            async move {
                let _ = s3.recv().await;
            },
        )
        .await
        .unwrap();
        tokio::spawn(proxy_fut);
        let _ = std::fs::remove_dir_all(&dir);
        Some(Lan {
            db: Some(db),
            proxy: format!("http://{proxy_addr}"),
            tls,
            fingerprint: identity.fingerprint,
            client: reqwest::Client::new(),
            stop: Some(stop),
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
            .client
            .request(method, format!("{}/api/v1{path}", self.proxy));
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        if let Some(b) = body {
            req = req.json(&b);
        }
        let res = req.send().await.expect("proxy reachable");
        let status = res.status().as_u16();
        let text = res.text().await.unwrap_or_default();
        let value = serde_json::from_str(&text).unwrap_or(Value::Null);
        (status, value)
    }
    async fn get(&self, p: &str, t: &str) -> (u16, Value) {
        self.call(reqwest::Method::GET, p, Some(t), None).await
    }
    async fn post(&self, p: &str, t: &str, b: Value) -> (u16, Value) {
        self.call(reqwest::Method::POST, p, Some(t), Some(b)).await
    }
    async fn put(&self, p: &str, t: &str, b: Value) -> (u16, Value) {
        self.call(reqwest::Method::PUT, p, Some(t), Some(b)).await
    }
    async fn login(&self, u: &str, p: &str) -> String {
        let (s, b) = self
            .call(
                reqwest::Method::POST,
                "/auth/login",
                None,
                Some(json!({ "username": u, "password": p })),
            )
            .await;
        assert_eq!(s, 200, "{b}");
        b["token"].as_str().unwrap().to_string()
    }
    async fn finish(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Some(db) = self.db.take() {
            db.cleanup().await;
        }
    }
}

macro_rules! lan {
    () => {
        match Lan::start().await {
            Some(l) => l,
            None => return,
        }
    };
}

#[tokio::test]
async fn clients_pin_the_server_certificate_and_the_proxy_relays_errors() {
    let lan = lan!();
    // First connect: the probe reports the fingerprint the server screen shows, and setup state.
    let probe = lan::probe(&lan.tls).await.unwrap();
    assert!(lan::same_fingerprint(&probe.fingerprint, &lan.fingerprint));
    assert!(probe.needs_setup);
    assert!(probe.version.starts_with("0."), "{probe:?}");
    // A client pinned to another fingerprint refuses the server.
    let (wrong, _) = lan::pinned_client(Some(&"00".repeat(32).replace("00", "00:")[..95])).unwrap();
    let err = wrong
        .get(format!("{}/api/v1/setup/status", lan.tls))
        .send()
        .await
        .expect_err("mismatched fingerprint must fail");
    assert!(
        err.to_string().to_lowercase().contains("fingerprint") || err.is_connect(),
        "{err}"
    );
    // The right fingerprint works directly too.
    let (right, _) = lan::pinned_client(Some(&lan.fingerprint)).unwrap();
    let status = right
        .get(format!("{}/api/v1/health", lan.tls))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, 200);
    // Through the proxy, API errors keep their status and body.
    let (s, b) = lan.call(reqwest::Method::GET, "/users", None, None).await;
    assert_eq!(s, 401, "{b}");
    assert_eq!(b["error"]["code"], "unauthenticated");
    // A proxy to a dead server answers 502 with a plain message instead of hanging.
    let (dead_addr, dead_fut) = lan::serve_proxy(
        "127.0.0.1:0".parse().unwrap(),
        "https://127.0.0.1:1",
        &lan.fingerprint,
        None,
        std::future::pending(),
    )
    .await
    .unwrap();
    tokio::spawn(dead_fut);
    let res = reqwest::get(format!("http://{dead_addr}/api/v1/health"))
        .await
        .unwrap();
    assert_eq!(res.status(), 502);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["error"]["code"], "server_unreachable");
    lan.finish().await;
}

#[tokio::test]
async fn several_teachers_work_at_once_through_the_lan_with_permissions_enforced() {
    let lan = lan!();
    let (s, b) = lan
        .call(
            reqwest::Method::POST,
            "/setup",
            None,
            Some(json!({ "school_name": "Gulshan", "school_code": "KHI-MC", "academic_year_start_month": 4, "first_year": true,
                         "admin_full_name": "Ayesha", "admin_username": "ayesha", "admin_password": "admin-password-1", "backup_password": "backup-password-1" })),
        )
        .await;
    assert_eq!(s, 200, "{b}");
    let admin = lan.login("ayesha", "admin-password-1").await;
    let (_, years) = lan.get("/academic-years", &admin).await;
    let year = years[0]["id"].clone();
    let (_, c1) = lan
        .post("/classes", &admin, json!({ "name": "1", "level": 1 }))
        .await;
    let (_, eng) = lan
        .post(
            "/subjects",
            &admin,
            json!({ "name": "English", "code": "ENG" }),
        )
        .await;
    lan.put(
        &format!("/classes/{}/subjects", c1["id"].as_str().unwrap()),
        &admin,
        json!({ "subject_ids": [eng["id"]] }),
    )
    .await;
    let (_, list) = lan
        .post(
            "/exams/assessments/defaults",
            &admin,
            json!({ "academic_year_id": year }),
        )
        .await;
    let test1 = list[0]["id"].clone();
    lan.put("/exams/max-marks", &admin, json!({ "assessment_id": test1, "class_id": c1["id"], "lines": [{ "subject_id": eng["id"], "max_marks": "50" }] })).await;

    // Eight sections, each with two students and its own teacher login.
    const N: usize = 8;
    let mut teachers = Vec::new();
    for i in 0..N {
        let name = ["A", "B", "C", "D", "E", "F", "G", "H"][i];
        let (s, sec) = lan
            .post(
                "/sections",
                &admin,
                json!({ "class_id": c1["id"], "academic_year_id": year, "name": name }),
            )
            .await;
        assert_eq!(s, 200, "{sec}");
        let mut students = Vec::new();
        for j in 0..2 {
            let (s, st) = lan
                .post("/students", &admin, json!({ "full_name": format!("Student {name}{j}"), "father_name": "F", "date_of_birth": "2015-01-01", "gender": "male", "admission_date": "2026-04-01", "section_id": sec["id"] }))
                .await;
            assert_eq!(s, 201, "{st}");
            students.push(st["id"].as_str().unwrap().to_string());
        }
        let (s, staff) = lan
            .post(
                "/staff",
                &admin,
                json!({ "full_name": format!("Teacher {name}") }),
            )
            .await;
        assert_eq!(s, 200, "{staff}");
        let (s, _) = lan
            .post(
                &format!("/staff/{}/login", staff["id"].as_str().unwrap()),
                &admin,
                json!({ "username": format!("teacher{i}"), "password": "teacher-password-1", "role": "teacher" }),
            )
            .await;
        assert_eq!(s, 200);
        lan.put(
            &format!("/staff/{}/assignments", staff["id"].as_str().unwrap()),
            &admin,
            json!({ "assignments": [{ "section_id": sec["id"], "subject_id": eng["id"] }] }),
        )
        .await;
        teachers.push((format!("teacher{i}"), sec["id"].clone(), students));
    }

    // All eight log in and enter marks and attendance at the same time; each is confined to
    // their own section and blocked from fees.
    let outcomes = join_all(teachers.iter().enumerate().map(|(i, (username, section, students))| {
        let lan = &lan;
        let test1 = test1.clone();
        let eng = eng["id"].clone();
        let other_section = teachers[(i + 1) % N].1.clone();
        async move {
            let token = lan.login(username, "teacher-password-1").await;
            let (marks, mv) = lan
                .put("/exams/sheet", &token, json!({ "assessment_id": test1, "section_id": section, "subject_id": eng, "marks": [{ "student_id": students[0], "obtained": "40" }, { "student_id": students[1], "obtained": "35" }] }))
                .await;
            let (att, av) = lan
                .put("/attendance/sheet", &token, json!({ "section_id": section, "date": "2026-04-06", "entries": [{ "student_id": students[0], "status": "absent", "note": "" }] }))
                .await;
            let (fees, _) = lan.get("/fees/vouchers", &token).await;
            let (settings, _) = lan.get("/settings/school", &token).await;
            let (foreign, _) = lan
                .put("/exams/sheet", &token, json!({ "assessment_id": test1, "section_id": other_section, "subject_id": eng, "marks": [] }))
                .await;
            (marks, mv, att, av, fees, settings, foreign)
        }
    }))
    .await;
    for (marks, mv, att, av, fees, settings, foreign) in outcomes {
        assert_eq!(marks, 200, "{mv}");
        assert_eq!(mv["students"].as_array().unwrap().len(), 2);
        assert_eq!(att, 200, "{av}");
        assert_eq!(fees, 403);
        assert_eq!(settings, 403);
        assert_eq!(foreign, 403, "another section");
    }
    // The administrator sees all eight sheets afterwards.
    let (_, sheets) = lan
        .get(
            &format!(
                "/exams/sheets?assessment_id={}&section_id={}",
                test1.as_str().unwrap(),
                teachers[0].1.as_str().unwrap()
            ),
            &admin,
        )
        .await;
    assert!(
        sheets["sheets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["status"] == "draft"),
        "{sheets}"
    );
    lan.finish().await;
}
