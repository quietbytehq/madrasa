//! M1 stage 6: import batches and rollback. Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

use chrono::NaiveDate;
use madrasa_core::hlc::Hlc;
use madrasa_core::password::hash_password;
use madrasa_db::testing::TestDb;
use madrasa_db::{
    academics, audit, fees, guardians, imports, schools, staff, students, test_db, users, Actor,
    DbError,
};
use rust_decimal_macros::dec;
use serde_json::json;
use uuid::Uuid;

fn hlc() -> Hlc {
    Hlc::new(19)
}
fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

#[tokio::test]
async fn batch_records_effects_and_rolls_them_back() {
    let db: TestDb = test_db!();
    let mut c = db.pool.acquire().await.unwrap();
    let school = schools::create(
        &mut c,
        &hlc(),
        Actor::SYSTEM,
        schools::NewSchool {
            name: "T".into(),
            code: "KHI-MC".into(),
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
    let sys = Actor::system_for(school.id);
    let admin = users::create(
        &mut c,
        &hlc(),
        sys,
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
    let actor = Actor::user(admin.id, school.id);
    let (name, start, end) = academics::default_year_bounds(2026, 4).unwrap();
    let year = academics::create_year(
        &mut c,
        &hlc(),
        actor,
        academics::NewAcademicYear {
            school_id: school.id,
            name,
            start_date: start,
            end_date: end,
            start_month: 4,
        },
    )
    .await
    .unwrap();
    academics::activate_year(&mut c, &hlc(), actor, year.id)
        .await
        .unwrap();
    let class = academics::create_class(&mut c, &hlc(), actor, school.id, "1", 1)
        .await
        .unwrap();
    let section = academics::create_section(
        &mut c,
        &hlc(),
        actor,
        academics::NewSection {
            school_id: school.id,
            class_id: class.id,
            academic_year_id: year.id,
            name: "A".into(),
            class_teacher_id: None,
            capacity: None,
        },
    )
    .await
    .unwrap();
    let new_student = |name: &str| students::NewStudent {
        school_id: school.id,
        student_number: None,
        legacy_id: None,
        full_name: name.into(),
        father_name: "Father".into(),
        date_of_birth: d(2015, 1, 1),
        gender: "male".into(),
        cnic: None,
        address: "".into(),
        admission_date: d(2026, 4, 1),
        board_roll_no: None,
        board_registration_no: None,
        custom: json!({}),
    };
    // An existing student the import will update.
    let existing = students::create(&mut c, &hlc(), actor, new_student("Old Name"))
        .await
        .unwrap();

    // Simulate what the API does when applying an import.
    let batch = imports::create_batch(
        &mut c,
        &hlc(),
        actor,
        imports::NewBatch {
            school_id: school.id,
            kind: "students".into(),
            file_name: "students.xlsx".into(),
            options: json!({ "duplicates": "update" }),
            total_rows: 3,
            applied_by: admin.id,
        },
    )
    .await
    .unwrap();
    let created = students::create(&mut c, &hlc(), actor, new_student("Ali Khan"))
        .await
        .unwrap();
    imports::add_row(
        &mut c,
        &hlc(),
        actor,
        &batch,
        2,
        "created",
        "student",
        Some(created.id),
        None,
    )
    .await
    .unwrap();
    let enrollment = students::enrol(&mut c, &hlc(), actor, created.id, section.id, Some(7))
        .await
        .unwrap();
    imports::add_row(
        &mut c,
        &hlc(),
        actor,
        &batch,
        2,
        "created",
        "enrollment",
        Some(enrollment.id),
        None,
    )
    .await
    .unwrap();
    let guardian = guardians::create(
        &mut c,
        &hlc(),
        actor,
        guardians::NewGuardian {
            school_id: school.id,
            full_name: "Father".into(),
            phone: Some("03001234567".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    imports::add_row(
        &mut c,
        &hlc(),
        actor,
        &batch,
        2,
        "created",
        "guardian",
        Some(guardian.id),
        None,
    )
    .await
    .unwrap();
    let link = guardians::link(
        &mut c,
        &hlc(),
        actor,
        created.id,
        guardian.id,
        "father",
        true,
    )
    .await
    .unwrap();
    imports::add_row(
        &mut c,
        &hlc(),
        actor,
        &batch,
        2,
        "created",
        "student_guardian",
        Some(link.id),
        None,
    )
    .await
    .unwrap();
    students::update(
        &mut c,
        &hlc(),
        actor,
        existing.id,
        students::StudentUpdate {
            full_name: Some("New Name".into()),
            address: Some("Gulshan".into()),
            legacy_id: None,
            father_name: None,
            date_of_birth: None,
            gender: None,
            cnic: None,
            admission_date: None,
            board_roll_no: None,
            board_registration_no: None,
            custom: None,
        },
    )
    .await
    .unwrap();
    imports::add_row(
        &mut c,
        &hlc(),
        actor,
        &batch,
        3,
        "updated",
        "student",
        Some(existing.id),
        Some(json!({ "full_name": "Old Name", "address": "" })),
    )
    .await
    .unwrap();
    let member = staff::create(
        &mut c,
        &hlc(),
        actor,
        staff::NewStaff {
            school_id: school.id,
            user_id: None,
            staff_number: "S-0001".into(),
            full_name: "Sana".into(),
            father_name: "".into(),
            gender: "female".into(),
            phone: None,
            email: None,
            cnic: None,
            designation: "Teacher".into(),
            joined_on: None,
        },
    )
    .await
    .unwrap();
    imports::add_row(
        &mut c,
        &hlc(),
        actor,
        &batch,
        4,
        "created",
        "staff",
        Some(member.id),
        None,
    )
    .await
    .unwrap();
    let voucher = fees::create_opening_balance(
        &mut c,
        &hlc(),
        actor,
        school.id,
        created.id,
        year.id,
        d(2026, 4, 1),
        dec!(4500.50),
        Some("old register"),
    )
    .await
    .unwrap();
    assert_eq!(
        (
            voucher.gross,
            voucher.arrears,
            voucher.total_due,
            voucher.status.as_str()
        ),
        (dec!(4500.50), dec!(0), dec!(4500.50), "unpaid")
    );
    assert!(voucher.voucher_no.starts_with("V202604-"));
    imports::add_row(
        &mut c,
        &hlc(),
        actor,
        &batch,
        2,
        "created",
        "fee_voucher",
        Some(voucher.id),
        None,
    )
    .await
    .unwrap();
    imports::add_row(
        &mut c,
        &hlc(),
        actor,
        &batch,
        5,
        "skipped",
        "student",
        None,
        None,
    )
    .await
    .unwrap();
    assert!(matches!(
        fees::create_opening_balance(
            &mut c,
            &hlc(),
            actor,
            school.id,
            created.id,
            year.id,
            d(2026, 4, 1),
            dec!(-1),
            None
        )
        .await,
        Err(DbError::Conflict(_))
    ));
    let batch = imports::finish_batch(&mut c, &hlc(), actor, batch.id, 3, 1, 1)
        .await
        .unwrap();
    assert_eq!(
        (
            batch.status.as_str(),
            batch.created_rows,
            batch.updated_rows,
            batch.skipped_rows
        ),
        ("applied", 3, 1, 1)
    );
    assert_eq!(
        imports::list_batches(&mut c, school.id, 10)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        imports::batch_rows(&mut c, batch.id).await.unwrap().len(),
        8
    );
    assert_eq!(
        students::get(&mut c, existing.id).await.unwrap().full_name,
        "New Name"
    );
    assert_eq!(
        fees::student_balance(&mut c, created.id).await.unwrap(),
        dec!(4500.50)
    );

    // Roll back: created rows disappear, the updated student gets the old values back.
    let reverted = imports::rollback(&mut c, &hlc(), actor, batch.id)
        .await
        .unwrap();
    assert_eq!(reverted, 7);
    assert!(
        students::get(&mut c, created.id).await.is_err(),
        "soft-deleted"
    );
    assert!(guardians::get(&mut c, guardian.id).await.is_err());
    assert!(staff::get(&mut c, member.id).await.is_err());
    assert_eq!(
        students::section_in_year(&mut c, created.id, year.id)
            .await
            .unwrap(),
        None
    );
    let back = students::get(&mut c, existing.id).await.unwrap();
    assert_eq!(
        (back.full_name.as_str(), back.address.as_str()),
        ("Old Name", "")
    );
    assert_eq!(
        fees::get_voucher(&mut c, voucher.id).await.unwrap().status,
        "cancelled"
    );
    let batch = imports::get_batch(&mut c, batch.id).await.unwrap();
    assert_eq!(batch.status, "rolled_back");
    assert!(batch.rolled_back_at.is_some());
    assert!(matches!(
        imports::rollback(&mut c, &hlc(), actor, batch.id).await,
        Err(DbError::Conflict(_))
    ));
    let trail = audit::for_entity(&mut c, "import_batch", batch.id)
        .await
        .unwrap();
    assert_eq!(
        trail.iter().map(|e| e.action.as_str()).collect::<Vec<_>>(),
        ["create", "update"]
    );
    let _ = Uuid::nil();
}
