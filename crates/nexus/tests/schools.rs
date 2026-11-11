//! Stage 4 on the hub: invitations, joining, syncing through the hub's endpoints, keeping
//! schools apart, leaving, removing and purging.

mod common;

use axum::http::{Method, StatusCode};
use common::{seed_school, SchoolSim};
use madrasa_core::hlc::Hlc;
use madrasa_core::sync::{PullRequest, PushRequest};
use madrasa_db::sync::round::{RoundError, TransportError};
use madrasa_db::testing::TestDb;
use madrasa_nexus::tenants;
use serde_json::json;
use uuid::Uuid;

macro_rules! school {
    ($name:expr, $code:expr) => {
        match SchoolSim::new($name, $code).await {
            Some(s) => s,
            None => return,
        }
    };
}

#[tokio::test]
async fn invitations_work_once_expire_and_can_be_revoked() {
    let hub = hub!();
    let admin = hub.setup().await;
    let (_, viewer) = hub
        .post("/users", &admin, json!({ "username": "sana", "full_name": "Sana", "password": "viewer-password-1", "permissions": ["nexus.reports"] }))
        .await;
    assert!(viewer["id"].is_string());
    let viewer = hub.login("sana", "viewer-password-1").await;

    // Only an administrator invites; the validity is bounded.
    let (s, _) = hub
        .post("/invitations", &viewer, json!({ "school_name": "X" }))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    for bad in [
        json!({ "school_name": " " }),
        json!({ "school_name": "X", "expires_in_days": 0 }),
        json!({ "school_name": "X", "expires_in_days": 91 }),
    ] {
        let (s, e) = hub.post("/invitations", &admin, bad).await;
        assert_eq!(
            (s, e["error"]["code"].as_str()),
            (StatusCode::BAD_REQUEST, Some("validation")),
            "{e}"
        );
    }
    let (s, inv) = hub
        .post(
            "/invitations",
            &admin,
            json!({ "school_name": "Gulshan High", "note": "North campus" }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{inv}");
    let code = inv["join_code"].as_str().unwrap().to_string();
    assert_eq!(code.len(), 39, "eight groups of four: {code}");
    assert_eq!(inv["hub_url"], "https://hub.example.org");
    // The code is shown once: not in the list, not in the audit log, not in the database.
    let (_, list) = hub.get("/invitations", &admin).await;
    let (_, audit) = hub.get("/audit", &admin).await;
    assert!(!list.to_string().contains(&code) && !audit.to_string().contains(&code));
    let stored: String = sqlx::query_scalar("SELECT code_hash FROM hub_invitations WHERE id = $1")
        .bind(inv["id"].as_str().unwrap().parse::<Uuid>().unwrap())
        .fetch_one(&hub.state.pool)
        .await
        .unwrap();
    assert_eq!(stored.len(), 64);

    let mut school = school!("Gulshan High", "KHI-GH");
    // A wrong code, and a lower-case, differently grouped right one.
    let (s, e) = school
        .join(&hub, "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("bad_join_code")),
        "{e}"
    );
    let sloppy = code.to_lowercase().replace('-', " ");
    let (s, joined) = school.join(&hub, &sloppy).await;
    assert_eq!(s, StatusCode::OK, "{joined}");
    assert!(joined["token"].as_str().unwrap().len() >= 40);
    assert_eq!(joined["hub_name"], "Gulshan Education Network");
    // Used once.
    let mut second = school!("Second School", "KHI-S2");
    let (s, e) = second.join(&hub, &code).await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::FORBIDDEN, Some("bad_join_code")),
        "{e}"
    );

    // Revoked and expired invitations do not work; a used one cannot be revoked.
    let revoked = hub.invite(&admin, "Second School").await;
    let (_, list) = hub.get("/invitations", &admin).await;
    let id = list[0]["id"].as_str().unwrap().to_string();
    let (s, b) = hub
        .post(&format!("/invitations/{id}/revoke"), &admin, json!({}))
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(b["revoked_at"].is_string());
    let (s, _) = second.join(&hub, &revoked).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let expired = hub.invite(&admin, "Second School").await;
    sqlx::query("UPDATE hub_invitations SET expires_at = now() - interval '1 minute' WHERE used_at IS NULL AND revoked_at IS NULL")
        .execute(&hub.state.pool)
        .await
        .unwrap();
    let (s, _) = second.join(&hub, &expired).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, e) = hub
        .post(
            &format!("/invitations/{}/revoke", inv["id"].as_str().unwrap()),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("conflict")),
        "{e}"
    );

    // Nothing was created for the refused joins.
    let (_, schools) = hub.get("/schools", &admin).await;
    assert_eq!(schools.as_array().unwrap().len(), 1);
    school.db.cleanup().await;
    second.db.cleanup().await;
    hub.finish().await;
}

