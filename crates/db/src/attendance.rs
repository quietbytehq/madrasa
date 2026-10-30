//! Attendance: school calendar (weekend days, holidays), daily sheets per section, per-student
//! history, absentee and status reports, and staff attendance.
//!
//! A sheet holds one entry per enrolled student (everyone present unless marked otherwise).
//! It is submitted when first saved; a later save is a correction, allowed only when the caller
//! passes `allow_correction` (the API requires `attendance.correct`), and every changed entry
//! is audit-logged with its old and new value.

use crate::audit::{self, snapshot, Action};
use crate::models::{
    AbsenteeRow, AttendanceEntry, AttendanceSheet, AuditEntry, Holiday, RosterRow,
    SectionDayStatus, StaffAttendance, StaffDayRow, StudentDayRow,
};
use crate::{academics, settings, Actor, DbError, DbResult};
use chrono::NaiveDate;
use madrasa_core::attendance::{
    self, AttendanceStatus, AttendanceSummary, DayKind, HolidaySpan, StaffStatus,
};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use serde_json::json;
use sqlx::PgConnection;
use std::collections::HashMap;
use uuid::Uuid;

pub const KEY_WEEKEND_DAYS: &str = "attendance.weekend_days";

// ---- calendar -----------------------------------------------------------------------------

