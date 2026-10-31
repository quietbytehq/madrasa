//! M1 stage 5: dump and restore. Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

use madrasa_core::hlc::Hlc;
use madrasa_core::password::hash_password;
use madrasa_db::testing::TestDb;
use madrasa_db::{academics, audit, backup, schools, test_db, users, Actor, DbError};
use uuid::Uuid;

fn hlc() -> Hlc {
    Hlc::new(17)
}

async fn seed(pool: &madrasa_db::PgPool) -> (Uuid, Uuid) {
    let mut c = pool.acquire().await.unwrap();
    let school = schools::create(
        &mut c,
        &hlc(),
        Actor::SYSTEM,
        schools::NewSchool {
            name: "Gulshan".into(),
            code: "KHI-MC".into(),
            city: "Karachi".into(),
            is_demo: false,
            academic_year_start_month: 4,
            currency: "PKR".into(),
            country: "PK".into(),
            time_zone: "Asia/Karachi".into(),
        },
    )
    .await
    .unwrap();
    let actor = Actor::system_for(school.id);
    let admin = users::create(
        &mut c,
        &hlc(),
        actor,
        users::NewUser {
            school_id: school.id,
            username: "admin".into(),
            full_name: "Admin".into(),
            email: None,
            password_hash: hash_password("admin-password-1").unwrap(),
        },
    )
    .await
    .unwrap();
    academics::create_class(&mut c, &hlc(), actor, school.id, "1", 1)
        .await
        .unwrap();
    (school.id, admin.id)
}

#[tokio::test]
async fn tables_are_ordered_parents_first_and_dump_round_trips() {
    let db: TestDb = test_db!();
    let (school_id, admin) = seed(&db.pool).await;
    let mut c = db.pool.acquire().await.unwrap();
    let order = backup::table_order(&mut c).await.unwrap();
    let pos = |name: &str| order.iter().position(|t| t == name).unwrap();
    assert!(pos("schools") < pos("users"));
    assert!(pos("students") < pos("enrollments"));
    assert!(pos("sections") < pos("enrollments"));
    assert!(pos("fee_vouchers") < pos("fee_receipts"));
    assert!(pos("mark_sheets") < pos("marks"));
    assert!(!order.iter().any(|t| t == "_sqlx_migrations"));
    let versions = backup::migration_versions(&mut c).await.unwrap();
    assert_eq!(versions.len(), 9);
    backup::check_schema(&versions, &versions).unwrap();
    assert!(matches!(
        backup::check_schema(&versions, &versions[..8]),
        Err(DbError::Conflict(_))
    ));

    let before = backup::record_counts(&mut c).await.unwrap();
    assert_eq!((before["schools"], before["users"]), (1, 1));
    let dump = backup::dump(&mut c).await.unwrap();
    assert!(dump.len() > 200);

    // Change the database after the dump, then restore and expect the dump's state.
    let actor = Actor::user(admin, school_id);
    academics::create_class(&mut c, &hlc(), actor, school_id, "2", 2)
        .await
        .unwrap();
    users::create(
        &mut c,
        &hlc(),
        actor,
        users::NewUser {
            school_id,
            username: "extra".into(),
            full_name: "Extra".into(),
            email: None,
            password_hash: hash_password("user-password-1").unwrap(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        academics::list_classes(&mut c, school_id)
            .await
            .unwrap()
            .len(),
        2
    );
    let audit_before = audit::list_recent(&mut c, Some(school_id), 100)
        .await
        .unwrap()
        .len();

    let report = backup::restore(&mut c, &dump).await.unwrap();
    assert!(report.tables >= 30);
    assert!(report.rows > 0);
    assert_eq!(
        academics::list_classes(&mut c, school_id)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(users::find_by_username(&mut c, school_id, "extra")
        .await
        .unwrap()
        .is_none());
    let after = backup::record_counts(&mut c).await.unwrap();
    assert_eq!(after, before, "counts match the dump");
    // The audit log is restored too (it is part of the data), and still append-only after.
    let audit_after = audit::list_recent(&mut c, Some(school_id), 100)
        .await
        .unwrap()
        .len();
    assert!(audit_after < audit_before);
    let res = sqlx::query("DELETE FROM audit_log").execute(&mut *c).await;
    assert!(res.is_err(), "triggers are re-enabled after the restore");
    // Restoring the same dump again is idempotent.
    let again = backup::restore(&mut c, &dump).await.unwrap();
    assert_eq!(again, report);
    // Garbage and a truncated dump are refused before anything is touched.
    assert!(matches!(
        backup::restore(&mut c, b"not a dump").await,
        Err(DbError::Conflict(_))
    ));
    assert!(matches!(
        backup::restore(&mut c, &dump[..dump.len() / 2]).await,
        Err(DbError::Conflict(_))
    ));
    assert_eq!(
        academics::list_classes(&mut c, school_id)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn run_history_and_audit() {
    let db: TestDb = test_db!();
    let (school_id, admin) = seed(&db.pool).await;
    let mut c = db.pool.acquire().await.unwrap();
    let actor = Actor::user(admin, school_id);
    assert!(backup::last_ok_run(&mut c, school_id)
        .await
        .unwrap()
        .is_none());
    let run = backup::start_run(&mut c, &hlc(), actor, school_id, "manual")
        .await
        .unwrap();
    assert_eq!(run.status, "running");
    let done = backup::finish_run(
        &mut c,
        &hlc(),
        actor,
        run.id,
        Ok(("/tmp/x.mbk".into(), 1234)),
    )
    .await
    .unwrap();
    assert_eq!((done.status.as_str(), done.file_size), ("ok", Some(1234)));
    let failed = backup::start_run(
        &mut c,
        &hlc(),
        Actor::system_for(school_id),
        school_id,
        "scheduled",
    )
    .await
    .unwrap();
    backup::finish_run(
        &mut c,
        &hlc(),
        Actor::system_for(school_id),
        failed.id,
        Err("disk full".into()),
    )
    .await
    .unwrap();
    let last = backup::last_ok_run(&mut c, school_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(last.id, done.id);
    let recent = backup::recent_runs(&mut c, school_id, 10).await.unwrap();
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].error.as_deref(), Some("disk full"));
    let trail: Vec<String> = audit::list_recent(&mut c, Some(school_id), 10)
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.entity == "backup")
        .map(|e| e.action)
        .collect();
    assert_eq!(
        trail,
        ["backup", "backup", "backup", "backup"],
        "start and finish of both runs"
    );
    assert_eq!(backup::fail_stale_runs(&mut c, &hlc()).await.unwrap(), 0);
}
