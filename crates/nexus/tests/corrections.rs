//! Nexus stage 7: head office corrections. A correction made at the hub reaches the school
//! at its next sync, beats a school edit of the same field (which is logged), leaves other
//! fields alone, and is refused for anything outside the short list of names and contact
//! details. Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

mod common;

use axum::http::{Method, StatusCode};
use common::{Hub, SchoolSim};
use serde_json::{json, Value};
use uuid::Uuid;

async fn joined(hub: &Hub, admin: &str, name: &str, code: &str) -> SchoolSim {
    let mut sim = SchoolSim::new(name, code).await.unwrap();
    // A guardian with a phone, so a contact detail can be corrected and emptied.
    sqlx::query(
        "INSERT INTO guardians (id, school_id, full_name, phone, email, occupation, address, custom, hlc)
         VALUES ($1, $2, 'Imran Khan', '0300-1111111', NULL, '', 'Block 5', '{}', $3)",
    )
    .bind(Uuid::now_v7())
    .bind(sim.school.id)
    .bind(sim.hlc.now().to_string())
    .execute(&sim.db.pool)
    .await
    .unwrap();
    let join_code = hub.invite(admin, name).await;
    let (s, b) = sim.join(hub, &join_code).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    sim.sync(hub).await.unwrap();
    sim
}

