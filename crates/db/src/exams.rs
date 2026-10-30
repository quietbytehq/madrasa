//! Exams: assessment types with weights, grading scheme, maximum marks, marks sheets with the
//! draft → submitted → approved workflow, and the raw rows the API turns into results.
//! Every change to marks or a sheet's state is audited.

use crate::audit::{self, snapshot, Action};
use crate::models::{
    AssessmentMaxMark, AssessmentType, Mark, MarkRow, MarkSheet, RosterRow, SheetSummaryRow,
};
use crate::{academics, attendance, settings, Actor, DbError, DbResult};
use chrono::NaiveDate;
use madrasa_core::exams::{self, GradingScheme, SheetStatus};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use rust_decimal::Decimal;
use serde_json::json;
use sqlx::PgConnection;
use std::collections::HashMap;
use uuid::Uuid;

pub const KEY_GRADING: &str = "exams.grading";

// ---- assessment types ---------------------------------------------------------------------

pub async fn list_assessments(
    conn: &mut PgConnection,
    academic_year_id: Uuid,
) -> DbResult<Vec<AssessmentType>> {
    Ok(sqlx::query_as::<_, AssessmentType>(
        "SELECT * FROM assessment_types WHERE academic_year_id = $1 AND deleted_at IS NULL
         ORDER BY position, name",
    )
    .bind(academic_year_id)
    .fetch_all(conn)
    .await?)
}