#[tokio::test]
async fn a_school_joins_uploads_and_syncs_both_ways() {
    let hub = hub!();
    let admin = hub.setup().await;
    let mut school = school!("Gulshan High", "KHI-GH");
    let code = hub.invite(&admin, "Gulshan High").await;
    let (s, joined) = school.join(&hub, &code).await;
    assert_eq!(s, StatusCode::OK, "{joined}");
    let id = school.school_ref.unwrap();

    // The register knows the school; its data has a schema of its own, already migrated.
    let row = hub.school_row(&admin, id).await;
    assert_eq!(
        (
            row["status"].as_str(),
            row["name"].as_str(),
            row["code"].as_str()
        ),
        (Some("active"), Some("Gulshan High"), Some("KHI-GH"))
    );
    assert_eq!(row["sync"]["role"], "head_office");
    assert!(
        row.get("schema_name").is_none(),
        "the schema name is not part of the API"
    );
    assert!(row.get("token_hash").is_none());
    let schema: String = sqlx::query_scalar("SELECT schema_name FROM hub_schools WHERE id = $1")
        .bind(id)
        .fetch_one(&hub.state.pool)
        .await
        .unwrap();
    assert!(tenants::is_schema_name(&schema));
    let before: Vec<String> = hub.in_school(id, "SELECT name FROM schools").await;
    assert_eq!(
        before,
        ["Gulshan High"],
        "the school's own record arrives with the join"
    );
    let none: Vec<String> = hub.in_school(id, "SELECT full_name FROM students").await;
    assert!(none.is_empty(), "nothing else until the first sync");

    // First sync: everything is uploaded.
    let report = school.sync(&hub).await.unwrap();
    assert!(report.sent > 10, "{report:?}");
    assert!(report.parked_at_hub.is_empty() && report.parked_here.is_empty());
    let names: Vec<String> = hub
        .in_school(id, "SELECT full_name FROM students ORDER BY 1")
        .await;
    assert_eq!(names, ["Ali Khan", "Sara Noor"]);
    let hashes: Vec<String> = hub.in_school(id, "SELECT password_hash FROM users").await;
    assert_eq!(hashes, ["!"], "password hashes stay at the school");
    let row = hub.school_row(&admin, id).await;
    assert!(row["last_sync_at"].is_string());
    assert_eq!(row["school_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(row["sync"]["pending"], 0);

    // The school edits; head office edits another record in the school's schema.
    school.rename_student("Ali Khan", "Ali Ahmed Khan").await;
    let _: Vec<Uuid> = hub
        .in_school(id, "UPDATE students SET father_name = 'Corrected by head office', updated_at = now(), hlc = '9999999999999-00000-00000000004b' WHERE full_name = 'Sara Noor' RETURNING id")
        .await;
    let report = school.sync(&hub).await.unwrap();
    assert!(report.sent >= 1 && report.received >= 1, "{report:?}");
    let hub_names: Vec<String> = hub
        .in_school(id, "SELECT full_name FROM students ORDER BY 1")
        .await;
    assert_eq!(hub_names, ["Ali Ahmed Khan", "Sara Noor"]);
    let father: String =
        sqlx::query_scalar("SELECT father_name FROM students WHERE full_name = 'Sara Noor'")
            .fetch_one(&school.db.pool)
            .await
            .unwrap();
    assert_eq!(father, "Corrected by head office");

    // Both edit the same field: head office wins, and the hub shows the log entry.
    let _: Vec<Uuid> = hub
        .in_school(id, "UPDATE students SET full_name = 'Sara Noor (head office)', updated_at = now(), hlc = '9999999999999-00001-00000000004b' WHERE full_name = 'Sara Noor' RETURNING id")
        .await;
    school
        .rename_student("Sara Noor", "Sara Noor (school)")
        .await;
    let report = school.sync(&hub).await.unwrap();
    assert_eq!(report.overwrites, 1, "{report:?}");
    assert_eq!(
        school.student_names().await,
        ["Ali Ahmed Khan", "Sara Noor (head office)"]
    );
    let (s, log) = hub.get(&format!("/schools/{id}/overwrites"), &admin).await;
    assert_eq!(s, StatusCode::OK, "{log}");
    assert_eq!(log.as_array().unwrap().len(), 1);
    assert_eq!(log[0]["kept"], "Sara Noor (head office)");
    assert_eq!(log[0]["discarded"], "Sara Noor (school)");
    assert_eq!(log[0]["reason"], "head_office_wins");
    let school_log: i64 = sqlx::query_scalar("SELECT count(*) FROM sync_overwrites")
        .fetch_one(&school.db.pool)
        .await
        .unwrap();
    assert_eq!(school_log, 1, "the school holds the same entry");
    let (s, parked) = hub.get(&format!("/schools/{id}/parked"), &admin).await;
    assert_eq!((s, parked), (StatusCode::OK, json!([])));

    // A school on a newer schema than the hub is told to wait for the hub.
    let token = school.token.clone().unwrap();
    let too_new = PushRequest {
        app_version: "9.9.9".into(),
        schema_version: tenants::hub_schema_version() + 1,
        ..Default::default()
    };
    let (s, e) = hub
        .call(
            Method::POST,
            "/sync/push",
            Some(&token),
            Some(json!(too_new)),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("hub_too_old")),
        "{e}"
    );
    assert!(e["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Update the hub first"));
    // Without a token, or with a hub user's session, the sync endpoints refuse.
    let (s, _) = hub
        .call(
            Method::POST,
            "/sync/pull",
            None,
            Some(json!(PullRequest { limit: 10 })),
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, e) = hub
        .call(
            Method::POST,
            "/sync/pull",
            Some(&admin),
            Some(json!(PullRequest { limit: 10 })),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::UNAUTHORIZED, Some("unknown_school")),
        "{e}"
    );
    // And a school's token opens no head office endpoint.
    let (s, _) = hub.get("/schools", &token).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, st) = hub.get("/sync/status", &token).await;
    assert_eq!(
        (s, st["status"].as_str(), st["school_name"].as_str()),
        (StatusCode::OK, Some("active"), Some("Gulshan High"))
    );

    // A hub restart migrates every school schema again and keeps capture on: a no-op here.
    assert_eq!(tenants::migrate_all(&hub.state.pool).await.unwrap(), 1);
    assert_eq!(hub.school_row(&admin, id).await["sync"]["enabled"], true);
    school.db.cleanup().await;
    hub.finish().await;
}

