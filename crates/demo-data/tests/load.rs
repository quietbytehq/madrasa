use madrasa_core::hlc::Hlc;
use madrasa_core::password::verify_password;
use madrasa_db::testing::TestDb;
use madrasa_db::{roles, schools, settings, test_db, users, Actor};
use madrasa_demo_data::{generate, load, DemoOptions, LoadError, LoadOptions, DEMO_PASSWORD};

#[tokio::test]
async fn loads_into_an_empty_database_and_marks_everything_demo() {
    let db: TestDb = test_db!();
    // Enough teachers for the 39 sections: a timetable needs roughly one per section.
    let demo = generate(&DemoOptions {
        students: 50,
        staff: 40,
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
    assert_eq!(report.staff_accounts, 40);
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
    // Standard modules ride along, out of sight until the edition is switched.
    let std = &report.standard;
    assert!(
        std.timetable_cards > 0,
        "the demo timetable has cards; notes: {:?}",
        std.notes
    );
    assert!(
        std.timetable_published,
        "the demo timetable is published; notes: {:?}",
        std.notes
    );
    assert!(std.notes.is_empty(), "{:?}", std.notes);
    assert_eq!(std.applications, 12);
    assert_eq!(std.books, 20);
    assert!(std.loans >= 1, "{}", std.loans);
    assert_eq!(std.routes, 2);
    assert!(std.transport_students > 0);
    assert!(std.hostel_residents > 0);
    assert_eq!(std.inventory_items, 12);
    assert!(std.leave_requests >= 1);
    assert!(std.api_key_prefix.starts_with("mk_demo"));
    assert_eq!(report.files_written.len(), 6);
    assert!(out.join("students.json").exists());
    std::fs::remove_dir_all(&out).ok();

    let mut conn = db.pool.acquire().await.unwrap();
    let school = schools::get(&mut conn, report.school_id).await.unwrap();
    assert!(school.is_demo);
    assert!(school.name.starts_with("DEMO"));
    assert_eq!(users::count(&mut conn, school.id).await.unwrap(), 40);
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
            country: "PK".into(),
            time_zone: "Asia/Karachi".into(),
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

/// When the generated data cannot make a timetable (three teachers for 39 sections), the
/// loader says so in a note the wizard shows, instead of skipping it silently.
#[tokio::test]
async fn an_impossible_timetable_leaves_a_visible_note() {
    let db: TestDb = test_db!();
    let demo = generate(&DemoOptions {
        students: 20,
        staff: 8,
        ..DemoOptions::default()
    });
    let hlc = Hlc::new(1);
    let report = load(
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
    .unwrap();
    let std = &report.standard;
    assert_eq!(std.timetable_cards, 0);
    assert!(!std.timetable_published);
    assert_eq!(std.notes.len(), 1, "{:?}", std.notes);
    assert!(
        std.notes[0].starts_with("The demo timetable was not generated: the timetable check found"),
        "{:?}",
        std.notes
    );
    db.cleanup().await;
}
