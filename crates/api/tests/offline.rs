//! Working offline on a LAN client (ADR 0009), end to end: the real API over TLS, the desktop
//! client's loopback proxy with its outbox, and "another PC" talking to the same server.
//!
//! The school server is unplugged in the middle of entering work, the client app is restarted
//! while it is still out of reach, the server comes back, and every count is checked exactly:
//! nothing lost, nothing applied twice, conflicts settled by the sync rules and shown.
//! Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

use madrasa_api::{lan, server, AppState};
use madrasa_db::testing::TestDb;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

struct Net {
    db: Option<TestDb>,
    state: AppState,
    identity: lan::Identity,
    tls_port: u16,
    tls: Option<(broadcast::Sender<()>, JoinHandle<()>)>,
    /// Another PC on the LAN, straight to the server (plain HTTP on loopback).
    other: String,
    other_stop: broadcast::Sender<()>,
    proxy: String,
    proxy_task: Option<(broadcast::Sender<()>, JoinHandle<()>)>,
    outbox: PathBuf,
    http: reqwest::Client,
    dir: PathBuf,
}

impl Net {
    async fn start() -> Option<Net> {
        lan::init();
        let db = TestDb::new().await?;
        let state = AppState::new(db.pool.clone()).await.unwrap();
        let dir = std::env::temp_dir().join(format!("madrasa-offline-{}", uuid::Uuid::now_v7()));
        let identity = lan::load_or_create_identity(&dir.join("id")).unwrap();
        let (other_stop, _) = broadcast::channel::<()>(2);
        let mut rx = other_stop.subscribe();
        let (other_addr, other_fut) =
            server::bind("127.0.0.1:0".parse().unwrap(), state.clone(), async move {
                let _ = rx.recv().await;
            })
            .await
            .unwrap();
        tokio::spawn(other_fut);
        let mut net = Net {
            db: Some(db),
            state,
            identity,
            tls_port: 0,
            tls: None,
            other: format!("http://{other_addr}"),
            other_stop,
            proxy: String::new(),
            proxy_task: None,
            outbox: dir.join("outbox"),
            http: reqwest::Client::new(),
            dir,
        };
        net.plug().await;
        net.open_app().await;
        Some(net)
    }

    /// The school server's network cable goes in (on the same address every time).
    async fn plug(&mut self) {
        let (stop, _) = broadcast::channel::<()>(2);
        let mut rx = stop.subscribe();
        let (addr, fut) = lan::serve_tls(
            format!("127.0.0.1:{}", self.tls_port).parse().unwrap(),
            self.state.clone(),
            &self.identity,
            async move {
                let _ = rx.recv().await;
            },
        )
        .await
        .unwrap();
        self.tls_port = addr.port();
        let task = tokio::spawn(async move {
            let _ = fut.await;
        });
        self.tls = Some((stop, task));
    }

    /// ... and comes out.
    async fn unplug(&mut self) {
        if let Some((stop, task)) = self.tls.take() {
            let _ = stop.send(());
            let _ = tokio::time::timeout(Duration::from_secs(5), task).await;
        }
    }

    /// Start the desktop app's proxy (client mode) on this PC's outbox.
    async fn open_app(&mut self) {
        let (stop, _) = broadcast::channel::<()>(2);
        let mut rx = stop.subscribe();
        let (addr, fut) = lan::serve_proxy(
            "127.0.0.1:0".parse().unwrap(),
            &format!("https://127.0.0.1:{}", self.tls_port),
            &self.identity.fingerprint,
            Some(&self.outbox),
            async move {
                let _ = rx.recv().await;
            },
        )
        .await
        .unwrap();
        self.proxy = format!("http://{addr}");
        let task = tokio::spawn(async move {
            let _ = fut.await;
        });
        self.proxy_task = Some((stop, task));
    }

