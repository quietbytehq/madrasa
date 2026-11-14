//! Parent app stage 1, the school's side: notices and conversations with guardians. Who may
//! do what, the class-teacher rule, and that nothing works until the school has joined a
//! hub. Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use madrasa_api::{router, AppState};
use madrasa_core::parent_app::Side;
use madrasa_core::sync::Origin;
use madrasa_db::nexus_link::{self, Link};
use madrasa_db::testing::TestDb;
use madrasa_db::{parent_app, Actor};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

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
    async fn ok(&self, p: &str, t: &str, b: Value) -> Value {
        let (s, v) = self.post(p, t, b).await;
        assert!(s.is_success(), "{p}: {s} {v}");
        v
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
    /// A member of staff with a login in the given role; returns (staff id, token).
    async fn staff(&self, admin: &str, name: &str, username: &str, role: &str) -> (Value, String) {
        let staff = self.ok("/staff", admin, json!({ "full_name": name })).await;
        self.ok(
            &format!("/staff/{}/login", staff["id"].as_str().unwrap()),
            admin,
            json!({ "username": username, "password": "staff-password-1", "role": role }),
        )
        .await;
        (
            staff["id"].clone(),
            self.login(username, "staff-password-1").await,
        )
    }
    /// A student in a section with one guardian; returns (student id, guardian id).
    async fn family(
        &self,
        admin: &str,
        section: &Value,
        child: &str,
        parent: &str,
        roll: i32,
    ) -> (Uuid, Uuid) {
        let student = self
            .ok(
                "/students",
                admin,
                json!({ "full_name": child, "father_name": parent, "date_of_birth": "2015-01-01", "gender": "male",
                        "admission_date": "2026-04-01", "section_id": section, "roll_no": roll }),
            )
            .await;
        let id: Uuid = student["id"].as_str().unwrap().parse().unwrap();
        self.ok(
            &format!("/students/{id}/guardians"),
            admin,
            json!({ "full_name": parent, "relationship": "father", "phone": "0300-1111111", "is_primary": true }),
        )
        .await;
        let guardian: Uuid =
            sqlx::query_scalar("SELECT guardian_id FROM student_guardians WHERE student_id = $1")
                .bind(id)
                .fetch_one(&self.db.pool)
                .await
                .unwrap();
        (id, guardian)
    }
    /// As the hub does when a parent writes in the app.
    async fn guardian_says(
        &self,
        school: Uuid,
        student: Uuid,
        guardian: Uuid,
        name: &str,
        text: &str,
    ) {
        let mut c = self.db.pool.acquire().await.unwrap();
        parent_app::send_message(
            &mut c,
            &self.state.hlc,
            Actor::system_for(school),
            parent_app::NewMessage {
                school_id: school,
                student_id: student,
                guardian_id: guardian,
                side: Side::Guardian,
                sender_user_id: None,
                sender_name: name.into(),
                body: text.into(),
            },
        )
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn notices_and_conversations_follow_roles_the_class_teacher_rule_and_need_a_hub() {
    let Some(app) = App::new().await else { return };
    let (s, b) = app
        .call(
            Method::POST,
            "/setup",
            None,
            Some(json!({
                "school_name": "Gulshan High", "school_code": "KHI-PA", "academic_year_start_month": 4,
                "admin_full_name": "Ayesha Principal", "admin_username": "ayesha", "admin_password": "admin-password-1",
                "backup_password": "backup-password-1"
            })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let principal = app.login("ayesha", "admin-password-1").await;
    let school: Uuid = sqlx::query_scalar("SELECT id FROM schools")
        .fetch_one(&app.db.pool)
        .await
        .unwrap();
    let year = app
        .ok(
            "/academic-years",
            &principal,
            json!({ "start_year": 2026, "start_month": 4, "activate": true }),
        )
        .await;
    let five = app
        .ok("/classes", &principal, json!({ "name": "5", "level": 5 }))
        .await;
    let six = app
        .ok("/classes", &principal, json!({ "name": "6", "level": 6 }))
        .await;
    let five_a = app
        .ok(
            "/sections",
            &principal,
            json!({ "class_id": five["id"], "academic_year_id": year["id"], "name": "A" }),
        )
        .await;
    let six_a = app
        .ok(
            "/sections",
            &principal,
            json!({ "class_id": six["id"], "academic_year_id": year["id"], "name": "A" }),
        )
        .await;
    let (sana_staff, sana) = app.staff(&principal, "Sana Malik", "sana", "teacher").await;
    let (bilal_staff, bilal) = app
        .staff(&principal, "Bilal Ahmed", "bilal", "teacher")
        .await;
    let (_, office) = app
        .staff(&principal, "Nadia Office", "nadia", "admin")
        .await;
    let (_, accounts) = app
        .staff(&principal, "Omar Accounts", "omar", "accountant")
        .await;
    for (section, teacher) in [(&five_a, &sana_staff), (&six_a, &bilal_staff)] {
        let (s, b) = app
            .call(
                Method::PATCH,
                &format!("/sections/{}", section["id"].as_str().unwrap()),
                Some(&principal),
                Some(json!({ "class_teacher_id": teacher })),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
    }
    let (ali, imran) = app
        .family(&principal, &five_a["id"], "Ali Khan", "Imran Khan", 1)
        .await;
    let (sara, hina) = app
        .family(&principal, &six_a["id"], "Sara Noor", "Hina Noor", 1)
        .await;
    let thread =
        |student: Uuid, guardian: Uuid| format!("/parent-app/conversations/{student}/{guardian}");

    // ---- a school that has not joined a hub: nothing here works, and says why -------------
    for (path, token) in [
        ("/parent-app/notices", &principal),
        ("/parent-app/conversations", &principal),
        ("/parent-app/conversations", &sana),
    ] {
        let (s, e) = app.get(path, token).await;
        assert_eq!(
            (s, e["error"]["code"].as_str()),
            (StatusCode::CONFLICT, Some("not_joined")),
            "{path}"
        );
        assert!(e["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Join one first"));
    }
    let (s, _) = app
        .post(
            "/parent-app/notices",
            &principal,
            json!({ "title": "Too early" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    // Permissions are checked before that: someone without them learns nothing.
    let (s, _) = app.get("/parent-app/notices", &sana).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.get("/parent-app/conversations", &accounts).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // The sidebar's question is answered without an error: there is nothing to show yet.
    for token in [&principal, &sana] {
        let (s, st) = app.get("/parent-app/status", token).await;
        assert_eq!(
            (s, &st),
            (
                StatusCode::OK,
                &json!({ "joined": false, "hub_name": null, "unread": 0 })
            )
        );
    }
    let (s, _) = app.get("/parent-app/status", &accounts).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let nothing: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM notices) + (SELECT count(*) FROM messages)",
    )
    .fetch_one(&app.db.pool)
    .await
    .unwrap();
    assert_eq!(nothing, 0);

    // ---- the school joins (the link is set as joining does) ------------------------------------
    {
        let mut tx = app.db.pool.begin().await.unwrap();
        madrasa_db::sync::enable(&mut tx, school, Origin::School)
            .await
            .unwrap();
        nexus_link::set(
            &mut tx,
            &app.state.hlc,
            Actor::system_for(school),
            school,
            &Link {
                hub_url: "https://nexus.example.org".into(),
                hub_name: "Gulshan Education Network".into(),
                school_ref: Uuid::now_v7(),
                joined_at: chrono::Utc::now(),
                ended: None,
                restored: None,
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }

    // ---- notices: the principal and the office; not teachers, not accounts --------------------
    for token in [&sana, &accounts] {
        let (s, _) = app.get("/parent-app/notices", token).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        let (s, _) = app
            .post(
                "/parent-app/notices",
                token,
                json!({ "title": "Not mine to write" }),
            )
            .await;
        assert_eq!(s, StatusCode::FORBIDDEN);
    }
    for (bad, message) in [
        (json!({ "title": "  " }), "A notice needs a title."),
        (
            json!({ "title": "x".repeat(201) }),
            "The title is too long (at most 200 characters).",
        ),
    ] {
        let (s, e) = app.post("/parent-app/notices", &office, bad).await;
        assert_eq!(
            (s, e["error"]["message"].as_str()),
            (StatusCode::BAD_REQUEST, Some(message))
        );
    }
    let (s, e) = app
        .post(
            "/parent-app/notices",
            &office,
            json!({ "title": "For nobody", "class_ids": [Uuid::now_v7()] }),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{e}");
    let (s, draft) = app
        .post(
            "/parent-app/notices",
            &office,
            json!({ "title": " Sports day ", "body": "Friday.\r\nBring water." }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{draft}");
    assert_eq!(
        (
            draft["title"].as_str(),
            draft["body"].as_str(),
            &draft["published_at"]
        ),
        (
            Some("Sports day"),
            Some("Friday.\nBring water."),
            &Value::Null
        )
    );
    let id = draft["id"].as_str().unwrap();
    let (s, published) = app
        .post(
            &format!("/parent-app/notices/{id}/publish"),
            &principal,
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{published}");
    assert!(published["published_at"].is_string());
    let (s, edited) = app
        .call(Method::PATCH, &format!("/parent-app/notices/{id}"), Some(&office), Some(json!({ "title": "Sports day", "body": "Saturday.", "class_ids": [five["id"]], "expires_on": "2026-12-01" })))
        .await;
    assert_eq!(s, StatusCode::OK, "{edited}");
    assert_eq!(
        (edited["class_ids"].clone(), edited["expires_on"].as_str()),
        (json!([five["id"]]), Some("2026-12-01"))
    );
    assert!(
        edited["published_at"].is_string(),
        "editing does not withdraw it"
    );
    let (s, withdrawn) = app
        .post(
            &format!("/parent-app/notices/{id}/withdraw"),
            &office,
            json!({}),
        )
        .await;
    assert_eq!(
        (s, &withdrawn["published_at"]),
        (StatusCode::OK, &Value::Null)
    );
    let (s, direct) = app
        .post(
            "/parent-app/notices",
            &principal,
            json!({ "title": "Holiday on Monday", "publish": true }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);
    assert!(direct["published_at"].is_string());
    let (s, _) = app
        .call(
            Method::DELETE,
            &format!("/parent-app/notices/{id}"),
            Some(&office),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, list) = app.get("/parent-app/notices", &office).await;
    assert_eq!(
        list.as_array()
            .unwrap()
            .iter()
            .map(|n| n["title"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["Holiday on Monday"]
    );
    let (s, _) = app
        .post(
            &format!("/parent-app/notices/{id}/publish"),
            &office,
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "a removed notice is gone");

    // ---- conversations -------------------------------------------------------------------------
    // Two parents write (through the hub): Ali's father to class 5-A, Sara's mother to 6-A.
    app.guardian_says(
        school,
        ali,
        imran,
        "Imran Khan",
        "Salaam, Ali was unwell today.",
    )
    .await;
    app.guardian_says(school, sara, hina, "Hina Noor", "When is the class trip?")
        .await;

    // Each class teacher sees their own class only; the principal sees both; others nothing.
    let names = |list: &Value| {
        list.as_array()
            .unwrap()
            .iter()
            .map(|c| c["student_name"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    let (s, mine) = app.get("/parent-app/conversations", &sana).await;
    assert_eq!(s, StatusCode::OK, "{mine}");
    assert_eq!(names(&mine), ["Ali Khan"]);
    assert_eq!(
        (
            mine[0]["unread"].as_i64(),
            mine[0]["can_reply"].as_bool(),
            mine[0]["last_preview"].as_str(),
            mine[0]["class_name"].as_str()
        ),
        (
            Some(1),
            Some(true),
            Some("Salaam, Ali was unwell today."),
            Some("5")
        )
    );
    let (_, his) = app.get("/parent-app/conversations", &bilal).await;
    assert_eq!(names(&his), ["Sara Noor"]);
    let (_, all) = app.get("/parent-app/conversations", &principal).await;
    assert_eq!(all.as_array().unwrap().len(), 2);
    assert!(
        all.as_array()
            .unwrap()
            .iter()
            .all(|c| c["can_reply"] == false),
        "the principal reads, does not answer"
    );
    for token in [&office, &accounts] {
        let (s, _) = app.get("/parent-app/conversations", token).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        let (s, _) = app.get(&thread(ali, imran), token).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
    }

    // Another teacher's class is closed: not to read, not to answer.
    let (s, _) = app.get(&thread(ali, imran), &bilal).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app
        .post(
            &format!("{}/messages", thread(ali, imran)),
            &bilal,
            json!({ "body": "Not my class" }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // A student and a guardian who do not belong together are not a conversation.
    let (s, _) = app.get(&thread(ali, hina), &principal).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = app
        .post(
            &format!("{}/messages", thread(ali, hina)),
            &sana,
            json!({ "body": "Wrong parent" }),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // The principal can read a conversation but neither answers nor marks it read.
    let (s, view) = app.get(&thread(ali, imran), &principal).await;
    assert_eq!(s, StatusCode::OK, "{view}");
    assert_eq!(
        (
            view["can_reply"].as_bool(),
            view["teacher_name"].as_str(),
            view["unread"].as_i64(),
            view["messages"].as_array().map(Vec::len)
        ),
        (Some(false), Some("Sana Malik"), Some(1), Some(1))
    );
    for action in ["messages", "read"] {
        let (s, e) = app
            .post(
                &format!("{}/{action}", thread(ali, imran)),
                &principal,
                json!({ "body": "From the principal" }),
            )
            .await;
        assert_eq!(
            (s, e["error"]["code"].as_str()),
            (StatusCode::FORBIDDEN, Some("not_class_teacher")),
            "{action}"
        );
    }

    // The class teacher answers. Empty and overlong messages are refused.
    for (bad, message) in [
        (json!({ "body": " \n " }), "Write a message first."),
        (
            json!({ "body": "x".repeat(2001) }),
            "The message is too long (at most 2000 characters).",
        ),
    ] {
        let (s, e) = app
            .post(&format!("{}/messages", thread(ali, imran)), &sana, bad)
            .await;
        assert_eq!(
            (s, e["error"]["message"].as_str()),
            (StatusCode::BAD_REQUEST, Some(message))
        );
    }
    let (s, sent) = app
        .post(
            &format!("{}/messages", thread(ali, imran)),
            &sana,
            json!({ "body": " Thank you. Please send his notebook tomorrow. " }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{sent}");
    assert_eq!(
        (
            sent["sender"].as_str(),
            sent["sender_name"].as_str(),
            sent["body"].as_str()
        ),
        (
            Some("staff"),
            Some("Sana Malik"),
            Some("Thank you. Please send his notebook tomorrow.")
        )
    );
    let (_, view) = app.get(&thread(ali, imran), &sana).await;
    let said: Vec<(&str, &str)> = view["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| (m["sender"].as_str().unwrap(), m["body"].as_str().unwrap()))
        .collect();
    assert_eq!(
        said,
        [
            ("guardian", "Salaam, Ali was unwell today."),
            ("staff", "Thank you. Please send his notebook tomorrow.")
        ]
    );
    assert_eq!(view["unread"], 0, "answering is reading");
    // A new message from the parent is unread again until the teacher opens and marks it.
    app.guardian_says(school, ali, imran, "Imran Khan", "He will bring it.")
        .await;
    let (_, mine) = app.get("/parent-app/conversations", &sana).await;
    assert_eq!(
        (
            mine[0]["unread"].as_i64(),
            mine[0]["messages"].as_i64(),
            mine[0]["last_sender"].as_str()
        ),
        (Some(1), Some(3), Some("guardian"))
    );
    // The sidebar count: the class teacher's unread messages; the principal, who reads
    // every conversation and answers none of them, has none waiting.
    let (_, st) = app.get("/parent-app/status", &sana).await;
    assert_eq!(
        (st["joined"].as_bool(), st["unread"].as_i64()),
        (Some(true), Some(1)),
        "{st}"
    );
    assert!(st["hub_name"].is_string());
    let (_, st) = app.get("/parent-app/status", &principal).await;
    assert_eq!(
        (st["joined"].as_bool(), st["unread"].as_i64()),
        (Some(true), Some(0))
    );
    let (s, _) = app
        .post(&format!("{}/read", thread(ali, imran)), &sana, json!({}))
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, mine) = app.get("/parent-app/conversations", &sana).await;
    assert_eq!(mine[0]["unread"], 0);
    let (_, st) = app.get("/parent-app/status", &sana).await;
    assert_eq!(st["unread"], 0);

    // A teacher may write first to a guardian of their own class.
    let (s, _) = app
        .post(
            &format!("{}/messages", thread(sara, hina)),
            &bilal,
            json!({ "body": "The trip is on the 20th." }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED);

    // The audit log records that messages were sent, by which side, never what they said.
    let logged: Vec<Value> =
        sqlx::query_scalar("SELECT new_value FROM audit_log WHERE entity = 'message' ORDER BY at")
            .fetch_all(&app.db.pool)
            .await
            .unwrap();
    assert_eq!(logged.len(), 5);
    assert!(
        logged
            .iter()
            .all(|v| v.get("body").is_none() && !v.to_string().contains("notebook")),
        "{logged:?}"
    );
    // Everything written here is waiting to go to the hub.
    let waiting: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT table_name FROM sync_changes ORDER BY 1")
            .fetch_all(&app.db.pool)
            .await
            .unwrap();
    for table in ["messages", "message_reads", "notices"] {
        assert!(
            waiting.iter().any(|t| t == table),
            "{table} is queued: {waiting:?}"
        );
    }

    // When the class changes teacher, the conversation goes with the class.
    let (s, _) = app
        .call(
            Method::PATCH,
            &format!("/sections/{}", five_a["id"].as_str().unwrap()),
            Some(&principal),
            Some(json!({ "class_teacher_id": bilal_staff })),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app.get(&thread(ali, imran), &sana).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (_, his) = app.get("/parent-app/conversations", &bilal).await;
    assert_eq!(his.as_array().unwrap().len(), 2);
    let (s, view) = app.get(&thread(ali, imran), &bilal).await;
    assert_eq!(
        (
            s,
            view["can_reply"].as_bool(),
            view["messages"].as_array().map(Vec::len)
        ),
        (StatusCode::OK, Some(true), Some(3))
    );

    app.db.cleanup().await;
}
