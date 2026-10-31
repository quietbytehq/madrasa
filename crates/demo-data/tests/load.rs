use madrasa_core::hlc::Hlc;
use madrasa_core::password::verify_password;
use madrasa_db::testing::TestDb;
use madrasa_db::{roles, schools, settings, test_db, users, Actor};
use madrasa_demo_data::{generate, load, DemoOptions, LoadError, LoadOptions, DEMO_PASSWORD};

#[tokio::test]
async fn loads_into_an_empty_database_and_marks_everything_demo() {
    let db: TestDb = test_db!();
    let demo = generate(&DemoOptions {
        students: 50,
        staff: 8,
        ..DemoOptions::default()
    });
    let hlc = Hlc::new(1);

    // Never implicit.
    let err = load(&db.pool, &hlc, &demo, &LoadOptions::default())
        .await
        .unwrap_err();
    assert!(matches!(err, LoadError::NotConfirmed));

    let out = std::env::temp_dir().join(format!("madrasa-demo-{}", uuid::Uuid::new_v4()));
    let report = load(
        &db.pool,
        &hlc,
        &demo,
        &LoadOptions {
            confirmed: true,
            out_dir: Some(out.clone()),
            full_year: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(report.staff_accounts, 8);
    assert_eq!(report.students, 50);
    assert_eq!(report.sections, 39);
    assert_eq!(report.vouchers, 100, "April and May for 50 students");
    assert!(
        report.receipts > 20 && report.receipts < 45,
        "{}",
        report.receipts
    );
    // April + May 2026 with Sunday off, Labour Day and four Eid days: 26 + 21 school days.
    assert_eq!(report.staff_attendance_days, 47);
    assert!(
        report.attendance_sheets > 0 && report.attendance_sheets % 47 == 0,
        "{}",
        report.attendance_sheets
    );
    assert!(
        report.mark_sheets > 0,
        "Test 1 and Mid-year marks for every section and subject"
    );
    assert_eq!(report.files_written.len(), 6);
    assert!(out.join("students.json").exists());
    std::fs::remove_dir_all(&out).ok();

    let mut conn = db.pool.acquire().await.unwrap();
    let school = schools::get(&mut conn, report.school_id).await.unwrap();
    assert!(school.is_demo);
    assert!(school.name.starts_with("DEMO"));
    assert_eq!(users::count(&mut conn, school.id).await.unwrap(), 8);
    assert!(settings::setup_completed(&mut conn).await.unwrap());

    let principal = users::find_by_username(&mut conn, school.id, &demo.staff[0].username)
        .await
        .unwrap()
        .unwrap();
    assert!(verify_password(DEMO_PASSWORD, &principal.password_hash));
    assert_eq!(
        roles::codes_for_user(&mut conn, principal.id)
            .await
            .unwrap(),
        ["principal"]
    );

    // Loading twice is refused.
    drop(conn);
    let err = load(
        &db.pool,
        &hlc,
        &demo,
        &LoadOptions {
            confirmed: true,
            out_dir: None,
            full_year: false,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, LoadError::AlreadyLoaded));
    db.cleanup().await;
}

#[tokio::test]
async fn refuses_when_a_real_school_exists() {
    let db: TestDb = test_db!();
    let hlc = Hlc::new(1);
    let mut conn = db.pool.acquire().await.unwrap();
    schools::create(
        &mut conn,
        &hlc,
        Actor::SYSTEM,
        schools::NewSchool {
            name: "Real School".into(),
            code: "REAL1".into(),
            city: "".into(),
            is_demo: false,
            academic_year_start_month: 4,
            currency: "PKR".into(),
        },
    )
    .await
    .unwrap();
    drop(conn);
    let demo = generate(&DemoOptions {
        students: 10,
        staff: 5,
        ..DemoOptions::default()
    });
    let err = load(
        &db.pool,
        &hlc,
        &demo,
        &LoadOptions {
            confirmed: true,
            out_dir: None,
            full_year: false,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, LoadError::RealSchoolPresent));
    let mut conn = db.pool.acquire().await.unwrap();
    assert_eq!(
        schools::count(&mut conn, true).await.unwrap(),
        1,
        "nothing was inserted"
    );
    drop(conn);
    db.cleanup().await;
}
