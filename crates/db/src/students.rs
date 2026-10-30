//! Students, enrollments and student numbers.

use crate::audit::{self, snapshot, Action};
use crate::models::{Enrollment, Student, StudentListRow};
use crate::{map_unique, settings, Actor, DbError, DbResult};
use chrono::{Datelike, NaiveDate};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use madrasa_core::students::{
    duplicate_reason, format_student_number, DuplicateProbe, DuplicateReason,
};
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

/// Columns selected for a `Student` (never the photo bytes).
const COLS: &str = "id, school_id, student_number, legacy_id, full_name, father_name, date_of_birth, gender, cnic, address, \
    admission_date, status, left_on, left_reason, board_roll_no, board_registration_no, (photo IS NOT NULL) AS has_photo, \
    custom, created_at, updated_at, updated_by, deleted_at, hlc";

// ---- sequences and numbering --------------------------------------------------------------

/// Next value of a per-school counter, under a row lock (call inside a transaction).
pub async fn next_sequence(
    conn: &mut PgConnection,
    hlc: &Hlc,
    school_id: Uuid,
    key: &str,
) -> DbResult<i32> {
    sqlx::query(
        "INSERT INTO school_sequences (id, school_id, key, next_value, hlc) VALUES ($1, $2, $3, 1, $4)
         ON CONFLICT (school_id, key) DO NOTHING",
    )
    .bind(new_id())
    .bind(school_id)
    .bind(key)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    let value: i32 = sqlx::query_scalar(
        "UPDATE school_sequences SET next_value = next_value + 1, updated_at = now(), hlc = $3
         WHERE school_id = $1 AND key = $2 RETURNING next_value - 1",
    )
    .bind(school_id)
    .bind(key)
    .bind(hlc.now().to_string())
    .fetch_one(conn)
    .await?;
    Ok(value)
}

pub const KEY_STUDENT_NUMBER_PREFIX: &str = "student_number.prefix";

/// Student number prefix: the `student_number.prefix` setting or the school code.
pub async fn number_prefix(conn: &mut PgConnection, school_id: Uuid) -> DbResult<String> {
    if let Some(v) = settings::get(&mut *conn, Some(school_id), KEY_STUDENT_NUMBER_PREFIX).await? {
        if let Some(s) = v.as_str().filter(|s| !s.trim().is_empty()) {
            return Ok(s.to_string());
        }
    }
    let code: String = sqlx::query_scalar("SELECT code FROM schools WHERE id = $1")
        .bind(school_id)
        .fetch_one(conn)
        .await?;
    Ok(code)
}

/// Allocate the next student number for an admission year.
pub async fn allocate_number(
    conn: &mut PgConnection,
    hlc: &Hlc,
    school_id: Uuid,
    admission_date: NaiveDate,
) -> DbResult<String> {
    let prefix = number_prefix(&mut *conn, school_id).await?;
    let year = admission_date.year();
    let seq = next_sequence(conn, hlc, school_id, &format!("student_number:{year}")).await?;
    Ok(format_student_number(&prefix, year, seq as u32))
}

// ---- students -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct NewStudent {
    pub school_id: Uuid,
    /// `None` allocates the next number.
    pub student_number: Option<String>,
    pub legacy_id: Option<String>,
    pub full_name: String,
    pub father_name: String,
    pub date_of_birth: NaiveDate,
    pub gender: String,
    pub cnic: Option<String>,
    pub address: String,
    pub admission_date: NaiveDate,
    pub board_roll_no: Option<String>,
    pub board_registration_no: Option<String>,
    pub custom: serde_json::Value,
}

