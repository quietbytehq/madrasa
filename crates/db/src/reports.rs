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

// ---- M2 stage 2: fees -----------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ReceiptRow {
    pub paid_on: NaiveDate,
    pub receipt_no: String,
    pub student_number: String,
    pub student_name: String,
    pub class_name: Option<String>,
    pub class_level: Option<i32>,
    pub section_name: Option<String>,
    pub voucher_no: String,
    pub billing_month: NaiveDate,
    pub method: String,
    pub reference: Option<String>,
    pub amount: rust_decimal::Decimal,
    pub is_reversal: bool,
    pub received_by: String,
}

/// Every receipt (reversals included, as negative amounts) between two dates, by day.
pub async fn receipts_between(
    conn: &mut PgConnection,
    school_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
    class_id: Option<Uuid>,
) -> DbResult<Vec<ReceiptRow>> {
    Ok(sqlx::query_as::<_, ReceiptRow>(
        "SELECT r.paid_on, r.receipt_no, s.student_number, s.full_name AS student_name,
                c.name AS class_name, c.level AS class_level, sec.name AS section_name,
                v.voucher_no, v.billing_month, r.method, r.reference, r.amount,
                (r.reverses_id IS NOT NULL) AS is_reversal, u.full_name AS received_by
         FROM fee_receipts r
         JOIN fee_vouchers v ON v.id = r.voucher_id
         JOIN students s ON s.id = r.student_id
         LEFT JOIN sections sec ON sec.id = v.section_id
         LEFT JOIN classes c ON c.id = sec.class_id
         JOIN users u ON u.id = r.received_by
         WHERE r.school_id = $1 AND r.paid_on BETWEEN $2 AND $3
           AND ($4::uuid IS NULL OR sec.class_id = $4)
         ORDER BY r.paid_on, r.created_at",
    )
    .bind(school_id)
    .bind(from)
    .bind(to)
    .bind(class_id)
    .fetch_all(conn)
    .await?)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MonthCollection {
    pub month: NaiveDate,
    pub receipts: i64,
    pub reversals: i64,
    pub amount: rust_decimal::Decimal,
}

/// Net collection per calendar month of payment between two dates.
pub async fn collection_by_month(
    conn: &mut PgConnection,
    school_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> DbResult<Vec<MonthCollection>> {
    Ok(sqlx::query_as::<_, MonthCollection>(
        "SELECT date_trunc('month', paid_on)::date AS month,
                count(*) FILTER (WHERE reverses_id IS NULL) AS receipts,
                count(*) FILTER (WHERE reverses_id IS NOT NULL) AS reversals,
                coalesce(sum(amount), 0)::numeric(14,2) AS amount
         FROM fee_receipts WHERE school_id = $1 AND paid_on BETWEEN $2 AND $3
         GROUP BY 1 ORDER BY 1",
    )
    .bind(school_id)
    .bind(from)
    .bind(to)
    .fetch_all(conn)
    .await?)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ConcessionRow {
    pub student_number: String,
    pub student_name: String,
    pub class_name: Option<String>,
    pub class_level: Option<i32>,
    pub section_name: Option<String>,
    pub kind: String,
    pub percent: Option<rust_decimal::Decimal>,
    pub amount: Option<rust_decimal::Decimal>,
    pub reason: String,
    pub approved_by: String,
    pub valid_from: NaiveDate,
    pub valid_to: Option<NaiveDate>,
}

/// Concessions valid at any point of a date range.
pub async fn concessions_register(
    conn: &mut PgConnection,
    school_id: Uuid,
    academic_year_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
    class_id: Option<Uuid>,
) -> DbResult<Vec<ConcessionRow>> {
    Ok(sqlx::query_as::<_, ConcessionRow>(
        "SELECT s.student_number, s.full_name AS student_name, c.name AS class_name, c.level AS class_level,
                sec.name AS section_name, fc.kind, fc.percent, fc.amount, fc.reason, u.full_name AS approved_by,
                fc.valid_from, fc.valid_to
         FROM fee_concessions fc
         JOIN students s ON s.id = fc.student_id AND s.deleted_at IS NULL
         JOIN users u ON u.id = fc.approved_by
         LEFT JOIN enrollments e ON e.student_id = s.id AND e.academic_year_id = $2 AND e.deleted_at IS NULL
         LEFT JOIN sections sec ON sec.id = e.section_id
         LEFT JOIN classes c ON c.id = sec.class_id
         WHERE fc.school_id = $1 AND fc.deleted_at IS NULL
           AND fc.valid_from <= $4 AND (fc.valid_to IS NULL OR fc.valid_to >= $3)
           AND ($5::uuid IS NULL OR sec.class_id = $5)
         ORDER BY c.level NULLS LAST, sec.name, lower(s.full_name), fc.valid_from",
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
pub struct LateFeeRow {
    pub voucher_no: String,
    pub billing_month: NaiveDate,
    pub due_date: NaiveDate,
    pub student_number: String,
    pub student_name: String,
    pub class_name: Option<String>,
    pub class_level: Option<i32>,
    pub section_name: Option<String>,
    pub late_fee: rust_decimal::Decimal,
    pub late_fee_waived: bool,
    pub waiver_reason: Option<String>,
    pub waived_by: Option<String>,
    pub status: String,
}

/// Vouchers of a billing-month range that carry a late fee or had one waived.
pub async fn late_fees(
    conn: &mut PgConnection,
    school_id: Uuid,
    from_month: NaiveDate,
    to_month: NaiveDate,
    class_id: Option<Uuid>,
) -> DbResult<Vec<LateFeeRow>> {
    Ok(sqlx::query_as::<_, LateFeeRow>(
        "SELECT v.voucher_no, v.billing_month, v.due_date, s.student_number, s.full_name AS student_name,
                c.name AS class_name, c.level AS class_level, sec.name AS section_name,
                v.late_fee, v.late_fee_waived, v.waiver_reason, u.full_name AS waived_by, v.status
         FROM fee_vouchers v
         JOIN students s ON s.id = v.student_id
         LEFT JOIN sections sec ON sec.id = v.section_id
         LEFT JOIN classes c ON c.id = sec.class_id
         LEFT JOIN users u ON u.id = v.waived_by
         WHERE v.school_id = $1 AND v.deleted_at IS NULL AND v.billing_month BETWEEN $2 AND $3
           AND (v.late_fee > 0 OR v.late_fee_waived)
           AND ($4::uuid IS NULL OR sec.class_id = $4)
         ORDER BY v.billing_month, c.level NULLS LAST, sec.name, lower(s.full_name)",
    )
    .bind(school_id)
    .bind(from_month)
    .bind(to_month)
    .bind(class_id)
    .fetch_all(conn)
    .await?)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct HeadRow {
    pub head: String,
    pub vouchers: i64,
    pub billed: rust_decimal::Decimal,
    /// Payments allocated to the head in proportion to its share of each voucher's gross.
    pub collected: rust_decimal::Decimal,
}

/// Billed and (proportionally) collected amounts per fee head for the vouchers of a year.
/// Payments are not recorded per head, so each voucher's net payment is split across its
/// lines in proportion to their amounts.
pub async fn head_wise(
    conn: &mut PgConnection,
    school_id: Uuid,
    academic_year_id: Uuid,
) -> DbResult<Vec<HeadRow>> {
    Ok(sqlx::query_as::<_, HeadRow>(
        "SELECT coalesce(h.name, l.description) AS head, count(DISTINCT v.id) AS vouchers,
                coalesce(sum(l.amount), 0)::numeric(14,2) AS billed,
                coalesce(sum(CASE WHEN v.gross > 0 THEN v.net_paid * l.amount / v.gross ELSE 0 END), 0)::numeric(14,2) AS collected
         FROM fee_voucher_lines l
         JOIN fee_vouchers v ON v.id = l.voucher_id AND v.deleted_at IS NULL AND v.status <> 'cancelled'
         LEFT JOIN fee_heads h ON h.id = l.fee_head_id
         WHERE v.school_id = $1 AND v.academic_year_id = $2
         GROUP BY coalesce(h.name, l.description), coalesce(h.position, 1000)
         ORDER BY coalesce(h.position, 1000), 1",
    )
    .bind(school_id)
    .bind(academic_year_id)
    .fetch_all(conn)
    .await?)
}

// ---- M2 stage 2: attendance ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SectionAttendanceRow {
    pub section_id: Uuid,
    pub class_name: String,
    pub class_level: i32,
    pub section_name: String,
    pub enrolled: i64,
    pub days_marked: i64,
    pub present: i64,
    pub absent: i64,
    pub late: i64,
    pub leave: i64,
}

/// Attendance totals per section over a date range (entries of submitted or draft sheets).
pub async fn attendance_by_section(
    conn: &mut PgConnection,
    school_id: Uuid,
    academic_year_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
    class_id: Option<Uuid>,
    section_scope: Option<&[Uuid]>,
) -> DbResult<Vec<SectionAttendanceRow>> {
    Ok(sqlx::query_as::<_, SectionAttendanceRow>(
        "SELECT sec.id AS section_id, c.name AS class_name, c.level AS class_level, sec.name AS section_name,
                (SELECT count(*) FROM enrollments e JOIN students s ON s.id = e.student_id AND s.deleted_at IS NULL AND s.status = 'active'
                  WHERE e.section_id = sec.id AND e.deleted_at IS NULL) AS enrolled,
                (SELECT count(DISTINCT sh.date) FROM attendance_sheets sh
                  WHERE sh.section_id = sec.id AND sh.deleted_at IS NULL AND sh.date BETWEEN $3 AND $4) AS days_marked,
                count(a.id) FILTER (WHERE a.status = 'present') AS present,
                count(a.id) FILTER (WHERE a.status = 'absent') AS absent,
                count(a.id) FILTER (WHERE a.status = 'late') AS late,
                count(a.id) FILTER (WHERE a.status = 'leave') AS leave
         FROM sections sec
         JOIN classes c ON c.id = sec.class_id
         LEFT JOIN attendance_sheets sh ON sh.section_id = sec.id AND sh.deleted_at IS NULL AND sh.date BETWEEN $3 AND $4
         LEFT JOIN attendance_entries a ON a.sheet_id = sh.id AND a.deleted_at IS NULL
         WHERE sec.school_id = $1 AND sec.academic_year_id = $2 AND sec.deleted_at IS NULL
           AND ($5::uuid IS NULL OR sec.class_id = $5)
           AND ($6::uuid[] IS NULL OR sec.id = ANY($6))
         GROUP BY sec.id, c.name, c.level, sec.name
         ORDER BY c.level, c.name, sec.name",
    )
    .bind(school_id)
    .bind(academic_year_id)
    .bind(from)
    .bind(to)
    .bind(class_id)
    .bind(section_scope)
    .fetch_all(conn)
    .await?)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AbsenteeRangeRow {
    pub date: NaiveDate,
    pub student_number: String,
    pub full_name: String,
    pub father_name: String,
    pub class_name: String,
    pub class_level: i32,
    pub section_name: String,
    pub roll_no: Option<i32>,
    pub status: String,
    pub note: Option<String>,
    pub guardian_phone: Option<String>,
}

/// Students not present (absent, late, leave) on each day of a range.
pub async fn absentees_between(
    conn: &mut PgConnection,
    school_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
    class_id: Option<Uuid>,
    section_id: Option<Uuid>,
    section_scope: Option<&[Uuid]>,
) -> DbResult<Vec<AbsenteeRangeRow>> {
    Ok(sqlx::query_as::<_, AbsenteeRangeRow>(
        "SELECT a.date, s.student_number, s.full_name, s.father_name, c.name AS class_name, c.level AS class_level,
                sec.name AS section_name, e.roll_no, a.status, a.note,
                (SELECT g.phone FROM student_guardians sg JOIN guardians g ON g.id = sg.guardian_id
                  WHERE sg.student_id = s.id AND sg.deleted_at IS NULL ORDER BY sg.is_primary DESC LIMIT 1) AS guardian_phone
         FROM attendance_entries a
         JOIN attendance_sheets sh ON sh.id = a.sheet_id AND sh.deleted_at IS NULL
         JOIN sections sec ON sec.id = sh.section_id
         JOIN classes c ON c.id = sec.class_id
         JOIN students s ON s.id = a.student_id
         LEFT JOIN enrollments e ON e.student_id = s.id AND e.section_id = sec.id AND e.deleted_at IS NULL
         WHERE a.school_id = $1 AND a.deleted_at IS NULL AND a.status <> 'present' AND a.date BETWEEN $2 AND $3
           AND ($4::uuid IS NULL OR sec.class_id = $4)
           AND ($5::uuid IS NULL OR sec.id = $5)
           AND ($6::uuid[] IS NULL OR sec.id = ANY($6))
         ORDER BY a.date, c.level, sec.name, e.roll_no NULLS LAST, lower(s.full_name)",
    )
    .bind(school_id)
    .bind(from)
    .bind(to)
    .bind(class_id)
    .bind(section_id)
    .bind(section_scope)
    .fetch_all(conn)
    .await?)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct StudentAttendanceRow {
    pub student_number: String,
    pub full_name: String,
    pub father_name: String,
    pub class_name: String,
    pub class_level: i32,
    pub section_name: String,
    pub roll_no: Option<i32>,
    pub present: i64,
    pub absent: i64,
    pub late: i64,
    pub leave: i64,
    pub guardian_phone: Option<String>,
}

/// Per-student counts over a range for the students enrolled in the sections in scope.
// The filters mirror the report's parameters; a struct would add noise for one caller.
#[allow(clippy::too_many_arguments)]
pub async fn student_attendance_totals(
    conn: &mut PgConnection,
    school_id: Uuid,
    academic_year_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
    class_id: Option<Uuid>,
    section_id: Option<Uuid>,
    section_scope: Option<&[Uuid]>,
) -> DbResult<Vec<StudentAttendanceRow>> {
    Ok(sqlx::query_as::<_, StudentAttendanceRow>(
        "SELECT s.student_number, s.full_name, s.father_name, c.name AS class_name, c.level AS class_level,
                sec.name AS section_name, e.roll_no,
                count(a.id) FILTER (WHERE a.status = 'present') AS present,
                count(a.id) FILTER (WHERE a.status = 'absent') AS absent,
                count(a.id) FILTER (WHERE a.status = 'late') AS late,
                count(a.id) FILTER (WHERE a.status = 'leave') AS leave,
                (SELECT g.phone FROM student_guardians sg JOIN guardians g ON g.id = sg.guardian_id
                  WHERE sg.student_id = s.id AND sg.deleted_at IS NULL ORDER BY sg.is_primary DESC LIMIT 1) AS guardian_phone
         FROM enrollments e
         JOIN students s ON s.id = e.student_id AND s.deleted_at IS NULL AND s.status = 'active'
         JOIN sections sec ON sec.id = e.section_id
         JOIN classes c ON c.id = sec.class_id
         LEFT JOIN attendance_sheets sh ON sh.section_id = sec.id AND sh.deleted_at IS NULL AND sh.date BETWEEN $3 AND $4
         LEFT JOIN attendance_entries a ON a.sheet_id = sh.id AND a.student_id = s.id AND a.deleted_at IS NULL
         WHERE e.school_id = $1 AND e.academic_year_id = $2 AND e.deleted_at IS NULL
           AND ($5::uuid IS NULL OR sec.class_id = $5)
           AND ($6::uuid IS NULL OR sec.id = $6)
           AND ($7::uuid[] IS NULL OR sec.id = ANY($7))
         GROUP BY s.id, s.student_number, s.full_name, s.father_name, c.name, c.level, sec.name, e.roll_no
         ORDER BY c.level, c.name, sec.name, e.roll_no NULLS LAST, lower(s.full_name)",
    )
    .bind(school_id)
    .bind(academic_year_id)
    .bind(from)
    .bind(to)
    .bind(class_id)
    .bind(section_id)
    .bind(section_scope)
    .fetch_all(conn)
    .await?)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct StaffMonthRow {
    pub staff_number: String,
    pub full_name: String,
    pub designation: String,
    pub present: i64,
    pub absent: i64,
    pub late: i64,
    pub leave: i64,
}

/// Staff attendance counts over a range, every staff member listed.
pub async fn staff_attendance_totals(
    conn: &mut PgConnection,
    school_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> DbResult<Vec<StaffMonthRow>> {
    Ok(sqlx::query_as::<_, StaffMonthRow>(
        "SELECT st.staff_number, st.full_name, st.designation,
                count(a.id) FILTER (WHERE a.status = 'present') AS present,
                count(a.id) FILTER (WHERE a.status = 'absent') AS absent,
                count(a.id) FILTER (WHERE a.status = 'late') AS late,
                count(a.id) FILTER (WHERE a.status = 'leave') AS leave
         FROM staff st
         LEFT JOIN staff_attendance a ON a.staff_id = st.id AND a.deleted_at IS NULL AND a.date BETWEEN $2 AND $3
         WHERE st.school_id = $1 AND st.deleted_at IS NULL
         GROUP BY st.id, st.staff_number, st.full_name, st.designation
         ORDER BY st.full_name",
    )
    .bind(school_id)
    .bind(from)
    .bind(to)
    .fetch_all(conn)
    .await?)
}