pub async fn get_assessment(conn: &mut PgConnection, id: Uuid) -> DbResult<AssessmentType> {
    sqlx::query_as::<_, AssessmentType>(
        "SELECT * FROM assessment_types WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("assessment".into()))
}

pub async fn create_assessment(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    academic_year_id: Uuid,
    name: &str,
    weight: Decimal,
    position: i32,
) -> DbResult<AssessmentType> {
    let name = name.trim();
    if name.is_empty() {
        return Err(DbError::Conflict("assessment name is required".into()));
    }
    if weight < Decimal::ZERO || weight > Decimal::ONE_HUNDRED {
        return Err(DbError::Conflict("weight must be between 0 and 100".into()));
    }
    let row = sqlx::query_as::<_, AssessmentType>(
        "INSERT INTO assessment_types (id, school_id, academic_year_id, name, position, weight, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING *",
    )
    .bind(new_id())
    .bind(school_id)
    .bind(academic_year_id)
    .bind(name)
    .bind(position)
    .bind(weight)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| crate::map_unique(e, "An assessment with this name already exists"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "assessment_type",
        Some(row.id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

/// Create the default four assessments for a year that has none yet. Returns the list.
pub async fn create_default_assessments(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    academic_year_id: Uuid,
) -> DbResult<Vec<AssessmentType>> {
    let existing = list_assessments(&mut *conn, academic_year_id).await?;
    if !existing.is_empty() {
        return Ok(existing);
    }
    for (i, (name, weight)) in exams::DEFAULT_ASSESSMENTS.iter().enumerate() {
        create_assessment(
            &mut *conn,
            hlc,
            actor,
            school_id,
            academic_year_id,
            name,
            Decimal::from(*weight),
            i as i32,
        )
        .await?;
    }
    list_assessments(conn, academic_year_id).await
}

pub async fn rename_assessment(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    name: &str,
    position: i32,
) -> DbResult<AssessmentType> {
    let old = get_assessment(&mut *conn, id).await?;
    let name = name.trim();
    if name.is_empty() {
        return Err(DbError::Conflict("assessment name is required".into()));
    }
    let row = sqlx::query_as::<_, AssessmentType>(
        "UPDATE assessment_types SET name = $2, position = $3, updated_at = now(), updated_by = $4, hlc = $5
         WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(name)
    .bind(position)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| crate::map_unique(e, "An assessment with this name already exists"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "assessment_type",
        Some(id),
        Some(snapshot(&old)),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

/// Set the weights of every assessment of a year at once; they must total exactly 100.
pub async fn set_weights(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    academic_year_id: Uuid,
    weights: &[(Uuid, Decimal)],
) -> DbResult<Vec<AssessmentType>> {
    let current = list_assessments(&mut *conn, academic_year_id).await?;
    if current.len() != weights.len()
        || !current
            .iter()
            .all(|a| weights.iter().any(|(id, _)| *id == a.id))
    {
        return Err(DbError::Conflict(
            "weights must be given for every assessment of the year".into(),
        ));
    }
    let values: Vec<Decimal> = weights.iter().map(|(_, w)| *w).collect();
    exams::validate_weights(&values).map_err(|e| DbError::Conflict(e.to_string()))?;
    for (id, weight) in weights {
        let old = current.iter().find(|a| a.id == *id).unwrap();
        if old.weight == *weight {
            continue;
        }
        sqlx::query(
            "UPDATE assessment_types SET weight = $2, updated_at = now(), updated_by = $3, hlc = $4 WHERE id = $1",
        )
        .bind(id)
        .bind(weight)
        .bind(actor.user_id)
        .bind(hlc.now().to_string())
        .execute(&mut *conn)
        .await?;
        audit::record(
            &mut *conn,
            hlc,
            actor,
            Action::Update,
            "assessment_type",
            Some(*id),
            Some(json!({ "weight": old.weight })),
            Some(json!({ "weight": weight })),
        )
        .await?;
    }
    list_assessments(conn, academic_year_id).await
}

/// Soft-delete an assessment that has no marks sheets yet.
pub async fn delete_assessment(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let old = get_assessment(&mut *conn, id).await?;
    let sheets: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM mark_sheets WHERE assessment_type_id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    if sheets > 0 {
        return Err(DbError::Conflict(
            "This assessment already has marks and cannot be removed".into(),
        ));
    }
    sqlx::query(
        "UPDATE assessment_types SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1",
    )
    .bind(id)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE assessment_max_marks SET deleted_at = now(), hlc = $2 WHERE assessment_type_id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Delete,
        "assessment_type",
        Some(id),
        Some(snapshot(&old)),
        None,
    )
    .await?;
    Ok(())
}

// ---- grading ------------------------------------------------------------------------------

pub async fn grading(conn: &mut PgConnection, school_id: Uuid) -> DbResult<GradingScheme> {
    Ok(settings::get(conn, Some(school_id), KEY_GRADING)
        .await?
        .and_then(|v| serde_json::from_value::<GradingScheme>(v).ok())
        .and_then(|g| g.normalized().ok())
        .unwrap_or_default())
}

pub async fn set_grading(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    scheme: GradingScheme,
) -> DbResult<GradingScheme> {
    let scheme = scheme
        .normalized()
        .map_err(|e| DbError::Conflict(e.to_string()))?;
    let value =
        serde_json::to_value(&scheme).map_err(|_| DbError::Conflict("invalid scheme".into()))?;
    settings::set(conn, hlc, actor, Some(school_id), KEY_GRADING, value).await?;
    Ok(scheme)
}

// ---- maximum marks ------------------------------------------------------------------------

pub async fn max_marks(
    conn: &mut PgConnection,
    assessment_type_id: Uuid,
    class_id: Uuid,
) -> DbResult<Vec<AssessmentMaxMark>> {
    Ok(sqlx::query_as::<_, AssessmentMaxMark>(
        "SELECT id, assessment_type_id, class_id, subject_id, max_marks FROM assessment_max_marks
         WHERE assessment_type_id = $1 AND class_id = $2 AND deleted_at IS NULL",
    )
    .bind(assessment_type_id)
    .bind(class_id)
    .fetch_all(conn)
    .await?)
}

/// Upsert the maximum of one subject; zero removes it. Refused once marks exist for it in
/// any section of the class.
pub async fn set_max_marks(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    assessment_type_id: Uuid,
    class_id: Uuid,
    subject_id: Uuid,
    max: Decimal,
) -> DbResult<()> {
    if max < Decimal::ZERO || exams::round2(max) != max {
        return Err(DbError::Conflict(
            "maximum marks must be a positive number with at most two decimals".into(),
        ));
    }
    let existing = sqlx::query_as::<_, AssessmentMaxMark>(
        "SELECT id, assessment_type_id, class_id, subject_id, max_marks FROM assessment_max_marks
         WHERE assessment_type_id = $1 AND class_id = $2 AND subject_id = $3 AND deleted_at IS NULL",
    )
    .bind(assessment_type_id)
    .bind(class_id)
    .bind(subject_id)
    .fetch_optional(&mut *conn)
    .await?;
    if existing.as_ref().map(|e| e.max_marks) == Some(max) {
        return Ok(());
    }
    let marks_exist: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM marks m
         JOIN mark_sheets sh ON sh.id = m.sheet_id AND sh.deleted_at IS NULL
         JOIN sections sec ON sec.id = sh.section_id
         WHERE sh.assessment_type_id = $1 AND sec.class_id = $2 AND sh.subject_id = $3
           AND m.deleted_at IS NULL AND (m.obtained IS NOT NULL OR m.is_absent)",
    )
    .bind(assessment_type_id)
    .bind(class_id)
    .bind(subject_id)
    .fetch_one(&mut *conn)
    .await?;
    if marks_exist > 0 {
        return Err(DbError::Conflict(
            "Marks have already been entered for this subject; the maximum is locked".into(),
        ));
    }
    let now = hlc.now().to_string();
    match (existing, max.is_zero()) {
        (Some(e), true) => {
            sqlx::query(
                "UPDATE assessment_max_marks SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1",
            )
            .bind(e.id)
            .bind(actor.user_id)
            .bind(&now)
            .execute(&mut *conn)
            .await?;
            audit::record(
                conn,
                hlc,
                actor,
                Action::Delete,
                "assessment_max_marks",
                Some(e.id),
                Some(snapshot(&e)),
                None,
            )
            .await?;
        }
        (None, true) => {}
        (Some(e), false) => {
            let row = sqlx::query_as::<_, AssessmentMaxMark>(
                "UPDATE assessment_max_marks SET max_marks = $2, updated_at = now(), updated_by = $3, hlc = $4
                 WHERE id = $1 RETURNING id, assessment_type_id, class_id, subject_id, max_marks",
            )
            .bind(e.id)
            .bind(max)
            .bind(actor.user_id)
            .bind(&now)
            .fetch_one(&mut *conn)
            .await?;
            audit::record(
                conn,
                hlc,
                actor,
                Action::Update,
                "assessment_max_marks",
                Some(e.id),
                Some(snapshot(&e)),
                Some(snapshot(&row)),
            )
            .await?;
        }
        (None, false) => {
            let row = sqlx::query_as::<_, AssessmentMaxMark>(
                "INSERT INTO assessment_max_marks (id, school_id, assessment_type_id, class_id, subject_id, max_marks, updated_by, hlc)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id, assessment_type_id, class_id, subject_id, max_marks",
            )
            .bind(new_id())
            .bind(school_id)
            .bind(assessment_type_id)
            .bind(class_id)
            .bind(subject_id)
            .bind(max)
            .bind(actor.user_id)
            .bind(&now)
            .fetch_one(&mut *conn)
            .await?;
            audit::record(
                conn,
                hlc,
                actor,
                Action::Create,
                "assessment_max_marks",
                Some(row.id),
                None,
                Some(snapshot(&row)),
            )
            .await?;
        }
    }
    Ok(())
}

// ---- sheets -------------------------------------------------------------------------------

/// Subjects of the section's class in this assessment with their maximum and sheet state.
pub async fn sheet_summaries(
    conn: &mut PgConnection,
    assessment_type_id: Uuid,
    section_id: Uuid,
) -> DbResult<Vec<SheetSummaryRow>> {
    Ok(sqlx::query_as::<_, SheetSummaryRow>(
        "SELECT s.id AS subject_id, s.name AS subject_name, s.code AS subject_code,
                mm.max_marks, sh.id AS sheet_id, sh.status, sh.submitted_at, sh.approved_at,
                (SELECT count(*) FROM marks m WHERE m.sheet_id = sh.id AND m.deleted_at IS NULL
                   AND (m.obtained IS NOT NULL OR m.is_absent)) AS entered
         FROM sections sec
         JOIN class_subjects cs ON cs.class_id = sec.class_id AND cs.deleted_at IS NULL
         JOIN subjects s ON s.id = cs.subject_id AND s.deleted_at IS NULL
         LEFT JOIN assessment_max_marks mm ON mm.assessment_type_id = $1 AND mm.class_id = sec.class_id
              AND mm.subject_id = s.id AND mm.deleted_at IS NULL
         LEFT JOIN mark_sheets sh ON sh.assessment_type_id = $1 AND sh.section_id = sec.id
              AND sh.subject_id = s.id AND sh.deleted_at IS NULL
         WHERE sec.id = $2
         ORDER BY s.name",
    )
    .bind(assessment_type_id)
    .bind(section_id)
    .fetch_all(conn)
    .await?)
}

pub async fn sheet(
    conn: &mut PgConnection,
    assessment_type_id: Uuid,
    section_id: Uuid,
    subject_id: Uuid,
) -> DbResult<Option<MarkSheet>> {
    Ok(sqlx::query_as::<_, MarkSheet>(
        "SELECT id, school_id, assessment_type_id, section_id, subject_id, status, submitted_by, submitted_at,
                approved_by, approved_at, updated_at, updated_by, hlc
         FROM mark_sheets WHERE assessment_type_id = $1 AND section_id = $2 AND subject_id = $3 AND deleted_at IS NULL",
    )
    .bind(assessment_type_id)
    .bind(section_id)
    .bind(subject_id)
    .fetch_optional(conn)
    .await?)
}

pub async fn get_sheet(conn: &mut PgConnection, id: Uuid) -> DbResult<MarkSheet> {
    sqlx::query_as::<_, MarkSheet>(
        "SELECT id, school_id, assessment_type_id, section_id, subject_id, status, submitted_by, submitted_at,
                approved_by, approved_at, updated_at, updated_by, hlc
         FROM mark_sheets WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("marks sheet".into()))
}

pub async fn marks(conn: &mut PgConnection, sheet_id: Uuid) -> DbResult<Vec<Mark>> {
    Ok(sqlx::query_as::<_, Mark>(
        "SELECT id, sheet_id, student_id, obtained, is_absent, updated_at, updated_by
         FROM marks WHERE sheet_id = $1 AND deleted_at IS NULL",
    )
    .bind(sheet_id)
    .fetch_all(conn)
    .await?)
}

/// Active students of the section (reuses the attendance roster query with a date that has
/// no sheet, so the status columns come back empty).
pub async fn roster(conn: &mut PgConnection, section_id: Uuid) -> DbResult<Vec<RosterRow>> {
    let never = NaiveDate::from_ymd_opt(1900, 1, 1).unwrap_or(NaiveDate::MIN);
    attendance::roster(conn, section_id, never).await
}

/// Maximum marks of one subject for the section's class, or a conflict when not set.
pub async fn max_for(
    conn: &mut PgConnection,
    assessment_type_id: Uuid,
    class_id: Uuid,
    subject_id: Uuid,
) -> DbResult<Decimal> {
    max_marks(conn, assessment_type_id, class_id)
        .await?
        .into_iter()
        .find(|m| m.subject_id == subject_id)
        .map(|m| m.max_marks)
        .ok_or_else(|| {
            DbError::Conflict("Set the maximum marks for this subject before entering marks".into())
        })
}

#[derive(Debug, Clone)]
pub struct MarkInput {
    pub student_id: Uuid,
    /// `None` with `is_absent = false` clears the mark (not entered yet).
    pub obtained: Option<Decimal>,
    pub is_absent: bool,
}

/// Save marks on a draft sheet, creating the sheet on first save. Validates each mark
/// against the maximum and refuses submitted or approved sheets. Returns the sheet and the
/// number of changed marks.
#[allow(clippy::too_many_arguments)]
pub async fn save_marks(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    assessment_type_id: Uuid,
    section_id: Uuid,
    subject_id: Uuid,
    input: &[MarkInput],
) -> DbResult<(MarkSheet, usize)> {
    let section = academics::get_section(&mut *conn, section_id).await?;
    if section.school_id != school_id {
        return Err(DbError::NotFound("section".into()));
    }
    let assessment = get_assessment(&mut *conn, assessment_type_id).await?;
    if assessment.academic_year_id != section.academic_year_id {
        return Err(DbError::Conflict(
            "The assessment belongs to a different academic year".into(),
        ));
    }
    if !academics::class_subject_ids(&mut *conn, section.class_id)
        .await?
        .contains(&subject_id)
    {
        return Err(DbError::Conflict(
            "This subject is not taught in the section's class".into(),
        ));
    }
    let max = max_for(&mut *conn, assessment_type_id, section.class_id, subject_id).await?;
    let roster = roster(&mut *conn, section_id).await?;
    let mut cleaned: Vec<(Uuid, Option<Decimal>, bool)> = Vec::with_capacity(input.len());
    for m in input {
        if !roster.iter().any(|r| r.student_id == m.student_id) {
            return Err(DbError::Conflict(
                "A student in the sheet is not enrolled in this section".into(),
            ));
        }
        let obtained = match (m.is_absent, m.obtained) {
            (true, _) => None,
            (false, Some(o)) => Some(exams::validate_mark(o, max).map_err(DbError::Conflict)?),
            (false, None) => None,
        };
        cleaned.push((m.student_id, obtained, m.is_absent));
    }
    let now = hlc.now().to_string();
    let sheet = match sheet(&mut *conn, assessment_type_id, section_id, subject_id).await? {
        Some(s) => s,
        None => {
            let s = sqlx::query_as::<_, MarkSheet>(
                "INSERT INTO mark_sheets (id, school_id, assessment_type_id, section_id, subject_id, updated_by, hlc)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 RETURNING id, school_id, assessment_type_id, section_id, subject_id, status, submitted_by, submitted_at,
                           approved_by, approved_at, updated_at, updated_by, hlc",
            )
            .bind(new_id())
            .bind(school_id)
            .bind(assessment_type_id)
            .bind(section_id)
            .bind(subject_id)
            .bind(actor.user_id)
            .bind(&now)
            .fetch_one(&mut *conn)
            .await?;
            audit::record(
                &mut *conn,
                hlc,
                actor,
                Action::Create,
                "mark_sheet",
                Some(s.id),
                None,
                Some(snapshot(&s)),
            )
            .await?;
            s
        }
    };
    if sheet.status != SheetStatus::Draft.as_str() {
        return Err(DbError::Conflict(
            "This sheet is submitted and locked; ask the principal to reopen it".into(),
        ));
    }
    let current: HashMap<Uuid, Mark> = marks(&mut *conn, sheet.id)
        .await?
        .into_iter()
        .map(|m| (m.student_id, m))
        .collect();
    let mut changes = Vec::new();
    for (student_id, obtained, is_absent) in cleaned {
        match current.get(&student_id) {
            Some(cur) if cur.obtained == obtained && cur.is_absent == is_absent => {}
            Some(cur) => {
                sqlx::query(
                    "UPDATE marks SET obtained = $2, is_absent = $3, updated_at = now(), updated_by = $4, hlc = $5 WHERE id = $1",
                )
                .bind(cur.id)
                .bind(obtained)
                .bind(is_absent)
                .bind(actor.user_id)
                .bind(&now)
                .execute(&mut *conn)
                .await?;
                changes.push(json!({ "student_id": student_id, "from": { "obtained": cur.obtained, "absent": cur.is_absent }, "to": { "obtained": obtained, "absent": is_absent } }));
            }
            None => {
                if obtained.is_none() && !is_absent {
                    continue;
                }
                sqlx::query(
                    "INSERT INTO marks (id, school_id, sheet_id, student_id, obtained, is_absent, updated_by, hlc)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                )
                .bind(new_id())
                .bind(school_id)
                .bind(sheet.id)
                .bind(student_id)
                .bind(obtained)
                .bind(is_absent)
                .bind(actor.user_id)
                .bind(&now)
                .execute(&mut *conn)
                .await?;
                changes.push(json!({ "student_id": student_id, "from": null, "to": { "obtained": obtained, "absent": is_absent } }));
            }
        }
    }
    let changed = changes.len();
    if changed > 0 {
        sqlx::query(
            "UPDATE mark_sheets SET updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1",
        )
        .bind(sheet.id)
        .bind(actor.user_id)
        .bind(&now)
        .execute(&mut *conn)
        .await?;
        audit::record(
            conn,
            hlc,
            actor,
            Action::Update,
            "mark_sheet",
            Some(sheet.id),
            None,
            Some(json!({ "marks_changed": changed, "changes": changes })),
        )
        .await?;
    }
    Ok((sheet, changed))
}

async fn set_status(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    sheet: &MarkSheet,
    status: SheetStatus,
    by: Option<Uuid>,
) -> DbResult<MarkSheet> {
    let (sub_by, sub_at, app_by, app_at) = match status {
        SheetStatus::Draft => (None, None, None, None),
        SheetStatus::Submitted => (by, Some(chrono::Utc::now()), None, None),
        SheetStatus::Approved => (
            sheet.submitted_by,
            sheet.submitted_at,
            by,
            Some(chrono::Utc::now()),
        ),
    };
    let row = sqlx::query_as::<_, MarkSheet>(
        "UPDATE mark_sheets SET status = $2, submitted_by = $3, submitted_at = $4, approved_by = $5, approved_at = $6,
            updated_at = now(), updated_by = $7, hlc = $8
         WHERE id = $1
         RETURNING id, school_id, assessment_type_id, section_id, subject_id, status, submitted_by, submitted_at,
                   approved_by, approved_at, updated_at, updated_by, hlc",
    )
    .bind(sheet.id)
    .bind(status.as_str())
    .bind(sub_by)
    .bind(sub_at)
    .bind(app_by)
    .bind(app_at)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "mark_sheet",
        Some(sheet.id),
        Some(json!({ "status": sheet.status })),
        Some(json!({ "status": status.as_str() })),
    )
    .await?;
    Ok(row)
}

/// Teacher submits a complete draft (every enrolled student has a mark or is absent).
pub async fn submit_sheet(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    sheet_id: Uuid,
    by: Uuid,
) -> DbResult<MarkSheet> {
    let sheet = get_sheet(&mut *conn, sheet_id).await?;
    if sheet.status != SheetStatus::Draft.as_str() {
        return Err(DbError::Conflict("This sheet is already submitted".into()));
    }
    let roster = roster(&mut *conn, sheet.section_id).await?;
    let marks = marks(&mut *conn, sheet.id).await?;
    let missing = roster
        .iter()
        .filter(|r| {
            !marks
                .iter()
                .any(|m| m.student_id == r.student_id && (m.obtained.is_some() || m.is_absent))
        })
        .count();
    if missing > 0 {
        return Err(DbError::Conflict(format!(
            "{missing} student(s) have no marks yet; enter marks or mark them absent"
        )));
    }
    set_status(conn, hlc, actor, &sheet, SheetStatus::Submitted, Some(by)).await
}

/// Principal approves a submitted sheet; marks are locked afterwards.
pub async fn approve_sheet(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    sheet_id: Uuid,
    by: Uuid,
) -> DbResult<MarkSheet> {
    let sheet = get_sheet(&mut *conn, sheet_id).await?;
    if sheet.status != SheetStatus::Submitted.as_str() {
        return Err(DbError::Conflict(
            "Only a submitted sheet can be approved".into(),
        ));
    }
    set_status(conn, hlc, actor, &sheet, SheetStatus::Approved, Some(by)).await
}

/// Principal sends a submitted or approved sheet back to draft.
pub async fn reopen_sheet(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    sheet_id: Uuid,
) -> DbResult<MarkSheet> {
    let sheet = get_sheet(&mut *conn, sheet_id).await?;
    if sheet.status == SheetStatus::Draft.as_str() {
        return Err(DbError::Conflict("This sheet is already open".into()));
    }
    set_status(conn, hlc, actor, &sheet, SheetStatus::Draft, None).await
}

// ---- result rows --------------------------------------------------------------------------

/// Every mark of a section in one assessment (subjects with a maximum only).
pub async fn section_marks(
    conn: &mut PgConnection,
    assessment_type_id: Uuid,
    section_id: Uuid,
) -> DbResult<Vec<MarkRow>> {
    Ok(sqlx::query_as::<_, MarkRow>(
        "SELECT sh.assessment_type_id, sh.subject_id, m.student_id, mm.max_marks, m.obtained, m.is_absent,
                sh.status AS sheet_status
         FROM mark_sheets sh
         JOIN sections sec ON sec.id = sh.section_id
         JOIN assessment_max_marks mm ON mm.assessment_type_id = sh.assessment_type_id AND mm.class_id = sec.class_id
              AND mm.subject_id = sh.subject_id AND mm.deleted_at IS NULL
         JOIN marks m ON m.sheet_id = sh.id AND m.deleted_at IS NULL
         WHERE sh.assessment_type_id = $1 AND sh.section_id = $2 AND sh.deleted_at IS NULL",
    )
    .bind(assessment_type_id)
    .bind(section_id)
    .fetch_all(conn)
    .await?)
}

/// Every mark of one student across the assessments of an academic year.
pub async fn student_marks(
    conn: &mut PgConnection,
    student_id: Uuid,
    academic_year_id: Uuid,
) -> DbResult<Vec<MarkRow>> {
    Ok(sqlx::query_as::<_, MarkRow>(
        "SELECT sh.assessment_type_id, sh.subject_id, m.student_id, mm.max_marks, m.obtained, m.is_absent,
                sh.status AS sheet_status
         FROM marks m
         JOIN mark_sheets sh ON sh.id = m.sheet_id AND sh.deleted_at IS NULL
         JOIN assessment_types a ON a.id = sh.assessment_type_id AND a.deleted_at IS NULL
         JOIN sections sec ON sec.id = sh.section_id
         JOIN assessment_max_marks mm ON mm.assessment_type_id = sh.assessment_type_id AND mm.class_id = sec.class_id
              AND mm.subject_id = sh.subject_id AND mm.deleted_at IS NULL
         WHERE m.student_id = $1 AND a.academic_year_id = $2 AND m.deleted_at IS NULL",
    )
    .bind(student_id)
    .bind(academic_year_id)
    .fetch_all(conn)
    .await?)
}
