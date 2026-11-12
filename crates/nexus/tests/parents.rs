//! Parent app stage 2: parent accounts at the hub. A school asks for one-time access codes;
//! a parent activates one, signs in and out; the three kinds of token (parent, console,
//! school) never open each other's routes. Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

mod common;

use axum::http::{Method, StatusCode};
use common::{Hub, SchoolSim};
use serde_json::{json, Value};
use uuid::Uuid;

/// A guardian in a school's own records, with one child (or none).
async fn guardian(sim: &SchoolSim, name: &str, child: Option<&str>) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO guardians (id, school_id, full_name, phone, hlc) VALUES ($1, $2, $3, '0300-0000000', $4)")
        .bind(id)
        .bind(sim.school.id)
        .bind(name)
        .bind(sim.hlc.now().to_string())
        .execute(&sim.db.pool)
        .await
        .unwrap();
    if let Some(child) = child {
        sqlx::query(
            "INSERT INTO student_guardians (id, school_id, student_id, guardian_id, relationship, is_primary, hlc)
             SELECT $1, $2, s.id, $3, 'father', TRUE, $4 FROM students s WHERE s.full_name = $5",
        )
        .bind(Uuid::now_v7())
        .bind(sim.school.id)
        .bind(id)
        .bind(sim.hlc.now().to_string())
        .bind(child)
        .execute(&sim.db.pool)
        .await
        .unwrap();
    }
    id
}

async fn joined(hub: &Hub, admin: &str, name: &str, code: &str) -> SchoolSim {
    let mut sim = SchoolSim::new(name, code).await.unwrap();
    let join_code = hub.invite(admin, name).await;
    let (s, b) = sim.join(hub, &join_code).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    sim
}

async fn as_school(
    hub: &Hub,
    sim: &SchoolSim,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    hub.call(method, path, sim.token.as_deref(), body).await
}

