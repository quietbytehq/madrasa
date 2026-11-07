//! Allocation screens (M2 stage 7): teacher cells (section × subject) and section rosters
//! for bulk student moves. Moves themselves go through `students::enrol`, which audits.

use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, Action};
use crate::{Actor, DbResult};

/// One teaching assignment with a subject, joined to the staff member.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TeacherCell {
    pub section_id: Uuid,
    pub subject_id: Uuid,
    pub staff_id: Uuid,
    pub staff_name: String,
}

/// Every subject assignment in the sections of an academic year.
pub async fn teacher_cells(
    conn: &mut PgConnection,
    academic_year_id: Uuid,
) -> DbResult<Vec<TeacherCell>> {
    Ok(sqlx::query_as::<_, TeacherCell>(
        "SELECT st.section_id, st.subject_id, st.staff_id, s.full_name AS staff_name
         FROM section_teachers st
         JOIN sections sec ON sec.id = st.section_id AND sec.deleted_at IS NULL
         JOIN staff s ON s.id = st.staff_id AND s.deleted_at IS NULL
         WHERE sec.academic_year_id = $1 AND st.subject_id IS NOT NULL AND st.deleted_at IS NULL
         ORDER BY st.section_id, st.subject_id, st.created_at",
    )
    .bind(academic_year_id)
    .fetch_all(conn)
    .await?)
}

/// Make `staff_id` the (only) teacher of a subject in a section, or clear the cell; audited
/// with the previous and new staff ids.
pub async fn set_cell_teacher(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    section_id: Uuid,
    subject_id: Uuid,
    staff_id: Option<Uuid>,
) -> DbResult<()> {
    let before: Vec<Uuid> = sqlx::query_scalar(
        "SELECT staff_id FROM section_teachers WHERE section_id = $1 AND subject_id = $2 AND deleted_at IS NULL ORDER BY created_at",
    )
    .bind(section_id)
    .bind(subject_id)
    .fetch_all(&mut *conn)
    .await?;
    if before.len() == 1 && before.first().copied() == staff_id
        || before.is_empty() && staff_id.is_none()
    {
        return Ok(());
    }
    let stamp = hlc.now().to_string();
    sqlx::query(
        "UPDATE section_teachers SET deleted_at = now(), updated_at = now(), updated_by = $3, hlc = $4
         WHERE section_id = $1 AND subject_id = $2 AND deleted_at IS NULL AND ($5::uuid IS NULL OR staff_id <> $5)",
    )
    .bind(section_id)
    .bind(subject_id)
    .bind(actor.user_id)
    .bind(&stamp)
    .bind(staff_id)
    .execute(&mut *conn)
    .await?;
    if let Some(staff_id) = staff_id {
        sqlx::query(
            "INSERT INTO section_teachers (id, school_id, staff_id, section_id, subject_id, updated_by, hlc)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (staff_id, section_id, subject_id) DO UPDATE SET deleted_at = NULL, updated_at = now(),
                updated_by = EXCLUDED.updated_by, hlc = EXCLUDED.hlc",
        )
        .bind(new_id())
        .bind(school_id)
        .bind(staff_id)
        .bind(section_id)
        .bind(subject_id)
        .bind(actor.user_id)
        .bind(&stamp)
        .execute(&mut *conn)
        .await?;
    }
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "section_teacher_cell",
        Some(section_id),
        Some(serde_json::json!({ "subject_id": subject_id, "staff_ids": before })),
        Some(serde_json::json!({ "subject_id": subject_id, "staff_ids": staff_id.into_iter().collect::<Vec<_>>() })),
    )
    .await?;
    Ok(())
}

/// A student of a section as shown on the moves screen.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SectionStudent {
    pub student_id: Uuid,
    pub enrollment_id: Uuid,
    pub section_id: Uuid,
    pub student_number: String,
    pub full_name: String,
    pub gender: String,
    pub roll_no: Option<i32>,
}

/// Active students of every section of a class in a year, in section and roll order.
pub async fn class_students(
    conn: &mut PgConnection,
    academic_year_id: Uuid,
    class_id: Uuid,
) -> DbResult<Vec<SectionStudent>> {
    Ok(sqlx::query_as::<_, SectionStudent>(
        "SELECT s.id AS student_id, e.id AS enrollment_id, e.section_id, s.student_number, s.full_name, s.gender, e.roll_no
         FROM enrollments e
         JOIN sections sec ON sec.id = e.section_id AND sec.deleted_at IS NULL
         JOIN students s ON s.id = e.student_id AND s.deleted_at IS NULL AND s.status = 'active'
         WHERE sec.academic_year_id = $1 AND sec.class_id = $2 AND e.deleted_at IS NULL
         ORDER BY sec.name, e.roll_no NULLS LAST, s.full_name",
    )
    .bind(academic_year_id)
    .bind(class_id)
    .fetch_all(conn)
    .await?)
}

/// The next free roll number in a section (max + 1, or 1).
pub async fn next_roll_no(conn: &mut PgConnection, section_id: Uuid) -> DbResult<i32> {
    let max: Option<i32> = sqlx::query_scalar(
        "SELECT max(roll_no) FROM enrollments WHERE section_id = $1 AND deleted_at IS NULL",
    )
    .bind(section_id)
    .fetch_one(conn)
    .await?;
    Ok(max.unwrap_or(0) + 1)
}