async fn find(hub: &Hub, admin: &str, school: Uuid, kind: &str, q: &str) -> Value {
    let (s, found) = hub
        .get(&format!("/schools/{school}/records/{kind}?q={q}"), admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{found}");
    found
}

async fn correct(
    hub: &Hub,
    token: &str,
    school: Uuid,
    kind: &str,
    record: &Value,
    fields: Value,
) -> (StatusCode, Value) {
    hub.patch(
        &format!(
            "/schools/{school}/records/{kind}/{}",
            record.as_str().unwrap()
        ),
        token,
        json!({ "fields": fields }),
    )
    .await
}

async fn text(sim: &SchoolSim, sql: &'static str) -> Vec<Option<String>> {
    sqlx::query_scalar(sql)
        .fetch_all(&sim.db.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_correction_reaches_the_school_and_beats_a_school_edit_of_the_same_field() {
    let hub = hub!();
    let admin = hub.setup().await;
    let sim = joined(&hub, &admin, "Gulshan High", "KHI-01").await;
    let school = sim.school_ref.unwrap();

    // What can be corrected is a short, fixed list.
    let (s, kinds) = hub.get("/corrections/kinds", &admin).await;
    assert_eq!(s, StatusCode::OK);
    let codes: Vec<&str> = kinds
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["school", "students", "guardians", "staff"]);
    assert_eq!(
        kinds[1]["fields"],
        json!([
            { "name": "full_name", "required": true }, { "name": "father_name", "required": true }, { "name": "address", "required": false }
        ])
    );

    // Find a student by part of the name; the search is on the school's own copy.
    let found = find(&hub, &admin, school, "students", "ali").await;
    assert_eq!(
        (found["total"].as_i64(), found["items"][0]["title"].as_str()),
        (Some(1), Some("Ali Khan"))
    );
    assert!(
        found["items"][0]["hint"].is_string(),
        "the student number tells namesakes apart"
    );
    let all = find(&hub, &admin, school, "students", "").await;
    assert_eq!(all["total"], 2);
    let none = find(&hub, &admin, school, "students", "%25").await;
    assert_eq!(none["total"], 0, "a typed % is not a wildcard");
    let ali = found["items"][0]["id"].clone();

    // A plain correction: head office fixes the spelling; the school gets it at its next sync.
    let (s, done) = correct(
        &hub,
        &admin,
        school,
        "students",
        &ali,
        json!({ "full_name": " Ali Ahmed Khan " }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{done}");
    assert_eq!(
        (done["changed"].clone(), done["record"]["title"].as_str()),
        (json!(["full_name"]), Some("Ali Ahmed Khan"))
    );
    assert_eq!(
        sim.student_names().await,
        ["Ali Khan", "Sara Noor"],
        "not before a sync"
    );
    sim.sync(&hub).await.unwrap();
    assert_eq!(sim.student_names().await, ["Ali Ahmed Khan", "Sara Noor"]);
    // The school's own audit log says who did it.
    let noted = text(&sim, "SELECT new_value->>'corrected_by' FROM audit_log WHERE entity = 'student' AND new_value ? 'corrected_by'").await;
    assert_eq!(noted, [Some("Head office: Head Office".to_string())]);
    // Saying the same thing again changes nothing and queues nothing.
    let (s, same) = correct(
        &hub,
        &admin,
        school,
        "students",
        &ali,
        json!({ "full_name": "Ali Ahmed Khan" }),
    )
    .await;
    assert_eq!((s, same["changed"].clone()), (StatusCode::OK, json!([])));

    // Both sides change the same field before a sync: head office wins, and the school's
    // value is in the overwrite log on both sides.
    sim.rename_student("Sara Noor", "Sara Noor (school)").await;
    let sara = find(&hub, &admin, school, "students", "sara").await["items"][0]["id"].clone();
    let (s, _) = correct(
        &hub,
        &admin,
        school,
        "students",
        &sara,
        json!({ "full_name": "Sara Noor (head office)" }),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    sim.sync(&hub).await.unwrap();
    assert_eq!(
        sim.student_names().await,
        ["Ali Ahmed Khan", "Sara Noor (head office)"]
    );
    let mut c = sim.db.pool.acquire().await.unwrap();
    let log = madrasa_db::sync::list_overwrites(&mut c, 10, 0)
        .await
        .unwrap();
    drop(c);
    assert_eq!(log.len(), 1, "{log:?}");
    assert_eq!(
        (
            log[0].table.as_str(),
            log[0].field.as_str(),
            &log[0].kept,
            &log[0].discarded,
            log[0].reason.as_str()
        ),
        (
            "students",
            "full_name",
            &json!("Sara Noor (head office)"),
            &json!("Sara Noor (school)"),
            "head_office_wins"
        )
    );
    let (s, hub_log) = hub
        .get(&format!("/schools/{school}/overwrites"), &admin)
        .await;
    assert_eq!(
        (s, hub_log.as_array().map(Vec::len)),
        (StatusCode::OK, Some(1)),
        "{hub_log}"
    );

    // Different fields of one record do not collide: both edits stand.
    sqlx::query("UPDATE students SET address = 'House 7, Gulshan', updated_at = now(), hlc = $1 WHERE full_name = 'Ali Ahmed Khan'")
        .bind(sim.hlc.now().to_string())
        .execute(&sim.db.pool)
        .await
        .unwrap();
    let (s, _) = correct(
        &hub,
        &admin,
        school,
        "students",
        &ali,
        json!({ "father_name": "Ahmed Khan" }),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    sim.sync(&hub).await.unwrap();
    assert_eq!(
        text(&sim, "SELECT father_name || ' / ' || address FROM students WHERE full_name = 'Ali Ahmed Khan'").await,
        [Some("Ahmed Khan / House 7, Gulshan".to_string())]
    );
    let hub_copy: Vec<String> = hub.in_school(school, "SELECT father_name || ' / ' || address FROM students WHERE full_name = 'Ali Ahmed Khan'").await;
    assert_eq!(hub_copy, ["Ahmed Khan / House 7, Gulshan"]);
    let mut c = sim.db.pool.acquire().await.unwrap();
    assert_eq!(
        madrasa_db::sync::list_overwrites(&mut c, 10, 0)
            .await
            .unwrap()
            .len(),
        1,
        "nothing new was overwritten"
    );
    drop(c);

    // A contact detail: corrected, then emptied ("no phone" is stored as no value).
    let imran = find(&hub, &admin, school, "guardians", "0300").await["items"][0]["id"].clone();
    let (s, done) = correct(
        &hub,
        &admin,
        school,
        "guardians",
        &imran,
        json!({ "phone": "0300-2222222", "email": "imran@example.org" }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{done}");
    sim.sync(&hub).await.unwrap();
    assert_eq!(
        text(&sim, "SELECT phone || ' ' || email FROM guardians").await,
        [Some("0300-2222222 imran@example.org".to_string())]
    );
    let (s, _) = correct(
        &hub,
        &admin,
        school,
        "guardians",
        &imran,
        json!({ "phone": "" }),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    sim.sync(&hub).await.unwrap();
    assert_eq!(text(&sim, "SELECT phone FROM guardians").await, [None]);

    // The school's own name: corrected in its copy, in the register, and at the school.
    let record = find(&hub, &admin, school, "school", "").await["items"][0].clone();
    assert_eq!(
        (record["title"].as_str(), record["hint"].as_str()),
        (Some("Gulshan High"), Some("KHI-01"))
    );
    let (s, _) = correct(
        &hub,
        &admin,
        school,
        "school",
        &record["id"],
        json!({ "name": "Gulshan High School" }),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        hub.school_row(&admin, school).await["name"],
        "Gulshan High School"
    );
    sim.sync(&hub).await.unwrap();
    assert_eq!(
        text(&sim, "SELECT name FROM schools").await,
        [Some("Gulshan High School".to_string())]
    );

    // Every correction is in the hub's audit log with the old and the new value.
    let (_, audit) = hub.get("/audit?page_size=200", &admin).await;
    let entries: Vec<&Value> = audit
        .as_array()
        .or_else(|| audit["items"].as_array())
        .unwrap()
        .iter()
        .filter(|e| e["action"] == "correct")
        .collect();
    assert_eq!(entries.len(), 6, "{audit}");
    let first = entries.last().unwrap();
    assert_eq!(
        (
            first["old_value"]["full_name"].as_str(),
            first["new_value"]["full_name"].as_str(),
            first["new_value"]["kind"].as_str()
        ),
        (Some("Ali Khan"), Some("Ali Ahmed Khan"), Some("students"))
    );

    sim.db.cleanup().await;
    hub.finish().await;
}

#[tokio::test]
async fn corrections_are_limited_to_names_and_contact_details_of_joined_schools() {
    let hub = hub!();
    let admin = hub.setup().await;
    let gulshan = joined(&hub, &admin, "Gulshan High", "KHI-01").await;
    let clifton = joined(&hub, &admin, "Clifton Grammar", "KHI-02").await;
    let (g, c) = (gulshan.school_ref.unwrap(), clifton.school_ref.unwrap());
    let ali = find(&hub, &admin, g, "students", "ali").await["items"][0]["id"].clone();

    // Administrators only.
    let (s, b) = hub.post("/users", &admin, json!({ "username": "reports", "full_name": "Reports", "password": "user-password-1", "permissions": ["nexus.reports", "nexus.escrow"] })).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let reports = hub.login("reports", "user-password-1").await;
    let (s, _) = hub
        .get(&format!("/schools/{g}/records/students"), &reports)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = correct(
        &hub,
        &reports,
        g,
        "students",
        &ali,
        json!({ "full_name": "Changed" }),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = hub.get("/corrections/kinds", &reports).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = hub
        .call(
            Method::GET,
            &format!("/schools/{g}/records/students"),
            gulshan.token.as_deref(),
            None,
        )
        .await;
    assert_eq!(
        s,
        StatusCode::UNAUTHORIZED,
        "a school's sync token is not a console login"
    );

    // Only the listed fields, with sensible values.
    for (fields, message) in [
        (
            json!({ "date_of_birth": "2014-01-01" }),
            "Head office cannot change \"date_of_birth\" of this kind of record.",
        ),
        (
            json!({ "status": "left" }),
            "Head office cannot change \"status\" of this kind of record.",
        ),
        (
            json!({ "full_name": "  " }),
            "The full name cannot be empty.",
        ),
        (json!({ "full_name": 5 }), "The full name must be text."),
        (json!({}), "Nothing to change."),
        (
            json!({ "full_name": "Ali", "student_number": "X-1" }),
            "Head office cannot change \"student_number\" of this kind of record.",
        ),
    ] {
        let (s, e) = correct(&hub, &admin, g, "students", &ali, fields).await;
        assert_eq!(
            (s, e["error"]["message"].as_str()),
            (StatusCode::BAD_REQUEST, Some(message))
        );
    }
    // Money, marks, accounts: not a kind at all.
    for kind in ["fee_receipts", "users", "fee_vouchers", "audit_log"] {
        let (s, _) = hub
            .get(&format!("/schools/{g}/records/{kind}"), &admin)
            .await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{kind}");
        let (s, _) = correct(&hub, &admin, g, kind, &ali, json!({ "full_name": "x" })).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{kind}");
    }
    assert_eq!(gulshan.student_names().await, ["Ali Khan", "Sara Noor"]);

    // Each school's records are its own: Gulshan's student is not found through Clifton.
    let (s, e) = correct(
        &hub,
        &admin,
        c,
        "students",
        &ali,
        json!({ "full_name": "Wrong school" }),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{e}");
    let (s, _) = correct(
        &hub,
        &admin,
        Uuid::now_v7(),
        "students",
        &ali,
        json!({ "full_name": "No school" }),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let names: Vec<String> = hub
        .in_school(g, "SELECT full_name FROM students ORDER BY 1")
        .await;
    assert_eq!(names, ["Ali Khan", "Sara Noor"]);

    // A school that has left is an archive: nothing in it is corrected.
    let (s, _) = hub
        .post(
            &format!("/schools/{c}/remove"),
            &admin,
            json!({ "reason": "Closed" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, e) = hub
        .get(&format!("/schools/{c}/records/students"), &admin)
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{e}");
    assert_eq!(
        e["error"]["message"],
        "Only the records of a school that is joined can be corrected."
    );

    gulshan.db.cleanup().await;
    clifton.db.cleanup().await;
    hub.finish().await;
}
