//! Academic years, classes, subjects and sections.

use crate::audit::{self, snapshot, Action};
use crate::models::{AcademicYear, Class, Section, Subject};
use crate::{map_unique, Actor, DbError, DbResult};
use chrono::NaiveDate;
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use sqlx::PgConnection;
use uuid::Uuid;

// ---- academic years ---------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct NewAcademicYear {
    pub school_id: Uuid,
    pub name: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub start_month: i16,
}

/// Default name and dates for a year starting in `start_month` of `year`, e.g. `2026-27`.
pub fn default_year_bounds(year: i32, start_month: u32) -> Option<(String, NaiveDate, NaiveDate)> {
    let start = NaiveDate::from_ymd_opt(year, start_month, 1)?;
    let end = NaiveDate::from_ymd_opt(year + 1, start_month, 1)?.pred_opt()?;
    Some((format!("{year}-{:02}", (year + 1) % 100), start, end))
}

pub async fn create_year(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    input: NewAcademicYear,
) -> DbResult<AcademicYear> {
    let id = new_id();
    let row = sqlx::query_as::<_, AcademicYear>(
        "INSERT INTO academic_years (id, school_id, name, start_date, end_date, start_month, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING *",
    )
    .bind(id)
    .bind(input.school_id)
    .bind(input.name.trim())
    .bind(input.start_date)
    .bind(input.end_date)
    .bind(input.start_month)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "an academic year with this name"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "academic_year",
        Some(id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn list_years(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Vec<AcademicYear>> {
    Ok(sqlx::query_as::<_, AcademicYear>(
        "SELECT * FROM academic_years WHERE school_id = $1 AND deleted_at IS NULL ORDER BY start_date DESC",
    )
    .bind(school_id)
    .fetch_all(conn)
    .await?)
}

pub async fn get_year(conn: &mut PgConnection, id: Uuid) -> DbResult<AcademicYear> {
    sqlx::query_as::<_, AcademicYear>(
        "SELECT * FROM academic_years WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("academic year".into()))
}

pub async fn active_year(
    conn: &mut PgConnection,
    school_id: Uuid,
) -> DbResult<Option<AcademicYear>> {
    Ok(sqlx::query_as::<_, AcademicYear>(
        "SELECT * FROM academic_years WHERE school_id = $1 AND is_active AND deleted_at IS NULL",
    )
    .bind(school_id)
    .fetch_optional(conn)
    .await?)
}

/// Make `id` the active year (deactivating the previous one), audited.
pub async fn activate_year(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<AcademicYear> {
    let target = get_year(&mut *conn, id).await?;
    if let Some(previous) = active_year(&mut *conn, target.school_id).await? {
        if previous.id == id {
            return Ok(previous);
        }
        let after = sqlx::query_as::<_, AcademicYear>(
            "UPDATE academic_years SET is_active = FALSE, updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1 RETURNING *",
        )
        .bind(previous.id)
        .bind(actor.user_id)
        .bind(hlc.now().to_string())
        .fetch_one(&mut *conn)
        .await?;
        audit::record(
            &mut *conn,
            hlc,
            actor,
            Action::Update,
            "academic_year",
            Some(previous.id),
            Some(snapshot(&previous)),
            Some(snapshot(&after)),
        )
        .await?;
    }
    let after = sqlx::query_as::<_, AcademicYear>(
        "UPDATE academic_years SET is_active = TRUE, updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "academic_year",
        Some(id),
        Some(snapshot(&target)),
        Some(snapshot(&after)),
    )
    .await?;
    Ok(after)
}

// ---- classes ------------------------------------------------------------------------------

pub async fn create_class(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    name: &str,
    level: i32,
) -> DbResult<Class> {
    let id = new_id();
    let row = sqlx::query_as::<_, Class>(
        "INSERT INTO classes (id, school_id, name, level, updated_by, hlc) VALUES ($1, $2, $3, $4, $5, $6) RETURNING *",
    )
    .bind(id)
    .bind(school_id)
    .bind(name.trim())
    .bind(level)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a class with this name"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "class",
        Some(id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn update_class(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    name: Option<&str>,
    level: Option<i32>,
) -> DbResult<Class> {
    let before = get_class(&mut *conn, id).await?;
    let after = sqlx::query_as::<_, Class>(
        "UPDATE classes SET name = $2, level = $3, updated_at = now(), updated_by = $4, hlc = $5 WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(name.map(str::trim).unwrap_or(&before.name))
    .bind(level.unwrap_or(before.level))
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a class with this name"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "class",
        Some(id),
        Some(snapshot(&before)),
        Some(snapshot(&after)),
    )
    .await?;
    Ok(after)
}

pub async fn get_class(conn: &mut PgConnection, id: Uuid) -> DbResult<Class> {
    sqlx::query_as::<_, Class>("SELECT * FROM classes WHERE id = $1 AND deleted_at IS NULL")
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| DbError::NotFound("class".into()))
}

pub async fn list_classes(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Vec<Class>> {
    Ok(sqlx::query_as::<_, Class>(
        "SELECT * FROM classes WHERE school_id = $1 AND deleted_at IS NULL ORDER BY level, name",
    )
    .bind(school_id)
    .fetch_all(conn)
    .await?)
}

// ---- subjects -----------------------------------------------------------------------------

pub async fn create_subject(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    name: &str,
    code: &str,
) -> DbResult<Subject> {
    let id = new_id();
    let row = sqlx::query_as::<_, Subject>(
        "INSERT INTO subjects (id, school_id, name, code, updated_by, hlc) VALUES ($1, $2, $3, $4, $5, $6) RETURNING *",
    )
    .bind(id)
    .bind(school_id)
    .bind(name.trim())
    .bind(code.trim().to_uppercase())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a subject with this code"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "subject",
        Some(id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn update_subject(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    name: Option<&str>,
    code: Option<&str>,
) -> DbResult<Subject> {
    let before = get_subject(&mut *conn, id).await?;
    let after = sqlx::query_as::<_, Subject>(
        "UPDATE subjects SET name = $2, code = $3, updated_at = now(), updated_by = $4, hlc = $5 WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(name.map(str::trim).unwrap_or(&before.name))
    .bind(code.map(|c| c.trim().to_uppercase()).unwrap_or_else(|| before.code.clone()))
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a subject with this code"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "subject",
        Some(id),
        Some(snapshot(&before)),
        Some(snapshot(&after)),
    )
    .await?;
    Ok(after)
}

pub async fn get_subject(conn: &mut PgConnection, id: Uuid) -> DbResult<Subject> {
    sqlx::query_as::<_, Subject>("SELECT * FROM subjects WHERE id = $1 AND deleted_at IS NULL")
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| DbError::NotFound("subject".into()))
}

pub async fn list_subjects(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Vec<Subject>> {
    Ok(sqlx::query_as::<_, Subject>(
        "SELECT * FROM subjects WHERE school_id = $1 AND deleted_at IS NULL ORDER BY name",
    )
    .bind(school_id)
    .fetch_all(conn)
    .await?)
}

/// Subject ids taught in a class.
pub async fn class_subject_ids(conn: &mut PgConnection, class_id: Uuid) -> DbResult<Vec<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT cs.subject_id FROM class_subjects cs JOIN subjects s ON s.id = cs.subject_id
         WHERE cs.class_id = $1 AND cs.deleted_at IS NULL AND s.deleted_at IS NULL ORDER BY s.name",
    )
    .bind(class_id)
    .fetch_all(conn)
    .await?)
}

/// Replace the subject set of a class (soft-deleting removed links), audited as one update.
pub async fn set_class_subjects(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    class_id: Uuid,
    subject_ids: &[Uuid],
) -> DbResult<Vec<Uuid>> {
    let class = get_class(&mut *conn, class_id).await?;
    let before = class_subject_ids(&mut *conn, class_id).await?;
    let stamp = hlc.now().to_string();
    sqlx::query(
        "UPDATE class_subjects SET deleted_at = now(), updated_at = now(), updated_by = $3, hlc = $4
         WHERE class_id = $1 AND deleted_at IS NULL AND NOT (subject_id = ANY($2))",
    )
    .bind(class_id)
    .bind(subject_ids)
    .bind(actor.user_id)
    .bind(&stamp)
    .execute(&mut *conn)
    .await?;
    for subject_id in subject_ids {
        sqlx::query(
            "INSERT INTO class_subjects (id, school_id, class_id, subject_id, updated_by, hlc) VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (class_id, subject_id) DO UPDATE SET deleted_at = NULL, updated_at = now(), updated_by = EXCLUDED.updated_by, hlc = EXCLUDED.hlc",
        )
        .bind(new_id())
        .bind(class.school_id)
        .bind(class_id)
        .bind(subject_id)
        .bind(actor.user_id)
        .bind(&stamp)
        .execute(&mut *conn)
        .await?;
    }
    let after = class_subject_ids(&mut *conn, class_id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "class_subjects",
        Some(class_id),
        Some(serde_json::json!({ "subject_ids": before })),
        Some(serde_json::json!({ "subject_ids": after })),
    )
    .await?;
    Ok(after)
}

// ---- sections -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct NewSection {
    pub school_id: Uuid,
    pub class_id: Uuid,
    pub academic_year_id: Uuid,
    pub name: String,
    pub class_teacher_id: Option<Uuid>,
    pub capacity: Option<i32>,
}

pub async fn create_section(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    input: NewSection,
) -> DbResult<Section> {
    let id = new_id();
    let row = sqlx::query_as::<_, Section>(
        "INSERT INTO sections (id, school_id, class_id, academic_year_id, name, class_teacher_id, capacity, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING *",
    )
    .bind(id)
    .bind(input.school_id)
    .bind(input.class_id)
    .bind(input.academic_year_id)
    .bind(input.name.trim().to_uppercase())
    .bind(input.class_teacher_id)
    .bind(input.capacity)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a section with this name in this class and year"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "section",
        Some(id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

#[derive(Debug, Clone, Default)]
pub struct SectionUpdate {
    pub name: Option<String>,
    /// `Some(None)` clears the class teacher.
    pub class_teacher_id: Option<Option<Uuid>>,
    pub capacity: Option<Option<i32>>,
}

pub async fn update_section(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    patch: SectionUpdate,
) -> DbResult<Section> {
    let before = get_section(&mut *conn, id).await?;
    let after = sqlx::query_as::<_, Section>(
        "UPDATE sections SET name = $2, class_teacher_id = $3, capacity = $4, updated_at = now(), updated_by = $5, hlc = $6
         WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(patch.name.map(|n| n.trim().to_uppercase()).unwrap_or_else(|| before.name.clone()))
    .bind(patch.class_teacher_id.unwrap_or(before.class_teacher_id))
    .bind(patch.capacity.unwrap_or(before.capacity))
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a section with this name in this class and year"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "section",
        Some(id),
        Some(snapshot(&before)),
        Some(snapshot(&after)),
    )
    .await?;
    Ok(after)
}

pub async fn get_section(conn: &mut PgConnection, id: Uuid) -> DbResult<Section> {
    sqlx::query_as::<_, Section>("SELECT * FROM sections WHERE id = $1 AND deleted_at IS NULL")
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| DbError::NotFound("section".into()))
}

/// Sections of a school, optionally filtered by year and class, ordered by class level then name.
pub async fn list_sections(
    conn: &mut PgConnection,
    school_id: Uuid,
    academic_year_id: Option<Uuid>,
    class_id: Option<Uuid>,
) -> DbResult<Vec<Section>> {
    Ok(sqlx::query_as::<_, Section>(
        "SELECT s.* FROM sections s JOIN classes c ON c.id = s.class_id
         WHERE s.school_id = $1 AND s.deleted_at IS NULL
           AND ($2::uuid IS NULL OR s.academic_year_id = $2)
           AND ($3::uuid IS NULL OR s.class_id = $3)
         ORDER BY c.level, c.name, s.name",
    )
    .bind(school_id)
    .bind(academic_year_id)
    .bind(class_id)
    .fetch_all(conn)
    .await?)
}
