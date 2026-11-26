//! "Export everything": the whole school as one workbook, so a school can always leave with
//! its data. The records people know (students, guardians, staff, fees, attendance, marks)
//! come as readable sheets with names in place of internal ids; every other table the school
//! owns follows as it is stored. Secrets are never included: no password hashes, sessions,
//! keys or tokens.
//!
//! Sheets are written to disk as they are read from the database, so a large school does not
//! have to fit in memory; a sheet that would pass Excel's row limit continues on the next.

use futures_util::StreamExt;
use rust_xlsxwriter::{Format, Workbook, Worksheet};
use sqlx::{Column, Row};
use uuid::Uuid;

use crate::ApiError;

/// Excel holds 1,048,576 rows on a sheet; stop a little before, heading included.
const ROWS_PER_SHEET: u32 = 1_000_000;

/// The readable sheets: name, the tables it covers, and its query (`$1` is the school).
/// Every column is selected as text under the heading it should have.
const SHEETS: &[(&str, &[&str], &str)] = &[
    (
        "Students",
        &["students", "enrollments"],
        r#"SELECT s.student_number AS "Student number", s.legacy_id AS "Legacy ID", s.full_name AS "Student name",
                  s.father_name AS "Father's name", s.date_of_birth::text AS "Date of birth", s.gender AS "Gender",
                  s.cnic AS "B-Form / ID number", s.admission_date::text AS "Admission date", s.status AS "Status",
                  s.left_on::text AS "Left on", s.left_reason AS "Reason for leaving",
                  y.name AS "Academic year", c.name AS "Class", sec.name AS "Section", e.roll_no::text AS "Roll no.",
                  s.address AS "Address", s.board_roll_no AS "Board roll no.", s.board_registration_no AS "Board registration no.",
                  NULLIF(s.custom::text, '{}') AS "Other details"
           FROM students s
           LEFT JOIN enrollments e ON e.student_id = s.id AND e.deleted_at IS NULL
           LEFT JOIN academic_years y ON y.id = e.academic_year_id
           LEFT JOIN sections sec ON sec.id = e.section_id
           LEFT JOIN classes c ON c.id = sec.class_id
           WHERE s.school_id = $1 AND s.deleted_at IS NULL
           ORDER BY s.student_number, y.start_date"#,
    ),
    (
        "Guardians",
        &["guardians", "student_guardians"],
        r#"SELECT g.full_name AS "Guardian name", g.phone AS "Phone", g.email AS "Email", g.cnic AS "CNIC / ID number",
                  g.occupation AS "Occupation", g.address AS "Address",
                  s.student_number AS "Student number", s.full_name AS "Student name",
                  sg.relationship AS "Relationship", CASE WHEN sg.is_primary THEN 'yes' WHEN sg.id IS NOT NULL THEN 'no' END AS "Primary contact"
           FROM guardians g
           LEFT JOIN student_guardians sg ON sg.guardian_id = g.id AND sg.deleted_at IS NULL
           LEFT JOIN students s ON s.id = sg.student_id AND s.deleted_at IS NULL
           WHERE g.school_id = $1 AND g.deleted_at IS NULL
           ORDER BY g.full_name, s.student_number"#,
    ),
    (
        "Staff",
        &["staff"],
        r#"SELECT st.staff_number AS "Staff number", st.full_name AS "Name", st.father_name AS "Father's name",
                  st.gender AS "Gender", st.phone AS "Phone", st.email AS "Email", st.cnic AS "CNIC / ID number",
                  st.designation AS "Designation", st.joined_on::text AS "Joined on", st.status AS "Status",
                  u.username AS "Login", NULLIF(st.custom::text, '{}') AS "Other details"
           FROM staff st LEFT JOIN users u ON u.id = st.user_id
           WHERE st.school_id = $1 AND st.deleted_at IS NULL ORDER BY st.staff_number"#,
    ),
    (
        "Logins",
        &["users", "user_roles"],
        r#"SELECT u.username AS "Username", u.full_name AS "Name", u.email AS "Email",
                  (SELECT string_agg(r.name, ', ' ORDER BY r.name) FROM user_roles ur JOIN roles r ON r.id = ur.role_id
                    WHERE ur.user_id = u.id AND ur.deleted_at IS NULL) AS "Roles",
                  CASE WHEN u.is_active THEN 'yes' ELSE 'no' END AS "Active", u.last_login_at::text AS "Last login"
           FROM users u WHERE u.school_id = $1 AND u.deleted_at IS NULL ORDER BY u.username"#,
    ),
    (
        "Classes and sections",
        &["sections"],
        r#"SELECT y.name AS "Academic year", c.name AS "Class", sec.name AS "Section", t.full_name AS "Class teacher",
                  sec.capacity::text AS "Capacity",
                  (SELECT count(*) FROM enrollments e JOIN students s ON s.id = e.student_id
                    WHERE e.section_id = sec.id AND e.deleted_at IS NULL AND s.deleted_at IS NULL)::text AS "Students"
           FROM sections sec JOIN classes c ON c.id = sec.class_id JOIN academic_years y ON y.id = sec.academic_year_id
           LEFT JOIN staff t ON t.id = sec.class_teacher_id
           WHERE sec.school_id = $1 AND sec.deleted_at IS NULL ORDER BY y.start_date, c.level, sec.name"#,
    ),
    (
        "Fee structure",
        &["fee_structures"],
        r#"SELECT y.name AS "Academic year", c.name AS "Class", h.name AS "Fee head", fs.amount::text AS "Amount"
           FROM fee_structures fs JOIN academic_years y ON y.id = fs.academic_year_id
           JOIN classes c ON c.id = fs.class_id JOIN fee_heads h ON h.id = fs.fee_head_id
           WHERE fs.school_id = $1 AND fs.deleted_at IS NULL ORDER BY y.start_date, c.level, h.position"#,
    ),
    (
        "Fee concessions",
        &["fee_concessions"],
        r#"SELECT s.student_number AS "Student number", s.full_name AS "Student name", fc.kind AS "Kind",
                  fc.percent::text AS "Percent", fc.amount::text AS "Amount", fc.reason AS "Reason",
                  fc.valid_from::text AS "From", fc.valid_to::text AS "To", u.full_name AS "Approved by"
           FROM fee_concessions fc JOIN students s ON s.id = fc.student_id LEFT JOIN users u ON u.id = fc.approved_by
           WHERE fc.school_id = $1 AND fc.deleted_at IS NULL ORDER BY s.student_number, fc.valid_from"#,
    ),
    (
        "Fee vouchers",
        &["fee_vouchers"],
        r#"SELECT v.voucher_no AS "Voucher no.", s.student_number AS "Student number", s.full_name AS "Student name",
                  to_char(v.billing_month, 'YYYY-MM') AS "Month", v.issue_date::text AS "Issued", v.due_date::text AS "Due",
                  v.gross::text AS "Fees", v.sibling_discount::text AS "Sibling discount",
                  v.concession_discount::text AS "Concession", v.arrears::text AS "Arrears", v.total_due::text AS "Total due",
                  v.late_fee::text AS "Late fee", CASE WHEN v.late_fee_waived THEN v.waiver_reason END AS "Late fee waived because",
                  v.net_paid::text AS "Paid", v.status AS "Status"
           FROM fee_vouchers v JOIN students s ON s.id = v.student_id
           WHERE v.school_id = $1 AND v.deleted_at IS NULL ORDER BY v.billing_month, v.voucher_no"#,
    ),
    (
        "Fee voucher lines",
        &["fee_voucher_lines"],
        r#"SELECT v.voucher_no AS "Voucher no.", h.name AS "Fee head", l.description AS "Description", l.amount::text AS "Amount"
           FROM fee_voucher_lines l JOIN fee_vouchers v ON v.id = l.voucher_id LEFT JOIN fee_heads h ON h.id = l.fee_head_id
           WHERE l.school_id = $1 AND l.deleted_at IS NULL AND v.deleted_at IS NULL ORDER BY v.billing_month, v.voucher_no, l.position"#,
    ),
    (
        "Fee receipts",
        &["fee_receipts"],
        r#"SELECT r.receipt_no AS "Receipt no.", r.paid_on::text AS "Paid on", v.voucher_no AS "Voucher no.",
                  s.student_number AS "Student number", s.full_name AS "Student name", r.amount::text AS "Amount",
                  r.method AS "Method", r.reference AS "Reference", r.note AS "Note",
                  o.receipt_no AS "Reverses receipt", r.reversal_reason AS "Reason for reversal", u.full_name AS "Received by"
           FROM fee_receipts r JOIN fee_vouchers v ON v.id = r.voucher_id JOIN students s ON s.id = r.student_id
           LEFT JOIN fee_receipts o ON o.id = r.reverses_id LEFT JOIN users u ON u.id = r.received_by
           WHERE r.school_id = $1 ORDER BY r.paid_on, r.receipt_no"#,
    ),
    (
        "Attendance",
        &["attendance_entries", "attendance_sheets"],
        r#"SELECT a.date::text AS "Date", c.name AS "Class", sec.name AS "Section", s.student_number AS "Student number",
                  s.full_name AS "Student name", a.status AS "Status", a.note AS "Note"
           FROM attendance_entries a JOIN attendance_sheets sh ON sh.id = a.sheet_id JOIN sections sec ON sec.id = sh.section_id
           JOIN classes c ON c.id = sec.class_id JOIN students s ON s.id = a.student_id
           WHERE a.school_id = $1 AND a.deleted_at IS NULL ORDER BY a.date, c.level, sec.name, s.student_number"#,
    ),
    (
        "Staff attendance",
        &["staff_attendance"],
        r#"SELECT a.date::text AS "Date", st.staff_number AS "Staff number", st.full_name AS "Name", a.status AS "Status",
                  a.arrived_at::text AS "Arrived at", a.note AS "Note"
           FROM staff_attendance a JOIN staff st ON st.id = a.staff_id
           WHERE a.school_id = $1 AND a.deleted_at IS NULL ORDER BY a.date, st.staff_number"#,
    ),
    (
        "Marks",
        &["marks", "mark_sheets"],
        r#"SELECT y.name AS "Academic year", at.name AS "Assessment", c.name AS "Class", sec.name AS "Section",
                  sub.name AS "Subject", s.student_number AS "Student number", s.full_name AS "Student name",
                  mm.max_marks::text AS "Maximum", m.obtained::text AS "Obtained",
                  CASE WHEN m.is_absent THEN 'yes' END AS "Absent", ms.status AS "Sheet status"
           FROM marks m JOIN mark_sheets ms ON ms.id = m.sheet_id JOIN assessment_types at ON at.id = ms.assessment_type_id
           JOIN academic_years y ON y.id = at.academic_year_id JOIN sections sec ON sec.id = ms.section_id
           JOIN classes c ON c.id = sec.class_id JOIN subjects sub ON sub.id = ms.subject_id JOIN students s ON s.id = m.student_id
           LEFT JOIN assessment_max_marks mm ON mm.assessment_type_id = at.id AND mm.class_id = c.id
                 AND mm.subject_id = sub.id AND mm.deleted_at IS NULL
           WHERE m.school_id = $1 AND m.deleted_at IS NULL
           ORDER BY y.start_date, at.position, c.level, sec.name, sub.name, s.student_number"#,
    ),
];

