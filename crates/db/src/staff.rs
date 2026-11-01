//! Staff profiles and teaching assignments.

use crate::audit::{self, snapshot, Action};
use crate::models::{SectionTeacher, Staff};
use crate::{map_unique, Actor, DbError, DbResult};
use chrono::NaiveDate;
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NewStaff {
    pub school_id: Uuid,
    pub user_id: Option<Uuid>,
    pub staff_number: String,
    pub full_name: String,
    pub father_name: String,
    pub gender: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub cnic: Option<String>,
    pub designation: String,
    pub joined_on: Option<NaiveDate>,
    /// Extra fields (staff type, department, grades from an import); `{}` when none.
    pub custom: serde_json::Value,
}

pub async fn create(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    input: NewStaff,
) -> DbResult<Staff> {
    let id = new_id();
    let row = sqlx::query_as::<_, Staff>(
        "INSERT INTO staff (id, school_id, user_id, staff_number, full_name, father_name, gender, phone, email, cnic, designation, joined_on, custom, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15) RETURNING *",
    )
    .bind(id)
    .bind(input.school_id)
    .bind(input.user_id)
    .bind(input.staff_number.trim().to_uppercase())
    .bind(input.full_name.trim())
    .bind(input.father_name.trim())
    .bind(input.gender)
    .bind(input.phone)
    .bind(input.email.map(|e| e.trim().to_lowercase()))
    .bind(input.cnic)
    .bind(input.designation.trim())
    .bind(input.joined_on)
    .bind(if input.custom.is_object() {
        input.custom
    } else {
        serde_json::json!({})
    })
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a staff member with this number or login"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "staff",
        Some(id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

#[derive(Debug, Clone, Default)]
pub struct StaffUpdate {
    pub user_id: Option<Option<Uuid>>,
    pub full_name: Option<String>,
    pub father_name: Option<String>,
    pub gender: Option<String>,
    pub phone: Option<Option<String>>,
    pub email: Option<Option<String>>,
    pub cnic: Option<Option<String>>,
    pub designation: Option<String>,
    pub joined_on: Option<Option<NaiveDate>>,
    pub status: Option<String>,
    pub custom: Option<serde_json::Value>,
}

pub async fn update(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    p: StaffUpdate,
) -> DbResult<Staff> {
    let b = get(&mut *conn, id).await?;
    let after = sqlx::query_as::<_, Staff>(
        "UPDATE staff SET user_id = $2, full_name = $3, father_name = $4, gender = $5, phone = $6, email = $7, cnic = $8,
           designation = $9, joined_on = $10, status = $11, custom = $12, updated_at = now(), updated_by = $13, hlc = $14
         WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(p.user_id.unwrap_or(b.user_id))
    .bind(p.full_name.map(|s| s.trim().to_string()).unwrap_or_else(|| b.full_name.clone()))
    .bind(p.father_name.map(|s| s.trim().to_string()).unwrap_or_else(|| b.father_name.clone()))
    .bind(p.gender.unwrap_or_else(|| b.gender.clone()))
    .bind(p.phone.unwrap_or_else(|| b.phone.clone()))
    .bind(p.email.unwrap_or_else(|| b.email.clone()).map(|e| e.trim().to_lowercase()))
    .bind(p.cnic.unwrap_or_else(|| b.cnic.clone()))
    .bind(p.designation.map(|s| s.trim().to_string()).unwrap_or_else(|| b.designation.clone()))
    .bind(p.joined_on.unwrap_or(b.joined_on))
    .bind(p.status.unwrap_or_else(|| b.status.clone()))
    .bind(p.custom.unwrap_or_else(|| b.custom.clone()))
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a staff member with this login"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "staff",
        Some(id),
        Some(snapshot(&b)),
        Some(snapshot(&after)),
    )
    .await?;
    Ok(after)
}

pub async fn find_by_number(
    conn: &mut PgConnection,
    school_id: Uuid,
    staff_number: &str,
) -> DbResult<Option<Staff>> {
    Ok(sqlx::query_as::<_, Staff>(
        "SELECT * FROM staff WHERE school_id = $1 AND upper(staff_number) = upper($2) AND deleted_at IS NULL",
    )
    .bind(school_id)
    .bind(staff_number.trim())
    .fetch_optional(conn)
    .await?)
}

pub async fn find_by_cnic(
    conn: &mut PgConnection,
    school_id: Uuid,
    cnic: &str,
) -> DbResult<Option<Staff>> {
    Ok(sqlx::query_as::<_, Staff>(
        "SELECT * FROM staff WHERE school_id = $1 AND cnic = $2 AND deleted_at IS NULL ORDER BY created_at LIMIT 1",
    )
    .bind(school_id)
    .bind(cnic)
    .fetch_optional(conn)
    .await?)
}

pub async fn get(conn: &mut PgConnection, id: Uuid) -> DbResult<Staff> {
    sqlx::query_as::<_, Staff>("SELECT * FROM staff WHERE id = $1 AND deleted_at IS NULL")
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| DbError::NotFound("staff member".into()))
}

pub async fn find_by_user(conn: &mut PgConnection, user_id: Uuid) -> DbResult<Option<Staff>> {
    Ok(
        sqlx::query_as::<_, Staff>("SELECT * FROM staff WHERE user_id = $1 AND deleted_at IS NULL")
            .bind(user_id)
            .fetch_optional(conn)
            .await?,
    )
}

pub async fn list(
    conn: &mut PgConnection,
    school_id: Uuid,
    q: Option<&str>,
) -> DbResult<Vec<Staff>> {
    Ok(sqlx::query_as::<_, Staff>(
        "SELECT * FROM staff WHERE school_id = $1 AND deleted_at IS NULL
           AND ($2::text IS NULL OR full_name ILIKE '%' || $2 || '%' OR staff_number ILIKE '%' || $2 || '%' OR designation ILIKE '%' || $2 || '%')
         ORDER BY full_name",
    )
    .bind(school_id)
    .bind(q.map(str::trim).filter(|s| !s.is_empty()))
    .fetch_all(conn)
    .await?)
}

pub async fn count(conn: &mut PgConnection, school_id: Uuid) -> DbResult<i64> {
    Ok(
        sqlx::query_scalar(
            "SELECT count(*) FROM staff WHERE school_id = $1 AND deleted_at IS NULL",
        )
        .bind(school_id)
        .fetch_one(conn)
        .await?,
    )
}

/// Teaching assignments of a staff member.
pub async fn assignments(conn: &mut PgConnection, staff_id: Uuid) -> DbResult<Vec<SectionTeacher>> {
    Ok(sqlx::query_as::<_, SectionTeacher>(
        "SELECT id, school_id, staff_id, section_id, subject_id, hlc FROM section_teachers
         WHERE staff_id = $1 AND deleted_at IS NULL ORDER BY section_id, subject_id",
    )
    .bind(staff_id)
    .fetch_all(conn)
    .await?)
}

/// Replace a staff member's assignments with `(section_id, subject_id)` pairs; audited.
pub async fn set_assignments(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    staff_id: Uuid,
    pairs: &[(Uuid, Option<Uuid>)],
) -> DbResult<Vec<SectionTeacher>> {
    let staff = get(&mut *conn, staff_id).await?;
    let before = assignments(&mut *conn, staff_id).await?;
    let stamp = hlc.now().to_string();
    sqlx::query("UPDATE section_teachers SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE staff_id = $1 AND deleted_at IS NULL")
        .bind(staff_id)
        .bind(actor.user_id)
        .bind(&stamp)
        .execute(&mut *conn)
        .await?;
    for (section_id, subject_id) in pairs {
        sqlx::query(
            "INSERT INTO section_teachers (id, school_id, staff_id, section_id, subject_id, updated_by, hlc) VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (staff_id, section_id, subject_id) DO UPDATE SET deleted_at = NULL, updated_at = now(), updated_by = EXCLUDED.updated_by, hlc = EXCLUDED.hlc",
        )
        .bind(new_id())
        .bind(staff.school_id)
        .bind(staff_id)
        .bind(section_id)
        .bind(subject_id)
        .bind(actor.user_id)
        .bind(&stamp)
        .execute(&mut *conn)
        .await?;
    }
    let after = assignments(&mut *conn, staff_id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "section_teachers",
        Some(staff_id),
        Some(snapshot(&before)),
        Some(snapshot(&after)),
    )
    .await?;
    Ok(after)
}

/// Section ids a staff member is responsible for: class teacher of, or teaching a subject in.
pub async fn section_scope(conn: &mut PgConnection, staff_id: Uuid) -> DbResult<Vec<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM sections WHERE class_teacher_id = $1 AND deleted_at IS NULL
         UNION SELECT section_id FROM section_teachers WHERE staff_id = $1 AND deleted_at IS NULL",
    )
    .bind(staff_id)
    .fetch_all(conn)
    .await?)
}
