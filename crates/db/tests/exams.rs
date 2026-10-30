//! M1 stage 4: exams. Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

use chrono::NaiveDate;
use madrasa_core::exams::{GradeBand, GradingScheme};
use madrasa_core::hlc::Hlc;
use madrasa_core::password::hash_password;
use madrasa_db::models::{AcademicYear, School, Section};
use madrasa_db::testing::TestDb;
use madrasa_db::{
    academics, audit, exams, schools, students, test_db, users, Actor, DbError, PgPool,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::json;
use uuid::Uuid;

fn hlc() -> Hlc {
    Hlc::new(13)
}
fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

struct Fixture {
    school: School,
    year: AcademicYear,
    sec1: Section,
    sec2: Section,
    english: Uuid,
    maths: Uuid,
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
    let english = academics::create_subject(&mut c, &hlc(), actor, school.id, "English", "ENG")
        .await
        .unwrap();
    let maths = academics::create_subject(&mut c, &hlc(), actor, school.id, "Mathematics", "MTH")
        .await
        .unwrap();
    academics::set_class_subjects(&mut c, &hlc(), actor, c1.id, &[english.id, maths.id])
        .await
        .unwrap();
    academics::set_class_subjects(&mut c, &hlc(), actor, c2.id, &[english.id])
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
        english: english.id,
        maths: maths.id,
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

fn mark(student_id: Uuid, obtained: Option<Decimal>, absent: bool) -> exams::MarkInput {
    exams::MarkInput {
        student_id,
        obtained,
        is_absent: absent,
    }
}

#[tokio::test]
async fn assessments_weights_and_grading_scheme() {
    let db: TestDb = test_db!();
    let f = fixture(&db.pool).await;
    let actor = Actor::user(f.admin, f.school.id);
    let mut c = db.pool.acquire().await.unwrap();
    let list = exams::create_default_assessments(&mut c, &hlc(), actor, f.school.id, f.year.id)
        .await
        .unwrap();
    assert_eq!(
        list.iter()
            .map(|a| (a.name.as_str(), a.weight, a.position))
            .collect::<Vec<_>>(),
        [
            ("Bi-monthly Test 1", dec!(15), 0),
            ("Bi-monthly Test 2", dec!(15), 1),
            ("Mid-year", dec!(30), 2),
            ("Final", dec!(40), 3)
        ]
    );
    let again = exams::create_default_assessments(&mut c, &hlc(), actor, f.school.id, f.year.id)
        .await
        .unwrap();
    assert_eq!(again.len(), 4, "idempotent");
    assert!(
        matches!(
            exams::create_assessment(
                &mut c,
                &hlc(),
                actor,
                f.school.id,
                f.year.id,
                "final",
                dec!(0),
                9
            )
            .await,
            Err(DbError::Conflict(_))
        ),
        "names are unique per year, case-insensitively"
    );

    // Weights must total 100 and cover every assessment.
    let ids: Vec<Uuid> = list.iter().map(|a| a.id).collect();
    assert!(matches!(
        exams::set_weights(&mut c, &hlc(), actor, f.year.id, &[(ids[0], dec!(50)), (ids[1], dec!(50)), (ids[2], dec!(0)), (ids[3], dec!(1))]).await,
        Err(DbError::Conflict(m)) if m.contains("total 100")
    ));
    assert!(matches!(
        exams::set_weights(&mut c, &hlc(), actor, f.year.id, &[(ids[0], dec!(100))]).await,
        Err(DbError::Conflict(m)) if m.contains("every assessment")
    ));
    let updated = exams::set_weights(
        &mut c,
        &hlc(),
        actor,
        f.year.id,
        &[
            (ids[0], dec!(10)),
            (ids[1], dec!(10)),
            (ids[2], dec!(30)),
            (ids[3], dec!(50)),
        ],
    )
    .await
    .unwrap();
    assert_eq!(updated[3].weight, dec!(50));
    let trail = audit::for_entity(&mut c, "assessment_type", ids[3])
        .await
        .unwrap();
    assert_eq!(trail.len(), 2, "create + weight change");
    assert_eq!(
        trail[1].old_value.as_ref().unwrap()["weight"]
            .as_str()
            .unwrap()
            .parse::<Decimal>()
            .unwrap(),
        dec!(40)
    );
    let renamed = exams::rename_assessment(&mut c, &hlc(), actor, ids[0], " Monthly Test 1 ", 0)
        .await
        .unwrap();
    assert_eq!(renamed.name, "Monthly Test 1");
    exams::delete_assessment(&mut c, &hlc(), actor, ids[1])
        .await
        .unwrap();
    assert_eq!(
        exams::list_assessments(&mut c, f.year.id)
            .await
            .unwrap()
            .len(),
        3
    );

    // Grading: default scheme, custom scheme normalized and validated.
    let g = exams::grading(&mut c, f.school.id).await.unwrap();
    assert_eq!(g.bands[0].grade, "A+");
    assert_eq!(g.pass_percent, dec!(40));
    let custom = GradingScheme {
        bands: vec![
            GradeBand {
                grade: "Fail".into(),
                min_percent: dec!(0),
                remarks: "".into(),
            },
            GradeBand {
                grade: "Pass".into(),
                min_percent: dec!(33),
                remarks: "".into(),
            },
            GradeBand {
                grade: "Distinction".into(),
                min_percent: dec!(80),
                remarks: "Well done".into(),
            },
        ],
        pass_percent: dec!(33),
    };
    let saved = exams::set_grading(&mut c, &hlc(), actor, f.school.id, custom)
        .await
        .unwrap();
    assert_eq!(saved.bands[0].grade, "Distinction");
    let back = exams::grading(&mut c, f.school.id).await.unwrap();
    assert_eq!(back, saved);
    assert_eq!(back.grade_for(dec!(79.99)).unwrap().grade, "Pass");
    assert!(matches!(
        exams::set_grading(
            &mut c,
            &hlc(),
            actor,
            f.school.id,
            GradingScheme {
                bands: vec![],
                pass_percent: dec!(40)
            }
        )
        .await,
        Err(DbError::Conflict(_))
    ));
}

#[tokio::test]
async fn max_marks_sheet_workflow_validation_and_locking() {
    let db: TestDb = test_db!();
    let f = fixture(&db.pool).await;
    let actor = Actor::user(f.admin, f.school.id);
    let ali = student(&db.pool, &f, "Ali", &f.sec1).await;
    let sara = student(&db.pool, &f, "Sara", &f.sec1).await;
    let omar = student(&db.pool, &f, "Omar", &f.sec1).await;
    let zain = student(&db.pool, &f, "Zain", &f.sec2).await;
    let mut c = db.pool.acquire().await.unwrap();
    let list = exams::create_default_assessments(&mut c, &hlc(), actor, f.school.id, f.year.id)
        .await
        .unwrap();
    let test1 = list[0].id;
    let class1 = f.sec1.class_id;

    // Maximum marks: set, change, remove, set again.
    macro_rules! set {
        ($c:expr, $subject:expr, $max:expr) => {
            exams::set_max_marks(
                $c,
                &hlc(),
                actor,
                f.school.id,
                test1,
                class1,
                $subject,
                $max,
            )
        };
    }
    set!(&mut c, f.english, dec!(50)).await.unwrap();
    set!(&mut c, f.english, dec!(100)).await.unwrap();
    set!(&mut c, f.english, dec!(0)).await.unwrap();
    assert!(exams::max_marks(&mut c, test1, class1)
        .await
        .unwrap()
        .is_empty());
    set!(&mut c, f.english, dec!(50)).await.unwrap();
    assert!(matches!(
        set!(&mut c, f.english, dec!(12.345)).await,
        Err(DbError::Conflict(_))
    ));
    let summaries = exams::sheet_summaries(&mut c, test1, f.sec1.id)
        .await
        .unwrap();
    assert_eq!(summaries.len(), 2, "both class subjects listed");
    let eng = summaries
        .iter()
        .find(|s| s.subject_id == f.english)
        .unwrap();
    assert_eq!(
        (eng.max_marks, eng.sheet_id, eng.entered),
        (Some(dec!(50)), None, 0)
    );
    let mth = summaries.iter().find(|s| s.subject_id == f.maths).unwrap();
    assert_eq!(mth.max_marks, None);

    // No maximum for maths yet → cannot enter marks.
    assert!(matches!(
        exams::save_marks(&mut c, &hlc(), actor, f.school.id, test1, f.sec1.id, f.maths, &[mark(ali, Some(dec!(10)), false)]).await,
        Err(DbError::Conflict(m)) if m.contains("maximum marks")
    ));
    // Validation: over the maximum, negative, too many decimals, foreign student.
    for bad in [
        vec![mark(ali, Some(dec!(50.01)), false)],
        vec![mark(ali, Some(dec!(-1)), false)],
        vec![mark(ali, Some(dec!(12.345)), false)],
        vec![mark(zain, Some(dec!(10)), false)],
    ] {
        assert!(matches!(
            exams::save_marks(
                &mut c,
                &hlc(),
                actor,
                f.school.id,
                test1,
                f.sec1.id,
                f.english,
                &bad
            )
            .await,
            Err(DbError::Conflict(_))
        ));
    }

    // First save creates a draft sheet; Omar has nothing yet.
    let (sheet, changed) = exams::save_marks(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        test1,
        f.sec1.id,
        f.english,
        &[
            mark(ali, Some(dec!(45)), false),
            mark(sara, None, true),
            mark(omar, None, false),
        ],
    )
    .await
    .unwrap();
    assert_eq!((sheet.status.as_str(), changed), ("draft", 2));
    let summaries = exams::sheet_summaries(&mut c, test1, f.sec1.id)
        .await
        .unwrap();
    let eng = summaries
        .iter()
        .find(|s| s.subject_id == f.english)
        .unwrap();
    assert_eq!(
        (eng.sheet_id, eng.entered, eng.status.as_deref()),
        (Some(sheet.id), 2, Some("draft"))
    );
    // Submit refuses an incomplete sheet.
    assert!(matches!(
        exams::submit_sheet(&mut c, &hlc(), actor, sheet.id, f.admin).await,
        Err(DbError::Conflict(m)) if m.contains("1 student(s)")
    ));
    // Same marks again: nothing changes, no audit noise.
    let (_, changed) = exams::save_marks(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        test1,
        f.sec1.id,
        f.english,
        &[mark(ali, Some(dec!(45)), false)],
    )
    .await
    .unwrap();
    assert_eq!(changed, 0);
    let (_, changed) = exams::save_marks(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        test1,
        f.sec1.id,
        f.english,
        &[mark(omar, Some(dec!(30.5)), false)],
    )
    .await
    .unwrap();
    assert_eq!(changed, 1);
    let marks = exams::marks(&mut c, sheet.id).await.unwrap();
    assert_eq!(marks.len(), 3);
    let sara_mark = marks.iter().find(|m| m.student_id == sara).unwrap();
    assert_eq!((sara_mark.obtained, sara_mark.is_absent), (None, true));

    // Workflow: submit → locked for teachers; approve; max marks locked; reopen.
    let submitted = exams::submit_sheet(&mut c, &hlc(), actor, sheet.id, f.admin)
        .await
        .unwrap();
    assert_eq!(submitted.status, "submitted");
    assert_eq!(submitted.submitted_by, Some(f.admin));
    assert!(submitted.submitted_at.is_some());
    assert!(matches!(
        exams::save_marks(&mut c, &hlc(), actor, f.school.id, test1, f.sec1.id, f.english, &[mark(ali, Some(dec!(46)), false)]).await,
        Err(DbError::Conflict(m)) if m.contains("locked")
    ));
    assert!(matches!(
        exams::submit_sheet(&mut c, &hlc(), actor, sheet.id, f.admin).await,
        Err(DbError::Conflict(_))
    ));
    let approved = exams::approve_sheet(&mut c, &hlc(), actor, sheet.id, f.admin)
        .await
        .unwrap();
    assert_eq!(
        (approved.status.as_str(), approved.approved_by),
        ("approved", Some(f.admin))
    );
    assert_eq!(approved.submitted_by, Some(f.admin), "submission is kept");
    assert!(matches!(
        exams::approve_sheet(&mut c, &hlc(), actor, sheet.id, f.admin).await,
        Err(DbError::Conflict(_))
    ));
    assert!(matches!(
        set!(&mut c, f.english, dec!(60)).await,
        Err(DbError::Conflict(m)) if m.contains("locked")
    ));
    assert!(matches!(
        exams::delete_assessment(&mut c, &hlc(), actor, test1).await,
        Err(DbError::Conflict(_))
    ));
    let reopened = exams::reopen_sheet(&mut c, &hlc(), actor, sheet.id)
        .await
        .unwrap();
    assert_eq!(
        (
            reopened.status.as_str(),
            reopened.approved_by,
            reopened.submitted_by
        ),
        ("draft", None, None)
    );
    assert!(matches!(
        exams::reopen_sheet(&mut c, &hlc(), actor, sheet.id).await,
        Err(DbError::Conflict(_))
    ));
    let (_, changed) = exams::save_marks(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        test1,
        f.sec1.id,
        f.english,
        &[mark(ali, Some(dec!(46)), false)],
    )
    .await
    .unwrap();
    assert_eq!(changed, 1, "editable again after reopening");

    let trail = audit::for_entity(&mut c, "mark_sheet", sheet.id)
        .await
        .unwrap();
    let kinds: Vec<String> = trail
        .iter()
        .map(|e| {
            if e.action == "create" {
                return "create:marks".to_string();
            }
            e.new_value
                .as_ref()
                .and_then(|v| v["status"].as_str().map(str::to_owned))
                .unwrap_or_else(|| format!("{}:marks", e.action))
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "create:marks",
            "update:marks",
            "update:marks",
            "submitted",
            "approved",
            "draft",
            "update:marks"
        ]
    );
    assert_eq!(trail[1].new_value.as_ref().unwrap()["marks_changed"], 2);
    assert_eq!(
        trail[6].new_value.as_ref().unwrap()["changes"][0]["from"]["obtained"]
            .as_str()
            .unwrap()
            .parse::<Decimal>()
            .unwrap(),
        dec!(45)
    );
}

#[tokio::test]
async fn section_and_student_mark_rows() {
    let db: TestDb = test_db!();
    let f = fixture(&db.pool).await;
    let actor = Actor::user(f.admin, f.school.id);
    let ali = student(&db.pool, &f, "Ali", &f.sec1).await;
    let sara = student(&db.pool, &f, "Sara", &f.sec1).await;
    let mut c = db.pool.acquire().await.unwrap();
    let list = exams::create_default_assessments(&mut c, &hlc(), actor, f.school.id, f.year.id)
        .await
        .unwrap();
    let (test1, mid) = (list[0].id, list[2].id);
    let class1 = f.sec1.class_id;
    for (assessment, max) in [(test1, dec!(20)), (mid, dec!(50))] {
        for subject in [f.english, f.maths] {
            exams::set_max_marks(
                &mut c,
                &hlc(),
                actor,
                f.school.id,
                assessment,
                class1,
                subject,
                max,
            )
            .await
            .unwrap();
        }
    }
    exams::save_marks(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        test1,
        f.sec1.id,
        f.english,
        &[
            mark(ali, Some(dec!(18)), false),
            mark(sara, Some(dec!(12)), false),
        ],
    )
    .await
    .unwrap();
    exams::save_marks(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        test1,
        f.sec1.id,
        f.maths,
        &[mark(ali, Some(dec!(20)), false), mark(sara, None, true)],
    )
    .await
    .unwrap();
    exams::save_marks(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        mid,
        f.sec1.id,
        f.english,
        &[mark(ali, Some(dec!(45)), false)],
    )
    .await
    .unwrap();

    let rows = exams::section_marks(&mut c, test1, f.sec1.id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 4);
    let sara_maths = rows
        .iter()
        .find(|r| r.student_id == sara && r.subject_id == f.maths)
        .unwrap();
    assert_eq!(
        (
            sara_maths.is_absent,
            sara_maths.obtained,
            sara_maths.max_marks
        ),
        (true, None, dec!(20))
    );
    assert!(rows.iter().all(|r| r.sheet_status == "draft"));
    let mine = exams::student_marks(&mut c, ali, f.year.id).await.unwrap();
    assert_eq!(mine.len(), 3);
    assert!(mine
        .iter()
        .any(|r| r.assessment_type_id == mid && r.obtained == Some(dec!(45))));
    assert!(exams::student_marks(&mut c, sara, f.year.id)
        .await
        .unwrap()
        .iter()
        .all(|r| r.assessment_type_id == test1));
    assert!(exams::section_marks(&mut c, mid, f.sec2.id)
        .await
        .unwrap()
        .is_empty());
}
