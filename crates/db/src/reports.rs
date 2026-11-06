//! Read-only queries behind the report library (M2). Each function returns plain rows for one
//! report; the API turns them into a `Report` document. Everything is scoped by `school_id`
//! and, where the caller is a teacher, by the sections they may see.

use chrono::NaiveDate;
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::DbResult;

/// Filters shared by the student reports.
#[derive(Debug, Clone, Default)]
pub struct StudentReportFilter {
    pub academic_year_id: Uuid,
    pub class_id: Option<Uuid>,
    pub section_id: Option<Uuid>,
    /// `active`, `left`, `graduated`; `None` = active students only (the usual list).
    pub status: Option<String>,
    /// Teachers: only these sections. `None` = whole school.
    pub section_scope: Option<Vec<Uuid>>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct StudentReportRow {
    pub student_number: String,
    pub full_name: String,
    pub father_name: String,
    pub gender: String,
    pub date_of_birth: NaiveDate,
    pub status: String,
    pub admission_date: NaiveDate,
    pub cnic: Option<String>,
    pub address: String,
    pub class_name: Option<String>,
    pub class_level: Option<i32>,
    pub section_name: Option<String>,
    pub roll_no: Option<i32>,
    pub guardian_name: Option<String>,
    pub guardian_phone: Option<String>,
    pub guardian_relationship: Option<String>,
    pub has_photo: bool,
    pub guardians: i64,
}

const STUDENT_BASE: &str = "FROM students s
     LEFT JOIN enrollments e ON e.student_id = s.id AND e.academic_year_id = $2 AND e.deleted_at IS NULL
     LEFT JOIN sections sec ON sec.id = e.section_id
     LEFT JOIN classes c ON c.id = sec.class_id
     LEFT JOIN LATERAL (
         SELECT g.full_name, g.phone, sg.relationship
         FROM student_guardians sg JOIN guardians g ON g.id = sg.guardian_id
         WHERE sg.student_id = s.id AND sg.deleted_at IS NULL AND g.deleted_at IS NULL
         ORDER BY sg.is_primary DESC, g.full_name LIMIT 1
     ) pg ON TRUE
     WHERE s.school_id = $1 AND s.deleted_at IS NULL
       AND ($3::uuid IS NULL OR sec.class_id = $3)
       AND ($4::uuid IS NULL OR e.section_id = $4)
       AND s.status = $5
       AND ($6::uuid[] IS NULL OR e.section_id = ANY($6))";

const STUDENT_SELECT: &str = "SELECT s.student_number, s.full_name, s.father_name, s.gender, s.date_of_birth, s.status,
            s.admission_date, s.cnic, s.address, c.name AS class_name, c.level AS class_level,
            sec.name AS section_name, e.roll_no, pg.full_name AS guardian_name, pg.phone AS guardian_phone,
            pg.relationship AS guardian_relationship, (s.photo IS NOT NULL) AS has_photo,
            (SELECT count(*) FROM student_guardians sg2 WHERE sg2.student_id = s.id AND sg2.deleted_at IS NULL) AS guardians";

/// Students with their current class, section, roll number and primary guardian, in class
/// order. Students with no enrolment in the year come last.
pub async fn student_list(
    conn: &mut PgConnection,
    school_id: Uuid,
    f: &StudentReportFilter,
) -> DbResult<Vec<StudentReportRow>> {
    Ok(sqlx::query_as::<_, StudentReportRow>(sqlx::AssertSqlSafe(format!(
        "{STUDENT_SELECT} {STUDENT_BASE}
         ORDER BY c.level NULLS LAST, c.name, sec.name, e.roll_no NULLS LAST, lower(s.full_name), s.id"
    )))
    .bind(school_id)
    .bind(f.academic_year_id)
    .bind(f.class_id)
    .bind(f.section_id)
    .bind(f.status.as_deref().unwrap_or("active"))
    .bind(f.section_scope.as_deref())
    .fetch_all(conn)
    .await?)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct StrengthRow {
    pub class_name: String,
    pub class_level: i32,
    pub section_name: String,
    pub class_teacher: Option<String>,
    pub capacity: Option<i32>,
    pub boys: i64,
    pub girls: i64,
    pub other: i64,
    pub total: i64,
}

/// Active students per section of the year, with the gender split; every section of the year
/// appears, even an empty one.
pub async fn class_strength(
    conn: &mut PgConnection,
    school_id: Uuid,
    academic_year_id: Uuid,
    class_id: Option<Uuid>,
) -> DbResult<Vec<StrengthRow>> {
    Ok(sqlx::query_as::<_, StrengthRow>(
        "SELECT c.name AS class_name, c.level AS class_level, sec.name AS section_name,
                st.full_name AS class_teacher, sec.capacity,
                count(s.id) FILTER (WHERE s.gender = 'male') AS boys,
                count(s.id) FILTER (WHERE s.gender = 'female') AS girls,
                count(s.id) FILTER (WHERE s.gender NOT IN ('male', 'female')) AS other,
                count(s.id) AS total
         FROM sections sec
         JOIN classes c ON c.id = sec.class_id
         LEFT JOIN staff st ON st.id = sec.class_teacher_id AND st.deleted_at IS NULL
         LEFT JOIN enrollments e ON e.section_id = sec.id AND e.academic_year_id = $2 AND e.deleted_at IS NULL
         LEFT JOIN students s ON s.id = e.student_id AND s.deleted_at IS NULL AND s.status = 'active'
         WHERE sec.school_id = $1 AND sec.academic_year_id = $2 AND sec.deleted_at IS NULL
           AND ($3::uuid IS NULL OR sec.class_id = $3)
         GROUP BY c.name, c.level, sec.name, st.full_name, sec.capacity
         ORDER BY c.level, c.name, sec.name",
    )
    .bind(school_id)
    .bind(academic_year_id)
    .bind(class_id)
    .fetch_all(conn)
    .await?)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MovementRow {
    pub student_number: String,
    pub full_name: String,
    pub father_name: String,
    pub gender: String,
    pub class_name: Option<String>,
    pub class_level: Option<i32>,
    pub section_name: Option<String>,
    pub on_date: NaiveDate,
    pub status: String,
    pub reason: Option<String>,
}

/// Students admitted between two dates (inclusive), newest first.
pub async fn admissions(
    conn: &mut PgConnection,
    school_id: Uuid,
    academic_year_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
    class_id: Option<Uuid>,
) -> DbResult<Vec<MovementRow>> {
    Ok(sqlx::query_as::<_, MovementRow>(
        "SELECT s.student_number, s.full_name, s.father_name, s.gender, c.name AS class_name, c.level AS class_level,
                sec.name AS section_name, s.admission_date AS on_date, s.status, NULL::text AS reason
         FROM students s
         LEFT JOIN enrollments e ON e.student_id = s.id AND e.academic_year_id = $2 AND e.deleted_at IS NULL
         LEFT JOIN sections sec ON sec.id = e.section_id
         LEFT JOIN classes c ON c.id = sec.class_id
         WHERE s.school_id = $1 AND s.deleted_at IS NULL AND s.admission_date BETWEEN $3 AND $4
           AND ($5::uuid IS NULL OR sec.class_id = $5)
         ORDER BY s.admission_date, c.level NULLS LAST, lower(s.full_name)",
    )
    .bind(school_id)
    .bind(academic_year_id)
    .bind(from)
    .bind(to)
    .bind(class_id)
    .fetch_all(conn)
    .await?)
}

/// Students who left or graduated between two dates (inclusive).
pub async fn withdrawals(
    conn: &mut PgConnection,
    school_id: Uuid,
    academic_year_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
    class_id: Option<Uuid>,
) -> DbResult<Vec<MovementRow>> {
    Ok(sqlx::query_as::<_, MovementRow>(
        "SELECT s.student_number, s.full_name, s.father_name, s.gender, c.name AS class_name, c.level AS class_level,
                sec.name AS section_name, s.left_on AS on_date, s.status, s.left_reason AS reason
         FROM students s
         LEFT JOIN enrollments e ON e.student_id = s.id AND e.academic_year_id = $2 AND e.deleted_at IS NULL
         LEFT JOIN sections sec ON sec.id = e.section_id
         LEFT JOIN classes c ON c.id = sec.class_id
         WHERE s.school_id = $1 AND s.deleted_at IS NULL AND s.status <> 'active'
           AND s.left_on IS NOT NULL AND s.left_on BETWEEN $3 AND $4
           AND ($5::uuid IS NULL OR sec.class_id = $5)
         ORDER BY s.left_on, c.level NULLS LAST, lower(s.full_name)",
    )
    .bind(school_id)
    .bind(academic_year_id)
    .bind(from)
    .bind(to)
    .bind(class_id)
    .fetch_all(conn)
    .await?)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ContactRow {
    pub student_number: String,
    pub full_name: String,
    pub class_name: Option<String>,
    pub class_level: Option<i32>,
    pub section_name: Option<String>,
    pub roll_no: Option<i32>,
    pub guardian_name: String,
    pub relationship: String,
    pub is_primary: bool,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub cnic: Option<String>,
    pub occupation: String,
    pub address: String,
}

/// One row per guardian link for the students in scope (active students only).
pub async fn guardian_contacts(
    conn: &mut PgConnection,
    school_id: Uuid,
    f: &StudentReportFilter,
) -> DbResult<Vec<ContactRow>> {
    Ok(sqlx::query_as::<_, ContactRow>(
        "SELECT s.student_number, s.full_name, c.name AS class_name, c.level AS class_level, sec.name AS section_name,
                e.roll_no, g.full_name AS guardian_name, sg.relationship, sg.is_primary, g.phone, g.email, g.cnic,
                g.occupation, g.address
         FROM students s
         LEFT JOIN enrollments e ON e.student_id = s.id AND e.academic_year_id = $2 AND e.deleted_at IS NULL
         LEFT JOIN sections sec ON sec.id = e.section_id
         LEFT JOIN classes c ON c.id = sec.class_id
         JOIN student_guardians sg ON sg.student_id = s.id AND sg.deleted_at IS NULL
         JOIN guardians g ON g.id = sg.guardian_id AND g.deleted_at IS NULL
         WHERE s.school_id = $1 AND s.deleted_at IS NULL AND s.status = $5
           AND ($3::uuid IS NULL OR sec.class_id = $3)
           AND ($4::uuid IS NULL OR e.section_id = $4)
           AND ($6::uuid[] IS NULL OR e.section_id = ANY($6))
         ORDER BY c.level NULLS LAST, c.name, sec.name, e.roll_no NULLS LAST, lower(s.full_name), sg.is_primary DESC, g.full_name",
    )
    .bind(school_id)
    .bind(f.academic_year_id)
    .bind(f.class_id)
    .bind(f.section_id)
    .bind(f.status.as_deref().unwrap_or("active"))
    .bind(f.section_scope.as_deref())
    .fetch_all(conn)
    .await?)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BirthdayRow {
    pub student_number: String,
    pub full_name: String,
    pub father_name: String,
    pub date_of_birth: NaiveDate,
    pub class_name: Option<String>,
    pub class_level: Option<i32>,
    pub section_name: Option<String>,
    pub roll_no: Option<i32>,
}

/// Active students born in a calendar month (1–12), by day.
pub async fn birthdays(
    conn: &mut PgConnection,
    school_id: Uuid,
    academic_year_id: Uuid,
    month: u32,
    class_id: Option<Uuid>,
    section_scope: Option<&[Uuid]>,
) -> DbResult<Vec<BirthdayRow>> {
    Ok(sqlx::query_as::<_, BirthdayRow>(
        "SELECT s.student_number, s.full_name, s.father_name, s.date_of_birth, c.name AS class_name, c.level AS class_level,
                sec.name AS section_name, e.roll_no
         FROM students s
         LEFT JOIN enrollments e ON e.student_id = s.id AND e.academic_year_id = $2 AND e.deleted_at IS NULL
         LEFT JOIN sections sec ON sec.id = e.section_id
         LEFT JOIN classes c ON c.id = sec.class_id
         WHERE s.school_id = $1 AND s.deleted_at IS NULL AND s.status = 'active'
           AND EXTRACT(MONTH FROM s.date_of_birth) = $3
           AND ($4::uuid IS NULL OR sec.class_id = $4)
           AND ($5::uuid[] IS NULL OR e.section_id = ANY($5))
         ORDER BY EXTRACT(DAY FROM s.date_of_birth), c.level NULLS LAST, lower(s.full_name)",
    )
    .bind(school_id)
    .bind(academic_year_id)
    .bind(month as i32)
    .bind(class_id)
    .bind(section_scope)
    .fetch_all(conn)
    .await?)
}

/// Active students missing data the office usually needs. Only students with at least one
/// gap are returned.
pub async fn missing_data(
    conn: &mut PgConnection,
    school_id: Uuid,
    f: &StudentReportFilter,
) -> DbResult<Vec<StudentReportRow>> {
    Ok(sqlx::query_as::<_, StudentReportRow>(sqlx::AssertSqlSafe(format!(
        "{STUDENT_SELECT} {STUDENT_BASE}
           AND (s.cnic IS NULL OR s.cnic = '' OR s.photo IS NULL OR pg.full_name IS NULL
                OR pg.phone IS NULL OR pg.phone = '' OR e.roll_no IS NULL OR e.id IS NULL OR s.address = '')
         ORDER BY c.level NULLS LAST, c.name, sec.name, e.roll_no NULLS LAST, lower(s.full_name), s.id"
    )))
    .bind(school_id)
    .bind(f.academic_year_id)
    .bind(f.class_id)
    .bind(f.section_id)
    .bind("active")
    .bind(f.section_scope.as_deref())
    .fetch_all(conn)
    .await?)
}
