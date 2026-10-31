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
    academics, attendance, exams, fees, guardians, roles, schools, settings, staff, students,
    users, Actor, PgPool,
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
    // Test 1 and Mid-year are held (positions 0 and 2); Test 2 and the Final are not yet.
    for (position, (assessment_id, max)) in maxima.iter().enumerate() {
        if position != 0 && position != 2 {
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
