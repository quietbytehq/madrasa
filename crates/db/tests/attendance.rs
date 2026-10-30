//! M1 stage 3: attendance. Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

use chrono::NaiveDate;
use madrasa_core::attendance::{AttendanceStatus, DayKind, StaffStatus};
use madrasa_core::hlc::Hlc;
use madrasa_core::password::hash_password;
use madrasa_db::models::{AcademicYear, School, Section};
use madrasa_db::testing::TestDb;
use madrasa_db::{
    academics, attendance, audit, guardians, schools, staff, students, test_db, users, Actor,
    DbError, PgPool,
};
use serde_json::json;
use uuid::Uuid;

fn hlc() -> Hlc {
    Hlc::new(11)
}
fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

struct Fixture {
    school: School,
    year: AcademicYear,
    sec1: Section,
    sec2: Section,
    admin: Uuid,
}

async fn fixture(pool: &PgPool) -> Fixture {
    let mut c = pool.acquire().await.unwrap();
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
    let actor = Actor::system_for(school.id);
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
    let mk = |class_id: Uuid| academics::NewSection {
        school_id: school.id,
        class_id,
        academic_year_id: year.id,
        name: "A".into(),
        class_teacher_id: None,
        capacity: None,
    };
    let sec1 = academics::create_section(&mut c, &hlc(), actor, mk(c1.id))
        .await
        .unwrap();
    let sec2 = academics::create_section(&mut c, &hlc(), actor, mk(c2.id))
        .await
        .unwrap();
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
    Fixture {
        school,
        year,
        sec1,
        sec2,
        admin: admin.id,
    }
}

async fn student(pool: &PgPool, f: &Fixture, name: &str, section: &Section) -> Uuid {
    let mut c = pool.acquire().await.unwrap();
    let actor = Actor::system_for(f.school.id);
    let s = students::create(
        &mut c,
        &hlc(),
        actor,
        students::NewStudent {
            school_id: f.school.id,
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
        },
    )
    .await
    .unwrap();
    students::enrol(&mut c, &hlc(), actor, s.id, section.id, None)
        .await
        .unwrap();
    s.id
}

fn entry(student_id: Uuid, status: AttendanceStatus, note: &str) -> attendance::EntryInput {
    attendance::EntryInput {
        student_id,
        status,
        note: if note.is_empty() {
            None
        } else {
            Some(note.into())
        },
    }
}

fn save(
    f: &Fixture,
    section: &Section,
    date: NaiveDate,
    entries: Vec<attendance::EntryInput>,
) -> attendance::SaveSheet {
    attendance::SaveSheet {
        school_id: f.school.id,
        section_id: section.id,
        date,
        marked_by: f.admin,
        entries,
    }
}

#[tokio::test]
async fn calendar_weekend_days_and_holidays() {
    let db: TestDb = test_db!();
    let f = fixture(&db.pool).await;
    let actor = Actor::system_for(f.school.id);
    let mut c = db.pool.acquire().await.unwrap();
    assert_eq!(
        attendance::weekend_days(&mut c, f.school.id).await.unwrap(),
        vec![7],
        "Sunday off by default"
    );
    assert_eq!(
        attendance::set_weekend_days(&mut c, &hlc(), actor, f.school.id, &[7, 6, 6])
            .await
            .unwrap(),
        vec![6, 7]
    );
    assert!(matches!(
        attendance::set_weekend_days(&mut c, &hlc(), actor, f.school.id, &[1, 2, 3, 4, 5, 6, 7])
            .await,
        Err(DbError::Conflict(_))
    ));
    let eid = attendance::create_holiday(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        " Eid ul Adha ",
        d(2026, 5, 27),
        d(2026, 5, 30),
    )
    .await
    .unwrap();
    assert_eq!(eid.name, "Eid ul Adha");
    assert!(matches!(
        attendance::create_holiday(
            &mut c,
            &hlc(),
            actor,
            f.school.id,
            "Bad",
            d(2026, 6, 2),
            d(2026, 6, 1)
        )
        .await,
        Err(DbError::Conflict(_))
    ));
    assert!(matches!(
        attendance::create_holiday(
            &mut c,
            &hlc(),
            actor,
            f.school.id,
            "  ",
            d(2026, 6, 1),
            d(2026, 6, 1)
        )
        .await,
        Err(DbError::Conflict(_))
    ));
    assert_eq!(
        attendance::day_kind(&mut c, f.school.id, d(2026, 5, 28))
            .await
            .unwrap(),
        DayKind::Holiday {
            name: "Eid ul Adha".into()
        }
    );
    // Saturday inside the holiday reports the holiday; the Sunday after is a weekend.
    assert!(matches!(
        attendance::day_kind(&mut c, f.school.id, d(2026, 5, 30))
            .await
            .unwrap(),
        DayKind::Holiday { .. }
    ));
    assert_eq!(
        attendance::day_kind(&mut c, f.school.id, d(2026, 5, 31))
            .await
            .unwrap(),
        DayKind::Weekend
    );
    assert_eq!(
        attendance::day_kind(&mut c, f.school.id, d(2026, 6, 1))
            .await
            .unwrap(),
        DayKind::School
    );
    // Range filter overlaps.
    assert_eq!(
        attendance::holidays(
            &mut c,
            f.school.id,
            Some(d(2026, 5, 30)),
            Some(d(2026, 6, 30))
        )
        .await
        .unwrap()
        .len(),
        1
    );
    assert!(
        attendance::holidays(&mut c, f.school.id, Some(d(2026, 6, 1)), None)
            .await
            .unwrap()
            .is_empty()
    );
    attendance::delete_holiday(&mut c, &hlc(), actor, eid.id)
        .await
        .unwrap();
    assert!(matches!(
        attendance::delete_holiday(&mut c, &hlc(), actor, eid.id).await,
        Err(DbError::NotFound(_))
    ));
    assert_eq!(
        attendance::day_kind(&mut c, f.school.id, d(2026, 5, 28))
            .await
            .unwrap(),
        DayKind::School
    );
    let trail = audit::for_entity(&mut c, "holiday", eid.id).await.unwrap();
    assert_eq!(
        trail.iter().map(|e| e.action.as_str()).collect::<Vec<_>>(),
        ["create", "delete"]
    );
}