pub async fn create(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    input: NewStudent,
) -> DbResult<Student> {
    let number = match input.student_number {
        Some(n) if !n.trim().is_empty() => n.trim().to_uppercase(),
        _ => allocate_number(&mut *conn, hlc, input.school_id, input.admission_date).await?,
    };
    let id = new_id();
    let row = sqlx::query_as::<_, Student>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO students (id, school_id, student_number, legacy_id, full_name, father_name, date_of_birth, gender, cnic, address,
            admission_date, board_roll_no, board_registration_no, custom, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16) RETURNING {COLS}"
    )))
    .bind(id)
    .bind(input.school_id)
    .bind(&number)
    .bind(input.legacy_id.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()))
    .bind(input.full_name.trim())
    .bind(input.father_name.trim())
    .bind(input.date_of_birth)
    .bind(input.gender)
    .bind(input.cnic)
    .bind(input.address.trim())
    .bind(input.admission_date)
    .bind(input.board_roll_no)
    .bind(input.board_registration_no)
    .bind(input.custom)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a student with this number"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "student",
        Some(id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

#[derive(Debug, Clone, Default)]
pub struct StudentUpdate {
    pub legacy_id: Option<Option<String>>,
    pub full_name: Option<String>,
    pub father_name: Option<String>,
    pub date_of_birth: Option<NaiveDate>,
    pub gender: Option<String>,
    pub cnic: Option<Option<String>>,
    pub address: Option<String>,
    pub admission_date: Option<NaiveDate>,
    pub board_roll_no: Option<Option<String>>,
    pub board_registration_no: Option<Option<String>>,
    pub custom: Option<serde_json::Value>,
}

pub async fn update(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    p: StudentUpdate,
) -> DbResult<Student> {
    let b = get(&mut *conn, id).await?;
    let after = sqlx::query_as::<_, Student>(sqlx::AssertSqlSafe(format!(
        "UPDATE students SET legacy_id = $2, full_name = $3, father_name = $4, date_of_birth = $5, gender = $6, cnic = $7, address = $8,
            admission_date = $9, board_roll_no = $10, board_registration_no = $11, custom = $12, updated_at = now(), updated_by = $13, hlc = $14
         WHERE id = $1 RETURNING {COLS}"
    )))
    .bind(id)
    .bind(p.legacy_id.unwrap_or_else(|| b.legacy_id.clone()))
    .bind(p.full_name.map(|s| s.trim().to_string()).unwrap_or_else(|| b.full_name.clone()))
    .bind(p.father_name.map(|s| s.trim().to_string()).unwrap_or_else(|| b.father_name.clone()))
    .bind(p.date_of_birth.unwrap_or(b.date_of_birth))
    .bind(p.gender.unwrap_or_else(|| b.gender.clone()))
    .bind(p.cnic.unwrap_or_else(|| b.cnic.clone()))
    .bind(p.address.map(|s| s.trim().to_string()).unwrap_or_else(|| b.address.clone()))
    .bind(p.admission_date.unwrap_or(b.admission_date))
    .bind(p.board_roll_no.unwrap_or_else(|| b.board_roll_no.clone()))
    .bind(p.board_registration_no.unwrap_or_else(|| b.board_registration_no.clone()))
    .bind(p.custom.unwrap_or_else(|| b.custom.clone()))
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "student",
        Some(id),
        Some(snapshot(&b)),
        Some(snapshot(&after)),
    )
    .await?;
    Ok(after)
}

/// Withdraw or graduate a student. There is no hard delete.
pub async fn set_status(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    status: &str,
    left_on: Option<NaiveDate>,
    reason: Option<&str>,
) -> DbResult<Student> {
    let b = get(&mut *conn, id).await?;
    let (left_on, reason) = if status == "active" {
        (None, None)
    } else {
        (left_on, reason)
    };
    let after = sqlx::query_as::<_, Student>(sqlx::AssertSqlSafe(format!(
        "UPDATE students SET status = $2, left_on = $3, left_reason = $4, updated_at = now(), updated_by = $5, hlc = $6 WHERE id = $1 RETURNING {COLS}"
    )))
    .bind(id)
    .bind(status)
    .bind(left_on)
    .bind(reason)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "student",
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
    student_number: &str,
) -> DbResult<Option<Student>> {
    Ok(sqlx::query_as::<_, Student>(
        "SELECT * FROM students WHERE school_id = $1 AND upper(student_number) = upper($2) AND deleted_at IS NULL",
    )
    .bind(school_id)
    .bind(student_number.trim())
    .fetch_optional(conn)
    .await?)
}

pub async fn find_by_legacy_id(
    conn: &mut PgConnection,
    school_id: Uuid,
    legacy_id: &str,
) -> DbResult<Option<Student>> {
    Ok(sqlx::query_as::<_, Student>(
        "SELECT * FROM students WHERE school_id = $1 AND upper(legacy_id) = upper($2) AND deleted_at IS NULL
         ORDER BY created_at LIMIT 1",
    )
    .bind(school_id)
    .bind(legacy_id.trim())
    .fetch_optional(conn)
    .await?)
}

pub async fn get(conn: &mut PgConnection, id: Uuid) -> DbResult<Student> {
    sqlx::query_as::<_, Student>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM students WHERE id = $1 AND deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("student".into()))
}

pub async fn count(
    conn: &mut PgConnection,
    school_id: Uuid,
    status: Option<&str>,
) -> DbResult<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM students WHERE school_id = $1 AND deleted_at IS NULL AND ($2::text IS NULL OR status = $2)",
    )
    .bind(school_id)
    .bind(status)
    .fetch_one(conn)
    .await?)
}

// ---- photo --------------------------------------------------------------------------------