/// Tables that are not the school's records: logins and their secrets, the plumbing of sync
/// and offline work, and bookkeeping of imports and backups.
const NOT_EXPORTED: &[&str] = &[
    "sessions",
    "users",
    "user_roles",
    "roles",
    "permissions",
    "api_keys",
    "api_key_calls",
    "idempotency_keys",
    "sync_changes",
    "sync_field_stamps",
    "sync_overwrites",
    "sync_parked",
    "sync_state",
    "school_sequences",
    "import_batches",
    "import_batch_rows",
    "backup_runs",
    "message_reads",
    "settings",
];

/// Columns left out of the as-stored sheets: internal stamps and pictures.
const NOT_COLUMNS: &[&str] = &["school_id", "hlc", "deleted_at", "photo", "photo_mime"];

/// A setting is left out when its name or anything inside it speaks of a secret.
fn is_secret_setting(key: &str, value: &serde_json::Value) -> bool {
    fn secret(name: &str) -> bool {
        let n = name.to_lowercase();
        ["password", "secret", "token", "sealed", "private", "key"]
            .iter()
            .any(|w| n.contains(w))
            && !n.ends_with("_id")
    }
    fn inside(v: &serde_json::Value) -> bool {
        match v {
            serde_json::Value::Object(o) => o.iter().any(|(k, v)| secret(k) || inside(v)),
            serde_json::Value::Array(a) => a.iter().any(inside),
            _ => false,
        }
    }
    secret(key) || key == "node_id" || inside(value)
}