    /// Close the app (or it crashes): the proxy stops.
    async fn close_app(&mut self) {
        if let Some((stop, task)) = self.proxy_task.take() {
            let _ = stop.send(());
            let _ = tokio::time::timeout(Duration::from_secs(5), task).await;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    async fn call(
        &self,
        base: &str,
        method: reqwest::Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (u16, Value, reqwest::header::HeaderMap) {
        let mut req = self.http.request(method, format!("{base}/api/v1{path}"));
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        if let Some(b) = body {
            req = req.json(&b);
        }
        let res = req.send().await.expect("reachable");
        let status = res.status().as_u16();
        let headers = res.headers().clone();
        let text = res.text().await.unwrap_or_default();
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::Null),
            headers,
        )
    }

    /// Through the app (the proxy).
    async fn app(&self, m: reqwest::Method, p: &str, t: &str, b: Option<Value>) -> (u16, Value) {
        let (s, v, _) = self.call(&self.proxy, m, p, Some(t), b).await;
        (s, v)
    }

    /// From the other PC.
    async fn pc2(&self, m: reqwest::Method, p: &str, t: &str, b: Option<Value>) -> (u16, Value) {
        let (s, v, _) = self.call(&self.other, m, p, Some(t), b).await;
        (s, v)
    }

    async fn status(&self) -> Value {
        self.http
            .get(format!("{}/__madrasa/outbox/status", self.proxy))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    async fn count(&self, sql: &'static str) -> i64 {
        sqlx::query_scalar::<_, i64>(sql)
            .fetch_one(&self.state.pool)
            .await
            .unwrap()
    }

    async fn finish(mut self) {
        self.close_app().await;
        self.unplug().await;
        let _ = self.other_stop.send(());
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Some(db) = self.db.take() {
            db.cleanup().await;
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

use reqwest::Method as M;

async fn login(net: &Net, base: &str) -> String {
    let (s, b, _) = net
        .call(
            base,
            M::POST,
            "/auth/login",
            None,
            Some(json!({ "username": "ayesha", "password": "admin-password-1" })),
        )
        .await;
    assert_eq!(s, 200, "{b}");
    b["token"].as_str().unwrap().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unplugged_mid_entry_restarted_and_reconnected_nothing_lost_nothing_twice() {
    let Some(mut net) = Net::start().await else {
        return;
    };

    // ---- a school, through the app, while connected ---------------------------------------
    let (s, b, _) = net
        .call(
            &net.proxy,
            M::POST,
            "/setup",
            None,
            Some(json!({
            "school_name": "Gulshan", "school_code": "KHI-MC", "academic_year_start_month": 4,
            "admin_full_name": "Ayesha", "admin_username": "ayesha",
            "admin_password": "admin-password-1", "backup_password": "backup-password-1" })),
        )
        .await;
    assert_eq!(s, 200, "{b}");
    let admin = login(&net, &net.proxy).await;
    // The app reads who is signed in, as the UI does at start.
    assert_eq!(net.app(M::GET, "/auth/me", &admin, None).await.0, 200);
    let (_, year) = net
        .app(
            M::POST,
            "/academic-years",
            &admin,
            Some(json!({ "start_year": 2026, "start_month": 4, "activate": true })),
        )
        .await;
    let (_, class) = net
        .app(
            M::POST,
            "/classes",
            &admin,
            Some(json!({ "name": "1", "level": 1 })),
        )
        .await;
    let (_, section) = net
        .app(
            M::POST,
            "/sections",
            &admin,
            Some(json!({ "class_id": class["id"], "academic_year_id": year["id"], "name": "A" })),
        )
        .await;
    let (_, head) = net
        .app(
            M::POST,
            "/fees/heads",
            &admin,
            Some(json!({ "name": "Tuition", "code": "TUI" })),
        )
        .await;
    let (s, b) = net
        .app(M::PUT, "/fees/structure", &admin, Some(json!({ "academic_year_id": year["id"], "class_id": class["id"], "lines": [{ "fee_head_id": head["id"], "amount": "2500" }] })))
        .await;
    assert_eq!(s, 200, "{b}");
    let mut ids = Vec::new();
    for name in ["Ali Khan", "Sara Ahmed"] {
        let (s, st) = net
            .app(M::POST, "/students", &admin, Some(json!({ "full_name": name, "father_name": "F", "date_of_birth": "2015-01-01", "gender": "male", "admission_date": "2026-04-01", "section_id": section["id"] })))
            .await;
        assert_eq!(s, 201, "{st}");
        ids.push(st["id"].as_str().unwrap().to_string());
    }
    let (ali, sara) = (ids[0].clone(), ids[1].clone());
    let (s, b) = net
        .app(
            M::POST,
            "/fees/vouchers/generate",
            &admin,
            Some(json!({ "month": "2026-04-01", "issue_date": "2026-04-01" })),
        )
        .await;
    assert_eq!((s, b["created"].as_u64()), (200, Some(2)), "{b}");
    let (_, list) = net.app(M::GET, "/fees/vouchers", &admin, None).await;
    let vouchers: Vec<String> = list["items"]
        .as_array()
        .or(list.as_array())
        .unwrap()
        .iter()
        .map(|v| v["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(vouchers.len(), 2);
    // The screens the user has open: Ali's profile, both vouchers, the student list.
    for p in [
        format!("/students/{ali}"),
        format!("/fees/vouchers/{}", vouchers[0]),
        format!("/fees/vouchers/{}", vouchers[1]),
        "/students".to_string(),
    ] {
        assert_eq!(net.app(M::GET, &p, &admin, None).await.0, 200);
    }
    let receipts_before = net.count("SELECT count(*) FROM fee_receipts").await;
    assert_eq!(receipts_before, 0);
    let st = net.status().await;
    assert_eq!(
        (st["link"].as_str(), st["waiting"].as_u64()),
        (Some("connected"), Some(0))
    );

    // ---- the cable comes out in the middle of the work -------------------------------------
    net.unplug().await;
    let (s, cached, h) = net
        .call(
            &net.proxy,
            M::GET,
            &format!("/students/{ali}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, 200, "screens keep showing what they showed");
    assert_eq!(h.get("x-madrasa-offline").unwrap(), "cached");
    assert_eq!(cached["full_name"], "Ali Khan");
    assert_ne!(net.status().await["link"], "connected");
    // Signing in needs the server: refused plainly, never queued.
    let (s, b, _) = net
        .call(
            &net.proxy,
            M::POST,
            "/auth/login",
            None,
            Some(json!({ "username": "ayesha", "password": "admin-password-1" })),
        )
        .await;
    assert_eq!(
        (s, b["error"]["code"].as_str()),
        (503, Some("server_unreachable"))
    );

    // 1. A new admission, and the same one again (Save pressed twice).
    let new_student = json!({ "full_name": "Bilal Offline", "father_name": "F", "date_of_birth": "2015-02-02", "gender": "male", "admission_date": "2026-04-01", "section_id": section["id"] });
    let (s, q1) = net
        .app(M::POST, "/students", &admin, Some(new_student.clone()))
        .await;
    assert_eq!(s, 202, "{q1}");
    let (s, q2) = net
        .app(M::POST, "/students", &admin, Some(new_student))
        .await;
    assert_eq!(s, 202);
    assert_eq!(q1["queued"]["id"], q2["queued"]["id"], "kept once");
    // 2. An edit of Ali (his name, and his address).
    let (s, _) = net
        .app(
            M::PATCH,
            &format!("/students/{ali}"),
            &admin,
            Some(json!({ "full_name": "Ali Raza Khan", "address": "12 New Road" })),
        )
        .await;
    assert_eq!(s, 202);
    // 3. Two fee payments: provisional receipts at once.
    let (s, p1) = net
        .app(
            M::POST,
            &format!("/fees/vouchers/{}/payments", vouchers[0]),
            &admin,
            Some(json!({ "amount": "1000", "method": "cash" })),
        )
        .await;
    assert_eq!(s, 202, "{p1}");
    let prov1 = p1["queued"]["provisional"]["provisional_no"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(madrasa_db::fees::is_provisional_no(&prov1), "{prov1}");
    assert_eq!(
        p1["queued"]["provisional"]["voucher"]["id"],
        vouchers[0].as_str(),
        "printable"
    );
    let (_, again) = net
        .app(
            M::POST,
            &format!("/fees/vouchers/{}/payments", vouchers[0]),
            &admin,
            Some(json!({ "amount": "1000", "method": "cash" })),
        )
        .await;
    assert_eq!(
        again["queued"]["provisional"]["provisional_no"],
        prov1.as_str(),
        "the same payment twice is one receipt"
    );
    let (_, p2) = net
        .app(
            M::POST,
            &format!("/fees/vouchers/{}/payments", vouchers[1]),
            &admin,
            Some(json!({ "amount": "1000", "method": "cash" })),
        )
        .await;
    let prov2 = p2["queued"]["provisional"]["provisional_no"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(prov1, prov2);
    // 4. Attendance for the class: Ali absent.
    let (s, _) = net
        .app(M::PUT, "/attendance/sheet", &admin, Some(json!({ "section_id": section["id"], "date": "2026-04-06", "entries": [{ "student_id": ali, "status": "absent", "note": "" }] })))
        .await;
    assert_eq!(s, 202);
    let st = net.status().await;
    assert_eq!(st["waiting"], 5, "{st}");

    // Meanwhile the office PC, still connected, works on the same records.
    let office = login(&net, &net.other).await;
    let (s, b) = net
        .pc2(
            M::PATCH,
            &format!("/students/{ali}"),
            &office,
            Some(json!({ "address": "7 Office Street", "father_name": "Raza Khan" })),
        )
        .await;
    assert_eq!(s, 200, "{b}");
    let (_, v2) = net
        .pc2(
            M::GET,
            &format!("/fees/vouchers/{}", vouchers[1]),
            &office,
            None,
        )
        .await;
    let (s, b) = net
        .pc2(
            M::POST,
            &format!("/fees/vouchers/{}/payments", vouchers[1]),
            &office,
            Some(json!({ "amount": v2["balance"], "method": "cash" })),
        )
        .await;
    assert_eq!(s, 201, "{b}");
    tokio::time::sleep(Duration::from_millis(20)).await;
    let (s, b) = net
        .pc2(M::PUT, "/attendance/sheet", &office, Some(json!({ "section_id": section["id"], "date": "2026-04-06", "entries": [{ "student_id": sara, "status": "late", "note": "" }] })))
        .await;
    assert_eq!(s, 200, "{b}");

    // ---- the app is closed (or crashes) and opened again, still offline ---------------------
    net.close_app().await;
    for sub in ["queue", "cache"] {
        for f in std::fs::read_dir(net.outbox.join(sub)).unwrap().flatten() {
            let text = String::from_utf8_lossy(&std::fs::read(f.path()).unwrap()).to_string();
            assert!(
                !text.contains("Bilal") && !text.contains("Ali Khan"),
                "sealed on disk"
            );
        }
    }
    net.open_app().await;
    let st = net.status().await;
    assert_eq!(st["waiting"], 5, "nothing lost in the restart: {st}");
    assert_eq!(st["items"][2]["provisional_no"], prov1.as_str());
    let (s, cached, _) = net
        .call(&net.proxy, M::GET, "/students", Some(&admin), None)
        .await;
    assert_eq!(s, 200, "{cached}");
    let receipt: Value = net
        .http
        .get(format!("{}/__madrasa/outbox/receipts/{prov1}", net.proxy))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        (receipt["amount"].as_str(), receipt["receipt_no"].as_null()),
        (Some("1000"), Some(()))
    );
    assert_eq!(
        net.count("SELECT count(*) FROM fee_receipts").await,
        1,
        "only the office's"
    );

    // ---- the cable goes back in ------------------------------------------------------------
    net.plug().await;
    let mut st = Value::Null;
    for _ in 0..150 {
        st = net.status().await;
        if st["waiting"] == 0 && st["link"] == "connected" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(
        (st["waiting"].as_u64(), st["link"].as_str()),
        (Some(0), Some("connected")),
        "{st}"
    );
    assert_eq!(st["failed"].as_array().unwrap().len(), 0, "{st}");
    assert_eq!(st["sent"].as_array().unwrap().len(), 5, "{st}");

    // Exact counts: one new student, three receipts (the office's and our two), no doubles.
    assert_eq!(net.count("SELECT count(*) FROM students").await, 3);
    assert_eq!(
        net.count("SELECT count(*) FROM students WHERE full_name = 'Bilal Offline'")
            .await,
        1
    );
    assert_eq!(net.count("SELECT count(*) FROM fee_receipts").await, 3);
    assert_eq!(
        net.count("SELECT count(*) FROM fee_receipts WHERE provisional_no IS NOT NULL")
            .await,
        2
    );
    assert_eq!(
        net.count("SELECT count(DISTINCT receipt_no) FROM fee_receipts")
            .await,
        3
    );
    assert_eq!(net.count("SELECT count(*) FROM attendance_sheets").await, 1);
    // The provisional numbers were replaced by real ones, and the real receipts keep them.
    let real: String =
        sqlx::query_scalar("SELECT receipt_no FROM fee_receipts WHERE provisional_no = $1")
            .bind(&prov1)
            .fetch_one(&net.state.pool)
            .await
            .unwrap();
    assert!(
        real.starts_with("R2026-") || real.starts_with('R'),
        "{real}"
    );
    let sent = st["sent"].as_array().unwrap();
    assert!(
        sent.iter()
            .any(|s| s["provisional_no"] == prov1.as_str() && s["receipt_no"] == real.as_str()),
        "{st}"
    );
    let receipt: Value = net
        .http
        .get(format!("{}/__madrasa/outbox/receipts/{prov1}", net.proxy))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(receipt["receipt_no"], real.as_str());

    // Conflicts, by the sync rules: Ali's name (changed only here) is ours; his father's name
    // (only at the office) is the office's; his address (changed on both) is the server's, and
    // ours is shown as overwritten.
    let (name, father, address): (String, String, String) =
        sqlx::query_as("SELECT full_name, father_name, address FROM students WHERE id = $1::uuid")
            .bind(&ali)
            .fetch_one(&net.state.pool)
            .await
            .unwrap();
    assert_eq!(
        (name.as_str(), father.as_str(), address.as_str()),
        ("Ali Raza Khan", "Raza Khan", "7 Office Street")
    );
    let over = st["overwritten"].as_array().unwrap();
    assert!(
        over.iter().any(|o| o["field"] == "address"
            && o["yours"] == "12 New Road"
            && o["kept"] == "7 Office Street"),
        "{st}"
    );
    // Attendance: the office's sheet was saved after Ali was marked here, so its marks stand
    // (Ali present, Sara late) and both differences are shown.
    let marks: Vec<(String, String)> = sqlx::query_as(
        "SELECT s.full_name, e.status FROM attendance_entries e JOIN students s ON s.id = e.student_id
          WHERE e.deleted_at IS NULL AND e.status <> 'present' ORDER BY 1",
    )
    .fetch_all(&net.state.pool)
    .await
    .unwrap();
    assert_eq!(marks, vec![("Sara Ahmed".to_string(), "late".to_string())]);
    assert_eq!(
        over.iter().filter(|o| o["field"] == "status").count(),
        2,
        "{st}"
    );
    // The office had paid voucher 2 in full: our receipt still stands (money was taken) and
    // the overpayment is flagged.
    let notices = st["notices"].as_array().unwrap();
    assert!(
        notices
            .iter()
            .any(|n| n["code"] == "overpaid" && n["detail"]["provisional_no"] == prov2.as_str()),
        "{st}"
    );

    // A change sent again with the same key is not applied twice.
    let key = uuid::Uuid::now_v7().to_string();
    let send = |key: String| {
        let net = &net;
        let section = section["id"].clone();
        let office = office.clone();
        async move {
            net.http
                .post(format!("{}/api/v1/students", net.other))
                .bearer_auth(office)
                .header("idempotency-key", key)
                .json(&json!({ "full_name": "Once Only", "father_name": "F", "date_of_birth": "2015-03-03", "gender": "female", "admission_date": "2026-04-01", "section_id": section }))
                .send()
                .await
                .unwrap()
        }
    };
    let first = send(key.clone()).await;
    assert_eq!(first.status(), 201);
    let first_body: Value = first.json().await.unwrap();
    let second = send(key.clone()).await;
    assert_eq!(second.status(), 201);
    assert_eq!(second.headers().get("idempotent-replay").unwrap(), "true");
    let second_body: Value = second.json().await.unwrap();
    assert_eq!(first_body["id"], second_body["id"]);
    assert_eq!(
        net.count("SELECT count(*) FROM students WHERE full_name = 'Once Only'")
            .await,
        1
    );
    // The same key with different contents is refused.
    let r = net
        .http
        .post(format!("{}/api/v1/students", net.other))
        .bearer_auth(&office)
        .header("idempotency-key", key)
        .json(&json!({ "full_name": "Someone Else", "father_name": "F", "date_of_birth": "2015-03-03", "gender": "female", "admission_date": "2026-04-01", "section_id": section["id"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 422);

    // The user reads and clears what was overwritten.
    let cleared: Value = net
        .http
        .post(format!("{}/__madrasa/outbox/dismiss", net.proxy))
        .json(&json!({ "kind": "overwritten" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(cleared["overwritten"].as_array().unwrap().len(), 0);

    net.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_provisional_number_only_comes_with_a_change_made_offline() {
    let Some(net) = Net::start().await else {
        return;
    };
    let (s, _, _) = net
        .call(
            &net.other,
            M::POST,
            "/setup",
            None,
            Some(json!({
            "school_name": "Gulshan", "school_code": "KHI-MC", "academic_year_start_month": 4,
            "admin_full_name": "Ayesha", "admin_username": "ayesha",
            "admin_password": "admin-password-1", "backup_password": "backup-password-1" })),
        )
        .await;
    assert_eq!(s, 200);
    let admin = login(&net, &net.other).await;
    let v = uuid::Uuid::now_v7();
    // Online, a provisional number is refused before anything is looked up.
    let (s, b) = net
        .pc2(
            M::POST,
            &format!("/fees/vouchers/{v}/payments"),
            &admin,
            Some(json!({ "amount": "10", "method": "cash", "provisional_no": "P-ABCD-0001" })),
        )
        .await;
    assert_eq!(
        (s, b["error"]["code"].as_str()),
        (400, Some("validation")),
        "{b}"
    );
    // A malformed queued time is refused rather than taken as online.
    let r = net
        .http
        .post(format!("{}/api/v1/fees/vouchers/{v}/payments", net.other))
        .bearer_auth(&admin)
        .header("x-madrasa-queued-at", "yesterday")
        .json(&json!({ "amount": "10", "method": "cash" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400);
    net.finish().await;
}

/// Setup, a class with one section and two students, and a teacher login for the section.
/// Returns (admin token, teacher token, section id, [student ids]).
async fn small_school(net: &Net) -> (String, String, Value, Vec<String>) {
    let (s, _, _) = net
        .call(
            &net.other,
            M::POST,
            "/setup",
            None,
            Some(json!({
            "school_name": "Gulshan", "school_code": "KHI-MC", "academic_year_start_month": 4,
            "admin_full_name": "Ayesha", "admin_username": "ayesha",
            "admin_password": "admin-password-1", "backup_password": "backup-password-1" })),
        )
        .await;
    assert_eq!(s, 200);
    let admin = login(net, &net.other).await;
    let (_, year) = net
        .pc2(
            M::POST,
            "/academic-years",
            &admin,
            Some(json!({ "start_year": 2026, "start_month": 4, "activate": true })),
        )
        .await;
    let (_, class) = net
        .pc2(
            M::POST,
            "/classes",
            &admin,
            Some(json!({ "name": "1", "level": 1 })),
        )
        .await;
    let (_, section) = net
        .pc2(
            M::POST,
            "/sections",
            &admin,
            Some(json!({ "class_id": class["id"], "academic_year_id": year["id"], "name": "A" })),
        )
        .await;
    let (_, subject) = net
        .pc2(
            M::POST,
            "/subjects",
            &admin,
            Some(json!({ "name": "English", "code": "ENG" })),
        )
        .await;
    let mut students = Vec::new();
    for name in ["Ali Khan", "Sara Ahmed"] {
        let (s, st) = net
            .pc2(M::POST, "/students", &admin, Some(json!({ "full_name": name, "father_name": "F", "date_of_birth": "2015-01-01", "gender": "male", "admission_date": "2026-04-01", "section_id": section["id"] })))
            .await;
        assert_eq!(s, 201, "{st}");
        students.push(st["id"].as_str().unwrap().to_string());
    }
    let (_, staff) = net
        .pc2(
            M::POST,
            "/staff",
            &admin,
            Some(json!({ "full_name": "Teacher A" })),
        )
        .await;
    let staff_id = staff["id"].as_str().unwrap().to_string();
    let (s, b) = net
        .pc2(M::POST, &format!("/staff/{staff_id}/login"), &admin, Some(json!({ "username": "teacher1", "password": "teacher-password-1", "role": "teacher" })))
        .await;
    assert_eq!(s, 200, "{b}");
    net.pc2(M::PUT, &format!("/staff/{staff_id}/assignments"), &admin, Some(json!({ "assignments": [{ "section_id": section["id"], "subject_id": subject["id"] }] })))
        .await;
    let (s, b, _) = net
        .call(
            &net.other,
            M::POST,
            "/auth/login",
            None,
            Some(json!({ "username": "teacher1", "password": "teacher-password-1" })),
        )
        .await;
    assert_eq!(s, 200, "{b}");
    let teacher = b["token"].as_str().unwrap().to_string();
    (admin, teacher, section["id"].clone(), students)
}

async fn queued_sheet(
    net: &Net,
    token: &str,
    at: &str,
    section: &Value,
    entries: Value,
) -> reqwest::Response {
    net.http
        .put(format!("{}/api/v1/attendance/sheet", net.other))
        .bearer_auth(token)
        .header("x-madrasa-queued-at", at)
        .header("idempotency-key", uuid::Uuid::now_v7().to_string())
        .json(&json!({ "section_id": section, "date": "2026-04-06", "entries": entries }))
        .send()
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_queued_sheet_never_lets_a_teacher_change_marks_already_saved() {
    let Some(net) = Net::start().await else {
        return;
    };
    let (admin, teacher, section, students) = small_school(&net).await;
    let marked_offline = (chrono::Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
    // The administrator saves the sheet now: after the teacher's offline marks.
    let (s, b) = net
        .pc2(M::PUT, "/attendance/sheet", &admin, Some(json!({ "section_id": section, "date": "2026-04-06", "entries": [{ "student_id": students[1], "status": "late", "note": "" }] })))
        .await;
    assert_eq!(s, 200, "{b}");
    // The teacher's queued sheet arrives: answered, but the later sheet stands and the
    // teacher's differing mark is reported.
    let r = queued_sheet(
        &net,
        &teacher,
        &marked_offline,
        &section,
        json!([{ "student_id": students[0], "status": "absent", "note": "" }]),
    )
    .await;
    assert_eq!(r.status(), 200);
    let over: Vec<madrasa_api::offline::Overwritten> = madrasa_api::offline::decode(
        &axum::http::HeaderValue::from_bytes(
            r.headers().get("x-madrasa-overwritten").unwrap().as_bytes(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(over.len(), 2, "{over:?}");
    assert!(over
        .iter()
        .any(|o| o.label.contains("Ali Khan") && o.yours == "absent" && o.kept == "present"));
    let absent = net
        .count("SELECT count(*) FROM attendance_entries WHERE status = 'absent' AND deleted_at IS NULL")
        .await;
    assert_eq!(absent, 0, "the saved marks did not change");
    // A queued sheet claiming to be newer than the saved one is refused, as online: a
    // teacher cannot correct a submitted sheet, offline or not.
    let r = queued_sheet(
        &net,
        &teacher,
        &chrono::Utc::now().to_rfc3339(),
        &section,
        json!([{ "student_id": students[0], "status": "absent", "note": "" }]),
    )
    .await;
    assert_eq!(r.status(), 403);
    let body: Value = r.json().await.unwrap();
    assert_eq!(body["error"]["code"], "already_submitted");
    // With the right to correct, a mark made after the saved one stands (the later mark).
    let r = queued_sheet(&net, &admin, &chrono::Utc::now().to_rfc3339(), &section, json!([{ "student_id": students[0], "status": "absent", "note": "" }, { "student_id": students[1], "status": "late", "note": "" }])).await;
    assert_eq!(r.status(), 200);
    assert!(r.headers().get("x-madrasa-overwritten").is_none());
    let absent = net
        .count("SELECT count(*) FROM attendance_entries WHERE status = 'absent' AND deleted_at IS NULL")
        .await;
    assert_eq!(absent, 1);
    net.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_change_queued_under_an_expired_session_waits_for_the_same_person_to_sign_in() {
    let Some(mut net) = Net::start().await else {
        return;
    };
    let (admin, teacher, _section, students) = small_school(&net).await;
    let _ = teacher;
    // Through the app: signed in, the profile open.
    let token = login(&net, &net.proxy).await;
    assert_eq!(net.app(M::GET, "/auth/me", &token, None).await.0, 200);
    assert_eq!(
        net.app(M::GET, &format!("/students/{}", students[0]), &token, None)
            .await
            .0,
        200
    );
    net.unplug().await;
    let (s, _) = net
        .app(
            M::PATCH,
            &format!("/students/{}", students[0]),
            &token,
            Some(json!({ "address": "1 Queue Lane" })),
        )
        .await;
    assert_eq!(s, 202);
    // The session ends while the server is away (expired, or signed out elsewhere).
    sqlx::query("UPDATE sessions SET revoked_at = now()")
        .execute(&net.state.pool)
        .await
        .unwrap();
    net.plug().await;
    let mut st = Value::Null;
    for _ in 0..100 {
        st = net.status().await;
        if st["needs_sign_in"] == true {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(
        (st["needs_sign_in"].as_bool(), st["waiting"].as_u64()),
        (Some(true), Some(1)),
        "{st}"
    );
    // Someone else signing in on this PC does not send it under their name.
    let (s, b, _) = net
        .call(
            &net.proxy,
            M::POST,
            "/auth/login",
            None,
            Some(json!({ "username": "teacher1", "password": "teacher-password-1" })),
        )
        .await;
    assert_eq!(s, 200, "{b}");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(net.status().await["waiting"], 1);
    // The same person signs in again: it goes, under their name.
    let _again = login(&net, &net.proxy).await;
    for _ in 0..100 {
        st = net.status().await;
        if st["waiting"] == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(st["waiting"], 0, "{st}");
    let (address, by): (String, uuid::Uuid) =
        sqlx::query_as("SELECT address, updated_by FROM students WHERE id = $1::uuid")
            .bind(&students[0])
            .fetch_one(&net.state.pool)
            .await
            .unwrap();
    let admin_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'ayesha'")
        .fetch_one(&net.state.pool)
        .await
        .unwrap();
    assert_eq!((address.as_str(), by), ("1 Queue Lane", admin_id));
    let _ = admin;
    net.finish().await;
}