pub async fn photo(conn: &mut PgConnection, id: Uuid) -> DbResult<Option<(Vec<u8>, String)>> {
    let row: Option<(Option<Vec<u8>>, Option<String>)> = sqlx::query_as(
        "SELECT photo, photo_mime FROM students WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(conn)
    .await?;
    match row {
        None => Err(DbError::NotFound("student".into())),
        Some((Some(bytes), Some(mime))) => Ok(Some((bytes, mime))),
        Some(_) => Ok(None),
    }
}

pub async fn set_photo(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    photo: Option<(Vec<u8>, String)>,
) -> DbResult<()> {
    let before = get(&mut *conn, id).await?;
    let (bytes, mime) = match &photo {
        Some((b, m)) => (Some(b.as_slice()), Some(m.as_str())),
        None => (None, None),
    };
    sqlx::query("UPDATE students SET photo = $2, photo_mime = $3, updated_at = now(), updated_by = $4, hlc = $5 WHERE id = $1")
        .bind(id)
        .bind(bytes)
        .bind(mime)
        .bind(actor.user_id)
        .bind(hlc.now().to_string())
        .execute(&mut *conn)
        .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "student",
        Some(id),
        Some(serde_json::json!({ "has_photo": before.has_photo })),
        Some(serde_json::json!({ "has_photo": photo.is_some() })),
    )
    .await?;
    Ok(())
}

// ---- enrollments --------------------------------------------------------------------------

/// Enrol (or move) a student in a section for the section's academic year.
pub async fn enrol(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    student_id: Uuid,
    section_id: Uuid,
    roll_no: Option<i32>,
) -> DbResult<Enrollment> {
    let (school_id, year_id): (Uuid, Uuid) = sqlx::query_as(
        "SELECT school_id, academic_year_id FROM sections WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(section_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| DbError::NotFound("section".into()))?;
    let existing = sqlx::query_as::<_, Enrollment>(
        "SELECT id, school_id, student_id, academic_year_id, section_id, roll_no, hlc FROM enrollments
         WHERE student_id = $1 AND academic_year_id = $2 AND deleted_at IS NULL",
    )
    .bind(student_id)
    .bind(year_id)
    .fetch_optional(&mut *conn)
    .await?;
    let row = match &existing {
        Some(e) => {
            sqlx::query_as::<_, Enrollment>(
                "UPDATE enrollments SET section_id = $2, roll_no = $3, updated_at = now(), updated_by = $4, hlc = $5 WHERE id = $1
                 RETURNING id, school_id, student_id, academic_year_id, section_id, roll_no, hlc",
            )
            .bind(e.id)
            .bind(section_id)
            .bind(roll_no)
            .bind(actor.user_id)
            .bind(hlc.now().to_string())
            .fetch_one(&mut *conn)
            .await?
        }
        None => {
            sqlx::query_as::<_, Enrollment>(
                "INSERT INTO enrollments (id, school_id, student_id, academic_year_id, section_id, roll_no, updated_by, hlc)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id, school_id, student_id, academic_year_id, section_id, roll_no, hlc",
            )
            .bind(new_id())
            .bind(school_id)
            .bind(student_id)
            .bind(year_id)
            .bind(section_id)
            .bind(roll_no)
            .bind(actor.user_id)
            .bind(hlc.now().to_string())
            .fetch_one(&mut *conn)
            .await?
        }
    };
    let action = if existing.is_some() {
        Action::Update
    } else {
        Action::Create
    };
    audit::record(
        conn,
        hlc,
        actor,
        action,
        "enrollment",
        Some(row.id),
        existing.as_ref().map(snapshot),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn enrollments(conn: &mut PgConnection, student_id: Uuid) -> DbResult<Vec<Enrollment>> {
    Ok(sqlx::query_as::<_, Enrollment>(
        "SELECT e.id, e.school_id, e.student_id, e.academic_year_id, e.section_id, e.roll_no, e.hlc FROM enrollments e
         JOIN academic_years y ON y.id = e.academic_year_id
         WHERE e.student_id = $1 AND e.deleted_at IS NULL ORDER BY y.start_date DESC",
    )
    .bind(student_id)
    .fetch_all(conn)
    .await?)
}

/// Section of the student's enrollment in the given year, if any.
pub async fn section_in_year(
    conn: &mut PgConnection,
    student_id: Uuid,
    academic_year_id: Uuid,
) -> DbResult<Option<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT section_id FROM enrollments WHERE student_id = $1 AND academic_year_id = $2 AND deleted_at IS NULL",
    )
    .bind(student_id)
    .bind(academic_year_id)
    .fetch_optional(conn)
    .await?)
}

// ---- directory ----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudentSort {
    Name,
    Number,
    Class,
    AdmissionDate,
}

