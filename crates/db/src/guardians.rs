//! Guardians and their links to students (siblings share a guardian).

use crate::audit::{self, snapshot, Action};
use crate::models::{Guardian, GuardianLink, StudentGuardian, StudentListRow};
use crate::{Actor, DbError, DbResult};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub struct NewGuardian {
    pub school_id: Uuid,
    pub full_name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub cnic: Option<String>,
    pub occupation: String,
    pub address: String,
    /// Extra fields (e.g. `family_id` from an imported register); `{}` when none.
    pub custom: serde_json::Value,
}

pub async fn create(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    input: NewGuardian,
) -> DbResult<Guardian> {
    let id = new_id();
    let row = sqlx::query_as::<_, Guardian>(
        "INSERT INTO guardians (id, school_id, full_name, phone, email, cnic, occupation, address, custom, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING *",
    )
    .bind(id)
    .bind(input.school_id)
    .bind(input.full_name.trim())
    .bind(input.phone)
    .bind(input.email.map(|e| e.trim().to_lowercase()).filter(|e| !e.is_empty()))
    .bind(input.cnic)
    .bind(input.occupation.trim())
    .bind(input.address.trim())
    .bind(if input.custom.is_object() {
        input.custom
    } else {
        serde_json::json!({})
    })
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "guardian",
        Some(id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

#[derive(Debug, Clone, Default)]
pub struct GuardianUpdate {
    pub full_name: Option<String>,
    pub phone: Option<Option<String>>,
    pub email: Option<Option<String>>,
    pub cnic: Option<Option<String>>,
    pub occupation: Option<String>,
    pub address: Option<String>,
}

pub async fn update(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    p: GuardianUpdate,
) -> DbResult<Guardian> {
    let b = get(&mut *conn, id).await?;
    let after = sqlx::query_as::<_, Guardian>(
        "UPDATE guardians SET full_name = $2, phone = $3, email = $4, cnic = $5, occupation = $6, address = $7,
            updated_at = now(), updated_by = $8, hlc = $9 WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(p.full_name.map(|s| s.trim().to_string()).unwrap_or_else(|| b.full_name.clone()))
    .bind(p.phone.unwrap_or_else(|| b.phone.clone()))
    .bind(p.email.unwrap_or_else(|| b.email.clone()).map(|e| e.trim().to_lowercase()).filter(|e| !e.is_empty()))
    .bind(p.cnic.unwrap_or_else(|| b.cnic.clone()))
    .bind(p.occupation.map(|s| s.trim().to_string()).unwrap_or_else(|| b.occupation.clone()))
    .bind(p.address.map(|s| s.trim().to_string()).unwrap_or_else(|| b.address.clone()))
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "guardian",
        Some(id),
        Some(snapshot(&b)),
        Some(snapshot(&after)),
    )
    .await?;
    Ok(after)
}

pub async fn get(conn: &mut PgConnection, id: Uuid) -> DbResult<Guardian> {
    sqlx::query_as::<_, Guardian>("SELECT * FROM guardians WHERE id = $1 AND deleted_at IS NULL")
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| DbError::NotFound("guardian".into()))
}

/// Search by name (substring), or by the digits of a phone number / CNIC (prefix match).
pub async fn search(
    conn: &mut PgConnection,
    school_id: Uuid,
    q: &str,
    limit: i64,
) -> DbResult<Vec<Guardian>> {
    let q = q.trim();
    let digits: String = q.chars().filter(|c| c.is_ascii_digit()).collect();
    let digits = if digits.len() >= 3 {
        digits
    } else {
        String::new()
    };
    Ok(sqlx::query_as::<_, Guardian>(
        "SELECT * FROM guardians WHERE school_id = $1 AND deleted_at IS NULL
           AND (full_name ILIKE '%' || $2 || '%'
                OR ($3 <> '' AND (phone LIKE $3 || '%' OR replace(coalesce(cnic, ''), '-', '') LIKE $3 || '%')))
         ORDER BY full_name LIMIT $4",
    )
    .bind(school_id)
    .bind(q)
    .bind(digits)
    .bind(limit)
    .fetch_all(conn)
    .await?)
}

/// Existing guardian with the same CNIC or phone, used to share records between siblings.
/// A guardian recorded under a family id (from an imported register) with this name, so
/// siblings in the same family share one guardian record even without a phone or ID number.
pub async fn find_by_family(
    conn: &mut PgConnection,
    school_id: Uuid,
    family_id: &str,
    full_name: &str,
) -> DbResult<Option<Guardian>> {
    Ok(sqlx::query_as::<_, Guardian>(
        "SELECT * FROM guardians WHERE school_id = $1 AND deleted_at IS NULL
           AND custom ->> 'family_id' = $2 AND lower(full_name) = lower($3)
         ORDER BY created_at LIMIT 1",
    )
    .bind(school_id)
    .bind(family_id)
    .bind(full_name.trim())
    .fetch_optional(conn)
    .await?)
}

/// The guardian record to reuse: the same ID number, or the same phone **and** the same name
/// (case-insensitive). A phone alone never merges two people, since families and even
/// unrelated households share phones.
pub async fn find_matching(
    conn: &mut PgConnection,
    school_id: Uuid,
    cnic: Option<&str>,
    phone: Option<&str>,
    full_name: &str,
) -> DbResult<Option<Guardian>> {
    Ok(sqlx::query_as::<_, Guardian>(
        "SELECT * FROM guardians WHERE school_id = $1 AND deleted_at IS NULL
           AND (($2::text IS NOT NULL AND cnic = $2)
             OR ($3::text IS NOT NULL AND phone = $3 AND lower(full_name) = lower($4)))
         ORDER BY (cnic = $2) DESC NULLS LAST, created_at LIMIT 1",
    )
    .bind(school_id)
    .bind(cnic)
    .bind(phone)
    .bind(full_name.trim())
    .fetch_optional(conn)
    .await?)
}

/// Guardians recorded under a phone number, for warning when a different name turns up with
/// the same phone.
pub async fn find_by_phone(
    conn: &mut PgConnection,
    school_id: Uuid,
    phone: &str,
) -> DbResult<Vec<Guardian>> {
    Ok(sqlx::query_as::<_, Guardian>(
        "SELECT * FROM guardians WHERE school_id = $1 AND deleted_at IS NULL AND phone = $2 ORDER BY created_at",
    )
    .bind(school_id)
    .bind(phone)
    .fetch_all(conn)
    .await?)
}

pub async fn link(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    student_id: Uuid,
    guardian_id: Uuid,
    relationship: &str,
    is_primary: bool,
) -> DbResult<StudentGuardian> {
    let guardian = get(&mut *conn, guardian_id).await?;
    if is_primary {
        sqlx::query("UPDATE student_guardians SET is_primary = FALSE, updated_at = now(), hlc = $2 WHERE student_id = $1 AND deleted_at IS NULL")
            .bind(student_id)
            .bind(hlc.now().to_string())
            .execute(&mut *conn)
            .await?;
    }
    let row = sqlx::query_as::<_, StudentGuardian>(
        "INSERT INTO student_guardians (id, school_id, student_id, guardian_id, relationship, is_primary, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (student_id, guardian_id) DO UPDATE SET relationship = EXCLUDED.relationship, is_primary = EXCLUDED.is_primary,
            deleted_at = NULL, updated_at = now(), updated_by = EXCLUDED.updated_by, hlc = EXCLUDED.hlc
         RETURNING id, school_id, student_id, guardian_id, relationship, is_primary, hlc",
    )
    .bind(new_id())
    .bind(guardian.school_id)
    .bind(student_id)
    .bind(guardian_id)
    .bind(relationship)
    .bind(is_primary)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "student_guardian",
        Some(row.id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn unlink(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    student_id: Uuid,
    guardian_id: Uuid,
) -> DbResult<()> {
    let before = sqlx::query_as::<_, StudentGuardian>(
        "SELECT id, school_id, student_id, guardian_id, relationship, is_primary, hlc FROM student_guardians
         WHERE student_id = $1 AND guardian_id = $2 AND deleted_at IS NULL",
    )
    .bind(student_id)
    .bind(guardian_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| DbError::NotFound("guardian link".into()))?;
    sqlx::query("UPDATE student_guardians SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
        .bind(before.id)
        .bind(actor.user_id)
        .bind(hlc.now().to_string())
        .execute(&mut *conn)
        .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Delete,
        "student_guardian",
        Some(before.id),
        Some(snapshot(&before)),
        None,
    )
    .await?;
    Ok(())
}

/// Guardians of a student with relationship details, primary first.
pub async fn for_student(conn: &mut PgConnection, student_id: Uuid) -> DbResult<Vec<GuardianLink>> {
    Ok(sqlx::query_as::<_, GuardianLink>(
        "SELECT g.id, g.school_id, g.full_name, g.phone, g.email, g.cnic, g.occupation, g.address, sg.relationship, sg.is_primary
         FROM student_guardians sg JOIN guardians g ON g.id = sg.guardian_id
         WHERE sg.student_id = $1 AND sg.deleted_at IS NULL AND g.deleted_at IS NULL
         ORDER BY sg.is_primary DESC, g.full_name",
    )
    .bind(student_id)
    .fetch_all(conn)
    .await?)
}

/// Other students sharing any guardian with this student.
pub async fn siblings(conn: &mut PgConnection, student_id: Uuid) -> DbResult<Vec<StudentListRow>> {
    Ok(sqlx::query_as::<_, StudentListRow>(
        "SELECT DISTINCT s.id, s.student_number, s.full_name, s.father_name, s.gender, s.date_of_birth, s.status, s.admission_date,
                c.id AS class_id, c.name AS class_name, sec.id AS section_id, sec.name AS section_name, e.roll_no,
                (s.photo IS NOT NULL) AS has_photo
         FROM student_guardians a
         JOIN student_guardians b ON b.guardian_id = a.guardian_id AND b.student_id <> a.student_id AND b.deleted_at IS NULL
         JOIN students s ON s.id = b.student_id AND s.deleted_at IS NULL
         LEFT JOIN academic_years y ON y.school_id = s.school_id AND y.is_active AND y.deleted_at IS NULL
         LEFT JOIN enrollments e ON e.student_id = s.id AND e.academic_year_id = y.id AND e.deleted_at IS NULL
         LEFT JOIN sections sec ON sec.id = e.section_id
         LEFT JOIN classes c ON c.id = sec.class_id
         WHERE a.student_id = $1 AND a.deleted_at IS NULL
         ORDER BY s.full_name",
    )
    .bind(student_id)
    .fetch_all(conn)
    .await?)
}

/// Students linked to a guardian.
pub async fn students_of(
    conn: &mut PgConnection,
    guardian_id: Uuid,
) -> DbResult<Vec<StudentListRow>> {
    Ok(sqlx::query_as::<_, StudentListRow>(
        "SELECT s.id, s.student_number, s.full_name, s.father_name, s.gender, s.date_of_birth, s.status, s.admission_date,
                c.id AS class_id, c.name AS class_name, sec.id AS section_id, sec.name AS section_name, e.roll_no,
                (s.photo IS NOT NULL) AS has_photo
         FROM student_guardians sg JOIN students s ON s.id = sg.student_id AND s.deleted_at IS NULL
         LEFT JOIN academic_years y ON y.school_id = s.school_id AND y.is_active AND y.deleted_at IS NULL
         LEFT JOIN enrollments e ON e.student_id = s.id AND e.academic_year_id = y.id AND e.deleted_at IS NULL
         LEFT JOIN sections sec ON sec.id = e.section_id
         LEFT JOIN classes c ON c.id = sec.class_id
         WHERE sg.guardian_id = $1 AND sg.deleted_at IS NULL ORDER BY s.full_name",
    )
    .bind(guardian_id)
    .fetch_all(conn)
    .await?)
}