struct Writer<'a> {
    wb: &'a mut Workbook,
    bold: Format,
    name: String,
    headers: Vec<String>,
    part: u32,
    row: u32,
    total: u64,
}

impl<'a> Writer<'a> {
    fn start(wb: &'a mut Workbook, name: &str, headers: Vec<String>) -> Result<Self, ApiError> {
        let mut w = Writer {
            wb,
            bold: Format::new().set_bold(),
            name: name.to_string(),
            headers,
            part: 0,
            row: 0,
            total: 0,
        };
        w.new_sheet()?;
        Ok(w)
    }
    fn sheet(&mut self) -> Result<&mut Worksheet, ApiError> {
        self.wb
            .worksheets_mut()
            .last_mut()
            .ok_or_else(ApiError::internal)
    }
    fn new_sheet(&mut self) -> Result<(), ApiError> {
        self.part += 1;
        let base: String = self.name.chars().take(26).collect();
        let name = if self.part == 1 {
            self.name.chars().take(31).collect()
        } else {
            format!("{base} ({})", self.part)
        };
        let headers = self.headers.clone();
        let bold = self.bold.clone();
        let ws = self
            .wb
            .add_worksheet_with_constant_memory()
            .map_err(|_| ApiError::internal())?;
        ws.set_name(&name).map_err(|_| ApiError::internal())?;
        for (c, h) in headers.iter().enumerate() {
            let _ = ws.set_column_width(c as u16, (h.chars().count().clamp(8, 30) + 6) as f64);
        }
        let _ = ws.set_freeze_panes(1, 0);
        for (c, h) in headers.iter().enumerate() {
            ws.write_string_with_format(0, c as u16, h, &bold)
                .map_err(|_| ApiError::internal())?;
        }
        self.row = 1;
        Ok(())
    }
    /// One record. Every cell is text: a phone keeps its zero and nothing is a formula.
    fn push(&mut self, cells: &[Option<String>]) -> Result<(), ApiError> {
        if self.row >= ROWS_PER_SHEET {
            self.new_sheet()?;
        }
        let row = self.row;
        let ws = self.sheet()?;
        for (c, v) in cells.iter().enumerate() {
            if let Some(v) = v.as_deref().filter(|v| !v.is_empty()) {
                // A cell holds 32,767 characters at most.
                let v: String = v.chars().take(32_000).collect();
                ws.write_string(row, c as u16, &v)
                    .map_err(|_| ApiError::internal())?;
            }
        }
        self.row += 1;
        self.total += 1;
        Ok(())
    }
}

