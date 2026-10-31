//! Academic year rollover: open the next year, promote every active student one class up
//! (or graduate from the top class), keep retained students in their class, carry the
//! class teachers, fee structure and assessment types over, and make the new year active.
//! Unpaid fee balances need no special step: the first voucher of the new year carries them
//! forward (see `fees::outstanding_before`).

use crate::audit::{self, Action};
use crate::models::{AcademicYear, Class, Section};
use crate::{academics, exams, fees, students, Actor, DbError, DbResult};
use chrono::Datelike;
use madrasa_core::hlc::Hlc;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use std::collections::HashMap;
use uuid::Uuid;

/// Where the students of one section go.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionMove {
    pub section_id: Uuid,
    pub class_name: String,
    pub section_name: String,
    pub students: i64,
    /// `None` means the class is the top one: its students graduate.
    pub target_class_id: Option<Uuid>,
    pub target_class_name: Option<String>,
}

/// Mirrored by `RolloverPreview` in the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloverPreview {
    pub old_year: AcademicYear,
    pub new_year_name: String,
    pub new_start_year: i32,
    pub moves: Vec<SectionMove>,
    pub promoted: i64,
    pub graduating: i64,
    pub students_with_balance: i64,
    pub balance_carried: Decimal,
    /// A year with that name already exists.
    pub new_year_exists: bool,
    /// Whole months since the active year started; the UI warns when this is small.
    pub months_into_year: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RolloverOptions {
    /// Students who repeat their class.
    #[serde(default)]
    pub retain: Vec<Uuid>,
    /// Students who leave instead of being promoted.
    #[serde(default)]
    pub graduate: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloverReport {
    pub new_year: AcademicYear,
    pub sections_created: usize,
    pub promoted: usize,
    pub retained: usize,
    pub graduated: usize,
    pub structure_lines_copied: usize,
    pub assessments_created: usize,
    pub balance_carried: Decimal,
}

fn next_class<'a>(classes: &'a [Class], current: &Class) -> Option<&'a Class> {
    classes
        .iter()
        .filter(|c| c.level > current.level)
        .min_by_key(|c| c.level)
}

async fn section_counts(conn: &mut PgConnection, year_id: Uuid) -> DbResult<HashMap<Uuid, i64>> {
    let rows: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT e.section_id, count(*) FROM enrollments e
         JOIN students s ON s.id = e.student_id AND s.deleted_at IS NULL AND s.status = 'active'
         WHERE e.academic_year_id = $1 AND e.deleted_at IS NULL GROUP BY e.section_id",
    )
    .bind(year_id)
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().collect())
}

pub async fn preview(conn: &mut PgConnection, school_id: Uuid) -> DbResult<RolloverPreview> {
    let old_year = academics::active_year(&mut *conn, school_id)
        .await?
        .ok_or_else(|| DbError::Conflict("There is no active academic year to roll over".into()))?;
    let classes = academics::list_classes(&mut *conn, school_id).await?;
    let sections = academics::list_sections(&mut *conn, school_id, Some(old_year.id), None).await?;
    let counts = section_counts(&mut *conn, old_year.id).await?;
    let mut moves = Vec::new();
    let (mut promoted, mut graduating) = (0i64, 0i64);
    for s in &sections {
        let class = classes.iter().find(|c| c.id == s.class_id);
        let Some(class) = class else { continue };
        let target = next_class(&classes, class);
        let n = *counts.get(&s.id).unwrap_or(&0);
        match target {
            Some(_) => promoted += n,
            None => graduating += n,
        }
        moves.push(SectionMove {
            section_id: s.id,
            class_name: class.name.clone(),
            section_name: s.name.clone(),
            students: n,
            target_class_id: target.map(|c| c.id),
            target_class_name: target.map(|c| c.name.clone()),
        });
    }
    let (students_with_balance, balance_carried): (i64, Option<Decimal>) = sqlx::query_as(
        "SELECT count(DISTINCT student_id), sum(total_due + late_fee - net_paid)
         FROM fee_vouchers WHERE school_id = $1 AND deleted_at IS NULL
           AND status IN ('unpaid', 'partial') AND total_due + late_fee - net_paid > 0",
    )
    .bind(school_id)
    .fetch_one(&mut *conn)
    .await?;
    let new_start_year = old_year.start_date.year() + 1;
    let (new_name, _, _) =
        academics::default_year_bounds(new_start_year, old_year.start_month as u32)
            .ok_or_else(|| DbError::Conflict("cannot compute the next year".into()))?;
    let new_year_exists = academics::list_years(&mut *conn, school_id)
        .await?
        .iter()
        .any(|y| y.name == new_name);
    let today = chrono::Utc::now().date_naive();
    let months_into_year = (today.year() - old_year.start_date.year()) * 12 + today.month() as i32
        - old_year.start_date.month() as i32;
    Ok(RolloverPreview {
        months_into_year,
        old_year,
        new_year_name: new_name,
        new_start_year,
        moves,
        promoted,
        graduating,
        students_with_balance,
        balance_carried: balance_carried.unwrap_or_default(),
        new_year_exists,
    })
}