#[tokio::test]
async fn sheet_submission_defaults_everyone_present_and_refuses_resubmission() {
    let db: TestDb = test_db!();
    let f = fixture(&db.pool).await;
    let actor = Actor::user(f.admin, f.school.id);
    let ali = student(&db.pool, &f, "Ali", &f.sec1).await;
    let sara = student(&db.pool, &f, "Sara", &f.sec1).await;
    let omar = student(&db.pool, &f, "Omar", &f.sec1).await;
    let zain = student(&db.pool, &f, "Zain", &f.sec2).await;
    let mut c = db.pool.acquire().await.unwrap();
    let monday = d(2026, 4, 6);

    let roster = attendance::roster(&mut c, f.sec1.id, monday).await.unwrap();
    assert_eq!(roster.len(), 3);
    assert!(roster.iter().all(|r| r.status.is_none()));
    assert!(attendance::sheet(&mut c, f.sec1.id, monday)
        .await
        .unwrap()
        .is_none());

    // Weekend and holiday refuse.
    assert!(matches!(
        attendance::save_sheet(&mut c, &hlc(), actor, save(&f, &f.sec1, d(2026, 4, 5), vec![]), false).await,
        Err(DbError::Conflict(m)) if m.contains("weekend")
    ));
    attendance::create_holiday(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        "Founders Day",
        d(2026, 4, 9),
        d(2026, 4, 9),
    )
    .await
    .unwrap();
    assert!(matches!(
        attendance::save_sheet(&mut c, &hlc(), actor, save(&f, &f.sec1, d(2026, 4, 9), vec![]), false).await,
        Err(DbError::Conflict(m)) if m.contains("Founders Day")
    ));
    // A student from another section is refused.
    assert!(matches!(
        attendance::save_sheet(&mut c, &hlc(), actor, save(&f, &f.sec1, monday, vec![entry(zain, AttendanceStatus::Absent, "")]), false).await,
        Err(DbError::Conflict(m)) if m.contains("not enrolled")
    ));

    // Only the exceptions are sent; everyone else is present.
    let (sheet, outcome) = attendance::save_sheet(
        &mut c,
        &hlc(),
        actor,
        save(
            &f,
            &f.sec1,
            monday,
            vec![
                entry(sara, AttendanceStatus::Absent, " sick "),
                entry(omar, AttendanceStatus::Late, ""),
            ],
        ),
        false,
    )
    .await
    .unwrap();
    assert!(outcome.created);
    assert_eq!(
        (sheet.present, sheet.absent, sheet.late, sheet.leave),
        (1, 1, 1, 0)
    );
    assert_eq!(sheet.marked_by, Some(f.admin));
    assert_eq!(sheet.academic_year_id, f.year.id);
    let entries = attendance::entries(&mut c, sheet.id).await.unwrap();
    assert_eq!(entries.len(), 3, "one entry per enrolled student");
    let by = |id: Uuid| entries.iter().find(|e| e.student_id == id).unwrap();
    assert_eq!(
        (by(ali).status.as_str(), by(ali).note.clone()),
        ("present", None)
    );
    assert_eq!(
        (by(sara).status.as_str(), by(sara).note.as_deref()),
        ("absent", Some("sick"))
    );
    assert_eq!(by(omar).status.as_str(), "late");
    let roster = attendance::roster(&mut c, f.sec1.id, monday).await.unwrap();
    assert_eq!(
        roster
            .iter()
            .filter(|r| r.status.as_deref() == Some("present"))
            .count(),
        1
    );

    // Saving again without the correction right is refused.
    assert!(matches!(
        attendance::save_sheet(&mut c, &hlc(), actor, save(&f, &f.sec1, monday, vec![]), false).await,
        Err(DbError::Conflict(m)) if m.contains("already submitted")
    ));
    let history = attendance::sheet_history(&mut c, sheet.id).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].action, "create");
    assert_eq!(
        history[0].new_value.as_ref().unwrap()["exceptions"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    // A section with no active students cannot be marked.
    let empty = academics::create_section(
        &mut c,
        &hlc(),
        actor,
        academics::NewSection {
            school_id: f.school.id,
            class_id: f.sec1.class_id,
            academic_year_id: f.year.id,
            name: "B".into(),
            class_teacher_id: None,
            capacity: None,
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        attendance::save_sheet(&mut c, &hlc(), actor, save(&f, &empty, monday, vec![]), false).await,
        Err(DbError::Conflict(m)) if m.contains("no active students")
    ));
}

#[tokio::test]
async fn corrections_change_only_what_differs_and_are_audited_per_entry() {
    let db: TestDb = test_db!();
    let f = fixture(&db.pool).await;
    let actor = Actor::user(f.admin, f.school.id);
    let _ali = student(&db.pool, &f, "Ali", &f.sec1).await;
    let sara = student(&db.pool, &f, "Sara", &f.sec1).await;
    let omar = student(&db.pool, &f, "Omar", &f.sec1).await;
    let mut c = db.pool.acquire().await.unwrap();
    let monday = d(2026, 4, 6);
    let (sheet, _) = attendance::save_sheet(
        &mut c,
        &hlc(),
        actor,
        save(
            &f,
            &f.sec1,
            monday,
            vec![
                entry(sara, AttendanceStatus::Absent, "sick"),
                entry(omar, AttendanceStatus::Late, ""),
            ],
        ),
        false,
    )
    .await
    .unwrap();

    // Correction sends the full list of exceptions: Sara was actually present; Omar still late.
    let (sheet2, outcome) = attendance::save_sheet(
        &mut c,
        &hlc(),
        actor,
        save(
            &f,
            &f.sec1,
            monday,
            vec![entry(omar, AttendanceStatus::Late, "")],
        ),
        true,
    )
    .await
    .unwrap();
    assert_eq!(sheet2.id, sheet.id);
    assert_eq!(
        outcome,
        attendance::SaveOutcome {
            created: false,
            changed: 1
        }
    );
    assert_eq!((sheet2.present, sheet2.absent, sheet2.late), (2, 0, 1));
    // An identical correction changes nothing and adds no audit noise.
    let (_, again) = attendance::save_sheet(
        &mut c,
        &hlc(),
        actor,
        save(
            &f,
            &f.sec1,
            monday,
            vec![entry(omar, AttendanceStatus::Late, "")],
        ),
        true,
    )
    .await
    .unwrap();
    assert_eq!(again.changed, 0);
    let history = attendance::sheet_history(&mut c, sheet.id).await.unwrap();
    let kinds: Vec<(&str, &str)> = history
        .iter()
        .map(|e| (e.entity.as_str(), e.action.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            ("attendance_sheet", "create"),
            ("attendance_entry", "update"),
            ("attendance_sheet", "update")
        ]
    );
    let fix = &history[1];
    assert_eq!(fix.actor_user_id, Some(f.admin));
    assert_eq!(fix.old_value.as_ref().unwrap()["status"], "absent");
    assert_eq!(fix.old_value.as_ref().unwrap()["note"], "sick");
    assert_eq!(fix.new_value.as_ref().unwrap()["status"], "present");
    assert_eq!(
        fix.new_value.as_ref().unwrap()["student_id"],
        json!(sara.to_string())
    );
    assert_eq!(
        history[2].new_value.as_ref().unwrap()["corrected_entries"],
        1
    );

    // A student enrolled after submission gets an entry on the next correction.
    let late_joiner = student(&db.pool, &f, "New", &f.sec1).await;
    let (sheet3, outcome) = attendance::save_sheet(
        &mut c,
        &hlc(),
        actor,
        save(
            &f,
            &f.sec1,
            monday,
            vec![
                entry(omar, AttendanceStatus::Late, ""),
                entry(late_joiner, AttendanceStatus::Leave, "joined"),
            ],
        ),
        true,
    )
    .await
    .unwrap();
    assert_eq!(outcome.changed, 1);
    assert_eq!((sheet3.present, sheet3.late, sheet3.leave), (2, 1, 1));
    assert_eq!(
        attendance::entries(&mut c, sheet.id).await.unwrap().len(),
        4
    );
    assert_eq!(
        attendance::student_history(&mut c, sara, d(2026, 4, 1), d(2026, 4, 30))
            .await
            .unwrap()
            .iter()
            .map(|r| r.status.as_str())
            .collect::<Vec<_>>(),
        ["present"]
    );
}

#[tokio::test]
async fn history_register_absentees_and_daily_status() {
    let db: TestDb = test_db!();
    let f = fixture(&db.pool).await;
    let actor = Actor::user(f.admin, f.school.id);
    let ali = student(&db.pool, &f, "Ali", &f.sec1).await;
    let sara = student(&db.pool, &f, "Sara", &f.sec1).await;
    let _zain = student(&db.pool, &f, "Zain", &f.sec2).await;
    let mut c = db.pool.acquire().await.unwrap();
    let father = guardians::create(
        &mut c,
        &hlc(),
        actor,
        guardians::NewGuardian {
            school_id: f.school.id,
            full_name: "Ahmed".into(),
            phone: Some("03001234567".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    guardians::link(&mut c, &hlc(), actor, sara, father.id, "father", true)
        .await
        .unwrap();

    for (date, sara_status) in [
        (d(2026, 4, 6), AttendanceStatus::Absent),
        (d(2026, 4, 7), AttendanceStatus::Absent),
        (d(2026, 4, 8), AttendanceStatus::Late),
    ] {
        attendance::save_sheet(
            &mut c,
            &hlc(),
            actor,
            save(&f, &f.sec1, date, vec![entry(sara, sara_status, "")]),
            false,
        )
        .await
        .unwrap();
    }
    let s = attendance::student_summary(&mut c, sara, d(2026, 4, 1), d(2026, 4, 30))
        .await
        .unwrap();
    assert_eq!((s.present, s.absent, s.late, s.leave), (0, 2, 1, 0));
    assert_eq!(s.percent(), Some(33.3));
    let ali_s = attendance::student_summary(&mut c, ali, d(2026, 4, 1), d(2026, 4, 30))
        .await
        .unwrap();
    assert_eq!((ali_s.present, ali_s.percent()), (3, Some(100.0)));
    let hist = attendance::student_history(&mut c, sara, d(2026, 4, 1), d(2026, 4, 30))
        .await
        .unwrap();
    assert_eq!(
        hist.iter().map(|r| r.date).collect::<Vec<_>>(),
        [d(2026, 4, 8), d(2026, 4, 7), d(2026, 4, 6)],
        "newest first"
    );
    assert_eq!(hist[0].section_id, f.sec1.id);
    // Outside the range: nothing.
    assert!(
        attendance::student_history(&mut c, sara, d(2026, 5, 1), d(2026, 5, 31))
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        attendance::section_entries(&mut c, f.sec1.id, d(2026, 4, 1), d(2026, 4, 30))
            .await
            .unwrap()
            .len(),
        6
    );

    let abs = attendance::absentees(&mut c, f.school.id, d(2026, 4, 6), None, None)
        .await
        .unwrap();
    assert_eq!(abs.len(), 1);
    assert_eq!(abs[0].full_name, "Sara");
    assert_eq!(abs[0].class_name, "1");
    assert_eq!(abs[0].status, "absent");
    assert_eq!(abs[0].guardian_phone.as_deref(), Some("03001234567"));
    assert!(attendance::absentees(
        &mut c,
        f.school.id,
        d(2026, 4, 6),
        Some(f.sec2.class_id),
        None
    )
    .await
    .unwrap()
    .is_empty());
    assert!(
        attendance::absentees(&mut c, f.school.id, d(2026, 4, 6), None, Some(&[f.sec2.id]))
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        attendance::absentees(&mut c, f.school.id, d(2026, 4, 8), None, Some(&[f.sec1.id]))
            .await
            .unwrap()[0]
            .status,
        "late"
    );

    let status = attendance::daily_status(&mut c, f.school.id, f.year.id, d(2026, 4, 6), None)
        .await
        .unwrap();
    assert_eq!(status.len(), 2);
    let s1 = status.iter().find(|r| r.section_id == f.sec1.id).unwrap();
    assert_eq!(
        (s1.enrolled, s1.present, s1.absent),
        (2, Some(2 - 1), Some(1))
    );
    assert!(s1.sheet_id.is_some());
    let s2 = status.iter().find(|r| r.section_id == f.sec2.id).unwrap();
    assert_eq!((s2.enrolled, s2.sheet_id), (1, None), "not yet marked");
    assert_eq!(
        attendance::daily_status(
            &mut c,
            f.school.id,
            f.year.id,
            d(2026, 4, 6),
            Some(&[f.sec1.id])
        )
        .await
        .unwrap()
        .len(),
        1
    );
}

#[tokio::test]
async fn staff_attendance_upserts_and_audits() {
    let db: TestDb = test_db!();
    let f = fixture(&db.pool).await;
    let actor = Actor::user(f.admin, f.school.id);
    let mut c = db.pool.acquire().await.unwrap();
    let mk = |n: &str, name: &str| staff::NewStaff {
        school_id: f.school.id,
        user_id: None,
        staff_number: n.into(),
        full_name: name.into(),
        father_name: "F".into(),
        gender: "female".into(),
        phone: None,
        email: None,
        cnic: None,
        designation: "Teacher".into(),
        joined_on: None,
    };
    let s1 = staff::create(&mut c, &hlc(), actor, mk("T-001", "Sana"))
        .await
        .unwrap();
    let s2 = staff::create(&mut c, &hlc(), actor, mk("T-002", "Hira"))
        .await
        .unwrap();
    let monday = d(2026, 4, 6);
    let day = attendance::staff_day(&mut c, f.school.id, monday)
        .await
        .unwrap();
    assert_eq!(day.len(), 2);
    assert!(day.iter().all(|r| r.status.is_none()));

    let leave = attendance::StaffEntryInput {
        staff_id: s1.id,
        status: StaffStatus::Leave,
        note: Some("casual leave".into()),
    };
    let n = attendance::save_staff_day(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        monday,
        f.admin,
        &[leave.clone()],
    )
    .await
    .unwrap();
    assert_eq!(n, 2, "Sana on leave, Hira present by default");
    let n = attendance::save_staff_day(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        monday,
        f.admin,
        &[leave.clone()],
    )
    .await
    .unwrap();
    assert_eq!(n, 0, "nothing changed");
    let n = attendance::save_staff_day(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        monday,
        f.admin,
        &[
            leave,
            attendance::StaffEntryInput {
                staff_id: s2.id,
                status: StaffStatus::Absent,
                note: None,
            },
        ],
    )
    .await
    .unwrap();
    assert_eq!(n, 1);
    let day = attendance::staff_day(&mut c, f.school.id, monday)
        .await
        .unwrap();
    let hira = day.iter().find(|r| r.staff_id == s2.id).unwrap();
    assert_eq!(hira.status.as_deref(), Some("absent"));
    let hist = attendance::staff_history(&mut c, s2.id, d(2026, 4, 1), d(2026, 4, 30))
        .await
        .unwrap();
    assert_eq!(hist.len(), 1);
    assert_eq!(hist[0].status, "absent");
    let trail = audit::for_entity(&mut c, "staff_attendance", hist[0].id)
        .await
        .unwrap();
    assert_eq!(
        trail.iter().map(|e| e.action.as_str()).collect::<Vec<_>>(),
        ["create", "update"]
    );
    assert_eq!(trail[1].old_value.as_ref().unwrap()["status"], "present");
    // Weekend and unknown staff refuse.
    assert!(matches!(
        attendance::save_staff_day(
            &mut c,
            &hlc(),
            actor,
            f.school.id,
            d(2026, 4, 5),
            f.admin,
            &[]
        )
        .await,
        Err(DbError::Conflict(_))
    ));
    assert!(matches!(
        attendance::save_staff_day(
            &mut c,
            &hlc(),
            actor,
            f.school.id,
            monday,
            f.admin,
            &[attendance::StaffEntryInput {
                staff_id: Uuid::nil(),
                status: StaffStatus::Absent,
                note: None
            }]
        )
        .await,
        Err(DbError::Conflict(_))
    ));
}
