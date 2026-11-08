//! Read queries behind the read-only API (Standard stage 9): plain rows for other systems,
//! paged where the set can be large.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::DbResult;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RoStudent {
    pub id: Uuid,
    pub student_number: String,
    pub full_name: String,
    pub father_name: String,
    pub gender: String,
    pub date_of_birth: NaiveDate,
    pub status: String,
    pub admission_date: NaiveDate,
    pub class_id: Option<Uuid>,
    pub class_name: Option<String>,
    pub section_id: Option<Uuid>,
    pub section_name: Option<String>,
    pub roll_no: Option<i32>,
    pub updated_at: DateTime<Utc>,
}

/// Students of the school with their enrolment in the year; paged.
#[allow(clippy::too_many_arguments)]
pub async fn students(
    conn: &mut PgConnection,
    school_id: Uuid,
    academic_year_id: Uuid,
    section_id: Option<Uuid>,
    status: Option<&str>,
    updated_since: Option<DateTime<Utc>>,
    page: i64,
    page_size: i64,
) -> DbResult<(Vec<RoStudent>, i64)> {
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM students s
         LEFT JOIN enrollments e ON e.student_id = s.id AND e.academic_year_id = $2 AND e.deleted_at IS NULL
         WHERE s.school_id = $1 AND s.deleted_at IS NULL
           AND ($3::uuid IS NULL OR e.section_id = $3)
           AND ($4::text IS NULL OR s.status = $4)
           AND ($5::timestamptz IS NULL OR s.updated_at >= $5)",
    )
    .bind(school_id)
    .bind(academic_year_id)
    .bind(section_id)
    .bind(status)
    .bind(updated_since)
    .fetch_one(&mut *conn)
    .await?;
    let rows = sqlx::query_as::<_, RoStudent>(
        "SELECT s.id, s.student_number, s.full_name, s.father_name, s.gender, s.date_of_birth, s.status, s.admission_date,
                sec.class_id, c.name AS class_name, e.section_id, sec.name AS section_name, e.roll_no, s.updated_at
         FROM students s
         LEFT JOIN enrollments e ON e.student_id = s.id AND e.academic_year_id = $2 AND e.deleted_at IS NULL
         LEFT JOIN sections sec ON sec.id = e.section_id
         LEFT JOIN classes c ON c.id = sec.class_id
         WHERE s.school_id = $1 AND s.deleted_at IS NULL
           AND ($3::uuid IS NULL OR e.section_id = $3)
           AND ($4::text IS NULL OR s.status = $4)
           AND ($5::timestamptz IS NULL OR s.updated_at >= $5)
         ORDER BY s.student_number LIMIT $6 OFFSET $7",
    )
    .bind(school_id)
    .bind(academic_year_id)
    .bind(section_id)
    .bind(status)
    .bind(updated_since)
    .bind(page_size)
    .bind((page - 1) * page_size)
    .fetch_all(conn)
    .await?;
    Ok((rows, total))
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RoSection {
    pub id: Uuid,
    pub class_id: Uuid,
    pub class_name: String,
    pub class_level: i32,
    pub name: String,
    pub students: i64,
}

