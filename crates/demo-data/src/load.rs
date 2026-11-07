//! Load a generated demo school into the database. Requires explicit confirmation and refuses
//! to touch a database that already holds a real (non-demo) school.

use crate::generate::DemoSchool;
use crate::DEMO_PASSWORD;
use chrono::{NaiveDate, Utc};
use madrasa_core::attendance::{AttendanceStatus, StaffStatus};
use madrasa_core::edition::Edition;
use madrasa_core::fees::PaymentMethod;
use madrasa_core::hlc::Hlc;
use madrasa_core::password::hash_password;
use madrasa_db::{
    academics, attendance, boards, exams, fees, guardians, roles, schools, settings, staff,
    students, users, Actor, PgPool,
};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    /// Must be `true`; the loader never runs implicitly.
    pub confirmed: bool,
    /// Also write the generated data as JSON files here.
    pub out_dir: Option<PathBuf>,
    /// Fees and attendance for the whole academic year up to today (performance checks)
    /// instead of April and May only.
    pub full_year: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadReport {
    pub school_id: uuid::Uuid,
    pub staff_accounts: usize,
    pub subjects: usize,
    pub classes: usize,
    pub sections: usize,
    pub students: usize,
    pub guardians: usize,
    pub vouchers: usize,
    pub receipts: usize,
    pub attendance_sheets: usize,
    pub staff_attendance_days: usize,
    pub mark_sheets: usize,
    pub board_candidates: usize,
    pub allocated_students: usize,
    pub consents_recorded: usize,
    pub files_written: Vec<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("refusing to load demo data without explicit confirmation")]
    NotConfirmed,
    #[error("refusing to load demo data: this database already contains a real school")]
    RealSchoolPresent,
    #[error("demo data is already loaded")]
    AlreadyLoaded,
    #[error(transparent)]
    Db(#[from] madrasa_db::DbError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Insert the demo school, roles and staff accounts in one transaction.
pub async fn load(
    pool: &PgPool,
    hlc: &Hlc,
    demo: &DemoSchool,
    options: &LoadOptions,
) -> Result<LoadReport, LoadError> {
    if !options.confirmed {
        return Err(LoadError::NotConfirmed);
    }
    let mut tx = pool.begin().await.map_err(madrasa_db::DbError::from)?;
    if schools::count(&mut tx, false).await? > 0 {
        return Err(LoadError::RealSchoolPresent);
    }
    if schools::count(&mut tx, true).await? > 0 {
        return Err(LoadError::AlreadyLoaded);
    }

    let school = schools::create(
        &mut tx,
        hlc,
        Actor::SYSTEM,
        schools::NewSchool {
            name: demo.school.name.clone(),
            code: demo.school.code.clone(),
            city: demo.school.city.clone(),
            is_demo: true,
            academic_year_start_month: demo.school.academic_year_start_month,
            currency: demo.school.currency.clone(),
            country: "PK".into(),
            time_zone: "Asia/Karachi".into(),
        },
    )
    .await?;
    let actor = Actor::system_for(school.id);
    let created_roles =
        roles::create_defaults(&mut tx, hlc, actor, school.id, Edition::Lite).await?;

    // One hash for all demo accounts (hashing 120 passwords would take a minute).
    let password_hash = hash_password(DEMO_PASSWORD).map_err(|e| anyhow::anyhow!(e))?;
    let mut staff_by_username: HashMap<String, uuid::Uuid> = HashMap::new();
    for (i, member) in demo.staff.iter().enumerate() {
        let user = users::create(
            &mut tx,
            hlc,
            actor,
            users::NewUser {
                school_id: school.id,
                username: member.username.clone(),
                full_name: member.full_name.clone(),
                email: None,
                password_hash: password_hash.clone(),
            },
        )
        .await?;
        if let Some(role) = created_roles.iter().find(|r| r.code == member.role) {
            roles::assign(&mut tx, hlc, actor, user.id, role).await?;
        }
        let profile = staff::create(
            &mut tx,
            hlc,
            actor,
            staff::NewStaff {
                school_id: school.id,
                user_id: Some(user.id),
                staff_number: format!("DEMO-S{:03}", i + 1),
                full_name: member.full_name.clone(),
                father_name: String::new(),
                gender: "other".into(),
                phone: Some(member.phone.clone()),
                email: None,
                cnic: Some(member.cnic.clone()),
                designation: match member.role.as_str() {
                    "teacher" => {
                        format!("Teacher ({})", member.subject.clone().unwrap_or_default())
                    }
                    "principal" => "Principal".into(),
                    "admin" => "Administrator".into(),
                    other => other.to_string(),
                },
                joined_on: NaiveDate::from_ymd_opt(demo.academic_year_start - 3, 4, 1),
                custom: serde_json::json!({}),
            },
        )
        .await?;
        staff_by_username.insert(member.username.clone(), profile.id);
    }

    // Academic year, subjects, classes and sections.
    let (name, start, end) = academics::default_year_bounds(demo.academic_year_start, 4)
        .ok_or_else(|| anyhow::anyhow!("bad year"))?;
    let year = academics::create_year(
        &mut tx,
        hlc,
        actor,
        academics::NewAcademicYear {
            school_id: school.id,
            name,
            start_date: start,
            end_date: end,
            start_month: 4,
        },
    )
    .await?;
    academics::activate_year(&mut tx, hlc, actor, year.id).await?;
    let mut subject_ids: HashMap<String, uuid::Uuid> = HashMap::new();
    for subj in &demo.subjects {
        let row = academics::create_subject(&mut tx, hlc, actor, school.id, &subj.name, &subj.code)
            .await?;
        subject_ids.insert(subj.name.clone(), row.id);
    }
    let mut class_ids: HashMap<u8, uuid::Uuid> = HashMap::new();
    let mut groups_by_class: HashMap<uuid::Uuid, (uuid::Uuid, uuid::Uuid)> = HashMap::new();
    for grade in 0..=12u8 {
        let class = academics::create_class(
            &mut tx,
            hlc,
            actor,
            school.id,
            &crate::generate::grade_name(grade),
            grade as i32,
        )
        .await?;
        let ids: Vec<uuid::Uuid> = demo
            .subjects
            .iter()
            .filter(|s| s.grades.contains(&grade))
            .filter_map(|s| subject_ids.get(&s.name).copied())
            .collect();
        academics::set_class_subjects(&mut tx, hlc, actor, class.id, &ids).await?;
        // Grades 9–12 choose a science or computer group (M2 stage 6).
        if grade >= 9 {
            let pick = |names: &[&str]| -> Vec<uuid::Uuid> {
                names
                    .iter()
                    .filter_map(|n| subject_ids.get(*n).copied())
                    .collect()
            };
            let electives = pick(&["Physics", "Chemistry", "Biology", "Computer"]);
            madrasa_db::subjects::set_class_electives(&mut tx, hlc, actor, class.id, &electives)
                .await?;
            let pre_med = madrasa_db::subjects::create_group(
                &mut tx,
                hlc,
                actor,
                class.id,
                "Pre-Medical",
                0,
                &pick(&["Physics", "Chemistry", "Biology"]),
            )
            .await?;
            let computer = madrasa_db::subjects::create_group(
                &mut tx,
                hlc,
                actor,
                class.id,
                "Computer Science",
                1,
                &pick(&["Physics", "Chemistry", "Computer"]),
            )
            .await?;
            groups_by_class.insert(class.id, (pre_med.group.id, computer.group.id));
        }
        class_ids.insert(grade, class.id);
    }
    let mut section_ids: HashMap<String, uuid::Uuid> = HashMap::new();
    for class in &demo.classes {
        let section = academics::create_section(
            &mut tx,
            hlc,
            actor,
            academics::NewSection {
                school_id: school.id,
                class_id: class_ids[&class.grade],
                academic_year_id: year.id,
                name: class.section.to_string(),
                class_teacher_id: class
                    .class_teacher
                    .as_ref()
                    .and_then(|u| staff_by_username.get(u).copied()),
                capacity: Some(35),
            },
        )
        .await?;
        section_ids.insert(class.name.clone(), section.id);
    }
    // Subject teachers: each teacher teaches their subject in a few sections.
    for member in demo.staff.iter().filter(|m| m.role == "teacher") {
        let (Some(subject), Some(staff_id)) = (
            member.subject.as_ref(),
            staff_by_username.get(&member.username),
        ) else {
            continue;
        };
        let Some(subject_id) = subject_ids.get(subject) else {
            continue;
        };
        let grades: Vec<u8> = demo
            .subjects
            .iter()
            .find(|s| &s.name == subject)
            .map(|s| s.grades.clone())
            .unwrap_or_default();
        let pairs: Vec<(uuid::Uuid, Option<uuid::Uuid>)> = demo
            .classes
            .iter()
            .filter(|c| {
                grades.contains(&c.grade)
                    && c.class_teacher.as_deref() == Some(member.username.as_str())
            })
            .filter_map(|c| section_ids.get(&c.name).map(|id| (*id, Some(*subject_id))))
            .collect();
        if !pairs.is_empty() {
            staff::set_assignments(&mut tx, hlc, actor, *staff_id, &pairs).await?;
        }
    }

    // Students with enrollments; guardians shared between siblings.
    let mut student_ids: HashMap<String, uuid::Uuid> = HashMap::new();
    for s in &demo.students {
        let row = students::create(
            &mut tx,
            hlc,
            actor,
            students::NewStudent {
                school_id: school.id,
                student_number: None,
                legacy_id: Some(s.admission_no.clone()),
                full_name: s.full_name.clone(),
                father_name: s.father_name.clone(),
                date_of_birth: s.date_of_birth,
                gender: if s.gender == 'F' {
                    "female".into()
                } else {
                    "male".into()
                },
                cnic: Some(s.b_form.clone()),
                address: s.address.clone(),
                admission_date: s.admission_date,
                board_roll_no: None,
                board_registration_no: None,
                custom: json!({ "demo": true }),
            },
        )
        .await?;
        if let Some(section_id) = section_ids.get(&s.class) {
            students::enrol(
                &mut tx,
                hlc,
                actor,
                row.id,
                *section_id,
                Some(s.roll_no as i32),
            )
            .await?;
        }
        student_ids.insert(s.admission_no.clone(), row.id);
    }
    for g in &demo.guardians {
        let row = guardians::create(
            &mut tx,
            hlc,
            actor,
            guardians::NewGuardian {
                school_id: school.id,
                full_name: g.full_name.clone(),
                phone: Some(g.phone.clone()),
                email: None,
                cnic: Some(g.cnic.clone()),
                occupation: g.occupation.clone(),
                address: String::new(),
                custom: serde_json::json!({}),
            },
        )
        .await?;
        for adm in &g.students {
            if let Some(sid) = student_ids.get(adm) {
                guardians::link(&mut tx, hlc, actor, *sid, row.id, &g.relationship, true).await?;
            }
        }
    }

    // Fees: heads, a structure per class, April and May vouchers, ~70% of April paid (some late).
    let principal_user = users::find_by_username(&mut tx, school.id, &demo.staff[0].username)
        .await?
        .map(|u| u.id)
        .ok_or_else(|| anyhow::anyhow!("principal missing"))?;
    let tuition =
        fees::create_head(&mut tx, hlc, actor, school.id, "Tuition", "TUI", true, 0).await?;
    let exam = fees::create_head(&mut tx, hlc, actor, school.id, "Exam", "EXM", true, 1).await?;
    let admission =
        fees::create_head(&mut tx, hlc, actor, school.id, "Admission", "ADM", false, 2).await?;
    for grade in 0..=12u8 {
        let class_id = class_ids[&grade];
        let tuition_amount = dec!(2500) + rust_decimal::Decimal::from(grade) * dec!(150);
        fees::set_structure(
            &mut tx,
            hlc,
            actor,
            school.id,
            year.id,
            class_id,
            tuition.id,
            tuition_amount,
        )
        .await?;
        fees::set_structure(
            &mut tx,
            hlc,
            actor,
            school.id,
            year.id,
            class_id,
            exam.id,
            dec!(300),
        )
        .await?;
        fees::set_structure(
            &mut tx,
            hlc,
            actor,
            school.id,
            year.id,
            class_id,
            admission.id,
            dec!(5000),
        )
        .await?;
    }
    let mut vouchers = 0usize;
    let mut receipts = 0usize;
    let mut rng = StdRng::seed_from_u64(77);
    let april = chrono::NaiveDate::from_ymd_opt(demo.academic_year_start, 4, 1).expect("date");
    let rep = fees::generate_vouchers(&mut tx, hlc, actor, school.id, year.id, april, None, april)
        .await?;
    vouchers += rep.created;
    let policy = fees::policy(&mut tx, school.id).await?;
    for vid in &rep.voucher_ids {
        let roll: f64 = rng.gen();
        if roll < 0.55 {
            // Paid in full on time.
            let v = fees::get_voucher(&mut tx, *vid).await?;
            let paid_on = april + chrono::Duration::days(rng.gen_range(1..=14));
            fees::record_payment(
                &mut tx,
                hlc,
                actor,
                &policy,
                fees::NewPayment {
                    voucher_id: *vid,
                    amount: v.balance(),
                    method: if rng.gen_bool(0.7) {
                        PaymentMethod::Cash
                    } else {
                        PaymentMethod::Bank
                    },
                    reference: Some(format!("DEMO-{}", rng.gen_range(1000..9999))),
                    note: None,
                    paid_on,
                    received_by: principal_user,
                },
            )
            .await?;
            receipts += 1;
        } else if roll < 0.70 {
            // Paid late (late fee assessed), sometimes partially.
            let v = fees::get_voucher(&mut tx, *vid).await?;
            let paid_on = april + chrono::Duration::days(rng.gen_range(16..=28));
            let amount = if rng.gen_bool(0.5) {
                v.balance()
            } else {
                (v.balance() / dec!(2)).round_dp(2)
            };
            fees::record_payment(
                &mut tx,
                hlc,
                actor,
                &policy,
                fees::NewPayment {
                    voucher_id: *vid,
                    amount,
                    method: PaymentMethod::Kuickpay,
                    reference: Some(format!("KP{}", rng.gen_range(100000..999999))),
                    note: None,
                    paid_on,
                    received_by: principal_user,
                },
            )
            .await?;
            receipts += 1;
        }
    }
    let may = chrono::NaiveDate::from_ymd_opt(demo.academic_year_start, 5, 1).expect("date");
    let rep =
        fees::generate_vouchers(&mut tx, hlc, actor, school.id, year.id, may, None, may).await?;
    vouchers += rep.created;
    if options.full_year {
        // June onwards up to the current month: generated, and ~70% paid in full on time.
        let last = Utc::now().date_naive();
        let mut month = madrasa_core::fees::add_months(may, 1);
        while month <= last {
            let rep = fees::generate_vouchers(
                &mut tx, hlc, actor, school.id, year.id, month, None, month,
            )
            .await?;
            vouchers += rep.created;
            for vid in &rep.voucher_ids {
                if !rng.gen_bool(0.7) {
                    continue;
                }
                let v = fees::get_voucher(&mut tx, *vid).await?;
                let balance = v.balance();
                if balance <= rust_decimal::Decimal::ZERO {
                    continue;
                }
                let paid_on = (month + chrono::Duration::days(rng.gen_range(1..=14))).min(last);
                fees::record_payment(
                    &mut tx,
                    hlc,
                    actor,
                    &policy,
                    fees::NewPayment {
                        voucher_id: *vid,
                        amount: balance,
                        method: PaymentMethod::Cash,
                        reference: None,
                        note: None,
                        paid_on,
                        received_by: principal_user,
                    },
                )
                .await?;
                receipts += 1;
            }
            month = madrasa_core::fees::add_months(month, 1);
        }
    }

    // Attendance: Sunday off, two holidays, every section marked on each school day of April
    // and May (≈4% absent, 2% late, 1% leave), and staff attendance for the same days.
    let april1 = chrono::NaiveDate::from_ymd_opt(demo.academic_year_start, 4, 1).expect("date");
    let may31 = if options.full_year {
        let today = Utc::now().date_naive();
        let year_end =
            chrono::NaiveDate::from_ymd_opt(demo.academic_year_start + 1, 3, 31).expect("date");
        today.min(year_end).max(april1)
    } else {
        chrono::NaiveDate::from_ymd_opt(demo.academic_year_start, 5, 31).expect("date")
    };
    attendance::create_holiday(
        &mut tx,
        hlc,
        actor,
        school.id,
        "Labour Day",
        chrono::NaiveDate::from_ymd_opt(demo.academic_year_start, 5, 1).expect("date"),
        chrono::NaiveDate::from_ymd_opt(demo.academic_year_start, 5, 1).expect("date"),
    )
    .await?;
    attendance::create_holiday(
        &mut tx,
        hlc,
        actor,
        school.id,
        "Eid ul Adha",
        chrono::NaiveDate::from_ymd_opt(demo.academic_year_start, 5, 27).expect("date"),
        chrono::NaiveDate::from_ymd_opt(demo.academic_year_start, 5, 30).expect("date"),
    )
    .await?;
    let (weekend, holidays) = attendance::calendar(&mut tx, school.id).await?;
    let school_days = madrasa_core::attendance::school_days(april1, may31, &weekend, &holidays);
    let mut marker_by_section: HashMap<uuid::Uuid, uuid::Uuid> = HashMap::new();
    for class in &demo.classes {
        if let (Some(section_id), Some(username)) =
            (section_ids.get(&class.name), class.class_teacher.as_ref())
        {
            if let Some(u) = users::find_by_username(&mut tx, school.id, username).await? {
                marker_by_section.insert(*section_id, u.id);
            }
        }
    }
    let mut students_by_section: HashMap<uuid::Uuid, Vec<uuid::Uuid>> = HashMap::new();
    for s in &demo.students {
        if let (Some(section_id), Some(student_id)) =
            (section_ids.get(&s.class), student_ids.get(&s.admission_no))
        {
            students_by_section
                .entry(*section_id)
                .or_default()
                .push(*student_id);
        }
    }
    let mut attendance_sheets = 0usize;
    let mut att_rng = StdRng::seed_from_u64(91);
    for day in &school_days {
        for (section_id, members) in &students_by_section {
            let marked_by = marker_by_section
                .get(section_id)
                .copied()
                .unwrap_or(principal_user);
            let mut entries = Vec::new();
            for student_id in members {
                let roll: f64 = att_rng.gen();
                let status = if roll < 0.04 {
                    AttendanceStatus::Absent
                } else if roll < 0.06 {
                    AttendanceStatus::Late
                } else if roll < 0.07 {
                    AttendanceStatus::Leave
                } else {
                    continue;
                };
                entries.push(attendance::EntryInput {
                    student_id: *student_id,
                    status,
                    note: if status == AttendanceStatus::Leave {
                        Some("Family event".into())
                    } else {
                        None
                    },
                });
            }
            attendance::save_sheet(
                &mut tx,
                hlc,
                Actor::user(marked_by, school.id),
                attendance::SaveSheet {
                    school_id: school.id,
                    section_id: *section_id,
                    date: *day,
                    marked_by,
                    entries,
                },
                false,
            )
            .await?;
            attendance_sheets += 1;
        }
    }
    let staff_rows = staff::list(&mut tx, school.id, None).await?;
    let mut staff_attendance_days = 0usize;
    for day in &school_days {
        let mut entries = Vec::new();
        for member in &staff_rows {
            let roll: f64 = att_rng.gen();
            if roll < 0.02 {
                entries.push(attendance::StaffEntryInput {
                    staff_id: member.id,
                    status: StaffStatus::Absent,
                    note: None,
                });
            } else if roll < 0.04 {
                entries.push(attendance::StaffEntryInput {
                    staff_id: member.id,
                    status: StaffStatus::Leave,
                    note: Some("Leave".into()),
                });
            }
        }
        attendance::save_staff_day(
            &mut tx,
            hlc,
            Actor::user(principal_user, school.id),
            school.id,
            *day,
            principal_user,
            &entries,
        )
        .await?;
        staff_attendance_days += 1;
    }

    // Exams: default assessments, maximum marks for every class and subject, Test 1 and
    // Mid-year marks for every section (≈2% absent), all submitted and approved.
    let assessments =
        exams::create_default_assessments(&mut tx, hlc, actor, school.id, year.id).await?;
    let maxima: Vec<(uuid::Uuid, Decimal)> = assessments
        .iter()
        .zip([dec!(20), dec!(20), dec!(50), dec!(100)])
        .map(|(a, m)| (a.id, m))
        .collect();
    let mut subjects_by_class: HashMap<uuid::Uuid, Vec<uuid::Uuid>> = HashMap::new();
    for class_id in class_ids.values() {
        let subjects = academics::class_subject_ids(&mut tx, *class_id).await?;
        for (assessment_id, max) in &maxima {
            for subject_id in &subjects {
                exams::set_max_marks(
                    &mut tx,
                    hlc,
                    actor,
                    school.id,
                    *assessment_id,
                    *class_id,
                    *subject_id,
                    *max,
                )
                .await?;
            }
        }
        subjects_by_class.insert(*class_id, subjects);
    }
    let mut mark_sheets = 0usize;
    let mut exam_rng = StdRng::seed_from_u64(123);
    let section_class: HashMap<uuid::Uuid, uuid::Uuid> = demo
        .classes
        .iter()
        .filter_map(|c| {
            section_ids
                .get(&c.name)
                .map(|sid| (*sid, class_ids[&c.grade]))
        })
        .collect();
    // Subject allocation (M2 stage 6): in grades 9–12 students alternate between the two
    // groups by roll order; one student per section is left unallocated (takes everything).
    let mut allocated_students = 0usize;
    let mut own_subjects: HashMap<uuid::Uuid, Vec<uuid::Uuid>> = HashMap::new();
    for (section_id, members) in &students_by_section {
        let Some(class_id) = section_class.get(section_id) else {
            continue;
        };
        let groups = groups_by_class.get(class_id).copied();
        for (i, student_id) in members.iter().enumerate() {
            if let (Some((pre_med, computer)), true) = (groups, i + 1 < members.len()) {
                let enrol = students::enrollments(&mut tx, *student_id)
                    .await?
                    .into_iter()
                    .find(|e| e.academic_year_id == year.id)
                    .ok_or_else(|| anyhow::anyhow!("student without enrolment"))?;
                let group_id = if i % 2 == 0 { pre_med } else { computer };
                let group = madrasa_db::subjects::get_group(&mut tx, group_id).await?;
                madrasa_db::subjects::set_allocation(
                    &mut tx,
                    hlc,
                    actor,
                    enrol.id,
                    &madrasa_db::subjects::AllocationValue {
                        group_id: Some(group_id),
                        subject_ids: group.subject_ids,
                    },
                )
                .await?;
                allocated_students += 1;
            }
        }
        own_subjects
            .extend(madrasa_db::subjects::effective_for_section(&mut tx, *section_id).await?);
    }
    // An assessment is held once its usual date has passed: Test 1 in mid-June, Test 2 in
    // mid-September, the Mid-year in mid-October and the Final in mid-March, relative to the
    // install date (v0.2.0-alpha B7). The marks date is the day the sheet was approved.
    let today = Utc::now().date_naive();
    let held_on = [75i64, 165, 200, 345].map(|days| start + chrono::Duration::days(days));
    for (position, (assessment_id, max)) in maxima.iter().enumerate() {
        if held_on[position] > today {
            continue;
        }
        for (section_id, members) in &students_by_section {
            let Some(class_id) = section_class.get(section_id) else {
                continue;
            };
            let marker = marker_by_section
                .get(section_id)
                .copied()
                .unwrap_or(principal_user);
            for subject_id in &subjects_by_class[class_id] {
                let marks: Vec<exams::MarkInput> = members
                    .iter()
                    .filter(|student_id| {
                        own_subjects
                            .get(*student_id)
                            .is_none_or(|own| own.contains(subject_id))
                    })
                    .map(|student_id| {
                        let roll: f64 = exam_rng.gen();
                        if roll < 0.02 {
                            exams::MarkInput {
                                student_id: *student_id,
                                obtained: None,
                                is_absent: true,
                            }
                        } else {
                            let pct: f64 = 35.0 + exam_rng.gen::<f64>() * 65.0;
                            let raw =
                                Decimal::from_f64_retain(pct * max.to_f64().unwrap_or(0.0) / 100.0)
                                    .unwrap_or(Decimal::ZERO);
                            exams::MarkInput {
                                student_id: *student_id,
                                obtained: Some(madrasa_core::exams::round2(raw).min(*max)),
                                is_absent: false,
                            }
                        }
                    })
                    .collect();
                let (sheet, _) = exams::save_marks(
                    &mut tx,
                    hlc,
                    Actor::user(marker, school.id),
                    school.id,
                    *assessment_id,
                    *section_id,
                    *subject_id,
                    &marks,
                )
                .await?;
                exams::submit_sheet(
                    &mut tx,
                    hlc,
                    Actor::user(marker, school.id),
                    sheet.id,
                    marker,
                )
                .await?;
                exams::approve_sheet(
                    &mut tx,
                    hlc,
                    Actor::user(principal_user, school.id),
                    sheet.id,
                    principal_user,
                )
                .await?;
                mark_sheets += 1;
            }
        }
    }

    // Consents (M2 stage 8): three school-defined types; most students have given them, a
    // few refused, some are still missing.
    let mut consent_types = Vec::new();
    for (name, description, given_by, renewal, position) in [
        (
            "Photos",
            "Use of the student's photo in school publications and notice boards",
            madrasa_core::consents::GivenBy::Guardian,
            madrasa_core::consents::Renewal::None,
            0,
        ),
        (
            "Trips",
            "Participation in school trips and outings this year",
            madrasa_core::consents::GivenBy::Guardian,
            madrasa_core::consents::Renewal::Yearly,
            1,
        ),
        (
            "Data use",
            "Sharing of contact details with the parents' committee",
            madrasa_core::consents::GivenBy::Either,
            madrasa_core::consents::Renewal::None,
            2,
        ),
    ] {
        let row = madrasa_db::consents::create_type(
            &mut tx,
            hlc,
            actor,
            school.id,
            &madrasa_db::consents::ConsentTypeInput {
                name: name.into(),
                description: description.into(),
                given_by,
                required: madrasa_core::consents::Required::All,
                min_level: None,
                max_level: None,
                renewal,
                position,
            },
        )
        .await?;
        consent_types.push(row.id);
    }
    let mut consent_rng = StdRng::seed_from_u64(2026);
    let mut consents_recorded = 0usize;
    for student_id in student_ids.values() {
        let guardian = guardians::for_student(&mut tx, *student_id)
            .await?
            .into_iter()
            .next()
            .map(|g| g.id);
        for type_id in &consent_types {
            let roll: f64 = consent_rng.gen();
            let status = if roll < 0.82 {
                madrasa_core::consents::Status::Given
            } else if roll < 0.88 {
                madrasa_core::consents::Status::Refused
            } else {
                continue;
            };
            madrasa_db::consents::record(
                &mut tx,
                hlc,
                actor,
                school.id,
                &madrasa_db::consents::NewConsent {
                    student_id: *student_id,
                    consent_type_id: *type_id,
                    status,
                    guardian_id: guardian,
                    academic_year_id: Some(year.id),
                    recorded_on: start
                        + chrono::Duration::days(i64::from(consent_rng.gen_range(3..20))),
                    note: String::new(),
                },
            )
            .await?;
            consents_recorded += 1;
        }
    }

    // A saved custom report (M2 stage 3), shared with everyone who can run reports.
    madrasa_db::report_definitions::create(
        &mut tx,
        hlc,
        Actor::user(principal_user, school.id),
        madrasa_db::report_definitions::NewDefinition {
            school_id: school.id,
            name: "Girls by section".into(),
            description: "Female students with their guardian's phone, grouped by section".into(),
            source: "students".into(),
            definition: json!({
                "source": "students",
                "columns": ["student_number", "full_name", "father_name", "roll_no", "guardian_phone"],
                "filters": [{ "field": "gender", "op": "eq", "value": "female" }],
                "sort": [{ "field": "full_name", "desc": false }],
                "group_by": "section"
            }),
            owner_user_id: principal_user,
            is_shared: true,
        },
    )
    .await?;

    // Board exams: BISE Karachi, SSC Part I for grade 9 and Part II for grade 10, every
    // student registered with a roll number and a full set of results.
    let mut board_candidates = 0usize;
    {
        let bands = json!([
            { "grade": "A+", "min_percent": 80 }, { "grade": "A", "min_percent": 70 },
            { "grade": "B", "min_percent": 60 }, { "grade": "C", "min_percent": 50 },
            { "grade": "D", "min_percent": 40 }, { "grade": "E", "min_percent": 33 }
        ]);
        let board = boards::create_board(
            &mut tx,
            hlc,
            actor,
            school.id,
            boards::BoardInput {
                name: "BISE Karachi".into(),
                short_code: "BISEK".into(),
                country: "PK".into(),
                grading_mode: "marks".into(),
                pass_percent: Decimal::from(33),
                grade_bands: bands.clone(),
            },
        )
        .await?;
        let subject_names: HashMap<uuid::Uuid, &str> = demo
            .subjects
            .iter()
            .filter_map(|s| subject_ids.get(&s.name).map(|id| (*id, s.name.as_str())))
            .collect();
        let core_bands: Vec<madrasa_core::boards::BoardBand> =
            serde_json::from_value(bands).unwrap_or_default();
        for (grade, part) in [(9u8, "I"), (10u8, "II")] {
            let Some(class_id) = class_ids.get(&grade).copied() else {
                continue;
            };
            let exam = boards::create_exam(
                &mut tx,
                hlc,
                actor,
                school.id,
                board.id,
                year.id,
                &format!("SSC Part {part} Annual {}", demo.academic_year_start + 1),
            )
            .await?;
            boards::set_exam_classes(&mut tx, hlc, actor, &exam, &[class_id]).await?;
            let inputs: Vec<boards::SubjectInput> = subjects_by_class
                .get(&class_id)
                .map(|ids| {
                    ids.iter()
                        .filter_map(|id| subject_names.get(id).map(|n| (*id, *n)))
                        .map(|(id, name)| boards::SubjectInput {
                            name: name.to_string(),
                            subject_id: Some(id),
                            max_marks: Decimal::from(if name == "Islamiat" { 50 } else { 75 }),
                        })
                        .collect()
                })
                .unwrap_or_default();
            let subjects = boards::set_exam_subjects(&mut tx, hlc, actor, &exam, &inputs).await?;
            let mut members: Vec<uuid::Uuid> = section_class
                .iter()
                .filter(|(_, c)| **c == class_id)
                .filter_map(|(sid, _)| students_by_section.get(sid))
                .flatten()
                .copied()
                .collect();
            members.sort();
            boards::register(&mut tx, hlc, actor, &exam, &members, Some("Science")).await?;
            let candidates = boards::candidates(&mut tx, exam.id).await?;
            for (i, cand) in candidates.iter().enumerate() {
                let roll = format!("{}{:04}", if grade == 9 { "12" } else { "13" }, i + 1);
                boards::update_candidate(
                    &mut tx,
                    hlc,
                    actor,
                    cand.id,
                    Some(&roll),
                    None,
                    Some("Science"),
                )
                .await?;
                // Deterministic, spread between 28% and 96%.
                let seed = (i as u32 * 37 + grade as u32 * 11) % 69;
                for (k, subject) in subjects.iter().enumerate() {
                    let pct = 28 + ((seed + k as u32 * 13) % 69);
                    let obtained =
                        (subject.max_marks * Decimal::from(pct) / Decimal::from(100)).round_dp(0);
                    let outcome = madrasa_core::boards::subject_outcome(
                        Some(obtained),
                        subject.max_marks,
                        None,
                        None,
                        board.pass_percent,
                        &core_bands,
                    );
                    boards::upsert_result(
                        &mut tx,
                        hlc,
                        actor,
                        boards::ResultInput {
                            candidate_id: cand.id,
                            board_subject_id: subject.id,
                            obtained: outcome.obtained,
                            max_marks: subject.max_marks,
                            grade: outcome.grade,
                            status: outcome.status.as_str().into(),
                            remarks: None,
                            batch_id: None,
                        },
                    )
                    .await?;
                }
                board_candidates += 1;
            }
            boards::update_exam(&mut tx, hlc, actor, exam.id, &exam.name, "results_imported")
                .await?;
        }
    }

    settings::set_edition(&mut tx, hlc, Actor::SYSTEM, Edition::Lite).await?;
    settings::set(
        &mut tx,
        hlc,
        actor,
        Some(school.id),
        settings::KEY_ACADEMIC_YEAR_START_MONTH,
        json!(4),
    )
    .await?;
    settings::set(
        &mut tx,
        hlc,
        actor,
        Some(school.id),
        settings::KEY_CURRENCY,
        json!("PKR"),
    )
    .await?;
    settings::set(
        &mut tx,
        hlc,
        actor,
        None,
        settings::KEY_SETUP_COMPLETED_AT,
        json!(Utc::now()),
    )
    .await?;
    tx.commit().await.map_err(madrasa_db::DbError::from)?;

    let mut files_written = Vec::new();
    if let Some(dir) = &options.out_dir {
        files_written = write_json(dir, demo)?;
    }

    Ok(LoadReport {
        school_id: school.id,
        staff_accounts: demo.staff.len(),
        subjects: demo.subjects.len(),
        classes: 13,
        sections: demo.classes.len(),
        students: demo.students.len(),
        guardians: demo.guardians.len(),
        vouchers,
        receipts,
        attendance_sheets,
        staff_attendance_days,
        mark_sheets,
        board_candidates,
        allocated_students,
        consents_recorded,
        files_written,
    })
}

/// Write the generated data as JSON (useful for import tests and inspection).
pub fn write_json(dir: &Path, demo: &DemoSchool) -> anyhow::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let mut written = Vec::new();
    for (name, value) in [
        ("school.json", serde_json::to_value(&demo.school)?),
        ("staff.json", serde_json::to_value(&demo.staff)?),
        ("classes.json", serde_json::to_value(&demo.classes)?),
        ("students.json", serde_json::to_value(&demo.students)?),
        ("guardians.json", serde_json::to_value(&demo.guardians)?),
        ("subjects.json", serde_json::to_value(&demo.subjects)?),
    ] {
        let path = dir.join(name);
        std::fs::write(&path, serde_json::to_string_pretty(&value)?)?;
        written.push(path);
    }
    Ok(written)
}