#[tokio::test]
async fn two_schools_with_identical_data_never_see_each_other() {
    let hub = hub!();
    let admin = hub.setup().await;
    // Two installs that cannot be told apart by their data: the same ids, the same code,
    // the same students (a copied database).
    let Some(seed) = TestDb::new().await else {
        return;
    };
    let hlc = Hlc::new(1);
    let school = seed_school(&seed, &hlc, "Twin School", "KHI-TW").await;
    let mut a = SchoolSim {
        db: seed.clone_database().await,
        hlc: Hlc::new(0xa),
        school: school.clone(),
        token: None,
        school_ref: None,
    };
    let mut b = SchoolSim {
        db: seed.clone_database().await,
        hlc: Hlc::new(0xb),
        school,
        token: None,
        school_ref: None,
    };
    for sim in [&mut a, &mut b] {
        let code = hub.invite(&admin, "Twin School").await;
        let (s, body) = sim.join(&hub, &code).await;
        assert_eq!(s, StatusCode::OK, "{body}");
        sim.sync(&hub).await.unwrap();
    }
    let (ida, idb) = (a.school_ref.unwrap(), b.school_ref.unwrap());
    assert_ne!(ida, idb);
    assert_ne!(a.token, b.token);
    let schemas: Vec<String> =
        sqlx::query_scalar("SELECT schema_name FROM hub_schools ORDER BY created_at")
            .fetch_all(&hub.state.pool)
            .await
            .unwrap();
    assert_eq!(schemas.len(), 2);
    assert_ne!(schemas[0], schemas[1]);

    // A changes a student. Only A's copy changes: B's copy, B's install and B's next sync
    // know nothing of it, although the record has the same id in both.
    a.rename_student("Ali Khan", "Ali of School A").await;
    a.sync(&hub).await.unwrap();
    let in_a: Vec<String> = hub
        .in_school(ida, "SELECT full_name FROM students ORDER BY 1")
        .await;
    let in_b: Vec<String> = hub
        .in_school(idb, "SELECT full_name FROM students ORDER BY 1")
        .await;
    assert_eq!(in_a, ["Ali of School A", "Sara Noor"]);
    assert_eq!(in_b, ["Ali Khan", "Sara Noor"]);
    let report = b.sync(&hub).await.unwrap();
    assert_eq!((report.sent, report.received), (0, 0));
    assert_eq!(b.student_names().await, ["Ali Khan", "Sara Noor"]);

    // Head office corrects B's copy: it reaches B only.
    let _: Vec<Uuid> = hub
        .in_school(idb, "UPDATE students SET full_name = 'Sara of School B', updated_at = now(), hlc = '9999999999999-00000-00000000004b' WHERE full_name = 'Sara Noor' RETURNING id")
        .await;
    a.sync(&hub).await.unwrap();
    b.sync(&hub).await.unwrap();
    assert_eq!(a.student_names().await, ["Ali of School A", "Sara Noor"]);
    assert_eq!(b.student_names().await, ["Ali Khan", "Sara of School B"]);

    // Inside a school's transaction the hub's own tables and other schools do not exist...
    let mut conn = hub.state.pool.acquire().await.unwrap();
    let school_a = tenants::get(&mut conn, ida).await.unwrap().unwrap();
    drop(conn);
    let mut tx = tenants::begin(&hub.state.pool, &school_a).await.unwrap();
    let path: String = sqlx::query_scalar("SHOW search_path")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        path.trim_matches('"'),
        schemas[0],
        "one schema, nothing else"
    );
    for hidden in [
        "SELECT count(*) FROM hub_users",
        "SELECT count(*) FROM hub_schools",
    ] {
        let mut sp = sqlx::Connection::begin(&mut *tx).await.unwrap();
        let err = sqlx::query(hidden).execute(&mut *sp).await.unwrap_err();
        assert!(err.to_string().contains("does not exist"), "{err}");
        sp.rollback().await.unwrap();
    }
    tx.commit().await.unwrap();
    // ...and afterwards every pooled connection is back on the hub's schema, where school
    // tables do not exist.
    for _ in 0..20 {
        let path: String = sqlx::query_scalar("SHOW search_path")
            .fetch_one(&hub.state.pool)
            .await
            .unwrap();
        assert_eq!(path, "nexus");
    }
    let err = sqlx::query("SELECT count(*) FROM students")
        .execute(&hub.state.pool)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("does not exist"), "{err}");

    // Purging A (after removing it) leaves B complete and syncing.
    let (s, e) = hub
        .post(
            &format!("/schools/{ida}/purge"),
            &admin,
            json!({ "confirm_code": "KHI-TW" }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("conflict")),
        "an active school cannot be purged: {e}"
    );
    let (s, e) = hub
        .post(
            &format!("/schools/{ida}/remove"),
            &admin,
            json!({ "reason": " " }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    let (s, removed) = hub
        .post(
            &format!("/schools/{ida}/remove"),
            &admin,
            json!({ "reason": "Joined twice by mistake." }),
        )
        .await;
    assert_eq!(
        (s, removed["status"].as_str()),
        (StatusCode::OK, Some("removed")),
        "{removed}"
    );
    let (s, e) = hub
        .post(
            &format!("/schools/{ida}/purge"),
            &admin,
            json!({ "confirm_code": "wrong" }),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("validation")),
        "{e}"
    );
    let (s, purged) = hub
        .post(
            &format!("/schools/{ida}/purge"),
            &admin,
            json!({ "confirm_code": "khi-tw" }),
        )
        .await;
    assert_eq!(
        (s, purged["status"].as_str()),
        (StatusCode::OK, Some("purged")),
        "{purged}"
    );
    let left: Vec<String> = sqlx::query_scalar(
        "SELECT nspname::text FROM pg_namespace WHERE nspname LIKE 't\\_%' ORDER BY 1",
    )
    .fetch_all(&hub.state.pool)
    .await
    .unwrap();
    assert_eq!(
        left,
        [schemas[1].clone()],
        "A's schema is gone, B's is untouched"
    );
    let in_b: Vec<String> = hub
        .in_school(idb, "SELECT full_name FROM students ORDER BY 1")
        .await;
    assert_eq!(in_b, ["Ali Khan", "Sara of School B"]);
    b.rename_student("Ali Khan", "Ali of School B").await;
    b.sync(&hub).await.unwrap();
    let in_b: Vec<String> = hub
        .in_school(idb, "SELECT full_name FROM students ORDER BY 1")
        .await;
    assert_eq!(in_b, ["Ali of School B", "Sara of School B"]);
    // A's own install still has all its data; its key no longer opens anything.
    assert_eq!(a.student_names().await, ["Ali of School A", "Sara Noor"]);
    let err = a.sync(&hub).await.unwrap_err();
    assert!(
        matches!(err, RoundError::Transport(TransportError::Refused { ref code, .. }) if code == "unknown_school"),
        "{err:?}"
    );
    let (s, e) = hub.get(&format!("/schools/{ida}/overwrites"), &admin).await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("conflict")),
        "{e}"
    );
    let (_, audit) = hub.get("/audit?page_size=100", &admin).await;
    let actions: Vec<&str> = audit
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["action"].as_str().unwrap())
        .collect();
    for expected in ["join", "removed", "purge"] {
        assert!(
            actions.contains(&expected),
            "{expected} is audited: {actions:?}"
        );
    }
    a.db.cleanup().await;
    b.db.cleanup().await;
    seed.cleanup().await;
    hub.finish().await;
}