/// Weekly off days as ISO weekday numbers (1 = Monday … 7 = Sunday).
pub async fn weekend_days(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Vec<u32>> {
    Ok(settings::get(conn, Some(school_id), KEY_WEEKEND_DAYS)
        .await?
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_else(attendance::default_weekend))
}

pub async fn set_weekend_days(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    days: &[u32],
) -> DbResult<Vec<u32>> {
    let days = attendance::validate_weekend(days).map_err(DbError::Conflict)?;
    settings::set(
        conn,
        hlc,
        actor,
        Some(school_id),
        KEY_WEEKEND_DAYS,
        json!(days),
    )
    .await?;
    Ok(days)
}

/// Holidays overlapping the optional range, soonest first.
pub async fn holidays(
    conn: &mut PgConnection,
    school_id: Uuid,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
) -> DbResult<Vec<Holiday>> {
    Ok(sqlx::query_as::<_, Holiday>(
        "SELECT * FROM holidays WHERE school_id = $1 AND deleted_at IS NULL
           AND ($2::date IS NULL OR ends_on >= $2) AND ($3::date IS NULL OR starts_on <= $3)
         ORDER BY starts_on, name",
    )
    .bind(school_id)
    .bind(from)
    .bind(to)
    .fetch_all(conn)
    .await?)
}

pub async fn create_holiday(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    name: &str,
    starts_on: NaiveDate,
    ends_on: NaiveDate,
) -> DbResult<Holiday> {
    let name = name.trim();
    if name.is_empty() {
        return Err(DbError::Conflict("holiday name is required".into()));
    }
    if ends_on < starts_on {
        return Err(DbError::Conflict(
            "holiday end date must not be before its start".into(),
        ));
    }
    let row = sqlx::query_as::<_, Holiday>(
        "INSERT INTO holidays (id, school_id, name, starts_on, ends_on, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING *",
    )
    .bind(new_id())
    .bind(school_id)
    .bind(name)
    .bind(starts_on)
    .bind(ends_on)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "holiday",
        Some(row.id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn delete_holiday(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let row = sqlx::query_as::<_, Holiday>(
        "UPDATE holidays SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3
         WHERE id = $1 AND deleted_at IS NULL RETURNING *",
    )
    .bind(id)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| DbError::NotFound("holiday".into()))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Delete,
        "holiday",
        Some(row.id),
        Some(snapshot(&row)),
        None,
    )
    .await?;
    Ok(())
}

/// Weekend days and every holiday of the school (holidays are few; load them all).
pub async fn calendar(
    conn: &mut PgConnection,
    school_id: Uuid,
) -> DbResult<(Vec<u32>, Vec<HolidaySpan>)> {
    let weekend = weekend_days(&mut *conn, school_id).await?;
    let spans = holidays(conn, school_id, None, None)
        .await?
        .into_iter()
        .map(|h| HolidaySpan {
            name: h.name,
            starts_on: h.starts_on,
            ends_on: h.ends_on,
        })
        .collect();
    Ok((weekend, spans))
}

pub async fn day_kind(
    conn: &mut PgConnection,
    school_id: Uuid,
    date: NaiveDate,
) -> DbResult<DayKind> {
    let (weekend, holidays) = calendar(conn, school_id).await?;
    Ok(attendance::day_kind(date, &weekend, &holidays))
}

// ---- sheets -------------------------------------------------------------------------------

/// Active students enrolled in the section with their entry for `date` (if a sheet exists).
pub async fn roster(
    conn: &mut PgConnection,
    section_id: Uuid,
    date: NaiveDate,
) -> DbResult<Vec<RosterRow>> {
    Ok(sqlx::query_as::<_, RosterRow>(
        "SELECT s.id AS student_id, s.student_number, s.full_name, s.father_name, e.roll_no,
                a.status, a.note
         FROM enrollments e
         JOIN students s ON s.id = e.student_id AND s.deleted_at IS NULL AND s.status = 'active'
         LEFT JOIN attendance_sheets sh ON sh.section_id = e.section_id AND sh.date = $2 AND sh.deleted_at IS NULL
         LEFT JOIN attendance_entries a ON a.sheet_id = sh.id AND a.student_id = s.id AND a.deleted_at IS NULL
         WHERE e.section_id = $1 AND e.deleted_at IS NULL
         ORDER BY e.roll_no NULLS LAST, s.full_name",
    )
    .bind(section_id)
    .bind(date)
    .fetch_all(conn)
    .await?)
}

pub async fn sheet(
    conn: &mut PgConnection,
    section_id: Uuid,
    date: NaiveDate,
) -> DbResult<Option<AttendanceSheet>> {
    Ok(sqlx::query_as::<_, AttendanceSheet>(
        "SELECT * FROM attendance_sheets WHERE section_id = $1 AND date = $2 AND deleted_at IS NULL",
    )
    .bind(section_id)
    .bind(date)
    .fetch_optional(conn)
    .await?)
}

pub async fn get_sheet(conn: &mut PgConnection, id: Uuid) -> DbResult<AttendanceSheet> {
    sqlx::query_as::<_, AttendanceSheet>(
        "SELECT * FROM attendance_sheets WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("attendance sheet".into()))
}

pub async fn entries(conn: &mut PgConnection, sheet_id: Uuid) -> DbResult<Vec<AttendanceEntry>> {
    Ok(sqlx::query_as::<_, AttendanceEntry>(
        "SELECT id, school_id, sheet_id, student_id, date, status, note, updated_at, updated_by, hlc
         FROM attendance_entries WHERE sheet_id = $1 AND deleted_at IS NULL",
    )
    .bind(sheet_id)
    .fetch_all(conn)
    .await?)
}

#[derive(Debug, Clone)]
pub struct EntryInput {
    pub student_id: Uuid,
    pub status: AttendanceStatus,
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SaveSheet {
    pub school_id: Uuid,
    pub section_id: Uuid,
    pub date: NaiveDate,
    pub marked_by: Uuid,
    /// Exceptions (or the full list); students not listed are present.
    pub entries: Vec<EntryInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveOutcome {
    pub created: bool,
    /// Entries whose status or note changed (corrections only).
    pub changed: usize,
}

fn clean_note(n: &Option<String>) -> Option<String> {
    n.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Save a section's sheet for a day. The first save submits it; later saves are corrections
/// and need `allow_correction`. Refused on weekends and holidays and for students who are not
/// enrolled in the section.
pub async fn save_sheet(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    input: SaveSheet,
    allow_correction: bool,
) -> DbResult<(AttendanceSheet, SaveOutcome)> {
    let section = academics::get_section(&mut *conn, input.section_id).await?;
    if section.school_id != input.school_id {
        return Err(DbError::NotFound("section".into()));
    }
    match day_kind(&mut *conn, input.school_id, input.date).await? {
        DayKind::School => {}
        DayKind::Weekend => {
            return Err(DbError::Conflict(
                "No attendance is taken on a weekend".into(),
            ))
        }
        DayKind::Holiday { name } => {
            return Err(DbError::Conflict(format!(
                "No attendance is taken on a holiday ({name})"
            )))
        }
    }
    let roster = roster(&mut *conn, input.section_id, input.date).await?;
    if roster.is_empty() {
        return Err(DbError::Conflict(
            "This section has no active students".into(),
        ));
    }
    let mut wanted: HashMap<Uuid, (AttendanceStatus, Option<String>)> = HashMap::new();
    for e in &input.entries {
        if !roster.iter().any(|r| r.student_id == e.student_id) {
            return Err(DbError::Conflict(
                "A student in the sheet is not enrolled in this section".into(),
            ));
        }
        wanted.insert(e.student_id, (e.status, clean_note(&e.note)));
    }
    let desired: Vec<(Uuid, AttendanceStatus, Option<String>)> = roster
        .iter()
        .map(|r| {
            let (st, note) = wanted
                .get(&r.student_id)
                .cloned()
                .unwrap_or((AttendanceStatus::Present, None));
            (r.student_id, st, note)
        })
        .collect();
    let summary = attendance::summarize(desired.iter().map(|(_, s, _)| *s));

    let existing = sheet(&mut *conn, input.section_id, input.date).await?;
    let Some(existing) = existing else {
        // First save: submit the sheet and bulk-insert one entry per student.
        let sheet_id = new_id();
        let sheet = sqlx::query_as::<_, AttendanceSheet>(
            "INSERT INTO attendance_sheets (id, school_id, section_id, academic_year_id, date, marked_by,
                                            present, absent, late, leave, updated_by, hlc)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $6, $11) RETURNING *",
        )
        .bind(sheet_id)
        .bind(input.school_id)
        .bind(input.section_id)
        .bind(section.academic_year_id)
        .bind(input.date)
        .bind(input.marked_by)
        .bind(summary.present as i32)
        .bind(summary.absent as i32)
        .bind(summary.late as i32)
        .bind(summary.leave as i32)
        .bind(hlc.now().to_string())
        .fetch_one(&mut *conn)
        .await?;
        let ids: Vec<Uuid> = desired.iter().map(|_| new_id()).collect();
        let students: Vec<Uuid> = desired.iter().map(|(s, _, _)| *s).collect();
        let statuses: Vec<String> = desired
            .iter()
            .map(|(_, s, _)| s.as_str().to_string())
            .collect();
        let notes: Vec<Option<String>> = desired.iter().map(|(_, _, n)| n.clone()).collect();
        sqlx::query(
            "INSERT INTO attendance_entries (id, school_id, sheet_id, student_id, date, status, note, updated_by, hlc)
             SELECT i, $2, $3, s, $4, st, n, $5, $6
             FROM UNNEST($1::uuid[], $7::uuid[], $8::text[], $9::text[]) AS t(i, s, st, n)",
        )
        .bind(&ids)
        .bind(input.school_id)
        .bind(sheet_id)
        .bind(input.date)
        .bind(input.marked_by)
        .bind(hlc.now().to_string())
        .bind(&students)
        .bind(&statuses)
        .bind(&notes)
        .execute(&mut *conn)
        .await?;
        audit::record(
            conn,
            hlc,
            actor,
            Action::Create,
            "attendance_sheet",
            Some(sheet.id),
            None,
            Some(json!({
                "section_id": sheet.section_id, "date": sheet.date, "present": summary.present,
                "absent": summary.absent, "late": summary.late, "leave": summary.leave,
                "exceptions": desired.iter().filter(|(_, s, _)| *s != AttendanceStatus::Present)
                    .map(|(id, s, n)| json!({ "student_id": id, "status": s.as_str(), "note": n })).collect::<Vec<_>>(),
            })),
        )
        .await?;
        return Ok((
            sheet,
            SaveOutcome {
                created: true,
                changed: 0,
            },
        ));
    };

    if !allow_correction {
        return Err(DbError::Conflict(
            "Attendance for this day is already submitted".into(),
        ));
    }
    let current: HashMap<Uuid, AttendanceEntry> = entries(&mut *conn, existing.id)
        .await?
        .into_iter()
        .map(|e| (e.student_id, e))
        .collect();
    let mut changed = 0usize;
    for (student_id, status, note) in &desired {
        match current.get(student_id) {
            Some(cur) if cur.status == status.as_str() && cur.note == *note => {}
            Some(cur) => {
                let updated = sqlx::query_as::<_, AttendanceEntry>(
                    "UPDATE attendance_entries SET status = $2, note = $3, updated_at = now(), updated_by = $4, hlc = $5
                     WHERE id = $1
                     RETURNING id, school_id, sheet_id, student_id, date, status, note, updated_at, updated_by, hlc",
                )
                .bind(cur.id)
                .bind(status.as_str())
                .bind(note)
                .bind(actor.user_id)
                .bind(hlc.now().to_string())
                .fetch_one(&mut *conn)
                .await?;
                audit::record(
                    &mut *conn,
                    hlc,
                    actor,
                    Action::Update,
                    "attendance_entry",
                    Some(cur.id),
                    Some(entry_json(cur)),
                    Some(entry_json(&updated)),
                )
                .await?;
                changed += 1;
            }
            None => {
                // Enrolled after the sheet was submitted.
                let inserted = sqlx::query_as::<_, AttendanceEntry>(
                    "INSERT INTO attendance_entries (id, school_id, sheet_id, student_id, date, status, note, updated_by, hlc)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                     RETURNING id, school_id, sheet_id, student_id, date, status, note, updated_at, updated_by, hlc",
                )
                .bind(new_id())
                .bind(input.school_id)
                .bind(existing.id)
                .bind(student_id)
                .bind(input.date)
                .bind(status.as_str())
                .bind(note)
                .bind(actor.user_id)
                .bind(hlc.now().to_string())
                .fetch_one(&mut *conn)
                .await?;
                audit::record(
                    &mut *conn,
                    hlc,
                    actor,
                    Action::Create,
                    "attendance_entry",
                    Some(inserted.id),
                    None,
                    Some(entry_json(&inserted)),
                )
                .await?;
                changed += 1;
            }
        }
    }
    let sheet = sqlx::query_as::<_, AttendanceSheet>(
        "UPDATE attendance_sheets SET present = $2, absent = $3, late = $4, leave = $5,
            updated_at = now(), updated_by = $6, hlc = $7
         WHERE id = $1 RETURNING *",
    )
    .bind(existing.id)
    .bind(summary.present as i32)
    .bind(summary.absent as i32)
    .bind(summary.late as i32)
    .bind(summary.leave as i32)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    if changed > 0 {
        audit::record(
            conn,
            hlc,
            actor,
            Action::Update,
            "attendance_sheet",
            Some(sheet.id),
            Some(json!({ "present": existing.present, "absent": existing.absent, "late": existing.late, "leave": existing.leave })),
            Some(json!({ "present": sheet.present, "absent": sheet.absent, "late": sheet.late, "leave": sheet.leave, "corrected_entries": changed })),
        )
        .await?;
    }
    Ok((
        sheet,
        SaveOutcome {
            created: false,
            changed,
        },
    ))
}

fn entry_json(e: &AttendanceEntry) -> serde_json::Value {
    json!({ "sheet_id": e.sheet_id, "student_id": e.student_id, "date": e.date, "status": e.status, "note": e.note })
}

/// Audit trail of a sheet: its submission and every corrected entry, oldest first.
pub async fn sheet_history(conn: &mut PgConnection, sheet_id: Uuid) -> DbResult<Vec<AuditEntry>> {
    Ok(sqlx::query_as::<_, AuditEntry>(
        "SELECT * FROM audit_log
         WHERE (entity = 'attendance_sheet' AND entity_id = $1)
            OR (entity = 'attendance_entry' AND new_value->>'sheet_id' = $1::text)
         ORDER BY at ASC, hlc ASC",
    )
    .bind(sheet_id)
    .fetch_all(conn)
    .await?)
}

// ---- student history ----------------------------------------------------------------------

pub async fn student_history(
    conn: &mut PgConnection,
    student_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> DbResult<Vec<StudentDayRow>> {
    Ok(sqlx::query_as::<_, StudentDayRow>(
        "SELECT a.date, a.status, a.note, sh.section_id
         FROM attendance_entries a JOIN attendance_sheets sh ON sh.id = a.sheet_id AND sh.deleted_at IS NULL
         WHERE a.student_id = $1 AND a.date BETWEEN $2 AND $3 AND a.deleted_at IS NULL
         ORDER BY a.date DESC",
    )
    .bind(student_id)
    .bind(from)
    .bind(to)
    .fetch_all(conn)
    .await?)
}

pub async fn student_summary(
    conn: &mut PgConnection,
    student_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> DbResult<AttendanceSummary> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT status, count(*) FROM attendance_entries
         WHERE student_id = $1 AND date BETWEEN $2 AND $3 AND deleted_at IS NULL GROUP BY status",
    )
    .bind(student_id)
    .bind(from)
    .bind(to)
    .fetch_all(conn)
    .await?;
    let mut s = AttendanceSummary::default();
    for (status, n) in rows {
        if let Ok(st) = status.parse::<AttendanceStatus>() {
            for _ in 0..n {
                s.add(st);
            }
        }
    }
    Ok(s)
}

/// Entries of a section between two dates (for the monthly register).
pub async fn section_entries(
    conn: &mut PgConnection,
    section_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> DbResult<Vec<AttendanceEntry>> {
    Ok(sqlx::query_as::<_, AttendanceEntry>(
        "SELECT a.id, a.school_id, a.sheet_id, a.student_id, a.date, a.status, a.note, a.updated_at, a.updated_by, a.hlc
         FROM attendance_entries a JOIN attendance_sheets sh ON sh.id = a.sheet_id AND sh.deleted_at IS NULL
         WHERE sh.section_id = $1 AND a.date BETWEEN $2 AND $3 AND a.deleted_at IS NULL",
    )
    .bind(section_id)
    .bind(from)
    .bind(to)
    .fetch_all(conn)
    .await?)
}

// ---- reports ------------------------------------------------------------------------------

/// Students not present on a day (absent, late, leave), optionally for one class and limited
/// to a section scope (teachers).
pub async fn absentees(
    conn: &mut PgConnection,
    school_id: Uuid,
    date: NaiveDate,
    class_id: Option<Uuid>,
    section_scope: Option<&[Uuid]>,
) -> DbResult<Vec<AbsenteeRow>> {
    Ok(sqlx::query_as::<_, AbsenteeRow>(
        "SELECT s.id AS student_id, s.student_number, s.full_name, s.father_name,
                c.name AS class_name, sec.id AS section_id, sec.name AS section_name, e.roll_no,
                a.status, a.note,
                (SELECT g.phone FROM student_guardians sg JOIN guardians g ON g.id = sg.guardian_id
                  WHERE sg.student_id = s.id AND sg.deleted_at IS NULL AND g.deleted_at IS NULL
                  ORDER BY sg.is_primary DESC, g.created_at LIMIT 1) AS guardian_phone
         FROM attendance_entries a
         JOIN attendance_sheets sh ON sh.id = a.sheet_id AND sh.deleted_at IS NULL
         JOIN sections sec ON sec.id = sh.section_id
         JOIN classes c ON c.id = sec.class_id
         JOIN students s ON s.id = a.student_id
         LEFT JOIN enrollments e ON e.student_id = s.id AND e.section_id = sec.id AND e.deleted_at IS NULL
         WHERE sh.school_id = $1 AND a.date = $2 AND a.status <> 'present' AND a.deleted_at IS NULL
           AND ($3::uuid IS NULL OR sec.class_id = $3)
           AND ($4::uuid[] IS NULL OR sec.id = ANY($4))
         ORDER BY c.level, c.name, sec.name, e.roll_no NULLS LAST, s.full_name",
    )
    .bind(school_id)
    .bind(date)
    .bind(class_id)
    .bind(section_scope)
    .fetch_all(conn)
    .await?)
}

/// Every section of the active year with its marking state for a day.
pub async fn daily_status(
    conn: &mut PgConnection,
    school_id: Uuid,
    academic_year_id: Uuid,
    date: NaiveDate,
    section_scope: Option<&[Uuid]>,
) -> DbResult<Vec<SectionDayStatus>> {
    Ok(sqlx::query_as::<_, SectionDayStatus>(
        "SELECT sec.id AS section_id, c.id AS class_id, c.name AS class_name, sec.name AS section_name,
                st.full_name AS class_teacher_name,
                (SELECT count(*) FROM enrollments e JOIN students s ON s.id = e.student_id AND s.deleted_at IS NULL AND s.status = 'active'
                  WHERE e.section_id = sec.id AND e.deleted_at IS NULL) AS enrolled,
                sh.id AS sheet_id, sh.submitted_at, sh.present, sh.absent, sh.late, sh.leave
         FROM sections sec
         JOIN classes c ON c.id = sec.class_id
         LEFT JOIN staff st ON st.id = sec.class_teacher_id
         LEFT JOIN attendance_sheets sh ON sh.section_id = sec.id AND sh.date = $3 AND sh.deleted_at IS NULL
         WHERE sec.school_id = $1 AND sec.academic_year_id = $2 AND sec.deleted_at IS NULL
           AND ($4::uuid[] IS NULL OR sec.id = ANY($4))
         ORDER BY c.level, c.name, sec.name",
    )
    .bind(school_id)
    .bind(academic_year_id)
    .bind(date)
    .bind(section_scope)
    .fetch_all(conn)
    .await?)
}

// ---- staff --------------------------------------------------------------------------------

pub async fn staff_day(
    conn: &mut PgConnection,
    school_id: Uuid,
    date: NaiveDate,
) -> DbResult<Vec<StaffDayRow>> {
    Ok(sqlx::query_as::<_, StaffDayRow>(
        "SELECT st.id AS staff_id, st.staff_number, st.full_name, st.designation, a.status, a.note
         FROM staff st
         LEFT JOIN staff_attendance a ON a.staff_id = st.id AND a.date = $2 AND a.deleted_at IS NULL
         WHERE st.school_id = $1 AND st.deleted_at IS NULL AND st.status = 'active'
         ORDER BY st.full_name",
    )
    .bind(school_id)
    .bind(date)
    .fetch_all(conn)
    .await?)
}

#[derive(Debug, Clone)]
pub struct StaffEntryInput {
    pub staff_id: Uuid,
    pub status: StaffStatus,
    pub note: Option<String>,
}

/// Upsert the day's staff records; everyone not listed is present. Returns how many rows
/// were created or changed. Changes are audited per staff member.
pub async fn save_staff_day(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    date: NaiveDate,
    marked_by: Uuid,
    entries: &[StaffEntryInput],
) -> DbResult<usize> {
    if !day_kind(&mut *conn, school_id, date).await?.is_school_day() {
        return Err(DbError::Conflict(
            "No attendance is taken on a weekend or holiday".into(),
        ));
    }
    let staff = staff_day(&mut *conn, school_id, date).await?;
    let mut wanted: HashMap<Uuid, (StaffStatus, Option<String>)> = HashMap::new();
    for e in entries {
        if !staff.iter().any(|s| s.staff_id == e.staff_id) {
            return Err(DbError::Conflict(
                "A staff member in the list is not active in this school".into(),
            ));
        }
        wanted.insert(e.staff_id, (e.status, clean_note(&e.note)));
    }
    let mut changed = 0usize;
    for row in &staff {
        let (status, note) = wanted
            .get(&row.staff_id)
            .cloned()
            .unwrap_or((StaffStatus::Present, None));
        if row.status.as_deref() == Some(status.as_str()) && row.note == note {
            continue;
        }
        let old = row
            .status
            .as_ref()
            .map(|s| json!({ "date": date, "status": s, "note": row.note }));
        let saved = sqlx::query_as::<_, StaffAttendance>(
            "INSERT INTO staff_attendance (id, school_id, staff_id, date, status, note, marked_by, updated_by, hlc)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (staff_id, date) WHERE deleted_at IS NULL DO UPDATE
               SET status = EXCLUDED.status, note = EXCLUDED.note, marked_by = EXCLUDED.marked_by,
                   updated_at = now(), updated_by = EXCLUDED.updated_by, hlc = EXCLUDED.hlc
             RETURNING id, school_id, staff_id, date, status, note, marked_by, updated_at, hlc",
        )
        .bind(new_id())
        .bind(school_id)
        .bind(row.staff_id)
        .bind(date)
        .bind(status.as_str())
        .bind(&note)
        .bind(marked_by)
        .bind(actor.user_id)
        .bind(hlc.now().to_string())
        .fetch_one(&mut *conn)
        .await?;
        audit::record(
            &mut *conn,
            hlc,
            actor,
            if old.is_some() { Action::Update } else { Action::Create },
            "staff_attendance",
            Some(saved.id),
            old,
            Some(json!({ "staff_id": saved.staff_id, "date": saved.date, "status": saved.status, "note": saved.note })),
        )
        .await?;
        changed += 1;
    }
    Ok(changed)
}

pub async fn staff_history(
    conn: &mut PgConnection,
    staff_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> DbResult<Vec<StaffAttendance>> {
    Ok(sqlx::query_as::<_, StaffAttendance>(
        "SELECT id, school_id, staff_id, date, status, note, marked_by, updated_at, hlc
         FROM staff_attendance WHERE staff_id = $1 AND date BETWEEN $2 AND $3 AND deleted_at IS NULL
         ORDER BY date DESC",
    )
    .bind(staff_id)
    .bind(from)
    .bind(to)
    .fetch_all(conn)
    .await?)
}