/// The school asks for a code for a guardian and gets it back in clear, once.
async fn code_for(hub: &Hub, sim: &SchoolSim, guardian: Uuid) -> String {
    let (s, b) = as_school(
        hub,
        sim,
        Method::POST,
        "/sync/parents/codes",
        Some(json!({ "guardian_id": guardian })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    b["code"].as_str().unwrap().to_string()
}

async fn activate(hub: &Hub, code: &str, phone: &str, password: &str) -> (StatusCode, Value) {
    hub.call(
        Method::POST,
        "/parent/activate",
        None,
        Some(json!({ "code": code, "phone": phone, "password": password })),
    )
    .await
}

async fn links(hub: &Hub, token: &str) -> i64 {
    let (s, me) = hub.get("/parent/me", token).await;
    assert_eq!(s, StatusCode::OK, "{me}");
    me["links"].as_i64().unwrap()
}

#[tokio::test]
async fn a_school_opens_access_with_a_code_and_the_parent_activates_it() {
    let hub = hub!();
    let admin = hub.setup().await;
    let gulshan = joined(&hub, &admin, "Gulshan High", "KHI-01").await;
    let imran = guardian(&gulshan, "Imran Khan", Some("Ali Khan")).await;
    let childless = guardian(&gulshan, "No Child", None).await;

    // The hub must have the guardian (the school syncs first) and the guardian a student.
    let (s, e) = as_school(
        &hub,
        &gulshan,
        Method::POST,
        "/sync/parents/codes",
        Some(json!({ "guardian_id": imran })),
    )
    .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::NOT_FOUND, Some("guardian_unknown")),
        "not synced yet"
    );
    gulshan.sync(&hub).await.unwrap();
    for unknown in [childless, Uuid::now_v7()] {
        let (s, _) = as_school(
            &hub,
            &gulshan,
            Method::POST,
            "/sync/parents/codes",
            Some(json!({ "guardian_id": unknown })),
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }
    // Only a school asks: no token, a console session and a made-up token are all refused.
    for token in [None, Some(admin.as_str()), Some("not-a-token")] {
        let (s, _) = hub
            .call(
                Method::POST,
                "/sync/parents/codes",
                token,
                Some(json!({ "guardian_id": imran })),
            )
            .await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{token:?}");
    }

    let (s, issued) = as_school(
        &hub,
        &gulshan,
        Method::POST,
        "/sync/parents/codes",
        Some(json!({ "guardian_id": imran })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{issued}");
    let first = issued["code"].as_str().unwrap().to_string();
    assert_eq!(
        (first.len(), issued["guardian_name"].as_str()),
        (14, Some("Imran Khan"))
    );
    // A second code replaces the first.
    let code = code_for(&hub, &gulshan, imran).await;
    let (s, list) = as_school(&hub, &gulshan, Method::GET, "/sync/parents", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(list.as_array().map(Vec::len), Some(1), "{list}");
    assert_eq!(
        (
            list[0]["guardian_id"].clone(),
            list[0]["state"].as_str(),
            &list[0]["phone_hint"]
        ),
        (json!(imran), Some("code"), &Value::Null)
    );

    // The app shows whose code it is before anything is created.
    for bad in ["", "ABCD-EFGH-JKMN", &first] {
        let (s, e) = hub
            .call(
                Method::POST,
                "/parent/activate/check",
                None,
                Some(json!({ "code": bad })),
            )
            .await;
        assert_eq!(
            (s, e["error"]["code"].as_str()),
            (StatusCode::FORBIDDEN, Some("bad_access_code")),
            "{bad:?}"
        );
    }
    let sloppy = code.to_lowercase().replace('-', " ");
    let (s, check) = hub
        .call(
            Method::POST,
            "/parent/activate/check",
            None,
            Some(json!({ "code": sloppy })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{check}");
    assert_eq!(
        check,
        json!({ "guardian_name": "Imran Khan", "school_name": "Gulshan High", "children": ["Ali Khan"], "existing_phone_hint": null })
    );

    // Activating: a real phone number and a real password are needed; the code is not
    // spent by a refusal.
    for (phone, password, message) in [
        (
            "12",
            "parent-password-1",
            "Enter the whole phone number (7 to 15 digits).",
        ),
        (
            "0300-1234567",
            "short",
            "Password: password must be at least 10 characters.",
        ),
    ] {
        let (s, e) = activate(&hub, &code, phone, password).await;
        assert_eq!(
            (s, e["error"]["message"].as_str()),
            (StatusCode::BAD_REQUEST, Some(message))
        );
    }
    let (s, session) = activate(&hub, &code, "0300-123 4567", "parent-password-1").await;
    assert_eq!(s, StatusCode::OK, "{session}");
    let token = session["token"].as_str().unwrap().to_string();
    assert_eq!(
        (
            session["parent"]["phone"].as_str(),
            session["parent"]["full_name"].as_str()
        ),
        (Some("03001234567"), Some("Imran Khan"))
    );
    assert!(session["parent"].get("password_hash").is_none());
    // A code works once.
    let (s, e) = activate(&hub, &code, "0311-7654321", "another-password-1").await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("bad_access_code"))
    );
    assert_eq!(links(&hub, &token).await, 1);

    // The school sees that access is open, with only a hint of the phone number.
    let (_, list) = as_school(&hub, &gulshan, Method::GET, "/sync/parents", None).await;
    assert_eq!(
        (
            list[0]["state"].as_str(),
            list[0]["phone_hint"].as_str(),
            list[0]["since"].is_string(),
            list[0]["last_login_at"].is_string()
        ),
        (Some("active"), Some("•••• 4567"), true, true)
    );
    assert!(!list.to_string().contains("03001234567"));

    // ---- three kinds of token, three separate doors ---------------------------------------
    // A parent session opens nothing in the console or the school sync API.
    for path in [
        "/users",
        "/schools",
        "/audit",
        "/escrow",
        "/reports",
        "/auth/me",
        "/sync/status",
        "/sync/parents",
    ] {
        let (s, _) = hub.get(path, &token).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{path} with a parent session");
    }
    let (s, _) = hub
        .call(
            Method::POST,
            "/sync/pull",
            Some(&token),
            Some(json!({ "limit": 10 })),
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    // A console session and a school's sync token open nothing for parents.
    for other in [admin.as_str(), gulshan.token.as_deref().unwrap()] {
        let (s, _) = hub.get("/parent/me", other).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        let (s, _) = hub.post("/parent/logout", other, json!({})).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
    }

    // ---- signing in and out ------------------------------------------------------------------
    let login = |phone: &'static str, password: &'static str| {
        hub.call(
            Method::POST,
            "/parent/login",
            None,
            Some(json!({ "phone": phone, "password": password })),
        )
    };
    let (s, e) = login("0300 1234567", "wrong-password-1").await;
    assert_eq!(
        (s, e["error"]["message"].as_str()),
        (
            StatusCode::UNAUTHORIZED,
            Some("The phone number or password is incorrect.")
        )
    );
    let (s, e) = login("0399 0000000", "parent-password-1").await;
    assert_eq!(
        (s, e["error"]["message"].as_str()),
        (
            StatusCode::UNAUTHORIZED,
            Some("The phone number or password is incorrect.")
        ),
        "an unknown number reads the same"
    );
    let (s, second) = login("0300-1234567", "parent-password-1").await;
    assert_eq!(s, StatusCode::OK, "{second}");
    let second = second["token"].as_str().unwrap().to_string();
    let (s, _) = hub.post("/parent/logout", &token, json!({})).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = hub.get("/parent/me", &token).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "signed out");
    assert_eq!(
        links(&hub, &second).await,
        1,
        "the other phone stays signed in"
    );

    // Changing the password signs every phone out and gives this one a new session.
    let (s, e) = hub
        .post(
            "/parent/password",
            &second,
            json!({ "current": "wrong-password-1", "new": "changed-password-1" }),
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "{e}");
    let (s, changed) = hub
        .post(
            "/parent/password",
            &second,
            json!({ "current": "parent-password-1", "new": "changed-password-1" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{changed}");
    let (s, _) = hub.get("/parent/me", &second).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let token = changed["token"].as_str().unwrap().to_string();
    assert_eq!(links(&hub, &token).await, 1);

    // ---- a forgotten password: the school issues a new code for the same guardian ----------
    let again = code_for(&hub, &gulshan, imran).await;
    let (_, check) = hub
        .call(
            Method::POST,
            "/parent/activate/check",
            None,
            Some(json!({ "code": again })),
        )
        .await;
    assert_eq!(check["existing_phone_hint"], "•••• 4567");
    let (s, reset) = activate(&hub, &again, "0300-1234567", "brand-new-password-1").await;
    assert_eq!(s, StatusCode::OK, "{reset}");
    let (s, _) = hub.get("/parent/me", &token).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "the old sessions are gone");
    let (s, _) = login("0300-1234567", "changed-password-1").await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, fresh) = login("0300-1234567", "brand-new-password-1").await;
    assert_eq!(s, StatusCode::OK);
    let token = fresh["token"].as_str().unwrap().to_string();
    assert_eq!(links(&hub, &token).await, 1, "still one link, not two");

    // ---- a second school on the same account -----------------------------------------------------
    let clifton = joined(&hub, &admin, "Clifton Grammar", "KHI-02").await;
    let imran_at_clifton = guardian(&clifton, "Imran Khan", Some("Sara Noor")).await;
    let hina = guardian(&clifton, "Hina Noor", Some("Ali Khan")).await;
    clifton.sync(&hub).await.unwrap();
    let second_school = code_for(&hub, &clifton, imran_at_clifton).await;
    // The phone number has an account: its password is needed, and a wrong one spends nothing.
    let (s, e) = activate(&hub, &second_school, "0300-1234567", "not-the-password-1").await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::UNAUTHORIZED, Some("account_exists")),
        "{e}"
    );
    let (s, added) = activate(&hub, &second_school, "0300-1234567", "brand-new-password-1").await;
    assert_eq!(s, StatusCode::OK, "{added}");
    assert_eq!(links(&hub, &token).await, 2, "both schools on one account");
    // Another family's code cannot be attached to this account without its password.
    let hina_code = code_for(&hub, &clifton, hina).await;
    let (s, _) = activate(&hub, &hina_code, "0300-1234567", "guessing-password-1").await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, hina_session) = activate(&hub, &hina_code, "+92 321 7654321", "hina-password-12").await;
    assert_eq!(
        s,
        StatusCode::OK,
        "the code was not spent by the refusal: {hina_session}"
    );
    let hina_token = hina_session["token"].as_str().unwrap().to_string();
    assert_eq!(links(&hub, &hina_token).await, 1);

    // ---- a new phone: a newer code for the same guardian moves the link ----------------------
    let moved = code_for(&hub, &gulshan, imran).await;
    let (s, new_phone) = activate(&hub, &moved, "0345-5555555", "new-phone-password-1").await;
    assert_eq!(s, StatusCode::OK, "{new_phone}");
    assert_eq!(links(&hub, new_phone["token"].as_str().unwrap()).await, 1);
    assert_eq!(
        links(&hub, &token).await,
        1,
        "the old account keeps only the other school"
    );

    // ---- the school ends access --------------------------------------------------------------------
    let (s, ended) = as_school(
        &hub,
        &clifton,
        Method::DELETE,
        &format!("/sync/parents/{imran_at_clifton}"),
        None,
    )
    .await;
    assert_eq!((s, ended["ended"].as_bool()), (StatusCode::OK, Some(true)));
    assert_eq!(links(&hub, &token).await, 0);
    let (_, ended) = as_school(
        &hub,
        &clifton,
        Method::DELETE,
        &format!("/sync/parents/{imran_at_clifton}"),
        None,
    )
    .await;
    assert_eq!(ended["ended"], false, "nothing left to end");
    // One school cannot end another school's guardian's access.
    let (_, ended) = as_school(
        &hub,
        &clifton,
        Method::DELETE,
        &format!("/sync/parents/{imran}"),
        None,
    )
    .await;
    assert_eq!(ended["ended"], false);
    assert_eq!(links(&hub, new_phone["token"].as_str().unwrap()).await, 1);

    // An unused code past its date stops working; so does one whose school has left.
    let stale = code_for(&hub, &gulshan, imran).await;
    sqlx::query("UPDATE hub_parent_access_codes SET expires_at = now() - interval '1 minute' WHERE used_at IS NULL AND revoked_at IS NULL")
        .execute(&hub.state.pool)
        .await
        .unwrap();
    let (s, _) = activate(&hub, &stale, "0300-9999999", "late-password-12").await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let waiting = code_for(&hub, &clifton, hina).await;
    let (s, _) = as_school(
        &hub,
        &clifton,
        Method::POST,
        "/sync/leave",
        Some(json!({ "reason": "Leaving" })),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        links(&hub, &hina_token).await,
        0,
        "a school that left takes its parents' access with it"
    );
    let (s, _) = hub
        .call(
            Method::POST,
            "/parent/activate/check",
            None,
            Some(json!({ "code": waiting })),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // The hub's log has every step, and no code, password, token or phone number.
    let (_, audit) = hub.get("/audit?page_size=200", &admin).await;
    let text = audit.to_string();
    let secrets: [&str; 6] = [
        &code,
        &again,
        &moved,
        "brand-new-password-1",
        "03001234567",
        &token,
    ];
    for secret in secrets {
        assert!(!text.contains(secret), "{secret} is in the audit log");
    }
    let entries = audit
        .as_array()
        .or_else(|| audit["items"].as_array())
        .unwrap();
    let outcomes: Vec<&str> = entries
        .iter()
        .filter(|e| e["action"] == "parent_activated")
        .filter_map(|e| e["new_value"]["outcome"].as_str())
        .collect();
    for outcome in ["new_account", "new_password", "added_to_account"] {
        assert!(outcomes.contains(&outcome), "{outcome}: {outcomes:?}");
    }
    assert!(entries.iter().any(|e| e["action"] == "parent_access_ended"));
    // Nothing of a parent account lives in a school's schema.
    let in_school: Vec<i64> = hub.in_school(gulshan.school_ref.unwrap(), "SELECT count(*) FROM information_schema.tables WHERE table_schema = current_schema() AND table_name LIKE 'hub\\_%'").await;
    assert_eq!(in_school, [0]);

    gulshan.db.cleanup().await;
    clifton.db.cleanup().await;
    hub.finish().await;
}

#[tokio::test]
async fn repeated_wrong_passwords_lock_a_parent_account_for_a_while() {
    let hub = hub!();
    let admin = hub.setup().await;
    let sim = joined(&hub, &admin, "Gulshan High", "KHI-01").await;
    let imran = guardian(&sim, "Imran Khan", Some("Ali Khan")).await;
    sim.sync(&hub).await.unwrap();
    let code = code_for(&hub, &sim, imran).await;
    let (s, _) = activate(&hub, &code, "0300-1234567", "parent-password-1").await;
    assert_eq!(s, StatusCode::OK);
    let login = |password: &'static str| {
        hub.call(
            Method::POST,
            "/parent/login",
            None,
            Some(json!({ "phone": "0300-1234567", "password": password })),
        )
    };
    for attempt in 1..=5 {
        let (s, e) = login("wrong-password-1").await;
        let expected = if attempt < 5 {
            StatusCode::UNAUTHORIZED
        } else {
            StatusCode::LOCKED
        };
        assert_eq!(s, expected, "attempt {attempt}: {e}");
    }
    // Locked: even the right password waits.
    let (s, e) = login("parent-password-1").await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::LOCKED, Some("account_locked"))
    );
    // The school can help at once: a new code sets a new password and lifts the lock.
    let help = code_for(&hub, &sim, imran).await;
    let (s, _) = activate(&hub, &help, "0300-1234567", "after-lock-password-1").await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = login("after-lock-password-1").await;
    assert_eq!(s, StatusCode::OK);
    sim.db.cleanup().await;
    hub.finish().await;
}
