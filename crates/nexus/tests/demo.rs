//! Nexus stage 9: the demo network. Three generated schools appear in an empty hub as if
//! they had joined and synced, each in its own schema; the console's reports, corrections
//! and register work on them; a hub that already has schools is left alone. Needs
//! PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

mod common;

use axum::http::StatusCode;
use madrasa_nexus::demo::{parent_codes, seed_network, DemoError, DEMO_NETWORK};
use serde_json::{json, Value};

#[tokio::test]
async fn the_demo_network_fills_an_empty_hub_with_three_separate_schools() {
    let hub = hub!();
    let admin = hub.setup().await;
    let schools = seed_network(&hub.state.pool, &hub.state.hlc).await.unwrap();
    assert_eq!(schools.len(), 3);

    // The register shows them as joined, with nothing waiting.
    let (s, list) = hub.get("/schools", &admin).await;
    assert_eq!(s, StatusCode::OK, "{list}");
    let rows = list.as_array().unwrap();
    let mut codes: Vec<&str> = rows.iter().map(|r| r["code"].as_str().unwrap()).collect();
    codes.sort_unstable();
    assert_eq!(codes, ["DEMO-I", "DEMO-N", "DEMO-S"]);
    for row in rows {
        assert_eq!(row["status"], "active", "{row}");
        assert_eq!(
            row["sync"]["pending"], 0,
            "a demo school has nobody to send to: {row}"
        );
        assert!(row["last_sync_at"].is_string());
    }

    // Each school is in a schema of its own with its own people: the sizes differ as asked,
    // and no student id appears in two schools.
    let mut all_ids: Vec<uuid::Uuid> = Vec::new();
    for (school, spec) in schools.iter().zip(DEMO_NETWORK) {
        let students: Vec<i64> = hub
            .in_school(school.id, "SELECT count(*) FROM students")
            .await;
        assert_eq!(students[0] as usize, spec.students, "{}", spec.code);
        let named: Vec<String> = hub
            .in_school(
                school.id,
                "SELECT name || '|' || code || '|' || currency FROM schools",
            )
            .await;
        assert_eq!(
            named,
            [format!("{}|{}|{}", spec.name, spec.code, spec.currency)]
        );
        // No password hash is kept in a hub copy.
        let hashes: Vec<String> = hub
            .in_school(school.id, "SELECT DISTINCT password_hash FROM users")
            .await;
        assert_eq!(hashes, ["!"]);
        all_ids.extend(
            hub.in_school::<uuid::Uuid>(school.id, "SELECT id FROM students")
                .await,
        );
    }
    let total = all_ids.len();
    all_ids.sort_unstable();
    all_ids.dedup();
    assert_eq!(all_ids.len(), total);
    assert_eq!(total, 320 + 220 + 140);

    // Reports run over all three; money is totalled per currency, never across.
    let (s, summary) = hub.get("/reports/summary", &admin).await;
    assert_eq!(s, StatusCode::OK, "{summary}");
    assert_eq!(
        (
            summary["schools"].as_i64(),
            summary["rows"].as_array().map(Vec::len)
        ),
        (Some(3), Some(3))
    );
    let (s, fees) = hub.get("/reports/fees", &admin).await;
    assert_eq!(s, StatusCode::OK, "{fees}");
    let currency = fees["columns"]
        .as_array()
        .unwrap()
        .iter()
        .position(|c| c["key"] == "currency")
        .unwrap();
    let mut totals: Vec<&str> = fees["totals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t[currency].as_str().unwrap())
        .collect();
    totals.sort_unstable();
    assert_eq!(totals, ["AED", "PKR"], "{}", fees["totals"]);
    assert!(!fees["rows"].as_array().unwrap().is_empty());

    // A correction can be made in a demo school like in any other.
    let north = schools.iter().find(|s| s.code == "DEMO-N").unwrap().id;
    let (s, found) = hub
        .get(&format!("/schools/{north}/records/students?q=a"), &admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{found}");
    let record = &found["items"][0];
    let (s, done) = hub
        .patch(
            &format!(
                "/schools/{north}/records/students/{}",
                record["id"].as_str().unwrap()
            ),
            &admin,
            json!({ "fields": { "address": "House 1, Demo Street" } }),
        )
        .await;
    assert_eq!(
        (s, done["changed"].clone()),
        (StatusCode::OK, json!(["address"])),
        "{done}"
    );

    // Nothing can sync as a demo school: there is no key to present.
    let (s, _) = hub
        .call(axum::http::Method::GET, "/sync/status", Some(""), None)
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let keys: i64 =
        sqlx::query_scalar("SELECT count(*) FROM hub_schools WHERE token_hash IS NOT NULL")
            .fetch_one(&hub.state.pool)
            .await
            .unwrap();
    assert_eq!(keys, 0);

    // ---- the parent app can be tried at once: a code for a made-up guardian of each ----
    let codes = parent_codes(&hub.state.pool, &hub.state.hlc).await.unwrap();
    assert_eq!(codes.len(), 3);
    let first = &codes[0];
    assert!(!first.children.is_empty(), "{first:?}");
    let activate = |code: String, phone: &'static str| {
        let hub = &hub;
        async move {
            hub.call(
                axum::http::Method::POST,
                "/parent/activate",
                None,
                Some(json!({ "code": code, "phone": phone, "password": "demo-parent-pass-1" })),
            )
            .await
        }
    };
    let (s, opened) = activate(first.code.clone(), "0300-1000001").await;
    assert_eq!(s, StatusCode::OK, "{opened}");
    let token = opened["token"].as_str().unwrap().to_string();
    let (s, list) = hub.get("/parent/children", &token).await;
    assert_eq!(s, StatusCode::OK, "{list}");
    let kids = list["children"].as_array().unwrap();
    let mut names: Vec<&str> = kids
        .iter()
        .map(|k| k["full_name"].as_str().unwrap())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        first
            .children
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    );
    // The demo conversation, the notices and every other section answer for those children.
    let mut messages = 0;
    for kid in kids {
        assert_eq!(kid["school_name"], first.school_name);
        let base = format!(
            "/parent/children/{}/{}",
            kid["school_ref"].as_str().unwrap(),
            kid["student_id"].as_str().unwrap()
        );
        for tail in [
            "",
            "/attendance",
            "/results",
            "/fees",
            "/notices",
            "/messages",
        ] {
            let (s, b) = hub.get(&format!("{base}{tail}"), &token).await;
            assert_eq!(s, StatusCode::OK, "{tail}: {b}");
            if tail == "/messages" {
                messages += b["messages"].as_array().unwrap().len();
            }
            if tail == "/notices" {
                let titles: Vec<&str> = b["notices"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|n| n["title"].as_str().unwrap())
                    .collect();
                assert!(
                    titles.contains(&"Parent teacher meeting on Saturday"),
                    "{titles:?}"
                );
                assert!(!titles.contains(&"Winter uniform"), "a draft is not shown");
            }
        }
    }
    assert_eq!(
        messages, 3,
        "the demo conversation is with one of the children"
    );
    // A code works once; asking again gives new codes and retires the unused old ones.
    let (s, _) = activate(first.code.clone(), "0300-1000002").await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let again = parent_codes(&hub.state.pool, &hub.state.hlc).await.unwrap();
    assert_eq!(again.len(), 3);
    assert_ne!(again[1].code, codes[1].code);
    let (s, _) = activate(codes[1].code.clone(), "0300-1000003").await;
    assert_eq!(s, StatusCode::FORBIDDEN, "replaced by a newer code");
    let (s, _) = activate(again[1].code.clone(), "0300-1000003").await;
    assert_eq!(s, StatusCode::OK);

    // The hub's log says where these schools came from.
    let (_, audit) = hub.get("/audit?page_size=200", &admin).await;
    let entries = audit
        .as_array()
        .or_else(|| audit["items"].as_array())
        .unwrap();
    assert_eq!(
        entries
            .iter()
            .filter(|e| e["action"] == "demo_school")
            .count(),
        3
    );

    // Loading it twice, or into a hub that has schools, is refused and changes nothing.
    let again = seed_network(&hub.state.pool, &hub.state.hlc).await;
    assert!(matches!(again, Err(DemoError::HubNotEmpty)), "{again:?}");
    let (_, list) = hub.get("/schools", &admin).await;
    assert_eq!(list.as_array().map(Vec::len), Some(3));

    // When done: remove and purge, like any school.
    let (s, _) = hub
        .post(
            &format!("/schools/{north}/remove"),
            &admin,
            json!({ "reason": "Trial over" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, purged) = hub
        .post(
            &format!("/schools/{north}/purge"),
            &admin,
            json!({ "confirm_code": "DEMO-N" }),
        )
        .await;
    assert_eq!(
        (s, purged["status"].as_str()),
        (StatusCode::OK, Some("purged")),
        "{purged}"
    );
    let (_, summary): (_, Value) = hub.get("/reports/summary", &admin).await;
    assert_eq!(summary["schools"], 2);

    hub.finish().await;
}