#[tokio::test]
async fn a_school_can_leave_and_a_removed_school_is_told() {
    let hub = hub!();
    let admin = hub.setup().await;
    let mut leaver = school!("Leaving School", "KHI-LV");
    let mut removed = school!("Removed School", "KHI-RM");
    for sim in [&mut leaver, &mut removed] {
        let code = hub.invite(&admin, "x").await;
        let (s, body) = sim.join(&hub, &code).await;
        assert_eq!(s, StatusCode::OK, "{body}");
        sim.sync(&hub).await.unwrap();
    }

    // The school leaves: the hub keeps an archive with capture off and forgets the key.
    let token = leaver.token.clone().unwrap();
    let (s, b) = hub
        .call(
            Method::POST,
            "/sync/leave",
            Some(&token),
            Some(json!({ "reason": "Changing network" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let row = hub.school_row(&admin, leaver.school_ref.unwrap()).await;
    assert_eq!(
        (row["status"].as_str(), row["leave_reason"].as_str()),
        (Some("left"), Some("Changing network"))
    );
    assert_eq!(row["sync"]["enabled"], false);
    let archived: Vec<String> = hub
        .in_school(
            leaver.school_ref.unwrap(),
            "SELECT full_name FROM students ORDER BY 1",
        )
        .await;
    assert_eq!(
        archived.len(),
        2,
        "the archive is kept until someone purges it"
    );
    let err = leaver.sync(&hub).await.unwrap_err();
    assert!(
        matches!(err, RoundError::Transport(TransportError::Refused { ref code, .. }) if code == "unknown_school"),
        "{err:?}"
    );
    // The school's own data is exactly as it was.
    assert_eq!(leaver.student_names().await, ["Ali Khan", "Sara Noor"]);

    // Head office removes the other school: its next sync is told so, with the reason.
    let id = removed.school_ref.unwrap();
    let (s, b) = hub
        .post(
            &format!("/schools/{id}/remove"),
            &admin,
            json!({ "reason": "The school closed." }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let (s, e) = hub
        .post(
            &format!("/schools/{id}/remove"),
            &admin,
            json!({ "reason": "again" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{e}");
    removed
        .rename_student("Ali Khan", "Changed after removal")
        .await;
    match removed.sync(&hub).await.unwrap_err() {
        RoundError::Transport(TransportError::Removed(message)) => {
            assert_eq!(
                message,
                "Head office removed this school from the network: The school closed."
            );
        }
        other => panic!("expected to be told about the removal: {other:?}"),
    }
    let names: Vec<String> = hub
        .in_school(id, "SELECT full_name FROM students ORDER BY 1")
        .await;
    assert_eq!(
        names,
        ["Ali Khan", "Sara Noor"],
        "nothing is accepted from a removed school"
    );
    let (s, e) = hub
        .post(&format!("/schools/{id}/resend"), &admin, json!({}))
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{e}");
    leaver.db.cleanup().await;
    removed.db.cleanup().await;
    hub.finish().await;
}

#[tokio::test]
async fn sending_everything_again_restores_what_a_school_lost() {
    let hub = hub!();
    let admin = hub.setup().await;
    let mut school = school!("Gulshan High", "KHI-GH");
    let code = hub.invite(&admin, "Gulshan High").await;
    school.join(&hub, &code).await;
    school.sync(&hub).await.unwrap();
    // The school goes back in time (an old backup restored): a record changes without the
    // change being noted for sending.
    let mut tx = school.db.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('madrasa.sync_apply', 'on', true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE students SET full_name = 'Old Value From Backup' WHERE full_name = 'Ali Khan'",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let report = school.sync(&hub).await.unwrap();
    assert_eq!(
        (report.sent, report.received),
        (0, 0),
        "an ordinary sync sees no difference"
    );

    let id = school.school_ref.unwrap();
    let (s, resent) = hub
        .post(&format!("/schools/{id}/resend"), &admin, json!({}))
        .await;
    assert_eq!(s, StatusCode::OK, "{resent}");
    assert!(resent["queued"].as_i64().unwrap() > 10);
    let report = school.sync(&hub).await.unwrap();
    assert!(report.received >= 1, "{report:?}");
    assert_eq!(school.student_names().await, ["Ali Khan", "Sara Noor"]);
    assert_eq!(hub.school_row(&admin, id).await["sync"]["pending"], 0);
    school.db.cleanup().await;
    hub.finish().await;
}
