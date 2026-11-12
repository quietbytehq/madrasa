//! Parent app stage 3: what a parent can see at the hub, and above all what they cannot.
//! A parent reaches their own children only: another family's child, a child at another
//! school and a child whose access has ended all answer `404`, on every route, with the
//! same body. Messages written in the app reach the school at its next sync and the
//! teacher's answer comes back. Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.
//!
//! The figures themselves (attendance, report card, fees) are compared with the school's
//! own API in `crates/api/tests/nexus_link.rs`.

mod common;

use axum::body::Body;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use common::{activate, as_school, code_for, guardian, joined, Hub, SchoolSim};
use madrasa_core::parent_app::Side;
use madrasa_db::parent_app::{self, NewMessage, NoticeInput};
use madrasa_db::Actor;
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

async fn student(sim: &SchoolSim, name: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM students WHERE full_name = $1")
        .bind(name)
        .fetch_one(&sim.db.pool)
        .await
        .unwrap()
}

/// Make the school's year the active one and put a student in section A of class 5, with
/// a class teacher. Returns (class id, teacher's staff id).
async fn enrol_with_teacher(sim: &SchoolSim, student_name: &str, teacher: &str) -> (Uuid, Uuid) {
    let hlc = || sim.hlc.now().to_string();
    let pool = &sim.db.pool;
    sqlx::query("UPDATE academic_years SET is_active = TRUE, updated_at = now(), hlc = $1")
        .bind(hlc())
        .execute(pool)
        .await
        .unwrap();
    let year: Uuid = sqlx::query_scalar("SELECT id FROM academic_years")
        .fetch_one(pool)
        .await
        .unwrap();
    let class: Uuid = sqlx::query_scalar("SELECT id FROM classes WHERE name = '5'")
        .fetch_one(pool)
        .await
        .unwrap();
    let staff = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO staff (id, school_id, staff_number, full_name, father_name, gender, designation, joined_on, hlc)
         VALUES ($1, $2, 'T-1', $3, '', 'female', 'Teacher', '2026-04-01', $4)",
    )
    .bind(staff)
    .bind(sim.school.id)
    .bind(teacher)
    .bind(hlc())
    .execute(pool)
    .await
    .unwrap();
    let section = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO sections (id, school_id, class_id, academic_year_id, name, class_teacher_id, hlc)
         VALUES ($1, $2, $3, $4, 'A', $5, $6)",
    )
    .bind(section)
    .bind(sim.school.id)
    .bind(class)
    .bind(year)
    .bind(staff)
    .bind(hlc())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO enrollments (id, school_id, student_id, academic_year_id, section_id, roll_no, hlc)
         SELECT $1, $2, s.id, $3, $4, 7, $5 FROM students s WHERE s.full_name = $6",
    )
    .bind(Uuid::now_v7())
    .bind(sim.school.id)
    .bind(year)
    .bind(section)
    .bind(hlc())
    .bind(student_name)
    .execute(pool)
    .await
    .unwrap();
    (class, staff)
}

async fn notice(sim: &SchoolSim, title: &str, classes: Vec<Uuid>, publish: bool) -> Uuid {
    let mut c = sim.db.pool.acquire().await.unwrap();
    parent_app::create_notice(
        &mut c,
        &sim.hlc,
        Actor::system_for(sim.school.id),
        sim.school.id,
        NoticeInput {
            title: title.into(),
            body: format!("{title}: details."),
            class_ids: classes,
            expires_on: None,
        },
        publish,
    )
    .await
    .unwrap()
    .id
}

/// A signed-in parent: the school gives a code, the parent activates it.
async fn parent(hub: &Hub, sim: &SchoolSim, guardian_id: Uuid, phone: &str) -> String {
    let code = code_for(hub, sim, guardian_id).await;
    let (s, b) = activate(hub, &code, phone, "parent-password-1").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    b["token"].as_str().unwrap().to_string()
}

fn child_path(school: Uuid, student: Uuid, tail: &str) -> String {
    format!("/parent/children/{school}/{student}{tail}")
}

