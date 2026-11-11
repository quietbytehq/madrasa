//! Nexus stage 6: backup key escrow at the hub. Setting it up, schools leaving their keys,
//! releasing one (permission, passphrase, reason), the record the school is shown, and the
//! sealed key being erased when a deposit ends. Needs PostgreSQL via
//! `MADRASA_TEST_DATABASE_URL`.

mod common;

use axum::http::{Method, StatusCode};
use common::{Hub, SchoolSim};
use madrasa_core::escrow;
use serde_json::{json, Value};
use uuid::Uuid;

const PASSPHRASE: &str = "correct horse battery";

/// A data key no school really has; what matters is that it comes back unchanged.
fn data_key(seed: u8) -> [u8; 32] {
    let mut k = [0u8; 32];
    for (i, b) in k.iter_mut().enumerate() {
        *b = seed.wrapping_add((i as u8).wrapping_mul(13));
    }
    k
}

async fn school(hub: &Hub, admin: &str, name: &str, code: &str) -> SchoolSim {
    let mut sim = SchoolSim::new(name, code).await.unwrap();
    let join_code = hub.invite(admin, name).await;
    let (s, b) = sim.join(hub, &join_code).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    sim.sync(hub).await.unwrap();
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

/// The school seals `key` to the hub's public key and leaves it there.
async fn deposit(hub: &Hub, sim: &SchoolSim, key: &[u8; 32]) -> (StatusCode, Value) {
    let (s, status) = as_school(hub, sim, Method::GET, "/sync/escrow", None).await;
    assert_eq!(s, StatusCode::OK, "{status}");
    let public_key = status["public_key"].as_str().expect("escrow is set up");
    let sealed = escrow::seal(public_key, key, &escrow::context_for(sim.school.id)).unwrap();
    as_school(
        hub,
        sim,
        Method::PUT,
        "/sync/escrow",
        Some(json!({ "deposit": sealed })),
    )
    .await
}

async fn user(hub: &Hub, admin: &str, username: &str, permissions: &[&str]) -> String {
    let (s, b) = hub
        .post(
            "/users",
            admin,
            json!({ "username": username, "full_name": format!("{username} of head office"),
                    "password": "user-password-1", "permissions": permissions }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    hub.login(username, "user-password-1").await
}

async fn deposit_rows(hub: &Hub) -> Vec<(bool, Option<String>)> {
    sqlx::query_as(
        "SELECT sealed IS NOT NULL, ended_reason FROM hub_escrow_deposits ORDER BY created_at, id",
    )
    .fetch_all(&hub.state.pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn a_key_is_left_released_with_passphrase_and_reason_and_the_school_is_told() {
    let hub = hub!();
    let admin = hub.setup().await;
    let gulshan = school(&hub, &admin, "Gulshan High", "KHI-01").await;
    let gulshan_ref = gulshan.school_ref.unwrap();

    // Before head office sets escrow up there is nothing to seal a key to.
    let (s, o) = hub.get("/escrow", &admin).await;
    assert_eq!(
        (s, o["set_up"].as_bool()),
        (StatusCode::OK, Some(false)),
        "{o}"
    );
    let (s, st) = as_school(&hub, &gulshan, Method::GET, "/sync/escrow", None).await;
    assert_eq!(
        (s, &st["public_key"], &st["deposit"]),
        (StatusCode::OK, &Value::Null, &Value::Null)
    );
    let stray = escrow::seal(
        &escrow::generate(PASSPHRASE).unwrap().public_key,
        &data_key(1),
        b"x",
    )
    .unwrap();
    let (s, e) = as_school(
        &hub,
        &gulshan,
        Method::PUT,
        "/sync/escrow",
        Some(json!({ "deposit": stray })),
    )
    .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("escrow_not_set_up"))
    );
    let (s, e) = hub
        .post(
            &format!("/schools/{gulshan_ref}/escrow/release"),
            &admin,
            json!({ "passphrase": PASSPHRASE, "reason": "test" }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("escrow_not_set_up"))
    );

    // Setting up: administrators only, a long passphrase, once.
    let reports = user(&hub, &admin, "reports", &["nexus.reports"]).await;
    let keeper = user(&hub, &admin, "keeper", &["nexus.escrow"]).await;
    for token in [&reports, &keeper] {
        let (s, _) = hub
            .post("/escrow/setup", token, json!({ "passphrase": PASSPHRASE }))
            .await;
        assert_eq!(s, StatusCode::FORBIDDEN);
    }
    let (s, e) = hub
        .post(
            "/escrow/setup",
            &admin,
            json!({ "passphrase": "too short" }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    assert_eq!(
        e["error"]["message"],
        "The escrow passphrase must be at least 12 characters."
    );
    let (s, _) = hub
        .post("/escrow/setup", &admin, json!({ "passphrase": PASSPHRASE }))
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, e) = hub
        .post(
            "/escrow/setup",
            &admin,
            json!({ "passphrase": "another passphrase" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{e}");

    // The school leaves its key. The hub checks the shape, cannot open it, and never
    // stores anything that reveals it.
    let key = data_key(7);
    let (s, e) = as_school(&hub, &gulshan, Method::PUT, "/sync/escrow",
        Some(json!({ "deposit": { "version": 1, "ephemeral_public": "", "nonce": "x", "ciphertext": "x", "key_id": "x" } }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    let (s, st) = deposit(&hub, &gulshan, &key).await;
    assert_eq!(s, StatusCode::OK, "{st}");
    assert_eq!(st["deposit"]["key_id"], escrow::key_id(&key));
    assert_eq!(st["releases"], json!([]));
    let stored: Vec<String> = sqlx::query_scalar(
        "SELECT value::text FROM hub_settings WHERE key = 'escrow.keys'
         UNION ALL SELECT sealed::text FROM hub_escrow_deposits",
    )
    .fetch_all(&hub.state.pool)
    .await
    .unwrap();
    assert_eq!(stored.len(), 2);
    use base64::Engine;
    let key_b64 = base64::engine::general_purpose::STANDARD.encode(key);
    for text in &stored {
        assert!(
            !text.contains(PASSPHRASE) && !text.contains(&key_b64),
            "{text}"
        );
    }
    // Without a session nothing is told; a sync token is not a console login.
    let (s, _) = hub.call(Method::GET, "/sync/escrow", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = hub
        .call(Method::GET, "/escrow", gulshan.token.as_deref(), None)
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    // Who sees the list: administrators and key keepers, not report readers.
    let (s, _) = hub.get("/escrow", &reports).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, o) = hub.get("/escrow", &keeper).await;
    assert_eq!(s, StatusCode::OK, "{o}");
    assert_eq!(o["set_up"], true);
    assert_eq!(o["deposits"].as_array().unwrap().len(), 1);
    assert_eq!(o["deposits"][0]["school_name"], "Gulshan High");
    assert!(
        !o.to_string().contains("ciphertext"),
        "the list never carries a sealed key: {o}"
    );

    // Releasing needs the permission, a reason and the passphrase.
    let path = format!("/schools/{gulshan_ref}/escrow/release");
    for token in [&reports] {
        let (s, _) = hub
            .post(
                &path,
                token,
                json!({ "passphrase": PASSPHRASE, "reason": "Lost password" }),
            )
            .await;
        assert_eq!(s, StatusCode::FORBIDDEN);
    }
    let (s, e) = hub
        .post(
            &path,
            &keeper,
            json!({ "passphrase": PASSPHRASE, "reason": "  " }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["message"].as_str()),
        (StatusCode::BAD_REQUEST, Some("A reason is required."))
    );
    let (s, e) = hub
        .post(
            &path,
            &keeper,
            json!({ "passphrase": "not the passphrase", "reason": "Lost password" }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::UNAUTHORIZED, Some("escrow_passphrase_invalid"))
    );
    let (s, e) = hub
        .post(
            &format!("/schools/{}/escrow/release", Uuid::now_v7()),
            &keeper,
            json!({ "passphrase": PASSPHRASE, "reason": "x" }),
        )
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{e}");
    let (_, st) = as_school(&hub, &gulshan, Method::GET, "/sync/escrow", None).await;
    assert_eq!(
        st["releases"],
        json!([]),
        "a failed attempt releases nothing"
    );

    let reason = "The principal lost the backup password and the recovery key";
    let (s, released) = hub
        .post(
            &path,
            &keeper,
            json!({ "passphrase": PASSPHRASE, "reason": reason }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{released}");
    let code = released["released_key"].as_str().unwrap().to_string();
    assert_eq!(
        escrow::parse_released_key(&code),
        Some(key),
        "the school's own key comes back"
    );
    assert_eq!(released["school_code"], "KHI-01");

    // The hub's audit log has the release and the failed attempt: who, which school, why.
    // Neither the passphrase nor the key is anywhere in it.
    let (s, audit) = hub.get("/audit?page_size=200", &admin).await;
    assert_eq!(s, StatusCode::OK);
    let text = audit.to_string();
    assert!(
        !text.contains(&code) && !text.contains(PASSPHRASE) && !text.contains(&key_b64),
        "{text}"
    );
    let entries: Vec<&Value> = audit
        .as_array()
        .or_else(|| audit["items"].as_array())
        .unwrap()
        .iter()
        .collect();
    let of = |action: &str| entries.iter().filter(|e| e["action"] == action).count();
    assert_eq!(
        (
            of("escrow_setup"),
            of("escrow_deposit"),
            of("escrow_passphrase_failed"),
            of("escrow_release")
        ),
        (1, 1, 1, 1),
        "{text}"
    );
    let entry = entries
        .iter()
        .find(|e| e["action"] == "escrow_release")
        .unwrap();
    assert_eq!(entry["new_value"]["reason"], reason);
    assert_eq!(entry["entity_id"], json!(gulshan_ref));

    // The school is shown the release until it confirms it recorded it, and after.
    let (_, st) = as_school(&hub, &gulshan, Method::GET, "/sync/escrow", None).await;
    let shown = &st["releases"][0];
    assert_eq!(
        (
            shown["released_by"].as_str(),
            shown["reason"].as_str(),
            shown["seen"].as_bool()
        ),
        (Some("keeper of head office"), Some(reason), Some(false))
    );
    assert!(!st.to_string().contains(&code));
    let (_, o) = hub.get("/escrow", &admin).await;
    assert_eq!(o["releases"][0]["seen_at"], Value::Null);

    // Another school sees nothing of this and cannot confirm on Gulshan's behalf.
    let clifton = school(&hub, &admin, "Clifton Grammar", "KHI-02").await;
    let (_, other) = as_school(&hub, &clifton, Method::GET, "/sync/escrow", None).await;
    assert_eq!(
        (&other["deposit"], &other["releases"]),
        (&Value::Null, &json!([]))
    );
    as_school(
        &hub,
        &clifton,
        Method::POST,
        "/sync/escrow/seen",
        Some(json!({ "ids": [shown["id"]] })),
    )
    .await;
    let (_, st) = as_school(&hub, &gulshan, Method::GET, "/sync/escrow", None).await;
    assert_eq!(st["releases"][0]["seen"], false);
    let (s, st) = as_school(
        &hub,
        &gulshan,
        Method::POST,
        "/sync/escrow/seen",
        Some(json!({ "ids": [shown["id"]] })),
    )
    .await;
    assert_eq!(
        (s, st["releases"][0]["seen"].as_bool()),
        (StatusCode::OK, Some(true))
    );
    let (_, o) = hub.get("/escrow", &admin).await;
    assert!(o["releases"][0]["seen_at"].is_string(), "{o}");

    // A key sealed for one school does not open as another's: Clifton uploads a copy of
    // Gulshan's sealed key; the release fails instead of handing Gulshan's key out.
    let copied: Value =
        sqlx::query_scalar("SELECT sealed FROM hub_escrow_deposits WHERE deleted_at IS NULL")
            .fetch_one(&hub.state.pool)
            .await
            .unwrap();
    let (s, _) = as_school(
        &hub,
        &clifton,
        Method::PUT,
        "/sync/escrow",
        Some(json!({ "deposit": copied })),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, e) = hub
        .post(
            &format!("/schools/{}/escrow/release", clifton.school_ref.unwrap()),
            &keeper,
            json!({ "passphrase": PASSPHRASE, "reason": "Asked by Clifton" }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("deposit_damaged")),
        "{e}"
    );

    // Changing the passphrase: needs the current one; deposits keep working.
    let (s, _) = hub
        .post(
            "/escrow/passphrase",
            &reports,
            json!({ "current": PASSPHRASE, "new": "a brand new passphrase" }),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, e) = hub
        .post(
            "/escrow/passphrase",
            &keeper,
            json!({ "current": "wrong wrong wrong", "new": "a brand new passphrase" }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::UNAUTHORIZED, Some("escrow_passphrase_invalid"))
    );
    let (s, _) = hub
        .post(
            "/escrow/passphrase",
            &keeper,
            json!({ "current": PASSPHRASE, "new": "short" }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = hub
        .post(
            "/escrow/passphrase",
            &keeper,
            json!({ "current": PASSPHRASE, "new": "a brand new passphrase" }),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = hub
        .post(
            &path,
            &keeper,
            json!({ "passphrase": PASSPHRASE, "reason": "Again" }),
        )
        .await;
    assert_eq!(
        s,
        StatusCode::UNAUTHORIZED,
        "the old passphrase no longer works"
    );
    let (s, again) = hub
        .post(
            &path,
            &keeper,
            json!({ "passphrase": "a brand new passphrase", "reason": "Again" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{again}");
    assert_eq!(
        escrow::parse_released_key(again["released_key"].as_str().unwrap()),
        Some(key)
    );
    let (_, st) = as_school(&hub, &gulshan, Method::GET, "/sync/escrow", None).await;
    assert_eq!(st["releases"].as_array().unwrap().len(), 2);
    assert_eq!(st["releases"][0]["reason"], "Again", "newest first");

    gulshan.db.cleanup().await;
    clifton.db.cleanup().await;
    hub.finish().await;
}

#[tokio::test]
async fn repeated_wrong_passphrases_stop_releases_for_a_while() {
    let hub = hub!();
    let admin = hub.setup().await;
    let sim = school(&hub, &admin, "Gulshan High", "KHI-01").await;
    let (s, _) = hub
        .post("/escrow/setup", &admin, json!({ "passphrase": PASSPHRASE }))
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = deposit(&hub, &sim, &data_key(3)).await;
    assert_eq!(s, StatusCode::OK);
    let path = format!("/schools/{}/escrow/release", sim.school_ref.unwrap());
    for _ in 0..5 {
        let (s, _) = hub
            .post(
                &path,
                &admin,
                json!({ "passphrase": "a wrong passphrase", "reason": "Guessing" }),
            )
            .await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
    }
    // Even the right passphrase waits now, for this user only.
    let (s, e) = hub
        .post(
            &path,
            &admin,
            json!({ "passphrase": PASSPHRASE, "reason": "Guessing" }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::TOO_MANY_REQUESTS, Some("escrow_locked")),
        "{e}"
    );
    let keeper = user(&hub, &admin, "keeper", &["nexus.escrow"]).await;
    let (s, _) = hub
        .post(
            &path,
            &keeper,
            json!({ "passphrase": PASSPHRASE, "reason": "Real need" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    sim.db.cleanup().await;
    hub.finish().await;
}

#[tokio::test]
async fn the_sealed_key_is_erased_when_a_deposit_ends() {
    let hub = hub!();
    // Escrow can be set up in the first-run setup itself.
    let (s, b) = hub
        .call(
            Method::POST,
            "/setup",
            None,
            Some(json!({
                "setup_token": common::SETUP_TOKEN, "hub_name": "Gulshan Education Network",
                "admin_username": "head.office", "admin_full_name": "Head Office",
                "admin_password": common::ADMIN_PASSWORD, "escrow_passphrase": PASSPHRASE,
            })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let admin = hub.login("head.office", common::ADMIN_PASSWORD).await;
    let (_, o) = hub.get("/escrow", &admin).await;
    assert_eq!(o["set_up"], true);

    let gulshan = school(&hub, &admin, "Gulshan High", "KHI-01").await;
    let clifton = school(&hub, &admin, "Clifton Grammar", "KHI-02").await;
    let release = |sim: &SchoolSim| format!("/schools/{}/escrow/release", sim.school_ref.unwrap());

    // Leaving a key twice replaces the first; taking it back erases it.
    deposit(&hub, &gulshan, &data_key(1)).await;
    deposit(&hub, &gulshan, &data_key(2)).await;
    assert_eq!(
        deposit_rows(&hub).await,
        [(false, Some("replaced".into())), (true, None)]
    );
    let (s, released) = hub
        .post(
            &release(&gulshan),
            &admin,
            json!({ "passphrase": PASSPHRASE, "reason": "Check" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        escrow::parse_released_key(released["released_key"].as_str().unwrap()),
        Some(data_key(2)),
        "the newer key"
    );
    let (s, st) = as_school(&hub, &gulshan, Method::DELETE, "/sync/escrow", None).await;
    assert_eq!((s, &st["deposit"]), (StatusCode::OK, &Value::Null));
    assert_eq!(
        st["releases"].as_array().unwrap().len(),
        1,
        "the record of a release stays"
    );
    assert_eq!(
        deposit_rows(&hub).await[1],
        (false, Some("withdrawn".into()))
    );
    let (s, e) = hub
        .post(
            &release(&gulshan),
            &admin,
            json!({ "passphrase": PASSPHRASE, "reason": "After" }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("no_deposit"))
    );
    // Taking back nothing is not an error.
    let (s, _) = as_school(&hub, &gulshan, Method::DELETE, "/sync/escrow", None).await;
    assert_eq!(s, StatusCode::OK);

    // A school that leaves takes its key with it.
    deposit(&hub, &gulshan, &data_key(3)).await;
    let (s, _) = as_school(
        &hub,
        &gulshan,
        Method::POST,
        "/sync/leave",
        Some(json!({ "reason": "Leaving" })),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(deposit_rows(&hub).await[2], (false, Some("left".into())));
    let (s, e) = hub
        .post(
            &release(&gulshan),
            &admin,
            json!({ "passphrase": PASSPHRASE, "reason": "After leaving" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{e}");
    assert_eq!(
        e["error"]["message"],
        "Only the key of a school that is joined can be released."
    );

    // So does one head office removes; purging leaves nothing sealed either.
    deposit(&hub, &clifton, &data_key(4)).await;
    let clifton_ref = clifton.school_ref.unwrap();
    let (s, _) = hub
        .post(
            &format!("/schools/{clifton_ref}/remove"),
            &admin,
            json!({ "reason": "Closed" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(deposit_rows(&hub).await[3], (false, Some("removed".into())));
    let (s, _) = hub
        .post(
            &format!("/schools/{clifton_ref}/purge"),
            &admin,
            json!({ "confirm_code": "KHI-02" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let rows = deposit_rows(&hub).await;
    assert_eq!(rows.len(), 4);
    assert!(
        rows.iter().all(|(sealed, _)| !sealed),
        "no sealed key is left: {rows:?}"
    );
    let (_, o) = hub.get("/escrow", &admin).await;
    assert_eq!(o["deposits"], json!([]));
    assert_eq!(
        o["releases"].as_array().unwrap().len(),
        1,
        "the history of releases stays"
    );

    gulshan.db.cleanup().await;
    clifton.db.cleanup().await;
    hub.finish().await;
}