#[derive(Debug, Clone, Default)]
pub struct StudentFilter {
    pub q: Option<String>,
    pub class_id: Option<Uuid>,
    pub section_id: Option<Uuid>,
    pub status: Option<String>,
    /// Restrict to these sections (teacher scope). `Some(empty)` matches nothing.
    pub section_scope: Option<Vec<Uuid>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub page: i64,
    pub page_size: i64,
    pub total: i64,
}

/// Directory listing joined with the enrollment in the active academic year.
pub async fn list(
    conn: &mut PgConnection,
    school_id: Uuid,
    filter: &StudentFilter,
    sort: StudentSort,
    desc: bool,
    page: i64,
    page_size: i64,
) -> DbResult<Page<StudentListRow>> {
    let order = match sort {
        StudentSort::Name => "lower(s.full_name)",
        StudentSort::Number => "s.student_number",
        StudentSort::Class => "c.level, c.name, sec.name, e.roll_no, lower(s.full_name)",
        StudentSort::AdmissionDate => "s.admission_date",
    };
    let dir = if desc { "DESC" } else { "ASC" };
    let order_sql = order
        .split(", ")
        .map(|col| format!("{col} {dir}"))
        .collect::<Vec<_>>()
        .join(", ");
    let base = "FROM students s
         LEFT JOIN academic_years y ON y.school_id = s.school_id AND y.is_active AND y.deleted_at IS NULL
         LEFT JOIN enrollments e ON e.student_id = s.id AND e.academic_year_id = y.id AND e.deleted_at IS NULL
         LEFT JOIN sections sec ON sec.id = e.section_id
         LEFT JOIN classes c ON c.id = sec.class_id
         WHERE s.school_id = $1 AND s.deleted_at IS NULL
           AND ($2::text IS NULL OR s.full_name ILIKE '%' || $2 || '%' OR s.father_name ILIKE '%' || $2 || '%'
                OR s.student_number ILIKE '%' || $2 || '%' OR s.cnic = $2 OR s.legacy_id ILIKE $2)
           AND ($3::uuid IS NULL OR sec.class_id = $3)
           AND ($4::uuid IS NULL OR e.section_id = $4)
           AND ($5::text IS NULL OR s.status = $5)
           AND ($6::uuid[] IS NULL OR e.section_id = ANY($6))";
    let q = filter.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) {base}")))
        .bind(school_id)
        .bind(q)
        .bind(filter.class_id)
        .bind(filter.section_id)
        .bind(filter.status.as_deref())
        .bind(filter.section_scope.as_deref())
        .fetch_one(&mut *conn)
        .await?;
    let page = page.max(1);
    let page_size = page_size.clamp(1, 2000);
    let items = sqlx::query_as::<_, StudentListRow>(sqlx::AssertSqlSafe(format!(
        "SELECT s.id, s.student_number, s.full_name, s.father_name, s.gender, s.date_of_birth, s.status, s.admission_date,
                c.id AS class_id, c.name AS class_name, sec.id AS section_id, sec.name AS section_name, e.roll_no,
                (s.photo IS NOT NULL) AS has_photo
         {base} ORDER BY {order_sql}, s.id LIMIT $7 OFFSET $8"
    )))
    .bind(school_id)
    .bind(q)
    .bind(filter.class_id)
    .bind(filter.section_id)
    .bind(filter.status.as_deref())
    .bind(filter.section_scope.as_deref())
    .bind(page_size)
    .bind((page - 1) * page_size)
    .fetch_all(conn)
    .await?;
    Ok(Page {
        items,
        page,
        page_size,
        total,
    })
}

// ---- duplicates ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateMatch {
    pub student: Student,
    pub reason: DuplicateReason,
}

/// Existing students that look like the probe (CNIC, or name + DOB + father's name).
pub async fn find_duplicates(
    conn: &mut PgConnection,
    school_id: Uuid,
    probe: &DuplicateProbe,
    exclude: Option<Uuid>,
) -> DbResult<Vec<DuplicateMatch>> {
    let candidates = sqlx::query_as::<_, Student>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM students WHERE school_id = $1 AND deleted_at IS NULL AND ($4::uuid IS NULL OR id <> $4)
           AND (($2::text IS NOT NULL AND cnic = $2) OR date_of_birth = $3)"
    )))
    .bind(school_id)
    .bind(probe.cnic.as_deref())
    .bind(probe.date_of_birth)
    .bind(exclude)
    .fetch_all(conn)
    .await?;
    Ok(candidates
        .into_iter()
        .filter_map(|s| {
            let existing = DuplicateProbe {
                full_name: s.full_name.clone(),
                father_name: s.father_name.clone(),
                date_of_birth: s.date_of_birth,
                cnic: s.cnic.clone(),
            };
            duplicate_reason(probe, &existing).map(|reason| DuplicateMatch { student: s, reason })
        })
        .collect())
}
