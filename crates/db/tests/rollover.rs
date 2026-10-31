//! M1 stage 7: academic year rollover. Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

use chrono::NaiveDate;
use madrasa_core::hlc::Hlc;
use madrasa_core::password::hash_password;
use madrasa_db::testing::TestDb;
use madrasa_db::{academics, exams, fees, rollover, schools, students, test_db, users, Actor};
use rust_decimal_macros::dec;
use serde_json::json;
use uuid::Uuid;

fn hlc() -> Hlc {
    Hlc::new(23)
}
fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

#[tokio::test]
async fn rollover_promotes_retains_graduates_and_copies_setup() {
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
    let c1 = academics::create_class(&mut c, &hlc(), actor, school.id, "1", 1)
        .await
        .unwrap();
    let c2 = academics::create_class(&mut c, &hlc(), actor, school.id, "2", 2)
        .await
        .unwrap();
    let mk = |class_id: Uuid, name: &str| academics::NewSection {
        school_id: school.id,
        class_id,
        academic_year_id: year.id,
        name: name.into(),
        class_teacher_id: None,
        capacity: Some(30),
    };
    let s1a = academics::create_section(&mut c, &hlc(), actor, mk(c1.id, "A"))
        .await
        .unwrap();
    let s1b = academics::create_section(&mut c, &hlc(), actor, mk(c1.id, "B"))
        .await
        .unwrap();
    let s2a = academics::create_section(&mut c, &hlc(), actor, mk(c2.id, "A"))
        .await
        .unwrap();
    let new_student = |name: &str| students::NewStudent {
        school_id: school.id,
        student_number: None,
        legacy_id: None,
        full_name: name.into(),
        father_name: "F".into(),
        date_of_birth: d(2015, 1, 1),
        gender: "male".into(),
        cnic: None,
        address: "".into(),
        admission_date: d(2026, 4, 1),
        board_roll_no: None,
        board_registration_no: None,
        custom: json!({}),
    };
    let mut ids = Vec::new();
    for (name, sec) in [
        ("Ali", &s1a),
        ("Sara", &s1a),
        ("Omar", &s1b),
        ("Zain", &s2a),
        ("Hira", &s2a),
    ] {
        let s = students::create(&mut c, &hlc(), actor, new_student(name))
            .await
            .unwrap();
        students::enrol(&mut c, &hlc(), actor, s.id, sec.id, None)
            .await
            .unwrap();
        ids.push(s.id);
    }
    let (ali, sara, omar, zain, hira) = (ids[0], ids[1], ids[2], ids[3], ids[4]);
    // A withdrawn student is not carried over.
    let gone = students::create(&mut c, &hlc(), actor, new_student("Gone"))
        .await
        .unwrap();
    students::enrol(&mut c, &hlc(), actor, gone.id, s1a.id, None)
        .await
        .unwrap();
    students::set_status(
        &mut c,
        &hlc(),
        actor,
        gone.id,
        "left",
        Some(d(2026, 9, 1)),
        Some("moved"),
    )
    .await
    .unwrap();
    // Fee structure and assessments to copy; an unpaid voucher to carry.
    let tuition = fees::create_head(&mut c, &hlc(), actor, school.id, "Tuition", "TUI", true, 0)
        .await
        .unwrap();
    fees::set_structure(
        &mut c,
        &hlc(),
        actor,
        school.id,
        year.id,
        c1.id,
        tuition.id,
        dec!(2500),
    )
    .await
    .unwrap();
    fees::set_structure(
        &mut c,
        &hlc(),
        actor,
        school.id,
        year.id,
        c2.id,
        tuition.id,
        dec!(3000),
    )
    .await
    .unwrap();
    exams::create_default_assessments(&mut c, &hlc(), actor, school.id, year.id)
        .await
        .unwrap();
    fees::generate_vouchers(
        &mut c,
        &hlc(),
        actor,
        school.id,
        year.id,
        d(2026, 4, 1),
        None,
        d(2026, 4, 1),
    )
    .await
    .unwrap();

    let p = rollover::preview(&mut c, school.id).await.unwrap();
    assert_eq!(
        (
            p.new_year_name.as_str(),
            p.new_start_year,
            p.new_year_exists
        ),
        ("2027-28", 2027, false)
    );
    assert_eq!(p.moves.len(), 3);
    let m1a = p.moves.iter().find(|m| m.section_id == s1a.id).unwrap();
    assert_eq!(
        (m1a.students, m1a.target_class_name.as_deref()),
        (2, Some("2")),
        "withdrawn student excluded"
    );
    let m2a = p.moves.iter().find(|m| m.section_id == s2a.id).unwrap();
    assert_eq!(
        (m2a.students, m2a.target_class_name.clone()),
        (2, None),
        "top class graduates"
    );
    assert_eq!((p.promoted, p.graduating), (3, 2));
    assert_eq!(p.students_with_balance, 5);
    assert_eq!(
        p.balance_carried,
        dec!(2500) * dec!(3) + dec!(3000) * dec!(2)
    );

    let r = rollover::apply(
        &mut c,
        &hlc(),
        actor,
        school.id,
        &rollover::RolloverOptions {
            retain: vec![sara],
            graduate: vec![omar],
        },
    )
    .await
    .unwrap();
    assert_eq!(r.new_year.name, "2027-28");
    assert!(r.new_year.is_active);
    assert_eq!(
        (r.promoted, r.retained, r.graduated),
        (1, 1, 3),
        "Ali up, Sara repeats, Omar leaves, Zain and Hira graduate"
    );
    assert_eq!(
        r.sections_created, 2,
        "2-A for Ali and 1-A for Sara; 1-B and 2 (none) not needed"
    );
    assert_eq!(r.structure_lines_copied, 2);
    assert_eq!(r.assessments_created, 4);
    assert_eq!(r.balance_carried, dec!(13500));
    assert!(
        !academics::get_year(&mut c, year.id)
            .await
            .unwrap()
            .is_active,
        "old year archived"
    );
    let new_sections = academics::list_sections(&mut c, school.id, Some(r.new_year.id), None)
        .await
        .unwrap();
    let sec_of = |sid: Uuid| new_sections.iter().find(|s| s.id == sid).unwrap();
    let ali_sec = students::section_in_year(&mut c, ali, r.new_year.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (sec_of(ali_sec).class_id, sec_of(ali_sec).name.as_str()),
        (c2.id, "A")
    );
    let sara_sec = students::section_in_year(&mut c, sara, r.new_year.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (sec_of(sara_sec).class_id, sec_of(sara_sec).name.as_str()),
        (c1.id, "A")
    );
    assert_eq!(
        students::section_in_year(&mut c, omar, r.new_year.id)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        students::get(&mut c, zain).await.unwrap().status,
        "graduated"
    );
    assert_eq!(
        students::get(&mut c, hira).await.unwrap().status,
        "graduated"
    );
    assert_eq!(
        students::get(&mut c, omar).await.unwrap().status,
        "graduated"
    );
    assert_eq!(students::get(&mut c, gone.id).await.unwrap().status, "left");
    assert_eq!(
        fees::list_structure(&mut c, r.new_year.id, Some(c2.id))
            .await
            .unwrap()[0]
            .amount,
        dec!(3000)
    );
    assert_eq!(
        exams::list_assessments(&mut c, r.new_year.id)
            .await
            .unwrap()
            .len(),
        4
    );
    // The first voucher of the new year carries Ali's April balance (class 2 now: 3,000 + 2,500 arrears).
    let rep = fees::generate_vouchers(
        &mut c,
        &hlc(),
        actor,
        school.id,
        r.new_year.id,
        d(2027, 4, 1),
        None,
        d(2027, 4, 1),
    )
    .await
    .unwrap();
    assert_eq!(rep.created, 2, "Ali and Sara");
    let ali_voucher = fees::vouchers_for_student(&mut c, ali)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(
        (ali_voucher.gross, ali_voucher.arrears),
        (dec!(3000), dec!(2500) + dec!(250)),
        "April 2026 balance plus its late fee carried into 2027-28"
    );
    // Rolling over again would open 2028-29; the preview shows how young the current year is.
    let again = rollover::preview(&mut c, school.id).await.unwrap();
    assert_eq!(
        (again.new_year_name.as_str(), again.new_year_exists),
        ("2028-29", false)
    );
    assert!(again.months_into_year < 12, "{}", again.months_into_year);
    assert_eq!(
        fees::collected_between(&mut c, school.id, d(2026, 1, 1), d(2027, 12, 31))
            .await
            .unwrap(),
        dec!(0)
    );
}