async fn write_query(
    conn: &mut sqlx::PgConnection,
    wb: &mut Workbook,
    name: &str,
    sql: String,
    school_id: Uuid,
) -> Result<u64, ApiError> {
    use sqlx::{Executor, SqlSafeStr, Statement};
    let headers: Vec<String> = conn
        .prepare(sqlx::AssertSqlSafe(sql.clone()).into_sql_str())
        .await?
        .columns()
        .iter()
        .map(|c| c.name().to_string())
        .collect();
    let width = headers.len();
    let mut w = Writer::start(wb, name, headers)?;
    let mut rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(school_id)
        .fetch(&mut *conn);
    while let Some(row) = rows.next().await {
        let row = row?;
        let cells: Vec<Option<String>> = (0..width)
            .map(|i| row.try_get::<Option<String>, _>(i).ok().flatten())
            .collect();
        w.push(&cells)?;
    }
    Ok(w.total)
}

fn ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// What the workbook holds: sheet names with their record counts, for the first sheet and
/// for the audit log.
pub struct Exported {
    pub bytes: Vec<u8>,
    pub sheets: Vec<(String, u64)>,
}

/// Build the workbook for one school.
pub async fn export(pool: &sqlx::PgPool, school_id: Uuid) -> Result<Exported, ApiError> {
    let mut conn = pool.acquire().await?;
    // One consistent picture of the school, however long the export takes.
    sqlx::query("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *conn)
        .await?;
    let result = export_in(&mut conn, school_id).await;
    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
    result
}

async fn export_in(conn: &mut sqlx::PgConnection, school_id: Uuid) -> Result<Exported, ApiError> {
    let school = madrasa_db::schools::get(conn, school_id).await?;
    let mut wb = Workbook::new();
    // The first sheet is written last (it lists what the others hold) but must come first.
    let about = wb.add_worksheet();
    about
        .set_name("About this file")
        .map_err(|_| ApiError::internal())?;
    let mut sheets: Vec<(String, u64)> = Vec::new();

    for (name, _, sql) in SHEETS {
        let n = write_query(conn, &mut wb, name, sql.to_string(), school_id).await?;
        sheets.push((name.to_string(), n));
    }

    // Settings, without anything secret.
    let settings: Vec<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT key, value FROM settings WHERE school_id = $1 AND deleted_at IS NULL ORDER BY key",
    )
    .bind(school_id)
    .fetch_all(&mut *conn)
    .await?;
    {
        let mut w = Writer::start(&mut wb, "Settings", vec!["Setting".into(), "Value".into()])?;
        for (key, value) in settings {
            if is_secret_setting(&key, &value) {
                continue;
            }
            let shown = match &value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            w.push(&[Some(key), Some(shown)])?;
        }
        sheets.push(("Settings".into(), w.total));
    }

    // Everything else the school owns, as it is stored.
    let covered: Vec<&str> = SHEETS
        .iter()
        .flat_map(|(_, t, _)| t.iter().copied())
        .collect();
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name::text FROM information_schema.columns
         WHERE table_schema = current_schema() AND column_name = 'school_id' ORDER BY table_name",
    )
    .fetch_all(&mut *conn)
    .await?;
    for table in tables {
        if covered.contains(&table.as_str()) || NOT_EXPORTED.contains(&table.as_str()) {
            continue;
        }
        let columns: Vec<(String, String)> = sqlx::query_as(
            "SELECT column_name::text, data_type::text FROM information_schema.columns
             WHERE table_schema = current_schema() AND table_name = $1 ORDER BY ordinal_position",
        )
        .bind(&table)
        .fetch_all(&mut *conn)
        .await?;
        let soft_delete = columns.iter().any(|(c, _)| c == "deleted_at");
        let list = columns
            .iter()
            .filter(|(c, t)| !NOT_COLUMNS.contains(&c.as_str()) && t != "bytea")
            .map(|(c, _)| format!("{}::text AS {}", ident(c), ident(c)))
            .collect::<Vec<_>>()
            .join(", ");
        let order = if columns.iter().any(|(c, _)| c == "created_at") {
            "created_at, id"
        } else {
            "id"
        };
        let sql = format!(
            "SELECT {list} FROM {} WHERE school_id = $1{} ORDER BY {order}",
            ident(&table),
            if soft_delete {
                " AND deleted_at IS NULL"
            } else {
                ""
            },
        );
        let name = format!("data {table}");
        let n = write_query(conn, &mut wb, &name, sql, school_id).await?;
        sheets.push((name, n));
    }

    // The first sheet: whose data this is and what each sheet holds.
    let bold = Format::new().set_bold();
    let about = wb
        .worksheet_from_name("About this file")
        .map_err(|_| ApiError::internal())?;
    let _ = about.set_column_width(0, 34.0);
    let _ = about.set_column_width(1, 60.0);
    let lines: Vec<(String, String)> = vec![
        ("School".into(), school.name.clone()),
        ("School code".into(), school.code.clone()),
        ("Exported on".into(), chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string()),
        ("Madrasa version".into(), env!("CARGO_PKG_VERSION").into()),
        (
            "What this is".into(),
            "Every record of this school. The first sheets are the records as people know them; the sheets named \"data …\" are the remaining tables as Madrasa stores them. Passwords, sign-in sessions and keys are not included. Photographs and uploaded files are not included: they are in the backup.".into(),
        ),
    ];
    let mut r = 0u32;
    for (k, v) in &lines {
        about
            .write_string_with_format(r, 0, k, &bold)
            .map_err(|_| ApiError::internal())?;
        about
            .write_string(r, 1, v)
            .map_err(|_| ApiError::internal())?;
        r += 1;
    }
    r += 1;
    about
        .write_string_with_format(r, 0, "Sheet", &bold)
        .map_err(|_| ApiError::internal())?;
    about
        .write_string_with_format(r, 1, "Records", &bold)
        .map_err(|_| ApiError::internal())?;
    for (name, n) in &sheets {
        r += 1;
        about
            .write_string(r, 0, name)
            .map_err(|_| ApiError::internal())?;
        about
            .write_number(r, 1, *n as f64)
            .map_err(|_| ApiError::internal())?;
    }
    let bytes = wb.save_to_buffer().map_err(|_| ApiError::internal())?;
    Ok(Exported { bytes, sheets })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn secrets_are_recognised_by_name_and_by_what_is_inside() {
        assert!(is_secret_setting("nexus.token", &json!("abc")));
        assert!(is_secret_setting("instance.key", &json!("abc")));
        assert!(is_secret_setting(
            "backup.keys",
            &json!({ "public_key": "x" })
        ));
        assert!(is_secret_setting(
            "directory",
            &json!({ "url": "ldap://x", "bind_password_sealed": "…" })
        ));
        assert!(is_secret_setting("node_id", &json!(1)));
        assert!(!is_secret_setting("currency", &json!("PKR")));
        assert!(!is_secret_setting(
            "admissions.fee_head_id",
            &json!("0191…")
        ));
        assert!(!is_secret_setting(
            "library.rules",
            &json!({ "staff": { "loan_days": 30 } })
        ));
    }

    #[test]
    fn every_readable_sheet_has_a_name_excel_accepts_and_covers_tables() {
        for (name, tables, sql) in SHEETS {
            assert!(name.chars().count() <= 26, "{name}");
            assert!(!tables.is_empty() && sql.contains("$1"), "{name}");
        }
    }
}
