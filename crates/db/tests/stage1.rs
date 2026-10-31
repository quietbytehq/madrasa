//! M1 stage 1: academic structure, staff, students, guardians.

use chrono::NaiveDate;
use madrasa_core::hlc::Hlc;
use madrasa_core::password::hash_password;
use madrasa_core::students::DuplicateProbe;
use madrasa_db::models::{AcademicYear, School};
use madrasa_db::testing::TestDb;
use madrasa_db::{
    academics, audit, guardians, schools, staff, students, test_db, users, Actor, DbError, PgPool,
};
use serde_json::json;

fn hlc() -> Hlc {
    Hlc::new(5)
}

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

async fn school(pool: &PgPool) -> School {
    let mut c = pool.acquire().await.unwrap();
    schools::create(
        &mut c,
        &hlc(),
        Actor::SYSTEM,
        schools::NewSchool {
            name: "Test".into(),
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
    .unwrap()
}

async fn year(pool: &PgPool, school: &School, y: i32, active: bool) -> AcademicYear {
    let mut c = pool.acquire().await.unwrap();
    let (name, start, end) = academics::default_year_bounds(y, 4).unwrap();
    let year = academics::create_year(
        &mut c,
        &hlc(),
        Actor::system_for(school.id),
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
    if active {
        academics::activate_year(&mut c, &hlc(), Actor::system_for(school.id), year.id)
            .await
            .unwrap()
    } else {
        year
    }
}

fn new_student(
    school_id: uuid::Uuid,
    name: &str,
    father: &str,
    dob: NaiveDate,
    cnic: Option<&str>,
) -> students::NewStudent {
    students::NewStudent {
        school_id,
        student_number: None,
        legacy_id: None,
        full_name: name.into(),
        father_name: father.into(),
        date_of_birth: dob,
        gender: "male".into(),
        cnic: cnic.map(String::from),
        address: "".into(),
        admission_date: d(2026, 4, 10),
        board_roll_no: None,
        board_registration_no: None,
        custom: json!({}),
    }
}

#[tokio::test]
async fn academic_years_single_active_and_default_bounds() {
    let db: TestDb = test_db!();
    let sch = school(&db.pool).await;
    let (name, start, end) = academics::default_year_bounds(2026, 4).unwrap();
    assert_eq!(
        (name.as_str(), start, end),
        ("2026-27", d(2026, 4, 1), d(2027, 3, 31))
    );
    let (name, start, end) = academics::default_year_bounds(2026, 8).unwrap();
    assert_eq!(
        (name.as_str(), start, end),
        ("2026-27", d(2026, 8, 1), d(2027, 7, 31))
    );

    let y1 = year(&db.pool, &sch, 2025, true).await;
    let y2 = year(&db.pool, &sch, 2026, false).await;
    let mut c = db.pool.acquire().await.unwrap();
    assert!(y1.is_active && !y2.is_active);
    let active = academics::activate_year(&mut c, &hlc(), Actor::system_for(sch.id), y2.id)
        .await
        .unwrap();
    assert!(active.is_active);
    assert_eq!(
        academics::active_year(&mut c, sch.id)
            .await
            .unwrap()
            .unwrap()
            .id,
        y2.id
    );
    assert!(!academics::get_year(&mut c, y1.id).await.unwrap().is_active);
    // Duplicate names refused, listing newest first.
    let dup = academics::create_year(
        &mut c,
        &hlc(),
        Actor::SYSTEM,
        academics::NewAcademicYear {
            school_id: sch.id,
            name: "2026-27".into(),
            start_date: d(2026, 4, 1),
            end_date: d(2027, 3, 31),
            start_month: 4,
        },
    )
    .await;
    assert!(matches!(dup, Err(DbError::Conflict(_))));
    let names: Vec<_> = academics::list_years(&mut c, sch.id)
        .await
        .unwrap()
        .into_iter()
        .map(|y| y.name)
        .collect();
    assert_eq!(names, ["2026-27", "2025-26"]);
    drop(c);
    db.cleanup().await;
}

#[tokio::test]
async fn classes_sections_subjects_and_class_teacher() {
    let db: TestDb = test_db!();
    let sch = school(&db.pool).await;
    let y = year(&db.pool, &sch, 2026, true).await;
    let actor = Actor::system_for(sch.id);
    let mut c = db.pool.acquire().await.unwrap();
    let kg = academics::create_class(&mut c, &hlc(), actor, sch.id, "KG", 0)
        .await
        .unwrap();
    let one = academics::create_class(&mut c, &hlc(), actor, sch.id, "1", 1)
        .await
        .unwrap();
    assert!(matches!(
        academics::create_class(&mut c, &hlc(), actor, sch.id, "kg", 9).await,
        Err(DbError::Conflict(_))
    ));
    let renamed = academics::update_class(&mut c, &hlc(), actor, kg.id, Some("Nursery"), None)
        .await
        .unwrap();
    assert_eq!(renamed.name, "Nursery");
    assert_eq!(
        academics::list_classes(&mut c, sch.id)
            .await
            .unwrap()
            .iter()
            .map(|x| x.name.as_str())
            .collect::<Vec<_>>(),
        ["Nursery", "1"]
    );

    let eng = academics::create_subject(&mut c, &hlc(), actor, sch.id, "English", "eng")
        .await
        .unwrap();
    let math = academics::create_subject(&mut c, &hlc(), actor, sch.id, "Mathematics", "MATH")
        .await
        .unwrap();
    assert_eq!(eng.code, "ENG");
    assert!(matches!(
        academics::create_subject(&mut c, &hlc(), actor, sch.id, "Eng 2", "ENG").await,
        Err(DbError::Conflict(_))
    ));
    let set = academics::set_class_subjects(&mut c, &hlc(), actor, one.id, &[eng.id, math.id])
        .await
        .unwrap();
    assert_eq!(set.len(), 2);
    let set = academics::set_class_subjects(&mut c, &hlc(), actor, one.id, &[math.id])
        .await
        .unwrap();
    assert_eq!(set, vec![math.id]);
    let set = academics::set_class_subjects(&mut c, &hlc(), actor, one.id, &[math.id, eng.id])
        .await
        .unwrap();
    assert_eq!(set.len(), 2, "re-adding a removed subject revives the link");

    let teacher = staff::create(
        &mut c,
        &hlc(),
        actor,
        staff::NewStaff {
            school_id: sch.id,
            user_id: None,
            staff_number: "t-001".into(),
            full_name: "Sana Malik".into(),
            father_name: "".into(),
            gender: "female".into(),
            phone: None,
            email: Some("Sana@Example.com".into()),
            cnic: None,
            designation: "Teacher".into(),
            joined_on: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(teacher.staff_number, "T-001");
    assert_eq!(teacher.email.as_deref(), Some("sana@example.com"));
    let a = academics::create_section(
        &mut c,
        &hlc(),
        actor,
        academics::NewSection {
            school_id: sch.id,
            class_id: one.id,
            academic_year_id: y.id,
            name: "a".into(),
            class_teacher_id: Some(teacher.id),
            capacity: Some(35),
        },
    )
    .await
    .unwrap();
    assert_eq!(a.name, "A");
    assert!(matches!(
        academics::create_section(
            &mut c,
            &hlc(),
            actor,
            academics::NewSection {
                school_id: sch.id,
                class_id: one.id,
                academic_year_id: y.id,
                name: "A".into(),
                class_teacher_id: None,
                capacity: None
            }
        )
        .await,
        Err(DbError::Conflict(_))
    ));
    let a = academics::update_section(
        &mut c,
        &hlc(),
        actor,
        a.id,
        academics::SectionUpdate {
            class_teacher_id: Some(None),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(a.class_teacher_id.is_none());
    assert_eq!(
        academics::list_sections(&mut c, sch.id, Some(y.id), Some(one.id))
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        academics::list_sections(&mut c, sch.id, None, Some(kg.id))
            .await
            .unwrap()
            .len(),
        0
    );

    // Teaching assignments and the resulting section scope.
    staff::set_assignments(
        &mut c,
        &hlc(),
        actor,
        teacher.id,
        &[(a.id, Some(math.id)), (a.id, None)],
    )
    .await
    .unwrap();
    assert_eq!(
        staff::assignments(&mut c, teacher.id).await.unwrap().len(),
        2
    );
    assert_eq!(
        staff::section_scope(&mut c, teacher.id).await.unwrap(),
        vec![a.id]
    );
    staff::set_assignments(&mut c, &hlc(), actor, teacher.id, &[])
        .await
        .unwrap();
    assert!(staff::section_scope(&mut c, teacher.id)
        .await
        .unwrap()
        .is_empty());
    academics::update_section(
        &mut c,
        &hlc(),
        actor,
        a.id,
        academics::SectionUpdate {
            class_teacher_id: Some(Some(teacher.id)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        staff::section_scope(&mut c, teacher.id).await.unwrap(),
        vec![a.id]
    );
    drop(c);
    db.cleanup().await;
}

#[tokio::test]
async fn student_numbers_follow_prefix_year_sequence() {
    let db: TestDb = test_db!();
    let sch = school(&db.pool).await;
    let actor = Actor::system_for(sch.id);
    let mut c = db.pool.acquire().await.unwrap();
    let s1 = students::create(
        &mut c,
        &hlc(),
        actor,
        new_student(sch.id, "Ali Khan", "Ahmed Khan", d(2015, 3, 4), None),
    )
    .await
    .unwrap();
    let s2 = students::create(
        &mut c,
        &hlc(),
        actor,
        new_student(sch.id, "Sara Khan", "Ahmed Khan", d(2017, 5, 6), None),
    )
    .await
    .unwrap();
    assert_eq!(s1.student_number, "KHI-MC-2026-00001");
    assert_eq!(s2.student_number, "KHI-MC-2026-00002");
    // Different admission year gets its own sequence; configured prefix wins.
    madrasa_db::settings::set(
        &mut c,
        &hlc(),
        actor,
        Some(sch.id),
        students::KEY_STUDENT_NUMBER_PREFIX,
        json!("khi mc 2"),
    )
    .await
    .unwrap();
    let mut n = new_student(sch.id, "Bilal", "X", d(2016, 1, 1), None);
    n.admission_date = d(2027, 8, 1);
    let s3 = students::create(&mut c, &hlc(), actor, n).await.unwrap();
    assert_eq!(s3.student_number, "KHI-MC-2-2027-00001");
    // Explicit numbers are kept (uppercased) and must be unique.
    let mut n = new_student(sch.id, "Legacy", "X", d(2016, 1, 1), None);
    n.student_number = Some("old-77".into());
    n.legacy_id = Some(" L77 ".into());
    let s4 = students::create(&mut c, &hlc(), actor, n).await.unwrap();
    assert_eq!(
        (s4.student_number.as_str(), s4.legacy_id.as_deref()),
        ("OLD-77", Some("L77"))
    );
    let mut n = new_student(sch.id, "Legacy 2", "X", d(2016, 1, 1), None);
    n.student_number = Some("OLD-77".into());
    assert!(matches!(
        students::create(&mut c, &hlc(), actor, n).await,
        Err(DbError::Conflict(_))
    ));
    assert_eq!(students::count(&mut c, sch.id, None).await.unwrap(), 4);
    drop(c);
    db.cleanup().await;
}

#[tokio::test]
async fn duplicates_withdrawal_photo_and_audit() {
    let db: TestDb = test_db!();
    let sch = school(&db.pool).await;
    let actor = Actor::system_for(sch.id);
    let mut c = db.pool.acquire().await.unwrap();
    let ali = students::create(
        &mut c,
        &hlc(),
        actor,
        new_student(
            sch.id,
            "Ali Khan",
            "Ahmed Khan",
            d(2015, 3, 4),
            Some("42101-1234567-1"),
        ),
    )
    .await
    .unwrap();

    let by_cnic = DuplicateProbe {
        full_name: "Other".into(),
        father_name: "Other".into(),
        date_of_birth: d(2010, 1, 1),
        cnic: Some("42101-1234567-1".into()),
    };
    let m = students::find_duplicates(&mut c, sch.id, &by_cnic, None)
        .await
        .unwrap();
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].student.id, ali.id);
    let by_identity = DuplicateProbe {
        full_name: " ALI khan".into(),
        father_name: "ahmed KHAN".into(),
        date_of_birth: d(2015, 3, 4),
        cnic: None,
    };
    assert_eq!(
        students::find_duplicates(&mut c, sch.id, &by_identity, None)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        students::find_duplicates(&mut c, sch.id, &by_identity, Some(ali.id))
            .await
            .unwrap()
            .is_empty(),
        "exclude self when editing"
    );
    let none = DuplicateProbe {
        full_name: "Ali Khan".into(),
        father_name: "Ahmed Khan".into(),
        date_of_birth: d(2015, 3, 5),
        cnic: None,
    };
    assert!(students::find_duplicates(&mut c, sch.id, &none, None)
        .await
        .unwrap()
        .is_empty());

    // Update, withdraw (no hard delete), reactivate.
    let upd = students::update(
        &mut c,
        &hlc(),
        actor,
        ali.id,
        students::StudentUpdate {
            address: Some(" Gulshan ".into()),
            custom: Some(json!({"house": "Red"})),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(upd.address, "Gulshan");
    assert_eq!(upd.custom["house"], "Red");
    let left = students::set_status(
        &mut c,
        &hlc(),
        actor,
        ali.id,
        "left",
        Some(d(2026, 6, 30)),
        Some("Moved city"),
    )
    .await
    .unwrap();
    assert_eq!(
        (
            left.status.as_str(),
            left.left_on,
            left.left_reason.as_deref()
        ),
        ("left", Some(d(2026, 6, 30)), Some("Moved city"))
    );
    assert_eq!(
        students::count(&mut c, sch.id, Some("active"))
            .await
            .unwrap(),
        0
    );
    let back = students::set_status(&mut c, &hlc(), actor, ali.id, "active", None, None)
        .await
        .unwrap();
    assert!(back.left_on.is_none() && back.left_reason.is_none());

    // Photo round trip; the row never carries the bytes.
    assert!(students::photo(&mut c, ali.id).await.unwrap().is_none());
    students::set_photo(
        &mut c,
        &hlc(),
        actor,
        ali.id,
        Some((vec![1, 2, 3], "image/png".into())),
    )
    .await
    .unwrap();
    assert!(students::get(&mut c, ali.id).await.unwrap().has_photo);
    assert_eq!(
        students::photo(&mut c, ali.id).await.unwrap().unwrap(),
        (vec![1, 2, 3], "image/png".to_string())
    );
    let snapshot = serde_json::to_value(students::get(&mut c, ali.id).await.unwrap()).unwrap();
    assert!(snapshot.get("photo").is_none());
    students::set_photo(&mut c, &hlc(), actor, ali.id, None)
        .await
        .unwrap();
    assert!(!students::get(&mut c, ali.id).await.unwrap().has_photo);

    let actions: Vec<_> = audit::for_entity(&mut c, "student", ali.id)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.action)
        .collect();
    assert_eq!(
        actions,
        ["create", "update", "update", "update", "update", "update"]
    );
    drop(c);
    db.cleanup().await;
}

#[tokio::test]
async fn guardians_link_siblings_and_directory_listing() {
    let db: TestDb = test_db!();
    let sch = school(&db.pool).await;
    let y = year(&db.pool, &sch, 2026, true).await;
    let actor = Actor::system_for(sch.id);
    let mut c = db.pool.acquire().await.unwrap();
    let one = academics::create_class(&mut c, &hlc(), actor, sch.id, "1", 1)
        .await
        .unwrap();
    let two = academics::create_class(&mut c, &hlc(), actor, sch.id, "2", 2)
        .await
        .unwrap();
    let sec1a = academics::create_section(
        &mut c,
        &hlc(),
        actor,
        academics::NewSection {
            school_id: sch.id,
            class_id: one.id,
            academic_year_id: y.id,
            name: "A".into(),
            class_teacher_id: None,
            capacity: None,
        },
    )
    .await
    .unwrap();
    let sec2a = academics::create_section(
        &mut c,
        &hlc(),
        actor,
        academics::NewSection {
            school_id: sch.id,
            class_id: two.id,
            academic_year_id: y.id,
            name: "A".into(),
            class_teacher_id: None,
            capacity: None,
        },
    )
    .await
    .unwrap();

    let ali = students::create(
        &mut c,
        &hlc(),
        actor,
        new_student(sch.id, "Ali Khan", "Ahmed Khan", d(2015, 3, 4), None),
    )
    .await
    .unwrap();
    let sara = students::create(
        &mut c,
        &hlc(),
        actor,
        new_student(sch.id, "Sara Khan", "Ahmed Khan", d(2017, 5, 6), None),
    )
    .await
    .unwrap();
    let omar = students::create(
        &mut c,
        &hlc(),
        actor,
        new_student(sch.id, "Omar Raza", "Tariq Raza", d(2016, 1, 1), None),
    )
    .await
    .unwrap();
    students::enrol(&mut c, &hlc(), actor, ali.id, sec2a.id, Some(7))
        .await
        .unwrap();
    students::enrol(&mut c, &hlc(), actor, sara.id, sec1a.id, Some(3))
        .await
        .unwrap();
    students::enrol(&mut c, &hlc(), actor, omar.id, sec1a.id, Some(4))
        .await
        .unwrap();
    // Moving within the same year updates the one enrollment.
    students::enrol(&mut c, &hlc(), actor, omar.id, sec2a.id, Some(9))
        .await
        .unwrap();
    assert_eq!(
        students::enrollments(&mut c, omar.id).await.unwrap().len(),
        1
    );
    assert_eq!(
        students::section_in_year(&mut c, omar.id, y.id)
            .await
            .unwrap(),
        Some(sec2a.id)
    );

    // One guardian shared by two siblings; matching by CNIC/phone finds it.
    let father = guardians::create(
        &mut c,
        &hlc(),
        actor,
        guardians::NewGuardian {
            school_id: sch.id,
            full_name: "Ahmed Khan".into(),
            phone: Some("03001234567".into()),
            cnic: Some("42101-7654321-1".into()),
            occupation: "Trader".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    guardians::link(&mut c, &hlc(), actor, ali.id, father.id, "father", true)
        .await
        .unwrap();
    guardians::link(&mut c, &hlc(), actor, sara.id, father.id, "father", true)
        .await
        .unwrap();
    let mother = guardians::create(
        &mut c,
        &hlc(),
        actor,
        guardians::NewGuardian {
            school_id: sch.id,
            full_name: "Nadia Khan".into(),
            phone: Some("03007654321".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    guardians::link(&mut c, &hlc(), actor, ali.id, mother.id, "mother", true)
        .await
        .unwrap();
    let links = guardians::for_student(&mut c, ali.id).await.unwrap();
    assert_eq!(
        links
            .iter()
            .map(|l| (l.full_name.as_str(), l.is_primary))
            .collect::<Vec<_>>(),
        [("Nadia Khan", true), ("Ahmed Khan", false)],
        "newest primary wins"
    );
    assert_eq!(
        guardians::find_matching(&mut c, sch.id, Some("42101-7654321-1"), None)
            .await
            .unwrap()
            .unwrap()
            .id,
        father.id
    );
    assert_eq!(
        guardians::find_matching(&mut c, sch.id, None, Some("03007654321"))
            .await
            .unwrap()
            .unwrap()
            .id,
        mother.id
    );
    assert!(guardians::find_matching(&mut c, sch.id, None, Some("0399"))
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        guardians::search(&mut c, sch.id, "0300", 10)
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        guardians::search(&mut c, sch.id, "nadia", 10)
            .await
            .unwrap()
            .len(),
        1
    );

    let sibs = guardians::siblings(&mut c, ali.id).await.unwrap();
    assert_eq!(
        sibs.iter()
            .map(|s| s.full_name.as_str())
            .collect::<Vec<_>>(),
        ["Sara Khan"]
    );
    assert_eq!(sibs[0].class_name.as_deref(), Some("1"));
    assert!(guardians::siblings(&mut c, omar.id)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        guardians::students_of(&mut c, father.id)
            .await
            .unwrap()
            .len(),
        2
    );
    guardians::unlink(&mut c, &hlc(), actor, sara.id, father.id)
        .await
        .unwrap();
    assert!(guardians::siblings(&mut c, ali.id)
        .await
        .unwrap()
        .is_empty());
    assert!(matches!(
        guardians::unlink(&mut c, &hlc(), actor, sara.id, father.id).await,
        Err(DbError::NotFound(_))
    ));

    // Directory: filters, scope, sorting, paging.
    let all = students::list(
        &mut c,
        sch.id,
        &students::StudentFilter::default(),
        students::StudentSort::Name,
        false,
        1,
        50,
    )
    .await
    .unwrap();
    assert_eq!(all.total, 3);
    assert_eq!(
        all.items
            .iter()
            .map(|r| r.full_name.as_str())
            .collect::<Vec<_>>(),
        ["Ali Khan", "Omar Raza", "Sara Khan"]
    );
    assert_eq!(all.items[0].class_name.as_deref(), Some("2"));
    assert_eq!(all.items[0].roll_no, Some(7));
    let by_class = students::list(
        &mut c,
        sch.id,
        &students::StudentFilter {
            class_id: Some(one.id),
            ..Default::default()
        },
        students::StudentSort::Class,
        false,
        1,
        50,
    )
    .await
    .unwrap();
    assert_eq!(
        by_class
            .items
            .iter()
            .map(|r| r.full_name.as_str())
            .collect::<Vec<_>>(),
        ["Sara Khan"]
    );
    let q = students::list(
        &mut c,
        sch.id,
        &students::StudentFilter {
            q: Some("raza".into()),
            ..Default::default()
        },
        students::StudentSort::Name,
        false,
        1,
        50,
    )
    .await
    .unwrap();
    assert_eq!(q.total, 1);
    let by_number = students::list(
        &mut c,
        sch.id,
        &students::StudentFilter {
            q: Some("00002".into()),
            ..Default::default()
        },
        students::StudentSort::Name,
        false,
        1,
        50,
    )
    .await
    .unwrap();
    assert_eq!(by_number.items[0].full_name, "Sara Khan");
    let scoped = students::list(
        &mut c,
        sch.id,
        &students::StudentFilter {
            section_scope: Some(vec![sec2a.id]),
            ..Default::default()
        },
        students::StudentSort::Name,
        true,
        1,
        50,
    )
    .await
    .unwrap();
    assert_eq!(
        scoped
            .items
            .iter()
            .map(|r| r.full_name.as_str())
            .collect::<Vec<_>>(),
        ["Omar Raza", "Ali Khan"]
    );
    let empty_scope = students::list(
        &mut c,
        sch.id,
        &students::StudentFilter {
            section_scope: Some(vec![]),
            ..Default::default()
        },
        students::StudentSort::Name,
        false,
        1,
        50,
    )
    .await
    .unwrap();
    assert_eq!(empty_scope.total, 0);
    let page2 = students::list(
        &mut c,
        sch.id,
        &students::StudentFilter::default(),
        students::StudentSort::Name,
        false,
        2,
        2,
    )
    .await
    .unwrap();
    assert_eq!((page2.total, page2.items.len(), page2.page), (3, 1, 2));
    students::set_status(
        &mut c,
        &hlc(),
        actor,
        omar.id,
        "graduated",
        Some(d(2026, 6, 1)),
        None,
    )
    .await
    .unwrap();
    let active = students::list(
        &mut c,
        sch.id,
        &students::StudentFilter {
            status: Some("active".into()),
            ..Default::default()
        },
        students::StudentSort::AdmissionDate,
        false,
        1,
        50,
    )
    .await
    .unwrap();
    assert_eq!(active.total, 2);
    drop(c);
    db.cleanup().await;
}

#[tokio::test]
async fn staff_linked_to_logins() {
    let db: TestDb = test_db!();
    let sch = school(&db.pool).await;
    let actor = Actor::system_for(sch.id);
    let mut c = db.pool.acquire().await.unwrap();
    let user = users::create(
        &mut c,
        &hlc(),
        actor,
        users::NewUser {
            school_id: sch.id,
            username: "t1".into(),
            full_name: "T One".into(),
            email: None,
            password_hash: hash_password("a-long-password").unwrap(),
        },
    )
    .await
    .unwrap();
    let st = staff::create(
        &mut c,
        &hlc(),
        actor,
        staff::NewStaff {
            school_id: sch.id,
            user_id: Some(user.id),
            staff_number: "S1".into(),
            full_name: "T One".into(),
            father_name: "".into(),
            gender: "male".into(),
            phone: None,
            email: None,
            cnic: None,
            designation: "Teacher".into(),
            joined_on: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        staff::find_by_user(&mut c, user.id)
            .await
            .unwrap()
            .unwrap()
            .id,
        st.id
    );
    // One profile per login.
    let dup = staff::create(
        &mut c,
        &hlc(),
        actor,
        staff::NewStaff {
            school_id: sch.id,
            user_id: Some(user.id),
            staff_number: "S2".into(),
            full_name: "Other".into(),
            father_name: "".into(),
            gender: "male".into(),
            phone: None,
            email: None,
            cnic: None,
            designation: "".into(),
            joined_on: None,
        },
    )
    .await;
    assert!(matches!(dup, Err(DbError::Conflict(_))));
    let upd = staff::update(
        &mut c,
        &hlc(),
        actor,
        st.id,
        staff::StaffUpdate {
            designation: Some("Senior Teacher".into()),
            status: Some("left".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        (upd.designation.as_str(), upd.status.as_str()),
        ("Senior Teacher", "left")
    );
    assert_eq!(
        staff::list(&mut c, sch.id, Some("senior"))
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        staff::list(&mut c, sch.id, Some("zzz"))
            .await
            .unwrap()
            .len(),
        0
    );
    assert_eq!(staff::count(&mut c, sch.id).await.unwrap(), 1);
    drop(c);
    db.cleanup().await;
}

#[tokio::test]
async fn migration_grants_academics_permissions_to_existing_roles() {
    let db: TestDb = test_db!();
    // The 0002 migration ran on an empty database; roles created afterwards get grants from core.
    let sch = school(&db.pool).await;
    let mut c = db.pool.acquire().await.unwrap();
    let roles = madrasa_db::roles::create_defaults(
        &mut c,
        &hlc(),
        Actor::system_for(sch.id),
        sch.id,
        madrasa_core::edition::Edition::Lite,
    )
    .await
    .unwrap();
    let teacher = roles.iter().find(|r| r.code == "teacher").unwrap();
    let codes: Vec<String> = sqlx::query_scalar(
        "SELECT code FROM permissions WHERE role_id = $1 AND deleted_at IS NULL ORDER BY code",
    )
    .bind(teacher.id)
    .fetch_all(&mut *c)
    .await
    .unwrap();
    assert!(codes.contains(&"academics.read".to_string()));
    assert!(!codes.contains(&"academics.manage".to_string()));
    drop(c);
    db.cleanup().await;
}
