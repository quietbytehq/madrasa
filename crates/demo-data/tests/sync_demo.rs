//! Nexus sync with a whole demo school: the first upload reproduces every synced table at
//! the hub copy exactly, and nothing secret travels.

use madrasa_core::hlc::Hlc;
use madrasa_core::sync::{policy, Origin, TABLES};
use madrasa_db::testing::TestDb;
use madrasa_db::{sync, test_db, PgPool};
use madrasa_demo_data::{generate, load, DemoOptions, LoadOptions};
use serde_json::Value;

/// The school's records of one table (instance-wide rows without a school are not synced).
async fn rows(pool: &PgPool, table: &str, school_id: uuid::Uuid) -> Vec<Value> {
    let p = policy(table).unwrap();
    let owner = if table == "schools" {
        "id"
    } else {
        "school_id"
    };
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('TimeZone', 'UTC', true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    let rows: Vec<Value> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT sync_row(to_jsonb(t)) FROM {table} t WHERE {owner} = $1 ORDER BY id"
    )))
    .bind(school_id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    rows.into_iter()
        .map(|mut r| {
            r.as_object_mut().unwrap().retain(|k, _| !p.is_excluded(k));
            r
        })
        .collect()
}

#[tokio::test]
async fn a_demo_school_uploads_completely_and_exactly() {
    let school: TestDb = test_db!();
    let hub: TestDb = test_db!();
    let demo = generate(&DemoOptions {
        students: 60,
        staff: 40,
        ..DemoOptions::default()
    });
    let report = load(
        &school.pool,
        &Hlc::new(1),
        &demo,
        &LoadOptions {
            confirmed: true,
            out_dir: None,
            full_year: false,
        },
    )
    .await
    .unwrap();

    let started = std::time::Instant::now();
    let mut c = school.pool.begin().await.unwrap();
    sync::enable(&mut c, report.school_id, Origin::School)
        .await
        .unwrap();
    let queued = sync::queue_snapshot(&mut c).await.unwrap();
    let first = sync::collect(&mut c, 1).await.unwrap();
    c.commit().await.unwrap();
    let mut h = hub.pool.begin().await.unwrap();
    sync::adopt_school(&mut h, &first[0], Origin::HeadOffice)
        .await
        .unwrap();
    h.commit().await.unwrap();

    let (mut sent, mut batches) = (0usize, 0usize);
    loop {
        let mut c = school.pool.begin().await.unwrap();
        let batch = sync::collect(&mut c, madrasa_core::sync::BATCH_SIZE as i64)
            .await
            .unwrap();
        c.commit().await.unwrap();
        if batch.is_empty() {
            break;
        }
        let text = serde_json::to_string(&batch).unwrap();
        assert!(
            !text.contains("$argon2"),
            "a password hash was about to leave the school"
        );
        let mut h = hub.pool.begin().await.unwrap();
        let applied = sync::apply(&mut h, &batch).await.unwrap();
        h.commit().await.unwrap();
        assert!(
            applied.parked.is_empty(),
            "nothing waits after a batch: {:?}",
            applied.parked
        );
        assert!(applied.overwrites.is_empty());
        let mut c = school.pool.begin().await.unwrap();
        sync::acknowledge(&mut c, &applied.handled).await.unwrap();
        c.commit().await.unwrap();
        sent += batch.len();
        batches += 1;
    }
    eprintln!(
        "first upload: {queued} records queued, {sent} sent in {batches} batches, {:.1}s",
        started.elapsed().as_secs_f32()
    );
    assert_eq!(sent as i64, queued);

    let mut total = 0;
    for p in TABLES.iter().filter(|p| p.is_synced()) {
        let here = rows(&school.pool, p.table, report.school_id).await;
        let there = rows(&hub.pool, p.table, report.school_id).await;
        assert_eq!(here.len(), there.len(), "{}: row count", p.table);
        assert_eq!(
            here, there,
            "{} differs between the school and the hub copy",
            p.table
        );
        total += here.len();
    }
    assert_eq!(total as i64, queued, "every queued record is at the hub");
    assert!(
        total > 5_000,
        "a demo school is thousands of records: {total}"
    );

    // Nothing that must stay at the school is at the hub.
    let hashes: Vec<String> = sqlx::query_scalar("SELECT DISTINCT password_hash FROM users")
        .fetch_all(&hub.pool)
        .await
        .unwrap();
    assert_eq!(hashes, ["!"]);
    for table in [
        "settings",
        "sessions",
        "api_keys",
        "school_sequences",
        "backup_runs",
    ] {
        let n: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(&hub.pool)
                .await
                .unwrap();
        assert_eq!(n, 0, "{table} must stay at the school");
    }
    // The upload leaves nothing queued on either side.
    for pool in [&school.pool, &hub.pool] {
        let mut c = pool.acquire().await.unwrap();
        assert_eq!(sync::status(&mut c).await.unwrap().pending, 0);
    }
    school.cleanup().await;
    hub.cleanup().await;
}