/// Perform the rollover. Call inside a transaction.
pub async fn apply(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    options: &RolloverOptions,
) -> DbResult<RolloverReport> {
    let p = preview(&mut *conn, school_id).await?;
    if p.new_year_exists {
        return Err(DbError::Conflict(format!(
            "The year {} already exists; rollover was already done",
            p.new_year_name
        )));
    }
    let (name, start, end) =
        academics::default_year_bounds(p.new_start_year, p.old_year.start_month as u32)
            .ok_or_else(|| DbError::Conflict("cannot compute the next year".into()))?;
    let new_year = academics::create_year(
        &mut *conn,
        hlc,
        actor,
        academics::NewAcademicYear {
            school_id,
            name,
            start_date: start,
            end_date: end,
            start_month: p.old_year.start_month,
        },
    )
    .await?;
    let classes = academics::list_classes(&mut *conn, school_id).await?;
    let old_sections =
        academics::list_sections(&mut *conn, school_id, Some(p.old_year.id), None).await?;

    // New sections: one per (target class, section name), keeping the class teacher.
    let mut new_sections: HashMap<(Uuid, String), Section> = HashMap::new();
    let mut sections_created = 0usize;
    let (mut promoted, mut retained, mut graduated) = (0usize, 0usize, 0usize);
    for old in &old_sections {
        let Some(class) = classes.iter().find(|c| c.id == old.class_id) else {
            continue;
        };
        let target = next_class(&classes, class);
        let rows: Vec<(Uuid, Option<i32>)> = sqlx::query_as(
            "SELECT e.student_id, e.roll_no FROM enrollments e
             JOIN students s ON s.id = e.student_id AND s.deleted_at IS NULL AND s.status = 'active'
             WHERE e.section_id = $1 AND e.deleted_at IS NULL ORDER BY e.roll_no NULLS LAST",
        )
        .bind(old.id)
        .fetch_all(&mut *conn)
        .await?;
        for (student_id, roll_no) in rows {
            let leaves = options.graduate.contains(&student_id);
            let stays = options.retain.contains(&student_id);
            let dest_class = if leaves {
                None
            } else if stays {
                Some(class)
            } else {
                target
            };
            match dest_class {
                None => {
                    students::set_status(
                        &mut *conn,
                        hlc,
                        actor,
                        student_id,
                        "graduated",
                        Some(p.old_year.end_date),
                        Some("Completed the final class"),
                    )
                    .await?;
                    graduated += 1;
                }
                Some(dest) => {
                    let key = (dest.id, old.name.clone());
                    let section = match new_sections.get(&key) {
                        Some(s) => s.clone(),
                        None => {
                            let s = academics::create_section(
                                &mut *conn,
                                hlc,
                                actor,
                                academics::NewSection {
                                    school_id,
                                    class_id: dest.id,
                                    academic_year_id: new_year.id,
                                    name: old.name.clone(),
                                    class_teacher_id: old.class_teacher_id,
                                    capacity: old.capacity,
                                },
                            )
                            .await?;
                            sections_created += 1;
                            new_sections.insert(key.clone(), s.clone());
                            s
                        }
                    };
                    students::enrol(&mut *conn, hlc, actor, student_id, section.id, roll_no)
                        .await?;
                    if stays {
                        retained += 1;
                    } else {
                        promoted += 1;
                    }
                }
            }
        }
    }
    // Fee structure: same amounts per class next year.
    let mut structure_lines_copied = 0usize;
    for line in fees::list_structure(&mut *conn, p.old_year.id, None).await? {
        fees::set_structure(
            &mut *conn,
            hlc,
            actor,
            school_id,
            new_year.id,
            line.class_id,
            line.fee_head_id,
            line.amount,
        )
        .await?;
        structure_lines_copied += 1;
    }
    // Assessment types with the same names and weights.
    let mut assessments_created = 0usize;
    for a in exams::list_assessments(&mut *conn, p.old_year.id).await? {
        exams::create_assessment(
            &mut *conn,
            hlc,
            actor,
            school_id,
            new_year.id,
            &a.name,
            a.weight,
            a.position,
        )
        .await?;
        assessments_created += 1;
    }
    let new_year = academics::activate_year(&mut *conn, hlc, actor, new_year.id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "academic_year",
        Some(new_year.id),
        Some(serde_json::json!({ "rolled_over_from": p.old_year.name })),
        Some(serde_json::json!({
            "promoted": promoted, "retained": retained, "graduated": graduated,
            "sections_created": sections_created, "balance_carried": p.balance_carried,
        })),
    )
    .await?;
    Ok(RolloverReport {
        new_year,
        sections_created,
        promoted,
        retained,
        graduated,
        structure_lines_copied,
        assessments_created,
        balance_carried: p.balance_carried,
    })
}
