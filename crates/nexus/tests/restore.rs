//! v0.4.3: a school that restored a backup while joined tells the hub at its next sync. The
//! hub sets its copy aside as an archive and starts a fresh one from the school record; the
//! archive goes when the school is purged. Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

mod common;

use axum::http::{Method, StatusCode};
use common::{joined, Hub};
use madrasa_db::sync;
use serde_json::{json, Value};

async fn namespaces(hub: &Hub, pattern: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT nspname::text FROM pg_namespace WHERE nspname LIKE $1 ORDER BY 1")
        .bind(pattern)
        .fetch_all(&hub.state.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_restored_school_gets_a_fresh_copy_and_the_old_one_is_archived_until_purged() {
    let hub = hub!();
    let admin = hub.setup().await;
    let gulshan = joined(&hub, &admin, "Gulshan High", "KHI-01").await;
    let clifton = joined(&hub, &admin, "Clifton Grammar", "KHI-02").await;
    gulshan.sync(&hub).await.unwrap();
    clifton.sync(&hub).await.unwrap();
    let g = gulshan.school_ref.unwrap();
    let students = |id| {
        let hub = &hub;
        async move {
            hub.in_school::<String>(id, "SELECT full_name FROM students ORDER BY full_name")
                .await
        }
    };
    assert_eq!(students(g).await, ["Ali Khan", "Sara Noor"]);

    let record = {
        let mut c = gulshan.db.pool.acquire().await.unwrap();
        sync::school_record(&mut c, gulshan.school.id)
            .await
            .unwrap()
    };
    let request = |school: Value| {
        json!({
            "school": school, "app_version": env!("CARGO_PKG_VERSION"),
            "schema_version": madrasa_nexus::tenants::hub_schema_version(),
            "backup_created_at": "2026-11-01T08:00:00Z", "restored_at": "2026-11-13T08:00:00Z",
        })
    };
    let call = |token: Option<String>, body: Value| {
        let hub = &hub;
        async move {
            hub.call(Method::POST, "/sync/restored", token.as_deref(), Some(body))
                .await
        }
    };

    // Only the school itself, with its own record.
    let (s, _) = call(None, request(json!(record))).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, e) = call(clifton.token.clone(), request(json!(record))).await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("validation")),
        "another school's record: {e}"
    );
    assert_eq!(
        students(g).await,
        ["Ali Khan", "Sara Noor"],
        "nothing changed"
    );

    let (s, done) = call(gulshan.token.clone(), request(json!(record))).await;
    assert_eq!(s, StatusCode::OK, "{done}");
    let archive = done["archive"].as_str().unwrap().to_string();
    assert!(
        madrasa_nexus::tenants::is_archive_name(&archive),
        "{archive}"
    );

    // A fresh copy: the school record only, ready for the school to send everything again.
    assert!(students(g).await.is_empty());
    let schools: Vec<String> = hub.in_school(g, "SELECT name FROM schools").await;
    assert_eq!(schools, ["Gulshan High"]);
    // The old copy is kept, whole, under the archive's name; the other school is untouched.
    let old: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT full_name FROM \"{archive}\".students ORDER BY full_name"
    )))
    .fetch_all(&hub.state.pool)
    .await
    .unwrap();
    assert_eq!(old, ["Ali Khan", "Sara Noor"]);
    assert_eq!(
        students(clifton.school_ref.unwrap()).await,
        ["Ali Khan", "Sara Noor"]
    );
    // The school sends again and the copy is complete.
    {
        let mut c = gulshan.db.pool.begin().await.unwrap();
        sync::queue_snapshot(&mut c).await.unwrap();
        c.commit().await.unwrap();
    }
    let report = gulshan.sync(&hub).await.unwrap();
    assert!(report.parked_at_hub.is_empty(), "{report:?}");
    assert_eq!(students(g).await, ["Ali Khan", "Sara Noor"]);
    // Logged at head office.
    let (_, log) = hub.get("/audit?page_size=50", &admin).await;
    let entries = log.as_array().or_else(|| log["items"].as_array()).unwrap();
    assert!(
        entries.iter().any(|e| e["action"] == "school_restored"),
        "{entries:?}"
    );

    // Purging the school drops its copy and the archive.
    let (s, _) = hub
        .post(
            &format!("/schools/{g}/remove"),
            &admin,
            json!({ "reason": "Closed." }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, purged) = hub
        .post(
            &format!("/schools/{g}/purge"),
            &admin,
            json!({ "confirm_code": "KHI-01" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{purged}");
    assert!(
        namespaces(&hub, "a\\_%").await.is_empty(),
        "the archive is gone"
    );
    let dropped: i64 =
        sqlx::query_scalar("SELECT count(*) FROM hub_school_archives WHERE dropped_at IS NOT NULL")
            .fetch_one(&hub.state.pool)
            .await
            .unwrap();
    assert_eq!(dropped, 1);
    assert_eq!(
        namespaces(&hub, "t\\_%").await.len(),
        1,
        "Clifton's copy stays"
    );

    hub.finish().await;
}