/// Every route that names a child: (method, tail, body).
fn child_routes() -> Vec<(Method, &'static str, Option<Value>)> {
    vec![
        (Method::GET, "", None),
        (Method::GET, "/attendance", None),
        (Method::GET, "/attendance?month=2026-05", None),
        (Method::GET, "/results", None),
        (Method::GET, "/fees", None),
        (Method::GET, "/notices", None),
        (Method::GET, "/messages", None),
        (Method::POST, "/messages", Some(json!({ "body": "Hello" }))),
        (Method::POST, "/messages/read", None),
    ]
}

/// Call every child route and require the same `404` from each.
async fn assert_hidden(hub: &Hub, token: &str, school: Uuid, student: Uuid, why: &str) {
    for (method, tail, body) in child_routes() {
        let (s, b) = hub
            .call(
                method.clone(),
                &child_path(school, student, tail),
                Some(token),
                body,
            )
            .await;
        assert_eq!(
            (s, &b),
            (
                StatusCode::NOT_FOUND,
                &json!({ "error": { "code": "not_found", "message": "The child was not found." } })
            ),
            "{why}: {method} {tail}"
        );
    }
}

async fn titles(hub: &Hub, token: &str, school: Uuid, student: Uuid) -> Vec<String> {
    let (s, b) = hub
        .get(&child_path(school, student, "/notices"), token)
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    b["notices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["title"].as_str().unwrap().to_string())
        .collect()
}

async fn raw(hub: &Hub, req: Request<Body>) -> (StatusCode, HeaderMap) {
    let res = hub.router.clone().oneshot(req).await.unwrap();
    (res.status(), res.headers().clone())
}

#[tokio::test]
async fn a_parent_reaches_their_own_children_and_nobody_elses() {
    let hub = hub!();
    let admin = hub.setup().await;
    let gulshan = joined(&hub, &admin, "Gulshan High", "KHI-01").await;
    let clifton = joined(&hub, &admin, "Clifton Grammar", "KHI-02").await;
    let g = gulshan.school_ref.unwrap();
    let c = clifton.school_ref.unwrap();

    // Gulshan: Imran is Ali's father, Nadia is Sara's mother. Clifton has its own Ali Khan,
    // whose mother is Hina.
    let imran = guardian(&gulshan, "Imran Khan", Some("Ali Khan")).await;
    let nadia = guardian(&gulshan, "Nadia Noor", Some("Sara Noor")).await;
    let hina = guardian(&clifton, "Hina Shah", Some("Ali Khan")).await;
    let (class5, _) = enrol_with_teacher(&gulshan, "Ali Khan", "Sana Malik").await;
    notice(&gulshan, "Sports day", vec![], true).await;
    notice(&gulshan, "Class 5 trip", vec![class5], true).await;
    notice(&gulshan, "Not ready", vec![], false).await;
    let other_class = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO classes (id, school_id, name, level, hlc) VALUES ($1, $2, '6', 6, $3)",
    )
    .bind(other_class)
    .bind(gulshan.school.id)
    .bind(gulshan.hlc.now().to_string())
    .execute(&gulshan.db.pool)
    .await
    .unwrap();
    notice(&gulshan, "Another class only", vec![other_class], true).await;
    let old = notice(&gulshan, "Last term's fair", vec![], true).await;
    sqlx::query("UPDATE notices SET expires_on = '2020-01-01', hlc = $2 WHERE id = $1")
        .bind(old)
        .bind(gulshan.hlc.now().to_string())
        .execute(&gulshan.db.pool)
        .await
        .unwrap();
    gulshan.sync(&hub).await.unwrap();
    clifton.sync(&hub).await.unwrap();

    let ali = student(&gulshan, "Ali Khan").await;
    let sara = student(&gulshan, "Sara Noor").await;
    let ali_clifton = student(&clifton, "Ali Khan").await;
    let imran_t = parent(&hub, &gulshan, imran, "0300-1111111").await;
    let nadia_t = parent(&hub, &gulshan, nadia, "0300-2222222").await;
    let hina_t = parent(&hub, &clifton, hina, "0300-3333333").await;

    // ---- the list of children ----------------------------------------------------------------
    let (s, list) = hub.get("/parent/children", &imran_t).await;
    assert_eq!(s, StatusCode::OK, "{list}");
    let kids = list["children"].as_array().unwrap();
    assert_eq!(kids.len(), 1, "{list}");
    let kid = &kids[0];
    assert_eq!(
        (
            kid["school_ref"].as_str(),
            kid["student_id"].as_str(),
            kid["full_name"].as_str(),
            kid["school_name"].as_str(),
            kid["class_name"].as_str(),
            kid["section_name"].as_str(),
            kid["roll_no"].as_i64(),
            kid["relationship"].as_str(),
            kid["unread"].as_i64(),
        ),
        (
            Some(g.to_string().as_str()),
            Some(ali.to_string().as_str()),
            Some("Ali Khan"),
            Some("Gulshan High"),
            Some("5"),
            Some("A"),
            Some(7),
            Some("father"),
            Some(0),
        ),
        "{kid}"
    );
    assert!(
        kid["synced_at"].is_string(),
        "says when the school last synced"
    );
    assert!(
        kid.get("guardian_id").is_none(),
        "the school's guardian id stays at the hub"
    );
    let (_, list) = hub.get("/parent/children", &nadia_t).await;
    assert_eq!(list["children"].as_array().unwrap().len(), 1);
    assert_eq!(list["children"][0]["full_name"], "Sara Noor");
    assert!(
        list["children"][0]["class_name"].is_null(),
        "not in a section this year"
    );
    let (_, list) = hub.get("/parent/children", &hina_t).await;
    assert_eq!(list["children"][0]["school_name"], "Clifton Grammar");

    // ---- a parent's own child: every route answers ------------------------------------------
    for (who, token, school, child) in [
        ("Imran", &imran_t, g, ali),
        ("Nadia", &nadia_t, g, sara),
        ("Hina", &hina_t, c, ali_clifton),
    ] {
        for (method, tail, body) in child_routes() {
            if method == Method::POST && tail == "/messages" {
                continue; // sending is tested below
            }
            let (s, b) = hub
                .call(
                    method.clone(),
                    &child_path(school, child, tail),
                    Some(token),
                    body,
                )
                .await;
            assert_eq!(s, StatusCode::OK, "{who}: {method} {tail}: {b}");
            if method == Method::GET {
                assert!(
                    b["synced_at"].is_string(),
                    "{who}: {tail} says when the school last synced"
                );
            }
        }
    }
    let (_, o) = hub.get(&child_path(g, ali, ""), &imran_t).await;
    assert_eq!(
        (
            o["full_name"].as_str(),
            o["class_teacher"].as_str(),
            o["currency"].as_str(),
            o["balance"].as_str(),
            o["open_vouchers"].as_i64(),
            o["notices"].as_i64(),
            o["attendance"]["total"].as_i64(),
        ),
        (
            Some("Ali Khan"),
            Some("Sana Malik"),
            Some("PKR"),
            Some("0"),
            Some(0),
            Some(2),
            Some(0)
        ),
        "{o}"
    );
    let (_, a) = hub
        .get(&child_path(g, ali, "/attendance?month=2026-05"), &imran_t)
        .await;
    assert_eq!(
        (a["month"].as_str(), a["days"].as_array().map(Vec::len)),
        (Some("2026-05"), Some(0))
    );
    let (s, e) = hub
        .get(&child_path(g, ali, "/attendance?month=May"), &imran_t)
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("validation"))
    );
    let (_, r) = hub.get(&child_path(g, ali, "/results"), &imran_t).await;
    assert_eq!(
        (
            r["card"]["full_name"].as_str(),
            r["card"]["class_name"].as_str()
        ),
        (Some("Ali Khan"), Some("5")),
        "{r}"
    );

    // ---- notices: published, in date, for the school or for the child's class ---------------
    assert_eq!(
        titles(&hub, &imran_t, g, ali).await,
        ["Class 5 trip", "Sports day"],
        "newest first; no draft, no expired notice, no other class's notice"
    );
    assert_eq!(titles(&hub, &nadia_t, g, sara).await, ["Sports day"]);
    assert!(titles(&hub, &hina_t, c, ali_clifton).await.is_empty());

    // ---- another family's child, another school's child --------------------------------------
    assert_hidden(&hub, &imran_t, g, sara, "another family's child").await;
    assert_hidden(&hub, &nadia_t, g, ali, "another family's child").await;
    assert_hidden(&hub, &imran_t, c, ali_clifton, "a child at another school").await;
    assert_hidden(&hub, &hina_t, g, ali, "a child at another school").await;
    assert_hidden(
        &hub,
        &imran_t,
        c,
        ali,
        "own child named under another school",
    )
    .await;
    assert_hidden(
        &hub,
        &hina_t,
        c,
        ali,
        "another school's child named under own school",
    )
    .await;
    assert_hidden(&hub, &imran_t, g, Uuid::now_v7(), "no such student").await;
    assert_hidden(&hub, &imran_t, Uuid::now_v7(), ali, "no such school").await;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM messages")
        .fetch_one(&gulshan.db.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert!(
        hub.in_school::<i64>(g, "SELECT count(*) FROM messages")
            .await
            == [0],
        "a refused message is not stored"
    );

    // ---- only a parent session opens these routes ---------------------------------------------
    for path in [
        "/parent/children".to_string(),
        child_path(g, ali, ""),
        child_path(g, ali, "/fees"),
    ] {
        for (who, token) in [
            ("nobody", None),
            ("head office", Some(admin.as_str())),
            ("the school", gulshan.token.as_deref()),
            ("a made-up token", Some("not-a-token")),
        ] {
            let (s, _) = hub.call(Method::GET, &path, token, None).await;
            assert_eq!(s, StatusCode::UNAUTHORIZED, "{who} on {path}");
        }
    }

    // ---- the app calls from another origin -----------------------------------------------------
    let prefix = madrasa_nexus::API_PREFIX;
    let (s, h) = raw(
        &hub,
        Request::builder()
            .method(Method::OPTIONS)
            .uri(format!("{prefix}/parent/children"))
            .header(header::ORIGIN, "http://tauri.localhost")
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "authorization")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
    assert!(h[header::ACCESS_CONTROL_ALLOW_HEADERS]
        .to_str()
        .unwrap()
        .contains("authorization"));
    assert!(
        !h.contains_key(header::ACCESS_CONTROL_ALLOW_CREDENTIALS),
        "cookies are never allowed across origins"
    );
    for path in ["/parent/login", "/parent/children"] {
        let (_, h) = raw(
            &hub,
            Request::builder()
                .method(Method::GET)
                .uri(format!("{prefix}{path}"))
                .header(header::ORIGIN, "http://tauri.localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(h[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*", "{path}");
    }
    // Head office's console and the schools' sync routes are not opened to other origins.
    for path in ["/schools", "/auth/login", "/sync/parents"] {
        let (_, h) = raw(
            &hub,
            Request::builder()
                .method(Method::GET)
                .uri(format!("{prefix}{path}"))
                .header(header::ORIGIN, "http://tauri.localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert!(
            !h.contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            "{path}"
        );
    }

    // ---- access that has ended ------------------------------------------------------------------
    // The school removes the guardian from the student in its own records.
    sqlx::query("UPDATE student_guardians SET deleted_at = now(), updated_at = now(), hlc = $2 WHERE guardian_id = $1")
        .bind(nadia)
        .bind(gulshan.hlc.now().to_string())
        .execute(&gulshan.db.pool)
        .await
        .unwrap();
    gulshan.sync(&hub).await.unwrap();
    assert_hidden(&hub, &nadia_t, g, sara, "no longer the student's guardian").await;
    let (_, list) = hub.get("/parent/children", &nadia_t).await;
    assert_eq!(list["children"], json!([]));

    // The school ends the parent's access.
    let (s, _) = hub.get(&child_path(g, ali, ""), &imran_t).await;
    assert_eq!(s, StatusCode::OK);
    let (s, ended) = as_school(
        &hub,
        &gulshan,
        Method::DELETE,
        &format!("/sync/parents/{imran}"),
        None,
    )
    .await;
    assert_eq!((s, ended["ended"].as_bool()), (StatusCode::OK, Some(true)));
    assert_hidden(&hub, &imran_t, g, ali, "access ended by the school").await;
    let (s, list) = hub.get("/parent/children", &imran_t).await;
    assert_eq!((s, &list["children"]), (StatusCode::OK, &json!([])));

    // The school leaves the hub.
    let (s, _) = hub.get(&child_path(c, ali_clifton, "/fees"), &hina_t).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = as_school(
        &hub,
        &clifton,
        Method::POST,
        "/sync/leave",
        Some(json!({ "reason": "Leaving" })),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_hidden(&hub, &hina_t, c, ali_clifton, "the school has left").await;
    let (_, list) = hub.get("/parent/children", &hina_t).await;
    assert_eq!(list["children"], json!([]));

    hub.finish().await;
}

#[tokio::test]
async fn a_parents_message_reaches_the_school_and_the_answer_comes_back() {
    let hub = hub!();
    let admin = hub.setup().await;
    let gulshan = joined(&hub, &admin, "Gulshan High", "KHI-01").await;
    let g = gulshan.school_ref.unwrap();
    let imran = guardian(&gulshan, "Imran Khan", Some("Ali Khan")).await;
    let nadia = guardian(&gulshan, "Nadia Noor", Some("Sara Noor")).await;
    enrol_with_teacher(&gulshan, "Ali Khan", "Sana Malik").await;
    gulshan.sync(&hub).await.unwrap();
    let ali = student(&gulshan, "Ali Khan").await;
    let sara = student(&gulshan, "Sara Noor").await;
    let imran_t = parent(&hub, &gulshan, imran, "0300-1111111").await;
    let nadia_t = parent(&hub, &gulshan, nadia, "0300-2222222").await;
    let thread = child_path(g, ali, "/messages");

    let (s, t) = hub.get(&thread, &imran_t).await;
    assert_eq!(s, StatusCode::OK, "{t}");
    assert_eq!(
        (
            t["class_teacher"].as_str(),
            t["class_name"].as_str(),
            t["section_name"].as_str(),
            t["can_send"].as_bool(),
            t["messages"].as_array().map(Vec::len),
            t["unread"].as_i64(),
        ),
        (
            Some("Sana Malik"),
            Some("5"),
            Some("A"),
            Some(true),
            Some(0),
            Some(0)
        ),
        "{t}"
    );

    // What cannot be sent.
    for (body, why) in [
        (json!({ "body": "   \n " }), "empty"),
        (json!({ "body": "x".repeat(2001) }), "too long"),
    ] {
        let (s, e) = hub.post(&thread, &imran_t, body).await;
        assert_eq!(
            (s, e["error"]["code"].as_str()),
            (StatusCode::BAD_REQUEST, Some("validation")),
            "{why}"
        );
    }
    // Sara is in no section, so there is no class teacher to write to.
    let (_, t) = hub.get(&child_path(g, sara, "/messages"), &nadia_t).await;
    assert_eq!(
        (t["can_send"].as_bool(), t["class_teacher"].as_str()),
        (Some(false), None)
    );
    let (s, e) = hub
        .post(
            &child_path(g, sara, "/messages"),
            &nadia_t,
            json!({ "body": "Hello" }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("no_class_teacher")),
        "{e}"
    );

    // The parent writes; the school has it after its next sync.
    let (s, sent) = hub
        .post(
            &thread,
            &imran_t,
            json!({ "body": "  Ali has a fever today.  " }),
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
            Some("guardian"),
            Some("Imran Khan"),
            Some("Ali has a fever today.")
        )
    );
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM messages")
        .fetch_one(&gulshan.db.pool)
        .await
        .unwrap();
    assert_eq!(before, 0, "not at the school until it syncs");
    gulshan.sync(&hub).await.unwrap();
    let at_school: Vec<(String, String, String, Uuid, Uuid)> = sqlx::query_as(
        "SELECT sender, sender_name, body, student_id, guardian_id FROM messages ORDER BY sent_at",
    )
    .fetch_all(&gulshan.db.pool)
    .await
    .unwrap();
    assert_eq!(
        at_school,
        [(
            "guardian".to_string(),
            "Imran Khan".to_string(),
            "Ali has a fever today.".to_string(),
            ali,
            imran
        )]
    );
    // The school's audit log has the event, never the text.
    let audit: Vec<String> = sqlx::query_scalar(
        "SELECT coalesce(new_value::text, '') FROM audit_log WHERE entity = 'message'",
    )
    .fetch_all(&gulshan.db.pool)
    .await
    .unwrap();
    assert_eq!(audit.len(), 1);
    assert!(!audit[0].contains("fever"), "{audit:?}");

    // The class teacher answers at the school; the parent sees it after the next sync.
    {
        let mut conn = gulshan.db.pool.acquire().await.unwrap();
        let unread = parent_app::unread(&mut conn, ali, imran, Side::Staff)
            .await
            .unwrap();
        assert_eq!(unread, 1, "the teacher has one unread message");
        parent_app::send_message(
            &mut conn,
            &gulshan.hlc,
            Actor::system_for(gulshan.school.id),
            NewMessage {
                school_id: gulshan.school.id,
                student_id: ali,
                guardian_id: imran,
                side: Side::Staff,
                sender_user_id: sqlx::query_scalar("SELECT id FROM users LIMIT 1")
                    .fetch_one(&gulshan.db.pool)
                    .await
                    .unwrap(),
                sender_name: "Sana Malik".into(),
                body: "Thank you, I hope he feels better soon.".into(),
            },
        )
        .await
        .unwrap();
    }
    let (_, t) = hub.get(&thread, &imran_t).await;
    assert_eq!(
        t["messages"].as_array().unwrap().len(),
        1,
        "not at the hub until the school syncs"
    );
    gulshan.sync(&hub).await.unwrap();
    let (_, t) = hub.get(&thread, &imran_t).await;
    let bodies: Vec<&str> = t["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["body"].as_str().unwrap())
        .collect();
    assert_eq!(
        bodies,
        [
            "Ali has a fever today.",
            "Thank you, I hope he feels better soon."
        ],
        "oldest first"
    );
    assert_eq!(
        (
            t["messages"][1]["sender"].as_str(),
            t["messages"][1]["sender_name"].as_str(),
            t["unread"].as_i64()
        ),
        (Some("staff"), Some("Sana Malik"), Some(1))
    );
    assert!(
        t["messages"][1].get("sender_user_id").is_none(),
        "no school login ids in the app"
    );
    let (_, list) = hub.get("/parent/children", &imran_t).await;
    assert_eq!(list["children"][0]["unread"], 1);

    // Reading it is the parent's own mark, and the school learns of it.
    let (s, mark) = hub
        .post(&child_path(g, ali, "/messages/read"), &imran_t, json!({}))
        .await;
    assert_eq!(
        (s, mark["unread"].as_i64()),
        (StatusCode::OK, Some(0)),
        "{mark}"
    );
    let (_, t) = hub.get(&thread, &imran_t).await;
    assert_eq!(t["unread"], 0);
    gulshan.sync(&hub).await.unwrap();
    let marks: Vec<String> = sqlx::query_scalar(
        "SELECT reader FROM message_reads WHERE deleted_at IS NULL ORDER BY reader",
    )
    .fetch_all(&gulshan.db.pool)
    .await
    .unwrap();
    assert_eq!(
        marks,
        ["guardian"],
        "the parent's mark only: the teacher has not opened it"
    );

    // No flood: a limit per hour and child.
    for i in 0..19 {
        let (s, b) = hub
            .post(&thread, &imran_t, json!({ "body": format!("Message {i}") }))
            .await;
        assert_eq!(s, StatusCode::CREATED, "{i}: {b}");
    }
    let (s, e) = hub
        .post(&thread, &imran_t, json!({ "body": "One more" }))
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::TOO_MANY_REQUESTS, Some("too_many_messages")),
        "{e}"
    );
    gulshan.sync(&hub).await.unwrap();
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM messages")
        .fetch_one(&gulshan.db.pool)
        .await
        .unwrap();
    assert_eq!(total, 21, "20 from the parent, one from the teacher");

    hub.finish().await;
}