pub async fn sections(
    conn: &mut PgConnection,
    school_id: Uuid,
    academic_year_id: Uuid,
) -> DbResult<Vec<RoSection>> {
    Ok(sqlx::query_as::<_, RoSection>(
        "SELECT sec.id, sec.class_id, c.name AS class_name, c.level AS class_level, sec.name,
                (SELECT count(*) FROM enrollments e JOIN students s ON s.id = e.student_id AND s.deleted_at IS NULL AND s.status = 'active'
                 WHERE e.section_id = sec.id AND e.deleted_at IS NULL) AS students
         FROM sections sec JOIN classes c ON c.id = sec.class_id
         WHERE sec.school_id = $1 AND sec.academic_year_id = $2 AND sec.deleted_at IS NULL
         ORDER BY c.level, c.name, sec.name",
    )
    .bind(school_id)
    .bind(academic_year_id)
    .fetch_all(conn)
    .await?)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RoAttendanceDay {
    pub section_id: Uuid,
    pub class_name: String,
    pub section_name: String,
    pub date: NaiveDate,
    pub marked: bool,
    pub present: i64,
    pub absent: i64,
    pub late: i64,
    pub leave: i64,
}

/// Every section of the year with its counts for a day (unmarked sections have zeros).
pub async fn attendance_daily(
    conn: &mut PgConnection,
    school_id: Uuid,
    academic_year_id: Uuid,
    date: NaiveDate,
    section_id: Option<Uuid>,
) -> DbResult<Vec<RoAttendanceDay>> {
    Ok(sqlx::query_as::<_, RoAttendanceDay>(
        "SELECT sec.id AS section_id, c.name AS class_name, sec.name AS section_name, $3::date AS date,
                sh.id IS NOT NULL AS marked,
                coalesce(count(a.id) FILTER (WHERE a.status = 'present'), 0) AS present,
                coalesce(count(a.id) FILTER (WHERE a.status = 'absent'), 0) AS absent,
                coalesce(count(a.id) FILTER (WHERE a.status = 'late'), 0) AS late,
                coalesce(count(a.id) FILTER (WHERE a.status = 'leave'), 0) AS leave
         FROM sections sec JOIN classes c ON c.id = sec.class_id
         LEFT JOIN attendance_sheets sh ON sh.section_id = sec.id AND sh.date = $3 AND sh.deleted_at IS NULL
         LEFT JOIN attendance_entries a ON a.sheet_id = sh.id AND a.deleted_at IS NULL
         WHERE sec.school_id = $1 AND sec.academic_year_id = $2 AND sec.deleted_at IS NULL
           AND ($4::uuid IS NULL OR sec.id = $4)
         GROUP BY sec.id, c.name, c.level, sec.name, sh.id
         ORDER BY c.level, c.name, sec.name",
    )
    .bind(school_id)
    .bind(academic_year_id)
    .bind(date)
    .bind(section_id)
    .fetch_all(conn)
    .await?)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RoBalance {
    pub student_id: Uuid,
    pub student_number: String,
    pub full_name: String,
    pub class_name: Option<String>,
    pub section_name: Option<String>,
    pub open_vouchers: i64,
    pub balance: Decimal,
}

/// Students with an unpaid or partly paid voucher and what they owe.
pub async fn fee_balances(
    conn: &mut PgConnection,
    school_id: Uuid,
    academic_year_id: Uuid,
    section_id: Option<Uuid>,
) -> DbResult<Vec<RoBalance>> {
    Ok(sqlx::query_as::<_, RoBalance>(
        "SELECT s.id AS student_id, s.student_number, s.full_name, c.name AS class_name, sec.name AS section_name,
                count(v.id) AS open_vouchers, sum(v.total_due + v.late_fee - v.net_paid)::numeric(14,2) AS balance
         FROM fee_vouchers v
         JOIN students s ON s.id = v.student_id AND s.deleted_at IS NULL
         LEFT JOIN enrollments e ON e.student_id = s.id AND e.academic_year_id = $2 AND e.deleted_at IS NULL
         LEFT JOIN sections sec ON sec.id = e.section_id
         LEFT JOIN classes c ON c.id = sec.class_id
         WHERE v.school_id = $1 AND v.status IN ('unpaid', 'partial') AND v.deleted_at IS NULL
           AND ($3::uuid IS NULL OR e.section_id = $3)
         GROUP BY s.id, s.student_number, s.full_name, c.name, c.level, sec.name
         HAVING sum(v.total_due + v.late_fee - v.net_paid) > 0
         ORDER BY c.level, c.name, sec.name, s.student_number",
    )
    .bind(school_id)
    .bind(academic_year_id)
    .bind(section_id)
    .fetch_all(conn)
    .await?)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RoAssessment {
    pub id: Uuid,
    pub name: String,
    pub position: i32,
}

pub async fn assessments(
    conn: &mut PgConnection,
    academic_year_id: Uuid,
) -> DbResult<Vec<RoAssessment>> {
    Ok(sqlx::query_as::<_, RoAssessment>(
        "SELECT id, name, position FROM assessment_types WHERE academic_year_id = $1 AND deleted_at IS NULL ORDER BY position, name",
    )
    .bind(academic_year_id)
    .fetch_all(conn)
    .await?)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RoResult {
    pub student_id: Uuid,
    pub student_number: String,
    pub full_name: String,
    pub subject_id: Uuid,
    pub subject_name: String,
    pub subject_code: String,
    pub max_marks: Option<Decimal>,
    pub obtained: Option<Decimal>,
    pub is_absent: bool,
}

/// Approved marks of an assessment in a section (one row per student and subject).
pub async fn results(
    conn: &mut PgConnection,
    school_id: Uuid,
    assessment_type_id: Uuid,
    section_id: Uuid,
) -> DbResult<Vec<RoResult>> {
    Ok(sqlx::query_as::<_, RoResult>(
        "SELECT m.student_id, s.student_number, s.full_name, sh.subject_id, sub.name AS subject_name, sub.code AS subject_code,
                mm.max_marks, m.obtained, m.is_absent
         FROM mark_sheets sh
         JOIN marks m ON m.sheet_id = sh.id AND m.deleted_at IS NULL
         JOIN students s ON s.id = m.student_id AND s.deleted_at IS NULL
         JOIN subjects sub ON sub.id = sh.subject_id
         JOIN sections sec ON sec.id = sh.section_id
         LEFT JOIN assessment_max_marks mm ON mm.assessment_type_id = sh.assessment_type_id AND mm.class_id = sec.class_id
              AND mm.subject_id = sh.subject_id AND mm.deleted_at IS NULL
         WHERE sh.school_id = $1 AND sh.assessment_type_id = $2 AND sh.section_id = $3 AND sh.status = 'approved' AND sh.deleted_at IS NULL
         ORDER BY s.student_number, sub.name",
    )
    .bind(school_id)
    .bind(assessment_type_id)
    .bind(section_id)
    .fetch_all(conn)
    .await?)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RoTimetableCard {
    pub day: i16,
    pub period: i16,
    pub length: i16,
    pub shift_name: String,
    pub subject_name: String,
    pub subject_code: String,
    pub teachers: Vec<String>,
    pub room_name: Option<String>,
}

/// The published timetable of a section: one row per card.
pub async fn timetable(
    conn: &mut PgConnection,
    school_id: Uuid,
    academic_year_id: Uuid,
    section_id: Uuid,
) -> DbResult<Option<Vec<RoTimetableCard>>> {
    let published: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM timetables WHERE school_id = $1 AND academic_year_id = $2 AND status = 'published' AND deleted_at IS NULL",
    )
    .bind(school_id)
    .bind(academic_year_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(tt) = published else {
        return Ok(None);
    };
    Ok(Some(
        sqlx::query_as::<_, RoTimetableCard>(
            "SELECT c.day, c.period, c.length, sh.name AS shift_name, sub.name AS subject_name, sub.code AS subject_code,
                    coalesce((SELECT array_agg(st.full_name ORDER BY st.full_name) FROM staff st WHERE st.id = ANY(l.teacher_ids)), '{}') AS teachers,
                    r.name AS room_name
             FROM timetable_cards c
             JOIN timetable_lessons l ON l.id = c.lesson_id
             JOIN subjects sub ON sub.id = l.subject_id
             JOIN timetable_shifts sh ON sh.id = c.shift_id
             LEFT JOIN rooms r ON r.id = c.room_id
             WHERE c.timetable_id = $1 AND $2 = ANY(l.section_ids) AND c.deleted_at IS NULL
             ORDER BY c.day, c.period",
        )
        .bind(tt)
        .bind(section_id)
        .fetch_all(conn)
        .await?,
    ))
}
